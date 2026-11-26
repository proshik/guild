//! The upstream editor must clip viewport-straddling diagnostics before shaping UTF-8.
use gpui::{AppContext, Entity, Render, Styled, TestAppContext, px};
use gpui_component::input::{Editor, EditorState, Position};

#[gpui::test]
fn diagnostics_outside_and_across_the_viewport_do_not_shift_unicode_runs(cx: &mut TestAppContext) {
    let text = (0..300)
        .map(|i| format!("let имя{i} = \"строка\";\n"))
        .collect::<String>();
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .language("rust")
                .soft_wrap(false)
                .default_value(text.clone())
        });
        state.update(cx, |state, cx| {
            let text = state.text().clone();
            let diagnostics = state.diagnostics_mut().unwrap();
            diagnostics.reset(&text);
            for (start, end) in [(0, 1), (0, 155), (145, 299), (290, 299)] {
                diagnostics.push(lsp_types::Diagnostic::new_simple(
                    lsp_types::Range::new(Position::new(start, 0), Position::new(end, 2)),
                    "viewport boundary".into(),
                ));
            }
            cx.notify();
        });
        editor = Some(state.clone());
        let view = cx.new(|_| Source(state));
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(400.)));
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(150, 0), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert!(
            state.visible_row_range().unwrap().start > 0,
            "exercise clipping above the viewport"
        );
        assert_eq!(state.value().as_ref(), text);
    });
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), text));
}

struct Source(Entity<EditorState>);

#[gpui::test]
fn public_geometry_tracks_wrapped_unicode_and_scrolling(cx: &mut TestAppContext) {
    use gpui_component::RopeExt as _;
    let text = format!(
        "{}\nцель:\n{}",
        "длинная строка ".repeat(80),
        "после\n".repeat(200)
    );
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            EditorState::new(window, cx)
                .soft_wrap(true)
                .default_value(text.clone())
        });
        assert!(state.read(cx).range_to_bounds(&(0..0)).is_none());
        editor = Some(state.clone());
        let view = cx.new(|_| Source(state));
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(400.), px(300.)));
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(1, 0), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let offset = state.text().line_start_offset(1);
        let row = state.range_to_bounds(&(offset..offset)).unwrap();
        assert!(row.top() >= state.input_bounds().top());
        assert!(row.bottom() <= state.input_bounds().bottom());
        assert_eq!(row.size.height, state.line_height().unwrap());
    });
    cx.simulate_resize(gpui::size(px(700.), px(400.)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let offset = state.text().line_start_offset(1);
        assert!(state.range_to_bounds(&(offset..offset)).is_some());
        assert_eq!(state.value().as_ref(), text);
    });
}
impl Render for Source {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        Editor::new(&self.0).size_full()
    }
}

/// Reproduce the former two-byte definition underline above Cyrillic text.
#[gpui::test]
fn scrolling_past_a_definition_hover_keeps_unicode_text_intact(cx: &mut TestAppContext) {
    use gpui::{App, Task, Window};
    use gpui_component::input::{DefinitionProvider, Rope};
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    struct Definition(Arc<AtomicUsize>);
    impl DefinitionProvider for Definition {
        fn definitions(
            &self,
            _: &Rope,
            _: usize,
            _: &mut Window,
            _: &mut App,
        ) -> Task<anyhow::Result<Vec<lsp_types::LocationLink>>> {
            self.0.fetch_add(1, Ordering::SeqCst);
            let range = lsp_types::Range::new(Position::new(0, 3), Position::new(0, 5));
            Task::ready(Ok(vec![lsp_types::LocationLink {
                origin_selection_range: Some(range),
                target_uri: "file:///test.rs".parse().unwrap(),
                target_range: range,
                target_selection_range: range,
            }]))
        }
    }
    let asked = Arc::new(AtomicUsize::new(0));
    let text = format!("fn ab() {{}}\n{}", "let имя = \"строка\";\n".repeat(300));
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .language("rust")
                .soft_wrap(false)
                .default_value(text.clone());
            state.lsp_mut().definition_provider = Some(std::rc::Rc::new(Definition(asked.clone())));
            state
        });
        editor = Some(state.clone());
        let view = cx.new(|_| Source(state));
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(400.)));
    let editor = editor.unwrap();
    let name = cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.focus(window, cx);
            state.range_to_bounds(&(3..5)).unwrap().center()
        })
    });
    cx.simulate_event(gpui::MouseMoveEvent {
        position: name,
        modifiers: gpui::Modifiers {
            platform: true,
            ..Default::default()
        },
        ..Default::default()
    });
    cx.run_until_parked();
    assert!(
        asked.load(Ordering::SeqCst) > 0,
        "the test must establish an actual definition hover"
    );
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(Position::new(150, 0), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(editor.read(cx).visible_row_range().unwrap().start > 1));
    cx.simulate_keystrokes("cmd-a cmd-c");
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), text));
}
