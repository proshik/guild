//! Actual Root tab strips, isolated before any action capable of saving.
#[cfg(test)]
mod tests {
    use super::super::*;
    use gpui::{AppContext, MouseButton, TestAppContext, VisualTestContext, point};
    use std::time::Duration;

    #[test]
    fn isolated_tab_drag_regressions() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "view::tabs::drag_tests::tests::tab_drag_",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_TAB_DRAG_TEST", "1")
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .output()
            .unwrap();
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    struct Fixture<'a> {
        _dir: tempfile::TempDir,
        root: gpui::Entity<Root>,
        cx: &'a mut VisualTestContext,
        terminals: Vec<gpui::Entity<crate::terminal::view::TerminalTab>>,
    }
    impl Drop for Fixture<'_> {
        fn drop(&mut self) {
            for view in &self.terminals {
                view.update(self.cx, |v, cx| v.stop(cx));
            }
            guild_session::store::flush();
        }
    }
    fn mount(cx: &mut TestAppContext) -> Fixture<'_> {
        assert_eq!(std::env::var("GUILD_TAB_DRAG_TEST").as_deref(), Ok("1"));
        assert!(
            crate::paths::workspace_file()
                .starts_with(std::env::var_os("GUILD_STATE_DIR").expect("isolated state"))
        );
        let dir = tempfile::tempdir().unwrap();
        for i in 0..16 {
            std::fs::write(
                dir.path().join(format!("document-{i:02}.txt")),
                "alpha\nbeta\n",
            )
            .unwrap();
        }
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            desk.open_in(desk.active_workspace(), dir.path());
            let root = cx.new(|cx| {
                let mut r = Root::diagnostic(desk, crate::paths::state_dir(), cx);
                r.isolated_render = true;
                r
            });
            root.update(cx, |r, cx| {
                for i in 0..16 {
                    r.open_file(
                        std::path::Path::new(&format!("document-{i:02}.txt")),
                        window,
                        cx,
                    );
                }
                r.active_tab = Some(r.tabs[0].id);
                r.refocus(window, cx);
                cx.notify();
            });
            made = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        cx.simulate_resize(gpui::size(px(1100.), px(650.)));
        for _ in 0..5 {
            cx.update(|w, cx| {
                w.simulate_next_frame(cx);
            });
            cx.run_until_parked();
        }
        Fixture {
            _dir: dir,
            root: made.unwrap(),
            cx,
            terminals: Vec::new(),
        }
    }
    fn frames(cx: &mut VisualTestContext, n: usize) {
        for _ in 0..n {
            cx.executor().advance_clock(Duration::from_micros(16667));
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
            cx.run_until_parked();
        }
    }
    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_escape_preserves_editor_cursors(cx: &mut TestAppContext) {
        let f = mount(cx);
        let root = f.root.clone();
        let cx = &mut *f.cx;
        let editor = root.read_with(cx, |r, cx| match r.active_body().unwrap() {
            Body::File { tab, .. } => tab.read(cx).editor().unwrap().clone(),
            _ => panic!("file"),
        });
        cx.update(|w, cx| {
            editor.update(cx, |s, cx| {
                s.set_cursor_position(gpui_component::input::Position::new(0, 0), w, cx)
            })
        });
        cx.run_until_parked();
        let (caret, height) = editor.read_with(cx, |s, _| {
            (s.cursor_layout().unwrap().0, s.line_height().unwrap())
        });
        cx.simulate_click(
            point(caret.left() + px(1.), caret.center().y + height),
            gpui::Modifiers {
                alt: true,
                ..Default::default()
            },
        );
        cx.simulate_keystrokes("q");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value().to_string()),
            "qalpha\nqbeta\n"
        );
        cx.simulate_keystrokes("cmd-z");
        let focus = cx.update(|w, cx| w.focused(cx));
        let scroll = root.read_with(cx, |r, _| r.strip_scroll(r.focused_area()));
        let source = scroll.bounds_for_item(0).unwrap().center() + scroll.offset();
        cx.simulate_mouse_down(source, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            point(source.x + px(20.), source.y),
            MouseButton::Left,
            Default::default(),
        );
        cx.simulate_keystrokes("escape");
        assert!(
            !cx.update(|_, cx| cx.has_active_drag()),
            "Escape cancels tab drag before editor actions"
        );
        cx.simulate_mouse_up(source, MouseButton::Left, Default::default());
        assert_eq!(cx.update(|w, cx| w.focused(cx)), focus);
        cx.simulate_keystrokes("q");
        assert_eq!(
            editor.read_with(cx, |s, _| s.value().to_string()),
            "qalpha\nqbeta\n",
            "tab drag Escape preserves both editor cursors"
        );
    }

    fn start(
        cx: &mut VisualTestContext,
        source: gpui::Point<gpui::Pixels>,
        edge: gpui::Point<gpui::Pixels>,
    ) {
        cx.simulate_mouse_down(source, MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            point(source.x + px(20.), source.y),
            MouseButton::Left,
            Default::default(),
        );
        cx.simulate_mouse_move(edge, MouseButton::Left, Default::default());
        assert!(cx.update(|_, cx| cx.has_active_drag()));
    }
    fn ids(
        root: &gpui::Entity<Root>,
        cx: &VisualTestContext,
        area: Option<guild_session::AreaId>,
    ) -> Vec<TabId> {
        root.read_with(cx, |r, _| r.area_tabs(area).map(|t| t.id).collect())
    }
    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_stationary_edges_drop_newly_revealed_and_preserve_other_order(
        cx: &mut TestAppContext,
    ) {
        let f = mount(cx);
        let root = f.root.clone();
        let cx = &mut *f.cx;
        let area = root.read_with(cx, |r, _| r.focused_area());
        let scroll = root.read_with(cx, |r, _| r.strip_scroll(area));
        for right in [true, false] {
            let order = ids(&root, cx, area);
            let dragged = if right {
                order[0]
            } else {
                *order.last().unwrap()
            };
            root.update(cx, |r, cx| {
                r.active_tab = Some(dragged);
                cx.notify();
            });
            frames(cx, 5);
            let index = ids(&root, cx, area)
                .iter()
                .position(|t| *t == dragged)
                .unwrap();
            let source = scroll.bounds_for_item(index).unwrap().center() + scroll.offset();
            let edge = point(
                if right {
                    scroll.bounds().right() - px(2.)
                } else {
                    scroll.bounds().left() + px(2.)
                },
                source.y,
            );
            let initial = scroll.offset();
            start(cx, source, edge);
            let focus = cx.update(|w, cx| w.focused(cx));
            frames(cx, 30);
            assert!(if right {
                scroll.offset().x < initial.x - px(100.)
            } else {
                scroll.offset().x > initial.x + px(100.)
            });
            let before = ids(&root, cx, area);
            let candidates: Vec<_> = before
                .iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    let mut rect = scroll.bounds_for_item(i)?;
                    rect.origin += scroll.offset();
                    let at = point(rect.left() + rect.size.width * 0.25, rect.center().y);
                    let mut old = scroll.bounds_for_item(i)?;
                    old.origin += initial;
                    (*id != dragged
                        && answers_drag(rect, scroll.bounds(), at)
                        && !answers_drag(old, scroll.bounds(), at))
                    .then_some((*id, at))
                })
                .collect();
            let (target, at) = *candidates
                .get(candidates.len() / 2)
                .expect("a target revealed by this hold");
            let mut expected = before.clone();
            expected.retain(|id| *id != dragged);
            let at_index = expected.iter().position(|id| *id == target).unwrap();
            expected.insert(at_index, dragged);
            // No final movement: release itself chooses the newly painted target.
            cx.simulate_mouse_up(at, MouseButton::Left, Default::default());
            frames(cx, 3);
            assert_eq!(ids(&root, cx, area), expected);
            guild_session::store::flush();
            let (workspace, expected_paths, active_index) = root.read_with(cx, |r, _| {
                let paths: Vec<_> = r
                    .area_tabs(area)
                    .map(|t| match t.body().unwrap() {
                        Body::File {
                            path,
                            origin: crate::Origin::Project,
                            ..
                        } => path.clone(),
                        _ => panic!("file fixture"),
                    })
                    .collect();
                (
                    r.active_workspace,
                    paths,
                    expected.iter().position(|id| *id == dragged),
                )
            });
            let saved = guild_session::project_state::load(f._dir.path());
            let seat = saved
                .seats
                .get(&workspace)
                .expect("drop saves the isolated seat");
            let saved_paths: Vec<_> = seat
                .tabs
                .iter()
                .map(|t| match &t.shows {
                    guild_session::project_state::Shows::File(path) => path.clone(),
                    _ => panic!("saved file"),
                })
                .collect();
            assert_eq!(
                saved_paths, expected_paths,
                "same-gesture drop order survives store reload"
            );
            assert_eq!(seat.active, active_index);

            assert_eq!(root.read_with(cx, |r, _| r.active_tab), Some(dragged));
            assert_eq!(cx.update(|w, cx| w.focused(cx)), focus);
            assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_none()));
        }
    }
    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_frames_pause_reverse_limit_chrome_and_buttonless_return(cx: &mut TestAppContext) {
        let f = mount(cx);
        let root = f.root.clone();
        let cx = &mut *f.cx;
        let scroll = root.read_with(cx, |r, _| r.strip_scroll(r.focused_area()));
        let source = scroll.bounds_for_item(0).unwrap().center() + scroll.offset();
        let chrome = point(scroll.bounds().right() + px(20.), source.y);
        start(cx, source, chrome);
        let before = scroll.offset();
        cx.executor().advance_clock(Duration::from_millis(48));
        cx.run_until_parked();
        assert_eq!(
            scroll.offset(),
            before,
            "motion requires a real scheduled frame"
        );
        frames(cx, 1);
        let stalled = (scroll.offset().x - before.x).abs();
        let row = cx.update(|w, _| px(crate::theme::space::TREE_ROW.0 * f32::from(w.rem_size())));
        assert!(stalled > px(0.) && stalled <= row * 0.65);
        let mut previous = scroll.offset().x;
        let mut max_step = px(0.);
        for _ in 0..20 {
            frames(cx, 1);
            max_step = max_step.max((scroll.offset().x - previous).abs());
            previous = scroll.offset().x;
        }
        assert!(max_step > px(0.) && max_step < row * 0.4);
        for pause in [scroll.bounds().center(), point(px(5.), px(5.))] {
            cx.simulate_mouse_move(pause, MouseButton::Left, Default::default());
            frames(cx, 2);
            let stopped = scroll.offset();
            frames(cx, 20);
            assert_eq!(scroll.offset(), stopped);
            let queued = cx.update(|w, cx| w.simulate_next_frame(cx));
            assert_eq!(
                queued, 0,
                "paused gestures must not sustain frame callbacks"
            );
            cx.simulate_mouse_move(chrome, MouseButton::Left, Default::default());
            frames(cx, 1);
            let resumed = (scroll.offset().x - stopped.x).abs();
            assert!(resumed > px(0.) && resumed < row * 0.4);
        }
        cx.update(|_, cx| {
            scroll.set_offset(point(-scroll.max_offset().x, px(0.)));
            root.update(cx, |_, cx| cx.notify());
        });
        frames(cx, 4);
        let limit = scroll.offset();
        frames(cx, 10);
        assert_eq!(scroll.offset(), limit);
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_some()));
        let left = point(scroll.bounds().left() + px(2.), source.y);
        cx.simulate_mouse_move(left, MouseButton::Left, Default::default());
        frames(cx, 4);
        assert!(
            scroll.offset().x > limit.x,
            "limit pauses; reversing resumes the same owner"
        );
        cx.simulate_mouse_move(point(px(5.), px(5.)), MouseButton::Left, Default::default());
        frames(cx, 2);
        let stopped = scroll.offset();
        cx.simulate_event(gpui::MouseMoveEvent {
            position: chrome,
            ..Default::default()
        });
        frames(cx, 5);
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_none()));
        assert_eq!(scroll.offset(), stopped);
        println!(
            "tab frame motion: normal_max={max_step:?}, stalled_frame={stalled:?}; chrome, pauses, reversal and buttonless return verified"
        );
    }
    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_adjacent_strips_transfer_and_lifecycle(cx: &mut TestAppContext) {
        let f = mount(cx);
        let root = f.root.clone();
        let cx = &mut *f.cx;
        cx.simulate_resize(gpui::size(px(1600.), px(650.)));
        let left = root.read_with(cx, |r, _| r.focused_area().unwrap());
        let right = cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                r.split_area(guild_session::layout::Axis::Horizontal, w, cx);
                let right = r.focused_area().expect("second area");
                assert_ne!(left, right);
                // Move half the existing actual file tabs through the production action.
                let move_ids: Vec<_> = r.tabs.iter().skip(8).map(|t| t.id).collect();
                for id in move_ids {
                    r.move_tab_to_area(id, right, None, w, cx);
                }
                right
            })
        });
        frames(cx, 5);
        let a = root.read_with(cx, |r, _| r.strip_scroll(Some(left)));
        let b = root.read_with(cx, |r, _| r.strip_scroll(Some(right)));
        root.update(cx, |_, cx| {
            b.set_offset(Default::default());
            cx.notify();
        });
        frames(cx, 2);
        let source = a.bounds_for_item(0).unwrap().center() + a.offset();
        let dragged = ids(&root, cx, Some(left))[0];
        let other_order = ids(&root, cx, Some(right));
        let chrome = point(a.bounds().right() + px(20.), source.y);
        start(cx, source, chrome);
        let b_before = b.offset();
        frames(cx, 20);
        assert!(a.offset().x < px(-80.));
        assert_eq!(b.offset(), b_before);
        assert_eq!(
            ids(&root, cx, Some(right)),
            other_order,
            "clipped left tabs must not reorder the neighbouring strip"
        );
        let a_before = a.offset();
        let b_edge = point(b.bounds().right() + px(20.), source.y);
        cx.simulate_mouse_move(b_edge, MouseButton::Left, Default::default());
        frames(cx, 20);
        assert_eq!(a.offset(), a_before);
        assert!(b.offset().x < b_before.x - px(80.));
        let mut target = b.bounds_for_item(3).unwrap();
        target.origin += b.offset();
        let at = target.intersect(&b.bounds()).center();
        cx.simulate_mouse_up(at, MouseButton::Left, Default::default());
        frames(cx, 3);
        assert!(root.read_with(cx, |r, _| {
            r.tabs.iter().any(|t| t.id == dragged && t.area == right)
        }));
        // Each destructive lifecycle transition invalidates queued frames/clock.
        for kind in 0..3 {
            let area = root.read_with(cx, |r, _| r.focused_area());
            let s = root.read_with(cx, |r, _| r.strip_scroll(area));
            root.update(cx, |_, cx| {
                s.set_offset(Default::default());
                cx.notify();
            });
            frames(cx, 2);
            let id = ids(&root, cx, area)[0];
            let source = s.bounds_for_item(0).unwrap().center() + s.offset();
            start(cx, source, point(s.bounds().right() - px(2.), source.y));
            cx.update(|w, cx| {
                root.update(cx, |r, cx| {
                    match kind {
                        0 => {
                            r.close_tab(id, cx);
                        }
                        1 => {
                            r.close_area(w, cx);
                        }
                        _ => {
                            let next = r.desk.add_workspace("Other", None).unwrap();
                            r.select_workspace(next, cx);
                        }
                    }
                    cx.notify();
                })
            });
            frames(cx, 10);
            assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_none()));
            assert!(!cx.update(|_, cx| cx.has_active_drag()));
            let stopped = s.offset();
            frames(cx, 5);
            assert_eq!(s.offset(), stopped);
            cx.simulate_mouse_up(source, MouseButton::Left, Default::default());
        }
    }

    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_escape_preserves_terminal_selection_session_and_focus(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let mut f = mount(cx);
        let root = f.root.clone();
        let terminal = f.cx.update(|_, cx| {
            let mut command = portable_pty::CommandBuilder::new("/bin/sh");
            command.arg("-c");
            command.arg("printf 'owned terminal selection\\n'; exec /bin/cat");
            let mut look = crate::terminal::view::Look::of(&Default::default(), Default::default());
            look.copy_on_select = false;
            crate::terminal::view::open_job(command, std::path::Path::new("/"), look, cx).unwrap()
        });
        f.terminals.push(terminal.clone());
        let cx = &mut *f.cx;
        let (id, session) = cx.update(|w, cx| {
            root.update(cx, |r, cx| {
                let seat = r.active_seat().unwrap();
                let session = r.desk.add_session(guild_session::SessionRecord::new(
                    "Owned shell".into(),
                    seat,
                    std::path::PathBuf::from("/"),
                    Runs::Shell,
                ));
                let facts = terminal.read(cx).facts().clone();
                let id = r.adopt(
                    seat,
                    seat.project,
                    Body::Terminal {
                        session,
                        view: terminal.clone(),
                        facts,
                    },
                    cx,
                );
                r.active_tab = Some(id);
                r.refocus(w, cx);
                cx.notify();
                (id, session)
            })
        });
        frames(cx, 5);
        for _ in 0..40 {
            if terminal.read_with(cx, |v, _| v.has_text()) {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
            frames(cx, 1);
        }
        terminal.update(cx, |v, cx| {
            assert!(v.has_text());
            v.select_all(cx);
            assert!(v.has_selection());
            assert!(v.take_interrupt().is_none());
        });
        let focus = cx.update(|w, cx| w.focused(cx));
        let area = root.read_with(cx, |r, _| r.focused_area());
        let scroll = root.read_with(cx, |r, _| r.strip_scroll(area));
        let index = ids(&root, cx, area).iter().position(|t| *t == id).unwrap();
        let source = scroll.bounds_for_item(index).unwrap().center() + scroll.offset();
        start(cx, source, point(scroll.bounds().left() + px(2.), source.y));
        frames(cx, 5);
        cx.simulate_keystrokes("escape");
        let order = ids(&root, cx, area);
        let stopped = scroll.offset();
        cx.simulate_mouse_up(source, MouseButton::Left, Default::default());
        frames(cx, 5);
        assert_eq!(
            ids(&root, cx, area),
            order,
            "Escape retains already-live placement and prevents a final drop"
        );
        assert_eq!(scroll.offset(), stopped);
        assert_eq!(cx.update(|w, cx| w.focused(cx)), focus);
        assert_eq!(root.read_with(cx, |r, _| r.active_tab), Some(id));
        root.read_with(cx, |r, _| {
            match r.tabs.iter().find(|t| t.id == id).unwrap().body().unwrap() {
                Body::Terminal {
                    session: actual,
                    view,
                    ..
                } => {
                    assert_eq!(*actual, session);
                    assert_eq!(*view, terminal);
                }
                _ => panic!("same terminal body"),
            }
        });
        terminal.update(cx, |v, _| {
            assert!(
                v.has_selection(),
                "drag Escape must not clear terminal selection"
            );
            assert!(
                v.take_interrupt().is_none(),
                "drag Escape must not interrupt the terminal"
            );
        });
        assert!(!cx.update(|_, cx| cx.has_active_drag()));
    }

    #[gpui::test]
    #[ignore = "run through isolated_tab_drag_regressions"]
    fn tab_drag_short_strip_unrelated_project_wheel_and_replacement_owner(cx: &mut TestAppContext) {
        let f = mount(cx);
        let root = f.root.clone();
        let cx = &mut *f.cx;
        let area = root.read_with(cx, |r, _| r.focused_area());
        let scroll = root.read_with(cx, |r, _| r.strip_scroll(area));
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: scroll.bounds().center(),
            delta: gpui::ScrollDelta::Pixels(point(px(-100.), px(0.))),
            ..Default::default()
        });
        assert!(scroll.offset().x < px(0.));
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_none()));
        root.update(cx, |_, cx| {
            scroll.set_offset(Default::default());
            cx.notify();
        });
        frames(cx, 2);
        let key = root.read_with(cx, |r, _| {
            r.column_groups_for(r.active_workspace)[0].key.clone()
        });
        let header = cx
            .debug_bounds(Box::leak(
                format!("project-header-{}", key.display()).into_boxed_str(),
            ))
            .unwrap()
            .center();
        let edge = point(
            scroll.bounds().right() + px(20.),
            scroll.bounds().center().y,
        );
        start(cx, header, edge);
        let before = scroll.offset();
        frames(cx, 10);
        assert_eq!(scroll.offset(), before);
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_none()));
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(edge, MouseButton::Left, Default::default());
        // Leave an old rightward callback queued, then replace its gesture before any frame.
        let source = scroll.bounds_for_item(0).unwrap().center() + scroll.offset();
        start(cx, source, edge);
        let live_order = ids(&root, cx, area);
        cx.simulate_mouse_up(edge, MouseButton::Left, Default::default());
        assert_eq!(
            ids(&root, cx, area),
            live_order,
            "fixed chrome is not a placement target"
        );
        root.update(cx, |_, cx| {
            scroll.set_offset(point(px(-200.), px(0.)));
            cx.notify();
        });
        let source = (0..16)
            .filter_map(|i| scroll.bounds_for_item(i))
            .map(|mut b| {
                b.origin += scroll.offset();
                b
            })
            .find(|b| b.left() > scroll.bounds().left() && b.right() < scroll.bounds().right())
            .unwrap()
            .center();
        let left = point(scroll.bounds().left() + px(2.), source.y);
        start(cx, source, left);
        let before = scroll.offset();
        frames(cx, 1);
        let step = scroll.offset().x - before.x;
        assert!(
            step > px(0.) && step < px(8.),
            "only replacement frame may advance: {step:?}"
        );
        frames(cx, 8);
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_some()));
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(left, MouseButton::Left, Default::default());
        // Existing tab-close action leaves a genuine non-overflowing production strip.
        root.update(cx, |r, cx| {
            let close: Vec<_> = r.area_tabs(area).skip(2).map(|t| t.id).collect();
            for id in close {
                r.close_tab(id, cx);
            }
            cx.notify();
        });
        frames(cx, 5);
        assert_eq!(scroll.max_offset().x, px(0.));
        let source = scroll.bounds_for_item(0).unwrap().center() + scroll.offset();
        start(
            cx,
            source,
            point(scroll.bounds().right() + px(20.), source.y),
        );
        let before = scroll.offset();
        frames(cx, 20);
        assert_eq!(scroll.offset(), before);
        assert!(root.read_with(cx, |r, _| r.tab_drag.active.is_some()));
        assert_eq!(cx.update(|w, cx| w.simulate_next_frame(cx)), 0);
        cx.simulate_keystrokes("escape");
        cx.simulate_mouse_up(source, MouseButton::Left, Default::default());
        frames(cx, 2);
        let menu = root.read_with(cx, |r, _| r.strip_at.borrow()[&area][2].unwrap().center());
        cx.simulate_click(menu, Default::default());
        assert!(
            root.read_with(cx, |r, _| r.context_menu.is_some()),
            "fixed actions remain clickable after a drag"
        );
        cx.simulate_keystrokes("escape");
        assert!(root.read_with(cx, |r, _| r.context_menu.is_none()));
    }
}
