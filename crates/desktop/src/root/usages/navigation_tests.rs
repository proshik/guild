//! Full result-list dispatch: a file selection must not hide match navigation.
use super::*;
use gpui::{Entity, Render, TestAppContext, div, prelude::*, px};

struct Host {
    root: Entity<Root>,
    find: bool,
    _changed: gpui::Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            div()
                .size_full()
                .relative()
                .flex()
                .flex_col()
                .child(crate::view::resize::capture(
                    cx,
                    Root::resize_motion,
                    Root::resize_release,
                ))
                .children(if self.find {
                    root.render_find(cx)
                } else {
                    root.render_search(cx)
                })
        })
    }
}

fn file_to_match(cx: &mut TestAppContext, find: bool) {
    assert_eq!(std::env::var("GUILD_NAVIGATION_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("docs")).unwrap();
    for name in ["a.rs", "b.rs"] {
        std::fs::write(dir.path().join("docs").join(name), "needle\n").unwrap();
    }
    let mut walk = crate::search::Walk::default();
    walk.restart(
        dir.path(),
        crate::search::Query {
            text: "needle".into(),
            ..Default::default()
        },
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while walk.running() {
        assert!(Instant::now() < deadline, "fixture search timed out");
        walk.take();
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(walk.hits().len(), 2);
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            r.active_project = Some(project);
            if find {
                let mut state = FindState::new(0.5, cx);
                state.tabs.push(FindTab {
                    serial: 1,
                    ask: Ask {
                        project,
                        path: "docs/a.rs".into(),
                        language: Language::of(std::path::Path::new("a.rs")).unwrap(),
                        position: Position::new(0, 0),
                        name: "needle".into(),
                    },
                    searching: false,
                    requests: Vec::new(),
                    found: Found {
                        hits: walk.hits().to_vec(),
                        project: 2,
                        links: walk
                            .hits()
                            .iter()
                            .map(|hit| {
                                let range =
                                    lsp_types::Range::new(Position::new(0, 0), Position::new(0, 6));
                                LocationLink {
                                    origin_selection_range: None,
                                    target_uri: crate::lsp::uri(&dir.path().join(&hit.path))
                                        .parse()
                                        .unwrap(),
                                    target_range: range,
                                    target_selection_range: range,
                                }
                            })
                            .collect(),
                    },
                    target: None,
                    folds: Folds::default(),
                    selected: None,
                    files: Default::default(),
                    explicit_file_selection: false,
                    scroll: UniformListScrollHandle::new(),
                });
                r.finds.by_project.insert(project, state);
            } else {
                r.ensure_search(window, cx);
                let state = r.searches.get_mut(&project).unwrap();
                state.query.text = "needle".into();
                state.walk = walk;
            }
            r
        });
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            root: entity,
            find,
        });
        gpui_component::Root::new(host, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(900.), px(500.)));
    let root = root.unwrap();
    if find {
        for direction in [1., -1.] {
            let start = cx.debug_bounds("find-grip").unwrap().center();
            let before = cx.update(|_, cx| root.read(cx).find_state().unwrap().split.fraction());
            cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
            cx.run_until_parked();
            for step in 1..=40 {
                cx.simulate_mouse_move(
                    start + gpui::point(px(step as f32 * direction), px(0.)),
                    gpui::MouseButton::Left,
                    Default::default(),
                );
                cx.run_until_parked();
            }
            let after = cx.update(|_, cx| root.read(cx).find_state().unwrap().split.fraction());
            assert!(
                (after - before) * direction >= 0.03,
                "slow Find drag must accumulate movement: {before} -> {after}"
            );
            cx.simulate_mouse_up(
                start + gpui::point(px(60. * direction), px(0.)),
                gpui::MouseButton::Left,
                Default::default(),
            );
            cx.run_until_parked();
            cx.update(|_, cx| assert!(root.read(cx).find_state().unwrap().drag.is_none()));
        }
    }
    // Modifier click selects without folding the file's visible matches.
    let mut file_index = 0;
    cx.update(|window, cx| root.update(cx, |r, cx| {
        let mods = gpui::Modifiers { platform: true, ..Default::default() };
        if find {
            file_index = r.find_state().unwrap().tab().unwrap().rows().iter()
                .position(|row| usages::file_of(row).is_some_and(|(_, path)| path == std::path::Path::new("docs/a.rs"))).unwrap();
            r.find_row_click(file_index, mods, window, cx);
        } else {
            file_index = r.search_rows().iter().position(|row| matches!(row, crate::search_tree::Row::File { path, .. } if path == std::path::Path::new("docs/a.rs"))).unwrap();
            r.search_file_click("docs/a.rs".into(), mods, window, cx);
        }
    }));
    cx.simulate_keystrokes("down");
    cx.update(|window, cx| {
        let r = root.read(cx);
        if find {
            let state = r.find_state().unwrap();
            let tab = state.tab().unwrap();
            assert_eq!(
                tab.selected,
                Some(file_index + 1),
                "Down must reach the match, not the next file"
            );
            assert!(tab.files.selected.is_empty());
            assert!(state.focus.is_focused(window));
        } else {
            let state = r.search_state().unwrap();
            assert_eq!(
                state.selected,
                Some(file_index + 1),
                "Down must reach the match, not the next file"
            );
            assert!(state.files.selected.is_empty());
            assert!(state.results_focus.is_focused(window));
        }
    });
    cx.simulate_keystrokes("up");
    cx.update(|_, cx| {
        let r = root.read(cx);
        if find {
            let tab = r.find_state().unwrap().tab().unwrap();
            assert_eq!(tab.selected, Some(file_index));
            assert!(
                tab.files
                    .contains(&(usages::Group::Project, "docs/a.rs".into())),
                "the file row must be visibly selected"
            );
        } else {
            let state = r.search_state().unwrap();
            assert_eq!(state.selected, Some(file_index));
            assert!(
                state.files.contains(std::path::Path::new("docs/a.rs")),
                "the file row must be visibly selected"
            );
        }
    });
    // Plain row navigation retains Enter's group-folding behavior.
    for expected in [true, false] {
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| {
            let r = root.read(cx);
            let folded = if find {
                let tab = r.find_state().unwrap().tab().unwrap();
                tab.folds.is_folded(&tab.rows()[file_index])
            } else {
                r.search_state()
                    .unwrap()
                    .folded
                    .contains(std::path::Path::new("docs/a.rs"))
            };
            assert_eq!(folded, expected, "Enter must toggle a navigated file group");
            assert!(r.tabs.is_empty(), "folding must not open an editor");
        });
    }
    // Command-click is the gesture that takes a row out of the selection
    // and back — Control-Space no longer is (decision 54).
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let mods = gpui::Modifiers {
                platform: true,
                ..Default::default()
            };
            if find {
                r.find_row_click(file_index, mods, window, cx);
            } else {
                r.search_file_click("docs/a.rs".into(), mods, window, cx);
            }
        })
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(r.tabs.is_empty(), "Enter must not open a deselected file");
        let folded = if find {
            let tab = r.find_state().unwrap().tab().unwrap();
            tab.folds.is_folded(&tab.rows()[file_index])
        } else {
            r.search_state()
                .unwrap()
                .folded
                .contains(std::path::Path::new("docs/a.rs"))
        };
        assert!(
            !folded,
            "explicit deselection must not turn Enter into folding"
        );
    });
    // Command-click is the gesture that takes a row out of the selection
    // and back — Control-Space no longer is (decision 54).
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let mods = gpui::Modifiers {
                platform: true,
                ..Default::default()
            };
            if find {
                r.find_row_click(file_index, mods, window, cx);
            } else {
                r.search_file_click("docs/a.rs".into(), mods, window, cx);
            }
        })
    });
    cx.simulate_keystrokes("shift-down");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let count = if find {
            r.find_state().unwrap().tab().unwrap().files.selected.len()
        } else {
            r.search_state().unwrap().files.selected.len()
        };
        assert_eq!(count, 2, "modified arrows still extend file selection");
    });
    cx.simulate_keystrokes("up");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let index = if find {
            r.find_state().unwrap().tab().unwrap().selected
        } else {
            r.search_state().unwrap().selected
        };
        assert_eq!(
            index,
            Some(file_index + 1),
            "plain arrows leave multi-selection for the preceding match"
        );
    });
    cx.simulate_keystrokes("home up");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let index = if find {
            r.find_state().unwrap().tab().unwrap().selected
        } else {
            r.search_state().unwrap().selected
        };
        assert_eq!(
            index,
            Some(0),
            "Home and Up reach and clamp at the first tree row"
        );
    });
    cx.simulate_keystrokes("end down");
    cx.update(|_, cx| {
        let r = root.read(cx);
        if find {
            let tab = r.find_state().unwrap().tab().unwrap();
            assert_eq!(tab.selected, Some(6));
            assert!(tab.files.selected.is_empty());
        } else {
            assert_eq!(r.search_state().unwrap().selected, Some(4));
            assert!(r.search_state().unwrap().files.selected.is_empty());
        }
    });
    // An explicit single-file selection still opens on Enter.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let mods = gpui::Modifiers {
                platform: true,
                ..Default::default()
            };
            if find {
                r.find_row_click(file_index, mods, window, cx);
            } else {
                r.search_file_click("docs/a.rs".into(), mods, window, cx);
            }
        })
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(matches!(root.read(cx).active_body(), Some(crate::Body::File { path, origin: crate::Origin::Project, .. }) if path == std::path::Path::new("docs/a.rs")));
    });
    cx.update(|window, cx| {
        let r = root.read(cx);
        let focus = if find {
            r.find_state().unwrap().focus.clone()
        } else {
            r.search_state().unwrap().results_focus.clone()
        };
        window.focus(&focus, cx);
    });
    cx.simulate_keystrokes("cmd-a enter");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let r = root.read(cx);
        for name in ["docs/a.rs", "docs/b.rs"] {
            assert!(r.tabs.iter().flat_map(crate::Tab::bodies).any(|body| matches!(body, crate::Body::File { path, origin: crate::Origin::Project, .. } if path == std::path::Path::new(name))), "Select All and Enter must open {name}");
        }
    });
}

#[gpui::test]
#[ignore = "run through isolated_mixed_tree_navigation"]
fn search_file_selection_keeps_matches_reachable(cx: &mut TestAppContext) {
    file_to_match(cx, false);
}

#[gpui::test]
#[ignore = "run through isolated_mixed_tree_navigation"]
fn find_file_selection_keeps_matches_reachable(cx: &mut TestAppContext) {
    file_to_match(cx, true);
}

#[test]
fn isolated_mixed_tree_navigation() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::usages::navigation_tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NAVIGATION_TEST", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
