use super::*;
use gpui::{AppContext, TestAppContext};
#[gpui::test]
fn inline_toggle_keeps_editor_and_rejects_old_generations(cx: &mut TestAppContext) {
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
    cx.simulate_resize(gpui::size(px(700.), px(400.)));
    let runs = vec![Run {
        sha: "a".repeat(40),
        author: "Иван Петров".into(),
        time: 1788383090,
        summary: "original".into(),
        first_line: 1,
        original_line: 20,
        filename: "old.txt".into(),
        lines: vec!["one".into(), "two".into()],
    }];
    let (editor, selection, scroll, generation) = cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            let editor = file.editor.clone().unwrap();
            editor.update(cx, |e, cx| {
                e.set_cursor_position(gpui_component::input::Position::new(1, 1), window, cx)
            });
            let selection = editor.read(cx).selected_range();
            let scroll = editor.read(cx).scroll_offset();
            file.toggle_blame(cx);
            let generation = file.blame.generation;
            file.blame_landed(generation, Path::new("file.txt"), Ok(runs.clone()), cx);
            (editor, selection, scroll, generation)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.simulate_next_frame(cx);
    });
    cx.run_until_parked();
    let cell = cx.debug_bounds("inline-blame-cell").unwrap();
    cx.update(|window, _| {
        let label: SharedString = "03.09.2026 Петров".into();
        let font = window.text_style().font();
        let shaped = window.text_system().shape_line(
            label.clone(),
            theme::size::META.to_pixels(window.rem_size()),
            &[gpui::TextRun {
                len: label.len(),
                font,
                color: gpui::black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        assert!(
            cell.size.width
                <= shaped.width.max(px(140.))
                    + theme::space::CARD_GAP.to_pixels(window.rem_size()) * 2.,
            "short attribution must not reserve the old fixed column: {:?}",
            cell.size
        );
    });
    cx.update(|window, cx| {
        file.update(cx, |file, cx| {
            assert_eq!(file.editor.as_ref(), Some(&editor));
            assert_eq!(editor.read(cx).selected_range(), selection);
            assert_eq!(editor.read(cx).scroll_offset(), scroll);
            assert_eq!(file.blame.lines[1].as_ref().unwrap().original_line, 21);
            editor.update(cx, |e, cx| {
                e.set_cursor_position(gpui_component::input::Position::new(0, 0), window, cx)
            });
        })
    });
    cx.simulate_input("added\n");
    cx.run_until_parked();
    cx.update(|_, cx| {
        file.update(cx, |file, cx| {
            assert!(file.blame.lines[0].is_none());
            assert_eq!(file.blame.lines[1].as_ref().unwrap().original_line, 20);
            file.toggle_blame(cx);
            file.toggle_blame(cx);
            file.blame_landed(generation, Path::new("file.txt"), Ok(runs), cx);
            assert!(
                file.blame.lines.is_empty(),
                "a toggled-off request cannot land after re-enabling"
            );
            assert_eq!(file.editor.as_ref(), Some(&editor));
        })
    });
    cx.simulate_keystrokes("cmd-z");
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).value().as_ref(),
            "addedone\ntwo\n",
            "first undo removes the newline transaction"
        )
    });
    cx.simulate_keystrokes("cmd-z");
    cx.run_until_parked();
    cx.update(|_, cx| {
        file.update(cx, |file, cx| {
            assert_eq!(
                editor.read(cx).value().as_ref(),
                "one\ntwo\n",
                "toggle preserves real editor undo history"
            );
            file.read_only = true;
            file.toggle_blame(cx);
            assert!(!file.blame_enabled(), "external dependency refusal");
        })
    });
}
