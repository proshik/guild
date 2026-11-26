//! A project's own state, kept where the project is.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_session::areas::SavedAreas;
use guild_session::flush;
use guild_session::layout::Axis;
use guild_session::panes::Panes;
use guild_session::{
    Columns, Commands, OpenTab, Panel, ProjectId, ProjectState, Runs, SeatState, SessionId,
    SessionMode, SessionRecord, Shows, project_state,
};

#[test]
fn legacy_tabs_become_the_first_workspace_seat_on_write() {
    let first = guild_session::WorkspaceId::new();
    let other = guild_session::WorkspaceId::new();
    let tab = OpenTab::showing(Shows::Settings);
    let mut state = ProjectState {
        tabs: vec![tab.clone()],
        active: Some(0),
        expanded: vec!["src".into()],
        ..Default::default()
    };

    assert_eq!(state.seat(first, first).tabs, vec![tab.clone()]);
    assert!(state.seat(other, first).tabs.is_empty());
    state.set_seat(other, first, guild_session::SeatState::default());

    let value = serde_json::to_value(&state).unwrap();
    assert!(value.get("tabs").is_none());
    assert!(value.get("active").is_none());
    assert!(value.get("expanded").is_none());
    assert_eq!(state.seat(first, first).tabs, vec![tab]);
}

fn temp(name: &str) -> Scratch {
    Scratch::new(&format!("guild-ps-{name}-"))
}

fn session(project: ProjectId, cwd: &Path) -> SessionRecord {
    SessionRecord::new(
        "guild".to_owned(),
        project,
        cwd.to_path_buf(),
        Runs::Claude(SessionMode::Terminal),
    )
}

/// The request this exists for: a project closed with sessions open comes
/// back with them. Before this, closing a project forgot them outright,
/// and reopening gave an empty window while `claude` still had the
/// conversations on disk with nothing pointing at them.
#[test]
fn a_reopened_project_has_its_sessions_back() {
    let dir = temp("reopen");
    let project = ProjectId::new();

    let mut state = ProjectState::default();
    let record = session(project, &dir);
    let id = record.id;
    state.sessions.push(record);
    state.tabs.push(OpenTab::showing(Shows::Session(id)));
    project_state::save(&dir, &state).expect("saved");
    flush();

    let back = project_state::load(&dir);

    assert_eq!(back.sessions.len(), 1);
    assert_eq!(back.sessions[0].id, id);
    assert_eq!(back.tabs, vec![OpenTab::showing(Shows::Session(id))]);
}

/// A tab is a view onto *something*, and settings is already not a
/// session. What survives a restart has to be able to say so, or the M8
/// file tab has nowhere to be written.
#[test]
fn a_tab_that_is_not_a_session_survives_too() {
    let dir = temp("settings-tab");

    let state = ProjectState {
        tabs: vec![OpenTab::showing(Shows::Settings)],
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");
    flush();

    assert_eq!(project_state::load(&dir).tabs[0].shows, Shows::Settings);
}

/// Guild must not put a directory into somebody's `git status`. The
/// directory ignores itself, because editing a person's own `.gitignore`
/// is not ours to do.
#[test]
fn the_state_directory_ignores_itself() {
    let dir = temp("gitignore");
    project_state::save(&dir, &ProjectState::default()).expect("saved");
    flush();

    let ignore = std::fs::read_to_string(project_state::dir(&dir).join(".gitignore"))
        .expect("the directory carries its own .gitignore");

    assert!(
        ignore.lines().any(|line| line.trim() == "*"),
        "everything in it is ignored: {ignore}"
    );
}

/// A person who deliberately un-ignores part of it must not have Guild put
/// it back on the next save.
#[test]
fn an_edited_gitignore_is_left_alone() {
    let dir = temp("edited-gitignore");
    project_state::save(&dir, &ProjectState::default()).expect("the first save");
    flush();

    let ignore = project_state::dir(&dir).join(".gitignore");
    std::fs::write(&ignore, "state.json\n").unwrap();

    project_state::save(&dir, &ProjectState::default()).expect("the second save");
    flush();

    assert_eq!(std::fs::read_to_string(&ignore).unwrap(), "state.json\n");
}

/// A project directory that has gone — moved, deleted, on a disk nobody
/// mounted — must not stop the window opening.
#[test]
fn a_missing_project_directory_is_not_a_failure() {
    let state = project_state::load(Path::new("/no/such/project/at/all"));

    assert!(state.sessions.is_empty());
    assert!(state.tabs.is_empty());
}

/// **Saving a project whose folder is gone does not bring the folder back.**
/// A worktree removed outside the window left its root open, the next save
/// made `<root>/.guild/` — and with it the root — and the window then saw
/// a folder where the worktree had been.
#[test]
fn saving_a_project_whose_folder_is_gone_writes_nothing() {
    let base = temp("gone");
    let missing = base.join("removed-worktree");

    project_state::save(&missing, &ProjectState::default()).expect("not a failure");
    flush();

    assert!(!missing.exists(), "the folder stays gone");
}

/// The state file goes through the same store as everything else, so it
/// inherits M5.3's atomic write and its backup rather than repeating them.
#[test]
fn the_state_file_keeps_a_previous_copy() {
    let dir = temp("backup");
    let project = ProjectId::new();

    project_state::save(&dir, &ProjectState::default()).expect("the first save");
    flush();
    let mut second = ProjectState::default();
    second.sessions.push(session(project, &dir));
    project_state::save(&dir, &second).expect("the second save");
    flush();

    assert!(
        project_state::dir(&dir).join("state.json.bak").exists(),
        "the previous copy is kept, as the store promises"
    );
}

/// The move out of the global file.
///
/// The shape below was taken off a real `~/.config/guild/workspace.json`
/// with the paths replaced and nothing else: the last time an old format
/// was guessed at, the migration test checked a key the real file never had
/// and passed on invented data.
#[test]
fn sessions_in_the_global_file_move_into_their_projects() {
    let root = temp("migration");
    let project_dir = root.join("guild");
    std::fs::create_dir_all(&project_dir).unwrap();
    let global = root.join("workspace.json");

    let old = format!(
        r#"{{
  "projects": [
    {{
      "id": "1b0c6d83-dbb0-468a-a046-1db734b9548a",
      "path": "{}",
      "name": "guild"
    }}
  ],
  "sessions": [
    {{
      "id": "175c80c7-3478-424e-be0a-be9862a5b68a",
      "project": "1b0c6d83-dbb0-468a-a046-1db734b9548a",
      "title": "guild",
      "cwd": "{}",
      "runs": {{ "claude": "terminal" }},
      "cost_usd": 0.0,
      "topic": "что за проект",
      "usage": {{
        "input_tokens": 12,
        "output_tokens": 2900,
        "cache_read_tokens": 161608,
        "cache_creation_tokens": 169647,
        "cost_usd": 0.7598934999999999
      }},
      "transcript_offset": 136167,
      "had_turn": true,
      "open": true
    }}
  ],
  "recent": [],
  "active_project": "1b0c6d83-dbb0-468a-a046-1db734b9548a",
  "active_session": "175c80c7-3478-424e-be0a-be9862a5b68a"
}}"#,
        project_dir.display(),
        project_dir.display()
    );
    std::fs::write(&global, old).unwrap();

    // Reading an old file keeps everything it holds.
    let workspace = guild_session::persist::load(&global);
    assert_eq!(workspace.sessions().count(), 1, "nothing was dropped");
    let session = workspace.sessions().next().unwrap();
    assert_eq!(session.topic.as_deref(), Some("что за проект"));
    assert_eq!(session.usage.output_tokens, 2900);

    // Writing it puts them where they belong.
    guild_session::persist::save(&workspace, &global).expect("saved");
    flush();

    let beside = project_state::load(&project_dir);
    assert_eq!(beside.sessions.len(), 1, "the session moved to its project");
    assert_eq!(beside.sessions[0].usage.output_tokens, 2900, "whole");

    let global_now: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&global).unwrap()).unwrap();
    assert_eq!(
        global_now["sessions"].as_array().map(Vec::len),
        Some(0),
        "and the global file no longer carries it"
    );

    // And reading again finds it in its new home, once.
    let again = guild_session::persist::load(&global);
    assert_eq!(again.sessions().count(), 1, "not twice");
}

