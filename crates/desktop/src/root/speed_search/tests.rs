use crate::Root;
mod rows;
use gpui::Focusable;
use gpui::{AppContext, Context, Entity, Render, TestAppContext, Window, div, prelude::*, px};

struct Host {
    root: Entity<Root>,
    mode: u8,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .when(self.mode == 2, |host| host.key_context("guild"))
                .children(match self.mode {
                    1 => root.render_changes(cx),
                    2 => root.render_log(window, cx),
                    _ => root.render_tree(cx),
                })
        })
    }
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_first_character_editing_and_escape(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("spec.rs"), "").unwrap();
    std::fs::create_dir(dir.path().join("folded")).unwrap();
    std::fs::write(dir.path().join("folded/hidden.rs"), "").unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.ensure_active_tree(cx);
            root
        });
        entity
            .read(cx)
            .tree()
            .unwrap()
            .focus
            .clone()
            .focus(window, cx);
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            root: entity,
            mode: 0,
        });
        gpui_component::Root::new(host, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(400.), px(250.)));
    cx.simulate_keystrokes("s");
    cx.update(|_, cx| {
        assert_eq!(
            root.as_ref()
                .unwrap()
                .read(cx)
                .tree()
                .unwrap()
                .speed
                .as_ref()
                .map(|s| s.query.as_str()),
            Some("s")
        )
    });
    assert!(
        cx.debug_bounds("speed-search").is_some(),
        "typing in Explorer must open speed search"
    );
    let root = root.unwrap();
    cx.update(|_, cx| assert!(root.read(cx).tabs.is_empty(), "typing must not open a file"));
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().speed.as_ref().unwrap().query,
            "",
            "the activating character must be undoable"
        )
    });
    cx.simulate_keystrokes("cmd-shift-z");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().speed.as_ref().unwrap().query,
            "s"
        )
    });
    cx.simulate_keystrokes("p e c");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let s = r.tree().unwrap().speed.as_ref().unwrap();
        assert_eq!(s.query, "spec");
        assert_eq!(
            s.matched(&r.tree_speed_rows()).unwrap().key.1,
            std::path::Path::new("spec.rs")
        );
    });
    cx.simulate_keystrokes("cmd-a h i d d e n");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let s = r.tree().unwrap().speed.as_ref().unwrap();
        assert_eq!(s.query, "hidden");
        assert!(s.matched(&r.tree_speed_rows()).is_none());
    });
    cx.simulate_keystrokes("escape");
    assert!(cx.debug_bounds("speed-search").is_none());
    cx.update(|window, cx| assert!(root.read(cx).tree().unwrap().focus.is_focused(window)));
    // A file cursor must not trap ordinary arrows in a file-only list.
    cx.simulate_keystrokes("down down down");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().selected.as_deref(),
            Some(std::path::Path::new("spec.rs"))
        );
    });
    cx.simulate_keystrokes("up");
    cx.update(|window, cx| {
        let state = root.read(cx).tree().unwrap();
        assert_eq!(
            state.selected.as_deref(),
            Some(std::path::Path::new("folded"))
        );
        assert!(state.focus.is_focused(window));
        assert_eq!(
            state.files.selected,
            vec![(crate::Origin::Project, "folded".into())]
        );
    });
    cx.simulate_keystrokes("down up");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().selected.as_deref(),
            Some(std::path::Path::new("folded"))
        );
    });
    cx.simulate_keystrokes("home up");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().selected.as_deref(),
            Some(std::path::Path::new(""))
        )
    });
    cx.simulate_keystrokes("end down");
    // A Command-click takes the file out of the selection and leaves the
    // cursor on it — Control-Space no longer does (decision 54).
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let (origin, path) = r.tree().unwrap().files.focus.clone().unwrap();
            let mods = gpui::Modifiers {
                platform: true,
                ..Default::default()
            };
            r.tree_file_click(origin, &path, mods, window, cx);
        })
    });
    cx.simulate_keystrokes("right enter f4");
    cx.update(|_, cx| {
        assert!(
            root.read(cx).tabs.is_empty(),
            "a deselected file must not open"
        )
    });
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert!(
            state.selected.is_none(),
            "a deselected cursor is not an action target"
        );
        assert!(state.files.selected.is_empty());
    });
    cx.simulate_keystrokes("up");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().selected.as_deref(),
            Some(std::path::Path::new("folded"))
        )
    });
    let library = dir.path().join("external-package");
    std::fs::create_dir(&library).unwrap();
    std::fs::write(library.join("outside.rs"), "").unwrap();
    std::fs::create_dir_all(library.join("a/b/c")).unwrap();
    std::fs::write(library.join("a/b/c/end.txt"), "").unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.note_library(r.active_project.unwrap(), &library);
            r.tree_mut().unwrap().libraries_open = true;
            cx.notify();
        })
    });
    cx.simulate_keystrokes("e x t e r n a l enter");
    cx.update(|_, cx| {
        assert!(
            root.read(cx).tree().unwrap().library_trees.contains_key(&0),
            "Enter on a collapsed library root must expand that root"
        );
        assert_eq!(
            root.read(cx).tree().unwrap().library_selected,
            Some((0, "".into())),
            "activating a library root must leave navigation on that root"
        );
    });
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let tree = r.tree_mut().unwrap().library_trees.get_mut(&0).unwrap();
            for path in ["a", "a/b", "a/b/c"] {
                tree.expand(std::path::Path::new(path));
            }
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a / b / c enter");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let tree = &r.tree().unwrap().library_trees[&0];
            let rows = tree.rows();
            assert!(
                rows.iter()
                    .any(|row| row.path() == std::path::Path::new("a") && !row.is_expanded()),
                "search Enter collapses the entire displayed library chain"
            );
            drop(rows);
            r.library_click(
                0,
                std::path::Path::new("a"),
                true,
                Default::default(),
                window,
                cx,
            );
            r.library_click(
                0,
                std::path::Path::new("a/b/c"),
                true,
                Default::default(),
                window,
                cx,
            );
            assert!(
                r.tree().unwrap().library_trees[&0]
                    .rows()
                    .iter()
                    .any(|row| row.path() == std::path::Path::new("a") && !row.is_expanded()),
                "ordinary click uses the same chain root"
            );
        })
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.tree_file_click(
                crate::Origin::External(0),
                std::path::Path::new("outside.rs"),
                gpui::Modifiers {
                    platform: true,
                    ..Default::default()
                },
                window,
                cx,
            );
        })
    });
    // The root's context-menu fold must retain the library cursor too.
    for expanded in [true, false] {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                window.focus(&r.focus, cx);
                r.run_deed(
                    crate::view::menu::Deed::Tree(crate::view::menu::TreeDeed::ToggleDirectory),
                    &crate::view::menu::Target::LibraryRow {
                        at: 0,
                        path: "".into(),
                        is_dir: true,
                        expanded,
                    },
                    window,
                    cx,
                );
                assert_eq!(r.tree().unwrap().library_selected, Some((0, "".into())));
                assert!(r.tree().unwrap().focus.is_focused(window));
            })
        });
    }
    cx.simulate_keystrokes("end");
    cx.simulate_keystrokes("up");
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert_eq!(
            state.library_selected,
            Some((0, "a".into())),
            "library arrows must reach directories in the same source"
        );
        assert!(
            state.selected.is_none(),
            "library navigation must not target project actions"
        );
        assert!(state.files.selected.is_empty());
    });
    cx.simulate_keystrokes("left");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().library_selected,
            Some((0, "".into()))
        )
    });
    cx.simulate_keystrokes("right right");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().library_selected,
            Some((0, "a/b/c".into()))
        )
    });
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert_eq!(state.library_selected, Some((0, "a/b/c/end.txt".into())));
        assert!(
            state
                .files
                .contains(&(crate::Origin::External(0), "a/b/c/end.txt".into()))
        );
    });
    cx.simulate_keystrokes("left left");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).tree().unwrap().library_selected,
            Some((0, "a".into()))
        )
    });
    cx.simulate_keystrokes("o u t s i d e enter");
    cx.update(|_, cx| assert!(matches!(root.read(cx).active_body(), Some(crate::Body::File { origin: crate::Origin::External(0), path, .. }) if path == std::path::Path::new("outside.rs"))));
    // Rename starts with the stem selected once, then leaves ordinary editing alone.
    for (name, directory, selected) in [
        ("report.test.ts", false, "report.test"),
        (".gitignore", false, ".gitignore"),
        ("notes", false, "notes"),
        ("docs.v2", true, "docs.v2"),
        ("проект🙂.rs", false, "проект🙂"),
    ] {
        if directory {
            std::fs::create_dir(dir.path().join(name)).unwrap();
        } else {
            std::fs::write(dir.path().join(name), "original").unwrap();
        }
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.run_deed(
                    crate::view::menu::Deed::Tree(crate::view::menu::TreeDeed::Rename),
                    &crate::view::menu::Target::TreeRow {
                        path: name.into(),
                        is_dir: directory,
                        expanded: false,
                    },
                    window,
                    cx,
                );
            })
        });
        cx.simulate_keystrokes("cmd-c");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(selected),
                "initial selection for {name}"
            )
        });
        cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("changed".into())));
        cx.simulate_keystrokes("cmd-v");
        let expected = if directory || !name.contains('.') || name == ".gitignore" {
            "changed"
        } else if name.ends_with(".ts") {
            "changed.ts"
        } else {
            "changed.rs"
        };
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).tree().unwrap().naming.as_ref().unwrap().typed,
                expected
            )
        });
        cx.simulate_keystrokes("cmd-z");
        cx.update(|_, cx| {
            assert_eq!(
                root.read(cx).tree().unwrap().naming.as_ref().unwrap().typed,
                name
            )
        });
        cx.simulate_keystrokes("cmd-shift-z");
        cx.simulate_keystrokes("cmd-left shift-cmd-right cmd-c");
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected)
            )
        });
        cx.simulate_keystrokes("cmd-right alt-left shift-alt-right cmd-c");
        cx.update(|_, cx| assert!(!cx.read_from_clipboard().unwrap().text().unwrap().is_empty()));
        cx.simulate_keystrokes("escape");
        assert!(dir.path().join(name).exists());
    }
    // Menu targets are independent of the selected library row.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.tree_file_click(
                crate::Origin::External(0),
                std::path::Path::new("outside.rs"),
                gpui::Modifiers {
                    platform: true,
                    ..Default::default()
                },
                window,
                cx,
            );
            r.run_deed(
                crate::view::menu::Deed::Tree(crate::view::menu::TreeDeed::Rename),
                &crate::view::menu::Target::TreeRow {
                    path: "spec.rs".into(),
                    is_dir: false,
                    expanded: false,
                },
                window,
                cx,
            );
        })
    });
    cx.simulate_keystrokes("cmd-a r e n a m e d . r s");
    cx.update(|window, cx| {
        assert!(
            crate::text_field::focused(window, cx),
            "the real name input must own the keys"
        )
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(dir.path().join("renamed.rs").is_file());
    cx.update(|window, cx| {
        let state = root.read(cx).tree().unwrap();
        assert_eq!(
            state.selected.as_deref(),
            Some(std::path::Path::new("renamed.rs"))
        );
        assert!(
            state.library_selected.is_none(),
            "a project rename must leave the library source"
        );
        assert!(
            state.focus.is_focused(window),
            "closing the name input must return the keys to Explorer"
        );
    });
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| assert!(matches!(root.read(cx).active_body(), Some(crate::Body::File { origin: crate::Origin::Project, path, .. }) if path == std::path::Path::new("renamed.rs"))));
    // Successful folder operations and cancellation dispose the same input.
    use crate::view::menu::{Deed, Target, TreeDeed};
    for (deed, path, name, commit) in [
        (TreeDeed::NewDirectory, "", "dirone", true),
        (TreeDeed::Rename, "dirone", "dirtwo", true),
        (TreeDeed::Rename, "dirtwo", "discarded", false),
    ] {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.tree_file_click(
                    crate::Origin::External(0),
                    std::path::Path::new("outside.rs"),
                    gpui::Modifiers {
                        platform: true,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                r.run_deed(
                    Deed::Tree(deed),
                    &Target::TreeRow {
                        path: path.into(),
                        is_dir: true,
                        expanded: false,
                    },
                    window,
                    cx,
                );
            })
        });
        let keys = name
            .chars()
            .map(|ch| ch.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        cx.simulate_keystrokes(&format!("cmd-a {keys}"));
        cx.update(|window, cx| assert!(crate::text_field::focused(window, cx)));
        cx.simulate_keystrokes(if commit { "enter" } else { "escape" });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let state = root.read(cx).tree().unwrap();
            assert!(state.naming.is_none());
            assert!(
                state.focus.is_focused(window),
                "Explorer must retain focus after {deed:?}, commit={commit}"
            );
            if commit {
                assert!(state.library_selected.is_none());
                assert_eq!(state.selected.as_deref(), Some(std::path::Path::new(name)));
                assert!(dir.path().join(name).is_dir());
            } else {
                // Context operations target the pressed project row even when cancelled.
                assert!(state.library_selected.is_none());
                assert_eq!(state.selected.as_deref(), Some(std::path::Path::new(path)));
                assert!(!dir.path().join(name).exists());
            }
        });
    }
    // A refused name keeps the live input; only cancellation hands focus back.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.run_deed(
                Deed::Tree(TreeDeed::Rename),
                &Target::TreeRow {
                    path: "dirtwo".into(),
                    is_dir: true,
                    expanded: false,
                },
                window,
                cx,
            );
        })
    });
    cx.simulate_keystrokes("cmd-a f o l d e d enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(root.read(cx).tree().unwrap().naming.is_some());
        assert!(crate::text_field::focused(window, cx));
    });
    assert!(dir.path().join("dirtwo").is_dir());
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(root.read(cx).tree().unwrap().focus.is_focused(window)));
    // Creating a file and renaming its open editor still open the resulting path.
    for (deed, path, name) in [
        (TreeDeed::NewFile, "", "one.rs"),
        (TreeDeed::Rename, "one.rs", "two.rs"),
    ] {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.run_deed(
                    Deed::Tree(deed),
                    &Target::TreeRow {
                        path: path.into(),
                        is_dir: path.is_empty(),
                        expanded: false,
                    },
                    window,
                    cx,
                );
            })
        });
        let keys = name
            .chars()
            .map(|ch| ch.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        cx.simulate_keystrokes(&format!("cmd-a {keys} enter"));
        cx.run_until_parked();
        assert!(dir.path().join(name).is_file());
        cx.update(|_, cx| assert!(matches!(root.read(cx).active_body(), Some(crate::Body::File { origin: crate::Origin::Project, path, .. }) if path == std::path::Path::new(name))));
    }
}

