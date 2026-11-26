use super::*;
use gpui::{AppContext, TestAppContext};

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn restored_task_controls_refresh_imports_without_output(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    std::fs::write(
        dir.path().join("Makefile"),
        "include common.mk\nroot:\n\t@echo root\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("common.mk"), "# initially independent\n").unwrap();
    std::fs::write(
        dir.path().join("nested/Makefile"),
        "child:\n\t@echo child\n",
    )
    .unwrap();
    let mut saved = guild_session::project_state::ProjectState {
        tabs: vec![guild_session::OpenTab::showing(guild_session::Shows::File(
            "nested/Makefile".into(),
        ))],
        active: Some(0),
        ..Default::default()
    };
    saved.panels.bottom.open = false;
    guild_session::project_state::save(dir.path(), &saved).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            assert!(root.declared.is_empty());
            // Exercise saved tab restoration through root/tabs, with no
            // open_file, Output, chooser or manual catalogue initialization.
            root.restore_tabs(cx);
            assert!(!root.panels().bottom.open);
            assert!(root.picker.is_none());
            assert!(root.runs.is_empty());
            assert_eq!(root.tabs.len(), 1);
            let crate::Body::File { tab: file, .. } = root.tabs[0].body().unwrap() else {
                panic!("restored source expected")
            };
            assert!(
                file.read(cx).editor().is_none(),
                "editor setup stays deferred until layout"
            );
            made = Some((cx.entity(), file.clone()));
            root
        });
        gpui_component::Root::new(root, window, cx)
    });
    let (root, file) = made.unwrap();
    cx.run_until_parked();
    for _ in 0..2 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    assert!(
        cx.debug_bounds("editor-task-control").is_some(),
        "the restored source initially paints its launch button"
    );
    let editor = cx.update(|_, cx| file.read(cx).editor().unwrap().entity_id());
    for (contents, available) in [
        (Some("include nested/Makefile\n"), false),
        (None, false),
        (Some("# independent again\n"), true),
    ] {
        match contents {
            Some(text) => std::fs::write(dir.path().join("common.mk"), text).unwrap(),
            None => std::fs::remove_file(dir.path().join("common.mk")).unwrap(),
        }
        let mut batch = Batch::new(&ignore::gitignore::Gitignore::empty());
        batch.push(std::path::Path::new("common.mk"));
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.rediscover(root.active_project.unwrap(), &batch, cx)
            })
        });
        cx.run_until_parked();
        for _ in 0..2 {
            cx.update(|window, cx| window.simulate_next_frame(cx));
            cx.run_until_parked();
        }
        assert_eq!(
            cx.debug_bounds("editor-task-control").is_some(),
            available,
            "the real restored launch control refreshes after {contents:?}"
        );
        cx.update(|_, cx| {
            assert_eq!(file.read(cx).can_run_file(), available);
            assert_eq!(file.read(cx).editor().unwrap().entity_id(), editor);
            assert!(root.read(cx).runs.is_empty());
            assert!(root.read(cx).picker.is_none());
            assert!(!root.read(cx).panels().bottom.open);
        });
    }
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn imported_file_changes_appearances_and_deletions_invalidate_the_cached_catalogue(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("nested")).unwrap();
    std::fs::write(
        dir.path().join("Makefile"),
        "include common.mk\nroot:\n\t@echo root\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("common.mk"), "# initially empty\n").unwrap();
    std::fs::write(
        dir.path().join("nested/Makefile"),
        "child:\n\t@echo child\n",
    )
    .unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            assert!(root.declared.is_empty());
            root.open_file(std::path::Path::new("nested/Makefile"), window, cx);
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = made.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let file = root
            .read(cx)
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                crate::Body::File { tab, .. } => Some(tab),
                _ => None,
            })
            .unwrap();
        assert!(file.read(cx).can_run_file());
    });
    for (contents, child_available) in [
        (Some("include nested/Makefile\n"), false),
        (None, false),
        (Some("# appeared again\n"), true),
    ] {
        match contents {
            Some(text) => std::fs::write(dir.path().join("common.mk"), text).unwrap(),
            None => std::fs::remove_file(dir.path().join("common.mk")).unwrap(),
        }
        let mut batch = Batch::new(&ignore::gitignore::Gitignore::empty());
        batch.push(std::path::Path::new("common.mk"));
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.rediscover(root.active_project.unwrap(), &batch, cx)
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let file = root
                .read(cx)
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|body| match body {
                    crate::Body::File { tab, .. } => Some(tab),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                file.read(cx).can_run_file(),
                child_available,
                "the open editor refreshes imports without opening Output after {contents:?}"
            );
            assert_eq!(
                root.read(cx)
                    .declared_tasks(root.read(cx).active_project.unwrap())
                    .iter()
                    .any(|t| t.identity.name == "child"),
                child_available,
                "cached catalogue after {contents:?}"
            )
        });
    }
}

