use super::*;
use gpui::{AppContext, TestAppContext};

// Disk completion creates the editor; its next display frame lands the caret.
fn settle_file_frames(visual: &mut gpui::VisualTestContext) {
    visual.run_until_parked();
    for _ in 0..3 {
        visual.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
        visual.run_until_parked();
    }
}

#[gpui::test]
#[ignore = "run through isolated_navigation_window_regressions"]
fn workspace_switch_records_the_empty_workspace(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let first = desk.active_workspace();
        let second = desk.add_workspace("Second", None).unwrap();
        let root = cx.new(|cx| Root::from_state(Default::default(), desk, cx));
        root.update(cx, |root, cx| {
            root.restored_workspaces.extend([first, second]);
            root.note_place(cx);
            root.select_workspace(second, cx);
            root.note_place(cx);
            assert!(
                root.history_directions().0,
                "a switch from an empty workspace must leave a Back entry"
            );
            assert_eq!(root.history.back.last().unwrap().workspace, first);
            let count = root.history.back.len();
            root.note_place(cx);
            root.note_place(cx);
            assert_eq!(
                root.history.back.len(),
                count,
                "an unchanged render sample records nothing"
            );
        });
    });
}

#[test]
fn isolated_navigation_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::history::surface_tests::",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NAVIGATION_TEST", "1")
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct Guard {
    terminals:
        std::rc::Rc<std::cell::RefCell<Vec<gpui::Entity<crate::terminal::view::TerminalTab>>>>,
    cx: TestAppContext,
}
impl Drop for Guard {
    fn drop(&mut self) {
        // A failing assertion can unwind while Root is leased. Own the actual
        // child views so cleanup never tries to lease Root a second time.
        self.cx.update(|cx| {
            for terminal in self.terminals.borrow_mut().drain(..) {
                terminal.update(cx, |terminal, cx| terminal.hang_up_for_quit(cx));
            }
        });
    }
}

fn session(
    root: &mut Root,
    project: ProjectId,
    cx: &mut Context<Root>,
) -> (SessionId, crate::TabId) {
    let seat = root.seat_for_project(project);
    let record = guild_session::SessionRecord::new(
        "Fixture".into(),
        seat,
        root.desk.project(project).unwrap().path.clone(),
        guild_session::Runs::Shell,
    );
    let id = root.desk.add_session(record);
    let tab = root.adopt(
        seat,
        project,
        Body::Broken {
            session: id,
            reason: "fixture".into(),
        },
        cx,
    );
    (id, tab)
}

