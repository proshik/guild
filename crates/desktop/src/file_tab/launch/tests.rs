use super::*;
use gpui::{AppContext, TestAppContext};

struct Host {
    file: Entity<FileTab>,
    launched: Vec<Invocation>,
}

#[gpui::test]
fn a_second_click_on_a_replaced_chooser_cannot_reselect_or_panic(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("justfile"),
        "first:\n    echo first\nsecond who:\n    echo {{who}}\n",
    )
    .unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("justfile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| file.update(cx, |file, cx| file.run_file(cx)));
    cx.run_until_parked();
    let position = cx.debug_bounds("task-choice-1").unwrap().center();
    let first_position = cx.debug_bounds("task-choice-0").unwrap().center();
    cx.update(|window, cx| {
        // Both clicks reach the same painted handlers. The first replaces the
        // two-task chooser with the second task's required-argument form.
        for click_count in [1, 2] {
            window.dispatch_event(
                gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                    position,
                    button: gpui::MouseButton::Left,
                    modifiers: gpui::Modifiers::none(),
                    click_count,
                    first_mouse: false,
                }),
                cx,
            );
            window.dispatch_event(
                gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                    position,
                    button: gpui::MouseButton::Left,
                    modifiers: gpui::Modifiers::none(),
                    click_count,
                }),
                cx,
            );
            let form = file.read(cx).launch.as_ref().unwrap();
            assert_eq!(form.tasks.len(), 1);
            assert_eq!(form.tasks[0].identity.name, "second");
        }
        // Index zero still exists in the replacement. An old first-row click
        // must also leave that form's focus and entered values untouched.
        let focus = file.read(cx).launch.as_ref().unwrap().focus.clone();
        file.update(cx, |file, _| {
            file.launch.as_mut().unwrap().values[0] = "World".into()
        });
        let position = first_position;
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
            }),
            cx,
        );
        let form = file.read(cx).launch.as_ref().unwrap();
        assert_eq!(form.focus, focus);
        assert_eq!(form.values[0], "World");
    });
}

#[gpui::test]
fn wheel_over_real_line_numbers_scrolls_the_retained_editor(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "one\ntwo\n".repeat(100)).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("file.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let position = cx.debug_bounds("editor-line-number").unwrap().center();
    for delta in [
        gpui::ScrollDelta::Pixels(gpui::point(gpui::px(0.), gpui::px(-80.))),
        gpui::ScrollDelta::Lines(gpui::point(0., -3.)),
    ] {
        let before = cx.update(|_, cx| {
            file.read(cx)
                .editor
                .as_ref()
                .unwrap()
                .read(cx)
                .scroll_offset()
        });
        cx.simulate_event(gpui::ScrollWheelEvent {
            position,
            delta,
            ..Default::default()
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(
                file.read(cx)
                    .editor
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .scroll_offset()
                    .y
                    < before.y,
                "an actual wheel over the number must reach the retained editor"
            )
        });
    }
}

#[gpui::test]
fn measured_line_numbers_keep_line_selection(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "one\ntwo\n").unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("file.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let number = cx.debug_bounds("editor-line-number").unwrap();
    let position = cx.update(|_, cx| {
        let state = file.read(cx).editor.as_ref().unwrap().read(cx);
        gpui::point(
            number.center().x,
            state.range_to_bounds(&(0..0)).unwrap().top() + state.line_height().unwrap() / 2.,
        )
    });
    cx.simulate_click(position, gpui::Modifiers::none());
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx)
                .editor
                .as_ref()
                .unwrap()
                .read(cx)
                .selected_range(),
            0..4,
            "clicking a number still selects its real buffer line"
        )
    });
}

