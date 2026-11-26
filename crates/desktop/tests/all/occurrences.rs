use gpui::{AppContext, TestAppContext};
use gpui_component::input::EditorState;
use guild_desktop::editor::Occurrences;

/// A caret, without selecting text, finds whole case-sensitive Unicode identifiers.
/// Movement, edits and selections must never leave stale backgrounds behind.
#[gpui::test]
fn caret_occurrences_follow_movement_edits_and_selection(cx: &mut TestAppContext) {
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx).default_value("имя имя Имя имя2 _имя a\u{301} a\u{301}")
        });
        let occurrences = Occurrences::attach(&editor, guild_desktop::theme::Theme::default(), cx);
        source = Some((editor.clone(), occurrences));
        gpui_component::Root::new(cx.new(|_| Source(editor)), window, cx)
    });
    let (editor, occurrences) = source.unwrap();
    cx.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(2..2, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(occurrences.read(cx).ranges(cx), vec![0..6, 7..13]);
        assert_eq!(editor.read(cx).selected_range(), 2..2);
    });
    for range in [6..6, 1..5] {
        cx.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(range, cx)));
        cx.run_until_parked();
        cx.update(|_, cx| assert!(occurrences.read(cx).ranges(cx).is_empty()));
    }
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("fn name() { name(); name2(); Name(); }", window, cx);
            s.set_selected_range(5..5, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), vec![3..7, 12..16]));
    // A replacement with an unchanged caret still refreshes the identifier.
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("fn next() { name(); next(); }", window, cx);
            s.set_selected_range(5..5, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), vec![3..7, 20..24]));
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("a\u{301} a\u{301} a a\u{301}2", window, cx);
            s.set_selected_range(1..1, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), vec![0..3, 4..7]));
    for (text, caret, expected) in [
        ("_x _x x", 1, vec![0..2, 3..5]),
        ("𐐀x 𐐀x 𐐨x", 4, vec![0..5, 6..11]),
        ("12 12", 1, vec![]),
        ("name(name)", 4, vec![]),
        ("name", 4, vec![]),
    ] {
        cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.set_value(text, window, cx);
                s.set_selected_range(caret..caret, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), expected, "{text}"));
    }
}

struct Source(gpui::Entity<EditorState>);
impl gpui::Render for Source {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui_component::input::Editor::new(&self.0)
    }
}

/// Occurrence refresh must neither own Find's decorations nor move the caret.
#[gpui::test]
fn occurrences_coexist_with_find_diagnostics_and_typing(cx: &mut TestAppContext) {
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let editor = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("rust")
                .default_value("let count = count + 1;")
        });
        let occurrences = Occurrences::attach(&editor, guild_desktop::theme::Theme::default(), cx);
        editor.update(cx, |s, cx| {
            let text = s.text().clone();
            let diagnostics = s.diagnostics_mut().unwrap();
            diagnostics.reset(&text);
            diagnostics.push(lsp_types::Diagnostic::new_simple(
                lsp_types::Range::new(
                    lsp_types::Position::new(0, 4),
                    lsp_types::Position::new(0, 9),
                ),
                "fixture diagnostic".into(),
            ));
            s.set_selected_range(6..6, cx);
        });
        let search = cx.new(|cx| {
            guild_desktop::local_search::SearchBar::new(
                editor.clone(),
                guild_desktop::theme::Theme::default(),
                guild_desktop::i18n::Ui(guild_session::Language::English),
                false,
                cx,
            )
        });
        search.update(cx, |s, cx| {
            s.show(false, window, cx);
            s.set_query(
                guild_desktop::search::Query {
                    text: "count".into(),
                    ..Default::default()
                },
                cx,
            )
        });
        editor.update(cx, |s, cx| s.focus(window, cx));
        source = Some((editor.clone(), occurrences, search));
        gpui_component::Root::new(cx.new(|_| Source(editor)), window, cx)
    });
    let (editor, occurrences, search) = source.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(occurrences.read(cx).ranges(cx), vec![4..9, 12..17]);
        assert_eq!(search.read(cx).found.len(), 2);
        assert_eq!(editor.read(cx).selected_range(), 6..6);
        assert_eq!(
            editor.read(cx).diagnostics().unwrap().range(4..9).count(),
            1
        );
    });
    cx.update(|_, cx| {
        occurrences.update(cx, |o, cx| {
            o.restyle(
                guild_desktop::theme::Theme::of(guild_desktop::theme::Kind::Light),
                cx,
            );
        })
    });
    cx.update(|_, cx| {
        assert_eq!(occurrences.read(cx).ranges(cx), vec![4..9, 12..17]);
        assert_eq!(editor.read(cx).selected_range(), 6..6);
    });
    cx.simulate_input("s");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).value().as_ref(), "let cosunt = count + 1;");
        assert_eq!(occurrences.read(cx).ranges(cx), vec![4..10]);
        assert_eq!(search.read(cx).found.len(), 1);
    });
    cx.simulate_keystrokes("shift-right");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(occurrences.read(cx).ranges(cx).is_empty());
        assert!(!editor.read(cx).selected_range().is_empty());
        assert_eq!(search.read(cx).found.len(), 1);
    });
}