/// A project whose directory has gone keeps its sessions in the global
/// file. Losing somebody's conversations because a disk was not mounted is
/// not a migration, and the directory may well be back tomorrow.
#[test]
fn a_project_whose_directory_is_gone_keeps_its_sessions() {
    let root = temp("homeless");
    let global = root.join("workspace.json");
    let missing = root.join("moved-away");

    let mut workspace = guild_session::Desk::default();
    let project = workspace.add_project(&missing);
    workspace.add_session(session(project, &missing));

    guild_session::persist::save(&workspace, &global).expect("saved");
    flush();

    let global_now: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&global).unwrap()).unwrap();
    assert_eq!(
        global_now["sessions"].as_array().map(Vec::len),
        Some(1),
        "kept rather than dropped"
    );
}

/// Which panels were open, and how big, belongs to the project rather than
/// to the application: a tree is opened in the repository being read, not
/// in the one merely being run, and given the width that repository's paths
/// need.
#[test]
fn a_project_remembers_its_panels() {
    let dir = temp("panels");

    let mut state = ProjectState::default();
    state.panels.left = guild_session::Panel {
        open: true,
        size: 260.0,
    };
    project_state::save(&dir, &state).expect("saved");
    flush();

    let back = project_state::load(&dir);

    assert!(back.panels.left.open);
    assert_eq!(back.panels.left.size, 260.0, "and the width it was given");
    assert!(!back.panels.right.open);
    assert!(!back.panels.bottom.open);
}

/// **A panel's ceiling is the window, not a number.** M5.7 gave the side
/// panels 560 and the bottom one 340, which read as generous until
/// somebody wanted the Log at the top of a tall screen. What a panel may
/// not take is the last of the work — enough that the tab strip and a
/// line of what it holds stay on screen, so the way back is reachable.
#[test]
fn a_panel_may_take_everything_but_the_last_of_the_work() {
    let tall = 1400.0;
    let ceiling = Panel::ceiling(tall, Panel::MIN_HEIGHT);
    assert_eq!(ceiling, tall - Panel::MIN_WORK);

    let mut bottom = Panel::of(198.0);
    bottom.resize(10_000.0, (Panel::MIN_HEIGHT, ceiling));
    assert_eq!(bottom.size, ceiling, "up to the top, less what must stay");
}

/// **Stretch to Left, Right, Top and Bottom**: the panel with the
/// focus has its border moved the way the arrow points — so the arrow that
/// grows the left panel shrinks the right one — and an arrow along an edge
/// the panel does not have does nothing. The step is five characters
/// across, five lines down.
#[test]
fn a_stretch_moves_a_panels_border_the_way_the_arrow_points() {
    use guild_session::{Dock, Stretch};

    assert_eq!(Dock::Left.stretched(Stretch::Right), Some(1.0));
    assert_eq!(Dock::Left.stretched(Stretch::Left), Some(-1.0));
    assert_eq!(Dock::Right.stretched(Stretch::Left), Some(1.0));
    assert_eq!(Dock::Right.stretched(Stretch::Right), Some(-1.0));
    assert_eq!(Dock::Bottom.stretched(Stretch::Top), Some(1.0));
    assert_eq!(Dock::Bottom.stretched(Stretch::Bottom), Some(-1.0));

    for side in [Dock::Left, Dock::Right] {
        assert_eq!(
            side.stretched(Stretch::Top),
            None,
            "{side:?} has no top edge"
        );
        assert_eq!(side.stretched(Stretch::Bottom), None);
    }
    assert_eq!(
        Dock::Bottom.stretched(Stretch::Left),
        None,
        "nor the bottom a side"
    );
    assert_eq!(Dock::Bottom.stretched(Stretch::Right), None);

    // A character 9 wide and a line 16 high.
    assert_eq!(Stretch::Left.step(9.0, 16.0), 45.0);
    assert_eq!(Stretch::Right.step(9.0, 16.0), 45.0);
    assert_eq!(Stretch::Top.step(9.0, 16.0), 80.0);
    assert_eq!(Stretch::Bottom.step(9.0, 16.0), 80.0);
}

/// A window too small for even that gets the floor rather than a ceiling
/// below it: a panel dragged to nothing looks exactly like one that
/// failed to open, and the way back from it is a pixel wide.
#[test]
fn a_tiny_window_still_leaves_a_panel_its_floor() {
    let ceiling = Panel::ceiling(100.0, Panel::MIN_HEIGHT);
    assert_eq!(ceiling, Panel::MIN_HEIGHT);

    let mut panel = Panel::of(198.0);
    panel.resize(-10_000.0, (Panel::MIN_HEIGHT, ceiling));
    assert_eq!(panel.size, Panel::MIN_HEIGHT);
}

/// The two axes are measured against their own side of the window: a
/// bottom panel is bounded by the height and a side one by the width.
#[test]
fn each_panel_is_bounded_by_its_own_axis() {
    let wide_short = (2000.0, 600.0);
    assert_eq!(
        Panel::ceiling(wide_short.0, Panel::MIN_WIDTH),
        2000.0 - Panel::MIN_WORK
    );
    assert_eq!(
        Panel::ceiling(wide_short.1, Panel::MIN_HEIGHT),
        600.0 - Panel::MIN_WORK
    );
}

/// A state file written before panels existed opens with them closed, at
/// the widths the design system gives. Nothing moves under somebody who
/// upgrades.
#[test]
fn a_file_without_panels_opens_with_them_closed() {
    let dir = temp("panels-absent");
    project_state::save(&dir, &ProjectState::default()).expect("the directory");
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions":[],"tabs":[]}"#,
    )
    .unwrap();

    let state = project_state::load(&dir);

    assert!(!state.panels.left.open);
    assert!(!state.panels.right.open);
    assert!(!state.panels.bottom.open);
    assert_eq!(state.panels.left.size, 230.0, "the file tree, section 5");
    assert_eq!(state.panels.right.size, 330.0, "the sessions panel");
    assert_eq!(state.panels.bottom.size, 198.0, "the agent event feed");
}