#[gpui::test]
fn an_old_click_handler_rejects_same_size_declaration_reordering(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let original = "one:\n    echo one\ntwo:\n    echo two\n";
    let swapped = "two:\n    echo two\none:\n    echo one\n";
    std::fs::write(dir.path().join("justfile"), original).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("justfile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        let observer = cx.new(|cx| {
            cx.subscribe(&file, |host: &mut Host, _, event: &LaunchRun, _| {
                host.launched.push(event.0.clone())
            })
            .detach();
            Host {
                file: file.clone(),
                launched: Vec::new(),
            }
        });
        made = Some((file.clone(), observer));
        gpui_component::Root::new(file, window, cx)
    });
    let (file, observer) = made.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let control = cx.debug_bounds("editor-task-control").unwrap();
    let position = cx.update(|_, cx| {
        let state = file.read(cx).editor.as_ref().unwrap().read(cx);
        let row = state.range_to_bounds(&(0..0)).unwrap();
        gpui::point(
            control.center().x,
            row.top() + state.line_height().unwrap() / 2.,
        )
    });
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.editor
                .as_ref()
                .unwrap()
                .update(cx, |editor, cx| editor.set_value(swapped, window, cx));
            file.tasks = source::parse(Path::new("justfile"), swapped);
        });
        // Dispatch to the previous painted handlers, without a new frame or
        // executor drain between the buffer change and the mouse events.
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        window.dispatch_event(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                position,
                button: gpui::MouseButton::Left,
                modifiers: gpui::Modifiers::none(),
                click_count: 1,
            }),
            cx,
        );
        assert!(
            file.read(cx).launch.is_none(),
            "old handler must not select one, which still exists on the other row"
        );
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("justfile")).unwrap(),
        original
    );
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            observer.read(cx).launched.is_empty(),
            "old handlers cannot emit a launch after asynchronous re-resolution"
        )
    });
}

