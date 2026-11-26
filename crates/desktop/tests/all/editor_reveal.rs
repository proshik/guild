//! Jump centering through the upstream editor's public layout and scroll APIs.
#[gpui::test]
fn a_jump_before_first_layout_is_centred(cx: &mut gpui::TestAppContext) {
    use gpui::{AppContext, Styled, px};
    use gpui_component::input::{Editor, EditorState, Position};
    use guild_desktop::editor::EditorReveal;
    let mut editor = None;
    let window = cx.open_window(gpui::size(px(600.), px(400.)), |window, cx| {
        guild_desktop::editor::init(cx);
        let state = cx.new(|cx| {
            let mut state = EditorState::new(window, cx)
                .soft_wrap(false)
                .default_value((0..300).map(|i| format!("line {i}\n")).collect::<String>());
            state.set_cursor_position_centred(Position::new(150, 0), window, cx);
            state
        });
        editor = Some(state.clone());
        let view = cx.new(|_| JumpView(state));
        gpui_component::Root::new(view, window, cx)
    });
    let cx = gpui::VisualTestContext::from_window(window.into(), cx).into_mut();
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
    }
    cx.run_until_parked();
    let editor = editor.unwrap();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(state.cursor_position(), Position::new(150, 0));
        let (cursor, line) = state.cursor_layout().unwrap();
        let viewport = state.input_bounds();
        assert!(
            (cursor.center().y + state.scroll_offset().y - viewport.center().y).abs() <= line,
            "cursor {cursor:?}, viewport {viewport:?}, scroll {:?}",
            state.scroll_offset()
        );
    });
    let before = cx.update(|_, cx| editor.read(cx).scroll_offset());
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position_centred(Position::new(151, 0), window, cx);
        })
    });
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
    }
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).scroll_offset().y,
            before.y,
            "a visible row does not move the viewport"
        )
    });

    // The upstream scroll API clamps jumps near either edge of the document.
    for row in [299, 0] {
        cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                state.set_cursor_position_centred(Position::new(row, 0), window, cx);
            })
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                window.simulate_next_frame(cx);
            });
        }
        cx.update(|_, cx| {
            let state = editor.read(cx);
            let at = state.cursor();
            let bounds = state.range_to_bounds(&(at..at)).unwrap();
            assert!(
                state.input_bounds().contains(&bounds.center()),
                "edge jump {row}: {bounds:?}"
            );
            if row == 0 {
                assert_eq!(state.scroll_offset().y, px(0.));
            }
        });
    }
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position_centred(Position::new(200, 0), window, cx);
            state.set_cursor_position(Position::new(10, 0), window, cx);
        })
    });
    for _ in 0..3 {
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
        });
    }
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(
            state.cursor_position(),
            Position::new(10, 0),
            "an obsolete reveal must not restore its caret"
        );
        let at = state.cursor();
        assert!(
            state
                .input_bounds()
                .contains(&state.range_to_bounds(&(at..at)).unwrap().center())
        );
    });

    struct JumpView(gpui::Entity<EditorState>);
    impl gpui::Render for JumpView {
        fn render(
            &mut self,
            _: &mut gpui::Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            Editor::new(&self.0).size_full()
        }
    }
}
