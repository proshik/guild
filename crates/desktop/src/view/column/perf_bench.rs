//! Measurement only: what a frame of the column costs with many projects.
//! Not a contract — it asserts nothing about the numbers, and is ignored.
//!
//! ```sh
//! BENCH_REPOS=40 BENCH_WT=3 BENCH_TABS=4 \
//!   cargo test --release -p guild-desktop --lib view::column::perf_bench \
//!   -- --ignored --nocapture
//! ```
//!
//! The production `Root` in a 1400×900 test window: `BENCH_REPOS`
//! repositories (12), each with `BENCH_WT` opened worktrees (3) and two
//! unopened, `BENCH_TABS` session tabs on every opened one (3), the first
//! `BENCH_WORKING` of them working (3); with `BENCH_EXPANDED=1` every
//! worktree's list of tabs is open under its row. It prints the median, p90 and max of
//! a frame after a notify of the root, a wheel step over the column, a
//! notify of the column's region alone — what a working ring's tick is —
//! and of building the column's elements without laying them out.
//!
//! **A test window draws itself** at the end of `flush_effects`, so each
//! sample is a whole event-to-frame cycle. The test platform is slower than
//! a window on a display: compare runs with each other, before and after a
//! change, never with a frame budget.
//!
//! **It runs in a child process with state of its own**: a production
//! `Root` saves its settings as it is made, and a `GUILD_STATE_DIR` that is
//! live — or empty, which falls back to the real one — would have the
//! fixture's settings written over the person's. The ignored test spawns
//! this binary for the measuring test with a fresh directory and prints
//! what it printed; the measuring test refuses to run anywhere else.
use super::*;
use gpui::{AppContext, TestAppContext};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};
use std::time::{Duration, Instant};

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn stats(label: &str, mut v: Vec<Duration>) {
    v.sort();
    let ms = |d: Duration| d.as_secs_f64() * 1000.;
    let n = v.len();
    let mean = v.iter().map(|d| ms(*d)).sum::<f64>() / n as f64;
    println!(
        "{label:<28} n={n:3} median={:7.2}ms p90={:7.2}ms max={:7.2}ms mean={:7.2}ms",
        ms(v[n / 2]),
        ms(v[n * 9 / 10]),
        ms(v[n - 1]),
        mean
    );
}

/// The marker the child is given: the state directory its parent made.
const CHILD_STATE: &str = "GUILD_PERF_BENCH_STATE";

