//! The real window runs in an isolated subprocess: no user state or background
//! services. Frame callbacks are dispatched before draw, as on the native host.
use super::*;
use crate::terminal::view::{Look, open_command};
use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px};
use guild_session::{Desk, Panels, Settings, Tool};

// The real Root render runs in a subprocess with its own state directory.
#[test]
fn isolated_terminal_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::menu::terminal_panel_tests::",
            "--ignored",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_PANEL_WINDOW_TEST", "1")
        .env("SHELL", "/nonexistent/guild-test-shell")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    Entity<crate::Root>,
    &mut VisualTestContext,
) {
    assert_eq!(std::env::var("GUILD_PANEL_WINDOW_TEST").as_deref(), Ok("1"));
    // This fixture owns real PTY workers, outside the deterministic scheduler.
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let mut panels = Panels::default();
        panels.left.open = false;
        panels.bottom.open = true;
        panels.docks_mut().show(Tool::Terminal);
        let entity = cx.new(|cx| {
            let mut root = crate::Root::from_state(
                Settings {
                    panels: Some(panels),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.isolated_render = true;
            let mut tabs = crate::terminal_panel::TerminalTabs::default();
            for _ in 0..2 {
                let command = portable_pty::CommandBuilder::new("/bin/cat");
                tabs.add(
                    open_command(
                        command,
                        dir.path(),
                        Look::of(&root.settings, root.theme),
                        None,
                        cx,
                    )
                    .unwrap(),
                );
            }
            root.panel_terminals.insert(project, tabs);
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(gpui::size(px(900.), px(500.)));
    (dir, root.unwrap(), cx)
}
fn frames(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|w, cx| w.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}
fn open_terminal_info(cx: &mut VisualTestContext, job: Option<guild_session::terminal_job::JobId>) {
    cx.update(|w, cx| w.simulate_mouse_move(gpui::point(px(0.), px(0.)), cx));
    frames(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    frames(cx);
    let selector: &'static str = match job {
        Some(id) => Box::leak(format!("terminal-job-info:{id}").into_boxed_str()),
        None => "terminal-job-info",
    };
    let bounds = cx
        .debug_bounds(selector)
        .expect("dedicated launch information affordance");
    cx.update(|w, cx| w.simulate_mouse_move(bounds.center(), cx));
    frames(cx);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(1));
    frames(cx);
    assert!(cx.debug_bounds("terminal-context-detail").is_some());
}

fn assert_terminal_focus(root: &Entity<crate::Root>, cx: &mut VisualTestContext) {
    frames(cx);
    cx.update(|window, cx| {
        let r = root.read(cx);
        let view = r.active_panel_terminal().unwrap();
        assert!(
            view.read(cx).focus_handle(cx).is_focused(window),
            "selected terminal must keep the keys after layout"
        );
    });
}
fn stop(root: &Entity<crate::Root>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.forget_panel_terminals(r.active_project.unwrap(), cx)
        })
    });
}
#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn selecting_a_hidden_terminal_keeps_input_out_of_the_start_screen(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.select_panel_terminal(r.active_project.unwrap(), 1, window, cx)
        });
        // Native callbacks run before draw; do not let test-mode update draw first.
        window.simulate_next_frame(cx);
    });
    assert_terminal_focus(&root, cx);
    stop(&root, cx);
}
#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn closing_active_terminal_focuses_its_newly_drawn_neighbor(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.close_panel_terminal(r.active_project.unwrap(), 2, window, cx)
        });
        window.simulate_next_frame(cx);
    });
    assert_terminal_focus(&root, cx);
    stop(&root, cx);
}
#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn dismissing_terminal_menu_returns_to_the_visible_terminal(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let target = Target::Terminal {
                at: TerminalAt::Panel(r.active_project.unwrap(), 2),
                selection: false,
            };
            r.refocus_after_menu(&target, window, cx);
        })
    });
    assert_terminal_focus(&root, cx);
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn terminal_menu_find_keeps_the_query_field_focused(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let target = Target::Terminal {
                at: TerminalAt::Panel(r.active_project.unwrap(), 2),
                selection: false,
            };
            r.refocus_after_menu(&target, window, cx);
            r.run_terminal_deed(TerminalDeed::Find, &target, window, cx);
        })
    });
    frames(cx);
    cx.update(|window, cx| {
        assert!(
            gpui_component::WindowExt::has_focused_input(window, cx),
            "Find must focus the shared query input"
        )
    });
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn smallest_ui_keeps_bottom_header_buttons_outside_resize_grip(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.ui_font_size = 10.;
            r.move_tool(Tool::Explorer, guild_session::Dock::Bottom, cx);
        })
    });
    frames(cx);
    let bounds = cx
        .debug_bounds("reveal-file")
        .expect("Explorer header drawn");
    let point = gpui::point(bounds.center().x, bounds.top() + px(0.25));
    cx.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| {
        assert!(
            root.read(cx).dragging.is_none(),
            "header button must not start resizing"
        )
    });
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    let boundary = cx.update(|_, cx| {
        root.read(cx).dock_head_at.get()[guild_session::Dock::Bottom as usize]
            .unwrap()
            .top()
            - px(1.)
    });
    let point = gpui::point(bounds.center().x, boundary);
    cx.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).dragging,
            Some(crate::view::panels::Side::Bottom),
            "the boundary must still resize"
        )
    });
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn a_later_focus_gesture_cancels_pending_dock_activation(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.select_panel_terminal(r.active_project.unwrap(), 1, window, cx)
        });
        window.simulate_next_frame(cx);
    });
    cx.update(|window, cx| window.focus(&root.read(cx).start_focus.clone(), cx));
    frames(cx);
    cx.update(|window, cx| {
        assert!(
            root.read(cx).start_focus.is_focused(window),
            "a later user focus choice must win"
        )
    });
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn failed_new_shell_restores_input_to_the_existing_terminal(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(r.dock_focus(crate::view::panels::Side::Bottom), cx);
            r.new_panel_terminal_by_hand(window, cx);
            assert_eq!(
                r.panel_terminals[&r.active_project.unwrap()]
                    .entries()
                    .len(),
                2
            );
        })
    });
    assert_terminal_focus(&root, cx);
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn empty_terminal_jobs_remain_discoverable(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| root.read_with(cx, |r, _| assert!(r.terminal_jobs.receipts().is_empty())));
    frames(cx);
    let button = cx
        .debug_bounds("terminal-jobs-trigger")
        .expect("empty Terminal Jobs button must stay visible");
    let usage = cx.debug_bounds("titlebar-usage").unwrap();
    let open = cx.debug_bounds("titlebar-open-in").unwrap();
    assert_eq!(
        open.left() - usage.right(),
        button.left() - open.right(),
        "titlebar control gaps must be equal"
    );
    assert_eq!(
        open.left() - usage.right(),
        px(8.),
        "titlebar spacing must be 8 points at default scale"
    );
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    frames(cx);
    cx.update(|_, cx| {
        root.read_with(cx, |r, _| {
            assert!(r.terminal_list.is_some(), "empty job list did not open");
            assert!(
                r.terminal_jobs.receipts().is_empty(),
                "opening the list started a job"
            );
        })
    });
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn narrow_window_keeps_settings_in_menu_and_shortcut(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.simulate_resize(gpui::size(px(500.), px(500.)));
    frames(cx);
    assert!(
        cx.debug_bounds("titlebar-settings").is_none(),
        "Settings gear still occupies the titlebar"
    );
    cx.simulate_keystrokes("cmd-,");
    frames(cx);
    cx.update(|_, cx| {
        root.read_with(cx, |r, _| {
            assert!(
                r.settings_page.is_some()
                    || matches!(r.active_body(), Some(crate::Body::Settings { .. })),
                "Settings shortcut stopped opening the existing host"
            );
        })
    });
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn fullscreen_navigation_stays_beside_the_toggle_when_column_folds(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|window, cx| {
        window.toggle_fullscreen();
        root.update(cx, |r, cx| {
            r.settings.column_width = 232.;
            r.settings.column_folded = false;
            cx.notify();
        });
    });
    frames(cx);
    let back = cx.debug_bounds("go-back").expect("Back drawn");
    let fixed_left = back.left();
    let toggle = cx
        .debug_bounds("column-button")
        .expect("Workspaces toggle drawn");
    let gap = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::CARD_GAP));
    assert_eq!(back.left(), toggle.right() + px(gap));
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.settings.column_folded = true;
            cx.notify();
        });
    });
    frames(cx);
    let back = cx
        .debug_bounds("go-back")
        .expect("Back drawn without column");
    let edge = cx.update(|_, cx| root.read(cx).panel_geometry().column);
    let toggle = cx
        .debug_bounds("column-button")
        .expect("Workspaces toggle drawn");
    let gap = cx.update(|_, cx| root.read(cx).rem_px(crate::theme::space::CARD_GAP));
    assert_eq!(toggle.left(), px(edge));
    assert_eq!(
        back.left(),
        fixed_left,
        "folding cannot move fullscreen history"
    );
    assert_eq!(back.left(), toggle.right() + px(gap));
    stop(&root, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn archive_quit_closes_view_restores_focus_and_keeps_output(cx: &mut TestAppContext) {
        use guild_session::{
            overlay::Target,
            terminal_job::{JobState, Presentation},
        };
        use std::os::unix::fs::PermissionsExt;
        let (dir, root, cx) = mount(cx);
        let (owner, body) = history_session(&root, dir.path(), cx);
        let program = dir.path().join("saved-viewer");
        std::fs::write(&program, "#!/bin/sh\nstty -echo\nprintf 'VIEWER_READY\\n'\nread -r answer\nprintf 'VIEWER_DIAGNOSTIC\\n'\nexit $(cat viewer-code)\n").unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
        let original = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":"archive-dismissal","program":"/bin/sh",
                    "args":["-c","printf ORIGINAL_OUTPUT"],"title":"saved-viewer (archive)",
                    "cwd":dir.path(),"target":{"kind":"session","id":owner},
                    "archive_viewer":{"directory":dir.path(),"program":program}
                }))
                .unwrap();
                let receipt = r
                    .terminal_jobs
                    .start(
                        owner,
                        r.active_project.unwrap(),
                        &spec,
                        Look::of(&r.settings, r.theme),
                        cx,
                    )
                    .unwrap();
                r.open_terminal_presentation(&receipt.id, cx).unwrap();
                receipt.id
            })
        });
        let wait = |job, cx: &mut VisualTestContext| {
            let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
            loop {
                let done = cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        if r.terminal_jobs.poll(cx) {
                            cx.notify();
                        }
                        r.terminal_jobs.receipt(&job).unwrap().state.is_terminal()
                    })
                });
                if done {
                    break;
                }
                assert!(std::time::Instant::now() < until, "fixture did not finish");
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            frames(cx);
        };
        wait(original, cx);
        let before = cx.update(|_, cx| {
            root.update(cx, |r, _| {
                let receipt = r.terminal_jobs.receipt(&original).unwrap();
                assert_eq!(
                    receipt.presentation,
                    Presentation::Shown,
                    "ordinary jobs retain output even with an archive-like title"
                );
                r.overlays
                    .close(&format!("terminal-{original}"), &owner.to_string())
                    .unwrap();
                r.terminal_jobs
                    .present(&original, None, Presentation::Closed)
                    .unwrap();
                r.terminal_jobs.receipt(&original).unwrap()
            })
        });
        for code in [0, 2] {
            std::fs::write(dir.path().join("viewer-code"), code.to_string()).unwrap();
            let viewer = cx.update(|window, cx| {
                root.update(cx, |r, cx| {
                    body.read(cx).focus_handle(cx).focus(window, cx);
                    r.open_revmux_archive(&original, window, cx).unwrap();
                    r.terminal_jobs
                        .receipts()
                        .into_iter()
                        .find(|r| r.is_archive_view() && !r.state.is_terminal())
                        .unwrap()
                        .id
                })
            });
            frames(cx);
            cx.update(|_, cx| {
                let terminal = root.read(cx).terminal_jobs.terminal(&viewer).unwrap();
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("q\n".into()));
                terminal.update(cx, |tab, cx| tab.paste(cx));
            });
            wait(viewer, cx);
            cx.update(|window, cx| {
                root.update(cx, |r, cx| {
                    let receipt = r.terminal_jobs.receipt(&viewer).unwrap();
                    assert_eq!(receipt.state, JobState::Exited);
                    assert_eq!(receipt.exit_code, Some(code));
                    assert_eq!(
                        receipt.presentation,
                        Presentation::Closed,
                        "quitting the archive must dismiss its presentation"
                    );
                    assert!(r.overlays.get(&format!("terminal-{viewer}")).is_none());
                    assert!(
                        body.read(cx).focus_handle(cx).is_focused(window),
                        "quitting must return keyboard focus to the initiating terminal"
                    );
                    assert!(
                        r.terminal_jobs
                            .terminal(&viewer)
                            .unwrap()
                            .read(cx)
                            .has_text()
                    );
                    assert_eq!(r.terminal_jobs.receipt(&original).unwrap(), before);
                    r.reattach_terminal_job(&viewer, Target::Session(owner), window, cx)
                        .unwrap();
                })
            });
            frames(cx);
            cx.update(|_, cx| {
                let r = root.read(cx);
                assert_eq!(
                    r.terminal_jobs.receipt(&viewer).unwrap().presentation,
                    Presentation::Shown,
                    "explicit Show output must stay open after completion"
                );
                assert!(
                    r.overlays
                        .get(&format!("terminal-{viewer}"))
                        .unwrap()
                        .visible
                );
            });
        }
        cx.update(|_, cx| body.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_cross_hides_live_output_and_reopens_at_the_same_geometry(cx: &mut TestAppContext) {
        let (dir, root, cx) = mount(cx);
        let (owner, body) = history_session(&root, dir.path(), cx);
        let job = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":"cross-keeps-view","program":"/bin/cat",
                    "title":"Live output","cwd":dir.path(),"target":{"kind":"session","id":owner}
                }))
                .unwrap();
                let receipt = r
                    .terminal_jobs
                    .start(owner, project, &spec, Look::of(&r.settings, r.theme), cx)
                    .unwrap();
                r.open_terminal_presentation(&receipt.id, cx).unwrap();
                receipt.id
            })
        });
        frames(cx);
        let id = format!("terminal-{job}");
        let geometry = crate::observer::Geometry {
            x: 12.,
            y: 18.,
            width: 400.,
            height: 240.,
        };
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.overlay_focus
                    .borrow_mut()
                    .get_mut(&id)
                    .unwrap()
                    .presentation
                    .remember(geometry);
                cx.notify();
            })
        });
        frames(cx);
        let close = cx.update(|window, _| {
            gpui_kit::base::test_support::find(window, &[], &gpui::ElementId::from("overlay-close"))
                .expect("terminal cross is painted")
                .bounds()
        });
        cx.simulate_click(close.center(), gpui::Modifiers::default());
        frames(cx);
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let receipt = r.terminal_jobs.receipt(&job).unwrap();
                assert_eq!(
                    receipt.presentation,
                    guild_session::terminal_job::Presentation::Hidden,
                    "the cross must hide the view, preserving Terminal Jobs recovery"
                );
                assert!(
                    !receipt.state.is_terminal(),
                    "the cross stopped the program"
                );
                assert!(receipt.cancellation.is_none());
                assert!(body.read(cx).focus_handle(cx).is_focused(window));
                assert_eq!(r.overlay_focus.borrow()[&id].presentation.compact, geometry);
                assert!(
                    gpui_kit::base::test_support::find(
                        window,
                        &[],
                        &gpui::ElementId::Name(format!("overlay-chip-{id}").into())
                    )
                    .is_none(),
                    "hidden terminal jobs must be recovered through Terminal Jobs, without a corner chip"
                );
                r.open_terminal_presentation(&job, cx).unwrap();
            })
        });
        frames(cx);
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                assert_eq!(r.overlay_focus.borrow()[&id].presentation.compact, geometry);
                assert_eq!(
                    r.terminal_jobs.receipt(&job).unwrap().presentation,
                    guild_session::terminal_job::Presentation::Shown
                );
                assert!(
                    gpui_kit::base::test_support::find(
                        window,
                        &[],
                        &gpui::ElementId::from("terminal-hide")
                    )
                    .is_none(),
                    "there must be only one terminal dismissal control"
                );
                r.terminal_jobs
                    .cancel(&job, "fixture complete", cx)
                    .unwrap();
            })
        });
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_reservations_capture_original_conversation_cwd_and_cached_branch(
        cx: &mut TestAppContext,
    ) {
        use guild_session::{
            Runs, SessionRecord,
            command::Answer,
            terminal_job::{HistoryContext, Scope, TerminalCommand},
        };
        let (dir, root, cx) = mount(cx);
        let cwd = dir.path().join("program-subdirectory");
        std::fs::create_dir(&cwd).unwrap();
        crate::git::run(dir.path(), &["init", "-q", "-b", "observed-a"]).unwrap();
        crate::git::run(dir.path(), &["config", "user.name", "Fixture"]).unwrap();
        crate::git::run(
            dir.path(),
            &["config", "user.email", "fixture@example.invalid"],
        )
        .unwrap();
        std::fs::write(dir.path().join("file"), "fixture").unwrap();
        crate::git::run(dir.path(), &["add", "file"]).unwrap();
        crate::git::run(dir.path(), &["commit", "-qm", "fixture"]).unwrap();
        let snapshot = crate::git::branch_update::Snapshot::read(dir.path()).unwrap();
        let (owner, resumed, project) = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                let seat = r.active_seat().unwrap();
                let a = r.desk.add_session(SessionRecord::new(
                    "Original A".into(),
                    seat,
                    dir.path().into(),
                    Runs::Shell,
                ));
                let b = r.desk.add_session(SessionRecord::new(
                    "Conversation B".into(),
                    seat,
                    dir.path().into(),
                    Runs::Shell,
                ));
                let tab = r.adopt(
                    seat,
                    project,
                    crate::Body::Broken {
                        session: a,
                        reason: "fixture".into(),
                    },
                    cx,
                );
                r.active_tab = Some(tab);
                (a, b, project)
            })
        });
        let request = |key: &str| -> TerminalCommand {
            serde_json::from_value(serde_json::json!({
        "operation":"start", "spec":{"version":1,"request_id":key,"program":"/missing/fixture", "cwd":cwd,
        "target":{"kind":"session","id":owner}}
    })).unwrap()
        };
        let launch = |key: &str, cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    match r.command_terminal(&request(key), Some(&owner.to_string()), cx) {
                        Answer::Terminal {
                            reply: guild_session::terminal_job::TerminalReply::Job { receipt },
                        } => receipt,
                        other => panic!("launch refused: {other:?}"),
                    }
                })
            })
        };
        let before_discovery = launch("before-discovery", cx);
        assert!(
            before_discovery
                .launch_context
                .as_ref()
                .unwrap()
                .repository
                .is_none()
        );
        assert!(
            before_discovery
                .launch_context
                .as_ref()
                .unwrap()
                .branch
                .is_none()
        );
        let until = std::time::Instant::now() + std::time::Duration::from_secs(8);
        loop {
            let discovered = cx.update(|_, cx| {
                root.update(cx, |r, cx| {
                    r.poll_git(cx);
                    r.desk.project(project).unwrap().repository.is_some()
                })
            });
            if discovered {
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "git worker did not discover fixture"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|_, cx| {
            root.update(cx, |r, _| {
                r.update_sources.insert(project, Ok(snapshot));
            })
        });
        crate::git::run(dir.path(), &["switch", "-qc", "external-b"]).unwrap();
        let original = launch("in-a", cx);
        let context = original.launch_context.as_ref().unwrap();
        assert_eq!(context.conversation, Some(owner));
        assert_eq!(
            context.branch.as_deref(),
            Some("observed-a"),
            "the cache is an observation, never freshly verified HEAD"
        );
        assert_eq!(context.program_cwd, cwd);
        assert_eq!(context.root_path.as_deref(), Some(dir.path()));
        cx.update(|_, cx| {
            root.update(cx, |r, _| {
                r.started_as.insert(owner, resumed);
                r.desk.close_session(owner);
                let tab = r
                    .tabs
                    .iter_mut()
                    .find(|t| Some(t.id) == r.active_tab)
                    .unwrap();
                let pane = tab.panes.focused();
                tab.panes.take(pane);
                tab.panes
                    .fill(
                        pane,
                        crate::Body::Broken {
                            session: resumed,
                            reason: "fixture".into(),
                        },
                    )
                    .unwrap_or_else(|_| panic!("pane"));
                r.desk.rename_session(resumed, "Renamed B");
            })
        });
        let second = launch("in-b", cx);
        assert_eq!(second.owner, owner);
        assert_eq!(
            second.launch_context.as_ref().unwrap().conversation,
            Some(resumed)
        );
        assert_eq!(
            second
                .launch_context
                .as_ref()
                .unwrap()
                .session_label
                .as_deref(),
            Some("Renamed B")
        );
        assert_eq!(
            launch("in-a", cx).launch_context,
            original.launch_context,
            "retry after resume must not refresh context"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, _| {
                let before = r.terminal_jobs.receipt(&before_discovery.id).unwrap();
                assert_eq!(before.launch_context, before_discovery.launch_context);
                let sibling = r.desk.add_project(&dir.path().join("sibling"));
                let repo = r.desk.project(project).unwrap().repository.clone().unwrap();
                r.desk.set_repository(sibling, Some(repo.clone()));
                let ctx = HistoryContext {
                    project: Some(sibling),
                    repositories: [(project, repo.clone()), (sibling, repo)].into(),
                    ..Default::default()
                };
                assert!(ctx.contains(Scope::Project, &before));
                r.desk.remove_session(owner);
                r.desk.remove_session(resumed);
                let recovered =
                    guild_session::terminal_job::Book::restore(r.terminal_jobs.receipts())
                        .unwrap()
                        .receipts();
                let ctx = HistoryContext {
                    conversation: Some(resumed),
                    ..Default::default()
                };
                assert_eq!(ctx.select(Scope::Session, &recovered).completed.len(), 1);
                assert_eq!(
                    recovered
                        .iter()
                        .find(|r| r.id == original.id)
                        .unwrap()
                        .launch_context,
                    original.launch_context
                );
            })
        });
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_has_a_scope_control_over_the_real_root(cx: &mut TestAppContext) {
        let (_dir, root, cx) = mount(cx);
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        assert!(
            cx.debug_bounds("terminal-job-scope").is_some(),
            "Terminal Jobs must offer an explicit scope control above its rows"
        );
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_refreshes_checkout_snapshot_with_popup_open(cx: &mut TestAppContext) {
        use crate::git::worktrees::Worktree;
        use guild_session::{
            SessionId,
            terminal_job::{Book, Scope},
        };
        let (dir, root, cx) = mount(cx);
        let owner = SessionId::new();
        let repository = dir.path().join(".git");
        let receipts = cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                r.desk.set_repository(project, Some(repository.clone()));
                let mut book = Book::default();
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":"refresh","program":"/missing/fixture",
                    "cwd":dir.path(),"target":{"kind":"session","id":owner}
                }))
                .unwrap();
                let context =
                    crate::terminal_job::launch_context(&r.desk, owner, project, dir.path(), None);
                let id = *book
                    .reserve_in(owner, project, &spec, Some(context))
                    .unwrap()
                    .job_id();
                book.get_mut(&id).unwrap().state = guild_session::terminal_job::JobState::Exited;
                r.toggle_terminal_list(window, cx);
                book.receipts()
            })
        });
        let selected = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                let r = root.read(cx);
                assert!(r.terminal_list.is_some());
                r.terminal_history_context()
                    .select(Scope::All, &receipts)
                    .completed
                    .len()
            })
        };
        assert_eq!(selected(cx), 1, "no successful discovery preserves history");
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.worktrees.insert(
                    repository.clone(),
                    vec![Worktree {
                        path: dir.path().to_owned(),
                        ..Default::default()
                    }],
                );
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(
            selected(cx),
            1,
            "listed checkout survives without an open owner session"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.worktrees.insert(
                    repository.clone(),
                    vec![
                        Worktree {
                            path: dir.path().to_owned(),
                            prunable: true,
                            ..Default::default()
                        },
                        Worktree {
                            path: dir.path().to_owned(),
                            bare: true,
                            ..Default::default()
                        },
                    ],
                );
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(
            selected(cx),
            0,
            "current shown-checkout policy filters removed history"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.worktrees.insert(
                    repository.clone(),
                    vec![Worktree {
                        path: dir.path().to_owned(),
                        locked: Some("fixture".into()),
                        ..Default::default()
                    }],
                );
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(
            selected(cx),
            1,
            "locked checkout remains usable after snapshot refresh"
        );
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_forgets_unobserved_snapshots_with_all_popup_open(cx: &mut TestAppContext) {
        use guild_session::{
            SessionId,
            terminal_job::{Book, JobState, RevmuxArchive, Scope},
        };
        let (dir, root, cx) = mount(cx);
        let repository = dir.path().join(".git");
        let receipt_file = dir.path().join("receipts.json");
        let report = dir.path().join("saved-report.json");
        std::fs::write(&report, "saved archive evidence").unwrap();
        let (project, sibling, before) = cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                let sibling = r
                    .desk
                    .open_in(r.active_workspace, &dir.path().join("sibling"));
                r.desk.set_repository(project, Some(repository.clone()));
                r.desk.set_repository(sibling, Some(repository.clone()));
                let owner = SessionId::new();
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":"observation","program":"/missing/fixture",
                    "cwd":dir.path(),"target":{"kind":"session","id":owner}
                }))
                .unwrap();
                let context =
                    crate::terminal_job::launch_context(&r.desk, owner, project, dir.path(), None);
                let mut book = Book::default();
                let id = *book
                    .reserve_in(owner, project, &spec, Some(context))
                    .unwrap()
                    .job_id();
                let receipt = book.get_mut(&id).unwrap();
                receipt.state = JobState::Exited;
                receipt.revmux_archive = Some(Box::new(RevmuxArchive {
                    directory: dir.path().to_owned(),
                    program: "/bin/echo".into(),
                }));
                std::fs::write(
                    &receipt_file,
                    serde_json::to_vec(&serde_json::json!({"jobs":book.receipts()})).unwrap(),
                )
                .unwrap();
                r.terminal_jobs = crate::terminal_job::LiveJobs::load(receipt_file.clone());
                let before = serde_json::to_vec(&r.terminal_jobs.receipts()).unwrap();
                r.worktrees.insert(repository.clone(), vec![]);
                r.toggle_terminal_list(window, cx);
                r.select_terminal_scope(Scope::All, cx);
                (project, sibling, before)
            })
        });
        let count = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                let r = root.read(cx);
                let list = r.terminal_list.as_ref().expect("popup remains open");
                assert_eq!(list.scope, Scope::All);
                let receipts = r.terminal_jobs.receipts();
                assert_eq!(
                    serde_json::to_vec(&receipts).unwrap(),
                    before,
                    "archives and receipt metadata are untouched"
                );
                r.terminal_history_context()
                    .select(list.scope, &receipts)
                    .completed
                    .len()
            })
        };
        frames(cx);
        assert_eq!(
            count(cx),
            0,
            "known absent checkout is hidden while observed"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.desk.remove_project(project);
                r.active_project = Some(sibling);
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(count(cx), 0, "sibling root keeps the repository observed");
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.desk.remove_project(sibling);
                r.active_project = None;
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(
            count(cx),
            1,
            "last observer closed: stale absence becomes unknown"
        );
        cx.update(|_, cx| {
            let r = root.read(cx);
            assert!(
                r.worktrees.contains_key(&repository),
                "column cache is retained"
            );
            assert!(
                !r.terminal_history_context()
                    .worktrees
                    .contains_key(&repository)
            );
        });
        assert_eq!(
            std::fs::read_to_string(&report).unwrap(),
            "saved archive evidence"
        );
        // The fixture removed every active project; release its original PTYs
        // explicitly rather than using the active-project teardown helper.
        cx.update(|_, cx| root.update(cx, |r, cx| r.forget_panel_terminals(project, cx)));
    }

    fn history_session(
        root: &Entity<crate::Root>,
        dir: &std::path::Path,
        cx: &mut VisualTestContext,
    ) -> (
        guild_session::SessionId,
        Entity<crate::terminal::view::TerminalTab>,
    ) {
        cx.update(|window, cx| root.update(cx, |r, cx| r.history_session_fixture(dir, window, cx)))
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_defaults_reset_for_session_file_empty_and_no_root(cx: &mut TestAppContext) {
        use guild_session::terminal_job::Scope;
        let (dir, root, cx) = mount(cx);
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                r.toggle_terminal_list(w, cx);
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::Worktree);
            })
        });
        frames(cx);
        let button = cx.debug_bounds("terminal-job-scope").unwrap();
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        frames(cx);
        cx.update(|_, cx| {
            let r = root.read(cx);
            assert!(r.terminal_list.is_some());
            assert!(r.context_menu.is_some());
        });
        cx.simulate_keystrokes("down down down enter");
        frames(cx);
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::All);
                assert!(r.terminal_list.as_ref().unwrap().focus.is_focused(w));
                r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
                r.toggle_terminal_list(w, cx);
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::Worktree);
                r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
            })
        });
        let (session, view) = history_session(&root, dir.path(), cx);
        frames(cx);
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                r.toggle_terminal_list(w, cx);
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::Session);
                assert_eq!(
                    r.terminal_list.as_ref().unwrap().context.conversation,
                    Some(session)
                );
                r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
            })
        });
        std::fs::write(dir.path().join("fixture.rs"), "fn main() {}\n").unwrap();
        cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                r.open_file(std::path::Path::new("fixture.rs"), w, cx);
                r.toggle_terminal_list(w, cx);
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::Worktree);
                r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
                let tab = r
                    .tabs
                    .iter_mut()
                    .find(|t| Some(t.id) == r.active_tab)
                    .unwrap();
                tab.panes.take(tab.panes.focused());
                r.toggle_terminal_list(w, cx);
                assert_eq!(r.terminal_list.as_ref().unwrap().scope, Scope::Worktree);
                r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
                r.active_project = None;
                r.active_tab = None;
                r.toggle_terminal_list(w, cx);
                let list = r.terminal_list.as_ref().unwrap();
                assert_eq!(list.scope, Scope::All);
                assert!(!Scope::Session.available(&list.context));
                assert!(!Scope::Worktree.available(&list.context));
                assert!(!Scope::Project.available(&list.context));
                r.active_project = Some(r.desk.projects().next().unwrap().id);
            })
        });
        cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_scope_gestures_restore_popup_then_terminal_without_leaking_input(
        cx: &mut TestAppContext,
    ) {
        let (dir, root, cx) = mount(cx);
        let (_, view) = history_session(&root, dir.path(), cx);
        frames(cx);
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !dir.path().join("input").exists() {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        cx.simulate_keystrokes("x tab enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        frames(cx);
        cx.update(|_, cx| {
            assert!(
                root.read(cx).context_menu.is_some(),
                "Tab/Enter opens the shared scope menu under Guild's keymap"
            )
        });
        cx.simulate_keystrokes("x escape");
        frames(cx);
        cx.update(|w, cx| {
            let r = root.read(cx);
            assert!(r.context_menu.is_none());
            assert!(r.terminal_list.as_ref().unwrap().focus.is_focused(w));
        });
        cx.simulate_keystrokes("x escape");
        frames(cx);
        cx.update(|w, cx| {
            assert!(root.read(cx).terminal_list.is_none());
            assert!(view.read(cx).focus_handle(cx).is_focused(w));
        });
        cx.simulate_keystrokes("y");
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let bytes = std::fs::read(dir.path().join("input")).unwrap();
            if !bytes.is_empty() {
                assert_eq!(
                    bytes, b"y",
                    "popup/menu text and Escape must not reach the child"
                );
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_empty_widening_and_scroll_reset_stay_inside_small_viewport(
        cx: &mut TestAppContext,
    ) {
        use guild_session::terminal_job::{LaunchSpec, Scope};
        let (dir, root, cx) = mount(cx);
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        let wider = cx.debug_bounds("terminal-wider").unwrap();
        cx.simulate_click(wider.center(), gpui::Modifiers::default());
        frames(cx);
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).terminal_list.as_ref().unwrap().scope,
                Scope::Project
            )
        });
        cx.update(|_,cx|root.update(cx,|r,cx| {
        let owner=guild_session::SessionId::new();let project=r.active_project.unwrap();
        for i in 0..12 {
            let spec:LaunchSpec=serde_json::from_value(serde_json::json!({"version":1,"request_id":format!("long-{i}"),"program":"/missing/program","title":"Long job title ".repeat(30),"cwd":dir.path(),"target":{"kind":"session","id":owner}})).unwrap();
            let mut context=crate::terminal_job::launch_context(&r.desk,owner,project,dir.path(),Some("observed-long-branch"));context.root_label=Some("long-worktree-label".repeat(20));
            r.terminal_jobs.start_in(owner,project,&spec,Some(context),Look::of(&r.settings,r.theme),cx).unwrap();
        }
        r.select_terminal_scope(Scope::All,cx);
    }));
        cx.simulate_resize(gpui::size(px(360.), px(260.)));
        frames(cx);
        let panel = cx.debug_bounds("terminal-jobs-panel").unwrap();
        let scope = cx.debug_bounds("terminal-job-scope").unwrap();
        let viewport = cx.update(|window, _| window.viewport_size());
        assert!(
            viewport.width >= px(728.) && viewport.height >= px(286.),
            "undersized requests stop at the main-window minimum"
        );
        assert!(
            panel.left() >= px(0.)
                && panel.right() <= viewport.width
                && panel.top() >= px(0.)
                && panel.bottom() <= viewport.height,
            "popup outside viewport: {panel:?}"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.terminal_list
                    .as_ref()
                    .unwrap()
                    .scroll
                    .set_offset(gpui::point(px(0.), px(-150.)));
                cx.notify();
            })
        });
        frames(cx);
        assert_eq!(
            cx.debug_bounds("terminal-job-scope").unwrap(),
            scope,
            "header must remain fixed while rows scroll"
        );
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                assert!(r.terminal_list.as_ref().unwrap().scroll.offset().y < px(0.));
                r.select_terminal_scope(Scope::Worktree, cx);
                assert_eq!(
                    r.terminal_list.as_ref().unwrap().scroll.offset(),
                    gpui::Point::default()
                );
            })
        });
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_uses_current_body_after_two_processes_resume(cx: &mut TestAppContext) {
        let (dir, root, cx) = mount(cx);
        let (a, a_view) = history_session(&root, dir.path(), cx);
        let (b, b_view) = history_session(&root, dir.path(), cx);
        cx.update(|window, cx| root.update(cx, |r, cx| {
            let mut records = r.desk.take_sessions();
            for record in &mut records {
                if [a,b].contains(&record.id) { record.runs = guild_session::Runs::Codex(guild_session::SessionMode::Terminal); }
            }
            r.desk.absorb_sessions(records);
            let c = r.desk.add_session(guild_session::SessionRecord::new(
                "Conversation C".into(), r.active_seat().unwrap(), dir.path().into(), guild_session::Runs::Codex(guild_session::SessionMode::Terminal),
            ));
            let tab_b = r.tabs.iter().find(|t| t.session() == Some(b)).unwrap().id;
            let tab_a = r.tabs.iter().find(|t| t.session() == Some(a)).unwrap().id;
            r.rebind(tab_b, b, c, cx);
            r.rebind(tab_a, a, b, cx);
            r.show_tab(tab_a, cx);
            let spec = serde_json::from_value(serde_json::json!({
                "version":1,"request_id":"two-resumes","program":"/missing/program","cwd":dir.path(),
                "target":{"kind":"session","id":a},"foreground":false
            })).unwrap();
            let request = guild_session::terminal_job::TerminalCommand::Start { spec };
            let answer = r.command_terminal(&request, Some(&a.to_string()), cx);
            assert!(matches!(answer, guild_session::command::Answer::Terminal { .. }), "{answer:?}");
            r.toggle_terminal_list(window, cx);
            let list = r.terminal_list.as_ref().unwrap();
            assert_eq!(list.context.conversation, Some(b), "Body already names the resumed conversation");
            let receipts = r.terminal_jobs.receipts();
            assert_eq!(r.terminal_history_context().select(list.scope, &receipts).completed.len(), 1);
        }));
        for view in [a_view, b_view] {
            cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        }
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_tabs_cycle_inside_popup_in_both_directions(cx: &mut TestAppContext) {
        let (dir, root, cx) = mount(cx);
        let (_, view) = history_session(&root, dir.path(), cx);
        frames(cx);
        cx.update(|_, cx| root.update(cx, |r, cx| r.open_settings(cx)));
        frames(cx);
        let previous = cx.update(|w, cx| w.focused(cx).unwrap());
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        for key in ["tab", "shift-tab"] {
            for _ in 0..12 {
                cx.simulate_keystrokes(key);
                frames(cx);
                cx.update(|w, cx| {
                    assert!(
                        root.read(cx)
                            .terminal_list
                            .as_ref()
                            .unwrap()
                            .focus
                            .contains_focused(w, cx),
                        "Tab traversal escaped the open Terminal Jobs popup"
                    );
                });
            }
        }
        cx.simulate_keystrokes("escape");
        frames(cx);
        cx.update(|_, cx| assert!(root.read(cx).terminal_list.is_none()));
        cx.update(|w, _cx| assert!(previous.is_focused(w)));
        cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn keyboard_widening_keeps_focus_when_the_empty_action_disappears(cx: &mut TestAppContext) {
        let (dir, root, cx) = mount(cx);
        let (other, other_view) = history_session(&root, dir.path(), cx);
        cx.update(|_, cx| root.update(cx, |r, cx| {
            let spec = serde_json::from_value(serde_json::json!({
                "version":1,"request_id":"other-history","program":"/missing/program","cwd":dir.path(),
                "target":{"kind":"session","id":other},"foreground":false
            })).unwrap();
            let request = guild_session::terminal_job::TerminalCommand::Start { spec };
            assert!(matches!(r.command_terminal(&request,Some(&other.to_string()),cx),guild_session::command::Answer::Terminal { .. }));
        }));
        let (_, view) = history_session(&root, dir.path(), cx);
        frames(cx);
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        assert!(cx.debug_bounds("terminal-wider").is_some());
        cx.simulate_keystrokes("tab tab enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        frames(cx);
        assert!(
            cx.debug_bounds("terminal-wider").is_none(),
            "widening should reveal the other session's receipt"
        );
        cx.update(|w, cx| {
            let list = root.read(cx).terminal_list.as_ref().unwrap();
            assert_eq!(list.scope, guild_session::terminal_job::Scope::Worktree);
            assert!(
                list.focus.contains_focused(w, cx),
                "removed wider button leaked focus to the terminal"
            );
        });
        cx.simulate_keystrokes("escape");
        frames(cx);
        cx.update(|w, cx| {
            assert!(root.read(cx).terminal_list.is_none());
            assert!(view.read(cx).focus_handle(cx).is_focused(w));
        });
        for view in [view, other_view] {
            cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
        }
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_tab_reveals_offscreen_actions_and_keeps_header_fixed(
        cx: &mut TestAppContext,
    ) {
        let (dir, root, cx) = mount(cx);
        cx.update(|_, cx| root.update(cx, |r, cx| {
            let owner = guild_session::SessionId::new();
            let project = r.active_project.unwrap();
            for i in 0..12 {
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":format!("tab-scroll-{i}"),"program":"/bin/cat",
                    "title":format!("Job {i}"),"cwd":dir.path(),"target":{"kind":"session","id":owner}
                })).unwrap();
                r.terminal_jobs.start(owner, project, &spec, Look::of(&r.settings, r.theme), cx).unwrap();
            }
        }));
        cx.simulate_resize(gpui::size(px(360.), px(260.)));
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        let header = cx.debug_bounds("terminal-job-scope").unwrap();
        for _ in 0..12 {
            cx.simulate_keystrokes("tab");
            frames(cx);
        }
        cx.update(|_, cx| {
            assert!(
                root.read(cx)
                    .terminal_list
                    .as_ref()
                    .unwrap()
                    .scroll
                    .offset()
                    .y
                    < px(0.),
                "Tab reached offscreen actions without revealing them"
            );
        });
        assert_eq!(cx.debug_bounds("terminal-job-scope").unwrap(), header);
        for _ in 0..11 {
            cx.simulate_keystrokes("shift-tab");
            frames(cx);
        }
        cx.update(|w, cx| {
            let list = root.read(cx).terminal_list.as_ref().unwrap();
            assert_eq!(list.scroll.offset().y, px(0.));
            assert!(list.focus.contains_focused(w, cx));
        });
        cx.simulate_keystrokes("escape");
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_history_tab_reveals_each_action_when_its_row_exceeds_the_viewport(
        cx: &mut TestAppContext,
    ) {
        let (dir, root, cx) = mount(cx);
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                let owner = guild_session::SessionId::new();
                let spec = serde_json::from_value(serde_json::json!({
                    "version":1,"request_id":"tall-row-tab","program":"/bin/cat",
                    "title":"Tall row","cwd":dir.path(),"target":{"kind":"session","id":owner}
                }))
                .unwrap();
                r.terminal_jobs
                    .start(
                        owner,
                        r.active_project.unwrap(),
                        &spec,
                        Look::of(&r.settings, r.theme),
                        cx,
                    )
                    .unwrap();
            })
        });
        cx.simulate_resize(gpui::size(px(360.), px(180.)));
        cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
        frames(cx);
        let header = cx.debug_bounds("terminal-job-scope").unwrap();
        cx.simulate_keystrokes("tab");
        frames(cx);
        for (key, action) in [
            ("tab", "terminal-show"),
            ("tab", "terminal-cancel"),
            ("shift-tab", "terminal-show"),
        ] {
            cx.simulate_keystrokes(key);
            frames(cx);
            let viewport = cx.debug_bounds("terminal-job-rows").unwrap();
            let button = cx.debug_bounds(action).unwrap();
            assert!(
                button.top() >= viewport.top() && button.bottom() <= viewport.bottom(),
                "{action} remains clipped: button {button:?}, viewport {viewport:?}"
            );
            assert_eq!(cx.debug_bounds("terminal-job-scope").unwrap(), header);
            // A later Tab must preserve visibility after manual scrolling too.
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: viewport.center(),
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-10000.))),
                ..Default::default()
            });
            frames(cx);
        }
        cx.simulate_keystrokes("escape");
        stop(&root, cx);
    }

    #[gpui::test]
    #[ignore = "runs in the isolated subprocess wrapper"]
    fn terminal_context_tooltips_keep_pointer_and_scroll_to_program_cwd(cx: &mut TestAppContext) {
        let (dir, root, cx) = mount(cx);
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                let owner = guild_session::SessionId::new();
                let project = r.active_project.unwrap();
                let spec = serde_json::from_value(serde_json::json!({
                "version":1,"request_id":"long-context-tip","program":"/missing/program",
                "title":"Context tooltip","cwd":dir.path(),"target":{"kind":"session","id":owner}
            })).unwrap();
                let mut context = crate::terminal_job::launch_context(
                    &r.desk,
                    owner,
                    project,
                    dir.path(),
                    Some("main"),
                );
                context.root_path = Some(std::path::PathBuf::from(format!(
                    "/{}",
                    "long-path-component/".repeat(60)
                )));
                context.repository = context.root_path.clone();
                r.terminal_jobs
                    .start_in(
                        owner,
                        project,
                        &spec,
                        Some(context),
                        Look::of(&r.settings, r.theme),
                        cx,
                    )
                    .unwrap();
            })
        });
        cx.simulate_resize(gpui::size(px(360.), px(260.)));
        {
            let origin = "terminal-job-info";
            cx.update(|w, cx| w.simulate_mouse_move(gpui::point(px(0.), px(0.)), cx));
            frames(cx);
            cx.update(|w, cx| {
                root.update(cx, |r, cx| {
                    r.toggle_terminal_list(w, cx);
                    r.select_terminal_scope(guild_session::terminal_job::Scope::Worktree, cx);
                })
            });
            frames(cx);
            let title = cx.debug_bounds("terminal-job-title").unwrap();
            cx.update(|w, cx| w.simulate_mouse_move(title.center(), cx));
            frames(cx);
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            frames(cx);
            assert!(
                cx.debug_bounds("terminal-context-detail").is_none(),
                "title hover must stay quiet"
            );
            let bounds = cx.debug_bounds(origin).unwrap();
            let rows = cx.debug_bounds("terminal-job-rows").unwrap();
            let at = gpui::point(
                bounds.center().x,
                bounds.center().y.min(rows.bottom() - px(2.)),
            );
            cx.update(|w, cx| w.simulate_mouse_move(at, cx));
            frames(cx);
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            frames(cx);
            let tip = cx
                .debug_bounds("terminal-context-detail")
                .expect("long context tooltip opened");
            let text_before = cx.debug_bounds("terminal-context-text").unwrap();
            assert!(text_before.size.height > tip.size.height);
            let inside = gpui::point(tip.left() + px(2.), tip.top() + px(2.));
            assert!(
                !bounds.contains(&inside),
                "pointer must leave the tooltip origin"
            );
            cx.update(|w, cx| w.simulate_mouse_move(inside, cx));
            frames(cx);
            cx.executor()
                .advance_clock(std::time::Duration::from_secs(1));
            frames(cx);
            assert!(
                cx.debug_bounds("terminal-context-detail").is_some(),
                "{origin}: pointer entering tooltip dismissed it"
            );
            cx.simulate_event(gpui::ScrollWheelEvent {
                position: inside,
                delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-10000.))),
                ..Default::default()
            });
            frames(cx);
            let text_after = cx.debug_bounds("terminal-context-text").unwrap();
            assert!(text_after.top() < text_before.top());
            assert!(
                text_after.bottom() <= tip.bottom(),
                "program cwd at the end remains clipped"
            );
            cx.simulate_keystrokes("escape");
            frames(cx);
        }
        stop(&root, cx);
    }
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn codex_queued_hooks_and_disk_completion_keep_exact_request(cx: &mut TestAppContext) {
    use guild_session::status::{Status, Why};
    use guild_session::{Runs, SessionMode, SessionRecord};
    use std::io::{Read, Write};
    let (dir, root, cx) = mount(cx);
    let conversation = "11111111-2222-3333-4444-555555555555";
    let home = dir.path().join("codex");
    let rollouts = home.join("sessions/2026/10/13");
    std::fs::create_dir_all(&rollouts).unwrap();
    let rollout = rollouts.join(format!("rollout-fixture-{conversation}.jsonl"));
    std::fs::write(
        &rollout,
        format!(
            "{}\n",
            serde_json::json!({"type":"session_meta","payload":{"id":conversation,"source":"cli"}})
        ),
    )
    .unwrap();
    let (session, socket) = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let seat = guild_session::Seat {
                workspace: r.active_workspace,
                project,
            };
            let session = r.desk.add_session(SessionRecord::new(
                "Codex fixture".into(),
                seat,
                dir.path().into(),
                Runs::Codex(SessionMode::Terminal),
            ));
            r.desk.record_harness_session(session, conversation.into());
            let view = r.active_panel_terminal().unwrap().clone();
            view.update(cx, |v, _| v.attach_test_codex_probe(home.clone()));
            r.adopt(
                seat,
                project,
                crate::Body::Terminal {
                    session,
                    facts: view.read(cx).facts().clone(),
                    view,
                },
                cx,
            );
            r.active_tab = None;
            r.window_focused = false;
            r.attach_test_hooks(dir.path());
            // GLD-078: a diagnostic window's launches write no agent's
            // global door — only a production window's may.
            assert_eq!(r.hook_launch().unwrap().doors, None);
            (session, r.hook_launch().unwrap().socket)
        })
    });
    let post = |event: &str,
                turn: &str,
                id: Option<&str>,
                command: &str,
                native: &str,
                agent: Option<&str>| {
        let mut payload = serde_json::json!({"session_id":session.to_string(),"guild_session":session.to_string(),"codex_session":native,"turn_id":turn,"hook_event_name":event,"tool_name":"Bash","tool_input":{"command":command}});
        if let Some(id) = id {
            payload["tool_use_id"] = id.into();
        }
        if let Some(agent) = agent {
            payload["agent_id"] = agent.into();
        }
        let payload = payload.to_string();
        let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(3)))
            .unwrap();
        write!(
            stream,
            "POST /hook HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
            payload.len(),
            payload
        )
        .unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        assert!(response.contains("204"));
    };
    let append = |payload: serde_json::Value| {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rollout)
            .unwrap();
        writeln!(
            file,
            "{}",
            serde_json::json!({"type":"event_msg","payload":payload})
        )
        .unwrap();
    };
    let poll = |expected, cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_status(cx);
                assert_eq!(r.latch.status(session), expected);
                assert_eq!(
                    r.desk.session(session).unwrap().harness_session.as_deref(),
                    Some(conversation)
                );
            })
        })
    };
    append(serde_json::json!({"type":"task_started","turn_id":"turn-one"}));
    post(
        "PreToolUse",
        "turn-one",
        Some("exec-one"),
        "cargo check",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-one",
        None,
        "cargo check",
        conversation,
        None,
    );
    // The command is already done on disk while the permission hook is queued.
    append(
        serde_json::json!({"type":"item_completed","thread_id":conversation,"turn_id":"turn-one","item":{"type":"CommandExecution","id":"exec-one","status":"failed","exit_code":101}}),
    );
    poll(Status::Working, cx);
    post("Stop", "turn-one", None, "", conversation, None);
    poll(Status::Working, cx);
    post(
        "PreToolUse",
        "turn-one",
        Some("exec-two"),
        "cargo test",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-one",
        None,
        "cargo test",
        conversation,
        None,
    );
    poll(Status::Waiting(Why::Permission), cx);
    post(
        "PostToolUse",
        "turn-one",
        Some("parallel"),
        "pwd",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-child",
        None,
        "child",
        "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
        Some("child"),
    );
    poll(Status::Waiting(Why::Permission), cx);
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            r.active_tab = r
                .tabs
                .iter()
                .find(|tab| tab.sessions().any(|id| id == session))
                .map(|tab| tab.id);
            r.window_focused = true;
        })
    });
    poll(Status::Waiting(Why::Permission), cx); // Opening/seeing answers nothing.
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            r.window_focused = false;
        })
    });
    poll(Status::Waiting(Why::Permission), cx); // Silence answers nothing.
    post("Stop", "turn-one", None, "", conversation, None);
    poll(Status::Waiting(Why::Permission), cx);
    post(
        "PreToolUse",
        "turn-one",
        Some("exec-three"),
        "continued",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-one",
        None,
        "continued",
        conversation,
        None,
    );
    poll(Status::Waiting(Why::Permission), cx);
    append(serde_json::json!({"type":"task_complete","turn_id":"turn-one"}));
    poll(Status::Done, cx);
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            r.latch.apply(session, guild_session::status::Signal::Seen);
        })
    });
    post("Stop", "turn-one", None, "", conversation, None);
    post(
        "PermissionRequest",
        "turn-one",
        None,
        "cargo test",
        conversation,
        None,
    );
    poll(Status::Idle, cx); // Delayed hooks cannot recreate unread/waiting.
    // A background automatic-review turn must not forget its policy when
    // output grows beyond the old tail search. The emitting tab is unfocused.
    append(serde_json::json!({"type":"task_started","turn_id":"turn-auto"}));
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rollout)
            .unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"turn_context","payload":{"turn_id":"turn-auto","approvals_reviewer":"auto_review"}})).unwrap();
    }
    post(
        "UserPromptSubmit",
        "turn-auto",
        None,
        "",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-auto",
        None,
        "first",
        conversation,
        None,
    );
    poll(Status::Working, cx);
    {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rollout)
            .unwrap();
        writeln!(file, "{}", serde_json::json!({"type":"response_item","payload":{"output":"x".repeat(2 * 1024 * 1024)}})).unwrap();
    }
    post(
        "PreToolUse",
        "turn-auto",
        Some("exec-large"),
        "second",
        conversation,
        None,
    );
    post(
        "PermissionRequest",
        "turn-auto",
        None,
        "second",
        conversation,
        None,
    );
    poll(Status::Working, cx);
    append(serde_json::json!({"type":"task_complete","turn_id":"turn-auto"}));
    poll(Status::Done, cx);

    // An in-TUI switch names the exact new conversation and current turn.
    let other = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    let records = [
        serde_json::json!({"type":"session_meta","payload":{"id":other}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"historical"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"task_complete","turn_id":"historical"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"task_started","turn_id":"resumed"}}),
        serde_json::json!({"type":"event_msg","payload":{"type":"item_completed","thread_id":other,"turn_id":"resumed","item":{"type":"CommandExecution","id":"exec-resumed","status":"failed","exit_code":101}}}),
    ];
    std::fs::write(
        rollouts.join(format!("rollout-fixture-{other}.jsonl")),
        records.iter().map(|v| format!("{v}\n")).collect::<String>(),
    )
    .unwrap();
    post("UserPromptSubmit", "resumed", None, "", other, None);
    post(
        "PreToolUse",
        "resumed",
        Some("exec-resumed"),
        "failed",
        other,
        None,
    );
    post("PermissionRequest", "resumed", None, "failed", other, None);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.poll_status(cx);
            r.poll_status(cx);
            assert_eq!(r.latch.status(session), Status::Working);
            assert_eq!(
                r.desk.session(session).unwrap().harness_session.as_deref(),
                Some(other)
            );
        })
    });
    stop(&root, cx);
}
#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn codex_interrupt_and_hook_prompt_keep_source_order(cx: &mut TestAppContext) {
    use guild_session::status::Status;
    use guild_session::{Runs, SessionMode, SessionRecord};
    use std::io::{Read, Write};
    for (interrupt_first, completion_queued, prompt_hook) in [
        (true, false, true),
        (false, false, true),
        (true, true, true),
        (false, true, true),
        (false, false, false),
        (false, true, false),
    ] {
        let (dir, root, cx) = mount(cx);
        let (session, socket) = cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                let seat = guild_session::Seat {
                    workspace: r.active_workspace,
                    project,
                };
                let session = r.desk.add_session(SessionRecord::new(
                    "Fixture".into(),
                    seat,
                    dir.path().into(),
                    Runs::Codex(SessionMode::Terminal),
                ));
                let conversation = "11111111-2222-3333-4444-555555555555";
                let home = dir.path().join("codex");
                let rollouts = home.join("sessions/2026/10/13");
                std::fs::create_dir_all(&rollouts).unwrap();
                std::fs::write(
                    rollouts.join(format!("rollout-{conversation}.jsonl")),
                    format!(
                        "{}\n",
                        serde_json::json!({"type":"session_meta","payload":{"id":conversation}})
                    ),
                )
                .unwrap();
                r.desk.record_harness_session(session, conversation.into());
                let view = r.active_panel_terminal().unwrap().clone();
                view.update(cx, |v, _| v.attach_test_codex_probe(home));
                r.adopt(
                    seat,
                    project,
                    crate::Body::Terminal {
                        session,
                        facts: view.read(cx).facts().clone(),
                        view: view.clone(),
                    },
                    cx,
                );
                r.active_tab = None;
                view.read(cx).focus_handle(cx).focus(window, cx);
                r.window_focused = false;
                r.latch
                    .apply(session, guild_session::status::Signal::PromptSubmitted);
                r.attach_test_hooks(dir.path());
                (session, r.hook_launch().unwrap().socket)
            })
        });
        let post = |event: &str| {
            let payload =
                serde_json::json!({"hook_event_name":event, "session_id":session.to_string(),"guild_session":session.to_string(),"codex_session":"11111111-2222-3333-4444-555555555555","turn_id":"new"})
                    .to_string();
            let mut stream = std::os::unix::net::UnixStream::connect(&socket).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            write!(
                stream,
                "POST /hook HTTP/1.1\r\nContent-Length: {}\r\n\r\n{}",
                payload.len(),
                payload
            )
            .unwrap();
            let mut response = String::new();
            stream.read_to_string(&mut response).unwrap();
            assert!(response.contains("204"));
        };
        let append = |kind: &str| {
            let file = dir.path().join(
                "codex/sessions/2026/10/13/rollout-11111111-2222-3333-4444-555555555555.jsonl",
            );
            let mut file = std::fs::OpenOptions::new().append(true).open(file).unwrap();
            writeln!(
                file,
                "{}",
                serde_json::json!({"type":"event_msg","payload":{"type":kind,"turn_id":"new"}})
            )
            .unwrap();
        };
        frames(cx);
        // Prime the exact cursor before the next disk-only turn starts.
        cx.update(|_, cx| root.update(cx, |r, cx| r.poll_status(cx)));
        if !prompt_hook {
            append("task_started");
        }
        if interrupt_first {
            cx.simulate_keystrokes("escape");
        }
        if prompt_hook {
            post("UserPromptSubmit");
        }
        if !interrupt_first {
            cx.simulate_keystrokes("escape");
        }
        if prompt_hook {
            append("task_started");
        }
        if completion_queued {
            append("task_complete");
        }
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_status(cx);
                assert_eq!(
                    r.latch.status(session),
                    if interrupt_first && completion_queued {
                        Status::Done
                    } else if interrupt_first {
                        Status::Working
                    } else {
                        Status::Idle
                    }
                );
            })
        });
        if !completion_queued {
            append("task_complete");
        }
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.poll_status(cx);
                assert_eq!(
                    r.latch.status(session),
                    if interrupt_first {
                        Status::Done
                    } else {
                        Status::Idle
                    }
                );
            })
        });
        if !prompt_hook {
            let file = dir.path().join(
                "codex/sessions/2026/10/13/rollout-11111111-2222-3333-4444-555555555555.jsonl",
            );
            let mut file = std::fs::OpenOptions::new().append(true).open(file).unwrap();
            for kind in ["task_started", "task_complete"] {
                writeln!(file, "{}", serde_json::json!({"type":"event_msg","payload":{"type":kind,"turn_id":"after-interrupt"}})).unwrap();
                cx.update(|_, cx| {
                    root.update(cx, |r, cx| {
                        r.poll_status(cx);
                        assert_eq!(
                            r.latch.status(session),
                            if kind == "task_started" {
                                Status::Working
                            } else {
                                Status::Done
                            }
                        );
                    })
                });
            }
        }
        stop(&root, cx);
    }
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn terminal_history_unavailable_receipt_keeps_archive_without_output_controls(
    cx: &mut TestAppContext,
) {
    use guild_session::{
        SessionId,
        terminal_job::{Book, JobState, RevmuxArchive, Scope},
    };
    let (dir, root, cx) = mount(cx);
    let before = cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            let owner = SessionId::new();
            let project = r.active_project.unwrap();
            let spec = serde_json::from_value(serde_json::json!({
                "version":1,"request_id":"unavailable-card","program":"/bin/echo",
                "title":"Saved run","cwd":dir.path(),"target":{"kind":"session","id":owner}
            }))
            .unwrap();
            let mut book = Book::default();
            let context = crate::terminal_job::launch_context(
                &r.desk,
                owner,
                project,
                dir.path(),
                Some("main"),
            );
            let id = *book
                .reserve_in(owner, project, &spec, Some(context))
                .unwrap()
                .job_id();
            let receipt = book.get_mut(&id).unwrap();
            receipt.state = JobState::Exited;
            receipt.revmux_archive = Some(Box::new(RevmuxArchive {
                directory: dir.path().to_owned(),
                program: "/bin/echo".into(),
            }));
            let file = dir.path().join("saved-jobs.json");
            std::fs::write(
                &file,
                serde_json::to_vec(&serde_json::json!({"jobs":book.receipts()})).unwrap(),
            )
            .unwrap();
            r.terminal_jobs = crate::terminal_job::LiveJobs::load(file);
            let before = r.terminal_jobs.receipts();
            r.toggle_terminal_list(w, cx);
            r.select_terminal_scope(Scope::Worktree, cx);
            before
        })
    });
    frames(cx);
    assert!(
        cx.debug_bounds("terminal-job-title").is_some(),
        "receipt remains visible"
    );
    assert!(
        cx.debug_bounds("terminal-open-run").is_some(),
        "archive recovery is independent of output"
    );
    assert!(cx.debug_bounds("terminal-show").is_none());
    assert!(cx.debug_bounds("terminal-release").is_none());
    assert!(
        cx.debug_bounds("terminal-output-explanation").is_none(),
        "unavailable output must not add a history explanation"
    );
    open_terminal_info(cx, None);
    let worktree = cx.update(|_, cx| {
        let r = root.read(cx);
        format!(
            "terminal-context:{}: {}",
            r.ui().text(crate::i18n::Text::TerminalContextWorktree),
            r.terminal_list
                .as_ref()
                .unwrap()
                .labels
                .root_label
                .as_deref()
                .unwrap()
        )
    });
    let worktree: &'static str = Box::leak(worktree.into_boxed_str());
    assert!(
        cx.debug_bounds(worktree).is_none(),
        "matching worktree is already identified in the header"
    );
    cx.update(|_, cx| root.update(cx, |r, cx| r.select_terminal_scope(Scope::Project, cx)));
    frames(cx);
    open_terminal_info(cx, None);
    assert!(
        cx.debug_bounds(worktree).is_some(),
        "wider scope retains original worktree"
    );
    cx.simulate_keystrokes("tab tab");
    frames(cx);
    cx.update(|w, cx| {
        assert!(
            root.read(cx)
                .terminal_list
                .as_ref()
                .unwrap()
                .focus
                .contains_focused(w, cx)
        )
    });
    cx.simulate_keystrokes("escape");
    frames(cx);
    cx.update(|_, cx| assert_eq!(root.read(cx).terminal_jobs.receipts(), before));
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn terminal_history_keeps_distinct_and_unknown_origin_in_both_languages(cx: &mut TestAppContext) {
    use guild_session::{
        SessionId,
        settings::Language,
        terminal_job::{Book, JobState, Scope},
    };
    let (dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.select_panel_terminal(r.active_project.unwrap(), 0, w, cx)
        })
    });
    frames(cx);
    assert_terminal_focus(&root, cx);
    for language in [Language::English, Language::Russian] {
        for origin in ["matching", "different-path", "missing-path"] {
            let selectors = cx.update(|w, cx| {
                root.update(cx, |r, cx| {
                    r.settings.language = language;
                    if r.terminal_list.is_some() {
                        r.close_terminal_list(crate::root::picker::Focus::Back, w, cx);
                    }
                    let owner = SessionId::new();
                    let project = r.active_project.unwrap();
                    let spec = serde_json::from_value(serde_json::json!({
                        "version":1,"request_id":origin,"program":"/bin/echo",
                        "cwd":dir.path(),"target":{"kind":"session","id":owner}
                    }))
                    .unwrap();
                    let mut context = crate::terminal_job::launch_context(
                        &r.desk,
                        owner,
                        project,
                        dir.path(),
                        Some("main"),
                    );
                    if origin == "different-path" {
                        context.root_path = Some(dir.path().join("different"));
                    }
                    if origin == "missing-path" {
                        context.root_path = None;
                    }
                    let root_label = context.root_label.clone().unwrap();
                    let mut book = Book::default();
                    let id = *book
                        .reserve_in(owner, project, &spec, Some(context))
                        .unwrap()
                        .job_id();
                    book.get_mut(&id).unwrap().state = JobState::Exited;
                    let file = dir.path().join("origin-jobs.json");
                    std::fs::write(
                        &file,
                        serde_json::to_vec(&serde_json::json!({"jobs":book.receipts()})).unwrap(),
                    )
                    .unwrap();
                    r.terminal_jobs = crate::terminal_job::LiveJobs::load(file);
                    r.toggle_terminal_list(w, cx);
                    r.select_terminal_scope(Scope::Worktree, cx);
                    let ui = r.ui();
                    let label = &root_label;
                    [
                        format!(
                            "terminal-context:{}: {label}",
                            ui.text(crate::i18n::Text::TerminalContextWorktree)
                        ),
                        format!(
                            "terminal-context:{}: {}",
                            ui.text(crate::i18n::Text::TerminalContextSession),
                            ui.text(crate::i18n::Text::TerminalUnknownSession)
                        ),
                    ]
                })
            });
            frames(cx);
            open_terminal_info(cx, None);
            let worktree: &'static str = Box::leak(selectors[0].clone().into_boxed_str());
            assert_eq!(
                cx.debug_bounds(worktree).is_some(),
                origin != "matching",
                "{language:?}: {origin}"
            );
            let session: &'static str = Box::leak(selectors[1].clone().into_boxed_str());
            assert!(
                cx.debug_bounds(session).is_some(),
                "session remains labelled"
            );
            cx.update(|_, cx| root.update(cx, |r, cx| r.select_terminal_scope(Scope::All, cx)));
            frames(cx);
            open_terminal_info(cx, None);
            assert!(
                cx.debug_bounds(worktree).is_some(),
                "All launches retains original worktree"
            );
            cx.simulate_keystrokes("escape");
            frames(cx);
            assert_terminal_focus(&root, cx);
        }
    }
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn terminal_history_empty_and_deleted_output_keep_their_explanation(cx: &mut TestAppContext) {
    use crate::terminal_job::OutputState;
    let (dir, root, cx) = mount(cx);
    frames(cx);
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.select_panel_terminal(r.active_project.unwrap(), 0, w, cx)
        })
    });
    frames(cx);
    assert_terminal_focus(&root, cx);
    let job = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let owner = guild_session::SessionId::new();
            let project = r.active_project.unwrap();
            let spec = serde_json::from_value(serde_json::json!({
                "version":1,"request_id":"empty-card","program":"/usr/bin/true",
                "cwd":dir.path(),"target":{"kind":"session","id":owner}
            }))
            .unwrap();
            let context =
                crate::terminal_job::launch_context(&r.desk, owner, project, dir.path(), None);
            r.terminal_jobs
                .start_in(
                    owner,
                    project,
                    &spec,
                    Some(context),
                    Look::of(&r.settings, r.theme),
                    cx,
                )
                .unwrap()
                .id
        })
    });
    let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        let done = cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.terminal_jobs.poll(cx);
                r.terminal_jobs.output_state(&job, cx) == Some(OutputState::Empty)
            })
        });
        if done {
            break;
        }
        assert!(
            std::time::Instant::now() < until,
            "empty process did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    cx.update(|w, cx| root.update(cx, |r, cx| r.toggle_terminal_list(w, cx)));
    frames(cx);
    assert!(cx.debug_bounds("terminal-output-explanation").is_some());
    assert!(cx.debug_bounds("terminal-show").is_none());
    assert!(cx.debug_bounds("terminal-release").is_some());
    let note = cx.debug_bounds("terminal-output-explanation").unwrap();
    let delete = cx.debug_bounds("terminal-release").unwrap();
    assert!(
        (note.center().y - delete.center().y).abs() <= px(1.),
        "empty-output state and actions share the compact footer"
    );
    let title = cx.debug_bounds("terminal-job-title").unwrap();
    for reduced in [false, true] {
        cx.update(|w, cx| {
            cx.set_reduce_motion(reduced);
            w.simulate_mouse_move(gpui::point(px(0.), px(0.)), cx);
        });
        frames(cx);
        cx.update(|w, cx| w.simulate_mouse_move(title.center(), cx));
        frames(cx);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(1));
        frames(cx);
        assert_eq!(
            cx.debug_bounds("terminal-release").unwrap(),
            delete,
            "hover must not move action targets, reduced motion={reduced}"
        );
    }
    cx.update(|_, cx| cx.set_reduce_motion(false));
    let release = cx.debug_bounds("terminal-release").unwrap();
    cx.simulate_click(release.center(), gpui::Modifiers::default());
    frames(cx);
    assert!(cx.debug_bounds("terminal-output-explanation").is_some());
    assert!(cx.debug_bounds("terminal-release").is_none());
    assert!(cx.debug_bounds("terminal-show").is_none());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).terminal_jobs.output_state(&job, cx),
            Some(OutputState::Deleted)
        )
    });
    cx.simulate_keystrokes("escape");
    frames(cx);
    assert_terminal_focus(&root, cx);
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn terminal_history_preserves_legacy_summary_and_completed_grouping(cx: &mut TestAppContext) {
    use guild_session::{
        SessionId,
        settings::Language,
        terminal_job::{Book, JobState, Scope},
    };
    let (dir, root, cx) = mount(cx);
    for language in [Language::English, Language::Russian] {
        let (legacy, dated, before, summaries) = cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                r.settings.language = language;
                let owner = SessionId::new();
                let project = r.active_project.unwrap();
                let mut book = Book::default();
                let mut ids = Vec::new();
                for (request, has_context) in [("legacy-summary", false), ("dated-summary", true)] {
                    let spec = serde_json::from_value(serde_json::json!({
                        "version":1,"request_id":request,"program":"/bin/echo",
                        "cwd":dir.path(),"target":{"kind":"session","id":owner}
                    }))
                    .unwrap();
                    let context = crate::terminal_job::launch_context(
                        &r.desk,
                        owner,
                        project,
                        dir.path(),
                        Some("main"),
                    );
                    let id = *book
                        .reserve_in(owner, project, &spec, has_context.then_some(context))
                        .unwrap()
                        .job_id();
                    let receipt = book.get_mut(&id).unwrap();
                    receipt.state = JobState::Exited;
                    receipt.started_at_ms = has_context.then_some(1000);
                    ids.push(id);
                }
                let file = dir.path().join("legacy-jobs.json");
                std::fs::write(
                    &file,
                    serde_json::to_vec(&serde_json::json!({"jobs":book.receipts()})).unwrap(),
                )
                .unwrap();
                r.terminal_jobs = crate::terminal_job::LiveJobs::load(file);
                let before = r.terminal_jobs.receipts();
                r.toggle_terminal_list(w, cx);
                let ui = r.ui();
                let session = ui.text(crate::i18n::Text::TerminalUnknownSession);
                let root = ui.text(crate::i18n::Text::TerminalUnknownRoot);
                let repo = ui.text(crate::i18n::Text::TerminalUnknownRepository);
                let branch = ui.text(crate::i18n::Text::TerminalUnknownBranch);
                (
                    ids[0],
                    ids[1],
                    before,
                    [
                        (Scope::Worktree, vec![session.to_owned(), branch.to_owned()]),
                        (
                            Scope::Project,
                            vec![format!("{root} · {branch}"), session.to_owned()],
                        ),
                        (
                            Scope::All,
                            vec![format!("{repo} / {root} · {branch}"), session.to_owned()],
                        ),
                    ],
                )
            })
        });
        for (scope, lines) in summaries {
            cx.update(|_, cx| root.update(cx, |r, cx| r.select_terminal_scope(scope, cx)));
            frames(cx);
            open_terminal_info(cx, Some(legacy));
            for line in lines {
                let selector: &'static str =
                    Box::leak(format!("terminal-context:{line}").into_boxed_str());
                assert!(
                    cx.debug_bounds(selector).is_some(),
                    "{language:?} {scope:?}: existing legacy text changed"
                );
            }
            cx.update(|_, cx| {
                let r = root.read(cx);
                let receipts = r.terminal_jobs.receipts();
                assert_eq!(receipts, before, "scope does not rewrite legacy metadata");
                let context = r.terminal_history_context();
                let groups = context.select(scope, &receipts);
                assert!(groups.ongoing.is_empty());
                assert_eq!(
                    groups
                        .completed
                        .iter()
                        .map(|receipt| receipt.id)
                        .collect::<Vec<_>>(),
                    [dated, legacy],
                    "unknown-time legacy receipt stays after dated jobs in Completed"
                );
            });
        }
        cx.simulate_keystrokes("escape");
        frames(cx);
    }
    stop(&root, cx);
}

