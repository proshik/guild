//! The division of a surface — M12.8. One tree for a seat's areas and a
//! tab's panes; everything here is structure and arithmetic.

use guild_session::layout::{
    Axis, Border, Direction, Floor, Frame, Side, Tree, room_to_split, toward,
};

const FLOOR: Floor = Floor {
    width: 80.0,
    height: 80.0,
};

fn window() -> Frame {
    Frame::new(0.0, 0.0, 800.0, 600.0)
}

/// `1 | (2 / 3)`: one on the left, two stacked on the right.
fn three() -> Tree<u8> {
    let mut tree = Tree::leaf(1);
    tree.split_leaf(&1, Axis::Horizontal, 2);
    tree.split_leaf(&2, Axis::Vertical, 3);
    tree
}

#[test]
fn a_leaf_is_the_whole_frame() {
    let tree = Tree::leaf(1u8);
    assert_eq!(tree.frames(window(), FLOOR), vec![(1, window())]);
}

#[test]
fn splitting_right_gives_each_side_half() {
    let mut tree = Tree::leaf(1u8);
    assert!(tree.split_leaf(&1, Axis::Horizontal, 2));
    assert_eq!(
        tree.frames(window(), FLOOR),
        vec![
            (1, Frame::new(0.0, 0.0, 400.0, 600.0)),
            (2, Frame::new(400.0, 0.0, 400.0, 600.0)),
        ]
    );
}

#[test]
fn a_leaf_that_is_not_there_is_not_split() {
    let mut tree = Tree::leaf(1u8);
    assert!(!tree.split_leaf(&9, Axis::Horizontal, 2));
    assert_eq!(tree.leaves(), vec![1]);
}

#[test]
fn leaves_are_in_the_order_they_are_drawn() {
    let mut tree = Tree::leaf(1u8);
    tree.split_leaf(&1, Axis::Horizontal, 2);
    tree.split_leaf(&1, Axis::Vertical, 3);
    assert_eq!(tree.leaves(), vec![1, 3, 2]);
}

#[test]
fn removing_a_leaf_gives_its_room_to_its_sibling() {
    let mut tree = three();
    assert!(tree.remove_leaf(&3));
    assert_eq!(tree.leaves(), vec![1, 2]);
    assert_eq!(
        tree.frames(window(), FLOOR)[1].1,
        Frame::new(400.0, 0.0, 400.0, 600.0)
    );
}

#[test]
fn the_last_leaf_cannot_be_removed() {
    let mut tree = Tree::leaf(1u8);
    assert!(!tree.remove_leaf(&1));
    assert!(
        !tree.remove_leaf(&9),
        "and one that is not there is not removed"
    );
    assert_eq!(tree.leaves(), vec![1]);
}

/// The heir is the leaf that takes the room — the nearest leaf of the
/// sibling: its first when the leaf going was first, its last when it was
/// second. Decision 9's tabs, and a closed pane's focus, go there.
#[test]
fn a_leaf_s_heir_is_the_neighbour_that_takes_its_room() {
    let tree = three();
    assert_eq!(tree.heir(&1), Some(2));
    assert_eq!(tree.heir(&2), Some(3));
    assert_eq!(tree.heir(&3), Some(2));
    assert_eq!(Tree::leaf(1u8).heir(&1), None, "the only leaf has none");
}

#[test]
fn every_border_is_named_by_its_path_from_the_root() {
    assert_eq!(
        three().borders(window(), FLOOR),
        vec![
            Border {
                path: vec![],
                axis: Axis::Horizontal,
                at: 400.0,
                span: window(),
            },
            Border {
                path: vec![Side::Second],
                axis: Axis::Vertical,
                at: 300.0,
                span: Frame::new(400.0, 0.0, 400.0, 600.0),
            },
        ]
    );
}

