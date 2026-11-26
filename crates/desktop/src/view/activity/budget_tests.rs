//! Continuous decorative motion must not inherit the display's refresh rate.
use super::*;
use gpui::{AppContext, Context, Render, TestAppContext, Window};
use std::{cell::Cell, rc::Rc, time::Duration};

struct Preview {
    renders: Rc<Cell<usize>>,
    working: bool,
}
impl Render for Preview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.renders.set(self.renders.get() + 1);
        div().flex().children((0..12).map(|_| {
            mark(
                if self.working {
                    Status::Working
                } else {
                    Status::Idle
                },
                theme::Theme::dark(),
                true,
                false,
            )
        }))
    }
}

#[gpui::test]
fn activity_motion_has_a_window_budget_and_stops_without_work(cx: &mut TestAppContext) {
    let renders = Rc::new(Cell::new(0));
    let mut preview = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let view = cx.new(|_| Preview {
            renders: renders.clone(),
            working: true,
        });
        preview = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let before = renders.get();
    // A 120 Hz display must not turn twelve small rings into 120 window rebuilds/s.
    for _ in 0..120 {
        cx.executor().advance_clock(Duration::from_micros(8333));
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    let draws = renders.get() - before;
    assert!(
        (10..=22).contains(&draws),
        "expected bounded continuing motion, got {draws} rebuilds/s"
    );
    assert_eq!(
        cx.update(|window, cx| window.simulate_next_frame(cx)),
        0,
        "decorative motion must not sustain display-rate callbacks"
    );

    preview.unwrap().update(cx, |view, cx| {
        view.working = false;
        cx.notify();
    });
    cx.run_until_parked();
    // Allow the single outstanding delayed request to settle.
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    let stopped = renders.get();
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert_eq!(
        renders.get(),
        stopped,
        "no repeating clock after work stops"
    );
}

pub(crate) fn redraws_in_tick(
    cx: &mut gpui::VisualTestContext,
    renders: &Rc<Cell<usize>>,
) -> usize {
    let before = renders.get();
    cx.executor().advance_clock(Duration::from_millis(60));
    cx.run_until_parked();
    renders.get() - before
}

#[gpui::test]
fn activity_reuses_event_driven_paints_instead_of_adding_frames(cx: &mut TestAppContext) {
    let renders = Rc::new(Cell::new(0));
    let mut preview = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let view = cx.new(|_| Preview {
            renders: renders.clone(),
            working: true,
        });
        preview = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let preview = preview.unwrap();
    let before = renders.get();
    for _ in 0..100 {
        preview.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
    }
    assert_eq!(
        renders.get() - before,
        100,
        "recent real updates already painted the marks; decorative ticks must add no redraws"
    );
}

#[gpui::test]
fn activity_stops_when_occluded_and_resumes_when_visible(cx: &mut TestAppContext) {
    let renders = Rc::new(Cell::new(0));
    let mut preview = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let view = cx.new(|_| Preview {
            renders: renders.clone(),
            working: true,
        });
        preview = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_visibility_change(gpui::WindowVisibility::Hidden);
    redraws_in_tick(cx, &renders);
    assert_eq!(
        redraws_in_tick(cx, &renders),
        0,
        "occluded work must not keep notifying"
    );
    cx.simulate_visibility_change(gpui::WindowVisibility::Visible);
    preview.unwrap().update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!((1..=2).contains(&redraws_in_tick(cx, &renders)));
    // Closing with a pending tick must leave no task accessing a released window.
    cx.update(|window, _| window.remove_window());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
}

struct Siblings {
    fast: gpui::Entity<Preview>,
    quiet: gpui::Entity<Preview>,
}
impl Render for Siblings {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .children([self.fast.clone(), self.quiet.clone()].map(|view| {
                view.cached(
                    gpui::StyleRefinement::default()
                        .w(gpui::px(200.))
                        .h(gpui::px(30.)),
                )
                .into_any_element()
            }))
    }
}

#[gpui::test]
fn activity_in_a_cached_sibling_keeps_moving_during_other_updates(cx: &mut TestAppContext) {
    let fast_renders = Rc::new(Cell::new(0));
    let quiet_renders = Rc::new(Cell::new(0));
    let mut fast = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let a = cx.new(|_| Preview {
            renders: fast_renders.clone(),
            working: true,
        });
        let b = cx.new(|_| Preview {
            renders: quiet_renders.clone(),
            working: true,
        });
        fast = Some(a.clone());
        let host = cx.new(|_| Siblings { fast: a, quiet: b });
        gpui_component::Root::new(host, window, cx)
    });
    cx.run_until_parked();
    let a = fast_renders.get();
    let b = quiet_renders.get();
    let fast = fast.unwrap();
    for _ in 0..100 {
        fast.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        cx.run_until_parked();
    }
    assert_eq!(
        fast_renders.get() - a,
        100,
        "real updates must stay immediate"
    );
    assert!(
        (10..=22).contains(&(quiet_renders.get() - b)),
        "another owner's frequent paints must not freeze a cached ring"
    );
}
