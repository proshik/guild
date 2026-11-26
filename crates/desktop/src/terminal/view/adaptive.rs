//! One demand consumer per opt-in terminal, without frame callbacks.
use super::*;
use crate::terminal::refresh::{self, Action};
#[derive(Default)]
pub(super) struct Adaptive {
    shown: Option<bool>,
    task: Option<gpui::Task<()>>,
    subscriptions: Vec<gpui::Subscription>,
    power: Option<crate::system::terminal_power::Watch>,
}
impl TerminalTab {
    #[cfg(any(test, feature = "native-preview"))]
    pub(crate) fn refresh_ownership(&self) -> (bool, bool, usize, bool) {
        (
            self.emulator.visual_demand().is_some(),
            self.adaptive.task.is_some(),
            self.adaptive.subscriptions.len(),
            self.adaptive.power.is_some(),
        )
    }

    #[cfg(feature = "native-preview")]
    pub(crate) fn refresh_probe(&self) -> serde_json::Value {
        serde_json::json!({"ownership":self.refresh_ownership(), "demand":self.emulator.visual_demand().map(|d| d.probe_state())})
    }

    pub(crate) fn set_refresh_enabled(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = enabled && cfg!(target_os = "macos");
        if self.emulator.visual_demand().is_some() == enabled {
            return;
        }
        self.emulator.set_adaptive_enabled(enabled);
        if enabled {
            if let Some(shown) = self.adaptive.shown {
                self.set_surface_visible(shown, window, cx);
            }
        } else {
            let shown = self.adaptive.shown;
            // Dropping the task drops its pending strict one-shot. Subscribers and
            // power watcher are owned by this terminal, never process-global work.
            self.adaptive = Adaptive {
                shown,
                ..Default::default()
            };
        }
    }

    /// Hosts must explicitly report both hide and reveal; rendering alone is
    /// not evidence that a retained entity is still on screen.
    pub(crate) fn set_surface_visible(
        &mut self,
        shown: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.adaptive.shown = Some(shown);
        let Some(demand) = self.emulator.visual_demand() else {
            return;
        };
        if self.adaptive.task.is_none() {
            self.adaptive.power = Some(crate::system::terminal_power::Watch::new(&demand.signal));
            self.adaptive
                .subscriptions
                .push(cx.observe_window_activation(window, |tab, window, cx| {
                    tab.refresh_policy_and_repaint(window, cx);
                }));
            self.adaptive
                .subscriptions
                .push(cx.observe_window_visibility(window, |tab, _, window, cx| {
                    tab.refresh_policy_and_repaint(window, cx);
                }));
            self.adaptive.subscriptions.push(cx.observe_window_bounds(
                window,
                |tab, window, cx| {
                    tab.refresh_policy_and_repaint(window, cx);
                },
            ));
            let thermal = demand.signal.clone();
            self.adaptive
                .subscriptions
                .push(cx.on_thermal_state_change(move |_| thermal.notify_one()));
            self.adaptive.task = Some(cx.spawn_in(window, async move |tab, cx| {
                loop {
                    demand.signal.notified().await;
                    #[cfg(feature = "native-preview")]
                    crate::native_preview::resource_probe::OUTPUT_WAKES.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
                    loop {
                        let action = cx.update(|window,cx| tab.update(cx, |tab,cx| {
                            tab.refresh_policy(window,cx);
                            let action = demand.action(std::time::Instant::now());
                            #[cfg(feature = "native-preview")]
                            match &action {
                                Action::Notify => crate::native_preview::frame_meter::demand_action(None),
                                Action::Wait(delay) => crate::native_preview::frame_meter::demand_action(Some(delay.as_nanos() as u64)),
                                Action::Idle => {},
                            }
                            if action == Action::Notify {
                                #[cfg(feature = "native-preview")]
                                crate::native_preview::resource_probe::OUTPUT_NOTIFIES.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
                                cx.notify();
                            }
                            action
                        }));
                        match action {
                            Ok(Ok(Action::Wait(delay))) => {
                                if let Some(timer) = crate::system::terminal_deadline::Deadline::new(delay) {
                                    tokio::select! { _ = timer => {}, _ = demand.signal.notified() => {} }
                                } else {
                                    let timer = cx.background_executor().timer(delay);
                                    tokio::select! { _ = timer => {}, _ = demand.signal.notified() => {} }
                                }
                            }
                            Ok(Ok(_)) => break,
                            _ => return,
                        }
                    }
                }
            }));
        }
        self.refresh_policy_and_repaint(window, cx);
    }

    /// The host's answer set as it stands, without asking the policy: what
    /// a covered window leaves when the host still has the terminal in
    /// front.
    #[cfg(test)]
    pub(crate) fn set_host_shown_for_test(&mut self, shown: bool) {
        self.adaptive.shown = Some(shown);
    }

    /// [`Self::refresh_policy`] from a host's or the window's news, outside
    /// a paint: a terminal shown again asks for the frame that paints it.
    /// Hidden, it cleared `painted`, so nothing it printed since asked for
    /// one, and the tab's cached body would replay the grid from before.
    fn refresh_policy_and_repaint(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.refresh_policy(window, cx) && !self.painted {
            cx.notify();
        }
    }
    pub(super) fn refresh_policy(&mut self, window: &Window, cx: &gpui::App) -> bool {
        let Some(demand) = self.emulator.visual_demand() else {
            return true;
        };
        let state = crate::system::terminal_power::state(window);
        let native = state.map(|s| s.0);
        // Unknown hosts retain Current. A known hidden host never paints.
        let shown = self.adaptive.shown != Some(false) && native != Some(false);
        let adaptive = refresh::eligible(
            self.adaptive.shown == Some(true),
            native,
            window.is_window_active() && state.is_some_and(|s| s.1),
            window
                .display(cx)
                .and_then(|_| crate::system::terminal_power::maximum_frames_per_second(window)),
            self.adaptive.power.as_ref().and_then(|p| p.normal()),
            cx.thermal_state(),
        );
        if demand.policy(shown, adaptive) {
            #[cfg(feature = "native-preview")]
            crate::native_preview::resource_probe::CURRENT_HANDOFFS
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            self.emulator.wake_current();
        }
        if !shown {
            self.painted = false;
        }
        shown
    }
}
