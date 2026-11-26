//! Feature-gated counters and isolated production-terminal fixtures.
use crate::terminal::view::{self, TerminalTab};
use gpui::{Context, Entity, Render, Window, div, prelude::*};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use std::time::Instant;

pub static OUTPUT_NOTIFIES: AtomicU64 = AtomicU64::new(0);
pub static OUTPUT_WAKES: AtomicU64 = AtomicU64::new(0);
pub static CURRENT_HANDOFFS: AtomicU64 = AtomicU64::new(0);
pub static POLLS: AtomicU64 = AtomicU64::new(0);
pub static PAINTS: AtomicU64 = AtomicU64::new(0);
pub static ROWS: AtomicU64 = AtomicU64::new(0);
static LATENCY_ENABLED: AtomicBool = AtomicBool::new(false);
static LATENCY: Mutex<Option<(usize, Instant)>> = Mutex::new(None);
static EXPECTED_BYTES: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static FIRST_BYTE: Mutex<Option<Instant>> = Mutex::new(None);
static FINAL_FRAME: AtomicBool = AtomicBool::new(false);

pub static GRID_HOLD_NS: AtomicU64 = AtomicU64::new(0);
pub static GRID_WAIT_NS: AtomicU64 = AtomicU64::new(0);
pub fn grid_held(duration: Duration) {
    GRID_HOLD_NS.fetch_max(duration.as_nanos() as u64, Ordering::Relaxed);
}
pub fn grid_waited(duration: Duration) {
    GRID_WAIT_NS.fetch_max(duration.as_nanos() as u64, Ordering::Relaxed);
}
pub fn wants_generations() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("GUILD_PROBE_MODE").as_deref() == Ok("generations"))
}
pub fn wants_text() -> bool {
    if wants_generations() {
        return true;
    }
    (LATENCY_ENABLED.load(Ordering::Relaxed) && LATENCY.lock().unwrap().is_some())
        || (EXPECTED_BYTES.load(Ordering::Relaxed) > 0 && !FINAL_FRAME.load(Ordering::Relaxed))
}

pub fn parsed_bytes(count: usize) {
    let expected = EXPECTED_BYTES.load(Ordering::Relaxed);
    if expected == 0 {
        return;
    }
    let total = BYTES.fetch_add(count as u64, Ordering::Relaxed) + count as u64;
    let mut first = FIRST_BYTE.lock().unwrap();
    if total == count as u64 {
        *first = Some(Instant::now());
    }
    if total >= expected
        && let Some(start) = first.take()
    {
        println!(
            "{{\"parsed_bytes\":{total},\"expected_bytes\":{expected},\"parse_seconds\":{:.6}}}",
            start.elapsed().as_secs_f64()
        );
    }
}

/// Invoked under the existing grid guard, only in this diagnostic feature.
pub fn painted_text(text: impl FnOnce() -> String) {
    if LATENCY_ENABLED.load(Ordering::Relaxed) {
        let mut pending = LATENCY.lock().unwrap();
        if let Some((seq, began)) = *pending
            && text().contains(&format!("ACK {seq:06}"))
        {
            println!(
                "{{\"input_sequence\":{seq},\"input_to_paint_ms\":{:.3}}}",
                began.elapsed().as_secs_f64() * 1000.
            );
            *pending = None;
        }
    } else if EXPECTED_BYTES.load(Ordering::Relaxed) > 0
        && !FINAL_FRAME.load(Ordering::Relaxed)
        && text().contains("END 019999")
    {
        FINAL_FRAME.store(true, Ordering::Relaxed);
        println!("{{\"final_grid_marker\":true}}");
    }
}

