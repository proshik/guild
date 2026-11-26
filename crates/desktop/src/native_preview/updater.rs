//! Focused owned-app restart fixture. The constructor always chooses Diagnostic;
//! it cannot start a provider, IDE, hooks, sessions or usage requests.
use crate::{Root, theme};
use gpui::{AppContext, Context, Entity, IntoElement, Render, Window, div, prelude::*};
use guild_session::update::{BuildInfo, bundle, transaction::Startup};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
/// Owns the fixture clock and its final evidence write through application quit.
pub struct UpdaterPreview {
    root: Entity<Root>,
}
impl UpdaterPreview {
    /// Caller validates the task-owned marker and captures startup context before GPUI.
    pub fn new(
        state: PathBuf,
        info: BuildInfo,
        startup: Option<Startup>,
        approval: Option<uuid::Uuid>,
        instance: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), state.clone(), cx);
            root.set_running_update_info(Some(info));
            root.set_update_startup(startup);
            root.enable_updater().expect("fixture updater worker");
            root.open_settings_at(crate::view::settings::Section::Updates, cx);
            root
        });
        let stopping = Arc::new(AtomicBool::new(false));
        let stop = stopping.clone();
        let weak = root.downgrade();
        let clock = window.spawn(cx, async move |cx| {
            let mut approval = approval;
            let mut recorded = false;
            loop {
                if stop.load(Ordering::Acquire) { break; }
                let snapshot = cx.update(|_, cx| weak.update(cx, |root, cx| {
                    root.poll_update(cx);
                    let snapshot = if !recorded && root.update_preferences_ready()
                        && (root.update_download_ready() || root.update_failure().is_some()) {
                        Some(serde_json::json!({"instance":instance,"pid":std::process::id(),
                            "preferences":root.update_preferences(),"build":root.running_update_info(),
                            "ready":root.update_download_ready(),"error":format!("{:?}",root.update_failure()),
                            "checking":root.update_checking(),"candidates":root.update_candidates().len()}))
                    } else { None };
                    if let Some(expected) = approval {
                        match root.approve_updater_fixture(expected, cx) {
                            Ok(true) => approval = None,
                            Ok(false) => (),
                            Err(reason) => { eprintln!("{reason}"); approval = None; }
                        }
                    }
                    snapshot
                }));
                match snapshot {
                    Ok(Ok(Some(snapshot))) => {
                        recorded = true;
                        let path = state.join(format!("ready-{instance}.json"));
                        let _ = cx.background_executor().spawn(async move { bundle::publish_json(&path, &snapshot) }).await;
                    }
                    Ok(Ok(None)) => (),
                    _ => break,
                }
                if stop.load(Ordering::Acquire) { break; }
                cx.background_executor().timer(Duration::from_millis(50)).await;
            }
        });
        root.update(cx, |root, _| {
            root.retain_updater_fixture_clock(clock, stopping)
        });
        Self { root }
    }
}
impl Render for UpdaterPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(gpui::rgb(theme::Theme::default().surface.app))
            .child(self.root.clone())
    }
}

/// Isolated production Settings composition; cannot acquire or install any app.
pub fn card_root(window: &mut Window, cx: &mut gpui::App) -> Entity<Root> {
    let state = crate::paths::state_dir();
    assert_eq!(
        std::fs::read_to_string(state.join(".owned-update-card-fixture"))
            .as_deref()
            .ok(),
        Some("Guild isolated update card fixture\n")
    );
    let settings =
        guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
            .load()
            .unwrap_or_default();
    let root = cx.new(|cx| {
        let mut root = Root::from_state_with_policy(
            settings,
            Default::default(),
            crate::root::isolation::Policy::Diagnostic { state_dir: state },
            cx,
        );
        root.enable_updater().unwrap();
        root.open_settings_at(crate::view::settings::Section::Updates, cx);
        root
    });
    let stopping = Arc::new(AtomicBool::new(false));
    let stop = stopping.clone();
    let weak = root.downgrade();
    let clock = window.spawn(cx, async move |cx| {
        let mut seeded = false;
        loop {
            if stop.load(Ordering::Acquire) {
                break;
            }
            let done = cx.update(|_, cx| {
                weak.update(cx, |root, cx| {
                    root.poll_update(cx);
                    if !seeded && root.update_ready_to_check() && !root.update_checking() {
                        root.seed_update_card_fixture(
                            &std::env::var("GUILD_UPDATE_CARD_STATE")
                                .unwrap_or_else(|_| "available".into()),
                            cx,
                        );
                        seeded = true;
                    }
                })
            });
            if done.is_err() || matches!(done, Ok(Err(_))) {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
    });
    root.update(cx, |root, _| {
        root.retain_updater_fixture_clock(clock, stopping)
    });
    root
}