#[test]
fn isolated_speed_search_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::speed_search::tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_SPEED_SEARCH_TEST", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn file_host(cx: &mut TestAppContext, log: bool) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap()
            .success()
    );
    let names = ["a.rs", "b.rs", "c target.rs"];
    for name in names {
        std::fs::write(dir.path().join(name), "file").unwrap();
    }
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            let entries = names
                .map(|name| crate::git::status::Entry {
                    path: name.into(),
                    kind: crate::git::status::Kind::Modified,
                    staged: false,
                    unstaged: true,
                    is_dir: false,
                })
                .to_vec();
            root.git_status.insert(
                project,
                Ok(crate::git::status::StatusMap::from_entries(entries)),
            );
            root.refresh_changes(project, cx);
            root.ensure_message_box(window, cx);
            root.ensure_log(cx);
            let state = root.logs.get_mut(&project).unwrap();
            state.commits.push(crate::git::log::Commit {
                sha: "123456789".into(),
                parents: vec![],
                author: "Fixture".into(),
                email: "fixture@example.invalid".into(),
                time: 0,
                refs: vec![],
                summary: "Fixture".into(),
                body: String::new(),
            });
            state.opened = Some("123456789".into());
            state.files.insert(
                "123456789".into(),
                Ok(names
                    .map(|name| crate::git::log::FileChange {
                        path: name.into(),
                        mark: crate::theme::Mark::Modified,
                        from: None,
                    })
                    .to_vec()),
            );
            state.file_tree =
                crate::files_tree::Tree::new(state.files["123456789"].as_ref().unwrap());
            state.graph = std::rc::Rc::new(crate::log_graph::Graph::of(&state.commits));
            let selection = if log {
                &mut state.file_selection
            } else {
                &mut root.changes_mut().unwrap().selection
            };
            let order = names.map(std::path::PathBuf::from);
            selection.click(&order[0], &order, false, false);
            selection.click(&order[1], &order, true, false);
            root
        });
        let focus = if log {
            entity.read(cx).log().unwrap().file_focus.clone()
        } else {
            entity.read(cx).changes().unwrap().focus.clone()
        };
        focus.focus(window, cx);
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            root: entity,
            mode: if log { 2 } else { 1 },
        });
        gpui_component::Root::new(host, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(900.), px(500.)));
    let root = root.unwrap();
    if log {
        let selected = cx.update(|_, cx| root.read(cx).log().unwrap().file_selection.clone());
        for expand in [true, false] {
            cx.simulate_keystrokes("c");
            cx.update(|window, cx| {
                assert!(
                    crate::text_field::focused(window, cx),
                    "start with the actual speed-search input focused"
                )
            });
            if expand {
                cx.simulate_keystrokes("cmd-alt-=");
            } else {
                cx.simulate_keystrokes("cmd-alt--");
            }
            cx.run_until_parked();
            cx.update(|window, cx| {
                let state = root.read(cx).log().unwrap();
                assert!(state.speed.is_none());
                assert!(
                    state.file_focus.is_focused(window),
                    "fold action must return input focus to Log files"
                );
            });
            cx.simulate_keystrokes("down");
            cx.update(|window, cx| {
                let state = root.read(cx).log().unwrap();
                assert!(state.file_focus.is_focused(window));
                assert_eq!(
                    state.file_selection.focus,
                    expand.then(|| std::path::PathBuf::from("c target.rs")),
                    "arrows must navigate only visible Log files"
                );
            });
            cx.simulate_keystrokes("b");
            cx.update(|window, cx| {
                assert!(crate::text_field::focused(window, cx));
                assert_eq!(
                    root.read(cx).log().unwrap().speed.as_ref().unwrap().query,
                    "b"
                );
            });
            cx.simulate_keystrokes("escape");
            cx.simulate_keystrokes("cmd-alt-=");
            cx.run_until_parked();
        }
        cx.simulate_keystrokes("cmd-alt--");
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(root.read(cx).log().unwrap().file_tree.root_folded);
            assert!(
                root.read(cx)
                    .log()
                    .unwrap()
                    .file_selection
                    .selected
                    .is_empty()
            );
        });
        cx.simulate_keystrokes("cmd-alt-=");
        cx.run_until_parked();
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                let state = root.logs.get_mut(&root.active_project.unwrap()).unwrap();
                assert!(!state.file_tree.root_folded);
                assert_eq!(state.file_paths().len(), 3);
                state.file_selection = selected;
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            let theme = root.read(cx).theme;
            let ground = theme.file_row(
                Some(crate::theme::Mark::Modified),
                crate::theme::Selected::No,
                theme.surface.app,
            );
            let quads = window.painted_quads();
            let row = quads
                .iter()
                .find(|q| {
                    q.background.as_solid() == Some(ground.into())
                        && q.bounds.size.width.0 > 100. * window.scale_factor()
                })
                .expect("expanded Log file keeps its actual Git status fill");
            assert_eq!(
                row.bounds.right(),
                window.viewport_size().width.scale(window.scale_factor()),
                "Log status reaches the tree viewport edge after fold/expand"
            );
            assert_eq!(
                row.corner_radii,
                Default::default(),
                "Git file rows stay square"
            );
            assert!(
                quads.iter().any(|q| q.background.as_solid()
                    == Some(gpui::rgb(theme.accent.patina_bg).into())
                    && q.bounds.left() == row.bounds.left()
                    && q.bounds.right() == row.bounds.right()),
                "selection spans the same full row as Git status"
            );
        });
    }
    if !log {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                use std::path::Path;
                r.changes_context(Path::new("a.rs"), window, cx);
                assert_eq!(
                    r.changes().unwrap().selection.selected.len(),
                    2,
                    "context click on a member preserves the group"
                );
                r.changes_context(Path::new("c target.rs"), window, cx);
                assert_eq!(
                    r.changes().unwrap().selection.selected,
                    vec![std::path::PathBuf::from("c target.rs")]
                );
                let before = r.changes().unwrap().selection.selected.clone();
                r.changes_toggle_row(Path::new("c target.rs"), cx);
                assert_eq!(
                    r.changes().unwrap().selection.selected,
                    before,
                    "checkbox membership never selects rows"
                );
                r.changes_toggle_row(Path::new("c target.rs"), cx);
                let state = r.changes_mut().unwrap();
                let paths = state.visible_paths();
                state
                    .selection
                    .click(Path::new("a.rs"), &paths, false, false);
                state
                    .selection
                    .click(Path::new("b.rs"), &paths, true, false);
                r.changes_trash_selected(window, cx);
            })
        });
        cx.run_until_parked();
        let (title, detail) = cx
            .pending_prompt()
            .expect("Commit Trash asks once for the selected group");
        let confirmation = format!("{title} {detail}");
        assert!(confirmation.contains("a.rs") && confirmation.contains("b.rs"));
        assert!(
            confirmation.contains("2 selected paths")
                && confirmation.contains("hidden and ignored")
        );
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(dir.path().join("a.rs").exists() && dir.path().join("b.rs").exists());
        cx.update(|_, cx| assert_eq!(root.read(cx).changes().unwrap().selection.selected.len(), 2));
        cx.simulate_keystrokes("space");
        cx.update(|_, cx| assert_eq!(root.read(cx).changes().unwrap().ticked.len(), 1));
        assert!(cx.debug_bounds("speed-search").is_none());
    }
    cx.simulate_keystrokes("c");
    assert!(
        cx.debug_bounds("speed-search").is_some(),
        "file-list typing activates its search"
    );
    cx.simulate_keystrokes("space t a r g e t down up");
    cx.update(|_, cx| {
        let r = root.read(cx);
        let selection = if log {
            &r.log().unwrap().file_selection
        } else {
            &r.changes().unwrap().selection
        };
        // The match is the selection — decision 54; ticks
        // are not the selection and stay.
        assert_eq!(
            selection.selected,
            vec![std::path::PathBuf::from("c target.rs")]
        );
        if !log {
            assert_eq!(r.changes().unwrap().ticked.len(), 1);
        }
        assert!(r.tabs.is_empty());
    });
    // Leaving the list must dismiss without stealing focus or consuming form text.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            if log {
                window.focus(&r.log().unwrap().focus, cx);
            } else {
                r.changes()
                    .unwrap()
                    .message
                    .as_ref()
                    .unwrap()
                    .clone()
                    .update(cx, |m, cx| m.focus(window, cx));
            }
            cx.notify();
        })
    });
    cx.simulate_keystrokes("x");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(if log {
            r.log().unwrap().speed.is_none()
        } else {
            r.changes().unwrap().speed.is_none()
        });
        if log {
            assert_eq!(r.log().unwrap().filter.text, "x");
        } else {
            assert_eq!(
                r.changes()
                    .unwrap()
                    .message
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .value()
                    .as_str(),
                "x"
            );
        }
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let focus = if log {
                r.log().unwrap().file_focus.clone()
            } else {
                r.changes().unwrap().focus.clone()
            };
            window.focus(&focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("c space t a r g e t enter");
    cx.update(|_, cx| {
        let r = root.read(cx); assert_eq!(r.tabs.len(), 1);
        assert!(matches!(r.active_body(), Some(crate::Body::Diff { path: Some(p), .. }) if p == std::path::Path::new("c target.rs")));
        if !log { assert_eq!(r.changes().unwrap().ticked.len(), 1); }
    });
    if !log {
        std::fs::create_dir(dir.path().join("newdir")).unwrap();
        std::fs::write(dir.path().join("newdir/child.txt"), "child").unwrap();
        std::fs::write(dir.path().join("newdir/other.txt"), "other").unwrap();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let project = r.active_project.unwrap();
                r.git_status.insert(
                    project,
                    Ok(crate::git::status::StatusMap::from_entries(vec![
                        crate::git::status::Entry {
                            path: "newdir".into(),
                            kind: crate::git::status::Kind::Untracked,
                            staged: false,
                            unstaged: true,
                            is_dir: true,
                        },
                    ])),
                );
                r.refresh_changes(project, cx);
                window.focus(&r.changes().unwrap().focus, cx);
                cx.notify();
            })
        });
        cx.simulate_keystrokes("n e w d i r");
        cx.update(|_, cx| {
            let r = root.read(cx);
            assert_eq!(
                r.file_speed(super::FileHost::Commit)
                    .unwrap()
                    .matched(&r.file_speed_rows(super::FileHost::Commit))
                    .unwrap()
                    .key,
                std::path::Path::new("newdir")
            );
        });
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(root.read(cx).tabs.len(), 1, "a directory has no file diff"));
        cx.run_until_parked();
        cx.simulate_keystrokes("down space");
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let state = r.changes().unwrap();
                assert_eq!(
                    state.selection.action_target().unwrap(),
                    std::path::Path::new("newdir/child.txt")
                );
                assert_eq!(
                    state.membership(std::path::Path::new("newdir")),
                    (false, true)
                );
                r.refresh_changes(r.active_project.unwrap(), cx);
                window.focus(&r.changes().unwrap().focus, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let state = r.changes_mut().unwrap();
                assert_eq!(
                    state.selection.action_target().unwrap(),
                    std::path::Path::new("newdir/child.txt"),
                    "directory refresh retains the selected child identity"
                );
                assert_eq!(
                    state.membership(std::path::Path::new("newdir")),
                    (false, true)
                );
                let rows = state.visible_paths();
                state
                    .selection
                    .click(std::path::Path::new("newdir"), &rows, true, false);
                r.changes_trash_selected(window, cx);
            })
        });
        cx.run_until_parked();
        let (_, detail) = cx
            .pending_prompt()
            .expect("directory group Trash asks before touching disk");
        assert!(
            detail.contains("1 selected path"),
            "parent subsumes its selected child: {detail}"
        );
        cx.simulate_prompt_answer("Cancel");
        cx.run_until_parked();
        assert!(dir.path().join("newdir/child.txt").exists());
    }
    if !log {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                let state = r.changes_mut().unwrap();
                state.list.clear();
                window.focus(&state.focus, cx);
                cx.notify();
            })
        });
        cx.simulate_keystrokes("q");
        assert!(
            cx.debug_bounds("speed-search").is_some(),
            "an empty Commit list still accepts a query"
        );
        cx.simulate_keystrokes("enter");
        cx.update(|_, cx| assert_eq!(root.read(cx).tabs.len(), 1));
    }
}
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn commit_search_keeps_ticks_and_opens_only_the_match(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    file_host(cx, false);
}
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn log_file_search_selects_the_match_and_opens_only_it(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    file_host(cx, true);
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn refresh_and_owner_disposal_reconcile_search(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.rs"), "").unwrap();
    std::fs::write(dir.path().join("z.rs"), "").unwrap();
    std::fs::write(library.path().join("external.rs"), "").unwrap();
    let root = cx.new(|cx| {
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let mut root = Root::from_state(
            guild_session::Settings {
                panels: Some(Default::default()),
                ..Default::default()
            },
            desk,
            cx,
        );
        root.active_project = Some(project);
        root.ensure_active_tree(cx);
        root.reveal_tool(guild_session::Tool::Explorer, cx);
        root.note_library(project, library.path());
        root.tree_mut().unwrap().libraries_open = true;
        root.toggle_library(0, cx);
        root
    });
    root.update(cx, |r, _| {
        let rows = r.tree_speed_rows();
        assert!(
            rows.iter()
                .any(|c| c.key == (crate::Origin::External(0), "external.rs".into()))
        );
        r.tree_mut()
            .unwrap()
            .library_trees
            .get_mut(&0)
            .unwrap()
            .toggle(std::path::Path::new(""));
        assert!(!r.tree_speed_rows().iter().any(|c| c.label == "external.rs"));
        r.tree_mut().unwrap().speed = Some(crate::speed_search::Search::new(
            ".rs",
            Some((crate::Origin::Project, "z.rs".into())),
        ));
        std::fs::remove_file(dir.path().join("z.rs")).unwrap();
        r.tree_mut().unwrap().tree.refresh(std::path::Path::new(""));
        r.settle_speed_search();
        assert_eq!(
            r.tree().unwrap().speed.as_ref().unwrap().current,
            Some((crate::Origin::Project, "a.rs".into())),
            "refresh must replace a removed current match before the next key"
        );
        assert_eq!(
            r.tree().unwrap().selected.as_deref(),
            Some(std::path::Path::new("a.rs")),
            "and the replacement is the selection — decision 54"
        );
        r.settings.panels.as_mut().unwrap().left.open = false;
        r.settle_speed_search();
        assert!(
            r.tree().unwrap().speed.is_none(),
            "disposing a panel must discard its transient query"
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn ordinary_explorer_click_keeps_typing_in_search(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.txt", "spec.txt"] {
        std::fs::write(dir.path().join(name), "unchanged\n").unwrap();
    }
    for index in 0..65 {
        std::fs::write(
            dir.path().join(format!("file-{index:02}.txt")),
            "unchanged\n",
        )
        .unwrap();
    }
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.reveal_tool(guild_session::Tool::Explorer, cx);
            root.open_file_from(
                crate::Origin::Project,
                std::path::Path::new("file-53.txt"),
                window,
                cx,
            );
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.simulate_resize(gpui::size(px(1200.), px(800.)));
    cx.run_until_parked();
    let root = root.unwrap();
    for opening in ["new", "already open"] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.tree()
                    .unwrap()
                    .scroll
                    .scroll_to_item(0, gpui::ScrollStrategy::Top);
                cx.notify();
            })
        });
        cx.run_until_parked();
        let row = cx.debug_bounds("explorer-row-a.txt").expect("a.txt row");
        cx.simulate_click(row.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|_, cx| {
            let r = root.read(cx);
            assert!(matches!(r.active_body(), Some(crate::Body::File { path, origin: crate::Origin::Project, .. }) if path == std::path::Path::new("a.txt")));
            assert_eq!(r.tabs.len(), 2, "click opens once, then reuses the same file");
        });
        cx.update(|window, cx| {
            let result = window.dispatch_event(
                gpui::PlatformInput::KeyDown(gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke {
                        key_char: Some("s".into()),
                        ..gpui::Keystroke::parse("s").unwrap()
                    },
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
            assert!(
                result.propagate,
                "the first key must reach native inputContext"
            );
            assert!(
                root.read(cx).tree().unwrap().speed.is_none(),
                "raw key_char is not committed or composed text"
            );
        });
        cx.simulate_keystrokes("s");
        // InputState emits Blur on app deactivation even though the same
        // input remains the window's focused handle. Returning must not
        // silently route the rest of the query into the opened editor.
        cx.deactivate_window();
        cx.run_until_parked();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.simulate_keystrokes("p");
        cx.run_until_parked();
        cx.update(|_, cx| {
            let r = root.read(cx);
            assert_eq!(
                r.tree().unwrap().speed.as_ref().map(|s| s.query.as_str()),
                Some("sp"),
                "ordinary click on {opening} file must retain list input"
            );
            let Some(crate::Body::File { tab, .. }) = r.active_body() else {
                panic!("click must open the file");
            };
            assert_eq!(
                tab.read(cx).editor().unwrap().read(cx).text().to_string(),
                "unchanged\n"
            );
        });
        cx.simulate_keystrokes("escape");
    }
    cx.simulate_keystrokes("s");
    cx.update(|window, cx| {
        root.read(cx)
            .active_body()
            .unwrap()
            .focus_handle(cx)
            .unwrap()
            .focus(window, cx);
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("q");
    cx.update(|_, cx| {
        assert!(
            root.read(cx).tree().unwrap().speed.is_none(),
            "real focus transfer must still dismiss search"
        );
        let Some(crate::Body::File { tab, .. }) = root.read(cx).active_body() else {
            panic!("file stays open")
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().to_string(),
            "qunchanged\n"
        );
    });
}

fn explorer_fixture<'a>(
    cx: &'a mut TestAppContext,
    path: &std::path::Path,
) -> (Entity<Root>, &'a mut gpui::VisualTestContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let mut root = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), path);
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            root.active_project = Some(project);
            root.reveal_tool(guild_session::Tool::Explorer, cx);
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    visual.update(|window, _| window.activate_window());
    visual.simulate_resize(gpui::size(px(1200.), px(800.)));
    visual.run_until_parked();
    (root.unwrap(), visual)
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_mixed_selection_survives_fold_and_refresh(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for folder in ["alpha", "beta"] {
        std::fs::create_dir(dir.path().join(folder)).unwrap();
        std::fs::write(dir.path().join(folder).join("child.txt"), "child").unwrap();
    }
    std::fs::write(dir.path().join("file.txt"), "file").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let click = |cx: &mut gpui::VisualTestContext, name: &'static str, platform, shift| {
        let bounds = cx.debug_bounds(name).unwrap();
        cx.simulate_click(
            bounds.center(),
            gpui::Modifiers {
                platform,
                shift,
                ..Default::default()
            },
        );
    };
    click(cx, "explorer-row-file.txt", false, false);
    click(cx, "explorer-row-alpha", true, false);
    click(cx, "explorer-row-beta", true, false);
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        for name in ["file.txt", "alpha", "beta"] {
            assert!(
                state.files.contains(&(crate::Origin::Project, name.into())),
                "{name}"
            );
        }
        assert!(
            !state.tree.is_expanded(std::path::Path::new("alpha")),
            "modifier click must not fold"
        );
    });
    cx.simulate_keystrokes("shift-end");
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert!(
            state
                .files
                .contains(&(crate::Origin::Project, "beta".into()))
        );
        assert!(
            state
                .files
                .contains(&(crate::Origin::Project, "file.txt".into()))
        );
    });
    click(cx, "explorer-row-alpha", false, false);
    click(cx, "explorer-row-alpha/child.txt", true, false);
    click(cx, "explorer-row-beta", true, false);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let state = r.tree_mut().unwrap();
            state.tree.collapse(std::path::Path::new("alpha"));
            state.tree.refresh(std::path::Path::new(""));
            state.settle_selection();
            cx.notify();
        })
    });
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert!(
            state
                .files
                .contains(&(crate::Origin::Project, "alpha/child.txt".into())),
            "folding must retain existing selected descendants"
        );
        assert!(
            state
                .files
                .contains(&(crate::Origin::Project, "beta".into()))
        );
    });
    cx.simulate_keystrokes("cmd-a");
    cx.update(|_, cx| {
        let state = root.read(cx).tree().unwrap();
        assert_eq!(
            state.files.selected,
            [".guild", "alpha", "beta", "file.txt"].map(|p| (crate::Origin::Project, p.into()))
        );
        assert!(
            !state
                .files
                .contains(&(crate::Origin::Project, std::path::PathBuf::new()))
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_file_clipboard_and_keyboard_move(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["source", "dest", "other"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    std::fs::write(dir.path().join("source/child.txt"), "child").unwrap();
    std::fs::write(dir.path().join("file.txt"), "file").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let click = |cx: &mut gpui::VisualTestContext, selector: &'static str, command| {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(
            bounds.center(),
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    };
    click(cx, "explorer-row-file.txt", false);
    cx.simulate_keystrokes("shift-f6");
    cx.update(|window, cx| assert!(crate::text_field::focused(window, cx)));
    cx.simulate_keystrokes("escape");
    click(cx, "explorer-row-source", true);
    cx.simulate_keystrokes("cmd-c");
    click(cx, "explorer-row-dest", false);
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/file.txt")).unwrap(),
        "file"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/source/child.txt")).unwrap(),
        "child"
    );
    assert!(dir.path().join("file.txt").exists());
    // Ordinary text takes ownership, even when it contains the same visible paths.
    cx.update(|_, cx| {
        let text = cx.read_from_clipboard().unwrap().text().unwrap();
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
    });
    click(cx, "explorer-row-other", false);
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    assert!(!dir.path().join("other/file.txt").exists());
    click(cx, "explorer-row-file.txt", false);
    cx.simulate_keystrokes("cmd-x");
    click(cx, "explorer-row-other", false);
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    assert!(dir.path().join("other/file.txt").exists());
    assert!(!dir.path().join("file.txt").exists());
    click(cx, "explorer-row-source", false);
    cx.simulate_keystrokes("f6 cmd-a o t h e r enter");
    cx.run_until_parked();
    assert!(dir.path().join("other/source/child.txt").exists());
    assert!(!dir.path().join("source").exists());
    cx.update(|_, cx| assert!(root.read(cx).active_project.is_some()));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn moving_and_renaming_keep_dirty_editors_and_panes(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    let library = tempfile::tempdir().unwrap();
    for base in [dir.path(), library.path()] {
        std::fs::create_dir_all(base.join("source")).unwrap();
        std::fs::write(base.join("source/file.rs"), "on disk").unwrap();
    }
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_file(Path::new("source/file.rs"), window, cx)
        })
    });
    cx.run_until_parked();
    let (view, editor, tab_id, panes, project) = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.settings.autosave = false;
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            let view = tab.clone();
            let editor = view.read(cx).editor().unwrap().clone();
            editor.update(cx, |editor, cx| {
                editor.replace_all("unsaved content", window, cx);
                editor.set_selected_range(2..7, cx);
            });
            let tab_id = r.active_tab.unwrap();
            let tab = r.tabs.iter_mut().find(|t| t.id == tab_id).unwrap();
            let pane = tab
                .panes
                .split(tab.panes.focused(), guild_session::layout::Axis::Horizontal)
                .unwrap();
            assert!(
                tab.panes
                    .fill(
                        pane,
                        crate::Body::File {
                            path: "source/file.rs".into(),
                            tab: view.clone(),
                            presentation: None,
                            origin: crate::Origin::Project
                        }
                    )
                    .is_ok()
            );
            let panes = tab.panes.ids();
            let project = r.active_project.unwrap();
            r.open_diff(
                Some("source/file.rs".into()),
                guild_session::Base::Head,
                window,
                cx,
            );
            // Historical persisted Annotate bodies remain restorable and movable;
            // the new gesture decorates FileTab instead of constructing this body.
            r.restore_annotate_tab(project, "source/file.rs".into(), cx);
            r.note_library(project, library.path());
            r.open_file_from(
                crate::Origin::External(0),
                Path::new("source/file.rs"),
                window,
                cx,
            );
            (view, editor, tab_id, panes, project)
        })
    });
    // Let opening the library finish its deferred reveal before selecting the source.
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.select_operation_target(Path::new("source"));
            r.tree_mut().unwrap().tree.expand(Path::new("source"));
            r.start_transfer(
                project,
                vec!["source".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    cx.update(|window,cx|root.update(cx,|r,cx| {
        let tab=r.tabs.iter().find(|t|t.id==tab_id).expect("original tab survives");
        assert_eq!(tab.panes.ids(),panes);
        for (_,body) in tab.panes.bodies() {
            let crate::Body::File{path,tab,origin:crate::Origin::Project,..}=body else{panic!()};
            assert_eq!(path,Path::new("dest/source/file.rs"));assert_eq!(tab.entity_id(),view.entity_id());
        }
        assert!(r.tree().unwrap().tree.expanded().any(|p|p==Path::new("dest/source")));
        assert!(r.tree().unwrap().files.contains(&(crate::Origin::Project,Path::new("dest/source").into())));
        assert!(r.tabs.iter().filter(|t|t.project==project).flat_map(|t|t.bodies()).any(|b|matches!(b,crate::Body::Diff{path:Some(p),tab,..} if p==Path::new("dest/source/file.rs") && tab.read(cx).follows(p))));
        assert!(r.tabs.iter().filter(|t|t.project==project).flat_map(|t|t.bodies()).any(|b|matches!(b,crate::Body::Annotate{path,tab} if path==Path::new("dest/source/file.rs") && tab.read(cx).path()==path)));
        assert_eq!(view.read(cx).relative(),Path::new("dest/source/file.rs"));
        assert_eq!(view.read(cx).editor().unwrap().entity_id(),editor.entity_id());
        assert_eq!(editor.read(cx).text().to_string(),"unsaved content");
        assert_eq!(editor.read(cx).selected_range(),2..7);
        assert!(editor.read(cx).lsp().definition_provider.is_some(), "shared panes must retain the relocated LSP provider");
        assert!(r.tabs.iter().flat_map(|t|t.bodies()).any(|b|matches!(b,crate::Body::File{origin:crate::Origin::External(0),path,..} if path==Path::new("source/file.rs"))));
        r.run_deed(crate::view::menu::Deed::Tree(crate::view::menu::TreeDeed::Rename),&crate::view::menu::Target::TreeRow{path:"dest/source/file.rs".into(),is_dir:false,expanded:false},window,cx);
    }));
    cx.simulate_keystrokes("n e w enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(view.read(cx).relative(), Path::new("dest/source/new.rs"));
        assert_eq!(
            view.read(cx).editor().unwrap().entity_id(),
            editor.entity_id()
        );
        root.update(cx, |r, cx| {
            r.open_file(Path::new("dest/source/new.rs"), window, cx)
        });
        editor.read(cx).focus_handle(cx).focus(window, cx);
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| assert_eq!(editor.read(cx).text().to_string(), "on disk"));
    cx.simulate_keystrokes("cmd-shift-z");
    cx.update(|_, cx| view.update(cx, |file, cx| file.save(cx).unwrap()));
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/source/new.rs")).unwrap(),
        "unsaved content"
    );
    assert!(!dir.path().join("source").exists());
    assert!(!dir.path().join("dest/source/file.rs").exists());
    assert_eq!(
        std::fs::read_to_string(library.path().join("source/file.rs")).unwrap(),
        "on disk"
    );
    cx.update(|_, cx| assert_eq!(root.read(cx).active_project, Some(project)));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn transfer_completion_keeps_the_origin_project_after_switch(cx: &mut TestAppContext) {
    use std::path::Path;
    let first = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    for root in [first.path(), other.path()] {
        std::fs::create_dir(root.join("dest")).unwrap();
        std::fs::write(root.join("same.txt"), "disk").unwrap();
    }
    let (root, cx) = explorer_fixture(cx, first.path());
    let (first_view, other_view, second) = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.settings.autosave = false;
            r.open_file(Path::new("same.txt"), window, cx);
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            let first_view = tab.clone();
            let project = r.active_project.unwrap();
            r.start_transfer(
                project,
                vec!["same.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            );
            let second = r.desk.open_in(r.active_workspace, other.path());
            r.adopt_project(second);
            r.select_project(second, cx);
            r.open_file(Path::new("same.txt"), window, cx);
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            (first_view, tab.clone(), second)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(root.read(cx).active_project, Some(second));
        assert_eq!(first_view.read(cx).relative(), Path::new("dest/same.txt"));
        assert_eq!(other_view.read(cx).relative(), Path::new("same.txt"));
    });
    assert!(first.path().join("dest/same.txt").exists());
    assert!(!first.path().join("same.txt").exists());
    assert!(other.path().join("same.txt").exists());
    assert!(!other.path().join("dest/same.txt").exists());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn trash_confirmation_names_the_normalized_selection_and_cancel_keeps_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), "a").unwrap();
    std::fs::create_dir(dir.path().join("beta")).unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    for (selector, command) in [
        ("explorer-row-alpha.txt", false),
        ("explorer-row-beta", true),
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        cx.simulate_click(
            bounds.center(),
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    }
    cx.simulate_keystrokes("cmd-backspace");
    cx.run_until_parked();
    let (title, detail) = cx.pending_prompt().expect("Trash must ask before writing");
    assert!(format!("{title} {detail}").contains("alpha.txt"));
    assert!(format!("{title} {detail}").contains("beta"));
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert!(dir.path().join("alpha.txt").exists());
    assert!(dir.path().join("beta").is_dir());
    cx.update(|_, cx| assert_eq!(root.read(cx).operation_sources().len(), 2));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_native_drag_moves_selected_group_and_escape_cancels(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    for name in ["a.txt", "b.txt", "cancel.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    for (selector, command) in [("explorer-row-a.txt", false), ("explorer-row-b.txt", true)] {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(
            at,
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    }
    let start = cx.debug_bounds("explorer-row-a.txt").unwrap().center();
    let end = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(12.), gpui::px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(dir.path().join("dest/a.txt").exists());
    assert!(dir.path().join("dest/b.txt").exists());
    assert!(!dir.path().join("a.txt").exists());
    assert!(!dir.path().join("b.txt").exists());
    let start = cx.debug_bounds("explorer-row-cancel.txt").unwrap().center();
    let end = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(12.), gpui::px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(dir.path().join("cancel.txt").exists());
    assert!(!dir.path().join("dest/cancel.txt").exists());
    cx.update(|_, cx| assert!(root.read(cx).file_operations.busy.is_none()));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_drag_hover_unfolds_and_outside_release_cleans_up(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("dest/child.txt"), "child").unwrap();
    std::fs::write(dir.path().join("source.txt"), "source").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let start = cx.debug_bounds("explorer-row-source.txt").unwrap().center();
    let end = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(gpui::px(12.), gpui::px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .tree()
                .unwrap()
                .tree
                .expanded()
                .any(|p| p == std::path::Path::new("dest")),
            "hover expands a closed folder"
        )
    });
    let outside = gpui::point(gpui::px(1100.), gpui::px(700.));
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(root.read(cx).file_operations.drag.is_none()));
    assert!(dir.path().join("source.txt").exists());
    assert!(!dir.path().join("dest/source.txt").exists());
}

