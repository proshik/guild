//! A released tab settling into its final layout slot, without moving its neighbours.

use crate::theme;
use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId, LayoutId,
    Pixels, Point, Window,
};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(crate) struct Landing {
    pub from: Point<Pixels>,
    started: Instant,
}

impl Landing {
    pub fn new(from: Point<Pixels>) -> Self {
        Self {
            from,
            started: Instant::now(),
        }
    }

    fn offset(self, target: Point<Pixels>, elapsed: Duration) -> Point<Pixels> {
        let remaining =
            1.0 - (elapsed.as_secs_f32() / theme::motion::TAB_SETTLE.as_secs_f32()).min(1.0);
        (self.from - target) * remaining.powi(3)
    }

    fn tint(elapsed: Duration) -> f32 {
        let remaining =
            1.0 - (elapsed.as_secs_f32() / theme::motion::TAB_FEEDBACK.as_secs_f32()).min(1.0);
        theme::motion::TAB_TINT * remaining
    }
}

/// The child still owns the final slot and width. Only its painted position
/// travels from the preview to that slot, measured in the current layout.
/// Expiry requests no further frames and leaves no extra hitbox or overlay.
pub(super) struct SettlingTab {
    child: AnyElement,
    landing: Option<Landing>,
    elapsed: Duration,
}

pub(super) fn animate(
    tab: gpui::Stateful<gpui::Div>,
    landing: Option<Landing>,
    theme: theme::Theme,
) -> SettlingTab {
    let elapsed = landing.map_or(Duration::ZERO, |landing| landing.started.elapsed());
    let tab = if landing.is_some() {
        let tint: gpui::Hsla = gpui::rgb(theme.accent.patina).into();
        tab.bg(tint.opacity(Landing::tint(elapsed)))
    } else {
        tab
    };
    SettlingTab {
        child: tab.into_any_element(),
        landing,
        elapsed,
    }
}

impl IntoElement for SettlingTab {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

impl Element for SettlingTab {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        if self.landing.is_some() && self.elapsed < theme::motion::TAB_FEEDBACK {
            window.request_animation_frame();
        }
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let offset = self.landing.map_or(Point::default(), |landing| {
            landing.offset(bounds.origin, self.elapsed)
        });
        window.with_element_offset(offset, |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.paint(window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{point, px};

    #[test]
    fn a_released_tab_starts_at_the_preview_and_finishes_in_its_slot() {
        let from = point(px(430.0), px(8.0));
        let target = point(px(300.0), px(0.0));
        let landing = Landing::new(from);
        assert_eq!(target + landing.offset(target, Duration::ZERO), from);
        let halfway = target + landing.offset(target, Duration::from_millis(90));
        assert!(halfway.x > target.x && halfway.x < from.x);
        assert_eq!(
            target + landing.offset(target, Duration::from_secs(1)),
            target
        );
    }

    #[test]
    fn landing_feedback_outlasts_the_motion_then_disappears() {
        assert!(Landing::tint(theme::motion::TAB_SETTLE) > 0.0);
        assert_eq!(Landing::tint(theme::motion::TAB_FEEDBACK), 0.0);
        assert_eq!(Landing::tint(Duration::from_secs(60)), 0.0);
    }

    #[gpui::test]
    fn settling_changes_the_painted_position_but_keeps_neighbours_in_place(
        cx: &mut gpui::TestAppContext,
    ) {
        use std::{cell::Cell, rc::Rc};
        struct Scene {
            elapsed: Duration,
            positions: [Rc<Cell<Point<Pixels>>>; 2],
        }
        impl gpui::Render for Scene {
            fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
                let tab = |index: usize| {
                    let position = self.positions[index].clone();
                    gpui::div()
                        .id(index)
                        .relative()
                        .flex_none()
                        .w(px(100.0))
                        .h(px(36.0))
                        .child(
                            gpui::canvas(
                                move |bounds, _, _| {
                                    position.set(bounds.origin);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        )
                };
                gpui::div()
                    .flex()
                    .w(px(500.0))
                    .h(px(36.0))
                    .child(SettlingTab {
                        child: tab(0).into_any_element(),
                        landing: Some(Landing::new(point(px(250.0), px(8.0)))),
                        elapsed: self.elapsed,
                    })
                    .child(tab(1))
            }
        }
        let positions = [Rc::new(Cell::default()), Rc::new(Cell::default())];
        let (scene, cx) = cx.add_window_view(|_, _| Scene {
            elapsed: Duration::ZERO,
            positions: positions.clone(),
        });
        assert_eq!(positions[0].get(), point(px(250.0), px(8.0)));
        assert_eq!(positions[1].get(), point(px(100.0), px(0.0)));
        cx.update(|_, cx| {
            scene.update(cx, |scene, cx| {
                scene.elapsed = theme::motion::TAB_FEEDBACK;
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert_eq!(positions[0].get(), point(px(0.0), px(0.0)));
        assert_eq!(positions[1].get(), point(px(100.0), px(0.0)));
    }
}
