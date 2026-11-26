//! These tests need FileTab's private gutter/source lifecycle, not test-only exports.
use super::*;
use gpui::{
    ScrollDelta, ScrollWheelEvent, TestAppContext,
    TouchPhase::{self, *},
    VisualTestContext, point, size,
};

struct Fixture<'a> {
    _dir: tempfile::TempDir,
    file: Entity<FileTab>,
    editor: Entity<EditorState>,
    cx: &'a mut VisualTestContext,
}
fn mount<'a>(cx: &'a mut TestAppContext, name: &str) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join(name),
        format!("{}\n", "abcdef ".repeat(90)).repeat(200),
    )
    .unwrap();
    let mut file = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let inherited = cx.global::<crate::actions::Inherited>().0.clone();
        cx.clear_key_bindings();
        cx.bind_keys(crate::actions::ordered(
            vec![("cmd-f".into(), crate::actions::keymap::Named::Find)],
            &inherited,
        ));
        let entity = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new(name),
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
    let editor = cx.update(|window, cx| {
        let editor = file.read(cx).editor.clone().unwrap();
        window.focus(&editor.read(cx).focus_handle(cx), cx);
        editor
    });
    Fixture {
        _dir: dir,
        file,
        editor,
        cx,
    }
}
impl Fixture<'_> {
    fn offset(&mut self) -> gpui::Point<gpui::Pixels> {
        self.cx.update(|_, cx| self.editor.read(cx).scroll_offset())
    }
    fn wheel(
        &mut self,
        gutter: bool,
        phase: TouchPhase,
        delta: ScrollDelta,
        modifiers: gpui::Modifiers,
    ) {
        let mut position = self
            .cx
            .update(|_, cx| self.editor.read(cx).input_bounds().center());
        if gutter {
            self.cx.update(|window, cx| window.simulate_next_frame(cx));
            self.cx.run_until_parked();
            position.x = self
                .cx
                .debug_bounds("editor-line-number")
                .unwrap()
                .center()
                .x;
        }
        self.cx.simulate_event(ScrollWheelEvent {
            position,
            delta,
            touch_phase: phase,
            modifiers,
        });
        self.cx.run_until_parked();
    }
    fn gesture(&mut self, gutter: bool, phase: TouchPhase, x: f32, y: f32) {
        self.wheel(
            gutter,
            phase,
            ScrollDelta::Pixels(point(px(x), px(y))),
            Default::default(),
        );
    }
}

#[gpui::test]
fn file_scroll_axis_survives_drift_and_momentum(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    for gutter in [false, true] {
        let before = f.offset();
        let selection = f.cx.update(|_, cx| f.editor.read(cx).selected_range());
        for (phase, x, y) in [
            (Started, -3., -30.),
            (Moved, -90., -10.),
            (Ended, -60., -2.),
            (Moved, -70., -5.),
        ] {
            f.gesture(gutter, phase, x, y);
            assert_eq!(
                f.offset().x,
                before.x,
                "vertical gesture must not drift sideways, including momentum"
            );
        }
        assert_eq!(
            f.offset().y,
            before.y - px(47.),
            "deliver each filtered event once"
        );
        let before = f.offset();
        for (phase, x, y) in [
            (Started, -30., -3.),
            (Moved, -10., -90.),
            (Ended, -2., -60.),
            (Moved, -5., -70.),
        ] {
            f.gesture(gutter, phase, x, y);
            assert_eq!(
                f.offset().y,
                before.y,
                "next horizontal gesture must not drift vertically"
            );
        }
        assert_eq!(f.offset().x, before.x - px(47.));
        f.cx.update(|window, cx| {
            assert_eq!(f.editor.read(cx).selected_range(), selection);
            assert!(f.editor.read(cx).focus_handle(cx).is_focused(window));
        });
    }
}

#[gpui::test]
fn file_scroll_axis_zero_tie_edges_and_wheels(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    f.gesture(false, Started, 0., 0.);
    f.gesture(false, Moved, 0., 0.);
    f.gesture(false, Moved, -20., -20.);
    assert_eq!(f.offset(), point(px(0.), px(-20.)));
    f.gesture(false, Moved, -100., 10000.);
    assert_eq!(
        f.offset(),
        point(px(0.), px(0.)),
        "edge clamps without switching axis"
    );
    f.gesture(false, Ended, 0., 0.);
    f.gesture(false, Moved, -100., 10.);
    assert_eq!(
        f.offset(),
        point(px(0.), px(0.)),
        "momentum at edge cannot escape sideways"
    );
    f.gesture(false, Started, -30., -1.);
    assert_eq!(f.offset(), point(px(-30.), px(0.)));
    // A discrete wheel must ignore a stale horizontal trackpad lock.
    f.wheel(
        false,
        Moved,
        ScrollDelta::Lines(point(0., -2.)),
        Default::default(),
    );
    let before = f.offset();
    assert!(before.y < px(0.));
    assert_eq!(before.x, px(-30.));
    f.wheel(
        false,
        Moved,
        ScrollDelta::Lines(point(-2., 0.)),
        gpui::Modifiers {
            shift: true,
            ..Default::default()
        },
    );
    assert_eq!(f.offset().y, before.y);
    assert!(f.offset().x < before.x);
    // Precise input arriving without Started has no reliable contact boundary.
    let before = f.offset();
    f.gesture(false, Moved, -5., -7.);
    assert_eq!(f.offset(), before + point(px(-5.), px(-7.)));
    f.gesture(false, Started, 0., 0.);
    f.gesture(false, Moved, -20., -1.);
    assert_eq!(
        f.offset().y,
        before.y - px(7.),
        "zero start must allow horizontal choice too"
    );
}

