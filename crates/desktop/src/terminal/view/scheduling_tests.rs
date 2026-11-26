use super::*;
use gpui::TestAppContext;

#[gpui::test]
fn quiet_hidden_terminal_has_no_fast_ui_clock(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    let tab = cx.update(|cx| {
        open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap()
    });
    cx.run_until_parked();
    let before = tab.read_with(cx, |tab, _| tab.wake_count);
    for _ in 0..100 {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
    }
    let after = tab.read_with(cx, |tab, _| tab.wake_count);
    assert!(
        after - before <= 3,
        "quiet hidden terminal woke {} times",
        after - before
    );
    drop(tab);
    cx.run_until_parked();
}

#[gpui::test]
fn completed_hidden_terminal_keeps_the_final_unclosed_sync_frame(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "printf '\\033[?2026hFINAL-FRAME'"]);
    let tab = cx.update(|cx| {
        open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap()
    });
    let until = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        if tab.read_with(cx, |tab, _| tab.exited() && tab.has_text()) {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "final synchronized output was lost"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    // Completion now publishes the frame immediately, so the reader's last
    // notification may still be inside the existing 16ms event coalescer.
    // No cursor, foreground or sync deadline may remain; drain only that
    // bounded final event before checking persistent inactivity.
    assert!(tab.read_with(cx, |tab, _| {
        tab.next_deadline(std::time::Instant::now()).is_none()
    }));
    for _ in 0..2 {
        cx.executor()
            .advance_clock(scheduling::FRAME_INTERVAL + Duration::from_millis(1));
        cx.run_until_parked();
    }
    let before = tab.read_with(cx, |tab, _| tab.wake_count);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(
        before,
        tab.read_with(cx, |tab, _| tab.wake_count),
        "completed retained view should sleep"
    );
}

