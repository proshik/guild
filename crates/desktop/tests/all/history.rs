//! Where the caret has been: the model behind `cmd-[`, `cmd-]` and
//! `cmd-shift-backspace`.
//!
//! Every one of these runs without a window and without a repository,
//! which is why the trail is a model rather than three fields on `Root`.
//! What is checked here is what a caller can reach — the promise the
//! crate makes about the merge distance and about the order the places
//! come back in.

use guild_desktop::root::history::{
    DEPTH, Front, History, LOCATIONS, MERGE, Place, Step, parse_target,
};

fn step(line: u32) -> Step {
    Step {
        workspace: guild_session::WorkspaceId::nil(),
        project: Some(project()),
        front: Front::File(at(line)),
    }
}
fn project() -> guild_session::ProjectId {
    serde_json::from_str("\"11111111-1111-1111-1111-111111111111\"").unwrap()
}
fn alive(_: &Step) -> bool {
    true
}

fn at(line: u32) -> Place {
    Place::new("src/lib.rs", line, 0)
}

/// **The one load-bearing constant**, named rather than inferred: four is the
/// common minimum distance between two changes, and the whole feature is
/// useless one step either side of it.
#[test]
fn the_merge_distance_is_four_lines() {
    assert_eq!(MERGE, 4);
    assert!(at(0).same_as(&at(3)));
    assert!(!at(0).same_as(&at(4)));
}

/// A place is a path, so it outlives the tab that showed it — and
/// `Back` can reopen a file that was closed. Decision 7 of M8.8.
#[test]
fn a_place_is_a_path_a_line_and_a_column() {
    let place = Place::new("crates/desktop/src/lib.rs", 300, 12);

    assert_eq!(
        place.path,
        std::path::PathBuf::from("crates/desktop/src/lib.rs")
    );
    assert_eq!((place.line, place.column), (300, 12));
}

/// Reading around a function does not fill the stack, and a real jump
/// does put a place in it.
#[test]
fn creeping_merges_and_jumping_records() {
    let mut trail = History::default();
    for line in 100..140 {
        trail.moved(step(line));
    }
    trail.moved(step(900));

    assert_eq!(trail.back(None, alive), Some(step(900)));
    assert_eq!(
        trail.back(None, alive),
        Some(step(139)),
        "forty single-line steps left one place, not forty"
    );
    assert_eq!(trail.back(None, alive), None);
}

/// Back, then Forward, then Back again: the two are each other's undo.
#[test]
fn back_and_forward_are_each_other_s_undo() {
    let mut trail = History::default();
    trail.moved(step(10));
    trail.moved(step(500));

    let back = trail
        .back(Some(&step(900)), alive)
        .expect("somewhere behind");
    assert_eq!(back, step(500));
    let forward = trail.forward(Some(&back), alive).expect("somewhere ahead");
    assert_eq!(forward, step(900));
    assert_eq!(trail.back(Some(&forward), alive), Some(step(500)));
}

/// Moving anywhere new throws away a future that was not taken.
#[test]
fn a_new_move_ends_the_forward_stack() {
    let mut trail = History::default();
    trail.moved(step(10));
    trail.back(Some(&step(500)), alive);
    assert!(trail.has_forward(None, alive));

    trail.moved(step(200));
    assert!(!trail.has_forward(None, alive));
}

/// The edit deque is walked only while nothing moves between presses.
#[test]
fn last_edit_location_walks_an_unbroken_run() {
    let mut trail = History::default();
    trail.trail(project()).changed(at(10));
    trail.trail(project()).changed(at(60));

    assert_eq!(
        trail.trail(project()).last_edit(Some(&at(900))),
        Some(at(60))
    );
    assert_eq!(
        trail.trail(project()).last_edit(Some(&at(60))),
        Some(at(10))
    );

    trail.moved(step(900));
    assert_eq!(
        trail.trail(project()).last_edit(Some(&at(900))),
        Some(at(60)),
        "a movement puts the walk back at the newest edit"
    );
}

/// Recent Locations is the same stack Back walks, read from the top —
/// one data structure, so the two cannot drift apart.
#[test]
fn recent_locations_reads_the_back_stack() {
    let mut trail = History::default();
    trail.moved(step(10));
    trail.moved(step(60));

    let rows = trail.locations(
        guild_session::WorkspaceId::nil(),
        project(),
        Some(&step(900)),
        LOCATIONS,
    );
    assert_eq!(rows, vec![at(900), at(60), at(10)]);
}

/// Both caps are the common ones, and neither deque may grow past its own.
#[test]
fn the_deques_are_capped() {
    assert_eq!(DEPTH, 150);
    assert_eq!(LOCATIONS, 25);

    let mut trail = History::default();
    for index in 0..500u32 {
        trail.moved(step(index * 10));
    }
    assert_eq!(
        trail
            .locations(
                guild_session::WorkspaceId::nil(),
                project(),
                None,
                LOCATIONS
            )
            .len(),
        LOCATIONS
    );

    let mut counted = 0;
    while trail.back(None, alive).is_some() {
        counted += 1;
    }
    assert_eq!(counted, DEPTH);
}