#[test]
fn isolated_run_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["root::run::tests::", "--ignored", "--test-threads=1"])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_RUN_WINDOW_TEST", "1")
        .env("SHELL", "/bin/sh")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn source_titles_are_compact_without_changing_the_captured_invocation(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let line = crate::run::source::shell_line(&[
        "just".into(),
        "--justfile".into(),
        "nested/justfile".into(),
        "--".into(),
        "greet".into(),
        "a long captured argument ".repeat(12),
    ]);
    let invocation = run::tabs::Invocation {
        cwd: dir.path().to_owned(),
        line: line.clone(),
        task: Some(crate::run::source::Identity {
            source: "nested/justfile".into(),
            runner: crate::run::source::Runner::Just,
            name: "greet".into(),
        }),
    };
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.reveal_tool(guild_session::Tool::Output, cx);
            root.start_invocation(project, invocation.clone(), cx);
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    cx.run_until_parked();
    let bounds = cx.debug_bounds("selected-run-tab").unwrap();
    cx.update(|_, cx| {
        made.unwrap().update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let entry = root.runs[&project].active().unwrap();
            let id = entry.id;
            assert_eq!(entry.invocation, invocation);
            assert_eq!(
                entry.value.line, line,
                "diagnostic command remains complete"
            );
            root.rerun(cx);
            assert_eq!(root.runs[&project].active().unwrap().id, id);
            root.stop_every_process(cx);
        })
    });
    assert!(
        bounds.size.width < gpui::px(200.),
        "source title must fit a short runner/task label: {bounds:?}"
    );
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn missing_runner_retains_a_failed_exit_and_diagnostics(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let root = cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.start_run(
                crate::run::source::shell_line(&[dir
                    .path()
                    .join("missing-task-runner")
                    .to_string_lossy()
                    .into_owned()]),
                cx,
            );
            root
        })
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        cx.run_until_parked();
        let over = cx.update(|cx| {
            root.update(cx, |root, cx| {
                root.poll_runs(cx);
                root.active_run().unwrap().state.is_over()
            })
        });
        if over {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.update(|cx| {
        root.update(cx, |root, cx| {
            assert_eq!(
                root.active_run().unwrap().state,
                run::State::Finished { code: 127 }
            );
            assert!(
                root.active_run()
                    .unwrap()
                    .view
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .has_text(),
                "shell failure is retained in the real terminal"
            );
            root.stop_every_process(cx);
        })
    });
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn starting_a_second_run_preserves_the_first_process(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    assert_eq!(std::env::var("GUILD_RUN_WINDOW_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let (root, project, first, first_id, second_id) = cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut r = Root::from_state(Default::default(), desk, cx);
            r.active_project = Some(project);
            r.isolated_render = true;
            r
        });
        let (first, first_id, second_id) = root.update(cx, |root, cx| {
            root.start_run("printf first; sleep 30".into(), cx);
            let first = root.active_run().unwrap().view.clone().unwrap();
            root.start_run("printf second; sleep 30".into(), cx);
            assert_eq!(
                root.terminals().len(),
                2,
                "each run owns a retained terminal"
            );
            assert!(
                first.update(cx, |v, _| v.exit_status()).is_none(),
                "another invocation killed the first"
            );
            let tabs = root.runs.get(&project).unwrap();
            let first_id = tabs.entries()[0].id;
            let second_id = tabs.active().unwrap().id;
            root.select_run(project, first_id, cx);
            root.stop_run(cx);
            assert_eq!(root.active_run().unwrap().state, run::State::Stopped);
            assert_eq!(
                root.runs
                    .get(&project)
                    .unwrap()
                    .get(second_id)
                    .unwrap()
                    .value
                    .state,
                run::State::Running
            );
            root.rerun(cx);
            // While cleanup is pending, the old view still owns this invocation.
            if first.read(cx).job_completion().is_none() {
                assert_eq!(root.active_run().unwrap().view.as_ref(), Some(&first));
            }
            root.stop_run(cx); // Stop must revoke an asynchronously pending rerun.
            (first, first_id, second_id)
        });
        (root, project, first, first_id, second_id)
    });
    await_run_cleanup(cx, &first);
    cx.run_until_parked();
    let second = cx.update(|cx| {
        root.update(cx, |root, cx| {
            assert_eq!(
                root.active_run().unwrap().view.as_ref(),
                Some(&first),
                "Stop revived a pending rerun after cleanup"
            );
            root.rerun(cx); // A fresh explicit rerun remains allowed after completion.
            assert_eq!(
                root.runs.get(&project).unwrap().active().unwrap().id,
                first_id
            );
            assert!(
                first.read(cx).job_completion().is_some(),
                "rerun must complete the old process tree before replacing it"
            );
            let current = root.active_run().unwrap().view.as_ref().unwrap();
            assert_ne!(current, &first, "rerun was never published");
            assert_eq!(current.read(cx).cwd(), dir.path());
            root.close_run_now(project, first_id, cx);
            assert_eq!(
                root.runs.get(&project).unwrap().active().unwrap().id,
                second_id
            );
            let second = root.active_run().unwrap().view.clone().unwrap();
            root.forget_run(project, cx);
            second
        })
    });
    await_run_cleanup(cx, &second);
    cx.update(|cx| {
        root.update(cx, |root, cx| {
            assert!(
                second.read(cx).job_completion().is_some(),
                "project disposal lost cleanup while a caller retained the terminal"
            );
            root.stop_every_process(cx);
        })
    });
}
fn await_run_cleanup(
    cx: &mut TestAppContext,
    view: &gpui::Entity<crate::terminal::view::TerminalTab>,
) {
    let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !cx.update(|cx| view.read(cx).job_completion().is_some()) {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(10));
        cx.run_until_parked();
        assert!(
            std::time::Instant::now() < end,
            "owned run cleanup did not complete"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(10));
    cx.run_until_parked();
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn live_close_can_cancel_and_then_stop_the_captured_run(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    assert_eq!(std::env::var("GUILD_RUN_WINDOW_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    let (project, id, view) = cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            root.start_run("printf retained; sleep 30".into(), cx);
            let project = root.active_project.unwrap();
            let entry = root.runs.get(&project).unwrap().active().unwrap();
            let id = entry.id;
            let view = entry.value.view.clone().unwrap();
            root.close_run_asking(project, id, window, cx);
            (project, id, view)
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            assert!(root.runs.get(&project).unwrap().get(id).is_some());
            assert!(view.read(cx).job_completion().is_none());
            root.close_run_asking(project, id, window, cx);
        })
    });
    cx.simulate_prompt_answer("Stop and close");
    cx.run_until_parked();
    await_run_cleanup(cx, &view);
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            assert!(root.runs.get(&project).unwrap().get(id).is_none());
            assert!(view.read(cx).job_completion().is_some());
            root.stop_every_process(cx);
        })
    });
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn run_shortcuts_address_the_selected_identity(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.start_run("printf first; sleep 30".into(), cx);
            root.start_run("printf second; sleep 30".into(), cx);
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = made.unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-alt-period");
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            assert_eq!(root.active_run().unwrap().state, run::State::Stopped);
            let entries = root.runs[&root.active_project.unwrap()].entries();
            assert_eq!(entries[0].value.state, run::State::Running);
            root.stop_every_process(cx);
        })
    });
}

