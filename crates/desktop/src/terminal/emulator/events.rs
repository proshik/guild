//! Coalesced wakeups and ordered terminal events. Only the PTY producer waits
//! for capacity, between parser chunks and outside the parser/grid locks.
use super::TerminalEvent;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
#[cfg(feature = "adaptive-terminal-refresh")]
use std::sync::{
    OnceLock,
    atomic::{AtomicBool, Ordering},
};
use tokio::sync::Notify;

const EVENTS: usize = 256;
const BYTES: usize = 256 * 1024;
const BATCH: usize = 128;

#[derive(Default)]
struct State {
    events: VecDeque<TerminalEvent>,
    bytes: usize,
    wakeup: bool,
    closed: bool,
    batching: bool,
    #[cfg(feature = "adaptive-terminal-refresh")]
    sync_deadline: Option<std::time::Instant>,
    #[cfg(feature = "adaptive-terminal-refresh")]
    service_pending: bool,
}
#[derive(Default)]
struct Shared {
    #[cfg(feature = "adaptive-terminal-refresh")]
    demand: OnceLock<Arc<crate::terminal::refresh::Demand>>,
    #[cfg(feature = "adaptive-terminal-refresh")]
    demand_enabled: AtomicBool,
    state: Mutex<State>,
    capacity: Condvar,
    signal: Arc<Notify>,
}
#[derive(Clone)]
pub(super) struct Sender(Arc<Shared>);
pub(super) struct Receiver(Arc<Shared>);

