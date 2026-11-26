//! Local search actions reach the actual FileTab without editing its contents.
use gpui::prelude::*;
use gpui::{AppContext, Entity, Focusable, TestAppContext};
use guild_desktop::{
    actions::{self, keymap::Named},
    file_tab::{FileTab, Look},
    i18n::Ui,
    theme::Theme,
};
use std::{cell::Cell, rc::Rc};

struct Host {
    file: Entity<FileTab>,
    global: Rc<Cell<usize>>,
}
impl gpui::Render for Host {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        let global = self.global.clone();
        gpui::div()
            .key_context("guild")
            .size_full()
            .on_action(move |_: &actions::FindInFiles, _, cx| {
                global.set(global.get() + 1);
                cx.stop_propagation();
            })
            .child(self.file.clone())
    }
}

#[gpui::test]
fn local_find_replace_and_global_find_follow_focused_file(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.txt"), "alpha beta alpha").unwrap();
    let global = Rc::new(Cell::new(0));
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let bindings = actions::ordered(
            vec![
                ("cmd-f".into(), Named::Find),
                ("cmd-r".into(), Named::Replace),
                ("cmd-shift-f".into(), Named::FindInFiles),
            ],
            &cx.global::<actions::Inherited>().0,
        );
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        let source = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("sample.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        window.focus(&source.read(cx).focus_handle(cx), cx);
        file = Some(source.clone());
        let host = cx.new(|_| Host {
            file: source,
            global: global.clone(),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let file = file.unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert!(search.open);
        assert!(search.replace);
    });
    cx.simulate_input("alpha");
    cx.update(|_, cx| {
        let editor = file.read(cx).editor().unwrap().read(cx);
        assert_eq!(editor.text().to_string(), "alpha beta alpha");
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).query.text,
            "alpha"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert!(search.open);
        assert!(!search.replace);
    });
    cx.simulate_keystrokes("cmd-shift-f");
    assert_eq!(
        global.get(),
        1,
        "global Find must bubble out of the search query Input"
    );
    cx.update(|_, cx| {
        let editor = file.read(cx).editor().unwrap().read(cx);
        assert!(!file.read(cx).local_search().unwrap().read(cx).replace);
        assert_eq!(editor.text().to_string(), "alpha beta alpha");
    });

    // Re-invoking from the query uses the same host; buttons preserve its focus.
    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(file.read(cx).local_search().unwrap().read(cx).replace));
    let regex = cx.debug_bounds("local-regex").unwrap();
    cx.simulate_click(regex.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("alpha|beta");
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert_eq!(search.query.text, "alpha|beta");
        assert_eq!(search.found.len(), 3);
    });
    cx.simulate_keystrokes("alt-c");
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert!(search.query.case_sensitive);
        assert_eq!(
            search.query.text, "alpha|beta",
            "option toggle must not insert an accented character"
        );
    });
    let multiline = cx.debug_bounds("local-multiline").unwrap();
    cx.simulate_click(multiline.center(), Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("alpha\nbeta");
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).query.text,
            "alpha\nbeta"
        );
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "alpha beta alpha"
        );
    });
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(!file.read(cx).local_search().unwrap().read(cx).open));
    cx.simulate_keystrokes("cmd-r");
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| {
            search.set_query(
                guild_desktop::search::Query {
                    text: "alpha".into(),
                    ..Default::default()
                },
                cx,
            )
        });
    });
    cx.run_until_parked();
    let selection = cx.debug_bounds("local-selection").unwrap();
    cx.simulate_click(selection.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found.len(),
            1,
            "selection scope stays on the original source selection"
        )
    });
    cx.simulate_keystrokes("tab");
    cx.simulate_input("omega");
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).replacement,
            "omega"
        )
    });
    let replace = cx.debug_bounds("local-replace-all").unwrap();
    cx.simulate_click(replace.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "omega beta alpha"
        )
    });
    cx.simulate_keystrokes("escape cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "alpha beta alpha"
        )
    });
}