#[gpui::test]
fn collapsed_rows_cannot_share_the_next_visible_gutter_hitbox(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("Makefile"),
        "serve:\n\t@echo ok\nwait:\n\t@sleep 120\n",
    )
    .unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("Makefile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let run = cx.debug_bounds("editor-task-control").unwrap();
    let folding = cx.update(|_, cx| {
        let state = file.read(cx).editor.as_ref().unwrap().read(cx);
        let text = state.range_to_bounds(&(0..0)).unwrap();
        // The folding control lies between the actual Run lane and text start.
        // Measure that region instead of copying private component padding.
        gpui::point(
            (run.right() + text.left()) / 2.,
            text.top() + state.line_height().unwrap() / 2.,
        )
    });
    cx.simulate_click(folding, gpui::Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    cx.update(|window, cx| {
        let file = file.read(cx);
        let state = file.editor.as_ref().unwrap().read(cx);
        let start = gpui_component::RopeExt::line_start_offset(state.text(), 1);
        let next = gpui_component::RopeExt::line_start_offset(state.text(), 2);
        let hidden = state.range_to_bounds(&(start..start)).unwrap();
        let visible = state.range_to_bounds(&(next..next)).unwrap();
        assert_eq!(
            hidden.top(),
            visible.top(),
            "the real component fold must be active"
        );
        assert!(file.gutter_row_contains(2, visible.center(), window, cx));
        assert!(
            !file.gutter_row_contains(1, visible.center(), window, cx),
            "hidden rows must not alias the next visible row"
        );
    });
}

#[gpui::test]
fn controls_follow_wrap_scroll_resize_font_and_blame_lane(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let source = format!(
        "# {}\nfirst second:\n\techo ok\n{}",
        "кириллица ".repeat(60),
        "# other\n".repeat(80)
    );
    std::fs::write(dir.path().join("Makefile"), &source).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("Makefile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    let file = made.unwrap();
    cx.run_until_parked();
    for (wrap, width, font, blame) in [
        (true, 700., 14., false),
        (true, 900., 18., true),
        (false, 500., 22., false),
    ] {
        cx.simulate_resize(gpui::size(px(width), px(650.)));
        cx.update(|window, cx| {
            file.update(cx, |file, cx| {
                let mut look = file.look.clone();
                look.font_size = font;
                file.restyle(look, file.ui, cx);
                if file.blame_enabled() != blame {
                    file.toggle_blame(cx);
                }
                file.editor.as_ref().unwrap().update(cx, |editor, cx| {
                    editor.set_soft_wrap(wrap, window, cx);
                    editor.set_scroll_offset(
                        gpui::point(px(if wrap { 0. } else { -120. }), px(0.)),
                        cx,
                    );
                });
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let control = cx
            .debug_bounds("editor-task-control")
            .expect("measured declaration control");
        let number = cx
            .debug_bounds("editor-line-number")
            .expect("measured line number before the run control");
        assert!(
            number.right() <= control.left(),
            "run icons belong to the right of line numbers"
        );
        assert!(
            control.size.width <= px(16.),
            "run controls must not add a heading-sized gap before folding"
        );
        cx.update(|window, cx| {
            let state = file.read(cx).editor.as_ref().unwrap().read(cx);
            let digits = gpui_component::RopeExt::lines_len(state.text()).to_string();
            let shaped = window.text_system().shape_line(
                digits.clone().into(),
                px(font),
                &[gpui::TextRun {
                    len: digits.len(),
                    font: gpui::Font { family: "monospace".into(), ..Default::default() },
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            assert!(
                // The host rounds the composed cell to whole layout pixels.
                number.size.width <= shaped.width + px(8.) + px(1.),
                "number padding must not create a wide gap before Run: actual {:?}, digits {:?}, font {font}, wrap {wrap}",
                number.size.width, shaped.width
            );
            let offset = gpui_component::RopeExt::line_start_offset(state.text(), 1);
            let text = state.range_to_bounds(&(offset..offset)).unwrap();
            assert!((f32::from(control.top() - text.top())).abs() < 1.);
            assert!(file.read(cx).gutter_row_contains(1, control.center(), window, cx));
        });
    }
    let old = cx.debug_bounds("editor-task-control").unwrap();
    cx.update(|window, cx| {
        file.read(cx)
            .editor
            .as_ref()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| {
                editor.set_cursor_position(gpui_component::input::Position::new(70, 0), window, cx)
            })
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    assert!(cx.debug_bounds("editor-task-control").is_none());
    cx.update(|window, cx| {
        assert!(
            !file
                .read(cx)
                .gutter_row_contains(1, old.center(), window, cx),
            "stale offscreen hitbox is inert"
        )
    });
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.file.clone()
    }
}

#[gpui::test]
fn clicking_a_specific_no_argument_task_saves_and_launches_without_a_chooser(
    cx: &mut TestAppContext,
) {
    for (name, source, runner) in [
        ("justfile", "check:\n    echo ok\n", "'just'"),
        ("Makefile", "serve:\n\t@echo ok\n", "'make'"),
        (
            "package.json",
            "{\"packageManager\":\"pnpm@9\",\"scripts\":{\"check\":\"echo ok\"}}",
            "'pnpm'",
        ),
        (
            "Taskfile.yml",
            "version: '3'\ntasks:\n  check:\n    cmds: [echo ok]\n",
            "'task'",
        ),
    ] {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(name), source).unwrap();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new(name),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let observer = cx.new(|cx| {
                cx.subscribe(&file, |host: &mut Host, _, event: &LaunchRun, _| {
                    host.launched.push(event.0.clone())
                })
                .detach();
                Host {
                    file: file.clone(),
                    launched: Vec::new(),
                }
            });
            made = Some((file, observer.clone()));
            gpui_component::Root::new(observer, window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.simulate_next_frame(cx));
        cx.run_until_parked();
        let control = cx.debug_bounds("editor-task-control").unwrap();
        cx.simulate_click(control.center(), gpui::Modifiers::none());
        cx.run_until_parked();
        let (file, observer) = made.unwrap();
        cx.update(|_, cx| {
            assert!(
                file.read(cx).launch.is_none(),
                "{name}: a specific action must not ask to choose the command again"
            );
            assert_eq!(observer.read(cx).launched.len(), 1, "{name}");
            assert!(observer.read(cx).launched[0].line.starts_with(runner));
        });
        if matches!(name, "Makefile" | "Taskfile.yml") {
            cx.simulate_click(
                control.center(),
                gpui::Modifiers {
                    alt: true,
                    ..Default::default()
                },
            );
            cx.run_until_parked();
            cx.simulate_input("WHO='А Б'");
            cx.simulate_keystrokes("enter");
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    observer.read(cx).launched.len(),
                    2,
                    "{name}: optional variables remain available on explicit configuration"
                );
                assert!(observer.read(cx).launched[1].line.ends_with("'WHO=А Б'"));
            });
        }
    }
}
#[gpui::test]
fn launch_saves_reparses_and_refuses_disk_conflicts(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("justfile");
    std::fs::write(&path, "run who:\n    echo {{who}}\n").unwrap();
    let mut made = None;
    let (host, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("justfile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        let kept = file.clone();
        let host = cx.new(|cx| {
            cx.subscribe(&file, |host: &mut Host, _, event: &LaunchRun, cx| {
                host.launched.push(event.0.clone());
                cx.notify();
            })
            .detach();
            Host {
                file,
                launched: Vec::new(),
            }
        });
        made = Some((kept, host.clone()));
        gpui_component::Root::new(host, window, cx)
    });
    let (file, observer) = made.unwrap();
    cx.simulate_resize(gpui::size(px(600.), px(400.)));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(file.read(cx).tasks().len(), 1));
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    let control = cx
        .debug_bounds("editor-task-control")
        .expect("native task control after painted geometry");
    cx.update(|_, cx| {
        let editor = file.read(cx).editor().unwrap().read(cx);
        let text = editor.range_to_bounds(&(0..0)).unwrap();
        assert!((f32::from(control.top() - text.top())).abs() < 1.);
    });
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            let task = source::parse(Path::new("justfile"), &file.clean).remove(0);
            file.choose_task(vec![task.clone()], cx);
            file.submit_task(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            assert!(
                file.launch.as_ref().unwrap().error.is_some(),
                "required args do not launch"
            );
            file.launch.as_mut().unwrap().values[0] = "a ' quote; $(touch NEVER)".into();
            std::fs::write(&path, "changed:\n    echo external\n").unwrap();
            file.submit_task(window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            assert!(file.launch.as_ref().unwrap().error.is_some());
            assert!(
                std::fs::read_to_string(&path).unwrap().contains("external"),
                "no overwrite of unobserved disk edits"
            );
            std::fs::write(&path, &file.clean).unwrap();
            file.submit_task(window, cx);
            // The save's watcher invalidates discovery before verification lands.
            file.discover_tasks(cx);
        })
    });
    cx.run_until_parked();
    let _ = host;
    cx.update(|_, cx| {
        file.update(cx, |file, _| assert!(file.launch.is_none()));
        let launched = &observer.read(cx).launched;
        assert_eq!(launched.len(), 1);
        assert_eq!(launched[0].cwd, dir.path());
        assert!(launched[0].line.contains("'a '"));
    });
    assert!(!dir.path().join("NEVER").exists());
    for changed in ["gone:\n    echo ok\n", "run who extra:\n    echo ok\n"] {
        cx.update(|window, cx| {
            file.update(cx, |file, cx| {
                let old =
                    source::parse(Path::new("justfile"), "run who:\n    echo {{who}}\n").remove(0);
                file.choose_task(vec![old], cx);
                file.launch.as_mut().unwrap().values[0] = "who".into();
                file.editor
                    .as_ref()
                    .unwrap()
                    .update(cx, |editor, cx| editor.set_value(changed, window, cx));
                file.submit_task(window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                observer.read(cx).launched.len(),
                1,
                "removed or changed declaration cannot launch old arguments"
            );
            assert!(file.read(cx).write_failed.is_some());
        });
    }
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.editor.as_ref().unwrap().update(cx, |editor, cx| {
                editor.set_value("run who:\n    echo {{who}}\n", window, cx)
            });
            file.save(cx).unwrap();
            file.choose_task(source::parse(Path::new("justfile"), &file.clean), cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("task-parameter-field").unwrap().size.height > px(0.));
    cx.simulate_input("Keyboard value");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            observer.read(cx).launched.len(),
            2,
            "keyboard can submit the focused parameter field"
        );
        assert!(
            observer.read(cx).launched[1]
                .line
                .contains("Keyboard value")
        );
    });
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.activate_task(file.tasks.clone(), false, window, cx)
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            file.read(cx).launch.is_none(),
            "configured required arguments rerun without another prompt"
        );
        assert_eq!(observer.read(cx).launched.len(), 3);
    });
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.activate_task(file.tasks.clone(), true, window, cx)
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| {
        assert!(file.read(cx).launch.is_none());
        assert_eq!(
            observer.read(cx).launched.len(),
            3,
            "cancelling explicit configuration never launches"
        );
    });
}

#[gpui::test]
fn new_unsaved_task_in_an_empty_source_has_a_control(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("justfile"), "# empty\n").unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("justfile"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Default::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.run_until_parked();
    cx.simulate_input("new:\n    echo ok\n");
    cx.run_until_parked();
    let file = made.unwrap();
    cx.update(|_, cx| {
        file.update(cx, |file, cx| {
            assert_eq!(file.tasks().len(), 1);
            file.run_file(cx);
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| {
        assert!(
            file.read(cx).launch.is_none(),
            "Escape cancels without a launch"
        )
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("justfile")).unwrap(),
        "# empty\n"
    );
}