/// Decision 12: a drag names its border, and no other moves.
#[test]
fn a_drag_moves_the_border_it_names_and_no_other() {
    let mut tree = three();
    assert!(tree.drag(&[Side::Second], -150.0, window(), FLOOR));
    assert_eq!(
        tree.frames(window(), FLOOR),
        vec![
            (1, Frame::new(0.0, 0.0, 400.0, 600.0)),
            (2, Frame::new(400.0, 0.0, 400.0, 150.0)),
            (3, Frame::new(400.0, 150.0, 400.0, 450.0)),
        ]
    );
    assert!(
        !tree.drag(&[Side::First], 10.0, window(), FLOOR),
        "a leaf has no border"
    );
}

/// Window check 3: a border sometimes ignored a drag — seven tries at
/// one place did nothing while a double click there worked. A drag was
/// followed move by move, and each move's few points were rounded away
/// with the split's hundredth (8 on this 800), then forgotten: a slow
/// hand never moved the border at all. **A border held is measured from
/// where it was taken**, so however slowly the hand goes, the border is
/// where the hand is, to the hundredth.
#[test]
fn a_border_held_follows_the_hand_however_slowly_it_moves() {
    let mut tree = three();
    let held = tree.split_at(&[]).expect("the root divides");
    for moved in 1..=40 {
        assert!(tree.drag_from(&[], held, moved as f32, window(), FLOOR));
    }
    assert_eq!(tree.frames(window(), FLOOR)[0].1.width, 440.0);
    // …and back past where it began, a point at a time.
    for moved in (-40..40).rev() {
        tree.drag_from(&[], held, moved as f32, window(), FLOOR);
    }
    assert_eq!(tree.frames(window(), FLOOR)[0].1.width, 360.0);
    assert!(
        tree.split_at(&[Side::First]).is_none(),
        "a leaf has no border"
    );
    assert!(
        !tree.drag_from(&[Side::First], held, 10.0, window(), FLOOR),
        "a leaf has no border to drag"
    );
}

#[test]
fn a_drag_that_comes_back_lands_where_it_began_and_a_reset_is_a_half() {
    let mut tree = three();
    tree.drag(&[], 200.0, window(), FLOOR);
    assert_eq!(tree.frames(window(), FLOOR)[0].1.width, 600.0);
    tree.drag(&[], -200.0, window(), FLOOR);
    assert_eq!(tree.frames(window(), FLOOR)[0].1.width, 400.0);
    tree.drag(&[], 200.0, window(), FLOOR);
    assert!(tree.reset(&[]));
    assert_eq!(tree.frames(window(), FLOOR)[0].1.width, 400.0);
}

#[test]
fn no_side_is_dragged_under_its_floor() {
    let floor = Floor {
        width: 200.0,
        height: 200.0,
    };
    let mut tree = three();
    tree.drag(&[], -900.0, window(), floor);
    assert_eq!(tree.frames(window(), floor)[0].1.width, 200.0);
}

/// A node's floor is its subtree's: a side holding a split on the same axis
/// is never squeezed under two floors.
#[test]
fn a_side_holding_a_split_keeps_the_floors_of_all_it_holds() {
    let floor = Floor {
        width: 200.0,
        height: 200.0,
    };
    // 1 | (2 | 3)
    let mut tree = Tree::leaf(1u8);
    tree.split_leaf(&1, Axis::Horizontal, 2);
    tree.split_leaf(&2, Axis::Horizontal, 3);
    tree.drag(&[], 900.0, window(), floor);
    let places = tree.frames(window(), floor);
    assert_eq!(places[0].1.width, 400.0, "the first side stops at 1 floor");
    assert_eq!(places[1].1.width, 200.0);
    assert_eq!(places[2].1.width, 200.0);
    let mut back = tree.clone();
    back.drag(&[], -900.0, window(), floor);
    assert_eq!(back.frames(window(), floor)[0].1.width, 200.0);
}