/// Which directories were expanded in the tree is the project's, like
/// its panels: a person opens `crates/desktop/src` in the repository they
/// read, and finds it open the next time.
#[test]
fn a_project_remembers_which_directories_were_expanded() {
    let dir = temp("expanded");

    let state = ProjectState {
        expanded: vec![PathBuf::from("crates"), PathBuf::from("crates/desktop")],
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");
    flush();

    let back = project_state::load(&dir);
    assert_eq!(back.expanded, state.expanded);
}

/// A state file from before the tree has no `expanded` and reads back
/// with none — not as an error, and not as a guess.
#[test]
fn an_older_state_file_has_nothing_expanded() {
    let dir = temp("expanded-old");
    let state = ProjectState::default();
    project_state::save(&dir, &state).expect("saved");
    flush();
    let file = project_state::dir(&dir).join("state.json");
    let json = std::fs::read_to_string(&file).unwrap();
    let without: serde_json::Value = serde_json::from_str(&json).unwrap();
    let mut without = without;
    without.as_object_mut().unwrap().remove("expanded");
    std::fs::write(&file, serde_json::to_string(&without).unwrap()).unwrap();

    assert!(project_state::load(&dir).expanded.is_empty());
}

/// The tabs of a project come back in the order they were in, with the
/// one that was in front: a session by its id, a file by its relative
/// path, settings as itself.
#[test]
fn a_project_remembers_its_tabs_in_order_and_the_one_in_front() {
    let dir = temp("tabs");
    let id = SessionId::new();

    let state = ProjectState {
        tabs: vec![
            OpenTab::showing(Shows::Session(id)),
            OpenTab::showing(Shows::File(PathBuf::from("crates/desktop/src/lib.rs"))),
            OpenTab::showing(Shows::Settings),
        ],
        active: Some(1),
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");
    flush();

    let back = project_state::load(&dir);
    assert_eq!(back.tabs, state.tabs);
    assert_eq!(back.active, Some(1));
}

/// A state file from before this pass has tabs but no `active`. Its
/// shape is a real one from disk, not one written from memory: the
/// first version of the sessions migration checked a key the old file
/// never had.
#[test]
fn a_state_file_without_an_active_tab_reads_back_with_none() {
    let dir = temp("no-active");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [{"shows": "settings"}], "panels": {"left": {"open": true, "size": 336.0703}, "right": {"open": false, "size": 330.0}, "bottom": {"open": false, "size": 198.0}}, "expanded": []}"#,
    )
    .unwrap();

    let back = project_state::load(&dir);
    assert_eq!(back.tabs.len(), 1);
    assert_eq!(back.active, None);
}

/// A diff tab comes back too — one file's, or the working tree's, which
/// has no path and is written as nothing.
#[test]
fn a_diff_tab_survives_a_restart_in_both_kinds() {
    let dir = temp("diff-tabs");
    let state = ProjectState {
        tabs: vec![
            OpenTab::showing(Shows::Diff {
                path: Some(PathBuf::from("src/lib.rs")),
                base: guild_session::Base::Head,
            }),
            OpenTab::showing(Shows::Diff {
                path: None,
                base: guild_session::Base::Head,
            }),
        ],
        active: Some(1),
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");
    let back = project_state::load(&dir);
    assert_eq!(back.tabs, state.tabs);
    assert_eq!(back.active, Some(1));
}

/// The panel's face is part of the project's state, and a file from
/// before the rail reads back showing the Explorer.
#[test]
fn a_project_remembers_the_panels_face() {
    use guild_session::Face;
    let dir = temp("face");
    let mut state = ProjectState::default();
    assert_eq!(state.panels.face, Face::Explorer, "the Explorer by default");
    state.panels.face = Face::Log;
    project_state::save(&dir, &state).expect("saved");
    assert_eq!(project_state::load(&dir).panels.face, Face::Log);

    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [], "panels": {"left": {"open": true, "size": 230.0}, "right": {"open": false, "size": 330.0}, "bottom": {"open": false, "size": 198.0}}, "expanded": []}"#,
    )
    .unwrap();
    assert_eq!(
        project_state::load(&dir).panels.face,
        Face::Explorer,
        "an old file"
    );
}

/// A diff tab keeps its base — the index, the working tree against the
/// index, a commit — and a file written before there was a base reads
/// back against `HEAD`, in both of the M8.2 shapes: a path, or null for
/// the working tree.
#[test]
fn a_diff_tab_keeps_its_base_and_an_old_file_reads_as_head() {
    use guild_session::Base;
    let dir = temp("diff-base");
    let state = ProjectState {
        tabs: vec![
            OpenTab::showing(Shows::Diff {
                path: Some(PathBuf::from("src/lib.rs")),
                base: Base::Index,
            }),
            OpenTab::showing(Shows::Diff {
                path: Some(PathBuf::from("src/lib.rs")),
                base: Base::Unstaged,
            }),
            OpenTab::showing(Shows::Diff {
                path: Some(PathBuf::from("src/lib.rs")),
                base: Base::Commit("a1b2c3d".into()),
            }),
            OpenTab::showing(Shows::Diff {
                path: None,
                base: Base::Head,
            }),
            OpenTab::showing(Shows::Annotate(PathBuf::from("src/lib.rs"))),
        ],
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");
    assert_eq!(project_state::load(&dir).tabs, state.tabs);

    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [{"shows": {"diff": "src/lib.rs"}}, {"shows": {"diff": null}}, {"shows": "settings"}], "expanded": []}"#,
    )
    .unwrap();
    let back = project_state::load(&dir);
    assert_eq!(
        back.tabs
            .iter()
            .map(|t| t.shows.clone())
            .collect::<Vec<_>>(),
        [
            Shows::Diff {
                path: Some(PathBuf::from("src/lib.rs")),
                base: Base::Head
            },
            Shows::Diff {
                path: None,
                base: Base::Head
            },
            Shows::Settings,
        ],
        "an old file"
    );
}

/// Every tool has a dock by default: the Explorer and the
/// Commit panel at the left, the Log, the Output, the Search, the
/// Problems and Find at the bottom, the right empty — and each dock shows its
/// first tool.
#[test]
fn the_default_docks_are_ideas() {
    use guild_session::{Dock, Docks, Tool};
    let docks = Docks::default();
    assert_eq!(docks.tools_in(Dock::Left), [Tool::Explorer, Tool::Commit]);
    assert_eq!(docks.tools_in(Dock::Right), []);
    assert_eq!(
        docks.tools_in(Dock::Bottom),
        [
            Tool::Git,
            Tool::Output,
            Tool::Terminal,
            Tool::Search,
            Tool::Problems,
            Tool::Find
        ]
    );
    assert_eq!(docks.shown(Dock::Left), Some(Tool::Explorer));
    assert_eq!(docks.shown(Dock::Right), None);
    assert_eq!(docks.shown(Dock::Bottom), Some(Tool::Git));
    assert_eq!(docks.dock_of(Tool::Git), Dock::Bottom);
}

/// Moving a tool takes it out of its dock's order and puts it last in
/// the new one, which shows it; the old dock shows the first tool it
/// has left, or nothing.
#[test]
fn a_moved_tool_is_shown_where_it_went_and_its_old_dock_shows_the_next() {
    use guild_session::{Dock, Docks, Tool};
    let mut docks = Docks::default();
    docks.move_tool(Tool::Explorer, Dock::Right);
    assert_eq!(docks.tools_in(Dock::Left), [Tool::Commit]);
    assert_eq!(docks.tools_in(Dock::Right), [Tool::Explorer]);
    assert_eq!(docks.shown(Dock::Right), Some(Tool::Explorer));
    assert_eq!(docks.shown(Dock::Left), Some(Tool::Commit));
    docks.move_tool(Tool::Commit, Dock::Bottom);
    assert_eq!(
        docks.tools_in(Dock::Bottom),
        [
            Tool::Git,
            Tool::Output,
            Tool::Terminal,
            Tool::Search,
            Tool::Problems,
            Tool::Find,
            Tool::Commit
        ]
    );
    assert_eq!(docks.shown(Dock::Bottom), Some(Tool::Commit));
    assert_eq!(docks.shown(Dock::Left), None);
    docks.move_tool(Tool::Commit, Dock::Bottom);
    assert_eq!(
        docks.tools_in(Dock::Bottom),
        [
            Tool::Git,
            Tool::Output,
            Tool::Terminal,
            Tool::Search,
            Tool::Problems,
            Tool::Find,
            Tool::Commit
        ],
        "moving to where it is changes nothing"
    );
}