/// Once an Explorer drag has left the window it belongs to the platform
/// (a copy in another application) and never drops inside the project,
/// even when it comes back over a folder: GPUI restores the same drag value
/// in the source window. The test platform refuses the native session, so
/// this is the refused path too: the in-window drag survives but is inert.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_drag_out_of_the_window_never_drops_inside(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::create_dir(dir.path().join("folder")).unwrap();
    std::fs::write(dir.path().join("folder/child.txt"), "child").unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    for (selector, command) in [("explorer-row-a.txt", false), ("explorer-row-folder", true)] {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(
            at,
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    }
    let start = cx.debug_bounds("explorer-row-a.txt").unwrap().center();
    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(px(12.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    let outside = gpui::point(px(-20.), start.y);
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_mouse_move(dest, gpui::MouseButton::Left, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            !root
                .read(cx)
                .tree()
                .unwrap()
                .tree
                .expanded()
                .any(|p| p == std::path::Path::new("dest")),
            "a drag handed out does not hover-expand folders on its way back"
        )
    });
    cx.simulate_mouse_up(dest, gpui::MouseButton::Left, Default::default());
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("folder/child.txt").exists());
    assert!(!dir.path().join("dest/a.txt").exists());
    assert!(!dir.path().join("dest/folder").exists());
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(root.file_operations.drag.is_none());
        assert!(root.file_operations.busy.is_none());
        assert!(!cx.has_active_drag());
    });
}

