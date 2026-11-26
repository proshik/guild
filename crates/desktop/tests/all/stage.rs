// crates/desktop/tests/all/stage.rs
//! What is in front of a seat — M12.8, decision 11. Pure: the window says
//! which tabs there are and where; this answers which area has the keys,
//! which tab is in front of each, where a new one lands and where an
//! arrow goes.

use guild_desktop::stage::{self, Place, Stage};
use guild_session::layout::{Axis, Direction, Frame, toward};
use guild_session::{AreaId, LastArea, PaneId};

fn two(stage: &mut Stage<&'static str, u32>) -> (AreaId, AreaId) {
    let a = stage.landing("guild");
    let b = stage.split("guild", a, Axis::Horizontal).expect("split");
    (a, b)
}

/// Decision 15.
#[test]
fn a_new_tab_lands_in_the_seat_s_focused_area() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    assert_eq!(stage.landing("guild"), b, "a new area has the keys");
    stage.bring_forward("guild", a, 1);
    assert_eq!(stage.landing("guild"), a);
}

/// Decision 7.
#[test]
fn each_area_keeps_its_own_tab_in_front() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    stage.bring_forward("guild", a, 2);
    stage.bring_forward("guild", b, 7);
    assert_eq!(stage.front(a, &[1, 2, 3]), Some(2));
    assert_eq!(stage.front(b, &[7, 8]), Some(7));
    assert_eq!(stage.focused("guild"), Some(b));
}

/// M12.9's left neighbour, within the area.
#[test]
fn closing_the_front_tab_brings_its_left_neighbour_in_its_own_area() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    stage.bring_forward("guild", b, 7);
    stage.bring_forward("guild", a, 3);
    stage.closing(a, &[1, 2, 3], 3);
    assert_eq!(stage.front(a, &[1, 2]), Some(2));
    assert_eq!(
        stage.front(b, &[7, 8]),
        Some(7),
        "the other area is untouched"
    );
}

/// Decision 7: the focused area is read from the tab in front.
#[test]
fn the_focus_is_read_from_the_tab_in_front() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    assert_eq!(stage.focused_with("guild", Some(("guild", a))), Some(a));
    assert_eq!(stage.focused_with("guild", None), Some(b));
    assert_eq!(
        stage.focused_with("guild", Some(("meadow", a))),
        Some(b),
        "another seat's tab says nothing about this one"
    );
}

#[test]
fn the_last_area_is_not_closed() {
    let mut stage: Stage<&str, u32> = Stage::default();
    let only = stage.landing("guild");
    assert_eq!(stage.close_area("guild", only), Err(LastArea));
}

/// Decision 9: closing an area closes no tab. **Its tabs arrive
/// together, after the heir's, in their own order** — the final review's
/// ruling: the window's one list is history, and `a1, b1, a2` interleaved
/// `b1` between the heir's tabs by nothing the person had arranged.
#[test]
fn a_closed_area_hands_its_tabs_to_its_heir() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    // Another area's tab — another seat's — keeps its place in the list.
    let elsewhere = AreaId::new();
    let mut order = vec![(1, a), (2, b), (7, elsewhere), (4, a), (3, b), (5, a)];
    let heir = stage.close_area("guild", b).expect("two areas");
    assert_eq!(heir, a);
    stage::rehome(&mut order, b, heir);
    assert_eq!(stage::strip(&order, a), vec![1, 4, 5, 2, 3]);
    assert_eq!(stage::strip(&order, elsewhere), vec![7]);
    assert_eq!(order.len(), 6, "a permutation: no tab made or lost");
    assert_eq!(stage.focused("guild"), Some(a));
}

/// Task 12: a reopened tab goes back to its area, or to the focused one
/// when that area is gone.
#[test]
fn a_reopened_tab_goes_back_to_its_area_or_else_the_focused_one() {
    let mut stage: Stage<&str, u32> = Stage::default();
    let (a, b) = two(&mut stage);
    stage.focus("guild", a);
    assert_eq!(stage.reopening("guild", Some(b)), b);
    stage.close_area("guild", b).unwrap();
    assert_eq!(stage.reopening("guild", Some(b)), a);
    assert_eq!(stage.reopening("guild", None), a);
}