#[gpui::test]
fn readme_source_scroll_axis_and_editing_remain_independent(cx: &mut TestAppContext) {
    let mut f = mount(cx, "README.md");
    f.gesture(false, Started, -3., -30.);
    f.gesture(false, Moved, -100., -10.);
    f.gesture(false, Ended, 0., 0.);
    f.gesture(false, Moved, -100., -5.);
    assert_eq!(f.offset(), point(px(0.), px(-45.)));
    // Programmatic reveal, selection and keyboard editing do not use the lock.
    f.cx.simulate_keystrokes("cmd-home");
    f.cx.simulate_input("Hello ");
    f.cx.update(|_, cx| {
        assert!(f.editor.read(cx).text().to_string().starts_with("Hello "));
        assert!(f.file.read(cx).is_dirty());
    });
    f.cx.simulate_keystrokes("cmd-z");
    f.cx.update(|_, cx| assert!(!f.editor.read(cx).text().to_string().starts_with("Hello ")));
    f.cx.simulate_keystrokes("cmd-a cmd-c");
    f.cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().unwrap(),
            f.editor.read(cx).text().to_string()
        )
    });
}

#[gpui::test]
fn separate_and_reopened_file_scroll_axis_does_not_inherit(cx: &mut TestAppContext) {
    {
        let mut f = mount(cx, "file.txt");
        f.gesture(false, Started, -1., -30.);
        f.gesture(false, Ended, 0., 0.);
        // Reopen the same on-disk file with a fresh FileTab, in the same window.
        let reopened = f.cx.update(|window, cx| {
            let file = cx.new(|cx| {
                FileTab::open(
                    f._dir.path(),
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
            window.replace_root(cx, |window, cx| {
                gpui_component::Root::new(file.clone(), window, cx)
            });
            file
        });
        f.cx.run_until_parked();
        f.editor =
            f.cx.update(|_, cx| reopened.read(cx).editor.clone().unwrap());
        f.file = reopened;
        f.cx.run_until_parked();
        // Orphan momentum has no old file lock; unlike Started this would
        // expose accidentally shared gesture state.
        f.gesture(false, Moved, -8., -3.);
        assert_eq!(f.offset(), point(px(-8.), px(-3.)));
        f.gesture(false, Started, -30., -1.);
        assert_eq!(f.offset(), point(px(-38.), px(-3.)));
    }
    let mut f = mount(cx, "file.txt");
    f.gesture(false, Moved, -4., -7.);
    assert_eq!(f.offset(), point(px(-4.), px(-7.)));
    f.cx.update(|_, cx| {
        f.editor.update(cx, |editor, cx| {
            editor.set_scroll_offset(point(px(0.), px(0.)), cx)
        })
    });
    f.gesture(false, Started, -30., -1.);
    assert_eq!(f.offset(), point(px(-30.), px(0.)));
    f.cx.update(|_, cx| f.file.read(cx).hide_source());
    f.gesture(false, Moved, -2., -4.);
    assert_eq!(
        f.offset(),
        point(px(-32.), px(-4.)),
        "hidden source must relinquish its gesture"
    );
}

#[gpui::test]
fn file_scroll_axis_leaves_find_input_and_focus_available(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    f.gesture(false, Started, -30., -1.);
    f.gesture(false, Ended, 0., 0.);
    f.cx.simulate_keystrokes("cmd-f");
    f.cx.run_until_parked();
    let query =
        f.cx.debug_bounds("local-query-field")
            .expect("Guild Find query is drawn");
    let (position, before) = f.cx.update(|_, cx| {
        assert!(f.file.read(cx).local_search.as_ref().unwrap().read(cx).open);
        assert!(
            f.editor.read(cx).input_bounds().top() >= query.bottom(),
            "Find is above the source viewport"
        );
        (query.center(), f.editor.read(cx).scroll_offset())
    });
    f.cx.simulate_event(ScrollWheelEvent {
        position,
        delta: ScrollDelta::Pixels(point(px(-50.), px(-3.))),
        touch_phase: Started,
        ..Default::default()
    });
    f.cx.run_until_parked();
    assert_eq!(f.offset(), before, "Find owns its wheel event");
    f.cx.simulate_input("abcdef");
    f.cx.update(|_, cx| {
        assert!(
            !f.file.read(cx).is_dirty(),
            "Find input must not edit the file"
        )
    });
    f.cx.simulate_keystrokes("escape");
    f.cx.update(|window, cx| assert!(f.editor.read(cx).focus_handle(cx).is_focused(window)));
    let before = f.offset();
    f.gesture(false, Started, -2., -20.);
    assert_eq!(f.offset().x, before.x);
    assert!(f.offset().y < before.y);
}

#[gpui::test]
fn file_scroll_axis_does_not_constrain_scrollbar_drag(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    f.gesture(false, Started, -30., -1.);
    f.gesture(false, Ended, 0., 0.);
    let before = f.offset();
    let bounds = f.cx.update(|_, cx| f.editor.read(cx).input_bounds());
    let start = point(bounds.right() - px(4.), bounds.top() + px(8.));
    let end = point(start.x, start.y + px(120.));
    f.cx.simulate_event(gpui::MouseMoveEvent {
        position: start,
        ..Default::default()
    });
    f.cx.update(|window, cx| window.simulate_next_frame(cx));
    f.cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    f.cx.simulate_mouse_move(end, gpui::MouseButton::Left, Default::default());
    f.cx.simulate_mouse_up(end, gpui::MouseButton::Left, Default::default());
    f.cx.run_until_parked();
    assert!(
        f.offset().y < before.y - px(100.),
        "vertical scrollbar drag must work after a horizontal gesture"
    );
    assert_eq!(f.offset().x, before.x);
}

#[gpui::test]
fn file_scroll_axis_leaves_completion_scroll_and_selection_available(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    f.cx.update(|_, cx| {
        f.editor.update(cx, |e, cx| {
            e.present_completion_items(
                0,
                "",
                (0..100)
                    .map(|i| lsp_types::CompletionItem {
                        label: format!("choice{i:03}"),
                        ..Default::default()
                    })
                    .collect(),
                cx,
            )
        })
    });
    f.cx.run_until_parked();
    let position = f.cx.update(|_, cx| {
        let state = f.editor.read(cx);
        assert!(state.completion_menu_state().open);
        let (cursor, height) = state.cursor_layout().unwrap();
        point(cursor.left() + px(60.), cursor.top() + height + px(30.))
    });
    let before = f.offset();
    for (phase, x, y) in [
        (Started, -2., -100.),
        (Moved, -80., -20.),
        (Ended, 0., 0.),
        (Moved, -30., -5.),
    ] {
        f.cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase: phase,
            ..Default::default()
        });
        f.cx.run_until_parked();
        assert_eq!(
            f.offset(),
            before,
            "completion owns its scroll instead of the file"
        );
    }
    f.cx.simulate_click(position, Default::default());
    f.cx.update(|_, cx| {
        let text = f.editor.read(cx).text().to_string();
        assert!(
            text.starts_with("choice"),
            "click a real completion row: {text:.30}"
        );
        assert!(
            !text.starts_with("choice000"),
            "scroll exposes a later completion row"
        );
        assert!(!f.editor.read(cx).completion_menu_state().open);
    });
}

#[gpui::test]
fn file_scroll_axis_leaves_hover_wheel_owned(cx: &mut TestAppContext) {
    let mut f = mount(cx, "file.txt");
    f.cx.update(|_, cx| {
        f.editor.update(cx, |e, cx| {
            e.present_hover(
                0..6,
                lsp_types::Hover {
                    contents: lsp_types::HoverContents::Markup(lsp_types::MarkupContent {
                        kind: lsp_types::MarkupKind::Markdown,
                        value: (0..60)
                            .map(|i| format!("Hover paragraph {i:03}.\n\n"))
                            .collect(),
                    }),
                    range: None,
                },
                cx,
            )
        })
    });
    f.cx.run_until_parked();
    let position = f.cx.update(|_, cx| {
        let state = f.editor.read(cx);
        assert!(state.hover_popover().is_some());
        let trigger = state.range_to_bounds(&(0..6)).unwrap();
        point(trigger.left() + px(55.), trigger.bottom() + px(12.))
    });
    let before = f.offset();
    for (phase, x, y) in [
        (Started, -2., -90.),
        (Moved, -50., -20.),
        (Ended, 0., 0.),
        (Moved, -20., -5.),
    ] {
        f.cx.simulate_event(ScrollWheelEvent {
            position,
            delta: ScrollDelta::Pixels(point(px(x), px(y))),
            touch_phase: phase,
            ..Default::default()
        });
        f.cx.run_until_parked();
        assert_eq!(
            f.offset(),
            before,
            "hover owns its scroll instead of the file"
        );
    }
    f.cx.update(|_, cx| {
        assert!(
            f.editor.read(cx).hover_popover().is_some(),
            "scrolling inside hover must retain the popover"
        )
    });
}