#[gpui::test]
fn replace_all_is_one_undo_and_preserves_prior_edits(cx: &mut TestAppContext) {
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx
            .new(|cx| gpui_component::input::EditorState::new(window, cx).default_value("cat cat"));
        window.focus(&state.read(cx).focus_handle(cx), cx);
        source = Some(state.clone());
        let host = cx.new(|_| EditorHost(state));
        gpui_component::Root::new(host, window, cx)
    });
    let source = source.unwrap();
    cx.simulate_input("prior ");
    cx.update(|window, cx| {
        source.update(cx, |source, cx| {
            let text = source.text().to_string();
            let found = guild_desktop::document_search::matches(
                &text,
                &guild_desktop::search::Query {
                    text: "cat".into(),
                    ..Default::default()
                },
                "dog",
                false,
            )
            .unwrap();
            assert_eq!(found.len(), 2);
            guild_desktop::document_search::apply(
                source,
                guild_desktop::document_search::replaced(&text, &found),
                window,
                cx,
            );
            assert_eq!(source.text().to_string(), "prior dog dog");
        })
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            source.read(cx).text().to_string(),
            "prior cat cat",
            "one Undo must restore both replacements while retaining prior typing"
        )
    });
}
struct EditorHost(Entity<gpui_component::input::EditorState>);
impl gpui::Render for EditorHost {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui_component::input::Editor::new(&self.0).size_full()
    }
}

#[gpui::test]
fn replacement_maps_selection_and_preserves_scrolled_viewport(cx: &mut TestAppContext) {
    let text = (0..200)
        .map(|n| format!("line {n}: cat cat\n"))
        .collect::<String>();
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            gpui_component::input::EditorState::new(window, cx).default_value(text.clone())
        });
        source = Some(state.clone());
        let host = cx.new(|_| EditorHost(state));
        gpui_component::Root::new(host, window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(600.), gpui::px(320.)));
    let source = source.unwrap();
    cx.update(|window, cx| {
        source.update(cx, |source, cx| {
            source.set_cursor_position(gpui_component::input::Position::new(100, 0), window, cx)
        })
    });
    cx.run_until_parked();
    let before = cx.update(|_, cx| source.read(cx).scroll_offset());
    cx.update(|_, cx| {
        source.update(cx, |source, cx| {
            let value = source.text().to_string();
            let at = value.find("cat").unwrap();
            source.set_selected_range(at..at + 3, cx);
            source.set_scroll_offset(before, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        source.update(cx, |source, cx| {
            let value = source.text().to_string();
            let found = guild_desktop::document_search::matches(
                &value,
                &guild_desktop::search::Query {
                    text: "cat".into(),
                    ..Default::default()
                },
                "longer",
                false,
            )
            .unwrap();
            guild_desktop::document_search::apply_matches(source, &found, window, cx);
            assert_eq!(source.selected_text().to_string(), "longer");
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(source.read(cx).scroll_offset(), before));
}

#[gpui::test]
fn editor_owned_replacement_keeps_bom_crlf_permissions_and_no_final_newline(
    cx: &mut TestAppContext,
) {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.txt");
    std::fs::write(&path, "\u{feff}cat\r\ncat").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let entity = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("source.txt"),
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
        file = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let file = file.unwrap();
    let snapshot = guild_desktop::project_search::Snapshot::read(
        dir.path(),
        std::path::Path::new("source.txt"),
        None,
    )
    .unwrap();
    snapshot.validate(dir.path()).unwrap();
    let edits = guild_desktop::document_search::matches(
        &snapshot.text,
        &guild_desktop::search::Query {
            text: "cat".into(),
            ..Default::default()
        },
        "dog",
        false,
    )
    .unwrap();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.replace_snapshot(&snapshot, edits, window, cx).unwrap();
            file.save(cx).unwrap();
        })
    });
    assert_eq!(
        std::fs::read(&path).unwrap(),
        "\u{feff}dog\r\ndog".as_bytes()
    );
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}

#[gpui::test]
fn selection_scope_follows_source_undo_redo_and_manual_input(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.txt"), "cat cat").unwrap();
    let global = Rc::new(Cell::new(0));
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let bindings = actions::ordered(
            vec![
                ("cmd-f".into(), Named::Find),
                ("cmd-r".into(), Named::Replace),
                ("cmd-shift-f".into(), Named::FindInFiles),
            ],
            &cx.global::<actions::Inherited>().0,
        );
        cx.clear_key_bindings();
        cx.bind_keys(bindings);
        let source = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("sample.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        window.focus(&source.read(cx).focus_handle(cx), cx);
        file = Some(source.clone());
        let host = cx.new(|_| Host {
            file: source,
            global: global.clone(),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let file = file.unwrap();
    cx.update(|_, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| editor.set_selected_range(0..3, cx));
    });
    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    let selection = cx.debug_bounds("local-selection").unwrap();
    cx.simulate_click(selection.center(), Default::default());
    cx.update(|window, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| {
            search.replacement = "elephant".into();
            search.replace_matches(true, window, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        window.dispatch_action(Box::new(gpui_component::input::Undo), cx);
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "cat cat"
        );
        assert_eq!(search.query.text, "cat");
        assert_eq!(
            search.found.len(),
            1,
            "Undo must restore the original selection boundary"
        );
    });
    cx.simulate_keystrokes("cmd-shift-z");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| search.replace_matches(true, window, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "elephant cat"
        )
    });
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| {
                editor.set_selected_range(0..0, cx);
                editor.insert("я ", window, cx);
            })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| search.replace_matches(true, window, cx));
    });
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "я elephant cat"
        )
    });
    cx.update(|window, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| search.close(window, cx));
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| {
                editor.replace_all("cat cat", window, cx);
                editor.set_selected_range(4..7, cx);
            });
        search.update(cx, |search, cx| search.show(true, window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| {
                editor.set_selected_range(0..4, cx);
                editor.replace("", window, cx);
            })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let search = file.read(cx).local_search().unwrap().clone();
        search.update(cx, |search, cx| search.replace_matches(true, window, cx));
    });
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "elephant",
            "tracked region must follow the second identical word after deletion before it"
        )
    });
    // Unrelated source edits must not evict the search operation's Undo region.
    for _ in 0..100 {
        cx.update(|window, cx| {
            file.read(cx)
                .editor()
                .unwrap()
                .clone()
                .update(cx, |editor, cx| {
                    let end = editor.text().len();
                    editor.set_selected_range(end..end, cx);
                    editor.insert("!", window, cx);
                })
        });
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx)
    });
    for _ in 0..101 {
        cx.simulate_keystrokes("cmd-z");
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            "cat"
        );
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found.len(),
            1
        );
    });
}