#[test]
fn a_moved_seat_keeps_its_areas_and_a_forgotten_one_loses_them() {
    let mut stage: Stage<&str, u32> = Stage::default();
    two(&mut stage);
    stage.moved("guild", "guild-elsewhere");
    assert!(stage.areas("guild").is_none());
    assert_eq!(stage.areas("guild-elsewhere").unwrap().ids().len(), 2);
    stage.forget("guild-elsewhere");
    assert!(stage.areas("guild-elsewhere").is_none());
}

/// Decision 8: a tab dragged onto another area's tab moves there, beside
/// it; the window's one order keeps every other tab where it was.
#[test]
fn a_tab_dragged_onto_another_area_s_tab_moves_there_beside_it() {
    let (a, b) = (AreaId::new(), AreaId::new());
    let mut order = vec![(1, a), (2, a), (3, b), (4, b)];
    assert!(stage::reorder(&mut order, 1, 4, true));
    assert_eq!(stage::strip(&order, a), vec![2]);
    assert_eq!(stage::strip(&order, b), vec![3, 1, 4]);
}

/// The same drag the other way along the window's list: a tab dropped
/// after a tab of another area that comes earlier in the list lands
/// after it, in that area.
#[test]
fn a_tab_dropped_after_an_earlier_area_s_tab_lands_after_it() {
    let (a, b) = (AreaId::new(), AreaId::new());
    let mut order = vec![(1, a), (2, a), (3, b), (4, b)];
    assert!(stage::reorder(&mut order, 4, 1, false));
    assert_eq!(stage::strip(&order, a), vec![1, 4, 2]);
    assert_eq!(stage::strip(&order, b), vec![3]);
}

/// Decision 8: *Move to Area ▸* puts the tab at the end of the other
/// area's strip, and every other tab keeps its place.
#[test]
fn a_tab_moved_from_its_menu_goes_to_the_end_of_the_other_strip() {
    let (a, b) = (AreaId::new(), AreaId::new());
    let mut order = vec![(1, a), (2, b), (3, a), (4, b)];
    assert!(stage::move_to_end(&mut order, 1, b));
    assert_eq!(stage::strip(&order, a), vec![3]);
    assert_eq!(stage::strip(&order, b), vec![2, 4, 1]);
}

/// Decision 8: a tab carried to another area comes forward there with
/// the keys, and the area it left brings its neighbour forward — as it
/// would had the tab been closed — rather than keep a front it no longer
/// holds.
#[test]
fn a_carried_tab_comes_forward_where_it_lands_and_its_old_area_keeps_a_neighbour() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    stage.bring_forward("guild", b, 7);
    stage.bring_forward("guild", a, 2);
    stage.carry("guild", 2, a, &[1, 2, 3], b);
    assert_eq!(stage.front(a, &[1, 3]), Some(1), "the left neighbour");
    assert_eq!(stage.front(b, &[7, 2]), Some(2));
    assert_eq!(stage.focused("guild"), Some(b));

    // A tab that was not its area's front leaves that front alone.
    stage.bring_forward("guild", a, 3);
    stage.carry("guild", 1, a, &[1, 3], b);
    assert_eq!(stage.front(a, &[3]), Some(3));
    assert_eq!(stage.front(b, &[7, 2, 1]), Some(1));
}

