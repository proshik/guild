//! Where a dragged tool lands: the zones' geometry, shared by the view
//! and this test so that they cannot disagree — and, since M12.8, which
//! tab a dragged tab is answered by.

use gpui::{Pixels, Point, Size, px};
use guild_desktop::view::drag::drop_target;
use guild_session::Dock;

fn at(x: f32, y: f32) -> Point<Pixels> {
    Point::new(px(x), px(y))
}

/// A point in the left 52 drops into the left dock, in the right 52 into
/// the right one, in the heading's height above the status bar into the
/// bottom; the status bar itself and the middle of the window are nowhere.
#[test]
fn the_edges_are_the_zones_and_the_middle_is_nowhere() {
    let window = Size {
        width: px(1512.0),
        height: px(949.0),
    };
    assert_eq!(drop_target(at(20.0, 400.0), window), Some(Dock::Left));
    assert_eq!(drop_target(at(1500.0, 400.0), window), Some(Dock::Right));
    assert_eq!(
        drop_target(at(700.0, 949.0 - 26.0 - 20.0), window),
        Some(Dock::Bottom)
    );
    assert_eq!(
        drop_target(at(700.0, 949.0 - 10.0), window),
        None,
        "the status bar itself is not a zone"
    );
    assert_eq!(drop_target(at(700.0, 400.0), window), None);
    assert_eq!(
        drop_target(at(20.0, 949.0 - 26.0 - 20.0), window),
        Some(Dock::Left),
        "a corner belongs to the side"
    );
}

/// **A dragged tab is answered only by the tab the hand can see** —
/// M12.8. A tab's bounds are its own, not clipped by the strip that
/// scrolls it: a strip's tabs scrolled out past its right end keep
/// bounds that lie over the next area's strip, and a drag there moved
/// the tab back and forth between the two areas on every move. GPUI's
/// own hit test clips a hitbox by its content mask; this does the same
/// with the strip's visible box.
#[test]
fn a_tab_scrolled_out_of_its_strip_does_not_answer_a_drag() {
    use gpui::Bounds;
    use guild_desktop::view::tabs::answers_drag;

    let bounds = |x: f32, width: f32| Bounds::new(at(x, 40.0), Size::new(px(width), px(36.0)));
    // The left area's strip shows 0–500; its fifth tab is scrolled out to
    // 520–640, under the right area's strip, which starts at 506.
    let left_strip = bounds(0.0, 500.0);
    let hidden_tab = bounds(520.0, 120.0);
    let seen_tab = bounds(380.0, 120.0);
    let over_the_right_strip = at(560.0, 58.0);

    assert!(!answers_drag(hidden_tab, left_strip, over_the_right_strip));
    assert!(answers_drag(seen_tab, left_strip, at(420.0, 58.0)));
    assert!(
        !answers_drag(seen_tab, left_strip, at(510.0, 58.0)),
        "the part of a tab past the strip's end is out of sight too"
    );
    assert!(!answers_drag(seen_tab, left_strip, at(420.0, 90.0)));
}