/// A press on a row outside the selection drags that row alone: no earlier
/// selection rides along with it, out of the window or into a folder.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_drag_from_outside_the_selection_carries_only_the_pressed_row(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    for name in ["a.txt", "b.txt", "c.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (_root, cx) = explorer_fixture(cx, dir.path());
    for (selector, command) in [("explorer-row-a.txt", false), ("explorer-row-b.txt", true)] {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(
            at,
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    }
    let start = cx.debug_bounds("explorer-row-c.txt").unwrap().center();
    let end = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(px(12.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(dir.path().join("dest/c.txt").exists());
    assert!(dir.path().join("a.txt").exists(), "not part of the gesture");
    assert!(dir.path().join("b.txt").exists(), "not part of the gesture");
}

fn drop_from_outside(
    cx: &mut gpui::VisualTestContext,
    paths: &[std::path::PathBuf],
    at: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: at,
        paths: gpui::ExternalPaths(paths.iter().cloned().collect()),
    });
    cx.simulate_event(gpui::FileDropEvent::Pending { position: at });
    cx.simulate_event(gpui::FileDropEvent::Submit { position: at });
    cx.simulate_event(gpui::FileDropEvent::Exited);
    cx.run_until_parked();
}

/// Files another application drops on the Explorer are copied: into a
/// folder row, a file row's folder, or the project root from empty space.
/// The originals stay; a collision asks; what is already there is refused.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_copies_what_other_applications_drop(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("dest/keep.txt"), "keep").unwrap();
    std::fs::write(dir.path().join("note.txt"), "note").unwrap();
    std::fs::create_dir(outside.path().join("folder")).unwrap();
    std::fs::write(outside.path().join("folder/child.txt"), "child").unwrap();
    for name in ["file.txt", "second.txt", "third.txt", "keep.txt"] {
        std::fs::write(outside.path().join(name), name).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    let o = |name: &str| outside.path().join(name);

    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    drop_from_outside(cx, &[o("file.txt"), o("folder")], dest);
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/file.txt")).unwrap(),
        "file.txt"
    );
    assert!(dir.path().join("dest/folder/child.txt").is_file());

    let note = cx.debug_bounds("explorer-row-note.txt").unwrap().center();
    drop_from_outside(cx, &[o("second.txt")], note);
    assert!(
        dir.path().join("second.txt").is_file(),
        "a file row means its folder"
    );

    let empty = note + gpui::point(px(0.), px(300.));
    drop_from_outside(cx, &[o("third.txt")], empty);
    assert!(
        dir.path().join("third.txt").is_file(),
        "empty space means the root"
    );

    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    drop_from_outside(cx, &[o("keep.txt")], dest);
    assert!(cx.has_pending_prompt(), "a collision asks");
    cx.simulate_prompt_answer("Keep Both");
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/keep.txt")).unwrap(),
        "keep"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/keep (2).txt")).unwrap(),
        "keep.txt"
    );

    let project_row = cx.debug_bounds("explorer-row-").unwrap().center();
    drop_from_outside(cx, &[dir.path().join("note.txt")], project_row);
    assert!(
        !dir.path().join("note (2).txt").exists(),
        "a file already in the folder is refused"
    );
    drop_from_outside(cx, &[dir.path().join("dest")], dest);
    assert!(!dir.path().join("dest/dest").exists(), "nor into itself");

    for name in [
        "file.txt",
        "folder/child.txt",
        "second.txt",
        "third.txt",
        "keep.txt",
    ] {
        assert!(o(name).is_file(), "{name} stays where it was");
    }
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(root.file_operations.busy.is_none());
        assert!(root.file_operations.drag.is_none());
        assert!(!cx.has_active_drag());
    });
}