#[gpui::test]
fn dense_prefix_deletions_keep_unicode_tail_and_one_undo(cx: &mut TestAppContext) {
    let value = "cat".repeat(5_000) + &"я🦀".repeat(20_000);
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            gpui_component::input::EditorState::new(window, cx).default_value(value.clone())
        });
        window.focus(&state.read(cx).focus_handle(cx), cx);
        source = Some(state.clone());
        let host = cx.new(|_| EditorHost(state));
        gpui_component::Root::new(host, window, cx)
    });
    let source = source.unwrap();
    cx.simulate_input("prior ");
    cx.update(|window, cx| {
        source.update(cx, |source, cx| {
            let text = source.text().to_string();
            let found = guild_desktop::document_search::matches(
                &text,
                &guild_desktop::search::Query {
                    text: "cat".into(),
                    ..Default::default()
                },
                "",
                false,
            )
            .unwrap();
            assert_eq!(found.len(), 5_000);
            guild_desktop::document_search::apply(
                source,
                guild_desktop::document_search::replaced(&text, &found),
                window,
                cx,
            );
            assert_eq!(
                source.text().to_string(),
                format!("prior {}", "я🦀".repeat(20_000))
            );
        })
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            source.read(cx).text().to_string(),
            format!("prior {value}"),
            "one Undo must restore both replacements while retaining prior typing"
        )
    });
}

