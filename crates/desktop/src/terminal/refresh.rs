//! Output demand only. Grid -> demand is the lock order; never acquire a grid here.
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Notify;
pub(crate) const INTERVAL: Duration = Duration::from_nanos(8_333_334);
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Capture {
    pub epoch: u64,
    pub generation: u64,
}
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    Idle,
    Wait(Duration),
    Notify,
}
#[derive(Default)]
pub(crate) struct Demand {
    state: Mutex<State>,
    pub signal: Arc<Notify>,
}
#[derive(Default)]
struct State {
    latest: u64,
    ack: u64,
    epoch: u64,
    shown: bool,
    adaptive: bool,
    queued: bool,
    awaiting: bool,
    closed: bool,
    last: Option<Instant>,
}
impl Demand {
    #[cfg(feature = "native-preview")]
    pub(crate) fn probe_state(&self) -> serde_json::Value {
        let s = self.state.lock().unwrap();
        serde_json::json!({"latest":s.latest,"ack":s.ack,"epoch":s.epoch,"shown":s.shown,"adaptive":s.adaptive,"queued":s.queued,"awaiting_paint":s.awaiting,"closed":s.closed})
    }

    pub fn active(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.shown && s.adaptive && !s.closed
    }
    pub fn advance(&self) {
        self.state.lock().unwrap().latest += 1;
    }
    pub fn capture(&self) -> Capture {
        let s = self.state.lock().unwrap();
        Capture {
            epoch: s.epoch,
            generation: s.latest,
        }
    }
    pub fn publish(&self) {
        let wake = {
            let mut s = self.state.lock().unwrap();
            claim(&mut s)
        };
        if wake {
            self.signal.notify_one();
        }
    }
    pub fn policy(&self, shown: bool, adaptive: bool) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.closed || (s.shown == shown && s.adaptive == adaptive) {
            return false;
        }
        s.epoch += 1;
        s.shown = shown;
        s.adaptive = adaptive;
        s.queued = false;
        s.awaiting = false;
        let current = shown && !adaptive && s.latest > s.ack;
        claim(&mut s);
        drop(s);
        // Also cancel a stale deadline. This is an event, never an idle clock.
        self.signal.notify_one();
        current
    }
    pub fn action(&self, now: Instant) -> Action {
        let mut s = self.state.lock().unwrap();
        if s.closed || !s.shown || !s.adaptive || !s.queued || s.awaiting {
            return Action::Idle;
        }
        if let Some(last) = s.last {
            let due = last + INTERVAL;
            if now < due {
                return Action::Wait(due - now);
            }
        }
        s.queued = false;
        s.awaiting = true;
        s.last = Some(now);
        Action::Notify
    }
    pub fn acknowledge(&self, capture: Capture) -> bool {
        let mut s = self.state.lock().unwrap();
        if s.closed || capture.epoch != s.epoch {
            return false;
        }
        let cancel_pending = s.queued;
        s.ack = s.ack.max(capture.generation);
        s.queued = false;
        s.awaiting = false;
        claim(&mut s) || cancel_pending
    }
    pub fn painted(self: Arc<Self>, capture: Capture, cx: &mut gpui::App) {
        if self.acknowledge(capture) {
            cx.defer(move |_| self.signal.notify_one());
        }
    }
    pub fn close(&self) {
        let mut s = self.state.lock().unwrap();
        s.closed = true;
        s.epoch += 1;
        s.queued = false;
        s.awaiting = false;
        drop(s);
        self.signal.notify_one();
    }
}
fn claim(s: &mut State) -> bool {
    if !s.closed && s.shown && s.adaptive && s.latest > s.ack && !s.queued && !s.awaiting {
        s.queued = true;
        true
    } else {
        false
    }
}
pub(crate) fn eligible(
    shown: bool,
    native: Option<bool>,
    active: bool,
    display: Option<u32>,
    normal_power: Option<bool>,
    thermal: gpui::ThermalState,
) -> bool {
    shown
        && native == Some(true)
        && active
        && display.is_some_and(|maximum_fps| maximum_fps > 60)
        && normal_power == Some(true)
        && matches!(
            thermal,
            gpui::ThermalState::Nominal | gpui::ThermalState::Fair
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn policy_only_allows_known_visible_active_normal_power_and_safe_thermal() {
        use gpui::ThermalState::*;
        assert!(eligible(
            true,
            Some(true),
            true,
            Some(120),
            Some(true),
            Nominal
        ));
        assert!(eligible(
            true,
            Some(true),
            true,
            Some(120),
            Some(true),
            Fair
        ));
        for power in [None, Some(false)] {
            assert!(!eligible(true, Some(true), true, Some(120), power, Nominal));
        }
        for native in [None, Some(false)] {
            assert!(!eligible(
                true,
                native,
                true,
                Some(120),
                Some(true),
                Nominal
            ));
        }
        for thermal in [Serious, Critical] {
            assert!(!eligible(
                true,
                Some(true),
                true,
                Some(120),
                Some(true),
                thermal
            ));
        }
        assert!(!eligible(
            false,
            Some(true),
            true,
            Some(120),
            Some(true),
            Nominal
        ));
        assert!(!eligible(
            true,
            Some(true),
            false,
            Some(120),
            Some(true),
            Nominal
        ));
        assert!(!eligible(true, Some(true), true, None, Some(true), Nominal));
    }
    #[test]
    fn sixty_hz_and_unknown_use_current_and_migration_preserves_dirty_generation() {
        use gpui::ThermalState::Nominal;
        for fps in [None, Some(0), Some(30), Some(60)] {
            assert!(!eligible(true, Some(true), true, fps, Some(true), Nominal));
        }
        for fps in [Some(120), Some(144)] {
            assert!(eligible(true, Some(true), true, fps, Some(true), Nominal));
        }
        let d = Demand::default();
        let now = Instant::now();
        d.policy(true, true);
        d.advance();
        d.publish();
        assert_eq!(d.action(now), Action::Notify);
        d.acknowledge(d.capture());
        d.advance();
        d.publish();
        assert!(matches!(d.action(now), Action::Wait(_)));
        let old = d.capture();
        for (index, fps) in [Some(60), None].into_iter().enumerate() {
            let adaptive = eligible(true, Some(true), true, fps, Some(true), Nominal);
            assert!(d.policy(true, adaptive));
            assert!(!d.active());
            assert_eq!(d.action(now + INTERVAL), Action::Idle);
            assert!(!d.acknowledge(old));
            d.policy(true, true);
            assert_eq!(d.capture().generation, old.generation);
            assert_eq!(
                d.action(now + INTERVAL * (index as u32 + 1)),
                Action::Notify
            );
        }
    }
    #[test]
    fn flood_has_one_outstanding_invalidation_until_captured_paint() {
        let d = Demand::default();
        d.policy(true, true);
        let now = Instant::now();
        for _ in 0..10000 {
            d.advance();
            d.publish();
        }
        assert_eq!(d.action(now), Action::Notify);
        assert_eq!(d.action(now + Duration::from_secs(1)), Action::Idle);
        let captured = d.capture();
        d.advance();
        d.publish();
        assert_eq!(d.action(now + Duration::from_secs(1)), Action::Idle);
        assert!(d.acknowledge(captured));
        assert_eq!(d.action(now + INTERVAL), Action::Notify);
        assert!(!d.acknowledge(d.capture()));
        assert_eq!(d.action(now + Duration::from_secs(5)), Action::Idle);
    }
    #[test]
    fn hidden_and_current_fence_old_paints_and_reveal_latest_without_output() {
        let d = Demand::default();
        let now = Instant::now();
        d.policy(true, true);
        d.advance();
        d.publish();
        assert_eq!(d.action(now), Action::Notify);
        let old = d.capture();
        d.policy(false, false);
        d.advance();
        d.publish();
        assert_eq!(d.action(now + INTERVAL), Action::Idle);
        d.policy(true, true);
        assert!(!d.acknowledge(old));
        assert_eq!(d.action(now + INTERVAL), Action::Notify);
        assert!(d.policy(true, false));
        assert!(!d.active());
        assert_eq!(d.action(now + Duration::from_secs(1)), Action::Idle);
        d.close();
        d.policy(true, true);
        d.advance();
        d.publish();
        assert_eq!(d.action(now + Duration::from_secs(2)), Action::Idle);
    }
    #[test]
    fn an_external_paint_cancels_a_pending_cap_deadline() {
        let d = Demand::default();
        let now = Instant::now();
        d.policy(true, true);
        d.advance();
        d.publish();
        assert_eq!(d.action(now), Action::Notify);
        d.acknowledge(d.capture());
        d.advance();
        d.publish();
        assert!(matches!(d.action(now), Action::Wait(_)));
        assert!(
            d.acknowledge(d.capture()),
            "wake the waiting consumer to cancel its deadline"
        );
        assert_eq!(d.action(now), Action::Idle);
        assert!(
            !d.acknowledge(d.capture()),
            "clean paints never form a wake chain"
        );
    }
    #[test]
    fn cap_is_pending_only_early_and_late_wakes_never_catch_up() {
        let d = Demand::default();
        let now = Instant::now();
        d.policy(true, true);
        d.advance();
        d.publish();
        assert_eq!(d.action(now), Action::Notify);
        assert!(!d.acknowledge(d.capture()));
        d.advance();
        d.publish();
        assert_eq!(
            d.action(now + Duration::from_millis(1)),
            Action::Wait(INTERVAL - Duration::from_millis(1))
        );
        assert_eq!(d.action(now + Duration::from_secs(2)), Action::Notify);
        assert_eq!(d.action(now + Duration::from_secs(2)), Action::Idle);
        assert!(!d.acknowledge(d.capture()));
        assert_eq!(d.action(now + Duration::from_secs(3)), Action::Idle);
    }
    #[test]
    fn publication_after_capture_and_clear_cannot_lose_a_wake() {
        let d = Arc::new(Demand::default());
        d.policy(true, true);
        let now = Instant::now();
        for round in 1..=1000 {
            let captured = d.capture();
            let reader = d.clone();
            let t = std::thread::spawn(move || {
                reader.advance();
                reader.publish();
            });
            d.acknowledge(captured);
            t.join().unwrap();
            assert_eq!(d.action(now + Duration::from_secs(round)), Action::Notify);
            d.acknowledge(d.capture());
        }
    }
}

#[cfg(test)]
mod draw_tests {
    use super::*;
    use gpui::{IntoElement, Render, TestAppContext, Window, canvas, prelude::*, px, size};
    use std::{
        cell::RefCell,
        future::Future,
        rc::Rc,
        task::{Context as PollContext, Poll, Waker},
    };
    fn ready(signal: &Notify) -> bool {
        let notified = signal.notified();
        let mut notified = std::pin::pin!(notified);
        matches!(
            notified
                .as_mut()
                .poll(&mut PollContext::from_waker(Waker::noop())),
            Poll::Ready(())
        )
    }
    struct PaintRace {
        demand: Arc<Demand>,
        order: Rc<RefCell<Vec<&'static str>>>,
    }
    impl Render for PaintRace {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            let demand = self.demand.clone();
            let order = self.order.clone();
            canvas(
                |_, _, _| (),
                move |_, _, _, cx| {
                    if !order.borrow().is_empty() {
                        return;
                    }
                    order.borrow_mut().push("paint");
                    let captured = demand.capture();
                    // Reader publication while the captured grid is being shaped.
                    demand.advance();
                    demand.publish();
                    demand.clone().painted(captured, cx);
                    assert!(!ready(&demand.signal), "successor published inside paint");
                    order.borrow_mut().push("paint-ended");
                    cx.defer(move |_| {
                        assert!(ready(&demand.signal));
                        assert_eq!(&*order.borrow(), &["paint", "paint-ended"]);
                        order.borrow_mut().push("deferred");
                    });
                },
            )
            .size_full()
        }
    }
    #[gpui::test]
    fn public_app_defer_rearms_after_actual_canvas_paint(cx: &mut TestAppContext) {
        let demand = Arc::new(Demand::default());
        demand.policy(true, true);
        demand.advance();
        demand.publish();
        assert_eq!(demand.action(Instant::now()), Action::Notify);
        assert!(ready(&demand.signal));
        let order = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(|_, _| PaintRace {
            demand: demand.clone(),
            order: order.clone(),
        });
        cx.simulate_resize(size(px(100.), px(100.)));
        cx.run_until_parked();
        assert_eq!(&*order.borrow(), &["paint", "paint-ended", "deferred"]);
        // No callback or timer was installed: external paints cannot loop it.
        for _ in 0..5 {
            cx.update(|window, cx| {
                window.refresh();
                window.simulate_next_frame(cx);
            });
            cx.run_until_parked();
        }
        assert_eq!(order.borrow().len(), 3);
    }
}