/// Showing a tool makes its dock show it and nothing else.
#[test]
fn showing_a_tool_is_its_docks_choice() {
    use guild_session::{Dock, Docks, Tool};
    let mut docks = Docks::default();
    docks.show(Tool::Commit);
    assert_eq!(docks.shown(Dock::Left), Some(Tool::Commit));
    assert_eq!(
        docks.shown(Dock::Bottom),
        Some(Tool::Git),
        "another dock is not touched"
    );
}

/// A state file from M8.3 has a face and no docks: it reads as the docks
/// that face implies, so a project left on the Log comes back on the Log,
/// at the bottom now.
#[test]
fn an_m8_3_file_with_a_face_reads_as_docks() {
    use guild_session::{Dock, Tool};
    let dir = temp("docks-from-face");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [], "panels": {"face": "log", "left": {"open": true, "size": 230.0}, "right": {"open": false, "size": 330.0}, "bottom": {"open": false, "size": 198.0}}, "expanded": []}"#,
    )
    .unwrap();
    let panels = project_state::load(&dir).panels;
    let docks = panels.docks();
    assert_eq!(docks.shown(Dock::Bottom), Some(Tool::Git));
    assert_eq!(docks.shown(Dock::Left), Some(Tool::Explorer));
    assert_eq!(docks.dock_of(Tool::Commit), Dock::Left);
    assert!(
        panels.bottom.open,
        "the Log was open, and it lives at the bottom now"
    );
    assert!(
        !panels.left.open,
        "the left was open for the Log, not for the Explorer"
    );
    assert!(
        panels.docks.is_some(),
        "made at load, so the next save writes them"
    );

    let mut state = project_state::load(&dir);
    state.panels.docks_mut().move_tool(Tool::Git, Dock::Right);
    project_state::save(&dir, &state).expect("saved");
    assert_eq!(
        project_state::load(&dir).panels.docks().dock_of(Tool::Git),
        Dock::Right,
        "written and read back"
    );
}

/// A dock open with no tool in it reads back closed: a file written
/// before a move closed emptied docks can say so, and an open panel
/// saying "Nothing here" is a hole, not a place.
#[test]
fn an_open_dock_with_no_tools_reads_back_closed() {
    use guild_session::{Dock, Tool};
    let dir = temp("empty-dock");
    let mut state = ProjectState::default();
    state.panels.docks_mut().move_tool(Tool::Git, Dock::Left);
    state.panels.docks_mut().move_tool(Tool::Output, Dock::Left);
    state.panels.docks_mut().move_tool(Tool::Search, Dock::Left);
    state
        .panels
        .docks_mut()
        .move_tool(Tool::Problems, Dock::Left);
    state.panels.docks_mut().move_tool(Tool::Find, Dock::Left);
    state
        .panels
        .docks_mut()
        .move_tool(Tool::Terminal, Dock::Left);
    state.panels.bottom.open = true;
    state.panels.right.open = true;
    project_state::save(&dir, &state).expect("saved");
    let back = project_state::load(&dir).panels;
    assert!(!back.bottom.open, "the bottom has no tools left");
    assert!(!back.right.open, "the right never had one");
}

/// The borders inside a tool start where the design system put them,
/// move by what the hand moved, and stop at both ends: a column dragged
/// to nothing looks exactly like a column that failed to draw.
#[test]
fn the_columns_start_at_the_design_systems_widths_and_clamp() {
    let mut columns = Columns::default();
    assert_eq!(columns.branches, Columns::BRANCHES);
    assert_eq!(columns.details, Columns::DETAILS);

    columns.resize_branches(40.0);
    assert_eq!(columns.branches, 240.0);
    columns.resize_branches(-1000.0);
    assert_eq!(columns.branches, Columns::BRANCHES_BOUNDS.0);
    columns.resize_branches(1000.0);
    assert_eq!(columns.branches, Columns::BRANCHES_BOUNDS.1);

    columns.resize_details(-60.0);
    assert_eq!(columns.details, 260.0);
    columns.resize_details(-1000.0);
    assert_eq!(columns.details, Columns::DETAILS_BOUNDS.0);
    columns.resize_details(1000.0);
    assert_eq!(columns.details, Columns::DETAILS_BOUNDS.1);
}

/// **The Push tab's files are dragged like the Log's details** — M12.3
/// task 6: from the 320 they had fixed, growing as the pointer comes back
/// left, stopping at both ends; a file from before has no such column and
/// reads back with the default.
#[test]
fn the_push_files_column_starts_at_its_width_and_clamps() {
    let mut columns = Columns::default();
    assert_eq!(columns.push_files, Columns::PUSH_FILES);
    assert_eq!(Columns::PUSH_FILES, 320.0);
    columns.resize_push_files(80.0);
    assert_eq!(columns.push_files, 400.0);
    columns.resize_push_files(-1000.0);
    assert_eq!(columns.push_files, Columns::PUSH_FILES_BOUNDS.0);
    columns.resize_push_files(1000.0);
    assert_eq!(columns.push_files, Columns::PUSH_FILES_BOUNDS.1);

    let read: Columns =
        serde_json::from_str(r#"{ "branches": 200.0, "details": 320.0, "message": 140.0 }"#)
            .unwrap();
    assert_eq!(read.push_files, Columns::PUSH_FILES);
}

/// A file written by M8.3.1 has no columns at all, and reads back with
/// the design system's — the field is new, not missing.
#[test]
fn an_m8_3_1_file_reads_back_with_the_default_columns() {
    let written = r#"{
        "panels": {
            "face": "explorer",
            "docks": {
                "placed": [["explorer", "left"], ["log", "bottom"]],
                "shown": [["left", "explorer"], ["bottom", "log"]]
            },
            "left": { "open": true, "size": 260.0 },
            "right": { "open": false, "size": 330.0 },
            "bottom": { "open": true, "size": 198.0 }
        }
    }"#;

    let state: ProjectState = serde_json::from_str(written).expect("an M8.3.1 file still reads");
    assert_eq!(state.panels.columns, Columns::default());
    assert_eq!(state.panels.left.size, 260.0);

    // And what is written now comes back the same.
    let again: ProjectState =
        serde_json::from_str(&serde_json::to_string(&state).unwrap()).expect("round trip");
    assert_eq!(again.panels.columns, state.panels.columns);
}

/// **The test M8.3.4's third task exists to pass.**
///
/// `Tool` is `snake_case` on the wire, so `Tool::Git` is the string
/// `"log"` inside `docks.placed` in every project's file, and
/// `Tool::Output` is `"output"`. Renaming or deleting a variant makes
/// serde fail the **whole file** — not the docks, the file — and
/// `Store::load` then answers `T::default()`. The project would lose its
/// sessions, its open tabs, its expanded directories and its panel sizes
/// because a tool was renamed.
///
/// So: a file written before this pass reads back with everything it
/// carried, the Git tool where the Log was, and no Output anywhere.
#[test]
fn a_file_written_before_the_git_tool_keeps_everything_it_carried() {
    use guild_session::{Dock, Tool};
    let dir = temp("git-tool-migration");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{
          "sessions": [],
          "tabs": [{"shows": {"file": "src/lib.rs"}}],
          "expanded": ["src"],
          "panels": {
            "face": "log",
            "docks": {
              "placed": [["explorer","left"],["commit","left"],["output","bottom"],["log","bottom"]],
              "shown": [["bottom","log"],["left","explorer"]]
            },
            "left": {"open": true, "size": 240.0},
            "right": {"open": false, "size": 330.0},
            "bottom": {"open": true, "size": 210.0},
            "columns": {"branches": 180.0, "details": 300.0}
          }
        }"#,
    )
    .unwrap();

    let state = project_state::load(&dir);

    assert_eq!(state.tabs.len(), 1, "the open tab survived a rename");
    assert_eq!(state.expanded.len(), 1, "and the expanded directory");
    assert_eq!(state.panels.left.size, 240.0, "and the panel sizes");
    assert_eq!(state.panels.bottom.size, 210.0);
    assert_eq!(state.panels.columns.branches, 180.0, "and the columns");

    let docks = state.panels.docks();
    assert_eq!(
        docks.shown(Dock::Bottom),
        Some(Tool::Git),
        "the Git tool is where the Log was"
    );
    assert_eq!(docks.dock_of(Tool::Git), Dock::Bottom);
    // **Changed in M8.4, and deliberately.** Until this pass the two
    // assertions here said Output was stripped from the docks and absent
    // from `Tool::ALL`: it was a retired tool with nothing behind its
    // button. It has content now, so a file that named it gets it back
    // exactly where it put it.
    assert_eq!(
        docks.tools_in(Dock::Bottom),
        [
            Tool::Output,
            Tool::Git,
            Tool::Terminal,
            Tool::Search,
            Tool::Problems,
            Tool::Find
        ],
        "Output keeps the place this file gave it, in the file's order, \
         and a tool the file has never heard of is appended"
    );
    assert!(
        Tool::ALL.contains(&Tool::Output),
        "and it is in the status bar again"
    );
}