/// The Explorer's visible rows, by path, in order.
fn explorer_rows(root: &Entity<Root>, cx: &mut gpui::VisualTestContext) -> Vec<String> {
    cx.update(|_, cx| {
        root.read(cx)
            .tree()
            .unwrap()
            .tree
            .rows()
            .iter()
            .map(|row| row.path().display().to_string())
            .collect()
    })
}

/// Draw a frame now, without letting a worker run first.
fn draw_now(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

/// A transfer's notice must not move the rows. It sat above them, so every
/// drop pushed the list down for as long as the copy ran and back up when it
/// finished: the Explorer twitched on release. The frame is drawn after the
/// transfer starts and before its worker runs, which for a copy of a few
/// files is the one frame a person sees. A running copy's staging folder
/// cannot be drawn here — the worker runs on this thread, all at once — and
/// its hiding is `tree::tests::a_running_copys_staging_folder_is_hidden_while_it_runs`.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_rows_stay_put_while_a_transfer_runs(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "a").unwrap();
    std::fs::write(dir.path().join("note.txt"), "note").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let before = cx.debug_bounds("explorer-row-note.txt").unwrap();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            root.start_transfer(
                project,
                vec!["a.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Copy,
                window,
                cx,
            )
        })
    });
    cx.update(|_, cx| assert!(root.read(cx).file_operations.busy.is_some()));
    draw_now(cx);
    assert_eq!(
        cx.debug_bounds("explorer-row-note.txt").unwrap(),
        before,
        "the rows stay where they were while the transfer runs"
    );
    cx.run_until_parked();
    assert!(dir.path().join("dest/a.txt").is_file());
    assert_eq!(
        explorer_rows(&root, cx),
        ["", "dest", "dest/a.txt", "a.txt", "note.txt"],
        "after it, the copy is the one new row, revealed in its folder"
    );
}

/// The notice must not take room from the list either: it shrank the
/// viewport, so a list scrolled to its end while a transfer ran dropped by
/// the notice's height when the transfer finished.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_rows_stay_put_at_the_end_of_a_long_list(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..80 {
        std::fs::write(dir.path().join(format!("f{i:02}.txt")), "x").unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    let to_the_end = |root: &Entity<Root>, cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let scroll = root
                .read(cx)
                .tree()
                .unwrap()
                .scroll
                .0
                .borrow()
                .base_handle
                .clone();
            scroll.set_offset(gpui::point(px(0.), px(-100_000.)));
        });
        draw_now(cx);
    };
    to_the_end(&root, cx);
    let before = cx.debug_bounds("explorer-row-f79.txt").unwrap();
    // A move onto its own folder: the transfer runs and changes no row.
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            root.start_transfer(
                project,
                vec!["f79.txt".into()],
                "".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            )
        })
    });
    cx.update(|_, cx| assert!(root.read(cx).file_operations.busy.is_some()));
    to_the_end(&root, cx);
    assert_eq!(
        cx.debug_bounds("explorer-row-f79.txt").unwrap(),
        before,
        "the end of the list is where it was while the notice shows"
    );
    cx.run_until_parked();
    cx.update(|_, cx| assert!(root.read(cx).file_operations.busy.is_none()));
    draw_now(cx);
    assert_eq!(
        cx.debug_bounds("explorer-row-f79.txt").unwrap(),
        before,
        "and when it goes"
    );
}

/// The dropped paths are read once per gesture while it hovers, however many
/// moves and frames it makes, and again only by workers at the drop: they
/// can be thousands, or on a volume that does not answer, and every move and
/// frame used to read them all on the window's thread. The counter is
/// global, so this runs alone, as every test here does.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_reads_what_is_dropped_once_per_gesture(cx: &mut TestAppContext) {
    use std::sync::atomic::Ordering;
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    for name in ["dest", "other", "third"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    let sources: Vec<_> = (0..50)
        .map(|i| {
            let path = outside.path().join(format!("f{i}.txt"));
            std::fs::write(&path, "x").unwrap();
            path
        })
        .collect();
    let (_root, cx) = explorer_fixture(cx, dir.path());
    let rows: Vec<_> = [
        "explorer-row-dest",
        "explorer-row-other",
        "explorer-row-third",
    ]
    .into_iter()
    .map(|row| cx.debug_bounds(row).unwrap().center())
    .collect();
    crate::file_operations::EXAMINED.store(0, Ordering::Relaxed);
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: rows[0],
        paths: gpui::ExternalPaths(sources.iter().cloned().collect()),
    });
    for round in 0..10 {
        for at in &rows {
            let at = *at + gpui::point(px(round as f32), px(0.));
            cx.simulate_event(gpui::FileDropEvent::Pending { position: at });
        }
    }
    cx.run_until_parked();
    assert_eq!(
        crate::file_operations::EXAMINED.load(Ordering::Relaxed),
        1,
        "thirty moves over three folders read the fifty paths once"
    );
    cx.simulate_event(gpui::FileDropEvent::Submit { position: rows[0] });
    cx.simulate_event(gpui::FileDropEvent::Exited);
    cx.run_until_parked();
    assert_eq!(
        crate::file_operations::EXAMINED.load(Ordering::Relaxed),
        3,
        "then once by the import's check and once by the copier's own, both on workers"
    );
    assert!(dir.path().join("dest/f49.txt").is_file());
}

/// The notice lies over the last rows, so it must take the mouse itself: a
/// click on it, Cancel included, reached the row hidden under it — selected
/// or opened it, folded or unfolded a folder, moving the rows it keeps still.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_notice_keeps_clicks_from_the_rows_under_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for i in 0..80 {
        std::fs::write(dir.path().join(format!("f{i:02}.txt")), "x").unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            root.start_transfer(
                project,
                vec!["f00.txt".into()],
                "".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            )
        })
    });
    draw_now(cx);
    let notice = cx.debug_bounds("explorer-transfer-notice").unwrap();
    let cancel = cx.debug_bounds("explorer-transfer-cancel").unwrap();
    let nothing_selected = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            let root = root.read(cx);
            let tree = root.tree().unwrap();
            tree.files.selected.is_empty()
                && tree.selected.is_none()
                && root.active_body().is_none()
        })
    };
    assert!(nothing_selected(cx));
    // Clicks dispatched without letting the transfer's worker run: the
    // notice must still be there for the second one.
    let click = |cx: &mut gpui::VisualTestContext, at: gpui::Point<gpui::Pixels>| {
        use gpui::InputEvent as _;
        cx.update(|window, cx| {
            for event in [
                gpui::MouseDownEvent {
                    position: at,
                    button: gpui::MouseButton::Left,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                gpui::MouseUpEvent {
                    position: at,
                    button: gpui::MouseButton::Left,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
            ] {
                window.dispatch_event(event, cx);
            }
        });
    };
    click(cx, notice.origin + gpui::point(px(4.), px(4.)));
    assert!(
        nothing_selected(cx),
        "a click on the notice is the notice's"
    );
    click(cx, cancel.center());
    assert!(nothing_selected(cx), "and so is Cancel's");
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .file_operations
                .busy
                .as_ref()
                .is_some_and(|busy| busy.cancelled()),
            "Cancel was heard"
        )
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(root.read(cx).file_operations.busy.is_none()));
}