#[cfg(test)]
mod transition_tests {
    use super::*;
    #[test]
    fn fallback_fences_queued_deadline_and_awaitpaint_without_losing_dirt() {
        for phase in 0..3 {
            for shown in [false, true] {
                let d = Demand::default();
                let now = Instant::now();
                d.policy(true, true);
                d.advance();
                d.publish();
                if phase > 0 {
                    assert_eq!(d.action(now), Action::Notify);
                }
                if phase == 2 {
                    d.acknowledge(d.capture());
                    d.advance();
                    d.publish();
                    assert!(matches!(d.action(now), Action::Wait(_)));
                }
                let old = d.capture();
                assert_eq!(d.policy(shown, false), shown);
                assert_eq!(d.action(now + INTERVAL), Action::Idle);
                assert!(!d.acknowledge(old));
                assert!(
                    !d.policy(shown, false),
                    "one Current handoff per transition"
                );
                d.policy(true, true);
                assert_eq!(d.action(now + INTERVAL), Action::Notify);
                d.close();
                assert!(!d.acknowledge(d.capture()));
                assert_eq!(d.action(now + INTERVAL), Action::Idle);
            }
        }
    }
    #[test]
    fn display_cadence_and_jitter_do_not_turn_output_cap_into_a_catchup_clock() {
        for periods in [
            vec![16_666_667],
            vec![8_333_334],
            vec![6_944_444],
            vec![16_683_350],
            vec![7_000_000, 11_000_000, 22_000_000, 8_000_000],
        ] {
            let d = Demand::default();
            d.policy(true, true);
            let mut now = Instant::now();
            let mut notified = None;
            for i in 0..100 {
                d.advance();
                d.publish();
                match d.action(now) {
                    Action::Notify => {
                        if let Some(last) = notified {
                            assert!(now.duration_since(last) >= INTERVAL);
                        }
                        notified = Some(now);
                        d.acknowledge(d.capture());
                    }
                    Action::Wait(delay) => assert!(delay <= INTERVAL),
                    Action::Idle => panic!("dirty active output was lost"),
                }
                now += Duration::from_nanos(periods[i % periods.len()]);
            }
            // External immediate paint clears any last pending output.
            d.acknowledge(d.capture());
            assert_eq!(d.action(now + Duration::from_secs(10)), Action::Idle);
        }
    }
}