/// **Was: a dock showing Output falls back to whatever else it holds.**
/// That was M8.3.4's rule for a retired tool, and M8.4 gives the tool
/// content, so the claim is inverted rather than left standing: a project
/// left on the Output panel comes back on it.
#[test]
fn a_dock_that_showed_output_shows_it_again() {
    use guild_session::{Dock, Tool};
    let dir = temp("output-was-shown");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [], "expanded": [], "panels": {
             "docks": {"placed": [["log","bottom"],["output","bottom"]],
                       "shown": [["bottom","output"]]},
             "left": {"open": false, "size": 230.0},
             "right": {"open": false, "size": 330.0},
             "bottom": {"open": true, "size": 198.0}}}"#,
    )
    .unwrap();

    let state = project_state::load(&dir);
    assert_eq!(state.panels.docks().shown(Dock::Bottom), Some(Tool::Output));
    assert!(state.panels.bottom.open, "and the dock stays open");
}

/// **Was: a dock holding only Output reads back closed**, because a dock
/// with no tools in it is a hole. M8.4 puts a tool back in it, so the
/// dock has something to hold and stays open on it. The rule about empty
/// docks is unchanged and is tested where it belongs, above.
#[test]
fn a_dock_holding_only_output_stays_open_on_it() {
    use guild_session::{Dock, Tool};
    let dir = temp("only-output");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [], "expanded": [], "panels": {
             "docks": {"placed": [["output","right"]], "shown": [["right","output"]]},
             "left": {"open": false, "size": 230.0},
             "right": {"open": true, "size": 330.0},
             "bottom": {"open": false, "size": 198.0}}}"#,
    )
    .unwrap();

    let state = project_state::load(&dir);
    assert_eq!(state.panels.docks().shown(Dock::Right), Some(Tool::Output));
    assert!(
        state.panels.right.open,
        "the dock holds a tool, so it is a place"
    );
}

/// **The file written between the retirement and M8.4 names no Output at
/// all**, and it is the commonest file there is — every project opened in
/// the eighteen hours the tool was retired. A tool the file does not name
/// arrives at its default dock, in the list as well as in `dock_of`:
/// otherwise the panel would be reachable by its shortcut and have no
/// button in the status bar, which is a tool that exists and cannot be
/// found.
#[test]
fn a_tool_a_file_does_not_name_arrives_at_its_default_dock() {
    use guild_session::{Dock, Tool};
    let dir = temp("no-output-named");
    std::fs::create_dir_all(project_state::dir(&dir)).unwrap();
    flush();
    std::fs::write(
        project_state::dir(&dir).join("state.json"),
        r#"{"sessions": [], "tabs": [], "expanded": [], "panels": {
             "docks": {"placed": [["explorer","left"],["commit","left"],["log","bottom"]],
                       "shown": [["left","explorer"],["bottom","log"]]},
             "left": {"open": true, "size": 230.0},
             "right": {"open": false, "size": 330.0},
             "bottom": {"open": false, "size": 198.0}}}"#,
    )
    .unwrap();

    let docks = project_state::load(&dir).panels.docks();

    assert_eq!(docks.dock_of(Tool::Output), Dock::Bottom);
    assert!(
        docks.tools_in(Dock::Bottom).contains(&Tool::Output),
        "and it has a button, which is the half `dock_of` alone never gave it"
    );
    // **And the newest tool, by name.** Every state file written before
    // M8.8.5 is this file as far as Problems is concerned, which is
    // every state file a person has. Output is the general case and this
    // is the one that is actually on disk today; a rule proven on a
    // stand-in is a rule nobody has run against the real omission.
    assert_eq!(docks.dock_of(Tool::Problems), Dock::Bottom);
    assert!(
        docks.tools_in(Dock::Bottom).contains(&Tool::Problems),
        "a file written before M8.8.5 gets the Problems button too"
    );
    assert_eq!(
        docks.shown(Dock::Bottom),
        Some(Tool::Git),
        "arriving does not take the dock from whatever it was showing"
    );
}

/// A fresh project holds every tool there is, each in its default dock.
#[test]
fn a_new_project_holds_every_tool() {
    use guild_session::{Dock, Docks, Tool};
    let docks = Docks::default();

    let held: usize = Dock::ALL.iter().map(|d| docks.tools_in(*d).len()).sum();
    assert_eq!(held, Tool::ALL.len(), "every tool is in a dock");
    assert_eq!(
        docks.dock_of(Tool::Output),
        Dock::Bottom,
        "beside the Git tool"
    );
}

/// M8.4: what a project has been asked to run is the project's, and the
/// button after a restart is the button before it.
#[test]
fn the_last_command_comes_back_with_the_project() {
    let dir = temp("commands");

    let mut state = ProjectState::default();
    state.commands.remember("just check");
    project_state::save(&dir, &state).expect("saved");
    flush();

    let back = project_state::load(&dir);
    assert_eq!(back.commands.last.as_deref(), Some("just check"));
    assert_eq!(back.commands.recent, vec!["just check".to_owned()]);
}

/// The newest is first, and running the same thing forty times leaves one
/// row rather than forty.
#[test]
fn remembering_a_command_moves_it_rather_than_repeating_it() {
    let mut commands = Commands::default();
    commands.remember("just check");
    commands.remember("cargo test");
    commands.remember("just check");

    assert_eq!(
        commands.recent,
        vec!["just check".to_owned(), "cargo test".to_owned()]
    );
    assert_eq!(commands.last.as_deref(), Some("just check"));
}

/// The list is short on purpose: it is what the popup offers above what
/// the project declares, and a hundred lines of history would bury that.
#[test]
fn the_history_is_capped_and_drops_the_oldest() {
    let mut commands = Commands::default();
    for n in 0..Commands::REMEMBERED + 3 {
        commands.remember(&format!("echo {n}"));
    }

    assert_eq!(commands.recent.len(), Commands::REMEMBERED);
    assert_eq!(
        commands.recent[0],
        format!("echo {}", Commands::REMEMBERED + 2)
    );
    assert!(!commands.recent.contains(&"echo 0".to_owned()));
}

/// Nothing is not a command, here as everywhere: a blank line is not
/// remembered and does not become the button's meaning.
#[test]
fn a_blank_line_is_not_remembered() {
    let mut commands = Commands::default();
    commands.remember("   ");

    assert!(commands.recent.is_empty());
    assert_eq!(commands.last, None);
}