#[gpui::test]
#[ignore = "runs in isolated_run_window_regressions"]
fn the_selected_run_tab_is_revealed_in_a_crowded_strip(cx: &mut TestAppContext) {
    // Real PTY workers can wake this test from an OS thread.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.reveal_tool(guild_session::Tool::Output, cx);
            for i in 0..8 {
                root.start_run(
                    format!("printf 'invocation-{i}-{}'", "long-value-".repeat(12)),
                    cx,
                );
            }
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(700.), gpui::px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let strip = cx.debug_bounds("run-tab-strip").unwrap();
    let selected = cx.debug_bounds("selected-run-tab").unwrap();
    let root = made.unwrap();
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let id = root.runs[&project].entries()[4].id;
            root.select_run(project, id, cx);
        })
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let id = root.runs[&project].entries()[0].id;
            root.close_run_now(project, id, cx);
        })
    });
    for _ in 0..2 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
    }
    let after_close = cx.debug_bounds("selected-run-tab").unwrap();
    cx.update(|_, cx| root.update(cx, |root, cx| root.stop_every_process(cx)));
    assert!(
        selected.left() >= strip.left() && selected.right() <= strip.right(),
        "new selected Run must remain fully reachable: {selected:?} in {strip:?}"
    );
    assert!(
        after_close.left() >= strip.left() && after_close.right() <= strip.right(),
        "closing a preceding Run must keep the selected identity reachable: {after_close:?} in {strip:?}"
    );
}