#[gpui::test]
#[ignore = "run through isolated_navigation_window_regressions"]
fn navigation_window(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    assert_eq!(std::env::var("GUILD_NAVIGATION_TEST").as_deref(), Ok("1"));
    let dirs = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    std::fs::write(dirs[0].path().join("a.txt"), "line\n".repeat(350)).unwrap();
    std::fs::write(dirs[0].path().join("b.txt"), "line\n".repeat(350)).unwrap();
    let cleanup_context = cx.clone();
    let mut entity = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dirs[0].path());
        let other = desk.open_in(workspace, dirs[1].path());
        let vacant = desk.add_workspace("Vacant", None).unwrap();
        desk.open_in(vacant, dirs[1].path());
        desk.select_workspace(workspace);
        desk.look_at(Some(project), None);
        let root = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.isolated_render = true;
            root.restored_workspaces.insert(workspace);
            let (_, tab) = session(&mut root, other, cx);
            root.stage.bring_forward(
                root.seat_for_project(other),
                root.tabs.iter().find(|t| t.id == tab).unwrap().area,
                tab,
            );
            root.history = History::default();
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    let guarded_terminals = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let _guard = Guard {
        terminals: guarded_terminals.clone(),
        cx: cleanup_context,
    };
    visual.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
    settle_file_frames(visual);
    visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.go_to_place(
                r.active_project.unwrap(),
                Place::new("a.txt", 120, 0),
                window,
                cx,
            );
        })
    });
    settle_file_frames(visual);
    let (project, workspace, other, a, b, tb, empty) = visual.update(|window,cx| root.update(cx, |r,cx| {
        let project = r.active_project.unwrap();
        let workspace = r.active_workspace;
        let other = r.desk.projects_of(workspace).find(|p| p.id != project).unwrap().id;
        // File -> harness/terminal -> Back -> same caret; Forward is its undo.
        let (a,ta) = session(r,project,cx);
        r.show_tab(ta,cx);
        r.go_back(window,cx);
        assert_eq!(r.caret_now(cx).unwrap().1.line,120);
        r.go_forward(window,cx);
        assert_eq!(r.active_session(),Some(a));
        let (b,tb) = session(r,project,cx);
        r.show_tab(tb,cx);
        r.go_back(window,cx); assert_eq!(r.active_session(),Some(a));
        r.go_forward(window,cx); assert_eq!(r.active_session(),Some(b));
        r.step_tab(false,cx); r.go_back(window,cx); assert_eq!(r.active_session(),Some(b));
        r.go_to_session_in_workspace(a,cx); r.go_back(window,cx); assert_eq!(r.active_session(),Some(b));
        // A worktree row uses the same seat-selection door as the real column.
        r.choose_row(&crate::column::Row::Opened { project: other, title: crate::column::Title { name: "Other".into(), primary: false }, branch: None,committed: None,lines: None }, cx); r.go_back(window,cx);
        assert_eq!(r.active_project,Some(project)); assert_eq!(r.active_session(),Some(b));
        r.go_forward(window,cx); assert_eq!(r.active_project,Some(other));
        r.go_back(window,cx);
        // A no-project workspace and a project with no tabs are both targets.
        let empty = r.desk.add_workspace("Empty",None).unwrap();
        r.restored_workspaces.insert(empty);
        r.select_workspace(empty,cx); r.go_back(window,cx);
        assert_eq!(r.active_workspace,workspace); assert_eq!(r.active_session(),Some(b));
        r.go_forward(window,cx); assert_eq!(r.active_workspace,empty); assert_eq!(r.active_project,None);
        r.go_back(window,cx);
        let vacant = r.desk.workspaces().iter().find(|w| w.name == "Vacant").unwrap().id;
        r.restored_workspaces.insert(vacant);
        r.select_workspace(vacant,cx);
        assert_eq!(r.active_project,Some(other)); assert_eq!(r.active_tab,None);
        r.go_back(window,cx); assert_eq!(r.active_workspace,workspace);
        r.go_forward(window,cx); assert_eq!(r.active_workspace,vacant);assert_eq!(r.active_tab,None);
        r.go_back(window,cx);
        // Two guild selections are separate steps even without a paint between.
        let third = r.desk.add_workspace("Third",None).unwrap();
        r.restored_workspaces.insert(third);
        for target in [empty,third] {
            let (answer,reply) = std::sync::mpsc::channel();
            r.take_command(crate::hooks::server::Asked { command: guild_session::command::Command::WorkspaceSelect { workspace: target.to_string() }, answer }, cx);
            assert!(matches!(reply.recv().unwrap(), guild_session::command::Answer::WorkspaceSelected { .. }));
        }
        r.go_back(window,cx); assert_eq!(r.active_workspace,empty);
        r.go_back(window,cx); assert_eq!(r.active_workspace,workspace);
        // Closed sessions are skipped; closed files reopen by their path.
        r.go_to_place(project,Place::new("a.txt",120,0),window,cx);
        let file = r.active_tab.unwrap();
        r.show_tab(ta,cx); r.show_tab(tb,cx); r.close_tab(ta,cx);
        r.close_tab(file,cx);
        r.go_back(window,cx);
        assert!(matches!(r.active_body(),Some(Body::File { path, origin: crate::Origin::Project, .. }) if path == std::path::Path::new("a.txt")));
        (project, workspace, other, a, b, tb, empty)
    }));
    settle_file_frames(visual);
    visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            // A restored editor lands before it can be sampled at 0:0.
            let path = "b.txt".into();
            r.restore_file_tab(project, dirs[0].path(), path, cx);
            r.note_place(cx);
            r.land_at(
                project,
                "b.txt".into(),
                lsp_types::Position::new(300, 0),
                window,
                cx,
            );
            assert!(r.landing.is_some());
            r.note_place(cx);
        })
    });
    settle_file_frames(visual);
    visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.note_place(cx);
            r.go_back(window, cx);
            r.land_history(window, cx);
            assert_eq!(r.caret_now(cx).unwrap().1, Place::new("a.txt", 120, 0));
            // Every non-session tab kind counts, including same-file diff bases.
            r.open_diff(Some("a.txt".into()), Base::Head, window, cx);
            let head = r.active_tab.unwrap();
            r.open_diff(Some("a.txt".into()), Base::Index, window, cx);
            let index = r.active_tab.unwrap();
            r.go_back(window, cx);
            assert_eq!(r.active_tab, Some(head));
            r.go_forward(window, cx);
            assert_eq!(r.active_tab, Some(index));
            let annotate = r.restore_annotate_tab(project, "a.txt".into(), cx);
            r.show_tab(annotate, cx);
            r.go_back(window, cx);
            assert_eq!(r.active_tab, Some(index));
            // Last Edit is file-only, and Back returns to the session it left.
            r.show_tab(tb, cx);
            r.history.trail(project).changed(Place::new("a.txt", 60, 0));
            r.go_to_last_edit(window, cx);
            assert_eq!(r.caret_now(cx).unwrap().1, Place::new("a.txt", 60, 0));
            r.go_back(window, cx);
            assert_eq!(r.active_session(), Some(b));
            // A stack with only a closed session disables both the arrow and action.
            r.history = History::default();
            r.note_place(cx);
            let dead_session = Step {
                workspace,
                project: Some(project),
                front: Front::Session(a),
            };
            r.history.moved(dead_session);
            assert_eq!(r.history_directions(), (false, false));
            let before = r.active_tab;
            r.go_back(window, cx);
            assert_eq!(r.active_tab, before);
            // A deleted workspace, project, or removed seat cannot be revived.
            let dead = Step {
                workspace: empty,
                project: None,
                front: Front::Empty {
                    area: None,
                    pane: None,
                },
            };
            r.desk.delete_workspace(empty).unwrap();
            assert!(!r.step_alive(&dead));
            let gone = Step {
                workspace,
                project: Some(other),
                front: Front::File(Place::new("x", 0, 0)),
            };
            r.desk.leave(workspace, other);
            assert!(!r.step_alive(&gone));
            // Fresh in-memory state never inherits the previous window's history.
            assert!(!History::default().has_back(None, |_| true));
        })
    });
    settle_file_frames(visual);
    // A real PTY, shared by the pane/focus cases, is owned by Guard above.
    let (tab, empty_pane, terminal_pane, terminal_focus) = visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let terminal = r
                .open_tab_with(project, guild_session::Runs::Shell, None, true, cx)
                .unwrap();
            guarded_terminals.borrow_mut().extend(r.terminals());
            let tab = r.active_tab.unwrap();
            let terminal_pane = r.tabs.iter().find(|t| t.id == tab).unwrap().panes.focused();
            r.split_pane(guild_session::layout::Axis::Horizontal, window, cx);
            let empty_pane = r.tabs.iter().find(|t| t.id == tab).unwrap().panes.focused();
            r.note_place(cx);
            r.press_pane(tab, terminal_pane, cx);
            r.go_back(window, cx);
            assert_eq!(r.empty_pane_in_front(), Some((tab, empty_pane)));
            assert!(r.focusing_front);
            let focus = r
                .find_showing(r.active_seat().unwrap(), |body| {
                    body.session() == Some(terminal)
                })
                .unwrap();
            let handle = r
                .tabs
                .iter()
                .find(|t| t.id == focus.0)
                .unwrap()
                .panes
                .body_of(focus.1)
                .unwrap()
                .focus_handle(cx)
                .unwrap();
            (tab, empty_pane, terminal_pane, handle)
        })
    });
    settle_file_frames(visual);
    visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            assert!(
                !terminal_focus.is_focused(window),
                "Back takes keys away from neighbouring terminal"
            );
            r.tabs
                .iter_mut()
                .find(|t| t.id == tab)
                .unwrap()
                .panes
                .fill(
                    empty_pane,
                    Body::Settings {
                        section: crate::view::settings::Section::General,
                    },
                )
                .ok()
                .unwrap();
            let empty = Step {
                workspace: r.active_workspace,
                project: r.active_project,
                front: Front::Empty {
                    area: r.focused_area(),
                    pane: Some((tab, empty_pane)),
                },
            };
            assert!(
                !r.step_alive(&empty),
                "a filled pane is no longer an empty target"
            );
            r.note_place(cx);
            r.press_pane(tab, terminal_pane, cx);
            r.go_back(window, cx);
            assert!(matches!(r.active_body(), Some(Body::Settings { .. })));
        })
    });
    settle_file_frames(visual);
    visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            assert!(
                r.focus.is_focused(window),
                "Settings takes the keyboard from the visible terminal"
            );
            assert!(!terminal_focus.is_focused(window));
            let before = r.history.back.len();
            r.note_place(cx);
            r.note_place(cx);
            assert_eq!(r.history.back.len(), before);
            let project = r.active_project.unwrap();
            assert!(
                r.history
                    .locations(
                        r.active_workspace,
                        project,
                        r.history.standing.as_ref(),
                        LOCATIONS
                    )
                    .iter()
                    .all(|place| place.path.extension().is_some())
            );
        })
    });
    for (back, forward) in [("cmd-[", "cmd-]"), ("cmd-alt-left", "cmd-alt-right")] {
        visual.update(|_, cx| root.update(cx, |r, cx| r.press_pane(tab, terminal_pane, cx)));
        settle_file_frames(visual);
        visual.update(|window, cx| {
            assert!(terminal_focus.is_focused(window));
            window.dispatch_keystroke(gpui::Keystroke::parse(back).unwrap(), cx);
        });
        settle_file_frames(visual);
        visual.update(|window, cx| {
            root.update(cx, |r, _| {
                assert!(
                    matches!(r.active_body(), Some(Body::Settings { .. })),
                    "Back stroke from terminal reaches Root"
                );
                assert!(!terminal_focus.is_focused(window));
            })
        });
        visual.update(|window, cx| {
            window.dispatch_keystroke(gpui::Keystroke::parse(forward).unwrap(), cx);
        });
        settle_file_frames(visual);
        visual.update(|window, _| assert!(terminal_focus.is_focused(window)));
    }
    let diff_focus = visual.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_diff(Some("a.txt".into()), Base::Head, window, cx);
            let diff = r.active_tab.unwrap();
            r.tabs
                .iter_mut()
                .find(|t| t.id == tab)
                .unwrap()
                .panes
                .take(empty_pane);
            r.pour(diff, (tab, empty_pane), cx);
            let focus = r.active_body().unwrap().focus_handle(cx).unwrap();
            r.note_place(cx);
            r.press_pane(tab, terminal_pane, cx);
            r.go_back(window, cx);
            assert!(matches!(r.active_body(), Some(Body::Diff { .. })));
            focus
        })
    });
    settle_file_frames(visual);
    visual.update(|window, _| {
        assert!(diff_focus.is_focused(window));
        assert!(!terminal_focus.is_focused(window));
    });
}