/// A library source keeps a caret across reloads but never accepts user edits.
#[gpui::test]
fn readonly_file_tab_keeps_caret_navigation_and_rejects_edits(cx: &mut TestAppContext) {
    use gpui::{Focusable as _, px};
    use guild_desktop::{
        file_tab::{FileTab, Look},
        i18n::Ui,
        theme::Theme,
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.txt");
    std::fs::write(&path, "name name").unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let tab = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("library.txt"),
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
            .read_only(true)
        });
        file = Some(tab.clone());
        gpui_component::Root::new(tab, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(320.)));
    cx.run_until_parked();
    let file = file.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    // Use the actual editor's public shaped geometry for an ordinary click.
    let at = cx.update(|_, cx| editor.read(cx).range_to_bounds(&(6..6)).unwrap().center());
    cx.simulate_click(at, Default::default());
    let clicked = cx.update(|_, cx| {
        let range = editor.read(cx).selected_range();
        assert!(
            range.is_empty() && (5..9).contains(&range.start),
            "click must reach the second word: {range:?}"
        );
        range.start
    });
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), clicked + 1..clicked + 1));
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).selected_range(), 0..9);
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            "name name"
        );
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("replacement".into()));
    });
    cx.simulate_keystrokes("cmd-v");
    cx.simulate_input("X");
    cx.simulate_keystrokes("backspace delete cmd-x");
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "name name"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "name name");
    std::fs::write(&path, "next next").unwrap();
    cx.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
    cx.run_until_parked();
    cx.simulate_keystrokes("right");
    cx.simulate_input("X");
    cx.update(|_, cx| {
        assert_eq!(
            file.read(cx).editor().unwrap().read(cx).value().as_ref(),
            "next next"
        );
        assert!(!file.read(cx).is_dirty());
        assert!(file.update(cx, |file, cx| file.save(cx)).is_err());
    });
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "next next");
}

#[gpui::test]
fn secondary_selection_clears_occurrences_even_when_primary_caret_stays_put(
    cx: &mut TestAppContext,
) {
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value("name\nname"));
        let occurrences = Occurrences::attach(&editor, guild_desktop::theme::Theme::default(), cx);
        source = Some((editor.clone(), occurrences));
        gpui_component::Root::new(cx.new(|_| Source(editor)), window, cx)
    });
    cx.run_until_parked();
    let (editor, occurrences) = source.unwrap();
    let at = cx.update(|_, cx| editor.read(cx).range_to_bounds(&(7..7)).unwrap().center());
    cx.simulate_click(
        at,
        gpui::Modifiers {
            alt: true,
            ..Default::default()
        },
    );
    cx.simulate_keystrokes("shift-left");
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).selected_range(),
            0..0,
            "keep the original active caret"
        );
        assert!(
            editor.read(cx).is_copyable(),
            "a secondary range was selected"
        );
        assert!(occurrences.read(cx).ranges(cx).is_empty());
    });
    cx.simulate_keystrokes("right");
    cx.update(|_, cx| {
        assert!(!editor.read(cx).is_copyable());
        assert_eq!(occurrences.read(cx).ranges(cx), vec![0..4, 5..9]);
    });
}

#[gpui::test]
fn dense_occurrences_skip_the_entire_file_then_recover_after_shrinking(cx: &mut TestAppContext) {
    let mut source = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let editor = cx.new(|cx| EditorState::new(window, cx).default_value("a;".repeat(4097)));
        let occurrences = Occurrences::attach(&editor, guild_desktop::theme::Theme::default(), cx);
        source = Some((editor.clone(), occurrences));
        gpui_component::Root::new(cx.new(|_| Source(editor)), window, cx)
    });
    let (editor, occurrences) = source.unwrap();
    cx.update(|_, cx| {
        assert!(
            occurrences.read(cx).ranges(cx).is_empty(),
            "no partial highlight beyond the 4096 budget"
        )
    });
    cx.update(|window, cx| editor.update(cx, |s, cx| s.set_value("a;".repeat(4096), window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let ranges = occurrences.read(cx).ranges(cx);
        assert_eq!(ranges.len(), 4096);
        assert_eq!(ranges.last(), Some(&(8190..8191)));
    });
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| s.set_value("a;".repeat(100_000), window, cx))
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(occurrences.read(cx).ranges(cx).is_empty()));
    cx.update(|window, cx| editor.update(cx, |s, cx| s.set_value("a a", window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), vec![0..1, 2..3]));
}
