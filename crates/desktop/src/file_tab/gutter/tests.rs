use super::{painted_numbers, visible_rows};
use crate::file_tab::{FileTab, Look};
use crate::i18n::Ui;
use gpui::{
    AppContext, Modifiers, MouseButton, MouseDownEvent, PlatformInput, ScrollDelta,
    ScrollWheelEvent, TestAppContext, point, px, size,
};

#[gpui::test]
fn numbers_are_painted_on_the_first_scrolling_frame(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    // A wide line also makes horizontal scrolling possible.
    std::fs::write(
        dir.path().join("file.txt"),
        format!("{}\n", "abcdef ".repeat(90)).repeat(200),
    )
    .unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let entity = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("file.txt"),
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
    cx.simulate_resize(size(px(600.), px(300.)));
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let editor = cx.update(|_, cx| file.read(cx).editor.clone().unwrap());
    for delta in [
        point(px(0.), px(-34.)),
        point(px(0.), px(-51.)),
        point(px(-40.), px(0.)),
        point(px(0.), px(34.)),
        point(px(40.), px(0.)),
    ] {
        let position = cx.update(|_, cx| editor.read(cx).input_bounds().center());
        cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(delta),
            ..Default::default()
        });
        // Inspect the first painted frame, before an animation retry can hide
        // the failure by restoring numbers on a later stationary frame.
        cx.update(|window, cx| {
            window.simulate_next_frame(cx);
            let state = editor.read(cx);
            let expected = visible_rows(state);
            assert!(
                !expected.is_empty(),
                "the retained editor must have visible text"
            );
            assert_eq!(
                painted_numbers(),
                expected,
                "every current logical row must have a number on this scrolling frame"
            );
        });
    }
}

#[gpui::test]
fn old_number_handler_rejects_scroll_before_replacement_frame(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("file.txt"), "one\ntwo\nthree\n".repeat(100)).unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let entity = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                std::path::Path::new("file.txt"),
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
    cx.run_until_parked();
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
    let number = cx.debug_bounds("editor-line-number").unwrap();
    let position = cx.update(|_, cx| {
        let state = file.read(cx).editor.as_ref().unwrap().read(cx);
        point(
            number.center().x,
            state.range_to_bounds(&(0..0)).unwrap().top() + state.line_height().unwrap() / 2.,
        )
    });
    cx.update(|window, cx| {
        let editor = file.read(cx).editor.clone().unwrap();
        let before = editor.read(cx).selected_range();
        let scroll = editor.read(cx).scroll_offset();
        let text_position = editor.read(cx).input_bounds().center();
        window.dispatch_event(
            PlatformInput::ScrollWheel(ScrollWheelEvent {
                position: text_position,
                delta: ScrollDelta::Pixels(point(px(0.), px(-100.))),
                ..Default::default()
            }),
            cx,
        );
        assert_ne!(
            editor.read(cx).scroll_offset(),
            scroll,
            "exercise applied wheel scrolling, not a deferred future request"
        );
        window.dispatch_event(
            PlatformInput::MouseDown(MouseDownEvent {
                position,
                button: MouseButton::Left,
                modifiers: Modifiers::none(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        assert_eq!(
            editor.read(cx).selected_range(),
            before,
            "an old number handler must not select a row using the previous scroll geometry"
        );
    });
}