#[test]
fn leaving_a_dependency_keeps_the_departure_and_other_projects_standing() {
    let project = ProjectId::new();
    let other = ProjectId::new();
    let step = Step {
        workspace: WorkspaceId::nil(),
        project: Some(project),
        front: Front::File(Place::new("a.txt", 10, 0)),
    };
    let mut h = History {
        standing: Some(step.clone()),
        ..Default::default()
    };
    h.leave(other);
    assert_eq!(h.standing, Some(step.clone()));
    h.leave(project);
    assert!(h.standing.is_none());
    assert_eq!(h.back(None, |_| true), Some(step));
}

fn correction_fixture(
    cx: &mut TestAppContext,
    case: impl FnOnce(
        &gpui::Entity<Root>,
        ProjectId,
        &std::path::Path,
        &std::path::Path,
        &mut gpui::VisualTestContext,
    ),
) {
    let project_dir = tempfile::tempdir().unwrap();
    let library_dir = tempfile::tempdir().unwrap();
    for path in [project_dir.path(), library_dir.path()] {
        std::fs::write(path.join("a.txt"), "line\n".repeat(350)).unwrap();
    }
    let mut made = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), project_dir.path());
        let root = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.isolated_render = true;
            root.restored_workspaces.insert(root.active_workspace);
            root
        });
        made = Some((root.clone(), project));
        gpui_component::Root::new(root, window, cx)
    });
    let (root, project) = made.unwrap();
    case(
        &root,
        project,
        project_dir.path(),
        library_dir.path(),
        visual,
    );
    settle_file_frames(visual);
}