fn scoped_file<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
    selection: Option<std::ops::Range<usize>>,
) -> (
    tempfile::TempDir,
    Entity<FileTab>,
    &'a mut gpui::VisualTestContext,
) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.txt"), text).unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let inherited = cx.global::<actions::Inherited>().0.clone();
        cx.bind_keys(actions::ordered(
            vec![
                ("cmd-f".into(), Named::Find),
                ("cmd-r".into(), Named::Replace),
            ],
            &inherited,
        ));
        let source = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("sample.txt"),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        window.focus(&source.read(cx).focus_handle(cx), cx);
        file = Some(source.clone());
        let host = cx.new(|_| Host {
            file: source,
            global: Rc::new(Cell::new(0)),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let file = file.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        if let Some(range) = selection.clone() {
            file.read(cx)
                .editor()
                .unwrap()
                .clone()
                .update(cx, |editor, cx| editor.set_selected_range(range, cx));
        }
    });

    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    if selection.is_some() {
        let bounds = cx.debug_bounds("local-selection").unwrap();
        cx.simulate_click(bounds.center(), Default::default());
    }
    configure_local(&file, "dog", cx);
    (dir, file, cx)
}
fn configure_local(file: &Entity<FileTab>, replacement: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        file.read(cx)
            .local_search()
            .unwrap()
            .clone()
            .update(cx, |search, cx| {
                search.replacement = replacement.into();
                search.set_query(
                    guild_desktop::search::Query {
                        text: "cat".into(),
                        ..Default::default()
                    },
                    cx,
                );
            })
    });
}
fn replace_local(file: &Entity<FileTab>, all: bool, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        file.read(cx)
            .local_search()
            .unwrap()
            .clone()
            .update(cx, |search, cx| search.replace_matches(all, window, cx))
    });
    cx.run_until_parked();
}
fn source_edit(
    file: &Entity<FileTab>,
    range: std::ops::Range<usize>,
    text: &str,
    cx: &mut gpui::VisualTestContext,
) {
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .clone()
            .update(cx, |editor, cx| {
                editor.set_selected_range(range, cx);
                editor.replace(text, window, cx);
            })
    });
    cx.run_until_parked();
}
fn source_history(file: &Entity<FileTab>, redo: bool, cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx)
    });
    cx.simulate_keystrokes(if redo { "cmd-shift-z" } else { "cmd-z" });
    cx.run_until_parked();
}
fn assert_source(file: &Entity<FileTab>, text: &str, cx: &mut gpui::VisualTestContext) {
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).text().to_string(),
            text
        )
    });
}

#[gpui::test]
fn project_edits_preserve_local_selection_scope(cx: &mut TestAppContext) {
    let (dir, file, cx) = scoped_file(cx, "cat cat x", Some(0..3));
    let snapshot = guild_desktop::project_search::Snapshot::read(
        dir.path(),
        std::path::Path::new("sample.txt"),
        None,
    )
    .unwrap();
    let edits = guild_desktop::document_search::matches(
        &snapshot.text,
        &guild_desktop::search::Query {
            text: "x".into(),
            ..Default::default()
        },
        "y",
        false,
    )
    .unwrap();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            file.replace_snapshot(&snapshot, edits, window, cx).unwrap()
        })
    });
    cx.run_until_parked();
    assert_source(&file, "cat cat y", cx);
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found.len(),
            1
        )
    });
    replace_local(&file, true, cx);
    assert_source(&file, "dog cat y", cx);
}

#[gpui::test]
fn partial_selection_delete_undo_redo_restores_exact_boundaries(cx: &mut TestAppContext) {
    let (_dir, file, cx) = scoped_file(cx, "cat cat", Some(0..3));
    source_edit(&file, 2..3, "", cx);
    source_history(&file, false, cx);
    assert_source(&file, "cat cat", cx);
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found.len(),
            1
        )
    });
    source_history(&file, true, cx);
    assert_source(&file, "ca cat", cx);
    cx.update(|_, cx| {
        assert!(
            file.read(cx)
                .local_search()
                .unwrap()
                .read(cx)
                .found
                .is_empty()
        )
    });
    source_history(&file, false, cx);
    replace_local(&file, true, cx);
    assert_source(&file, "dog cat", cx);
}

