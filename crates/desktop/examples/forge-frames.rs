//! The frames a landed merge request answer costs a large column in a
//! native window — GLD-063. A measurement, not a contract: it prints each
//! phase's `Window::draw` times from GPUI's profiler and asserts nothing
//! about them. The window must stay visible: an occluded GPUI window draws
//! nothing, and samples taken while occluded are counted apart.
//!
//! ```sh
//! cd / && env GUILD_STATE_DIR=<fresh absolute dir> GUILD_NO_GLOBAL_HOOKS=1 \
//!   cargo run --release --manifest-path <repo>/Cargo.toml -p guild-desktop \
//!   --features native-policy-probe --example forge-frames
//! ```
//!
//! `FORGE_REPOS` (40), `FORGE_WT` (3), `FORGE_TABS` (4) size the column as
//! the column's perf bench does. Three phases of 40 answers each, a
//! repository at a time: the same MRs again notified always (`poll_forge`
//! before the notify fix), the same MRs notified only on a change (now),
//! and a changed MR.
use gpui::profiler::journal::{ForegroundJournalEntry as Entry, IntervalBoundary as Boundary};
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use guild_desktop::native_preview::forge_frames::{self as driver, Notify};
use std::time::{Duration, Instant};

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn summary(
    label: &str,
    answers: usize,
    notified: usize,
    occluded: usize,
    mut draws: Vec<Duration>,
) {
    draws.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.;
    let at = |q: f64| {
        draws
            .get(((draws.len() as f64 - 1.) * q) as usize)
            .map(|d| ms(*d))
    };
    println!(
        "{}",
        serde_json::json!({
            "phase": label,
            "answers": answers,
            "notified": notified,
            "frames": draws.len(),
            "occluded_samples": occluded,
            "draw_ms_median": at(0.5),
            "draw_ms_p90": at(0.9),
            "draw_ms_max": draws.last().map(|d| ms(*d)),
            "draw_ms_total": draws.iter().map(|d| ms(*d)).sum::<f64>(),
        })
    );
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
    std::fs::create_dir_all(&state)?;
    let fixture = state.join("fixture");
    let (repos, worktrees, tabs) = (
        env("FORGE_REPOS", 40),
        env("FORGE_WT", 3),
        env("FORGE_TABS", 4),
    );
    gpui_kit::application()
        .with_assets(guild_desktop::icons::Icons)
        .run(move |cx: &mut App| {
            guild_desktop::fonts::register(cx);
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            let mut made = None;
            let window = cx
                .open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                            gpui::point(px(80.), px(60.)),
                            size(px(1400.), px(900.)),
                        ))),
                        titlebar: Some(gpui::TitlebarOptions {
                            title: Some("Guild forge frames probe".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    |window, cx| {
                        let root = driver::root(&fixture, repos, worktrees, tabs, cx);
                        made = Some(root.clone());
                        cx.new(|cx| gpui_component::Root::new(root, window, cx))
                    },
                )
                .unwrap();
            let root = made.unwrap();
            let observer = window
                .update(cx, |_, w, _| guild_desktop::system::window_probe::Observer::new(w).unwrap())
                .unwrap();
            let mut collector = cx.foreground_journal().collector();
            cx.activate(true);
            cx.spawn(async move |cx| {
                cx.background_executor().timer(Duration::from_secs(2)).await;
                let mut clock = Instant::now();
                cx.update(|cx| driver::land_all(&root, clock, cx));
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let _ = collector.collect_unseen();
                println!("{}", serde_json::json!({"fixture": {"repos": repos, "worktrees": worktrees, "tabs": tabs}, "pid": std::process::id()}));
                let phases: [(&str, Option<(Notify, bool)>); 4] = [
                    ("same MRs, notified always (before)", Some((Notify::Always, false))),
                    ("same MRs, notified on a change (after)", Some((Notify::IfChanged, false))),
                    ("changed MR", Some((Notify::IfChanged, true))),
                    ("root notify", None),
                ];
                let mut iid = 1;
                for (label, phase) in phases {
                    let mut draws = Vec::new();
                    let (mut notified, mut occluded) = (0, 0);
                    for i in 0..40 {
                        clock += Duration::from_secs(200);
                        if phase.is_some_and(|(_, changed)| changed) {
                            iid += 1;
                        }
                        let did = cx.update(|cx| match phase {
                            Some((notify, _)) => {
                                driver::land_one(&root, &fixture, i % repos, iid, clock, notify, cx)
                            }
                            None => {
                                driver::notify(&root, cx);
                                true
                            }
                        });
                        notified += usize::from(did);
                        cx.background_executor().timer(Duration::from_millis(120)).await;
                        let sample = observer.sample();
                        if sample.occluded || !sample.visible {
                            occluded += 1;
                        }
                        for entry in collector.collect_unseen().entries {
                            if let Entry::Boundary(Boundary::Presented(v)) = entry
                                && v.frame.window_id == window.window_id()
                            {
                                draws.push(v.frame.draw_duration());
                            }
                        }
                    }
                    summary(label, 40, notified, occluded, draws);
                }
                cx.update(|cx| cx.quit());
            })
            .detach();
        });
    Ok(())
}