#[gpui::test]
#[ignore = "runs in the isolated subprocess wrapper"]
fn review_activity_schema_reaches_production_card_and_accessible_details(cx: &mut TestAppContext) {
    use guild_session::terminal_job::{TerminalCommand, TerminalReply};
    let (dir, root, cx) = mount(cx);
    let (owner, view) = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.history_session_fixture(dir.path(), window, cx)
        })
    });
    let archive = dir.path().join("review");
    std::fs::create_dir_all(archive.join("revust")).unwrap();
    std::fs::create_dir(archive.join("stages")).unwrap();
    let receipt=cx.update(|_,cx|root.update(cx,|r,cx|{
        let spec=serde_json::from_value(serde_json::json!({"version":1,"request_id":"review-card","program":"/bin/cat","cwd":dir.path(),"title":"Review keyboard ownership","target":{"kind":"session","id":owner},"archive_viewer":{"directory":archive,"program":"/fixture/revust"}})).unwrap();
        match r.command_terminal(&TerminalCommand::Start{spec},Some(&owner.to_string()),cx){guild_session::command::Answer::Terminal{reply:TerminalReply::Job{receipt}}=>*receipt,other=>panic!("{other:?}")}
    }));
    std::fs::write(
        archive.join("revust/run.json"),
        r#"{"version":1,"identity":{"task":"focus","run":"one"}}"#,
    )
    .unwrap();
    std::fs::write(archive.join("manifest.json"), "").unwrap();
    std::fs::write(archive.join("events.jsonl"),"{\"kind\":\"stage\",\"stage\":\"find\"}\n{\"kind\":\"agent_progress\",\"text\":\"rg focus src\"}\n").unwrap();
    std::fs::write(archive.join("stages/1-found.json"),serde_json::json!({"scope":{"task":"","run":""},"findings":[{"id":"focus-1","severity":"major","title":"Keyboard focus leaks behind popup","body":"Restore focus to the invoking pane.","file":"src/focus.rs","line":42},{"id":"rejected-1","severity":"minor","verdict":"rejected","title":"Rejected keyboard candidate","body":"Existing focus guard handles this case.","file":"src/guard.rs","line":7}],"open_questions":[{"id":"question-1","severity":"minor","title":"Confirm IME focus ownership","body":"Observe the native composition boundary.","file":"src/ime.rs","line":9}]}).to_string()).unwrap();
    let snapshot = crate::review_activity::fixture_snapshot(&receipt, dir.path().join("pins.json"));
    assert_eq!(snapshot.counts(), (1, 0, 0));
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.review_activity.snapshots.insert(receipt.id, snapshot);
            r.toggle_terminal_list(window, cx);
            cx.notify();
        })
    });
    frames(cx);
    assert!(cx.debug_bounds("review-running-spinner").is_some());
    assert!(cx.debug_bounds("review-counts").is_some());
    assert!(cx.debug_bounds("review-finding-details").is_none());
    let button = cx.debug_bounds("review-details").unwrap();
    cx.simulate_click(button.center(), gpui::Modifiers::default());
    frames(cx);
    assert!(cx.debug_bounds("review-finding-details").is_some());
    assert!(cx.debug_bounds("review-category-Rejected").is_some());
    assert!(cx.debug_bounds("review-category-OpenQuestion").is_some());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.terminal_list.as_ref().unwrap().focus.focus(window, cx)
        })
    });
    let mut reached = false;
    for _ in 0..16 {
        cx.simulate_keystrokes("tab");
        frames(cx);
        reached = cx.update(|window, cx| {
            root.read(cx)
                .terminal_list
                .as_ref()
                .unwrap()
                .actions
                .borrow()
                .get(&(receipt.id, "findings"))
                .unwrap()
                .focus
                .contains_focused(window, cx)
        });
        if reached {
            break;
        }
    }
    assert!(reached, "Tab reaches the Findings details button");
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    frames(cx);
    assert!(
        cx.debug_bounds("review-finding-details").is_none(),
        "Details has a keyboard equivalent"
    );
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let data = r.review_activity.snapshots.get_mut(&receipt.id).unwrap();
            data.final_result = true;
            data.assessed = true;
            cx.notify();
        })
    });
    frames(cx);
    assert!(cx.debug_bounds("review-running-spinner").is_none());
    cx.simulate_keystrokes("escape");
    frames(cx);
    cx.update(|_, cx| root.update(cx, |r, _| assert!(r.terminal_list.is_none())));
    cx.update(|_, cx| view.update(cx, |v, cx| v.hang_up_for_quit(cx)));
    stop(&root, cx);
}