#[gpui::test]
fn unrelated_window_paints_do_not_wake_a_quiet_terminal(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "printf '\\033[?25lready'; sleep 30"]);
    let mut terminal = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let tab = open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap();
        terminal = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    let terminal = terminal.unwrap();
    cx.simulate_resize(gpui::size(px(640.), px(360.)));
    let until = std::time::Instant::now() + Duration::from_secs(5);
    while !terminal.read_with(cx, |tab, _| tab.has_text()) {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    for _ in 0..2 {
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    let before = terminal.read_with(cx, |tab, _| tab.wake_count);
    for _ in 0..20 {
        cx.update(|window, cx| {
            window.refresh();
            window.simulate_next_frame(cx);
        });
        cx.run_until_parked();
    }
    assert_eq!(
        terminal.read_with(cx, |tab, _| tab.wake_count),
        before,
        "unchanged paint woke the terminal event consumer"
    );
}

#[gpui::test]
fn a_burst_of_output_signals_coalesces_ui_updates(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("30");
    let tab = cx.update(|cx| {
        open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap()
    });
    cx.run_until_parked();
    let signal = tab.read_with(cx, |tab, _| tab.emulator.event_signal());
    let before = tab.read_with(cx, |tab, _| tab.wake_count);
    let began = std::time::Instant::now();
    for _ in 0..100 {
        signal.notify_one();
        std::thread::sleep(Duration::from_millis(1));
        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
    }
    let elapsed = began.elapsed().as_millis();
    let updates = tab.read_with(cx, |tab, _| tab.wake_count) - before;
    assert!(
        updates as u128 <= elapsed.div_ceil(16) + 3,
        "{updates} UI updates for a {elapsed}ms event burst; expected the existing 16ms budget"
    );
}

#[gpui::test]
fn intentional_harness_handoff_marks_closed_without_failure(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sleep");
    command.arg("20");
    let tab = cx.update(|cx| {
        open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap()
    });
    let (facts, cleanup) = tab.update(cx, |tab, _| {
        tab.launched_harness = Some(HarnessKind::Codex);
        (tab.facts.clone(), tab.let_go())
    });
    if let Some(cleanup) = cleanup {
        cleanup.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    assert!(
        facts.exited(),
        "an intentional handoff must retain its closed lifecycle fact"
    );
    assert!(facts.failure().is_none());
}

fn late_supervised_completion(cx: &mut TestAppContext, failed: bool) {
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "exit 0"]);
    let mut emulator = Emulator::spawn_job(command, Path::new("/"), GridSize::new(80, 24)).unwrap();
    let publication = emulator.hold_supervised_publication();
    let signal = emulator.event_signal();
    let tab = cx.update(|cx| {
        cx.new(|cx| {
            TerminalTab::new(
                emulator,
                Look::of(&Default::default(), Default::default()),
                PathBuf::from("/"),
                None,
                cx,
            )
        })
    });
    let until = std::time::Instant::now() + Duration::from_secs(4);
    while tab.read_with(cx, |tab, _| tab.reader_exit_count) == 0 {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
    }
    assert!(
        !tab.read_with(cx, |tab, _| tab.facts.exited()),
        "EOF cannot publish final process facts before supervisor result"
    );
    tab.update(cx, |tab, _| {
        tab.caret.visible(std::time::Instant::now(), true);
    });
    *publication.lock().unwrap() = Some(if failed {
        Err("controlled cleanup failure".into())
    } else {
        Ok(portable_pty::ExitStatus::with_exit_code(0))
    });
    signal.notify_one();
    for _ in 0..4 {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
    }
    assert!(
        tab.read_with(cx, |tab, _| tab.facts.exited()),
        "late supervisor result must independently publish exited facts (failed={failed})"
    );
    assert!(
        tab.read_with(cx, |tab, _| tab
            .next_deadline(std::time::Instant::now())
            .is_none()),
        "completed view must retain neither cursor nor foreground clock"
    );
    let before = tab.read_with(cx, |tab, _| tab.wake_count);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(before, tab.read_with(cx, |tab, _| tab.wake_count));
}
#[gpui::test]
fn supervised_completion_after_reader_exit(cx: &mut TestAppContext) {
    late_supervised_completion(cx, false);
}
#[gpui::test]
fn supervised_failure_after_reader_exit(cx: &mut TestAppContext) {
    late_supervised_completion(cx, true);
}

#[gpui::test]
fn fast_nonzero_exit_classifies_the_final_unclosed_sync_frame(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let mut command = CommandBuilder::new("/bin/sh");
    command.args(["-c", "printf '\\033[?2026hDNS lookup failed'; exit 7"]);
    let tab = cx.update(|cx| {
        let tab = open_command(
            command,
            Path::new("/"),
            Look::of(&Default::default(), Default::default()),
            None,
            cx,
        )
        .unwrap();
        tab.update(cx, |tab, _| tab.launched_harness = Some(HarnessKind::Codex));
        tab
    });
    let until = std::time::Instant::now() + Duration::from_secs(3);
    while !tab.read_with(cx, |tab, _| tab.facts.exited()) {
        assert!(std::time::Instant::now() < until);
        std::thread::sleep(Duration::from_millis(1));
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
    }
    let (failure, text_at_exit, deadline) = tab.read_with(cx, |tab, _| {
        (
            tab.facts.failure().unwrap(),
            tab.has_text(),
            tab.emulator.sync_deadline(),
        )
    });
    // Pin the fast-exit ordering, rather than accidentally testing the stale
    // timeout path on an overloaded machine. After the fix no frame remains.
    if let Some(deadline) = deadline {
        assert!(
            deadline > std::time::Instant::now(),
            "fixture missed the 150ms frame window"
        );
        std::thread::sleep(
            deadline.saturating_duration_since(std::time::Instant::now())
                + Duration::from_millis(5),
        );
        cx.executor().advance_clock(Duration::from_millis(200));
        cx.run_until_parked();
    }
    let final_text = tab.read_with(cx, |tab, _| tab.has_text());
    assert_eq!(failure.code, 7);
    assert!(
        final_text,
        "final synchronized diagnostic must remain selectable"
    );
    assert!(
        failure.network && text_at_exit,
        "classification raced final frame: network={}, text_at_exit={text_at_exit}, text_after_timeout={final_text}",
        failure.network
    );
    assert!(tab.read_with(cx, |tab, _| tab.emulator.sync_deadline().is_none()));
}