pub(super) fn channel() -> (Sender, Receiver) {
    let shared = Arc::new(Shared::default());
    let receiver = Receiver(shared.clone());
    #[cfg(all(test, feature = "adaptive-terminal-refresh"))]
    if TEST_ADAPTIVE.get() {
        receiver.set_demand_enabled(true);
    }
    (Sender(shared), receiver)
}
#[cfg(feature = "adaptive-terminal-refresh")]
impl Shared {
    fn demand(&self) -> Option<Arc<crate::terminal::refresh::Demand>> {
        if !self.demand_enabled.load(Ordering::Acquire) {
            return None;
        }
        self.demand.get().cloned()
    }
}
#[cfg(feature = "adaptive-terminal-refresh")]
fn claim_service(state: &mut State) -> bool {
    if !state.events.is_empty() && !state.service_pending {
        state.service_pending = true;
        true
    } else {
        false
    }
}
#[cfg(all(test, feature = "adaptive-terminal-refresh"))]
thread_local! { static TEST_ADAPTIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) }; }
#[cfg(all(test, feature = "adaptive-terminal-refresh"))]
pub(super) fn with_adaptive<T>(f: impl FnOnce() -> T) -> T {
    struct Reset(bool);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_ADAPTIVE.set(self.0);
        }
    }
    let _reset = Reset(TEST_ADAPTIVE.replace(true));
    f()
}
fn bytes(event: &TerminalEvent) -> usize {
    match event {
        TerminalEvent::Title(text) | TerminalEvent::ClipboardStore(text) => text.len(),
        TerminalEvent::Reply(crate::terminal::reply::Reply::Text(text)) => text.len(),
        _ => 0,
    }
}
impl Sender {
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn demand_enabled(&self) -> bool {
        self.0.demand_enabled.load(Ordering::Acquire)
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    fn adaptive(&self) -> bool {
        self.0.demand().is_some_and(|d| d.active())
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn demand(&self) -> Option<Arc<crate::terminal::refresh::Demand>> {
        self.0.demand()
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn publish_parsed(&self, deadline: Option<std::time::Instant>) {
        if !self.0.demand_enabled.load(Ordering::Acquire) {
            self.publish_batch();
            return;
        }
        let changed = {
            let mut s = self.0.state.lock().unwrap();
            let changed = s.sync_deadline != deadline;
            s.sync_deadline = deadline;
            changed
        };
        self.publish_batch();
        if changed {
            self.0.signal.notify_one();
        }
        if let Some(d) = self.0.demand() {
            d.publish();
        }
    }
    pub(super) fn signal(&self) -> Arc<Notify> {
        self.0.signal.clone()
    }
    pub(super) fn send(&self, event: TerminalEvent) -> Result<(), ()> {
        let mut state = self.0.state.lock().unwrap();
        if state.closed {
            return Err(());
        }
        if matches!(event, TerminalEvent::Wakeup) {
            state.wakeup = true;
        } else {
            state.bytes += bytes(&event);
            state.events.push_back(event);
        }
        let publish = !state.batching;
        #[cfg(feature = "adaptive-terminal-refresh")]
        let publish = publish && (!self.adaptive() || claim_service(&mut state));
        drop(state);
        if publish {
            self.0.signal.notify_one();
        }
        Ok(())
    }
    pub(super) fn begin_batch(&self) {
        self.0.state.lock().unwrap().batching = true;
    }
    pub(super) fn publish_batch(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.batching = false;
        #[cfg(feature = "adaptive-terminal-refresh")]
        let publish = !self.adaptive() || claim_service(&mut state);
        #[cfg(not(feature = "adaptive-terminal-refresh"))]
        let publish = true;
        drop(state);
        if publish {
            self.0.signal.notify_one();
        }
    }
    /// One parser chunk may exceed the high-water mark; no next chunk is
    /// parsed until its events have been consumed. A large accepted OSC
    /// payload is kept intact, rather than silently truncated.
    pub(super) fn wait_capacity(&self) -> bool {
        let mut state = self.0.state.lock().unwrap();
        while !state.closed && (state.events.len() >= EVENTS || state.bytes >= BYTES) {
            state = self.0.capacity.wait(state).unwrap();
        }
        !state.closed
    }
}
impl Receiver {
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn demand_enabled(&self) -> bool {
        self.0.demand_enabled.load(Ordering::Acquire)
    }
    /// Caller serializes this with parser publication using the grid lock.
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn set_demand_enabled(&self, enabled: bool) {
        if self.0.demand_enabled.load(Ordering::Acquire) == enabled {
            return;
        }
        if enabled {
            let demand = self.0.demand.get_or_init(Default::default);
            demand.advance(); // Include the already existing grid on first/re-enable.
            self.0.demand_enabled.store(true, Ordering::Release);
        } else {
            if let Some(demand) = self.0.demand.get() {
                let dirty = demand.policy(true, false);
                self.0.demand_enabled.store(false, Ordering::Release);
                if dirty {
                    self.wake_current();
                }
            }
        }
    }

    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn demand(&self) -> Option<Arc<crate::terminal::refresh::Demand>> {
        self.0.demand()
    }
    #[cfg(feature = "adaptive-terminal-refresh")]
    pub(super) fn wake_current(&self) {
        self.0.state.lock().unwrap().wakeup = true;
        self.0.signal.notify_one();
    }
    pub(super) fn has_semantics(&self) -> bool {
        let state = self.0.state.lock().unwrap();
        !state.batching && !state.events.is_empty()
    }

    pub(super) fn signal(&self) -> Arc<Notify> {
        self.0.signal.clone()
    }
    pub(super) fn next(&self) -> Option<TerminalEvent> {
        let mut state = self.0.state.lock().unwrap();
        if state.batching {
            #[cfg(feature = "adaptive-terminal-refresh")]
            {
                state.service_pending = false;
            }
            return None;
        }
        let event = if let Some(event) = state.events.pop_front() {
            state.bytes -= bytes(&event);
            Some(event)
        } else if std::mem::take(&mut state.wakeup) {
            #[cfg(feature = "adaptive-terminal-refresh")]
            if self.0.demand().is_some_and(|d| d.active()) {
                state.service_pending = false;
                return None;
            }
            Some(TerminalEvent::Wakeup)
        } else {
            None
        };
        let more = !state.events.is_empty() || state.wakeup;
        #[cfg(feature = "adaptive-terminal-refresh")]
        let more = {
            if state.events.is_empty() {
                state.service_pending = false;
            }
            more && !self.0.demand().is_some_and(|d| d.active())
        };
        drop(state);
        self.0.capacity.notify_one();
        if more {
            self.0.signal.notify_one();
        }
        event
    }
    pub(super) fn try_iter(&self) -> impl Iterator<Item = TerminalEvent> + '_ {
        std::iter::from_fn(|| self.next()).take(BATCH)
    }
}
impl Receiver {
    pub(super) fn close(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.closed = true;
        state.events.clear();
        state.bytes = 0;
        self.0.capacity.notify_all();
        self.0.signal.notify_one();
    }
}
impl Drop for Receiver {
    fn drop(&mut self) {
        self.0.state.lock().unwrap().closed = true;
        #[cfg(feature = "adaptive-terminal-refresh")]
        if let Some(d) = self.0.demand.get() {
            d.close();
        }
        self.0.capacity.notify_all();
        self.0.signal.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn notification_before_registration_is_retained() {
        let (sender, receiver) = channel();
        sender.send(TerminalEvent::Title("before".into())).unwrap();
        let signal = receiver.signal();
        tokio::time::timeout(std::time::Duration::from_secs(1), signal.notified())
            .await
            .unwrap();
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            vec![TerminalEvent::Title("before".into())]
        );
    }
    #[tokio::test]
    async fn parser_batch_publishes_only_after_its_locks_are_released() {
        let (sender, receiver) = channel();
        sender.begin_batch();
        sender.send(TerminalEvent::Title("batch".into())).unwrap();
        sender.send(TerminalEvent::Wakeup).unwrap();
        assert!(receiver.next().is_none());
        let signal = receiver.signal();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(5), signal.notified())
                .await
                .is_err()
        );
        sender.publish_batch();
        tokio::time::timeout(std::time::Duration::from_secs(1), signal.notified())
            .await
            .unwrap();
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            vec![TerminalEvent::Title("batch".into()), TerminalEvent::Wakeup]
        );
    }
    #[test]
    fn wakeups_coalesce_without_discarding_semantics() {
        let (sender, receiver) = channel();
        for i in 0..1000 {
            sender.send(TerminalEvent::Wakeup).unwrap();
            if i % 100 == 0 {
                sender.send(TerminalEvent::Title(i.to_string())).unwrap();
            }
        }
        let events = receiver.try_iter().collect::<Vec<_>>();
        assert_eq!(events.len(), 11);
        assert_eq!(events.last(), Some(&TerminalEvent::Wakeup));
        assert_eq!(
            &events[..10],
            &(0..1000)
                .step_by(100)
                .map(|i| TerminalEvent::Title(i.to_string()))
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn closing_receiver_releases_backpressured_producer() {
        let (sender, receiver) = channel();
        sender
            .send(TerminalEvent::ClipboardStore("x".repeat(BYTES + 1)))
            .unwrap();
        let worker = std::thread::spawn(move || sender.wait_capacity());
        drop(receiver);
        assert!(!worker.join().unwrap());
    }
    #[test]
    fn a_slow_consumer_bounds_the_producer_without_losing_order() {
        let (sender, receiver) = channel();
        let observer = sender.clone();
        let worker = std::thread::spawn(move || {
            for chunk in 0..100 {
                assert!(sender.wait_capacity());
                sender.begin_batch();
                for offset in 0..32 {
                    sender
                        .send(TerminalEvent::Title((chunk * 32 + offset).to_string()))
                        .unwrap();
                }
                sender.publish_batch();
            }
        });
        let mut seen = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while seen < 3200 {
            assert!(std::time::Instant::now() < deadline);
            {
                let state = observer.0.state.lock().unwrap();
                assert!(
                    state.events.len() <= EVENTS + 32,
                    "more than one pending parser batch"
                );
            }
            for event in receiver.try_iter() {
                assert_eq!(event, TerminalEvent::Title(seen.to_string()));
                seen += 1;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        worker.join().unwrap();
    }
}

#[cfg(all(test, feature = "adaptive-terminal-refresh"))]
mod adaptive_tests {
    use super::*;
    #[test]
    fn runtime_setting_is_lazy_and_fences_existing_pending_work_without_losing_events() {
        use crate::terminal::refresh::Action;
        let (sender, receiver) = channel();
        assert!(
            receiver.0.demand.get().is_none(),
            "disabled channels allocate no demand"
        );
        receiver.set_demand_enabled(false);
        assert!(receiver.0.demand.get().is_none());
        receiver.set_demand_enabled(true);
        let demand = receiver.demand().unwrap();
        demand.policy(true, true);
        let now = std::time::Instant::now();
        assert_eq!(demand.action(now), Action::Notify);
        demand.acknowledge(demand.capture());
        demand.advance();
        demand.publish();
        assert!(matches!(demand.action(now), Action::Wait(_)));
        let old = demand.capture();
        sender
            .send(TerminalEvent::Title("retained".into()))
            .unwrap();
        receiver.set_demand_enabled(false);
        assert!(receiver.demand().is_none());
        assert_eq!(demand.action(now), Action::Idle);
        assert!(!demand.acknowledge(old));
        let events: Vec<_> = receiver.try_iter().collect();
        assert!(events.contains(&TerminalEvent::Wakeup));
        assert!(events.contains(&TerminalEvent::Title("retained".into())));
        receiver.set_demand_enabled(true);
        demand.policy(true, true);
        assert!(demand.capture().generation > old.generation);
        assert_eq!(
            demand.action(now + crate::terminal::refresh::INTERVAL),
            Action::Notify
        );
        // Disable while waiting for paint as well as while waiting for the cap.
        receiver.set_demand_enabled(false);
        assert_eq!(
            demand.action(now + std::time::Duration::from_secs(2)),
            Action::Idle
        );
    }
    #[test]
    fn sixty_hz_fallback_delivers_the_same_batched_events_as_current() {
        use crate::terminal::refresh::{Demand, eligible};
        let drain = |fps, experimental: bool| {
            let demand = Arc::new(Demand::default());
            let adaptive = eligible(
                true,
                Some(true),
                true,
                fps,
                Some(true),
                gpui::ThermalState::Nominal,
            );
            demand.policy(true, adaptive);
            let shared = Arc::new(Shared {
                demand: if experimental {
                    OnceLock::from(demand)
                } else {
                    OnceLock::new()
                },
                demand_enabled: AtomicBool::new(experimental),
                ..Default::default()
            });
            let sender = Sender(shared.clone());
            let receiver = Receiver(shared);
            sender.begin_batch();
            for _ in 0..10000 {
                sender.send(TerminalEvent::Wakeup).unwrap();
            }
            sender.send(TerminalEvent::Title("ordered".into())).unwrap();
            sender.send(TerminalEvent::Exited).unwrap();
            assert_eq!(receiver.next(), None, "batch not published yet");
            sender.publish_batch();
            receiver.try_iter().collect::<Vec<_>>()
        };
        let current = drain(None, false);
        assert!(current.contains(&TerminalEvent::Wakeup));
        assert_eq!(
            current
                .iter()
                .filter(|e| matches!(e, TerminalEvent::Wakeup))
                .count(),
            1
        );
        assert_eq!(drain(None, true), current);
        assert_eq!(drain(Some(60), true), current);
        assert!(!drain(Some(120), true).contains(&TerminalEvent::Wakeup));
    }
    #[tokio::test]
    async fn batching_race_releases_service_claim_for_publication() {
        let demand = Arc::new(crate::terminal::refresh::Demand::default());
        demand.policy(true, true);
        let shared = Arc::new(Shared {
            demand: OnceLock::from(demand),
            demand_enabled: AtomicBool::new(true),
            ..Default::default()
        });
        let sender = Sender(shared.clone());
        let receiver = Receiver(shared);
        sender.send(TerminalEvent::Title("first".into())).unwrap();
        receiver.signal().notified().await;
        sender.begin_batch();
        assert_eq!(receiver.next(), None);
        assert!(!receiver.has_semantics());
        sender.publish_batch();
        tokio::time::timeout(
            std::time::Duration::from_millis(20),
            receiver.signal().notified(),
        )
        .await
        .unwrap();
        assert_eq!(receiver.next(), Some(TerminalEvent::Title("first".into())));
    }
    #[tokio::test]
    async fn semantic_claim_stays_owned_until_queue_is_drained() {
        let demand = Arc::new(crate::terminal::refresh::Demand::default());
        demand.policy(true, true);
        let shared = Arc::new(Shared {
            demand: OnceLock::from(demand),
            demand_enabled: AtomicBool::new(true),
            ..Default::default()
        });
        let sender = Sender(shared.clone());
        let receiver = Receiver(shared);
        sender.send(TerminalEvent::Title("first".into())).unwrap();
        receiver.signal().notified().await;
        sender.begin_batch();
        sender.send(TerminalEvent::Exited).unwrap();
        sender.publish_batch();
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                receiver.signal().notified()
            )
            .await
            .is_err()
        );
        assert_eq!(receiver.try_iter().count(), 2);
        sender
            .send(TerminalEvent::Reply(crate::terminal::reply::Reply::Text(
                "reply".into(),
            )))
            .unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            receiver.signal().notified(),
        )
        .await
        .unwrap();
        assert_eq!(receiver.try_iter().count(), 1);
    }
    #[tokio::test]
    async fn output_flood_does_not_schedule_semantic_updates_but_protocol_does() {
        let demand = Arc::new(crate::terminal::refresh::Demand::default());
        demand.policy(true, true);
        let shared = Arc::new(Shared {
            demand: OnceLock::from(demand),
            demand_enabled: AtomicBool::new(true),
            ..Default::default()
        });
        let sender = Sender(shared.clone());
        let receiver = Receiver(shared);
        for _ in 0..10000 {
            sender.begin_batch();
            sender.send(TerminalEvent::Wakeup).unwrap();
            sender.publish_batch();
        }
        assert!(
            tokio::time::timeout(
                std::time::Duration::from_millis(1),
                receiver.signal().notified()
            )
            .await
            .is_err()
        );
        sender.send(TerminalEvent::Title("ordered".into())).unwrap();
        sender.send(TerminalEvent::Exited).unwrap();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            receiver.signal().notified(),
        )
        .await
        .unwrap();
        assert_eq!(
            receiver.try_iter().collect::<Vec<_>>(),
            vec![
                TerminalEvent::Title("ordered".into()),
                TerminalEvent::Exited
            ]
        );
    }
}