#[test]
#[ignore = "measurement"]
fn column_frame_cost_at_scale() {
    let state = tempfile::tempdir().unwrap();
    let name = concat!(module_path!(), "::measure_column_frames");
    let name = name.strip_prefix("guild_desktop::").unwrap_or(name);
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            name,
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env(CHILD_STATE, state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // The harness prints the child's first line after its own `test … `.
    for line in stdout.lines() {
        if let Some(at) = line.find("fixture:") {
            println!("{}", &line[at..]);
        } else if line.contains(" median=") || line.starts_with("scroll offset") {
            println!("{line}");
        }
    }
    assert!(
        output.status.success() && stdout.contains("1 passed; 0 failed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "run through column_frame_cost_at_scale"]
fn measure_column_frames(cx: &mut TestAppContext) {
    let Some(state) = std::env::var_os(CHILD_STATE) else {
        // Matched by a filter in the parent's own process: not isolated.
        return;
    };
    let state = std::path::PathBuf::from(state);
    assert_eq!(crate::paths::settings_file(), state.join("settings.json"));
    assert_eq!(crate::paths::workspace_file(), state.join("workspace.json"));
    let expanded = env("BENCH_EXPANDED", 0) != 0;
    let repos = env("BENCH_REPOS", 12);
    let worktrees = env("BENCH_WT", 3);
    let tabs = env("BENCH_TABS", 3);
    let working = env("BENCH_WORKING", 3);
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let mut lists = Vec::new();
        let mut seats = Vec::new();
        for r in 0..repos {
            let main = dir.path().join(format!("repository-{r}"));
            let common = main.join(".git");
            let mut porcelain = String::new();
            for w in 0..worktrees + 2 {
                let path = if w == 0 {
                    main.clone()
                } else {
                    dir.path().join(format!("repository-{r}-feature-branch-{w}"))
                };
                std::fs::create_dir_all(&path).unwrap();
                let branch = if w == 0 {
                    "main".to_string()
                } else {
                    format!("feature/long-branch-name-{w}")
                };
                porcelain.push_str(&format!(
                    "worktree {}\0HEAD 0123456789012345678901234567890123456789\0branch refs/heads/{branch}\0\0",
                    path.display()
                ));
                // The last two are listed by git but not opened.
                if w < worktrees {
                    let project = desk.open_in(workspace, &path);
                    desk.set_repository(project, Some(common.clone()));
                    if expanded {
                        desk.toggle_worktree_tabs(workspace, &path);
                    }
                    seats.push(Seat { workspace, project });
                }
            }
            let mut list = crate::git::worktrees::parse(porcelain.as_bytes()).unwrap();
            for (i, w) in list.iter_mut().enumerate() {
                w.committed = Some(1_700_000_000 + i as i64);
                w.lines = Some((120 + i as u32, 30));
            }
            lists.push((common, list));
        }
        desk.select_workspace(workspace);
        let mut sessions = Vec::new();
        for seat in &seats {
            for t in 0..tabs {
                let id = desk.add_session(SessionRecord::new(
                    format!("Agent session {t}"),
                    *seat,
                    dir.path().into(),
                    Runs::Claude(SessionMode::Terminal),
                ));
                sessions.push((*seat, id));
            }
        }
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.isolated_render = true;
            for (common, list) in lists {
                root.worktrees.insert(common, list);
            }
            root.active_workspace = workspace;
            root.restored_workspaces.insert(workspace);
            for (i, (seat, id)) in sessions.iter().enumerate() {
                root.adopt(
                    *seat,
                    seat.project,
                    crate::Body::Broken {
                        session: *id,
                        reason: "Fixture without a provider".into(),
                    },
                    cx,
                );
                use guild_session::status::Reported;
                let report = if i < working {
                    Some(Reported::Working)
                } else {
                    [
                        None,
                        Some(Reported::Done),
                        Some(Reported::Blocked(None)),
                        None,
                    ][i % 4]
                };
                root.board.report(*id, report);
            }
            root.active_project = seats.first().map(|s| s.project);
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    cx.run_until_parked();
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let start = Instant::now();
            window.draw(cx).clear(cx);
            start.elapsed()
        })
    };
    for _ in 0..5 {
        cx.update(|window, _| window.refresh());
        draw(cx);
    }
    let rows: usize = root.read_with(cx, |r, _| {
        r.column_groups_for(r.active_workspace)
            .iter()
            .map(|g| g.rows.len())
            .sum()
    });
    println!(
        "fixture: repos={repos} rows={rows} session tabs={} working={working} expanded={expanded}",
        repos * worktrees * tabs
    );

    // A notify of the root: the root renders, and so does every region,
    // since each observes the root — the column rebuilt with it. Not a
    // frame of cache replay.
    let mut idle = Vec::new();
    for _ in 0..40 {
        // A test window draws itself at the end of the update's effects.
        let start = Instant::now();
        root.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        idle.push(start.elapsed());
    }
    stats("root notify (all regions)", idle);

    // Scrolling the column: GPUI notifies the region, and every view above.
    let column = cx.update(|_, _| gpui::point(px(120.), px(450.)));
    let mut scroll = Vec::new();
    for i in 0..60 {
        let dy = if (i / 15) % 2 == 0 { -24. } else { 24. };
        let start = Instant::now();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: column,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(dy))),
            modifiers: gpui::Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        scroll.push(start.elapsed());
    }
    let offset = root.read_with(cx, |r, _| {
        r.project_drag.scroll(r.active_workspace).offset()
    });
    let max = root.read_with(cx, |r, _| {
        r.project_drag.scroll(r.active_workspace).max_offset()
    });
    println!("scroll offset after: {offset:?} max {max:?}");
    stats("column scroll frame", scroll);

    // The working ring's clock: a notify of the column region alone.
    let region = root.read_with(cx, |r, _| {
        r.regions[&crate::view::region::Part::Column].clone()
    });
    let mut tick = Vec::new();
    for _ in 0..40 {
        let start = Instant::now();
        region.update(cx, |_, cx| cx.notify());
        cx.run_until_parked();
        tick.push(start.elapsed());
    }
    stats("column region notify", tick);

    // Building the column's elements alone, no layout.
    let mut build = Vec::new();
    for _ in 0..40 {
        build.push(root.update(cx, |root, cx| {
            let start = Instant::now();
            let element = root.render_column(cx);
            let took = start.elapsed();
            drop(element);
            took
        }));
    }
    stats("render_column build only", build);
}