pub struct ResourceProbe {
    tabs: Vec<Entity<TerminalTab>>,
    visible: bool,
    _quitting: gpui::Subscription,
}
impl ResourceProbe {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Coalesced observations, not a lossless transition history.
        // This is native visibility only, never a presentation timestamp.
        let visibility_started = Instant::now();
        println!(
            "{{\"observed_visibility_seconds\":0,\"visible\":{}}}",
            window.is_visible()
        );
        window
            .observe_window_visibility(move |visibility, _, _| {
                println!(
                    "{{\"observed_visibility_seconds\":{:.6},\"visible\":{}}}",
                    visibility_started.elapsed().as_secs_f64(),
                    visibility.is_visible()
                );
            })
            .detach();
        let count: usize = std::env::var("GUILD_PROBE_COUNT")
            .unwrap_or("1".into())
            .parse()
            .unwrap();
        assert!((1..=20).contains(&count));
        let mode = std::env::var("GUILD_PROBE_MODE").unwrap_or("quiet".into());
        assert!(
            [
                "quiet",
                "hidden",
                "output",
                "completed",
                "cycles",
                "one-row",
                "latency",
                "flood",
                "generations",
                "stripe",
                "stripe-ack",
                "bounded-input"
            ]
            .contains(&mode.as_str())
        );
        let seconds: u64 = std::env::var("GUILD_PROBE_SECONDS")
            .unwrap_or("75".into())
            .parse()
            .unwrap();
        assert!((5..=300).contains(&seconds));
        LATENCY_ENABLED.store(mode == "latency", Ordering::Relaxed);
        if mode == "flood" {
            let bytes = "\x1b[?25l".len()
                + 20_000 * "row 000000: throughput fixture\r\n".len()
                + "END 019999\r\n".len();
            EXPECTED_BYTES.store(bytes as u64, Ordering::Relaxed);
        }
        if mode == "latency" {
            window.spawn(cx, async move |cx| {
                cx.background_executor().timer(Duration::from_secs(15)).await;
                for seq in 1..=120 {
                    let _ = cx.update(|window, cx| {
                        // A framed sequence tolerates incidental physical input
                        // while the authorized test window is temporarily in front.
                        window.dispatch_keystroke(gpui::Keystroke::parse("escape").unwrap(), cx);
                        for digit in format!("{seq:06}").chars() {
                            window.dispatch_keystroke(gpui::Keystroke::parse(&digit.to_string()).unwrap(), cx);
                        }
                        let mut pending = LATENCY.lock().unwrap();
                        if let Some((previous, _)) = pending.take() {
                            println!("{{\"missed_input_sequence\":{previous}}}");
                        }
                        *pending = Some((seq, Instant::now()));
                        drop(pending);
                        let handled = window.dispatch_keystroke(gpui::Keystroke::parse("enter").unwrap(), cx);
                        println!("{{\"sent_sequence\":{seq},\"dispatch_handled\":{handled},\"visible\":{}}}", window.is_visible());
                    });
                    cx.background_executor().timer(Duration::from_millis(500)).await;
                }
            }).detach();
        }
        let mut tabs = Vec::new();
        if mode != "cycles" {
            for _ in 0..count {
                tabs.push(terminal(&mode, cx));
            }
        } else {
            cx.spawn(async move |view, cx| {
                let cycle_started = std::time::Instant::now();
                for cycle in 1..=25 {
                    let Ok(weak) = view.update(cx, |view, cx| {
                        let tab = terminal("output", cx);
                        let weak = tab.downgrade();
                        view.tabs.push(tab);
                        cx.notify();
                        weak
                    }) else {
                        return;
                    };
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    let _ = view.update(cx, |view, cx| {
                        for tab in &view.tabs {
                            tab.update(cx, |tab, _| tab.cancel_job());
                        }
                        view.tabs.clear();
                        cx.notify();
                    });
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    println!(
                        "{{\"cycle\":{cycle},\"cycle_elapsed\":{:.3},\"retained_views\":{}}}",
                        cycle_started.elapsed().as_secs_f64(),
                        usize::from(weak.upgrade().is_some())
                    );
                }
            })
            .detach();
        }
        window.spawn(cx, async move |cx| {
            let started = visibility_started;
            for tick in 1..=seconds / 5 {
                cx.background_executor().timer(Duration::from_secs(5)).await;
                let _ = cx.update(|window, _| {
                    println!("{{\"elapsed\":{:.3},\"polls\":{},\"paints\":{},\"rows\":{},\"visible\":{},\"active\":{}}}",
                        started.elapsed().as_secs_f64(), POLLS.load(Ordering::Relaxed),
                        PAINTS.load(Ordering::Relaxed), ROWS.load(Ordering::Relaxed),
                        window.is_visible(), window.is_window_active());
                    println!("{{\"max_grid_hold_ms\":{:.6},\"max_grid_wait_ms\":{:.6}}}", GRID_HOLD_NS.load(Ordering::Relaxed) as f64 / 1e6, GRID_WAIT_NS.load(Ordering::Relaxed) as f64 / 1e6);
                });
                if tick == seconds / 5 { break; }
            }
        }).detach();
        cx.spawn(async move |view, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            let _ = view.update(cx, |view, cx| {
                for tab in &view.tabs {
                    tab.update(cx, |tab, _| tab.cancel_job());
                }
                view.tabs.clear();
                cx.notify();
            });
            cx.background_executor().timer(Duration::from_secs(2)).await;
            cx.update(|cx| cx.quit());
        })
        .detach();
        let quitting = cx.on_app_quit(|probe, cx| {
            for tab in &probe.tabs {
                tab.update(cx, |tab, cx| tab.hang_up_for_quit(cx));
            }
            Box::pin(crate::transport::finish_closing())
        });
        Self {
            tabs,
            visible: mode != "hidden",
            _quitting: quitting,
        }
    }
}
impl Render for ResourceProbe {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(feature = "adaptive-terminal-refresh")]
        for (index, tab) in self.tabs.iter().enumerate() {
            tab.update(_cx, |tab, cx| {
                tab.set_surface_visible(self.visible && index == 0, _window, cx)
            });
        }
        div()
            .size_full()
            .children(self.visible.then(|| self.tabs.first().cloned()).flatten())
    }
}