/// The final review's Minor 2: a live drag carries the tab into every
/// area it passes over, and an area it merely passed through ends with
/// the front it had — dragging from A to C across B leaves B as it was,
/// rather than showing whichever neighbour of the tab's brief place came
/// forward when it left.
#[test]
fn an_area_a_carried_tab_passes_through_keeps_its_front() {
    let mut stage = Stage::default();
    let a = stage.landing("guild");
    let b = stage.split("guild", a, Axis::Horizontal).expect("split");
    let c = stage.split("guild", b, Axis::Horizontal).expect("split");
    stage.bring_forward("guild", c, 9);
    stage.bring_forward("guild", b, 5);
    stage.bring_forward("guild", a, 1);
    // Over B's strip, between 4 and 5, and on to C.
    stage.carry("guild", 1, a, &[1, 2], b);
    stage.carry("guild", 1, b, &[4, 1, 5, 6], c);
    assert_eq!(
        stage.front(b, &[4, 5, 6]),
        Some(5),
        "B shows what it showed"
    );
    assert_eq!(stage.front(c, &[9, 1]), Some(1));
    assert_eq!(stage.front(a, &[2]), Some(2));

    // Across B and back to A: B as it was, the tab in front of A again.
    stage.carry("guild", 1, c, &[9, 1], b);
    stage.carry("guild", 1, b, &[4, 5, 1, 6], a);
    assert_eq!(stage.front(b, &[4, 5, 6]), Some(5));
    assert_eq!(stage.front(c, &[9]), Some(9), "C as it was too");
    assert_eq!(stage.front(a, &[2, 1]), Some(1));
}

/// …and only while the tab stays the front it was carried in as: once
/// another tab is brought forward there, the area's own front is that
/// one, and the tab leaving is an ordinary leaving.
#[test]
fn an_area_whose_front_was_chosen_since_forgets_what_it_had() {
    let mut stage = Stage::default();
    let (a, b) = two(&mut stage);
    stage.bring_forward("guild", b, 5);
    stage.bring_forward("guild", a, 1);
    stage.carry("guild", 1, a, &[1, 2], b);
    stage.bring_forward("guild", b, 6);
    stage.bring_forward("guild", b, 1);
    stage.carry("guild", 1, b, &[5, 6, 1], a);
    assert_eq!(stage.front(b, &[5, 6]), Some(6), "the left neighbour");
}

/// Decision 21: *Move to Area ▸* names the areas an arrow from the tab's
/// own area reaches — left, right, up, down — and nothing where there is
/// nothing.
#[test]
fn an_area_s_neighbours_are_what_an_arrow_from_it_reaches() {
    // One area on the left, two stacked on the right.
    let (left, top, bottom) = (AreaId::new(), AreaId::new(), AreaId::new());
    let areas = [
        (left, Frame::new(0.0, 0.0, 500.0, 600.0)),
        (top, Frame::new(500.0, 0.0, 500.0, 300.0)),
        (bottom, Frame::new(500.0, 300.0, 500.0, 300.0)),
    ];
    assert_eq!(
        stage::neighbours(&areas, top),
        [Some(left), None, None, Some(bottom)]
    );
    assert_eq!(
        stage::neighbours(&areas, bottom),
        [Some(left), None, Some(top), None]
    );
    assert_eq!(stage::neighbours(&areas, left)[0], None);
    assert!(stage::neighbours(&areas, left)[1].is_some());
    assert_eq!(
        stage::neighbours(&[(left, Frame::new(0.0, 0.0, 500.0, 600.0))], left),
        [None; 4],
        "one area has nowhere to send a tab but a new one"
    );
}

#[test]
fn a_drag_that_lands_where_it_began_changes_nothing() {
    let a = AreaId::new();
    let mut order = vec![(1, a), (2, a)];
    assert!(!stage::reorder(&mut order, 1, 2, true));
    assert!(!stage::reorder(&mut order, 1, 1, false));
    assert_eq!(order, vec![(1, a), (2, a)]);
}

#[test]
fn a_tab_dropped_on_an_empty_area_is_its_only_tab() {
    let (a, c) = (AreaId::new(), AreaId::new());
    let mut order = vec![(1, a), (2, a)];
    assert!(stage::move_to_end(&mut order, 2, c));
    assert_eq!(stage::strip(&order, c), vec![2]);
    assert!(!stage::move_to_end(&mut order, 2, c), "already there");
}

