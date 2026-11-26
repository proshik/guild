use super::*;
use gpui::{AppContext, Entity, Render, TestAppContext, div, px};

struct Host {
    root: Entity<Root>,
    _changed: gpui::Subscription,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            root.viewport = window.viewport_size();
            div().size_full().children(root.render_picker(window, cx))
        })
    }
}

#[test]
fn isolated_recent_picker_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::recent_files::window_tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_RECENT_PICKER_TEST", "1")
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
#[ignore = "run through isolated_recent_picker_window_regressions"]
fn populated_faces_have_readable_bounded_rows_and_reveal_keyboard_selection(
    cx: &mut TestAppContext,
) {
    assert_eq!(
        std::env::var("GUILD_RECENT_PICKER_TEST").as_deref(),
        Ok("1")
    );
    let dir = tempfile::tempdir().unwrap();
    let files: Vec<PathBuf> = (0..50).map(|n| format!("file-{n:02}.txt").into()).collect();
    for path in &files {
        std::fs::write(dir.path().join(path), "one\ntwo\nthree\nfour\nfive\n").unwrap();
    }
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
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
            root.recent_files.lists.insert(project, files.clone());
            root
        });
        entity = Some(root.clone());
        let host = cx.new(|cx| Host {
            _changed: cx.observe(&root, |_, _, cx| cx.notify()),
            root,
        });
        gpui_component::Root::new(host, window, cx)
    });
    let root = entity.unwrap();
    cx.update(|window, _| window.activate_window());
    for viewport in [(900., 700.), (360., 240.)] {
        cx.simulate_resize(gpui::size(px(viewport.0), px(viewport.1)));
        for face in [Face::Files, Face::Locations] {
            for count in [2, 25, 50] {
                cx.update(|window, cx| {
                    root.update(cx, |root, cx| {
                        let project = root.active_project.unwrap();
                        root.recent_files
                            .lists
                            .insert(project, files[..count].to_vec());
                        root.open_picker(
                            Holds::RecentFiles(Popup {
                                face,
                                locations: files[..count]
                                    .iter()
                                    .map(|path| Location {
                                        place: crate::root::history::Place::new(path.clone(), 2, 0),
                                        lines: (0..EXCERPT)
                                            .map(|n| format!("context {n}"))
                                            .collect(),
                                        first: 1,
                                    })
                                    .collect(),
                                edited: false,
                                pending_close: None,
                            }),
                            Some(project),
                            Scroll::children(),
                            window,
                            cx,
                        );
                    })
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    let open = root.read(cx).picker.as_ref().unwrap();
                    let Scroll::Children(scroll) = &open.scroll else { unreachable!() };
                    let bounds = scroll.bounds();
                    let first = scroll.bounds_for_item(0).unwrap();
                    assert!(bounds.size.height >= px(60.), "{face:?}/{count}/{viewport:?}: list collapsed to {bounds:?}");
                    assert!(first.size.height >= if face == Face::Files { px(22.) } else { px(28.) }, "{face:?}/{count}/{viewport:?}: rows must retain their readable height: {first:?}");
                    assert!(bounds.origin.x >= px(0.) && bounds.right() <= px(viewport.0), "list width must fit the window");
                    assert!(bounds.origin.y >= px(0.) && bounds.bottom() <= px(viewport.1), "list height must fit the window");
                    assert_eq!(crate::text_field::focused(window, cx), face == Face::Locations, "only Locations owns a query input");
                });
                if count == 25 && face == Face::Locations {
                    // Exercise the real input adapter while its list disappears
                    // and comes back; model updates must retain undo and focus.
                    cx.update(|_, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string("file-00".into()))
                    });
                    cx.simulate_keystrokes("cmd-v");
                    cx.update(|window, cx| {
                        assert_eq!(root.read(cx).picker_query(), "file-00");
                        assert!(crate::text_field::focused(window, cx));
                    });
                    cx.simulate_keystrokes("cmd-a z z z");
                    cx.update(|_, cx| assert_eq!(root.read(cx).picker_lit_row(), None));
                    cx.simulate_keystrokes("cmd-a backspace");
                    cx.update(|_, cx| assert_eq!(root.read(cx).picker_query(), ""));
                    cx.simulate_keystrokes("cmd-z");
                    cx.update(|_, cx| assert_eq!(root.read(cx).picker_query(), "zzz"));
                    cx.simulate_keystrokes("cmd-shift-z");
                    cx.update(|window, cx| {
                        assert_eq!(root.read(cx).picker_query(), "");
                        assert!(crate::text_field::focused(window, cx));
                    });
                    let point = cx.update(|_, cx| {
                        let Scroll::Children(scroll) =
                            &root.read(cx).picker.as_ref().unwrap().scroll
                        else {
                            unreachable!()
                        };
                        scroll.bounds().center()
                    });
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position: point,
                        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-120.))),
                        ..Default::default()
                    });
                    cx.run_until_parked();
                    cx.update(|_, cx| {
                        let Scroll::Children(scroll) =
                            &root.read(cx).picker.as_ref().unwrap().scroll
                        else {
                            unreachable!()
                        };
                        assert!(scroll.offset().y < px(0.), "wheel must move the real list");
                    });
                }
                for _ in 0..count - 1 {
                    cx.simulate_keystrokes("down");
                }
                cx.run_until_parked();
                cx.update(|_, cx| {
                    let open = root.read(cx).picker.as_ref().unwrap();
                    assert_eq!(open.picker.lit(), Some(count - 1));
                    let Scroll::Children(scroll) = &open.scroll else {
                        unreachable!()
                    };
                    let mut row = scroll.bounds_for_item(count - 1).unwrap();
                    row.origin += scroll.offset();
                    let view = scroll.bounds();
                    assert!(
                        row.top() >= view.top() && row.bottom() <= view.bottom(),
                        "keyboard must reveal the chosen row: {row:?} in {view:?}"
                    );
                });
                if count == 2 {
                    let expected = files[count - 1].clone();
                    cx.simulate_keystrokes("enter");
                    cx.update(|_, cx| {
                        assert!(root.read(cx).picker.is_none());
                        assert!(matches!(root.read(cx).active_body(), Some(Body::File { origin: crate::Origin::Project, path, .. }) if path == &expected), "Enter must open the highlighted file");
                    });
                    // Reopen using the actual face entry point, then click the
                    // same item after revealing it with the keyboard.
                    if face == Face::Files {
                        cx.update(|window, cx| {
                            root.update(cx, |root, cx| root.toggle_recent(face, window, cx))
                        });
                        cx.run_until_parked();
                        cx.simulate_keystrokes("down");
                        let position = cx.update(|_, cx| {
                            let open = root.read(cx).picker.as_ref().unwrap();
                            let Scroll::Children(scroll) = &open.scroll else {
                                unreachable!()
                            };
                            let mut bounds =
                                scroll.bounds_for_item(open.picker.lit().unwrap()).unwrap();
                            bounds.origin += scroll.offset();
                            bounds.center()
                        });
                        cx.simulate_click(position, gpui::Modifiers::none());
                        cx.update(|_, cx| {
                            assert!(root.read(cx).picker.is_none());
                            assert!(matches!(root.read(cx).active_body(), Some(Body::File { origin: crate::Origin::Project, path, .. }) if path == &expected), "mouse and Enter must open the same item");
                        });
                    }
                } else {
                    cx.simulate_keystrokes("escape");
                    cx.update(|_, cx| assert!(root.read(cx).picker.is_none()));
                }
            }
        }
    }
}
