use guild_desktop::observer::*;
fn rect() -> Geometry {
    Geometry {
        x: 100.,
        y: 80.,
        width: 620.,
        height: 360.,
    }
}
#[test]
fn shrinking_viewport_keeps_the_whole_panel_reachable() {
    let g = rect().clamped([280., 180.]);
    assert!(g.x >= 0. && g.y >= 0.);
    assert!(g.x + g.width <= 280. && g.y + g.height <= 180.);
    let tiny = rect().clamped([0., 0.]);
    assert_eq!(tiny.width, 0.);
    assert_eq!(tiny.height, 0.);
}
#[test]
fn modes_remember_separately_and_missing_release_ends_the_gesture() {
    let mut p = Presentation::new(
        rect(),
        Geometry {
            width: 900.,
            height: 600.,
            ..rect()
        },
        [[440., 300.], [440., 460.]],
    );
    p.gesture = Some(Gesture::Move {
        pointer: [0., 0.],
        start: rect(),
    });
    p.motion([30., 20.], true, [1600., 1000.]);
    assert_eq!(p.compact.x, 130.);
    let compact = p.compact;
    p.motion([100., 100.], false, [1600., 1000.]);
    p.motion([200., 200.], true, [1600., 1000.]);
    assert_eq!(p.compact, compact);
    p.mode = Mode::Expanded;
    assert_eq!(p.geometry([1600., 1000.]).width, 900.);
    p.gesture = Some(Gesture::Resize {
        edge: ResizeEdge::TopLeft,
        pointer: [0., 0.],
        start: p.expanded,
    });
    p.motion([2000., 2000.], true, [1600., 1000.]);
    assert_eq!(
        p.expanded,
        Geometry {
            x: 560.,
            y: 220.,
            width: 440.,
            height: 460.
        }
    );
    p.motion([2000., 2000.], false, [1600., 1000.]);
    p.mode = Mode::Compact;
    assert_eq!(p.geometry([1600., 1000.]), compact);
    assert_eq!(
        Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]).compact,
        rect()
    );
}

#[test]
fn resizing_cannot_hide_controls_but_small_viewports_still_fit() {
    let mut p = Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]);
    p.gesture = Some(Gesture::Resize {
        edge: ResizeEdge::BottomRight,
        pointer: [0., 0.],
        start: rect(),
    });
    p.motion([-1000., -1000.], true, [1200., 800.]);
    assert!(p.compact.width >= 440. && p.compact.height >= 300.);
    let g = p.geometry([240., 160.]);
    assert_eq!([g.width, g.height], [240., 160.]);
}

#[test]
fn terminal_default_and_explicit_geometry_preserve_moves_and_fit_small_panes() {
    let mut p = Presentation::new(
        rect(),
        Geometry {
            width: 1000.,
            height: 640.,
            ..rect()
        },
        [[440., 300.], [440., 460.]],
    );
    p.terminal_geometry(None, [1200., 800.]);
    assert_eq!(p.mode, Mode::Compact);
    assert_eq!(p.geometry([1200., 800.]), rect());
    p.terminal_geometry(
        Some(&guild_session::terminal_job::Geometry {
            width_percent: 70,
            height_percent: 65,
        }),
        [1200., 800.],
    );
    let requested = p.geometry([1200., 800.]);
    assert_eq!([requested.width, requested.height], [840., 520.]);
    p.gesture = Some(Gesture::Move {
        pointer: [0., 0.],
        start: requested,
    });
    p.motion([20., 10.], true, [1200., 800.]);
    p.motion([20., 10.], false, [1200., 800.]);
    let moved = p.geometry([1200., 800.]);
    assert_eq!([moved.x, moved.y], [requested.x + 20., requested.y + 10.]);
    assert_eq!(
        [
            p.geometry([200., 120.]).width,
            p.geometry([200., 120.]).height
        ],
        [200., 120.]
    );
    assert_eq!(p.geometry([1200., 800.]), moved);
}

#[test]
fn resizing_stops_at_the_pane_edge_without_moving_the_opposite_corner() {
    let mut p = Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]);
    p.gesture = Some(Gesture::Resize {
        edge: ResizeEdge::BottomRight,
        pointer: [0., 0.],
        start: rect(),
    });
    p.motion([900., 900.], true, [1000., 600.]);
    assert_eq!(
        p.compact,
        Geometry {
            x: 100.,
            y: 80.,
            width: 900.,
            height: 520.
        }
    );
    p.motion([900., 900.], false, [1000., 600.]);
    p.motion([0., 0.], true, [1000., 600.]);
    assert_eq!(
        p.compact,
        Geometry {
            x: 100.,
            y: 80.,
            width: 900.,
            height: 520.
        }
    );
}