/// Drop from outside and stop before any worker runs.
fn drop_from_outside_unparked(
    cx: &mut gpui::VisualTestContext,
    paths: &[std::path::PathBuf],
    at: gpui::Point<gpui::Pixels>,
) {
    use gpui::InputEvent as _;
    let paths = gpui::ExternalPaths(paths.iter().cloned().collect());
    cx.update(|window, cx| {
        for event in [
            gpui::FileDropEvent::Entered {
                position: at,
                paths: paths.clone(),
            },
            gpui::FileDropEvent::Submit { position: at },
            gpui::FileDropEvent::Exited,
        ] {
            window.dispatch_event(event.to_platform_input(), cx);
        }
    });
}

/// A drop is a transfer from the moment it is accepted, while its paths are
/// still being checked on a worker: Cancel reaches it, nothing else starts
/// meanwhile, the accepted drop is not lost to whatever tried, and a check
/// that fails says so.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_import_is_a_transfer_while_it_is_checked(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("note.txt"), "note").unwrap();
    for name in ["one.txt", "two.txt"] {
        std::fs::write(outside.path().join(name), name).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    let busy = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| root.read(cx).file_operations.busy.is_some())
    };

    drop_from_outside_unparked(cx, &[outside.path().join("one.txt")], dest);
    assert!(busy(cx), "the drop is a transfer at once");
    cx.update(|_, cx| root.update(cx, |root, cx| root.cancel_file_operation(cx)));
    cx.run_until_parked();
    assert!(
        !dir.path().join("dest/one.txt").exists(),
        "Cancel reached it"
    );
    assert!(!busy(cx));

    drop_from_outside_unparked(cx, &[outside.path().join("two.txt")], dest);
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            root.start_transfer(
                project,
                vec!["note.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(
        dir.path().join("dest/two.txt").is_file(),
        "the drop was not lost"
    );
    assert!(
        dir.path().join("note.txt").is_file(),
        "nothing else started meanwhile"
    );

    let errors = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| root.read(cx).notices.visible().count())
    };
    let before = errors(cx);
    let empty = dest + gpui::point(px(0.), px(300.));
    drop_from_outside_unparked(cx, &[dir.path().join("note.txt")], empty);
    cx.run_until_parked();
    assert_eq!(errors(cx), before + 1, "a refused check says why");
    assert!(!busy(cx));
}

/// A move with no button held ends an Explorer drag whose release went
/// missing; it must not end a drag from another application, which is
/// AppKit's until it drops — stopping GPUI's drag there loses the drop.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_keeps_a_drag_from_outside_through_a_move_without_a_button(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(outside.path().join("file.txt"), "file").unwrap();
    let (_root, cx) = explorer_fixture(cx, dir.path());
    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: dest,
        paths: gpui::ExternalPaths([outside.path().join("file.txt")].into_iter().collect()),
    });
    cx.simulate_event(gpui::FileDropEvent::Pending { position: dest });
    cx.simulate_mouse_move(dest, None, Default::default());
    cx.simulate_event(gpui::FileDropEvent::Submit { position: dest });
    cx.simulate_event(gpui::FileDropEvent::Exited);
    cx.run_until_parked();
    assert!(dir.path().join("dest/file.txt").is_file());
}

/// Hovering a closed folder with files from another application opens it,
/// as an Explorer drag does, and leaving the window ends the hover.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_hover_from_outside_unfolds_and_exit_cleans_up(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("dest/child.txt"), "child").unwrap();
    std::fs::write(outside.path().join("file.txt"), "file").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let dest = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_event(gpui::FileDropEvent::Entered {
        position: dest,
        paths: gpui::ExternalPaths([outside.path().join("file.txt")].into_iter().collect()),
    });
    cx.simulate_event(gpui::FileDropEvent::Pending { position: dest });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(800));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            root.read(cx)
                .tree()
                .unwrap()
                .tree
                .expanded()
                .any(|p| p == std::path::Path::new("dest")),
            "hover expands a closed folder"
        )
    });
    cx.simulate_event(gpui::FileDropEvent::Exited);
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(root.read(cx).file_operations.drag.is_none());
        assert!(!cx.has_active_drag());
    });
    assert!(!dir.path().join("dest/file.txt").exists());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_drag_targets_and_project_switch_cleanup(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("folder/sub")).unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("folder/item.txt"), "item").unwrap();
    // Keep this directory uncompressed after item.txt moves out.
    std::fs::write(dir.path().join("folder/stay.txt"), "stay").unwrap();
    std::fs::write(dir.path().join("dest/anchor.txt"), "anchor").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.tree_mut().unwrap().tree.expand(Path::new("folder"));
            r.tree_mut().unwrap().tree.expand(Path::new("dest"));
            cx.notify();
        })
    });
    cx.run_until_parked();
    let gesture = |cx: &mut gpui::VisualTestContext, from: &'static str, to: &'static str| {
        let start = cx.debug_bounds(from).unwrap().center();
        let end = cx.debug_bounds(to).unwrap().center();
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_move(
            start + gpui::point(px(12.), px(0.)),
            gpui::MouseButton::Left,
            Default::default(),
        );
        cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
        cx.run_until_parked();
    };
    gesture(cx, "explorer-row-folder", "explorer-row-folder/sub");
    assert!(dir.path().join("folder/item.txt").exists());
    assert!(!dir.path().join("folder/sub/folder").exists());
    gesture(
        cx,
        "explorer-row-folder/item.txt",
        "explorer-row-dest/anchor.txt",
    );
    assert!(
        dir.path().join("dest/item.txt").exists(),
        "a file row targets its parent directory"
    );
    gesture(cx, "explorer-row-dest/item.txt", "explorer-row-");
    assert!(
        dir.path().join("item.txt").exists(),
        "the root accepts a move back"
    );
    std::fs::create_dir(dir.path().join("sink")).unwrap();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.tree_mut().unwrap().tree.refresh(std::path::Path::new(""));
            cx.notify();
        })
    });
    cx.run_until_parked();
    for (selector, command) in [("explorer-row-folder", false), ("explorer-row-dest", true)] {
        let at = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(
            at,
            gpui::Modifiers {
                platform: command,
                ..Default::default()
            },
        );
    }
    gesture(cx, "explorer-row-folder", "explorer-row-sink");
    assert!(dir.path().join("sink/folder/sub").is_dir());
    assert!(dir.path().join("sink/dest/anchor.txt").exists());
    let start = cx.debug_bounds("explorer-row-item.txt").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(px(12.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        root.update(cx, |r, _| {
            r.forget_file_operations(r.active_project.unwrap());
            r.active_project = None;
        })
    });
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(160));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(root.read(cx).file_operations.drag.is_none());
        assert!(
            !cx.has_active_drag(),
            "project changes must clear the native drag preview too"
        );
    });
    assert!(dir.path().join("item.txt").exists());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn transfer_skip_reports_completed_and_remaining_paths(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    for (name, text) in [("a.txt", "a"), ("b.txt", "b"), ("dest/a.txt", "existing")] {
        std::fs::write(dir.path().join(name), text).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.start_transfer(
                r.active_project.unwrap(),
                vec!["a.txt".into(), "b.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Skip");
    cx.run_until_parked();
    assert!(dir.path().join("a.txt").exists());
    assert!(dir.path().join("dest/b.txt").exists());
    cx.update(|_, cx| {
        let notices = root
            .read(cx)
            .notices
            .visible()
            .map(|(_, n)| n.text.clone())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            notices.contains("dest/b.txt") && notices.contains("a.txt"),
            "partial completion must name both completed and retained items: {notices}"
        );
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn cancelling_cut_collision_does_not_replay_the_file_payload(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    for name in ["a.txt", "dest/a.txt"] {
        std::fs::write(dir.path().join(name), name).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    let at = cx.debug_bounds("explorer-row-a.txt").unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.simulate_keystrokes("cmd-x");
    let at = cx.debug_bounds("explorer-row-dest").unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            assert!(
                r.cut_sources(cx).is_empty(),
                "Cancel disarms the cut payload"
            )
        })
    });
    cx.simulate_keystrokes("cmd-v");
    cx.run_until_parked();
    assert!(cx.pending_prompt().is_none());
    assert!(dir.path().join("a.txt").exists());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn replacing_a_clipboard_source_cannot_move_the_new_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "original").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let at = cx.debug_bounds("explorer-row-a.txt").unwrap().center();
    cx.simulate_click(at, Default::default());
    cx.simulate_keystrokes("cmd-x");
    std::fs::rename(dir.path().join("a.txt"), dir.path().join("retired.txt")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "replacement").unwrap();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.select_operation_target(std::path::Path::new("dest"));
            r.paste_files(window, cx);
        })
    });
    cx.run_until_parked();
    assert!(
        !dir.path().join("dest/a.txt").exists(),
        "file clipboard ownership does not authorize a replacement source"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "replacement"
    );
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn moved_paths_reject_stale_agent_proposals_in_the_origin_project(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "disk").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let id = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let project = r.active_project.unwrap();
            let look = r.file_look();
            let ui = r.ui();
            let mode = r.settings.diff_view;
            let diff = cx.new(|cx| {
                crate::diff_tab::DiffTab::open_proposal(
                    dir.path(),
                    "a.txt".into(),
                    Some(b"disk".to_vec()),
                    b"proposed".to_vec(),
                    mode,
                    look,
                    ui,
                    cx,
                )
            });
            let id = r.adopt(
                r.seat_for_project(project),
                project,
                crate::Body::Diff {
                    path: Some("a.txt".into()),
                    base: guild_session::Base::Proposal,
                    tab: diff,
                },
                cx,
            );
            let pane = r.tabs.iter().find(|t| t.id == id).unwrap().panes.focused();
            r.parked.insert(
                id,
                crate::root::ide::Parked {
                    connection: 42,
                    id: serde_json::json!(1),
                    tab_name: "test".into(),
                    proposed: "proposed".into(),
                    pane,
                },
            );
            r.start_transfer(
                project,
                vec!["a.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            );
            id
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            !root.read(cx).is_proposal(id),
            "an agent must never receive acceptance for a stale old path"
        )
    });
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/a.txt")).unwrap(),
        "disk"
    );
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn rename_extension_updates_presentation_without_replacing_editor(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "# heading").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| root.update(cx, |r, cx| r.open_file(Path::new("note.txt"), window, cx)));
    cx.run_until_parked();
    let editor = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            tab.read(cx).editor().unwrap().entity_id()
        })
    });
    cx.run_until_parked();
    for (from, to, markdown) in [("note.txt", "note.md", true), ("note.md", "note.rs", false)] {
        std::fs::rename(dir.path().join(from), dir.path().join(to)).unwrap();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.relocate_project_paths(
                    r.active_project.unwrap(),
                    Path::new(from),
                    Path::new(to),
                    window,
                    cx,
                );
                let crate::Body::File {
                    tab, presentation, ..
                } = r.active_body().unwrap()
                else {
                    panic!()
                };
                assert_eq!(
                    presentation.is_some(),
                    markdown,
                    "presentation follows the new extension"
                );
                assert_eq!(tab.read(cx).editor().unwrap().entity_id(), editor);
            })
        });
    }
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_rename_noop_and_replacement_release_editors(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("folder")).unwrap();
    std::fs::write(dir.path().join("folder/a.txt"), "original").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let file = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.settings.autosave = false;
            r.open_file(Path::new("folder/a.txt"), window, cx);
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            tab.clone()
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |e, cx| e.replace_all("unsaved", window, cx))
    });
    cx.run_until_parked();
    for (path, replace) in [
        ("folder/a.txt", false),
        ("folder", false),
        ("folder/a.txt", true),
    ] {
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.start_naming(
                    crate::root::tree::Naming::rename(Path::new(path)).unwrap(),
                    window,
                    cx,
                );
                cx.notify();
            })
        });
        cx.run_until_parked();
        if replace {
            cx.update(|window, cx| {
                root.update(cx, |r, cx| {
                    r.start_naming(
                        crate::root::tree::Naming::file(Path::new(""), true),
                        window,
                        cx,
                    );
                    cx.notify();
                })
            });
        } else {
            cx.simulate_keystrokes("enter");
        }
        cx.update(|_, cx| {
            file.update(cx, |f, cx| {
                assert!(
                    f.warning_text().is_none(),
                    "unchanged disk must not create a false conflict"
                );
                assert!(
                    f.save(cx).is_ok(),
                    "Rename must release {path}; replacement={replace}"
                );
            })
        });
        cx.simulate_keystrokes("escape");
    }
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_rename_reconciles_external_changes_on_cancel_and_success(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "original").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let file = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.settings.autosave = false;
            r.open_file(Path::new("a.txt"), window, cx);
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            tab.clone()
        })
    });
    for (dirty, rename) in [(false, false), (true, false), (true, true)] {
        cx.update(|window, cx| {
            file.update(cx, |f, cx| {
                f.reload(window, cx);
                if dirty {
                    f.editor()
                        .unwrap()
                        .update(cx, |e, cx| e.replace_all("unsaved", window, cx));
                }
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.start_naming(
                    crate::root::tree::Naming::rename(Path::new("a.txt")).unwrap(),
                    window,
                    cx,
                );
                cx.notify();
            })
        });
        cx.run_until_parked();
        let external = format!("external {dirty} {rename}");
        std::fs::write(dir.path().join("a.txt"), &external).unwrap();
        cx.update(|window, cx| file.update(cx, |f, cx| f.disk_changed(window, cx)));
        cx.simulate_keystrokes(if rename {
            "cmd-a b . t x t enter"
        } else {
            "escape"
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let f = file.read(cx);
            let text = f.editor().unwrap().read(cx).text().to_string();
            if dirty {
                assert_eq!(text, "unsaved");
                assert!(
                    f.warning_text().is_some(),
                    "external write must block autosave after thaw"
                );
            } else {
                assert_eq!(
                    text, external,
                    "clean buffer must catch up after Rename cancellation"
                );
            }
        });
    }
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_clipboard_replacement_during_collision_is_refused(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("dest/a.txt"), "existing").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    for cut in [false, true] {
        std::fs::write(dir.path().join("a.txt"), "original").unwrap();
        cx.update(|window, cx| {
            root.update(cx, |r, cx| {
                r.select_operation_target(std::path::Path::new("a.txt"));
                r.file_clipboard(cut, cx);
                r.select_operation_target(std::path::Path::new("dest"));
                r.paste_files(window, cx);
            })
        });
        cx.run_until_parked();
        assert!(cx.pending_prompt().is_some());
        std::fs::rename(
            dir.path().join("a.txt"),
            dir.path().join(format!("retired-{cut}.txt")),
        )
        .unwrap();
        std::fs::write(dir.path().join("a.txt"), "replacement").unwrap();
        cx.simulate_prompt_answer("Keep Both");
        cx.run_until_parked();
        assert!(
            !dir.path().join("dest/a (2).txt").exists(),
            "collision consent never authorizes a replacement source"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "replacement"
        );
        cx.update(|window, cx| root.update(cx, |r, cx| r.paste_files(window, cx)));
        cx.run_until_parked();
        assert!(
            cx.pending_prompt().is_none(),
            "stale payload must be invalidated"
        );
    }
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_drag_release_without_mouse_up_removes_native_preview(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "original").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let start = cx.debug_bounds("explorer-row-a.txt").unwrap().center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        start + gpui::point(px(12.), px(0.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|window, cx| {
        assert!(cx.has_active_drag());
        window.dispatch_event(
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position: start,
                pressed_button: None,
                modifiers: Default::default(),
            }),
            cx,
        );
        assert!(root.read(cx).file_operations.drag.is_none());
        assert!(
            !cx.has_active_drag(),
            "release cleanup must remove preview before destroying its timer"
        );
    });
    assert!(dir.path().join("a.txt").exists());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_large_renamed_buffer_stays_without_syntax_parser(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("large.txt"), "x\n".repeat(1024 * 1024 + 1)).unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| r.open_file(Path::new("large.txt"), window, cx))
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
                panic!()
            };
            let editor = tab.read(cx).editor().unwrap().clone();
            let plain = editor.read(cx).language_name();
            std::fs::rename(dir.path().join("large.txt"), dir.path().join("large.rs")).unwrap();
            r.relocate_project_paths(
                r.active_project.unwrap(),
                Path::new("large.txt"),
                Path::new("large.rs"),
                window,
                cx,
            );
            assert_eq!(
                editor.read(cx).language_name(),
                plain,
                "rename must honor the same parser size limit as opening"
            );
        })
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn review_explicit_move_uses_fresh_selection_not_an_old_clipboard(cx: &mut TestAppContext) {
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("dest")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "original").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.select_operation_target(Path::new("a.txt"));
            r.file_clipboard(true, cx);
        })
    });
    std::fs::rename(dir.path().join("a.txt"), dir.path().join("retired.txt")).unwrap();
    std::fs::write(dir.path().join("a.txt"), "fresh selection").unwrap();
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.start_transfer(
                r.active_project.unwrap(),
                vec!["a.txt".into()],
                "dest".into(),
                crate::file_operations::Operation::Move,
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("dest/a.txt")).unwrap(),
        "fresh selection"
    );
}

