use guild_desktop::editor::SelectionHistory;
use std::ops::Range;

fn range(text: &str, part: &str) -> Range<usize> {
    let start = text.find(part).unwrap();
    start..start + part.len()
}

#[test]
fn mouse_word_grows_through_string_and_syntax_then_shrinks_exactly() {
    let text = "fn main() {\n    call(\"hello мир\");\n}\n";
    let original = range(text, "мир");
    let mut history = SelectionHistory::default();
    let mut selected = original.clone();
    let mut visited = vec![selected.clone()];
    for expected in [
        "hello мир",
        "\"hello мир\"",
        "(\"hello мир\")",
        "call(\"hello мир\")",
    ] {
        selected = history.extend(text, "rust", selected.clone());
        assert_eq!(&text[selected.clone()], expected);
        visited.push(selected.clone());
    }
    while selected != (0..text.len()) {
        let next = history.extend(text, "rust", selected.clone());
        assert!(next.start <= selected.start && next.end >= selected.end && next != selected);
        assert!(text.is_char_boundary(next.start) && text.is_char_boundary(next.end));
        selected = next;
        visited.push(selected.clone());
    }
    assert_eq!(history.extend(text, "rust", selected.clone()), selected);
    for expected in visited.into_iter().rev().skip(1) {
        selected = history.shrink(text, selected);
        assert_eq!(selected, expected);
    }
    assert_eq!(selected, original);
}

#[test]
fn caret_plain_text_and_resets() {
    let text = "first line\nsecond word\n\nlast";
    let mut history = SelectionHistory::default();
    let caret = 3..3;
    let word = history.extend(text, "text", caret.clone());
    assert_eq!(word, 0..5);
    assert_eq!(history.shrink(text, word), caret);
    let line = history.extend(text, "text", 0..5);
    assert_eq!(line, 0..10);
    let line_with_newline = history.extend(text, "text", line);
    assert_eq!(line_with_newline, 0..11);
    let paragraph = history.extend(text, "text", line_with_newline);
    assert_eq!(paragraph, 0..22);
    let paragraph_newline = history.extend(text, "text", paragraph);
    assert_eq!(paragraph_newline, 0..23);
    assert_eq!(
        history.extend(text, "text", paragraph_newline),
        0..text.len()
    );
    assert_eq!(
        history.shrink(text, 24..24),
        24..24,
        "manual movement resets"
    );
    let selected = history.extend(text, "text", 0..5);
    assert_eq!(
        history.shrink("changed", selected.clone()),
        selected,
        "edits reset"
    );
}

use gpui::{AppContext, Focusable, TestAppContext, px, size};
use guild_desktop::{
    file_tab::{FileTab, Look},
    i18n::Ui,
    theme::Theme,
};
use std::{path::Path, time::Duration};

#[gpui::test]
fn production_file_keys_restore_mouse_word_caret_and_reset_on_manual_changes(
    cx: &mut TestAppContext,
) {
    production_keys(cx, false);
}

#[gpui::test]
fn readonly_source_uses_the_same_selection_keys(cx: &mut TestAppContext) {
    production_keys(cx, true);
}