#[test]
fn an_edge_minimum_does_not_enlarge_the_other_requested_dimension() {
    let start = Geometry {
        width: 300.,
        height: 180.,
        ..rect()
    };
    for (edge, expected) in [
        (ResizeEdge::Right, [440., 180.]),
        (ResizeEdge::Bottom, [300., 300.]),
    ] {
        let mut p = Presentation::new(start, start, [[440., 300.], [440., 460.]]);
        p.gesture = Some(Gesture::Resize {
            edge,
            pointer: [0., 0.],
            start,
        });
        p.motion([-1000., -1000.], true, [1200., 800.]);
        assert_eq!([p.compact.width, p.compact.height], expected, "{edge:?}");
        p.motion([-1000., -1000.], true, [240., 160.]);
        assert_eq!(
            p.geometry([240., 160.]),
            Geometry {
                x: 0.,
                y: 0.,
                width: 240.,
                height: 160.
            }
        );
    }
}

#[test]
fn every_frame_edge_keeps_the_opposite_edges_anchored() {
    for (edge, expected) in [
        (ResizeEdge::Left, [60., 80., 660., 360.]),
        (ResizeEdge::Right, [100., 80., 580., 360.]),
        (ResizeEdge::Top, [100., 60., 620., 380.]),
        (ResizeEdge::Bottom, [100., 80., 620., 340.]),
        (ResizeEdge::TopLeft, [60., 60., 660., 380.]),
        (ResizeEdge::TopRight, [100., 60., 580., 380.]),
        (ResizeEdge::BottomLeft, [60., 80., 660., 340.]),
        (ResizeEdge::BottomRight, [100., 80., 580., 340.]),
    ] {
        let mut p = Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]);
        p.gesture = Some(Gesture::Resize {
            edge,
            pointer: [0., 0.],
            start: rect(),
        });
        p.motion([-40., -20.], true, [1200., 800.]);
        let g = p.compact;
        assert_eq!([g.x, g.y, g.width, g.height], expected, "{edge:?}");
        p.motion([400., 500.], false, [1200., 800.]);
        p.motion([800., 700.], true, [1200., 800.]);
        assert_eq!(p.compact, g, "release must stop {edge:?}");
        p.gesture = Some(Gesture::Resize {
            edge,
            pointer: [0., 0.],
            start: g,
        });
        p.motion([-800., -800.], true, [240., 160.]);
        assert_eq!(
            p.geometry([240., 160.]),
            Geometry {
                x: 0.,
                y: 0.,
                width: 240.,
                height: 160.
            },
            "shrinking pane during {edge:?}"
        );
    }
}

#[test]
fn near_edges_stop_at_minima_and_pane_limits_without_shifting_the_far_edges() {
    for (edge, pointer, expected) in [
        (ResizeEdge::Left, [2000., 2000.], [280., 80., 440., 360.]),
        (ResizeEdge::Top, [2000., 2000.], [100., 140., 620., 300.]),
        (
            ResizeEdge::TopLeft,
            [2000., 2000.],
            [280., 140., 440., 300.],
        ),
        (
            ResizeEdge::TopRight,
            [-2000., 2000.],
            [100., 140., 440., 300.],
        ),
        (
            ResizeEdge::BottomLeft,
            [2000., -2000.],
            [280., 80., 440., 300.],
        ),
        (ResizeEdge::Left, [-2000., -2000.], [0., 80., 720., 360.]),
        (ResizeEdge::Top, [-2000., -2000.], [100., 0., 620., 440.]),
        (ResizeEdge::TopLeft, [-2000., -2000.], [0., 0., 720., 440.]),
        (
            ResizeEdge::TopRight,
            [2000., -2000.],
            [100., 0., 1100., 440.],
        ),
        (
            ResizeEdge::BottomLeft,
            [-2000., 2000.],
            [0., 80., 720., 720.],
        ),
    ] {
        let mut p = Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]);
        p.gesture = Some(Gesture::Resize {
            edge,
            pointer: [0., 0.],
            start: rect(),
        });
        p.motion(pointer, true, [1200., 800.]);
        let g = p.compact;
        assert_eq!([g.x, g.y, g.width, g.height], expected, "{edge:?}");
    }
}

#[test]
fn release_uses_its_final_position_even_when_the_last_drag_motion_was_missed() {
    let mut p = Presentation::new(rect(), rect(), [[440., 300.], [440., 460.]]);
    p.gesture = Some(Gesture::Resize {
        edge: ResizeEdge::TopLeft,
        pointer: [0., 0.],
        start: rect(),
    });
    p.motion([-10., -10.], true, [1200., 800.]);
    p.release([-900., -800.], [1200., 800.]);
    assert_eq!(
        p.compact,
        Geometry {
            x: 0.,
            y: 0.,
            width: 720.,
            height: 440.
        }
    );
    assert!(p.gesture.is_none());
    let released = p.compact;
    p.motion([500., 500.], true, [1200., 800.]);
    assert_eq!(p.compact, released);
    assert_eq!(p.expanded, rect());
}