fn restored_hidden_file_after_folder_rename_cancel(
    cx: &mut TestAppContext,
    replacement: Option<&[u8]>,
) {
    use crate::file_tab::{FileState, FileTab};
    use std::path::Path;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("folder")).unwrap();
    let path = dir.path().join("folder/a.txt");
    std::fs::write(&path, "restored snapshot").unwrap();
    std::fs::write(dir.path().join("visible.txt"), "visible").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let hidden = cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.settings.autosave = false;
            r.open_file(Path::new("visible.txt"), window, cx);
            let project = r.active_project.unwrap();
            let file = cx.new(|cx| {
                FileTab::restore(
                    dir.path(),
                    Path::new("folder/a.txt"),
                    r.file_look(),
                    r.ui(),
                    cx,
                )
            });
            r.adopt(
                r.seat_for_project(project),
                project,
                crate::Body::File {
                    path: "folder/a.txt".into(),
                    tab: file.clone(),
                    origin: crate::Origin::Project,
                    presentation: None,
                },
                cx,
            );
            r.start_naming(
                crate::root::tree::Naming::rename(Path::new("folder")).unwrap(),
                window,
                cx,
            );
            cx.notify();
            file
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            hidden.read(cx).editor().is_none(),
            "restored tab must remain unrealised during Rename"
        )
    });
    match replacement {
        Some(bytes) => std::fs::write(&path, bytes).unwrap(),
        None => std::fs::remove_file(&path).unwrap(),
    }
    cx.update(|window, cx| hidden.update(cx, |file, cx| file.disk_changed(window, cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        hidden.update(cx, |file, cx| {
            let editor_after_reload = file.editor().map(|editor| editor.entity_id());
            file.realise(window, cx);
            match replacement {
                Some(b"fresh external text") => {
                    assert_eq!(
                        file.editor().unwrap().read(cx).text().to_string(),
                        "fresh external text",
                        "first presentation must use reloaded disk content"
                    );
                    assert_eq!(
                        file.editor().map(|editor| editor.entity_id()),
                        editor_after_reload,
                        "realise must not replace the editor already installed by reload"
                    );
                    assert_eq!(file.state(), &FileState::Text);
                }
                _ => {
                    assert!(
                        file.editor().is_none(),
                        "realise must not resurrect the old text after a non-text reload"
                    );
                    assert_eq!(
                        file.state(),
                        if replacement.is_some() {
                            &FileState::NotText
                        } else {
                            &FileState::Gone
                        }
                    );
                }
            }
            assert!(!file.is_dirty());
            assert!(file.warning_text().is_none());
        })
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn restored_hidden_file_rename_cancel_keeps_fresh_text(cx: &mut TestAppContext) {
    restored_hidden_file_after_folder_rename_cancel(cx, Some(b"fresh external text"));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn restored_hidden_file_rename_cancel_keeps_missing_state(cx: &mut TestAppContext) {
    restored_hidden_file_after_folder_rename_cancel(cx, None);
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn restored_hidden_file_rename_cancel_keeps_binary_state(cx: &mut TestAppContext) {
    restored_hidden_file_after_folder_rename_cancel(cx, Some(b"binary\0content"));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn sidebar_click_typing_escape_and_enter(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    let key = cx.update(|_, cx| {
        root.read(cx)
            .column_groups_for(root.read(cx).active_workspace)[0]
            .key
            .clone()
    });
    let header = cx
        .debug_bounds(Box::leak(
            format!("project-header-{}", key.display()).into_boxed_str(),
        ))
        .unwrap();
    cx.simulate_click(header.center(), Default::default());
    cx.simulate_keystrokes("t");
    assert!(
        cx.debug_bounds("speed-search").is_some(),
        "a project header click must give sidebar native text input"
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| assert!(root.read(cx).sidebar_focus.is_focused(window)));
    assert!(cx.debug_bounds("speed-search").is_none());
    cx.simulate_keystrokes("t enter");
    assert!(cx.debug_bounds("speed-search").is_none());
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn sidebar_matches_displayed_groups_and_refreshes_without_filtering(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["first", "second", "linked"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, &dir.path().join("first"));
    let (first, second, linked) = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let first = r.active_project.unwrap();
            let second = r
                .desk
                .open_in(r.active_workspace, &dir.path().join("second"));
            let linked = r
                .desk
                .open_in(r.active_workspace, &dir.path().join("linked"));
            let repository = dir.path().join("first/.git");
            r.desk.set_repository(first, Some(repository.clone()));
            r.desk.set_repository(linked, Some(repository.clone()));
            r.desk.rename_repository(&repository, "Привет Alpha");
            r.desk
                .rename_repository(&dir.path().join("second"), "Alpha Two");
            r.desk.toggle_fold(&repository);
            cx.notify();
            (first, second, linked)
        })
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("g h b d t n"); // Russian keyboard-position alternative: привет.
    cx.update(|_, cx| {
        let r = root.read(cx);
        let rows = r.sidebar_speed_rows();
        assert_eq!(rows.len(), 2, "worktree names are not candidates");
        assert_eq!(
            r.sidebar_speed
                .as_ref()
                .unwrap()
                .matched(&rows)
                .unwrap()
                .label,
            "Привет Alpha"
        );
        assert_eq!(r.active_project, Some(first), "typing never activates");
        assert!(
            r.desk.is_folded(&dir.path().join("first/.git")),
            "search does not unfold"
        );
    });
    cx.simulate_keystrokes("cmd-a a l p h a");
    let before = cx.update(|_, cx| {
        root.read(cx)
            .sidebar_speed
            .as_ref()
            .unwrap()
            .current
            .clone()
    });
    cx.simulate_keystrokes("down");
    cx.update(|_, cx| {
        assert_ne!(
            root.read(cx).sidebar_speed.as_ref().unwrap().current,
            before
        )
    });
    cx.simulate_keystrokes("up");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).sidebar_speed.as_ref().unwrap().current,
            before
        )
    });
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.drop_repository(
                r.active_workspace,
                &dir.path().join("second"),
                &[second],
                crate::column::DropOn::Repository {
                    group: None,
                    itself: false,
                },
                Some(first),
                cx,
            );
            r.settle_speed_search();
            assert_eq!(r.sidebar_speed_rows()[0].label, "Alpha Two");
        })
    });
    cx.simulate_keystrokes("cmd-a z z z enter");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).active_project,
            Some(first),
            "no-match Enter does nothing"
        )
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.active_project = Some(linked);
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a l p h a enter");
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).active_project,
            Some(linked),
            "Enter preserves the group's current worktree"
        )
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("t w o escape");
    cx.update(|window, cx| {
        let r = root.read(cx);
        assert_eq!(r.active_project, Some(linked));
        assert!(r.sidebar_focus.is_focused(window));
    });
    cx.simulate_keystrokes("t w o enter");
    cx.update(|_, cx| assert_eq!(root.read(cx).active_project, Some(second)));
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a l p h a");
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.desk
                .rename_repository(&dir.path().join("second"), "Renamed");
            r.settle_speed_search();
            assert_eq!(
                r.sidebar_speed
                    .as_ref()
                    .unwrap()
                    .matched(&r.sidebar_speed_rows())
                    .unwrap()
                    .label,
                "Привет Alpha"
            );
            r.desk.leave(r.active_workspace, first);
            r.desk.leave(r.active_workspace, linked);
            r.settle_speed_search();
            assert!(
                r.sidebar_speed
                    .as_ref()
                    .unwrap()
                    .matched(&r.sidebar_speed_rows())
                    .is_none()
            );
            let other = r.desk.add_workspace("Other", None).unwrap();
            r.select_workspace(other, cx);
            assert!(r.sidebar_speed.is_none());
        })
    });
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn sidebar_blur_and_owned_fields_keep_input(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("note.txt"), "").unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a");
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let previous = r.active_workspace;
            let other = r.desk.add_workspace("Indirect switch", None).unwrap();
            r.active_workspace = other;
            r.settle_speed_search();
            assert!(
                r.sidebar_speed.is_none(),
                "indirect workspace changes must reset search too"
            );
            r.active_workspace = previous;
            cx.notify();
        })
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a tab");
    cx.update(|window, cx| {
        assert!(root.read(cx).sidebar_speed.is_none());
        assert!(root.read(cx).sidebar_focus.is_focused(window));
    });
    cx.simulate_keystrokes("a");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_file(std::path::Path::new("note.txt"), window, cx);
            r.refocus(window, cx);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("x");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(
            r.sidebar_speed.is_none(),
            "editor focus dismisses the query"
        );
        let crate::Body::File { tab, .. } = r.active_body().unwrap() else {
            panic!()
        };
        assert_eq!(
            tab.read(cx).editor().unwrap().read(cx).text().to_string(),
            "x"
        );
    });
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            let group = r.column_groups_for(r.active_workspace).remove(0);
            r.start_renaming_project(group.key, group.name, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a n e w");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(
            r.sidebar_speed.is_none(),
            "a sidebar descendant field owns its input"
        );
        assert_eq!(r.renaming_project().unwrap().1, "new");
    });
    cx.simulate_keystrokes("escape");
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn sidebar_scroll_menu_and_new_worktree_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (root, cx) = explorer_fixture(cx, dir.path());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            for i in 0..20 {
                let path = dir.path().join(format!("Project {i:02}"));
                std::fs::create_dir(&path).unwrap();
                r.desk.open_in(r.active_workspace, &path);
            }
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("1 9");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let r = root.read(cx);
        let scroll = r.project_drag.scroll(r.active_workspace);
        let row = r
            .sidebar_speed
            .as_ref()
            .unwrap()
            .matched(&r.sidebar_speed_rows())
            .unwrap()
            .row;
        let mut bounds = scroll.bounds_for_item(row).unwrap();
        bounds.origin += scroll.offset();
        assert!(bounds.top() >= scroll.bounds().top());
        assert!(
            bounds.top() < scroll.bounds().bottom(),
            "search must reveal the offscreen header"
        );
        assert_eq!(r.column_groups_for(r.active_workspace).len(), 21);
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_menu(
                crate::view::menu::Target::ProjectSort {
                    workspace: r.active_workspace,
                },
                crate::view::menu::Anchor::Pointer(gpui::point(px(20.), px(80.))),
                window,
                cx,
            );
        })
    });
    cx.simulate_keystrokes("a");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(r.context_menu.is_some());
        assert!(
            r.sidebar_speed.is_none(),
            "a menu must not activate sidebar search"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.open_new_worktree(
                r.active_project.unwrap(),
                gpui::point(px(20.), px(80.)),
                window,
                cx,
            );
        })
    });
    cx.simulate_keystrokes("a b");
    cx.update(|_, cx| {
        let r = root.read(cx);
        assert!(r.sidebar_speed.is_none());
        assert_eq!(r.new_worktree.as_ref().unwrap().typed, "ab");
    });
    cx.simulate_keystrokes("escape");
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn explorer_render_reads_cut_clipboard_once(cx: &mut TestAppContext) {
    use crate::root::transfers::clipboard_probe::READS;
    let dir = tempfile::tempdir().unwrap();
    for at in 0..120 {
        std::fs::write(dir.path().join(format!("file-{at:03}.txt")), "text").unwrap();
    }
    let (root, cx) = explorer_fixture(cx, dir.path());
    let bounds = cx.debug_bounds("explorer-row-file-000.txt").unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            for cut in [false, true] {
                root.file_clipboard(cut, cx);
                READS.with(|n| n.set(0));
                drop(root.render_tree(cx));
                assert_eq!(READS.with(|n| n.get()), usize::from(cut));
            }
            let project = root.active_project;
            let other = tempfile::tempdir().unwrap();
            std::fs::write(other.path().join("other.txt"), "other").unwrap();
            let other_project = root
                .desk
                .open_in(root.desk.active_workspace(), other.path());
            root.active_project = Some(other_project);
            root.ensure_active_tree(cx);
            READS.with(|n| n.set(0));
            drop(root.render_tree(cx));
            assert_eq!(
                READS.with(|n| n.get()),
                0,
                "another project's cut is irrelevant"
            );
            root.active_project = project;
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("replacement".into()));
            READS.with(|n| n.set(0));
            drop(root.render_tree(cx));
            assert_eq!(READS.with(|n| n.get()), 1);
            assert!(root.cut_sources(cx).is_empty());
        });
    });
}