fn saved_fixture_settings(path: &std::path::Path) -> guild_session::Settings {
    guild_session::Store::new(path.to_path_buf())
        .load()
        .unwrap_or_default()
}

pub(super) fn fixture_settings() -> guild_session::Settings {
    if std::env::var("GUILD_PROBE_SCENARIO").as_deref() == Ok("settings") {
        return saved_fixture_settings(&crate::paths::settings_file());
    }
    #[allow(unused_mut)]
    let mut settings = guild_session::Settings::default();
    // BEGIN adaptive fixture override (not copied into historical A sources).
    #[cfg(feature = "adaptive-terminal-refresh")]
    {
        settings.adaptive_terminal_refresh =
            std::env::var("GUILD_PROBE_REFRESH").as_deref() == Ok("adaptive");
    }
    // END adaptive fixture override.
    settings
}

pub(super) fn terminal(mode: &str, cx: &mut gpui::App) -> Entity<TerminalTab> {
    if mode == "bounded-input" {
        let state = crate::paths::state_dir();
        assert!(state.is_absolute());
        std::fs::create_dir_all(&state).unwrap();
        std::fs::write(
            state.join(".owned-runtime-input"),
            "Guild owned transport input\n",
        )
        .unwrap();
    }
    let mut command = portable_pty::CommandBuilder::new("/usr/bin/python3");
    command.arg("-u");
    command.arg("-c");
    command.arg(match mode {
                "stripe" | "stripe-ack" | "hidden-output" => include_str!("../../../../scripts/terminal-refresh/producer.py"),
                "generations" => "import time,sys\nprint('\\x1b[?25l\\x1b[2J',end='')\nfor i in range(200):\n sys.stdout.write('\\x1b[1;1HOwned policy probe generation %06d\\x1b[K' % i);sys.stdout.flush();time.sleep(.1)\ntime.sleep(10)",
                "output" => "import time,sys\nprint('\\x1b[?25l',end='')\nfor i in range(15000):\n print('row %06d: terminal output fixture' % i,flush=True); time.sleep(.01)",
                "one-row" => "import time,sys\ntime.sleep(2)\nprint('\\x1b[?25l\\x1b[2J',end='')\nfor i in range(40): print('static %02d: Ж é 界 🙂 \\x1b[31mred\\x1b[0m' % i)\nfor i in range(15000):\n sys.stdout.write('\\x1b[1;1Hrow %06d: one changed row\\x1b[K' % i);sys.stdout.flush();time.sleep(.01)",
                "bounded-input" => include_str!("../../../../test-support/terminal_transport_fixture.py"),
                "latency" => "import os,tty,select,sys,time\ntty.setraw(0)\nsys.stdout.write('\\x1b[?25l\\x1b[2J');sys.stdout.flush()\ni=0;line=''\nwhile True:\n ready,_,_=select.select([0],[],[],.01)\n if ready:\n  for c in os.read(0,1024):\n   if c==27: line=''\n   elif c in (10,13):\n    if len(line)==6 and line.isdigit(): sys.stdout.write('\\x1b[2;1HACK '+line+'\\x1b[K')\n    line=''\n   else: line+=chr(c)\n i+=1;sys.stdout.write('\\x1b[1;1Houtput %06d\\x1b[K' % i);sys.stdout.flush()",
                "flood" => "import time,sys,tty\ntty.setraw(1)\ntime.sleep(15)\nsys.stdout.write('\\x1b[?25l'+''.join('row %06d: throughput fixture\\r\\n' % i for i in range(20000))+'END 019999\\r\\n');sys.stdout.flush();time.sleep(360)",
                "completed" => "print('completed terminal fixture')",
                _ => "import time\nprint('\\x1b[?25lquiet terminal fixture',flush=True)\ntime.sleep(360)",
            });
    let mut look = view::Look::of(&fixture_settings(), Default::default());
    // Synthetic selection trials must never replace the user clipboard.
    look.copy_on_select = false;
    view::open_job(command, std::path::Path::new("/"), look, cx).unwrap()
}

#[cfg(test)]
mod settings_restart_tests {
    #[test]
    fn settings_fixture_reads_saved_preference_without_reinjecting_policy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert!(!super::saved_fixture_settings(&path).adaptive_terminal_refresh);
        let settings = guild_session::Settings {
            adaptive_terminal_refresh: true,
            ..Default::default()
        };
        guild_session::Store::new(path.clone())
            .save(&settings)
            .unwrap();
        assert!(super::saved_fixture_settings(&path).adaptive_terminal_refresh);
        guild_session::Store::new(path.clone())
            .save(&guild_session::Settings::default())
            .unwrap();
        assert!(!super::saved_fixture_settings(&path).adaptive_terminal_refresh);
    }
}