#[gpui::test]
fn elapsed_time_does_not_notify_root_or_hidden_output(cx: &mut TestAppContext) {
    if !crate::root::isolation::isolated_test(concat!(
        module_path!(),
        "::elapsed_time_does_not_notify_root_or_hidden_output"
    )) {
        return;
    }
    use std::{cell::Cell, rc::Rc};
    let dir = tempfile::tempdir().unwrap();
    let root = cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.runs.entry(project).or_default().insert(
                crate::run::tabs::Invocation {
                    cwd: dir.path().into(),
                    line: "synthetic".into(),
                    task: None,
                },
                Run {
                    line: "synthetic".into(),
                    view: None,
                    restart: None,
                    started: Instant::now() - Duration::from_secs(5),
                    ended: None,
                    state: run::State::Running,
                    shown: 0,
                },
            );
            root
        })
    });
    cx.run_until_parked();
    let notifications = Rc::new(Cell::new(0));
    let counted = notifications.clone();
    let _subscription =
        cx.update(|cx| cx.observe(&root, move |_, _| counted.set(counted.get() + 1)));
    root.update(cx, |root, cx| root.poll_runs(cx));
    cx.run_until_parked();
    assert_eq!(
        notifications.get(),
        0,
        "elapsed time invalidated the whole root"
    );
    root.read_with(cx, |root, _| assert!(root.active_run().unwrap().shown >= 5));
    let region = root.update(cx, |root, cx| {
        root.reveal_tool(Tool::Output, cx);
        let part = crate::view::region::Part::Dock(root.docks().dock_of(Tool::Output));
        root.region(part, cx)
    });
    cx.run_until_parked();
    notifications.set(0);
    let region_notifications = Rc::new(Cell::new(0));
    let counted = region_notifications.clone();
    let _region_subscription =
        cx.update(|cx| cx.observe(&region, move |_, _| counted.set(counted.get() + 1)));
    root.update(cx, |root, cx| {
        let project = root.active_project.unwrap();
        root.runs
            .get_mut(&project)
            .unwrap()
            .active_mut()
            .unwrap()
            .value
            .shown = 0;
        root.poll_runs(cx);
    });
    cx.run_until_parked();
    assert_eq!(
        notifications.get(),
        0,
        "visible elapsed time invalidated Root"
    );
    assert_eq!(
        region_notifications.get(),
        1,
        "visible Output clock was not refreshed"
    );
}
