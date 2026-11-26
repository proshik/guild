//! Capability-only real Root fixture. No providers, user project or refresh changes.
use crate::{Body, Root};
use gpui::{App, AppContext, Entity, Window};
use guild_session::{Desk, Runs, Seat, SessionRecord, Store};
use std::{
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

pub static ROOT_RENDERS: AtomicU64 = AtomicU64::new(0);
pub static REGION_RENDERS: AtomicU64 = AtomicU64::new(0);

pub fn root(fixture: &Path, window: &mut Window, cx: &mut App) -> anyhow::Result<Entity<Root>> {
    let mode = std::env::var("GUILD_PROBE_MODE").unwrap_or("output".into());
    Ok(root_with_terminal(fixture, window, cx, &mode)?.0)
}

/// The policy trial uses a quiet synthetic PTY and notifies only its terminal entity.
pub fn root_with_terminal(
    fixture: &Path,
    window: &mut Window,
    cx: &mut App,
    mode: &str,
) -> anyhow::Result<(Entity<Root>, Entity<crate::terminal::view::TerminalTab>)> {
    anyhow::ensure!(
        [
            "quiet",
            "hidden",
            "hidden-output",
            "output",
            "generations",
            "stripe",
            "stripe-ack"
        ]
        .contains(&mode),
        "Unsupported fixture mode"
    );
    anyhow::ensure!(
        fixture.is_absolute()
            && fixture.starts_with(crate::paths::state_dir())
            && !fixture.exists(),
        "Use fresh task-owned fixture"
    );
    anyhow::ensure!(
        std::env::var_os("GIT_CEILING_DIRECTORIES").as_deref()
            == Some(crate::paths::state_dir().as_os_str()),
        "Set GIT_CEILING_DIRECTORIES to the isolated state directory; synthetic projects must not resolve the enclosing checkout"
    );
    let mut desk = Desk::default();
    let mut first = None;
    for i in 0..12 {
        let path = fixture.join(format!("project-{i:02}"));
        std::fs::create_dir_all(&path)?;
        std::fs::write(
            path.join("fixture.txt"),
            "Synthetic Root capability fixture\n",
        )?;
        let id = desk.open_in(desk.active_workspace(), &path);
        first.get_or_insert(id);
    }
    desk.look_at(first, None);
    Store::new(crate::paths::workspace_file()).save(&desk)?;
    let state_dir = crate::paths::state_dir();
    let root = cx.new(|cx| Root::diagnostic(desk, state_dir, cx));
    let terminal = root.update(cx, |root, cx| {
        root.settings = super::resource_probe::fixture_settings();
        let project = root.active_project.unwrap();
        let seat = Seat {
            workspace: root.desk.active_workspace(),
            project,
        };
        let count: usize = std::env::var("GUILD_PROBE_TABS")
            .unwrap_or("1".into())
            .parse()
            .unwrap();
        assert!((1..=20).contains(&count));
        let mut last = None;
        for index in 0..count {
            let session = root.desk.add_session(SessionRecord::new(
                "Synthetic output".into(),
                seat,
                fixture.join("project-00"),
                Runs::Shell,
            ));
            let view = super::resource_probe::terminal(
                if index + 1 == count { mode } else { "quiet" },
                cx,
            );
            let facts = view.read(cx).facts().clone();
            let tab = root.adopt(
                seat,
                project,
                Body::Terminal {
                    session,
                    view: view.clone(),
                    facts,
                },
                cx,
            );
            root.active_tab = Some(tab);
            root.focusing_front = true;
            root.save();
            cx.notify();
            last = Some(view);
        }
        // Hidden: the window's Settings page in front of every terminal,
        // as the removed Sessions page once stood over them.
        if mode.starts_with("hidden") {
            root.open_settings(cx);
        }
        last.unwrap()
    });
    let seconds: u64 = std::env::var("GUILD_PROBE_SECONDS")
        .unwrap_or("20".into())
        .parse()
        .unwrap();
    assert!((6..=600).contains(&seconds));
    if mode == "stripe-ack" {
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                for seq in 1..=(seconds - 4) * 10 {
                    let _ = cx.update(|window, cx| {
                        super::frame_meter::input(seq);
                        for digit in format!("{seq:06}").chars() {
                            window.dispatch_keystroke(
                                gpui::Keystroke::parse(&digit.to_string()).unwrap(),
                                cx,
                            );
                        }
                        window.dispatch_keystroke(gpui::Keystroke::parse("enter").unwrap(), cx);
                    });
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
            })
            .detach();
    }
    let scenario = std::env::var("GUILD_PROBE_SCENARIO").unwrap_or_default();
    anyhow::ensure!(
        ["", "scroll", "selection", "migration", "settings"].contains(&scenario.as_str()),
        "Unsupported interaction scenario"
    );
    anyhow::ensure!(
        scenario != "scroll" || mode == "output",
        "Scroll needs output history"
    );
    anyhow::ensure!(
        scenario != "migration" || cfg!(feature = "native-policy-probe"),
        "Migration requires native-policy-probe"
    );
    if scenario == "settings" {
        root.update(cx, |root, cx| {
            root.open_settings_at(crate::view::settings::Section::Terminal, cx)
        });
    }
    let started = Instant::now();
    println!(
        "{{\"observed_visibility_seconds\":0,\"visible\":{}}}",
        window.is_visible()
    );
    window
        .observe_window_visibility(move |visibility, _, _| {
            println!(
                "{{\"observed_visibility_seconds\":{:.6},\"visible\":{}}}",
                started.elapsed().as_secs_f64(),
                visibility.is_visible()
            );
        })
        .detach();
    let observed_root = root.downgrade();
    let observed_terminal = terminal.clone();
    window.spawn(cx, async move |cx| {
        for tick in 1..=seconds {
            cx.background_executor().timer(Duration::from_secs(1)).await;
            let _ = cx.update(|window, cx| {
                if scenario == "scroll" && [4, 6, 8].contains(&tick) {
                    let key = match tick { 4 => "shift-pageup", 6 => "shift-pagedown", _ => "cmd-end" };
                    window.dispatch_keystroke(gpui::Keystroke::parse(key).unwrap(), cx);
                    println!("{}", serde_json::json!({"scenario":"scroll","tick":tick,"key":key,"endpoint":"public GPUI input dispatch; not hardware"}));
                }
                if scenario == "selection" && [4, 6].contains(&tick) {
                    if tick == 4 {
                        window.dispatch_action(Box::new(crate::actions::SelectAll), cx);
                    } else {
                        window.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), cx);
                    }
                    println!("{}", serde_json::json!({"scenario":"selection","tick":tick,"action":if tick==4 {"select-all"} else {"clear-by-input"},"clipboard":"disabled"}));
                }
                if !scenario.is_empty() {
                    let (offset, selected) = observed_terminal.read(cx).probe_interaction_state();
                    println!("{}", serde_json::json!({"scenario_observation":&scenario,"tick":tick,"scroll_offset":offset,"selected":selected}));
                }
                #[cfg(feature = "native-policy-probe")]
                if scenario == "migration" && [4, 8, 12].contains(&tick) {
                    let target = if tick == 8 { 1 } else { 4 };
                    let moved = crate::system::window_probe::move_to_display(window, target);
                    println!("{}", serde_json::json!({"scenario":"migration","tick":tick,"target_display":target,"move_requested":moved,"observed_display":window.display(cx).map(|d|u64::from(d.id()))}));
                }
                println!("{{\"root_probe_seconds\":{},\"root_renders\":{},\"region_renders\":{},\"terminal_paints\":{},\"visible\":{},\"active\":{}}}", tick, ROOT_RENDERS.load(Ordering::Relaxed), REGION_RENDERS.load(Ordering::Relaxed), super::resource_probe::PAINTS.load(Ordering::Relaxed), window.is_visible(), window.is_window_active());
                let _ = observed_root.update(cx, |root, cx| {
                    #[cfg(feature = "adaptive-terminal-refresh")]
                    if scenario == "settings" {
                        println!("{}", serde_json::json!({"settings_tick":tick,"preference":root.settings.adaptive_terminal_refresh,"language":format!("{:?}",root.settings.language),"terminal":observed_terminal.read(cx).refresh_probe()}));
                    }
                    println!("{{\"fixture_tabs\":{},\"fixture_has_front\":{},\"fixture_hidden\":{},\"terminal_exited\":{},\"polls\":{}}}", root.tabs.len(), root.active_tab.is_some(), matches!(root.active_body(), Some(Body::Settings { .. })), observed_terminal.read(cx).facts().exited(), super::resource_probe::POLLS.load(Ordering::Relaxed));
                });
                println!("{{\"output_notifies\":{},\"output_wakes\":{},\"current_handoffs\":{}}}", super::resource_probe::OUTPUT_NOTIFIES.load(Ordering::Relaxed),super::resource_probe::OUTPUT_WAKES.load(Ordering::Relaxed),super::resource_probe::CURRENT_HANDOFFS.load(Ordering::Relaxed));
                println!("{}",serde_json::json!({"environment_mach_ticks":crate::system::probe_mach_ticks(),"display":window.display(cx).map(|d|u64::from(d.id())),"scale":window.scale_factor(),"thermal":format!("{:?}",cx.thermal_state()),"power":crate::system::probe_low_power(),"gpu_ns":crate::system::probe_gpu_ns()}));
                if tick == seconds {
                    super::frame_meter::flush();
                    observed_terminal.update(cx, |terminal, _| terminal.cancel_job());
                    cx.quit();
                }
            });
        }
    }).detach();
    Ok((root, terminal))
}