fn production_keys(cx: &mut TestAppContext, readonly: bool) {
    let dir = tempfile::tempdir().unwrap();
    let source = "fn main() {\n    call(\"hello мир\");\n}\n";
    std::fs::write(dir.path().join("main.rs"), source).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::actions::install(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new("main.rs"),
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
            .read_only(readonly)
        });
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.simulate_resize(size(px(650.), px(320.)));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(200));
    cx.run_until_parked();
    let file = made.unwrap();
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor().unwrap().clone();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    let original = range(source, "мир");
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_selected_range(original.start + 2..original.start + 2, cx)
        });
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    let position = cx.update(|window, cx| {
        window.simulate_next_frame(cx);
        editor.read(cx).cursor_layout().unwrap().0.center()
    });
    cx.simulate_event(gpui::MouseDownEvent {
        button: gpui::MouseButton::Left,
        position,
        modifiers: Default::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui::MouseUpEvent {
        button: gpui::MouseButton::Left,
        position,
        modifiers: Default::default(),
        click_count: 2,
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).selected_range(),
            original,
            "production double click selects the word"
        )
    });
    cx.update(|window, cx| {
        let contexts = window.context_stack();
        let bindings = cx
            .key_bindings()
            .borrow()
            .bindings_for_input(&[gpui::Keystroke::parse("cmd-up").unwrap()], &contexts)
            .0;
        assert!(
            bindings
                .first()
                .unwrap()
                .action()
                .partial_eq(&guild_desktop::editor::ExtendSelection),
            "{contexts:?}: {bindings:?}"
        );
        assert!(window.is_action_available(&guild_desktop::editor::ExtendSelection, cx));
    });
    for expected in [
        "hello мир",
        "\"hello мир\"",
        "(\"hello мир\")",
        "call(\"hello мир\")",
    ] {
        cx.simulate_keystrokes("cmd-up");
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.read(cx).selected_value().as_ref(), expected));
    }
    cx.simulate_keystrokes("cmd-down cmd-down cmd-down cmd-down");
    cx.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), original));
    let caret = original.start + 2;
    cx.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(caret..caret, cx)));
    cx.simulate_keystrokes("cmd-up cmd-down");
    cx.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), caret..caret));
    cx.simulate_keystrokes("cmd-up left right cmd-down");
    cx.update(|_, cx| {
        assert!(
            editor.read(cx).selected_range().is_empty(),
            "manual movement discards chain"
        )
    });
    if !readonly {
        // A no-op shrink retains every cursor. A real extension deliberately
        // adopts the active cursor, matching the component's public setter.
        cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.set_value("one\none\n", window, cx);
                s.set_selected_range(0..0, cx);
            })
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("cmd-alt-down cmd-down");
        cx.simulate_input("X");
        cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "Xone\nXone\n"));
        cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.set_value("one\none\n", window, cx);
                s.set_selected_range(0..0, cx);
            })
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("cmd-alt-down cmd-up cmd-down");
        cx.simulate_input("X");
        cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "Xone\none\n"));
        // An actual edit also revokes the old chain.
        cx.simulate_keystrokes("cmd-up");
        cx.simulate_input("changed");
        let edited = cx.update(|_, cx| editor.read(cx).selected_range());
        cx.simulate_keystrokes("cmd-down");
        cx.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), edited));
        cx.update(|window, cx| editor.update(cx, |s, cx| s.set_value(source, window, cx)));
    }
    // Programmatic replacement does not emit Change, but still invalidates history.
    cx.update(|_, cx| editor.update(cx, |s, cx| s.set_selected_range(original.clone(), cx)));
    cx.simulate_keystrokes("cmd-up");
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            s.set_value("replacement", window, cx);
            s.set_selected_range(0..4, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-down");
    cx.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), 0..4));
    // Find is a separate input; the editor's override must not capture its keys.
    cx.update(|window, cx| window.dispatch_action(Box::new(guild_desktop::actions::Find), cx));
    cx.run_until_parked();
    cx.simulate_input("abc");
    cx.run_until_parked();
    let before_keys = cx.update(|window, cx| {
        assert!(!editor.read(cx).focus_handle(cx).is_focused(window));
        let search = file.read(cx).local_search().unwrap().read(cx);
        assert!(search.open);
        assert_eq!(search.query.text, "abc");
        editor.read(cx).selected_range()
    });
    cx.simulate_keystrokes("cmd-up cmd-down");
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).selected_range(), before_keys);
        assert_eq!(
            file.read(cx).local_search().unwrap().read(cx).query.text,
            "abc"
        );
    });
}

