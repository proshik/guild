// crates/session/tests/all/areas.rs
//! A seat's areas — M12.8. The division and the area with the keys;
//! what each area holds is the window's, and the state file's (task 3).

use guild_session::areas::{Areas, LastArea};
use guild_session::layout::Axis;

#[test]
fn a_seat_starts_as_one_focused_area() {
    let areas = Areas::one();
    assert_eq!(areas.ids(), vec![areas.focused()]);
}

#[test]
fn splitting_makes_an_area_beside_and_focuses_it() {
    let mut areas = Areas::one();
    let first = areas.focused();
    let second = areas
        .split(first, Axis::Horizontal)
        .expect("the area is there");
    assert_ne!(first, second);
    assert_eq!(areas.focused(), second);
    assert_eq!(areas.ids(), vec![first, second]);
}

#[test]
fn an_area_that_is_not_there_is_not_split() {
    let mut areas = Areas::one();
    let elsewhere = Areas::one().focused();
    assert_eq!(areas.split(elsewhere, Axis::Vertical), None);
    assert_eq!(areas.ids().len(), 1);
}

/// Invariant 1.
#[test]
fn the_last_area_cannot_be_closed() {
    let mut areas = Areas::one();
    let only = areas.focused();
    assert_eq!(areas.close(only), Err(LastArea));
    assert_eq!(areas.ids(), vec![only]);
}

/// Decision 9: the area that takes the room is the heir, and takes the
/// focus only when the closed one had it.
#[test]
fn a_closed_area_s_heir_is_the_one_that_takes_its_room() {
    let mut areas = Areas::one();
    let a = areas.focused();
    let b = areas.split(a, Axis::Horizontal).unwrap();
    let c = areas.split(b, Axis::Horizontal).unwrap();
    assert_eq!(areas.ids(), vec![a, b, c]);
    assert!(areas.focus(a));
    assert_eq!(areas.close(b), Ok(c));
    assert_eq!(
        areas.focused(),
        a,
        "an area closed out of focus leaves the focus alone"
    );
    assert_eq!(areas.close(a), Ok(c));
    assert_eq!(areas.focused(), c);
}

#[test]
fn an_area_that_is_not_there_is_not_focused() {
    let mut areas = Areas::one();
    let first = areas.focused();
    assert!(!areas.focus(Areas::one().focused()));
    assert_eq!(areas.focused(), first);
}

#[test]
fn areas_survive_a_round_trip_with_their_focus() {
    let mut areas = Areas::one();
    let first = areas.focused();
    areas.split(first, Axis::Vertical);
    areas.focus(first);
    let back: Areas = serde_json::from_str(&serde_json::to_string(&areas).unwrap()).unwrap();
    assert_eq!(back, areas);
}