/// `cmd-g` takes three shapes and the third is the one that is
/// forgotten: a column alone, for the line one is already on.
#[test]
fn go_to_line_reads_three_shapes() {
    let target = parse_target("120").expect("a line alone");
    assert_eq!((target.line, target.column), (Some(120), None));

    let target = parse_target(" 120:12 ").expect("a line and a column");
    assert_eq!((target.line, target.column), (Some(120), Some(12)));

    let target = parse_target(":12").expect("a column alone");
    assert_eq!((target.line, target.column), (None, Some(12)));
}

/// Anything else is refused rather than rounded to line 1. A jump that
/// went somewhere invented would put the place it left on the back stack
/// and the place it went to would be a lie.
#[test]
fn go_to_line_refuses_what_it_does_not_understand() {
    for text in ["", "   ", ":", "abc", "12:abc", "-4", "0", "12:0", "1:2:3"] {
        assert!(parse_target(text).is_none(), "{text:?} is not a place");
    }
}

#[test]
fn dead_and_standing_entries_are_skipped_in_both_directions() {
    let mut h = History::default();
    h.moved(step(10));
    h.moved(step(50));
    h.moved(step(100));
    let live = |s: &Step| *s != step(50);
    assert!(h.has_back(Some(&step(100)), live));
    assert_eq!(h.back(Some(&step(100)), live), Some(step(10)));
    assert_eq!(h.forward(Some(&step(10)), live), Some(step(100)));
    assert!(!h.has_forward(None, live));
    h.moved(step(50));
    assert!(!h.has_back(Some(&step(10)), |_| false));
    assert_eq!(h.back(None, |_| false), None);
    h.moved(step(10));
    assert!(!h.has_back(Some(&step(11)), alive));
    assert_eq!(h.back(Some(&step(11)), alive), None);
}

#[test]
fn identity_includes_workspace_project_and_each_content_kind() {
    let a = step(10);
    assert!(a.same_as(&step(13)));
    assert!(!a.same_as(&step(14)));
    let mut other = a.clone();
    other.workspace = guild_session::WorkspaceId::new();
    assert!(!a.same_as(&other));
    other = a.clone();
    other.project = None;
    assert!(!a.same_as(&other));
    let fronts = [
        Front::Session(guild_session::SessionId::new()),
        Front::Settings,
        Front::Diff {
            path: Some("a.rs".into()),
            base: guild_session::Base::Head,
        },
        Front::Diff {
            path: Some("a.rs".into()),
            base: guild_session::Base::Index,
        },
        Front::Annotate {
            path: "a.rs".into(),
        },
        Front::Empty {
            area: None,
            pane: None,
        },
    ];
    for (i, front) in fronts.iter().enumerate() {
        let mut x = a.clone();
        x.front = front.clone();
        assert!(x.same_as(&x));
        for other in &fronts[i + 1..] {
            let mut y = x.clone();
            y.front = other.clone();
            assert!(!x.same_as(&y));
        }
    }
}

#[test]
fn movement_in_another_workspace_settles_all_edit_cursors() {
    let mut h = History::default();
    let project = project();
    h.trail(project).changed(at(10));
    h.trail(project).changed(at(60));
    assert_eq!(h.trail(project).last_edit(None), Some(at(60)));
    let mut other = step(200);
    other.project = None;
    other.workspace = guild_session::WorkspaceId::new();
    other.front = Front::Settings;
    h.moved(other.clone());
    assert_eq!(h.trail(project).last_edit(None), Some(at(60)));
    h.back(None, alive);
    assert_eq!(h.trail(project).last_edit(None), Some(at(60)));
}

#[test]
fn recent_locations_excludes_other_workspaces_projects_and_nonfiles() {
    let mut h = History::default();
    h.moved(step(10));
    let mut elsewhere = step(50);
    elsewhere.workspace = guild_session::WorkspaceId::new();
    h.moved(elsewhere);
    let mut elsewhere = step(70);
    elsewhere.project = Some(guild_session::ProjectId::new());
    h.moved(elsewhere);
    let mut terminal = step(90);
    terminal.front = Front::Session(guild_session::SessionId::new());
    h.moved(terminal);
    assert_eq!(
        h.locations(
            guild_session::WorkspaceId::nil(),
            project(),
            Some(&step(100)),
            LOCATIONS
        ),
        vec![at(100), at(10)]
    );
}
#[test]
fn forward_skips_a_standing_target_and_dead_targets() {
    let mut h = History::default();
    h.moved(step(10));
    h.moved(step(100));
    h.back(Some(&step(900)), alive);
    h.back(Some(&step(100)), alive);
    assert_eq!(h.forward(Some(&step(101)), alive), Some(step(900)));
    h.back(Some(&step(900)), alive);
    assert!(!h.has_forward(None, |_| false));
    assert_eq!(h.forward(None, |_| false), None);
}