#[test]
fn unicode_carets_and_malformed_source_always_grow_strictly() {
    for (grammar, text) in [
        ("rust", "fn f() { let x = \"á 🦀 мир\";"),
        ("unknown", "á 🦀 мир\n\nend"),
        ("python", "f('hello мир')\n"),
        ("javascript", "f(`hello мир`);\n"),
    ] {
        for caret in text.char_indices().map(|(i, _)| i).chain([text.len()]) {
            let mut history = SelectionHistory::default();
            let mut selected = caret..caret;
            while selected != (0..text.len()) {
                let next = history.extend(text, grammar, selected.clone());
                assert!(
                    next != selected && next.start <= selected.start && next.end >= selected.end,
                    "{grammar} {selected:?} -> {next:?}"
                );
                assert!(text.is_char_boundary(next.start) && text.is_char_boundary(next.end));
                selected = next;
            }
        }
    }
}

#[test]
fn arbitrary_prose_quotes_sentences_and_crlf_are_independent_of_grammar() {
    for (text, word, interiors) in [
        (
            "She said 'several words'. Another sentence.",
            "words",
            vec![
                "several words",
                "'several words'",
                "She said 'several words'.",
            ],
        ),
        (
            "Он сказал „несколько слов“. Ещё предложение.",
            "слов",
            vec![
                "несколько слов",
                "„несколько слов“",
                "Он сказал „несколько слов“.",
            ],
        ),
        (
            "Он сказал: «русский текст». Потом ушёл.\r\nСледующая строка.\r\n\r\nКонец.",
            "текст",
            vec![
                "русский текст",
                "«русский текст»",
                "Он сказал: «русский текст».",
            ],
        ),
        (
            r#"She said: "hello \"quoted\" world". Next sentence!"#,
            "world",
            vec![
                r#"hello \"quoted\" world"#,
                r#""hello \"quoted\" world""#,
                r#"She said: "hello \"quoted\" world"."#,
            ],
        ),
        (
            "We heard “outer «русский текст» here”. Another sentence.",
            "текст",
            vec![
                "русский текст",
                "«русский текст»",
                "outer «русский текст» here",
                "“outer «русский текст» here”",
                "We heard “outer «русский текст» here”.",
            ],
        ),
        (
            "Read (the [small phrase] carefully). Continue here.",
            "phrase",
            vec![
                "small phrase",
                "[small phrase]",
                "the [small phrase] carefully",
                "(the [small phrase] carefully)",
                "Read (the [small phrase] carefully).",
            ],
        ),
        (
            "Don't split John's words. They're useful! Last sentence.",
            "words",
            vec!["Don't split John's words."],
        ),
        (
            "An unmatched «quote stays useful. Next sentence.",
            "useful",
            vec!["An unmatched «quote stays useful."],
        ),
    ] {
        let mut results = Vec::new();
        for grammar in ["text", "unknown-prose"] {
            let mut history = SelectionHistory::default();
            let original = range(text, word);
            let mut selected = original.clone();
            let mut visited = vec![selected.clone()];
            for expected in &interiors {
                selected = history.extend(text, grammar, selected);
                assert_eq!(&text[selected.clone()], *expected, "{grammar}: {text}");
                visited.push(selected.clone());
            }
            while selected != (0..text.len()) {
                let next = history.extend(text, grammar, selected.clone());
                assert!(
                    next != selected && next.start <= selected.start && next.end >= selected.end
                );
                assert!(text.is_char_boundary(next.start) && text.is_char_boundary(next.end));
                selected = next;
                visited.push(selected.clone());
            }
            results.push(visited.clone());
            for previous in visited.into_iter().rev().skip(1) {
                selected = history.shrink(text, selected);
                assert_eq!(selected, previous);
            }
            assert_eq!(selected, original);
        }
        assert_eq!(results[0], results[1]);
    }
}