#[gpui::test]
#[ignore = "run through isolated_navigation_window_regressions"]
fn failed_lsp_landing_does_not_disable_all_future_navigation(cx: &mut TestAppContext) {
    correction_fixture(cx, |root, project, directory, _, visual| {
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.go_to_place(project, Place::new("a.txt", 120, 0), window, cx);
            })
        });
        settle_file_frames(visual);
        std::fs::write(directory.join("binary.bin"), [0, 1, 2, 0]).unwrap();
        std::fs::create_dir(directory.join("directory")).unwrap();
        for path in ["gone.txt", "binary.bin", "directory"] {
            visual.update(|window, cx| {
                root.update(cx, |r, cx| {
                    r.land_at(
                        project,
                        path.into(),
                        lsp_types::Position::new(10, 0),
                        window,
                        cx,
                    );
                })
            });
            settle_file_frames(visual);
            visual.update(|_, cx| {
                root.update(cx, |r, _| {
                    assert!(
                        r.landing.is_none(),
                        "{path} cannot keep a pending LSP landing"
                    );
                })
            });
        }
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                std::fs::write(directory.join("later.txt"), "one\ntwo").unwrap();
                r.restore_file_tab(project, directory, "later.txt".into(), cx);
                r.land_at(
                    project,
                    "later.txt".into(),
                    lsp_types::Position::new(1, 0),
                    window,
                    cx,
                );
                assert!(
                    r.landing.is_some(),
                    "an undrawn text editor is allowed to wait"
                );
                r.close_tab(r.active_tab.unwrap(), cx);
                r.place_caret(window, cx);
                assert!(
                    r.landing.is_none(),
                    "closing the deferred target cancels its landing"
                );
                r.open_settings(cx);
                let (_, tab) = session(r, project, cx);
                r.show_tab(tab, cx);
                r.go_back(window, cx);
                assert!(matches!(r.active_body(), Some(Body::Settings { .. })));
            })
        });
    });
}

