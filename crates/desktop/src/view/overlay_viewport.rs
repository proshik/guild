//! Measured pane bounds invalidate the presentation, not just its paint.
use gpui::{Context, IntoElement, prelude::*};
use std::{cell::Cell, rc::Rc};

pub(super) fn measure<T: 'static>(
    viewport: Rc<Cell<[f32; 2]>>,
    cx: &Context<T>,
) -> impl IntoElement {
    let entity = cx.entity().downgrade();
    gpui::canvas(
        move |bounds, _, cx| {
            let size = [f32::from(bounds.size.width), f32::from(bounds.size.height)];
            if viewport.replace(size) != size {
                let entity = entity.clone();
                cx.defer(move |cx| {
                    let _ = entity.update(cx, |_, cx| cx.notify());
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observer::Geometry;
    use gpui::{Render, TestAppContext, Window, div, px, size};

    #[derive(Default)]
    struct Frame {
        viewport: Rc<Cell<[f32; 2]>>,
    }
    impl Render for Frame {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let card = Geometry {
                x: 350.,
                y: 0.,
                width: 650.,
                height: 340.,
            }
            .clamped(self.viewport.get());
            div()
                .relative()
                .size_full()
                .child(measure(self.viewport.clone(), cx))
                .child(
                    div()
                        .absolute()
                        .left(px(card.x))
                        .top(px(card.y))
                        .w(px(card.width))
                        .h(px(card.height))
                        .debug_selector(|| "bounded-observer".into()),
                )
        }
    }
    #[gpui::test]
    fn window_resize_keeps_the_entire_observer_inside_the_pane(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| Frame::default());
        cx.simulate_resize(size(px(1000.), px(760.)));
        let wide = cx.debug_bounds("bounded-observer").unwrap();
        assert_eq!(wide.right(), px(1000.));
        cx.simulate_resize(size(px(640.), px(528.)));
        let narrow = cx.debug_bounds("bounded-observer").unwrap();
        assert_eq!(narrow.left(), px(0.));
        assert_eq!(narrow.right(), px(640.));
        assert!(narrow.bottom() <= px(528.));
    }
}