#[gpui::test]
fn arbitrary_text_file_double_click_extends_and_shrinks(cx: &mut TestAppContext) {
    for filename in ["notes.txt", "notes.unknown-extension"] {
        let dir = tempfile::tempdir().unwrap();
        let source = "Он сказал: «русский текст». Потом ушёл.\nДругая строка.\n\nКонец.\n";
        std::fs::write(dir.path().join(filename), source.replace('\n', "\r\n")).unwrap();
        let mut made = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            guild_desktop::editor::init(cx);
            guild_desktop::actions::install(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new(filename),
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
            made = Some(file.clone());
            gpui_component::Root::new(file, window, cx)
        });
        visual.simulate_resize(size(px(800.), px(320.)));
        visual.run_until_parked();
        visual.executor().advance_clock(Duration::from_millis(200));
        visual.run_until_parked();
        let file = made.unwrap();
        let original = range(source, "текст");
        let editor = visual.update(|window, cx| {
            let editor = file.read(cx).editor().unwrap().clone();
            window.focus(&editor.read(cx).focus_handle(cx), cx);
            editor.update(cx, |s, cx| {
                s.set_selected_range(original.start + 2..original.start + 2, cx)
            });
            editor
        });
        visual.run_until_parked();
        let position = visual.update(|window, cx| {
            window.simulate_next_frame(cx);
            editor.read(cx).cursor_layout().unwrap().0.center()
        });
        visual.simulate_event(gpui::MouseDownEvent {
            button: gpui::MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 2,
            first_mouse: false,
        });
        visual.simulate_event(gpui::MouseUpEvent {
            button: gpui::MouseButton::Left,
            position,
            modifiers: Default::default(),
            click_count: 2,
        });
        visual.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), original));
        for expected in [
            "русский текст",
            "«русский текст»",
            "Он сказал: «русский текст».",
            "Он сказал: «русский текст». Потом ушёл.",
        ] {
            visual.simulate_keystrokes("cmd-up");
            visual.update(|_, cx| {
                assert_eq!(
                    editor.read(cx).selected_value().as_ref(),
                    expected,
                    "{filename}"
                )
            });
        }
        visual.simulate_keystrokes("cmd-down cmd-down cmd-down cmd-down");
        visual.update(|_, cx| assert_eq!(editor.read(cx).selected_range(), original));
    }
}

#[test]
fn malformed_and_deep_prose_punctuation_remains_bounded_and_unicode_safe() {
    for text in [
        format!("{}мир рядом{}", "«([".repeat(1000), "])»".repeat(1000)),
        "Before «unmatched [nested “мир рядом). Still useful!\r\n\r\nLast.".into(),
        format!(
            "{}мир. {}",
            "prefix ".repeat(10_000),
            "tail ".repeat(10_000)
        ),
    ] {
        let original = range(&text, "мир");
        let mut chains = Vec::new();
        for grammar in ["text", "unknown"] {
            let mut history = SelectionHistory::default();
            let mut selected = original.clone();
            let mut visited = vec![selected.clone()];
            while selected != (0..text.len()) {
                let next = history.extend(&text, grammar, selected.clone());
                assert!(
                    next != selected && next.start <= selected.start && next.end >= selected.end
                );
                assert!(text.is_char_boundary(next.start) && text.is_char_boundary(next.end));
                selected = next;
                visited.push(selected.clone());
            }
            chains.push(visited.clone());
            for previous in visited.into_iter().rev().skip(1) {
                selected = history.shrink(&text, selected);
                assert_eq!(selected, previous);
            }
        }
        assert_eq!(chains[0], chains[1]);
    }
}

#[test]
fn prose_crlf_line_and_paragraph_ranges_never_split_the_line_ending() {
    let text = "He said «some words». Next.\r\nSecond line.\r\n\r\nLast.";
    let mut history = SelectionHistory::default();
    let mut selected = range(text, "words");
    for expected in [
        "some words",
        "«some words»",
        "He said «some words».",
        "He said «some words». Next.",
        "He said «some words». Next.\r\n",
        "He said «some words». Next.\r\nSecond line.",
        "He said «some words». Next.\r\nSecond line.\r\n",
        text,
    ] {
        selected = history.extend(text, "text", selected);
        assert_eq!(&text[selected.clone()], expected);
    }
}
