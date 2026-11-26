//! Where the Recent Projects popup sits, and what a drag does to it.
//!
//! The rules are geometry, so they are checked here rather than by opening
//! a window and looking: "does the left edge stop when the popup is at its
//! narrowest" is a question about arithmetic, and arithmetic is the part
//! that goes wrong.

use guild_session::{Grab, PickerPlace};

/// A window big enough that nothing is clamped by its edges.
const ROOM: (f32, f32) = (1600.0, 1000.0);

/// How far down the popup starts when it has not been moved.
const TOP: f32 = 64.0;

#[test]
fn an_unmoved_popup_is_centred_under_the_title_bar() {
    let place = PickerPlace::default();
    let (x, y) = place.place(ROOM, TOP);

    assert_eq!(x, (ROOM.0 - place.width) / 2.0, "centred across the window");
    assert_eq!(y, TOP, "and below the title bar, not over it");
}

/// The window's width changes; a position expressed as a number would not
/// follow it, and a popup that was centred yesterday would be off-centre
/// today.
#[test]
fn the_default_position_follows_the_window() {
    let place = PickerPlace::default();

    let (narrow, _) = place.place((900.0, 700.0), TOP);
    let (wide, _) = place.place((1900.0, 700.0), TOP);

    assert!(wide > narrow, "a wider window centres it further right");
}

#[test]
fn dragging_the_header_moves_the_popup() {
    let mut place = PickerPlace::default();
    let (x, y) = place.place(ROOM, TOP);

    place.drag(Grab::Move, (40.0, 30.0), ROOM, TOP);
    let (moved_x, moved_y) = place.place(ROOM, TOP);

    assert_eq!(moved_x, x + 40.0);
    assert_eq!(moved_y, y + 30.0);
}

/// A popup dragged out of the window cannot be dragged back: there is
/// nothing left to take hold of.
#[test]
fn a_popup_cannot_be_dragged_off_the_window() {
    let mut place = PickerPlace::default();

    place.drag(Grab::Move, (-5000.0, -5000.0), ROOM, TOP);
    let (x, y) = place.place(ROOM, TOP);
    assert!(x >= 0.0 && y >= 0.0, "the top left corner stays on screen");

    place.drag(Grab::Move, (5000.0, 5000.0), ROOM, TOP);
    let (x, y) = place.place(ROOM, TOP);
    assert!(x < ROOM.0, "and the far corner does not leave either");
    assert!(y < ROOM.1);
}

#[test]
fn the_right_edge_widens_and_narrows() {
    let mut place = PickerPlace::default();
    let was = place.width;

    place.drag(Grab::Right, (60.0, 0.0), ROOM, TOP);
    assert_eq!(place.width, was + 60.0);

    place.drag(Grab::Right, (-60.0, 0.0), ROOM, TOP);
    assert_eq!(place.width, was);
}

/// The left edge moves the popup and changes its width by the same amount:
/// the right edge must not travel while the left one is dragged.
#[test]
fn the_left_edge_keeps_the_right_edge_still() {
    let mut place = PickerPlace::default();
    let (x, _) = place.place(ROOM, TOP);
    let right = x + place.width;

    place.drag(Grab::Left, (50.0, 0.0), ROOM, TOP);

    let (moved, _) = place.place(ROOM, TOP);
    assert_eq!(moved, x + 50.0, "the left edge follows the hand");
    assert_eq!(moved + place.width, right, "the right edge does not move");
}

/// **The trap.** At the narrowest width the left edge has to stop *with*
/// the width — carry on moving it and the popup walks right while its
/// right edge stays put, which looks like the popup shrinking on its own.
#[test]
fn the_left_edge_stops_where_the_width_stops() {
    let mut place = PickerPlace::default();

    place.drag(Grab::Left, (5000.0, 0.0), ROOM, TOP);
    let (x, _) = place.place(ROOM, TOP);
    let right = x + place.width;

    place.drag(Grab::Left, (200.0, 0.0), ROOM, TOP);
    let (again, _) = place.place(ROOM, TOP);

    assert_eq!(place.width, PickerPlace::MIN_WIDTH, "as narrow as it goes");
    assert_eq!(again, x, "and the left edge stopped with it");
    assert_eq!(again + place.width, right, "the right edge never moved");
}

#[test]
fn a_size_is_bounded_at_both_ends() {
    let mut place = PickerPlace::default();

    place.drag(Grab::Right, (9000.0, 0.0), ROOM, TOP);
    place.drag(Grab::Bottom, (0.0, 9000.0), ROOM, TOP);
    assert_eq!(place.width, PickerPlace::MAX_WIDTH);
    assert_eq!(place.height, PickerPlace::MAX_HEIGHT);

    place.drag(Grab::Right, (-9000.0, 0.0), ROOM, TOP);
    place.drag(Grab::Bottom, (0.0, -9000.0), ROOM, TOP);
    assert_eq!(place.width, PickerPlace::MIN_WIDTH);
    assert_eq!(place.height, PickerPlace::MIN_HEIGHT);
}

/// A corner is the two edges it joins, and it must not be a special case
/// that drifts from them.
#[test]
fn a_corner_is_its_two_edges() {
    let mut corner = PickerPlace::default();
    let mut edges = PickerPlace::default();

    corner.drag(Grab::BottomRight, (40.0, 25.0), ROOM, TOP);
    edges.drag(Grab::Right, (40.0, 0.0), ROOM, TOP);
    edges.drag(Grab::Bottom, (0.0, 25.0), ROOM, TOP);

    assert_eq!(corner.width, edges.width);
    assert_eq!(corner.height, edges.height);
}

/// A window shrunk below the popup — or a remembered place from a bigger
/// monitor — leaves it hanging off the edge. It has to come back.
#[test]
fn a_place_from_a_bigger_window_is_brought_back() {
    let mut place = PickerPlace::default();
    place.drag(Grab::Move, (900.0, 600.0), ROOM, TOP);

    place.settle((700.0, 500.0), TOP);
    let (x, y) = place.place((700.0, 500.0), TOP);

    assert!(x + place.width <= 700.0, "the popup fits across");
    assert!(y + place.height <= 500.0, "and down");
}

/// A file written by hand, or by an older version, must not put the popup
/// somewhere it cannot be reached.
#[test]
fn a_nonsense_size_is_replaced_by_the_default() {
    let mut place = PickerPlace {
        width: f32::NAN,
        height: -40.0,
        ..PickerPlace::default()
    };

    place.settle(ROOM, TOP);

    assert_eq!(place.width, PickerPlace::default().width);
    assert_eq!(place.height, PickerPlace::default().height);
}