/// **A query survives a restart and the hits do not.** M8.7: a search is
/// a walk that costs milliseconds and can be run again, and what a person
/// chose — the words, the toggles, the mask — is work they did.
#[test]
fn a_search_query_survives_a_restart() {
    let dir = temp("search-query");
    let state = ProjectState {
        search: guild_session::Search {
            query: "TabId".to_owned(),
            match_case: true,
            whole_word: false,
            regex: false,
            mask: "*.rs".to_owned(),
            ..guild_session::Search::default()
        },
        ..ProjectState::default()
    };
    project_state::save(&dir, &state).expect("saved");

    let back = project_state::load(&dir).search;

    assert_eq!(back.query, "TabId");
    assert!(back.match_case);
    assert_eq!(back.mask, "*.rs");
}

/// A file written before M8.7 reads back with an empty field rather than
/// failing the whole file — which `Store::load` would answer by throwing
/// away the project's tabs, sessions and panels.
#[test]
fn a_file_written_before_search_reads_back_with_an_empty_query() {
    let dir = temp("search-old");
    std::fs::create_dir_all(dir.join(".guild")).expect("a directory");
    std::fs::write(
        dir.join(".guild/state.json"),
        r#"{"sessions": [], "tabs": [], "active": null}"#,
    )
    .expect("an old file");

    let back = project_state::load(&dir);

    assert_eq!(back.search, guild_session::Search::default());
}

/// **Push is no longer a tool** — M8.3.6: Push Commits is a window of
/// its own. A file of M12.3's names it in a dock, and here showing it —
/// the shape taken off a real `settings.json` of 2026-09-16 — and reads
/// back with it gone and the dock showing what else it holds. The variant
/// stays, because serde fails the whole file on a name it does not know.
#[test]
fn a_dock_naming_push_reads_and_loses_it() {
    use guild_session::{Dock, Docks, Tool};

    let written = r#"{"placed": [["log", "bottom"], ["output", "bottom"], ["search", "bottom"], ["problems", "bottom"], ["commit", "right"], ["push", "bottom"], ["explorer", "left"]], "shown": [["right", "commit"], ["left", "explorer"], ["bottom", "push"]]}"#;
    let mut docks: Docks = serde_json::from_str(written).expect("an M12.3 file reads");
    assert!(docks.docked(Tool::Push), "as the file says");

    docks.adopt_missing();
    assert!(!docks.docked(Tool::Push), "and adopting takes it out");
    assert!(!docks.tools_in(Dock::Bottom).contains(&Tool::Push));
    assert_eq!(
        docks.shown(Dock::Bottom),
        Some(Tool::Git),
        "the bottom dock shows the first tool it has left"
    );
    assert!(!Tool::ALL.contains(&Tool::Push));
}