#[gpui::test]
#[ignore = "run through isolated_navigation_window_regressions"]
fn back_from_a_dependency_returns_to_its_departure(cx: &mut TestAppContext) {
    correction_fixture(cx, |root, project, _, library, visual| {
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.go_to_place(project, Place::new("a.txt", 120, 0), window, cx);
            })
        });
        settle_file_frames(visual);
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                let origin = r.note_library(project, library);
                r.go_to_library(origin, std::path::Path::new("a.txt"), None, window, cx);
                r.go_back(window, cx);
                assert_eq!(r.caret_now(cx).unwrap().1, Place::new("a.txt", 120, 0));
            })
        });
    });
}

#[gpui::test]
#[ignore = "run through isolated_navigation_window_regressions"]
fn one_arrow_across_areas_leaves_one_departure(cx: &mut TestAppContext) {
    correction_fixture(cx, |root, project, _, _, visual| {
        visual.update(|window, cx| {
            root.update(cx, |r, cx| {
                use guild_session::layout::{Axis, Direction};
                let (_, left_tab) = session(r, project, cx);
                r.show_tab(left_tab, cx);
                let left_area = r.focused_area().unwrap();
                r.work_at.set(Some(gpui::Bounds {
                    origin: gpui::point(gpui::px(0.), gpui::px(0.)),
                    size: gpui::size(gpui::px(1400.), gpui::px(900.)),
                }));
                r.split_area(Axis::Horizontal, window, cx);
                let right_area = r.focused_area().unwrap();
                assert_ne!(left_area, right_area);
                let path = std::path::PathBuf::from("annotation.txt");
                let look = r.file_look();
                let ui = r.ui();
                let view =
                    cx.new(|cx| crate::annotate_tab::AnnotateTab::new(path.clone(), look, ui, cx));
                let mut panes = guild_session::Panes::one(Body::Annotate { path, tab: view });
                let left_pane = panes.focused();
                let right_pane = panes.split(left_pane, Axis::Horizontal).unwrap();
                panes
                    .fill(
                        right_pane,
                        Body::Settings {
                            section: crate::view::settings::Section::General,
                        },
                    )
                    .ok()
                    .unwrap();
                let right_tab = r.adopt_panes(r.active_seat().unwrap(), project, panes, cx);
                r.show_tab(right_tab, cx);
                r.focus_area(left_area, cx);
                r.history = History::default();
                r.note_place(cx);
                r.focus_toward(Direction::Right, window, cx);
                assert_eq!(r.active_tab, Some(right_tab));
                assert!(
                    matches!(r.active_body(), Some(Body::Annotate { .. })),
                    "geometry lands on the left pane of the right area"
                );
                r.go_back(window, cx);
                assert_eq!(
                    r.active_tab,
                    Some(left_tab),
                    "Back must skip the unseen previous pane of the destination area"
                );
            })
        });
    });
}