/// Decision 17: a split that would make a side under the floor is not
/// offered.
#[test]
fn a_split_needs_room_for_two_floors() {
    let floor = Floor {
        width: 320.0,
        height: 160.0,
    };
    assert!(room_to_split(
        Frame::new(0.0, 0.0, 640.0, 300.0),
        Axis::Horizontal,
        floor
    ));
    assert!(!room_to_split(
        Frame::new(0.0, 0.0, 639.0, 300.0),
        Axis::Horizontal,
        floor
    ));
    assert!(!room_to_split(
        Frame::new(0.0, 0.0, 640.0, 300.0),
        Axis::Vertical,
        floor
    ));
}

/// Decision 14: an arrow reaches the neighbour on its side; a tie goes to
/// the first drawn.
#[test]
fn the_arrows_reach_the_neighbour_on_that_side() {
    let places = three().frames(window(), FLOOR);
    assert_eq!(toward(&places, &1, Direction::Right), Some(2));
    assert_eq!(toward(&places, &3, Direction::Up), Some(2));
    assert_eq!(toward(&places, &2, Direction::Down), Some(3));
    assert_eq!(toward(&places, &3, Direction::Left), Some(1));
}

#[test]
fn an_arrow_at_the_edge_goes_nowhere() {
    let places = three().frames(window(), FLOOR);
    assert_eq!(toward(&places, &1, Direction::Left), None);
    assert_eq!(toward(&places, &2, Direction::Up), None);
    assert_eq!(toward(&places, &9, Direction::Up), None, "nor from nowhere");
}

#[test]
fn the_nearer_of_two_on_one_side_wins() {
    let mut tree = Tree::leaf(1u8);
    tree.split_leaf(&1, Axis::Horizontal, 2);
    tree.split_leaf(&2, Axis::Horizontal, 3);
    assert_eq!(
        toward(&tree.frames(window(), FLOOR), &1, Direction::Right),
        Some(2)
    );
}

#[test]
fn a_tree_survives_a_round_trip() {
    let mut tree = three();
    tree.drag(&[], 200.0, window(), FLOOR);
    let text = serde_json::to_string(&tree).unwrap();
    let back: Tree<u8> = serde_json::from_str(&text).unwrap();
    assert_eq!(back, tree);
}

/// **A place starts after the hairline at its leading border** — M12.8's
/// second window check, O4: `Border::at` is the second side's first
/// point, and its one-point line was drawn there, inside the second
/// pane, so the focused outline stood 2 from the line in the first pane
/// and 1 in the second. What a place draws in is its frame less the
/// line at a border it has on its left or its top; an edge of the room
/// has no line.
#[test]
fn a_place_starts_after_the_line_at_its_leading_border() {
    let room = window();
    let mut tree = Tree::leaf(1);
    tree.split_leaf(&1, Axis::Horizontal, 2);
    tree.split_leaf(&2, Axis::Vertical, 3);
    let frames = tree.frames(room, FLOOR);
    let inside: Vec<Frame> = frames
        .iter()
        .map(|(_, frame)| frame.inside_lines(room, 1.0))
        .collect();
    assert_eq!(
        inside,
        vec![
            // At the room's left and top: nothing to leave out.
            Frame::new(0.0, 0.0, 400.0, 600.0),
            // After the vertical line.
            Frame::new(401.0, 0.0, 399.0, 300.0),
            // After the vertical line and the horizontal one.
            Frame::new(401.0, 301.0, 399.0, 299.0),
        ]
    );
    // Every line is where a place's drawing is not.
    for border in tree.borders(room, FLOOR) {
        for frame in &inside {
            let (start, end) = match border.axis {
                Axis::Horizontal => (frame.x, frame.x + frame.width),
                Axis::Vertical => (frame.y, frame.y + frame.height),
            };
            let crosses = match border.axis {
                Axis::Horizontal => {
                    frame.y < border.span.y + border.span.height
                        && border.span.y < frame.y + frame.height
                }
                Axis::Vertical => {
                    frame.x < border.span.x + border.span.width
                        && border.span.x < frame.x + frame.width
                }
            };
            assert!(
                !crosses || border.at + 1.0 <= start || end <= border.at,
                "{frame:?} is under the line at {}",
                border.at
            );
        }
    }
}