/// **A `state.json` written before M8.9 reads back whole.** The fixture is
/// a file taken off the disk — this repository's own `.guild/state.json`
/// on 2026-09-15, before the seams commit gave `Search` its history, its
/// masks and `mask_off` — and not written from memory: the trap about the
/// old `records` key is a migration test that checked a key the old file
/// never had. A struct is a wire format, and a field added without
/// `serde(default)` fails the whole file, which `Store::load` answers by
/// throwing away the project's tabs and sessions.
///
/// No file on this machine had a remembered query, so a second read puts
/// one into the same text: the shape stays the disk's.
#[test]
fn a_state_file_from_before_m8_9_keeps_its_tabs_and_its_search() {
    let text = include_str!("../fixtures/state-before-m8-9.json");
    for (name, text, query, mask) in [
        ("m89-as-found", text.to_owned(), "", ""),
        (
            "m89-with-query",
            text.replace(r#""query": """#, r#""query": "TabId""#)
                .replace(r#""mask": """#, r#""mask": "*.rs""#),
            "TabId",
            "*.rs",
        ),
    ] {
        let dir = temp(name);
        std::fs::create_dir_all(dir.join(".guild")).expect("a directory");
        std::fs::write(dir.join(".guild/state.json"), &text).expect("the old file");

        let back = project_state::load(&dir);

        assert_eq!(back.tabs.len(), 1, "{name}: the tab came back");
        assert_eq!(back.sessions.len(), 1, "{name}: the session came back");
        assert_eq!(back.recent_files.len(), 16, "{name}: the recent files too");
        assert_eq!(
            back.search,
            guild_session::Search {
                query: query.to_owned(),
                mask: mask.to_owned(),
                ..guild_session::Search::default()
            },
            "{name}: the search came back, with no history and the mask on"
        );
    }
}

/// **Search has no button of its own** — 2026-09-23. It is a Find
/// window: the popup's *Open in Find Window* brings it up, and the bar
/// names it only while it is there. The rest are always in the bar.
#[test]
fn only_search_is_a_tool_on_demand() {
    use guild_session::Tool;
    let on_demand: Vec<Tool> = Tool::ALL
        .iter()
        .copied()
        .filter(|tool| tool.on_demand())
        .collect();
    assert_eq!(on_demand, vec![Tool::Search]);
}

/// **A panel's ceiling is what the others leave** — 2026-09-24. It was
/// the window less 120, for each panel alone, so `cmd-alt-→` on the left
/// panel and `cmd-alt-←` on the right one together pushed the work off
/// the screen. What stands beside it on the same axis — the other side
/// panel, the sidebar — is taken first.
#[test]
fn a_panel_can_take_only_what_the_others_leave() {
    use guild_session::Panel;
    assert_eq!(Panel::room(1600.0, 0.0, 180.0), 1600.0 - Panel::MIN_WORK);
    assert_eq!(
        Panel::room(1600.0, 700.0, 180.0),
        1600.0 - 700.0 - Panel::MIN_WORK
    );
    assert_eq!(
        Panel::room(1000.0, 900.0, 180.0),
        180.0,
        "never under its own least"
    );
}

/// Decision 5: a seat written before M12.8 is one area holding its tabs.
#[test]
fn a_seat_written_before_m12_8_reads_as_one_area() {
    let json = r#"{"tabs": [{"shows": "settings"}], "active": 0, "expanded": []}"#;
    let seat: SeatState = serde_json::from_str(json).expect("reads");
    let saved = seat.areas_or_one();
    let only = saved.areas.focused();
    assert_eq!(saved.areas.ids(), vec![only]);
    assert_eq!(
        saved.strip(only).unwrap().tabs,
        vec![OpenTab::showing(Shows::Settings)]
    );
    assert_eq!(saved.strip(only).unwrap().active, Some(0));
}

/// …and one that was never divided is written as M12.5 wrote it.
#[test]
fn an_undivided_seat_is_written_without_areas() {
    let saved = SavedAreas::one(vec![OpenTab::showing(Shows::Settings)], Some(0));
    let seat = SeatState::from_areas(saved, vec![]);
    let value = serde_json::to_value(&seat).unwrap();
    assert!(value.get("areas").is_none(), "{value}");
    assert_eq!(value["tabs"], serde_json::json!([{"shows": "settings"}]));
}

/// Decision 5's other direction: `tabs` and `active` mirror the focused
/// area, so an older build opens what the person was looking at.
#[test]
fn the_old_fields_mirror_the_focused_area() {
    let mut saved = SavedAreas::one(vec![OpenTab::showing(Shows::Settings)], Some(0));
    let second = saved
        .areas
        .split(saved.areas.focused(), Axis::Horizontal)
        .unwrap();
    let file = OpenTab::showing(Shows::File("src/lib.rs".into()));
    saved.strip_mut(second).tabs.push(file.clone());
    saved.strip_mut(second).active = Some(0);
    let seat = SeatState::from_areas(saved, vec![]);
    assert_eq!(seat.tabs, vec![file]);
    assert_eq!(seat.active, Some(0));
    assert!(seat.areas.is_some());
}

/// …or, when the focused area is empty, the first area that has tabs —
/// an older build opening an empty window reads as lost tabs.
#[test]
fn an_empty_focused_area_mirrors_the_first_area_with_tabs() {
    let mut saved = SavedAreas::one(vec![OpenTab::showing(Shows::Settings)], Some(0));
    saved
        .areas
        .split(saved.areas.focused(), Axis::Horizontal)
        .unwrap();
    let seat = SeatState::from_areas(saved, vec![]);
    assert_eq!(seat.tabs, vec![OpenTab::showing(Shows::Settings)]);
}

/// Invariant 2: an empty area survives a restart.
#[test]
fn an_empty_area_survives_a_round_trip() {
    let mut saved = SavedAreas::one(vec![OpenTab::showing(Shows::Settings)], Some(0));
    let second = saved
        .areas
        .split(saved.areas.focused(), Axis::Vertical)
        .unwrap();
    let seat = SeatState::from_areas(saved, vec![]);
    let back: SeatState = serde_json::from_str(&serde_json::to_string(&seat).unwrap()).unwrap();
    let back = back.areas_or_one();
    assert_eq!(back.areas.ids().len(), 2);
    assert_eq!(back.areas.focused(), second);
    assert!(back.strip(second).unwrap().tabs.is_empty());
}

#[test]
fn a_tab_written_before_m12_8_reads_as_one_pane_showing_what_it_showed() {
    let tab: OpenTab = serde_json::from_str(r#"{"shows": "settings"}"#).expect("reads");
    let panes = tab.panes_or_one();
    assert_eq!(panes.ids().len(), 1);
    assert_eq!(panes.body(), Some(&Shows::Settings));
}

#[test]
fn a_tab_of_one_pane_is_written_as_m12_5_wrote_it() {
    let value = serde_json::to_value(OpenTab::showing(Shows::Settings)).unwrap();
    assert_eq!(value, serde_json::json!({"shows": "settings"}));
}

#[test]
fn a_divided_tab_mirrors_its_focused_pane_for_an_older_build() {
    let mut panes = Panes::one(Shows::Settings);
    let second = panes.split(panes.focused(), Axis::Vertical).unwrap();
    panes
        .fill(second, Shows::File("src/lib.rs".into()))
        .unwrap();
    let tab = OpenTab::from_panes(panes).expect("a pane shows something");
    assert_eq!(tab.shows, Shows::File("src/lib.rs".into()));
    let back: OpenTab = serde_json::from_str(&serde_json::to_string(&tab).unwrap()).unwrap();
    assert_eq!(back.panes_or_one().ids().len(), 2);
}

/// `restorable`'s rule, per pane: a pane whose file is gone is dropped,
/// and the tab with it when nothing is left.
#[test]
fn a_pane_whose_file_is_gone_is_dropped_and_the_tab_with_the_last() {
    let gone = Shows::File("gone.rs".into());
    let mut panes = Panes::one(gone.clone());
    let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
    panes.fill(second, Shows::Settings).unwrap();
    let tab = OpenTab::from_panes(panes).unwrap();
    let kept = tab
        .keeping(|shows| *shows != gone)
        .expect("settings is left");
    assert_eq!(kept.panes_or_one().ids().len(), 1);
    assert_eq!(kept.shows, Shows::Settings);
    assert!(OpenTab::showing(gone).keeping(|_| false).is_none());
}

/// Panes that show nothing are not written by this build; a file that
/// says so is read as the tab's `shows`, never as a tab showing nothing.
#[test]
fn panes_that_show_nothing_fall_back_to_shows() {
    let mut panes = Panes::one(Shows::Settings);
    let only = panes.focused();
    panes.split(only, Axis::Horizontal).unwrap();
    panes.take(only);
    assert_eq!(panes.showing(), 0);
    let tab = OpenTab {
        shows: Shows::File("a.rs".into()),
        panes: Some(panes),
        markdown_mode: None,
        markdown_modes: Default::default(),
    };
    let back: OpenTab = serde_json::from_str(&serde_json::to_string(&tab).unwrap()).unwrap();
    let read = back.panes_or_one();
    assert_eq!(read.ids().len(), 1);
    assert_eq!(read.body(), Some(&Shows::File("a.rs".into())));
}

/// M12.8, task 12: a divided tab comes back as it was left — every pane's
/// `shows`, the division with the fraction a person dragged, the focus on
/// the pane that had it, and an empty pane as an empty pane (decision 13:
/// written as a leaf with no entry).
#[test]
fn a_divided_tab_round_trips_its_fractions_its_focus_and_its_empty_pane() {
    use guild_session::layout::{Floor, Frame};
    let mut panes = Panes::one(Shows::File("src/lib.rs".into()));
    let first = panes.focused();
    let second = panes.split(first, Axis::Horizontal).unwrap();
    panes.fill(second, Shows::Settings).unwrap();
    let third = panes.split(second, Axis::Vertical).unwrap();
    let room = Frame::new(0.0, 0.0, 1200.0, 800.0);
    let floor = Floor {
        width: 320.0,
        height: 160.0,
    };
    assert!(panes.tree_mut().drag(&[], 240.0, room, floor), "dragged");
    panes.focus(first);
    let tab = OpenTab::from_panes(panes.clone()).expect("two panes show something");

    let back: OpenTab = serde_json::from_str(&serde_json::to_string(&tab).unwrap()).unwrap();
    let read = back.panes_or_one();
    assert_eq!(
        read, panes,
        "the tree, its fraction, the focus and the bodies"
    );
    assert_eq!(read.focused(), first);
    assert_eq!(read.body_of(third), None, "the empty pane stays empty");
    assert_eq!(read.ids().len(), 3);
}

/// Git has removed the contents and is about to remove the root. A seat
/// still saving must not recreate .guild between those two filesystem deeds.
#[test]
fn a_removal_excludes_state_writes_until_git_has_finished() {
    let base = temp("removal-gate");
    let root = base.join("worktree");
    std::fs::create_dir(&root).unwrap();
    project_state::save(&root, &ProjectState::default()).unwrap();
    flush();

    let removal = project_state::prepare_removal(&root).unwrap();
    std::fs::remove_dir_all(root.join(".guild")).unwrap();
    let late_save = project_state::save(&root, &ProjectState::default());
    flush();
    let removed = std::fs::remove_dir(&root);
    assert!(
        removed.is_ok(),
        "git's final rmdir must succeed: {removed:?}"
    );
    assert!(
        late_save.is_err(),
        "a caller must know its state was not saved"
    );
    drop(removal);
    project_state::save(&root, &ProjectState::default()).unwrap();
    flush();
    assert!(!root.exists(), "a late save cannot resurrect the worktree");
}

/// Quiescing one root must neither discard its last snapshot on a refused
/// removal nor suppress saves of roots in other workspaces.
#[test]
fn a_removal_drains_its_snapshot_and_releases_saving_after_refusal() {
    let base = temp("removal-refused");
    let root = base.join("worktree");
    let survivor = base.join("survivor");
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(&survivor).unwrap();
    let state = ProjectState {
        tabs: vec![OpenTab::showing(Shows::Settings)],
        ..Default::default()
    };
    project_state::save(&root, &state).unwrap();
    let removal = project_state::prepare_removal(&root).unwrap();
    let on_disk: ProjectState = serde_json::from_slice(
        &std::fs::read(root.join(".guild/state.json")).expect("queued save drained"),
    )
    .unwrap();
    assert_eq!(on_disk.tabs, state.tabs);
    project_state::save(&survivor, &state).unwrap();
    flush();
    assert_eq!(project_state::load(&survivor).tabs, state.tabs);
    // Git refused: the root is still there when the guard is dropped.
    drop(removal);
    project_state::save(&root, &ProjectState::default()).unwrap();
    flush();
    assert!(project_state::load(&root).tabs.is_empty());
    assert_eq!(project_state::load(&survivor).tabs, state.tabs);
}

#[cfg(unix)]
#[test]
fn a_removal_and_saves_through_a_symlink_share_one_queue_and_gate() {
    let base = temp("removal-alias");
    let root = base.join("worktree");
    let alias = base.join("alias");
    std::fs::create_dir(&root).unwrap();
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let state = ProjectState {
        tabs: vec![OpenTab::showing(Shows::Settings)],
        ..Default::default()
    };
    project_state::save(&alias, &state).unwrap();
    // Loading through the other spelling must see the latest queued state.
    assert_eq!(project_state::load(&root).tabs, state.tabs);
    let removal = project_state::prepare_removal(&root).unwrap();
    let bytes = std::fs::read(root.join(".guild/state.json")).unwrap();
    let on_disk: ProjectState = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(on_disk.tabs, state.tabs);
    assert!(project_state::save(&alias, &ProjectState::default()).is_err());
    assert!(project_state::prepare_removal(&alias).is_err());
    drop(removal);
    project_state::save(&alias, &ProjectState::default()).unwrap();
    flush();
    assert!(project_state::load(&root).tabs.is_empty());
}

/// Restart and Update use the same on-disk project records. Identity is
/// independent of a completed-turn witness or the harness chosen by default.
#[test]
fn codex_restart_preserves_its_native_identity_before_a_completed_turn() {
    use guild_session::{Conversation, Resumption, resumption};
    let dir = temp("codex-restart");
    let mut record = session(ProjectId::new(), &dir);
    record.runs = Runs::Codex(SessionMode::Terminal);
    record.harness_session = Some("native-codex-conversation".into());
    let id = record.id;
    let state = ProjectState {
        sessions: vec![record],
        tabs: vec![OpenTab::showing(Shows::Session(id))],
        ..Default::default()
    };
    project_state::save(&dir, &state).unwrap();
    flush();
    let back = project_state::load(&dir);
    let record = &back.sessions[0];
    assert_eq!(record.id, id);
    assert_eq!(record.runs, Runs::Codex(SessionMode::Terminal));
    assert!(!record.had_turn);
    assert_eq!(back.tabs[0].shows, Shows::Session(id));
    assert_eq!(
        resumption(
            record.runs,
            &id.to_string(),
            record.harness_session.as_deref(),
            || panic!("Codex must not consult Claude transcripts")
        ),
        Resumption::Continue(Conversation::Resume("native-codex-conversation".into()))
    );
}

#[test]
fn review_tab_round_trips_as_a_non_session_surface() {
    let tab = OpenTab::showing(Shows::LegacyReview);
    let json = serde_json::to_string(&tab).unwrap();
    assert_eq!(serde_json::from_str::<OpenTab>(&json).unwrap(), tab);
    assert!(json.contains("review"));
}

#[test]
fn retiring_a_background_review_preserves_an_empty_siblings_focus_after_reload() {
    let mut panes = Panes::one(Shows::File("src/lib.rs".into()));
    let file = panes.focused();
    let review = panes.split(file, Axis::Horizontal).unwrap();
    panes.fill(review, Shows::LegacyReview).unwrap();
    let empty = panes.split(review, Axis::Vertical).unwrap();
    assert_eq!(panes.focused(), empty);
    let tab = OpenTab::from_panes(panes.clone()).unwrap();
    let restored = tab.without_native_review().unwrap();
    panes.take(review);
    assert_eq!(restored.panes_or_one(), panes);

    let dir = temp("retired-review-empty-focus");
    project_state::save(
        &dir,
        &ProjectState {
            tabs: vec![restored],
            ..Default::default()
        },
    )
    .unwrap();
    flush();
    let reloaded = project_state::load(&dir);
    assert_eq!(reloaded.tabs[0].panes_or_one(), panes);
    assert_eq!(reloaded.tabs[0].panes_or_one().focused(), empty);
}

#[test]
fn terminal_is_adopted_from_old_dock_settings_and_moves_without_resetting_neighbors() {
    use guild_session::{Dock, Docks, Tool};
    let mut docks: Docks =
        serde_json::from_str(r#"{"placed":[["explorer","right"]],"shown":[["right","explorer"]]}"#)
            .unwrap();
    docks.adopt_missing();
    assert_eq!(docks.dock_of(Tool::Terminal), Dock::Bottom);
    assert_eq!(docks.dock_of(Tool::Explorer), Dock::Right);
    docks.move_tool(Tool::Terminal, Dock::Left);
    let read: Docks = serde_json::from_str(&serde_json::to_string(&docks).unwrap()).unwrap();
    assert_eq!(read.shown(Dock::Left), Some(Tool::Terminal));
    assert_eq!(read.dock_of(Tool::Explorer), Dock::Right);
}

/// Mixed Process panes keep legacy body tags and do not borrow a session ID.
#[test]
fn process_reference_survives_mixed_panes_without_changing_old_tags() {
    let run: guild_session::workflow::RunId =
        "23f806ea-25ea-4db8-9fe0-6cf13b941e77".parse().unwrap();
    let id = SessionId::new();
    let old = vec![
        Shows::Session(id),
        Shows::LegacyReview,
        Shows::Settings,
        Shows::File("src/main.rs".into()),
    ];
    assert_eq!(
        serde_json::to_value(&old).unwrap(),
        serde_json::json!([{"session":id},"review","settings",{"file":"src/main.rs"}])
    );
    let mut panes = Panes::one(Shows::Process { run });
    let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
    panes.fill(second, Shows::LegacyReview).unwrap();
    let tab = OpenTab::from_panes(panes).unwrap();
    let state = ProjectState {
        tabs: vec![tab],
        ..Default::default()
    };
    let dir = temp("process-mixed");
    project_state::save(&dir, &state).unwrap();
    flush();
    let back = project_state::load(&dir);
    assert_eq!(back.tabs, state.tabs);
    assert!(
        back.tabs[0]
            .panes_or_one()
            .bodies()
            .iter()
            .any(|(_, b)| **b == Shows::Process { run })
    );
}

/// **A change built on what a project holds is read only from a folder
/// that is there** — GLD-058. `load` answers an empty state for a folder
/// that is away, a volume not mounted, and a change saved on top of that
/// once the folder is back writes the emptiness over what it holds.
#[test]
fn a_change_is_read_only_from_a_folder_that_is_there() {
    let base = temp("for-change");
    let missing = base.join("unmounted");
    assert!(
        project_state::load_for_change(&missing).is_none(),
        "an absent folder gives nothing to change"
    );

    let fresh = base.join("fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    let state = project_state::load_for_change(&fresh).expect("a folder with no state yet");
    assert!(state.tabs.is_empty() && state.seats.is_empty());

    let kept = base.join("kept");
    std::fs::create_dir_all(&kept).unwrap();
    let held = ProjectState {
        tabs: vec![OpenTab::showing(Shows::Settings)],
        ..Default::default()
    };
    project_state::save(&kept, &held).unwrap();
    flush();
    assert_eq!(
        project_state::load_for_change(&kept)
            .expect("its state")
            .tabs,
        held.tabs
    );

    // A file that is there and cannot be read is not something to change.
    let locked = base.join("locked");
    std::fs::create_dir_all(&locked).unwrap();
    project_state::save(&locked, &held).unwrap();
    flush();
    let file = project_state::dir(&locked).join("state.json");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o000)).unwrap();
    let read = project_state::load_for_change(&locked);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        read.is_none(),
        "an unreadable state gives nothing to change"
    );
}
