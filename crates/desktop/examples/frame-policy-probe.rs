//! Short native characterization of production GPUI frame policy, not an FPS benchmark.
use gpui::profiler::journal::{ForegroundJournalEntry as Entry, IntervalBoundary as Boundary};
use gpui::{
    App, AppContext, Bounds, Context, Render, Window, WindowBounds, WindowOptions, div, prelude::*,
    px, size,
};
use std::time::{Duration, Instant};

struct Marker {
    generation: u64,
    root: Option<gpui::AnyView>,
    terminal: Option<gpui::Entity<guild_desktop::terminal::view::TerminalTab>>,
}
impl Render for Marker {
    fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        println!(
            "{}",
            serde_json::json!({"paint_generation": self.generation, "active": window.is_window_active(), "observed_visible": window.is_visible()})
        );
        div()
            .size_full()
            .children(self.root.clone())
            .when(self.root.is_none(), |d| {
                d.child(format!("Owned policy probe generation {}", self.generation))
            })
    }
}
fn options(x: f32, title: &str) -> WindowOptions {
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            gpui::point(px(x), px(100.)),
            size(px(460.), px(360.)),
        ))),
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(title.to_owned().into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert_eq!(
        options.inactive_frame_interval,
        Some(Duration::from_micros(33333))
    );
    options
}
fn main() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::current_dir()? == std::path::Path::new("/"),
        "Run from /"
    );
    let state = std::path::PathBuf::from(
        std::env::var_os("GUILD_STATE_DIR")
            .ok_or_else(|| anyhow::anyhow!("Fresh state required"))?,
    );
    anyhow::ensure!(
        state.is_absolute() && !state.exists(),
        "Fresh absolute state required"
    );
    anyhow::ensure!(
        std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref() == Ok("1"),
        "Disable hooks"
    );
    anyhow::ensure!(
        std::env::var_os("GIT_CEILING_DIRECTORIES").as_deref() == Some(state.as_os_str()),
        "Git ceiling required"
    );
    std::fs::create_dir_all(&state)?;
    let root_mode = std::env::var("GUILD_PROBE_ROOT").as_deref() == Ok("1");
    println!(
        "{}",
        serde_json::json!({"permissions": guild_desktop::system::window_probe::permissions(), "pid": std::process::id(), "inactive_us": 33333, "acceptance": "scheduling_only"})
    );
    gpui_kit::application().with_assets(guild_desktop::icons::Icons).run(move |cx: &mut App| {
        guild_desktop::fonts::register(cx);
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(),13.,cx);
        let mut target = None;
        let first = cx.open_window(options(50., "Owned frame policy target"), |window,cx| {
            let (root,terminal) = if root_mode {
                let (root,terminal) = guild_desktop::native_preview::adaptive_refresh_probe::root_with_terminal(&state.join("fixture"),window,cx,"quiet").unwrap();
                (Some(root.into()),Some(terminal))
            } else { (None,None) };
            let marker = cx.new(|_| Marker {generation:0,root,terminal});
            target = Some(marker.clone());
            cx.new(|cx| gpui_component::Root::new(marker,window,cx))
        }).unwrap();
        let target = target.unwrap();
        let second = cx.open_window(options(550., "Owned activation control"), |_,cx| cx.new(|_| Marker { generation: 0, root:None, terminal:None })).unwrap();
        let observer = first.update(cx, |_,w,_| guild_desktop::system::window_probe::Observer::new(w).unwrap()).unwrap();
        let mut collector = cx.foreground_journal().collector();
        cx.activate(true);
        cx.spawn(async move |cx| {
            // Allow initial activation, resize and input-rate windows to settle.
            cx.background_executor().timer(Duration::from_secs(2)).await;
            let mut total_lost = 0;
            for mode in ["current", "fenced", "candidate", "current-unrelated", "candidate-unrelated"] {
                first.update(cx, |_,w,_| w.activate_window()).unwrap();
                cx.background_executor().timer(Duration::from_millis(1100)).await;
                total_lost += collector.collect_unseen().lost;
                let began = Instant::now();
                for generation in 1..=24 {
                    first.update(cx, |_,w,cx| target.update(cx, |marker,cx| {
                        // The only difference is the rejected terminal-specific ticket.
                        // It deliberately does no work when drained: fencing cannot remove it.
                        if mode == "fenced" || mode.ends_with("-unrelated") { w.on_next_frame(|_,_| {}); }
                        marker.generation = generation;
                        if let Some(terminal) = &marker.terminal { terminal.update(cx,|_,cx| cx.notify()); } else { cx.notify(); }
                        if generation == 1 { second.update(cx, |_,w,_| w.activate_window()).unwrap(); }
                    })).unwrap();
                    // Finite diagnostic stimulus only, never the product refresh scheduler.
                    cx.background_executor().timer(Duration::from_millis(8)).await;
                    let drained = collector.collect_unseen();
                    total_lost += drained.lost;
                    for entry in drained.entries {
                        let record = match entry {
                            Entry::Boundary(Boundary::FrameSkipped(v)) if v.window_id == first.window_id() => Some(serde_json::json!({"kind":"skip", "reason":format!("{:?}",v.reason), "seconds":v.at.duration_since(began).as_secs_f64()})),
                            Entry::Boundary(Boundary::Presented(v)) if v.frame.window_id == first.window_id() => Some(serde_json::json!({"kind":"submission", "seconds":v.presentation.present_end.duration_since(began).as_secs_f64(), "invalidations":v.frame.invalidations})),
                            Entry::PlatformSignal(v) if v.window_id == first.window_id() => Some(serde_json::json!({"kind":"signal", "source":format!("{:?}",v.source), "seconds":v.handled_at.duration_since(began).as_secs_f64()})),
                            _ => None,
                        };
                        if let Some(record) = record { println!("{}",serde_json::json!({"mode":mode,"event":record})); }
                    }
                    let native = observer.sample();
                    println!("{}",serde_json::json!({"mode":mode, "generation":generation, "native_key":native.key,"native_visible":native.visible,"native_occluded":native.occluded,"lost":drained.lost, "root_renders":guild_desktop::native_preview::adaptive_refresh_probe::ROOT_RENDERS.load(std::sync::atomic::Ordering::Relaxed), "region_renders":guild_desktop::native_preview::adaptive_refresh_probe::REGION_RENDERS.load(std::sync::atomic::Ordering::Relaxed)}));
                }
                cx.background_executor().timer(Duration::from_millis(100)).await;
            }
            total_lost += collector.collect_unseen().lost;
            let (before, lost) = observer.drain();
            let (hidden, revealed) = observer.hide_reveal();
            let (immediate, lost2) = observer.drain();
            cx.background_executor().timer(Duration::from_millis(100)).await;
            let (later,lost3) = observer.drain();
            println!("{}",serde_json::json!({"visibility_trial":true,"prior_events":before.len(),"commanded_hidden":!hidden.visible,"commanded_revealed":revealed.visible,"immediate_hidden_events":immediate.iter().filter(|v| !v.visible).count(),"later_hidden_events":later.iter().filter(|v| !v.visible).count(),"visibility_lost":lost+lost2+lost3,"journal_lost":total_lost,"continuous_visibility":"UNVERIFIED"}));
            target.update(cx, |marker,cx| { if let Some(terminal) = &marker.terminal { terminal.update(cx,|terminal,_| terminal.cancel_job()); } });
            drop(observer);
            cx.update(|cx| cx.quit());
        }).detach();
    });
    Ok(())
}