/// The column's search finds project groups as well as projects: a group's
/// name is a candidate the arrows and Enter reach, and a project inside a
/// collapsed group is drawn — its group open — while the query finds it,
/// without the group's saved state changing.
#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn sidebar_finds_groups_and_projects_inside_collapsed_groups(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["revust", "agent-tools", "atlas", "skills"] {
        std::fs::create_dir(dir.path().join(name)).unwrap();
    }
    let (root, cx) = explorer_fixture(cx, &dir.path().join("revust"));
    let path = |name: &str| dir.path().join(name);
    let (review, acme, skills) = cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            let workspace = r.active_workspace;
            r.desk.open_in(workspace, &path("agent-tools"));
            // The search starts from the project in front.
            r.active_project = Some(r.desk.open_in(workspace, &path("atlas")));
            // Enough projects that the collapsed group is far below.
            for i in 0..20 {
                let filler = dir.path().join(format!("filler-{i:02}"));
                std::fs::create_dir(&filler).unwrap();
                r.desk.open_in(workspace, &filler);
            }
            let skills = r.desk.open_in(workspace, &path("skills"));
            let review = r
                .desk
                .create_project_group(workspace, "agent-review", &path("revust"))
                .unwrap();
            let acme = r
                .desk
                .create_project_group(workspace, "Acme", &path("skills"))
                .unwrap();
            r.desk.toggle_project_group(workspace, acme);
            cx.notify();
            (review, acme, skills)
        })
    });
    let group = crate::column::Listed::Group;
    let repository = |name: &str| crate::column::Listed::Repository(path(name));
    let current = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| {
            root.read(cx)
                .sidebar_speed
                .as_ref()
                .and_then(|s| s.current.clone())
        })
    };
    let collapsed = |cx: &mut gpui::VisualTestContext, id| {
        cx.update(|_, cx| {
            root.read(cx)
                .desk
                .workspace(root.read(cx).active_workspace)
                .unwrap()
                .project_group(id)
                .unwrap()
                .collapsed
        })
    };
    let skills_header: &'static str =
        Box::leak(format!("project-header-{}", path("skills").display()).into_boxed_str());
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });

    // "agent", from atlas: round to the group's name, then the ungrouped
    // project after it.
    cx.simulate_keystrokes("a g e n t");
    cx.run_until_parked();
    assert_eq!(current(cx), Some(group(review)));
    let head: &'static str = Box::leak(format!("project-group-{review}").into_boxed_str());
    assert!(cx.debug_bounds(head).is_some());
    cx.simulate_keystrokes("down");
    assert_eq!(current(cx), Some(repository("agent-tools")));
    cx.simulate_keystrokes("up");
    assert_eq!(current(cx), Some(group(review)));

    // A project inside the collapsed group is drawn while it is found.
    assert!(cx.debug_bounds(skills_header).is_none(), "collapsed");
    cx.simulate_keystrokes("cmd-a s k i l l s");
    cx.run_until_parked();
    assert_eq!(current(cx), Some(repository("skills")));
    let header = cx
        .debug_bounds(skills_header)
        .expect("a found project's group is open while the query finds it");
    // The list is scrolled to that project's own head: the search's row
    // counts the groups' headers as the drawn list does.
    cx.update(|_, cx| {
        let r = root.read(cx);
        let scroll = r.project_drag.scroll(r.active_workspace);
        let row = r
            .sidebar_speed
            .as_ref()
            .unwrap()
            .matched(&r.sidebar_speed_rows())
            .unwrap()
            .row;
        let mut item = scroll.bounds_for_item(row).unwrap();
        item.origin += scroll.offset();
        assert!(
            item.top() >= scroll.bounds().top() && item.top() < scroll.bounds().bottom(),
            "the found row is scrolled into view: {item:?} in {:?}",
            scroll.bounds()
        );
        assert!(
            (f32::from(item.top() - header.top())).abs() < 1.0,
            "the row scrolled to is the found project's: {item:?} vs {header:?}"
        );
    });
    assert!(collapsed(cx, acme), "the saved state is untouched");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds(skills_header).is_none(), "collapsed again");

    // Enter on a group expands it; Enter on a project opens it, as before.
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("a c m e enter");
    cx.run_until_parked();
    assert!(!collapsed(cx, acme), "Enter on a group expands it");
    cx.update(|_, cx| assert_ne!(root.read(cx).active_project, Some(skills)));
    cx.update(|window, cx| {
        root.update(cx, |r, cx| {
            r.desk.toggle_project_group(r.active_workspace, acme);
            window.focus(&r.sidebar_focus, cx);
            cx.notify();
        })
    });
    cx.simulate_keystrokes("s k i l l s enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(root.read(cx).active_project, Some(skills)));
    assert!(!collapsed(cx, acme), "the project in front is never hidden");
}
