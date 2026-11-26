//! Where an area's card stands inside its frame — M12.8, decision 18: the
//! cards are 6 apart on the window's colour, and flush with the room at
//! its own edges, where the gap to the next thing is the room's.

use guild_desktop::view::areas::{Insets, insets};
use guild_session::layout::{Axis, Floor, Frame, Tree};

const GAP: f32 = 6.0;
const FLOOR: Floor = Floor {
    width: 320.0,
    height: 160.0,
};

fn room() -> Frame {
    Frame::new(0.0, 0.0, 1200.0, 800.0)
}

/// The card's own rectangle: its frame less its insets.
fn card(frame: Frame, inset: Insets) -> Frame {
    Frame::new(
        frame.x + inset.left,
        frame.y + inset.top,
        frame.width - inset.left - inset.right,
        frame.height - inset.top - inset.bottom,
    )
}

/// One area is the whole room: no edge is shared, so nothing is taken.
#[test]
fn a_lone_area_fills_the_room() {
    let inset = insets(room(), room(), GAP);
    assert_eq!(
        inset,
        Insets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0
        }
    );
}

/// Two side by side are one gap apart — half of it out of each — and
/// flush with the room everywhere else.
#[test]
fn two_areas_side_by_side_stand_one_gap_apart() {
    let mut tree = Tree::leaf(1);
    assert!(tree.split_leaf(&1, Axis::Horizontal, 2));
    let frames = tree.frames(room(), FLOOR);
    let [(_, left), (_, right)] = frames[..] else {
        panic!("two frames: {frames:?}")
    };
    let (one, two) = (insets(left, room(), GAP), insets(right, room(), GAP));
    assert_eq!((one.left, one.top, one.bottom), (0.0, 0.0, 0.0));
    assert_eq!(one.right, GAP / 2.0);
    assert_eq!((two.right, two.top, two.bottom), (0.0, 0.0, 0.0));
    assert_eq!(two.left, GAP / 2.0);
    let (a, b) = (card(left, one), card(right, two));
    assert_eq!(b.x - (a.x + a.width), GAP);
}

/// A nested division shares edges both ways: the lower right area of
/// `1 | (2 / 3)` gives half a gap to its left and to its top, and none to
/// the room's right and bottom.
#[test]
fn a_nested_area_gives_half_a_gap_on_each_inner_edge() {
    let mut tree = Tree::leaf(1);
    assert!(tree.split_leaf(&1, Axis::Horizontal, 2));
    assert!(tree.split_leaf(&2, Axis::Vertical, 3));
    let frames = tree.frames(room(), FLOOR);
    let upper = frames.iter().find(|(id, _)| *id == 2).expect("2").1;
    let lower = frames.iter().find(|(id, _)| *id == 3).expect("3").1;
    let inset = insets(lower, room(), GAP);
    assert_eq!(
        inset,
        Insets {
            top: GAP / 2.0,
            right: 0.0,
            bottom: 0.0,
            left: GAP / 2.0
        }
    );
    let (a, b) = (card(upper, insets(upper, room(), GAP)), card(lower, inset));
    assert_eq!(
        b.y - (a.y + a.height),
        GAP,
        "the two stacked are one gap apart too"
    );
}

/// The room is measured in the window's own coordinates, so an edge is
/// the room's by where the room is, not by where zero is.
#[test]
fn the_room_s_edges_are_where_the_room_is() {
    let room = Frame::new(240.0, 40.0, 900.0, 700.0);
    assert_eq!(
        insets(room, room, GAP),
        Insets {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0
        }
    );
}