/// Decision 14: one geometry — every visible pane, and an empty area as
/// one place — so an arrow crosses from a pane into the next area.
#[test]
fn every_visible_pane_is_a_place_and_an_empty_area_is_one_too() {
    let (a, b) = (AreaId::new(), AreaId::new());
    let (left, right) = (PaneId::new(), PaneId::new());
    let areas = [
        (a, Frame::new(0.0, 0.0, 500.0, 600.0)),
        (b, Frame::new(500.0, 0.0, 500.0, 600.0)),
    ];
    let places = stage::places(&areas, 36.0, |area, body| {
        if area == a {
            vec![
                (left, Frame::new(body.x, body.y, 250.0, body.height)),
                (
                    right,
                    Frame::new(body.x + 250.0, body.y, 250.0, body.height),
                ),
            ]
        } else {
            vec![]
        }
    });
    let at = |area, pane| Place { area, pane };
    assert_eq!(
        places,
        vec![
            (at(a, Some(left)), Frame::new(0.0, 36.0, 250.0, 564.0)),
            (at(a, Some(right)), Frame::new(250.0, 36.0, 250.0, 564.0)),
            (at(b, None), Frame::new(500.0, 36.0, 500.0, 564.0)),
        ]
    );
    assert_eq!(
        toward(&places, &at(a, Some(right)), Direction::Right),
        Some(at(b, None))
    );
    assert_eq!(
        toward(&places, &at(b, None), Direction::Left),
        Some(at(a, Some(right)))
    );
}

/// A tab with no name of its own is *Session N* by its place in **its
/// own** strip — what the strip draws, and so what the close sheet and
/// the agents list must say. The sheet counted every tab of the project
/// across the areas, and asked `Close "Session 5"?` of a tab the strip
/// called *Session 1*: the fifth of the seat, the first of its area
/// (M12.8's window check, F5).
#[test]
fn a_tab_s_number_is_its_place_in_its_own_strip() {
    // Four tabs on the left, a new one alone in the area on the right.
    let order = [
        (1, "left"),
        (2, "left"),
        (3, "left"),
        (4, "left"),
        (5, "right"),
    ];
    assert_eq!(stage::place(&order, 5), Some(0));
    assert_eq!(stage::place(&order, 3), Some(2));
    // The window's one list interleaves the strips; a place is counted
    // among the tabs of the same strip only.
    let order = [(1, "left"), (5, "right"), (2, "left"), (6, "right")];
    assert_eq!(stage::place(&order, 6), Some(1));
    assert_eq!(stage::place(&order, 2), Some(1));
    assert_eq!(stage::place(&order, 9), None);
}

/// **A divided tab's mark is its most urgent pane's** — M12.8's second
/// window check, O7: an agent's tab with its shell pane focused read as
/// a shell, and the agent's pulse and its dot left the strip while it
/// worked or waited. The name stays the focused pane's; the mark is the
/// pane that asks most of the person, by `Status::urgency` — the order a
/// project's tab and the Dock already fold statuses by — and the focused
/// pane, listed first, among equals.
#[test]
fn a_divided_tab_wears_its_most_urgent_pane_s_mark() {
    use guild_session::status::{Status, Why};
    // The focused shell first, as the window lists them.
    assert_eq!(
        stage::loudest([("shell", Status::Idle), ("agent", Status::Working)]),
        Some("agent")
    );
    assert_eq!(
        stage::loudest([
            ("working", Status::Working),
            ("blocked", Status::Waiting(Why::Permission)),
            ("done", Status::Done),
        ]),
        Some("blocked")
    );
    // Nothing to say anywhere: the focused pane's own mark.
    assert_eq!(
        stage::loudest([("shell", Status::Idle), ("agent", Status::Idle)]),
        Some("shell")
    );
    assert_eq!(
        stage::loudest([("focused", Status::Working), ("other", Status::Working)]),
        Some("focused")
    );
    assert_eq!(stage::loudest::<&str>([]), None);
}