#[gpui::test]
fn repeated_content_from_typing_cannot_restore_an_undone_scope(cx: &mut TestAppContext) {
    let (_dir, file, cx) = scoped_file(cx, "catcat", Some(0..3));
    configure_local(&file, "catcat", cx);
    replace_local(&file, true, cx);
    assert_source(&file, "catcatcat", cx);
    source_history(&file, false, cx);
    assert_source(&file, "catcat", cx);
    source_edit(&file, 6..6, "cat", cx);
    configure_local(&file, "dog", cx);
    replace_local(&file, true, cx);
    assert_source(&file, "dogcatcat", cx);
}

#[gpui::test]
fn repeated_content_from_another_delete_cannot_restore_an_undone_scope(cx: &mut TestAppContext) {
    let (_dir, file, cx) = scoped_file(cx, "catcat", Some(3..6));
    source_edit(&file, 0..3, "", cx);
    source_history(&file, false, cx);
    assert_source(&file, "catcat", cx);
    source_edit(&file, 3..6, "", cx);
    replace_local(&file, true, cx);
    assert_source(&file, "cat", cx);
}

#[gpui::test]
fn reload_updates_ranges_and_recovery_rebinds_search_source(cx: &mut TestAppContext) {
    let (dir, file, cx) = scoped_file(cx, "cat dog", None);
    std::fs::write(dir.path().join("sample.txt"), "dog cat").unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found[0].range,
            4..7
        )
    });
    cx.simulate_keystrokes("enter");
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .selected_text()
                .to_string(),
            "cat"
        )
    });
}

#[gpui::test]
fn missing_then_restored_file_gets_a_new_search_source(cx: &mut TestAppContext) {
    let (dir, file, cx) = scoped_file(cx, "dog cat", None);
    let (old_editor, old_search) = cx.update(|_, cx| {
        (
            file.read(cx).editor().unwrap().clone(),
            file.read(cx).local_search().unwrap().clone(),
        )
    });
    std::fs::remove_file(dir.path().join("sample.txt")).unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(file.read(cx).local_search().is_none());
        assert!(!old_search.read(cx).open);
    });
    std::fs::write(dir.path().join("sample.txt"), "fresh cat").unwrap();
    cx.update(|window, cx| {
        file.update(cx, |file, cx| file.reload(window, cx));
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        file.read(cx)
            .editor()
            .unwrap()
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
    });
    cx.simulate_keystrokes("cmd-r");
    cx.run_until_parked();
    configure_local(&file, "dog", cx);
    replace_local(&file, true, cx);
    assert_source(&file, "fresh dog", cx);
    cx.update(|_, cx| assert_eq!(old_editor.read(cx).text().to_string(), "dog cat"));
}

#[gpui::test]
fn single_local_replace_does_not_validate_hypothetical_all_output(cx: &mut TestAppContext) {
    let (_dir, file, cx) = scoped_file(cx, &"cat".repeat(30_000), None);
    configure_local(&file, &"a".repeat(1024), cx);
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).found.len(),
            30_000
        )
    });
    replace_local(&file, false, cx);
    let expected = "a".repeat(1024) + &"cat".repeat(29_999);
    assert_source(&file, &expected, cx);
    replace_local(&file, true, cx);
    assert_source(&file, &expected, cx);
}

#[gpui::test]
fn local_search_fields_share_left_edge_and_width_at_each_viewport(cx: &mut TestAppContext) {
    let (_dir, file, cx) = scoped_file(cx, "cat cat", None);
    for width in [1000., 1280.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(700.)));
        cx.run_until_parked();
        let query = cx.debug_bounds("local-query-field").unwrap();
        let replacement = cx.debug_bounds("local-replacement-field").unwrap();
        assert_eq!(query.origin.x, replacement.origin.x, "shared left edge");
        assert_eq!(
            query.size.width, replacement.size.width,
            "shared field width"
        );
        assert!(cx.debug_bounds("local-history").unwrap().size.width >= gpui::px(24.));
    }
    configure_local(&file, "dog", cx);
    replace_local(&file, true, cx);
    assert_source(&file, "dog dog", cx);
}