#[gpui::test]
fn retained_terminal_count_matrix_keeps_final_output_and_sleeps(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    for adaptive in [false, true] {
        let mut look = Look::of(&Default::default(), Default::default());
        look.adaptive_terminal_refresh = adaptive;
        for count in [1, 10, 30] {
            let mut quiet = Vec::new();
            for _ in 0..count {
                let mut command = CommandBuilder::new("/bin/sleep");
                command.arg("30");
                quiet.push(cx.update(|cx| {
                    open_command(command, Path::new("/"), look.clone(), None, cx).unwrap()
                }));
            }
            cx.run_until_parked();
            let before: Vec<_> = quiet
                .iter()
                .map(|t| t.read_with(cx, |tab, _| tab.wake_count))
                .collect();
            for _ in 0..100 {
                cx.executor().advance_clock(Duration::from_millis(20));
                cx.run_until_parked();
            }
            for (terminal, before) in quiet.iter().zip(before) {
                assert!(
                    terminal.read_with(cx, |tab, _| tab.wake_count) - before <= 3,
                    "quiet hidden clock, count {count}, adaptive {adaptive}"
                );
            }
            drop(quiet);
            cx.run_until_parked();
            let mut terminals = Vec::new();
            for _ in 0..count {
                let mut command = CommandBuilder::new("/bin/sh");
                command.args(["-c","i=0; while [ $i -lt 200 ]; do printf 'ROW-%03d\\r\\n' \"$i\"; i=$((i+1)); done; printf '\\033[?2026hFINAL-200'"]);
                terminals.push(cx.update(|cx| {
                    open_command(command, Path::new("/"), look.clone(), None, cx).unwrap()
                }));
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(15);
            loop {
                cx.executor().advance_clock(Duration::from_millis(20));
                cx.run_until_parked();
                if terminals
                    .iter()
                    .all(|t| t.read_with(cx, |tab, _| tab.exited()))
                {
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "{count} retained terminals failed to complete"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
            for terminal in &terminals {
                terminal.read_with(cx, |tab, _| {
                    use alacritty_terminal::grid::Dimensions;
                    use alacritty_terminal::index::Line;
                    let term = tab.emulator.term().lock().unwrap();
                    let mut text = String::new();
                    for line in -(term.history_size() as i32)..term.screen_lines() as i32 {
                        for column in 0..term.columns() {
                            text.push(
                                term.grid()[Line(line)][alacritty_terminal::index::Column(column)]
                                    .c,
                            );
                        }
                        text.push('\n');
                    }
                    for index in 0..200 {
                        assert!(
                            text.contains(&format!("ROW-{index:03}")),
                            "missing row {index}, count {count}"
                        );
                    }
                    assert!(
                        text.contains("FINAL-200"),
                        "unclosed final synchronized frame was lost"
                    );
                });
            }
            for _ in 0..3 {
                cx.executor().advance_clock(Duration::from_millis(20));
                cx.run_until_parked();
            }
            let wakes: Vec<_> = terminals
                .iter()
                .map(|t| t.read_with(cx, |tab, _| tab.wake_count))
                .collect();
            cx.executor().advance_clock(Duration::from_secs(5));
            cx.run_until_parked();
            for (terminal, before) in terminals.iter().zip(wakes) {
                assert_eq!(
                    terminal.read_with(cx, |tab, _| tab.wake_count),
                    before,
                    "retained terminal has a hidden redraw loop, count {count}"
                );
            }
            drop(terminals);
            cx.run_until_parked();
        }
    }
}
