//! Real Root gestures, isolated from user settings, projects and providers.
use crate::Root;
use gpui::{AppContext, Focusable, TestAppContext, px};
use guild_session::action_bar::{self, Item::*};

#[test]
fn isolated_action_bar_gestures() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::settings::action_bar_tests::real_",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("GUILD_ACTION_BAR_TEST", "1")
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_drag_cancel_keyboard_reset_and_titlebar(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            root.settings_page = Some(super::Section::Appearance);
            root.settings_query = "Action Bar".into();
            root
        });
        root.read(cx).focus.clone().focus(window, cx);
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(850.)));
    settle(cx);
    root.update(cx, |root, cx| {
        root.settings_query = "Action Bar".into();
        cx.notify();
    });
    settle(cx);
    let order = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |root, _| {
            action_bar::visible(
                &root.settings.action_bar_order,
                &root.settings.action_bar_excluded,
            )
        })
    };
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    let source = cx
        .debug_bounds("action-bar-handle-terminal_jobs")
        .expect("search draws Action Bar rows")
        .center();
    let usage_row = cx.debug_bounds("action-bar-row-usage").unwrap();
    let open_row = cx.debug_bounds("action-bar-row-open_in").unwrap();
    let jobs_row = cx.debug_bounds("action-bar-row-terminal_jobs").unwrap();
    assert_eq!(
        usage_row.top(),
        open_row.top(),
        "preview must be horizontal"
    );
    assert_eq!(
        usage_row.top(),
        jobs_row.top(),
        "preview must remain one row"
    );
    assert!(usage_row.right() <= open_row.left() && open_row.right() <= jobs_row.left());
    let sample = cx.debug_bounds("action-bar-sample-usage").unwrap();
    let padding = root.read_with(cx, |root, _| px(root.rem_px(crate::theme::space::CARD_GAP)));
    assert!(
        sample.top() - usage_row.top() <= padding + px(1.),
        "one compact padding layer"
    );
    let remove = cx.debug_bounds("action-bar-remove-usage").unwrap();
    let glyph = cx.debug_bounds("action-bar-remove-glyph-usage").unwrap();
    let minimum = root.read_with(cx, |root, _| px(root.rem_px(crate::theme::size::META) / 2.));
    assert!(
        glyph.size.width >= minimum
            && glyph.left() >= remove.left()
            && glyph.right() <= remove.right(),
        "removal glyph must be readable inside its target: {glyph:?} in {remove:?}"
    );
    let target = gpui::point(usage_row.center().x, source.y);
    let focus = cx.update(|window, cx| window.focused(cx));
    cx.simulate_mouse_move(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    assert_eq!(cx.update(|window, cx| window.focused(cx)), focus);
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    // Release outside every valid row must not commit the preview.
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    let outside = gpui::point(px(1190.), px(800.));
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_mouse_down(source, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, Usage, OpenIn]);
    let jobs = cx.debug_bounds("terminal-jobs-trigger").unwrap();
    let usage = cx.debug_bounds("titlebar-usage").unwrap();
    let open = cx.debug_bounds("titlebar-open-in").unwrap();
    assert!(jobs.left() < usage.left() && usage.left() < open.left());
    root.read_with(cx, |root, _| {
        assert!(usage.contains(&root.usage_at.get().center()));
        assert!(open.contains(&root.open_in_at.get().unwrap().center()));
    });
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, Usage, OpenIn]);
    guild_session::store::flush();
    let saved =
        guild_session::Store::<guild_session::Settings>::new(dir.path().join("settings.json"))
            .load()
            .unwrap();
    assert_eq!(
        action_bar::visible(&saved.action_bar_order, &saved.action_bar_excluded),
        [TerminalJobs, Usage, OpenIn]
    );
    let reset = cx.debug_bounds("action-bar-reset").unwrap().center();
    cx.simulate_click(reset, Default::default());
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    // Preview samples configure order only; a click must not open the real controls.
    for selector in [
        "action-bar-sample-usage",
        "action-bar-sample-open_in",
        "action-bar-sample-terminal_jobs",
    ] {
        let sample = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(sample, Default::default());
        settle(cx);
        root.read_with(cx, |root, _| {
            assert!(!root.showing_usage);
            assert!(root.context_menu.is_none());
            assert!(root.terminal_list.is_none());
        });
    }
    // The padded card itself owns the full drag target, including its edge.
    let source = cx.debug_bounds("action-bar-row-terminal_jobs").unwrap();
    let edge = gpui::point(source.left() + px(3.), source.top() + px(3.));
    let target = cx.debug_bounds("action-bar-row-usage").unwrap().center();
    cx.simulate_mouse_down(edge, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert!(cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, Usage, OpenIn]);
    // Press/move/release on the nested removal control cannot start a card drag.
    let remove = cx.debug_bounds("action-bar-remove-usage").unwrap().center();
    cx.simulate_mouse_down(remove, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert!(!cx.update(|_, cx| cx.has_active_drag()));
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, Default::default());
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, Usage, OpenIn]);
    cx.simulate_click(remove, Default::default());
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, OpenIn]);
    assert!(cx.debug_bounds("titlebar-usage").is_none());
    let focused = root.read_with(cx, |root, _| {
        root.action_bar_scroll.items.borrow()["open_in"].clone()
    });
    assert!(cx.update(|window, _| focused.is_focused(window)));
    cx.simulate_keystrokes("alt-right");
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs, OpenIn], "subset boundary no-op");
    cx.simulate_keystrokes("alt-left");
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs]);
    // The menu contains only excluded items; selecting one appends and focuses it.
    let add = cx.debug_bounds("action-bar-add").unwrap().center();
    cx.simulate_click(add, Default::default());
    settle(cx);
    assert!(cx.debug_bounds("menu-item-1").is_none());
    cx.simulate_keystrokes("escape");
    settle(cx);
    assert!(cx.debug_bounds("menu-item-0").is_none());
    assert_eq!(order(cx), [OpenIn, TerminalJobs]);
    let add = cx.debug_bounds("action-bar-add").unwrap().center();
    cx.simulate_click(add, Default::default());
    settle(cx);
    let add_usage = cx.debug_bounds("menu-item-0").unwrap().center();
    cx.simulate_click(add_usage, Default::default());
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs, Usage]);
    let focus = root.read_with(cx, |root, _| {
        root.action_bar_scroll.items.borrow()["usage"].clone()
    });
    assert_item_visible(cx, "action-bar-row-usage", &focus);
    for selector in [
        "action-bar-remove-usage",
        "action-bar-remove-open_in",
        "action-bar-remove-terminal_jobs",
    ] {
        let remove = cx.debug_bounds(selector).unwrap().center();
        cx.simulate_click(remove, Default::default());
        settle(cx);
    }
    assert!(order(cx).is_empty());
    assert!(cx.debug_bounds("action-bar-empty").is_some());
    for selector in [
        "titlebar-usage",
        "titlebar-open-in",
        "terminal-jobs-trigger",
    ] {
        assert!(cx.debug_bounds(selector).is_none());
    }
    assert!(
        cx.update(|window, cx| window.focused(cx).is_some()),
        "last removal leaves reachable Add focus"
    );
    guild_session::store::flush();
    let stored =
        guild_session::Store::<guild_session::Settings>::new(dir.path().join("settings.json"))
            .load()
            .unwrap();
    assert!(action_bar::visible(&stored.action_bar_order, &stored.action_bar_excluded).is_empty());
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    settle(cx);
    // Choices reuse the real action marks, including the last cached Usage harness.
    root.read_with(cx, |root, _| {
        use crate::view::menu::{Entry, Mark};
        for front in [
            crate::agents::Agent::ClaudeCode,
            crate::agents::Agent::Codex,
            crate::agents::Agent::OpenCode,
        ] {
            root.usage_last.set(front);
            let choices = root.action_bar_choices();
            let marks = choices
                .iter()
                .map(|entry| match entry {
                    Entry::Choice(choice) => choice.mark.clone(),
                    _ => panic!("Add contains choices only"),
                })
                .collect::<Vec<_>>();
            assert_eq!(
                marks,
                [
                    Mark::Harness(front.mark()),
                    Mark::Icon("ui/title-open-in.svg"),
                    Mark::Icon("ui/workspace-terminal.svg")
                ],
                "Add identifies each real action"
            );
        }
    });
    let add_jobs = cx.debug_bounds("menu-item-2").unwrap().center();
    cx.simulate_click(add_jobs, Default::default());
    settle(cx);
    assert_eq!(order(cx), [TerminalJobs]);
    assert!(cx.debug_bounds("terminal-jobs-trigger").is_some());
    let reset = cx.debug_bounds("action-bar-reset").unwrap().center();
    cx.simulate_click(reset, Default::default());
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    root.read_with(cx, |root, _| {
        assert!(root.settings.action_bar_order.is_empty());
        assert!(root.settings.action_bar_excluded.is_empty());
        assert!(!root.showing_usage && root.context_menu.is_none() && root.terminal_list.is_none());
    });
}

struct KeyboardHost {
    root: gpui::Entity<Root>,
    focus: gpui::FocusHandle,
    search: gpui::Entity<gpui_component::input::InputState>,
    _changed: gpui::Subscription,
}
impl gpui::Render for KeyboardHost {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use gpui::prelude::*;
        window.set_rem_size(crate::scale::rem_size(
            self.root.read(cx).settings.ui_font_size,
        ));
        gpui::div()
            .size_full()
            .flex()
            .flex_col()
            .key_context("guild")
            .track_focus(&self.focus)
            .on_key_down(|event, window, cx| {
                if crate::view::controls::tab_navigation(event, window, cx) {
                    cx.stop_propagation();
                }
            })
            .child(
                crate::text_field::TextField::new(
                    "action-bar-search-fixture",
                    "Action Bar",
                    |_, _, _| {},
                )
                .input(self.search.clone())
                .placeholder("Search settings"),
            )
            .child(
                self.root
                    .update(cx, |root, cx| root.render_action_bar_order(cx)),
            )
            .children(self.root.update(cx, |root, cx| {
                root.viewport = window.viewport_size();
                root.render_context_menu(cx)
            }))
    }
}
#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_keyboard_moves_use_the_saved_mutation(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.settings_page = Some(super::Section::Appearance);
            root
        });
        entity = Some(root.clone());
        let host = cx.new(|cx| KeyboardHost {
            _changed: cx.observe(&root, |_, _, cx| cx.notify()),
            root,
            focus: cx.focus_handle(),
            search: cx.new(|cx| gpui_component::input::InputState::new(window, cx)),
        });
        host.read(cx)
            .search
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        gpui_component::Root::new(host, window, cx)
    });
    cx.simulate_resize(gpui::size(px(300.), px(600.)));
    settle(cx);
    let root = entity.unwrap();
    let order = |cx: &mut gpui::VisualTestContext| {
        root.read_with(cx, |root, _| {
            action_bar::visible(
                &root.settings.action_bar_order,
                &root.settings.action_bar_excluded,
            )
        })
    };
    // The real shared search input owns these strokes, outside the preview.
    assert!(cx.update(|window, cx| crate::text_field::focused(window, cx)));
    let search_focus = cx.update(|window, cx| window.focused(cx));
    cx.simulate_keystrokes("alt-left alt-right");
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    assert_eq!(cx.update(|window, cx| window.focused(cx)), search_focus);
    cx.simulate_keystrokes("tab");
    settle(cx);
    let usage_focus = cx.update(|window, cx| window.focused(cx)).unwrap();
    let expected = root.read_with(cx, |root, _| {
        root.action_bar_scroll.items.borrow()["usage"].clone()
    });
    assert_eq!(usage_focus, expected, "Tab focuses the item itself");
    // One stable item handles both directions. A boundary is a true no-op.
    cx.simulate_keystrokes("alt-left");
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    cx.simulate_keystrokes("alt-right");
    settle(cx);
    assert_eq!(
        order(cx),
        [OpenIn, Usage, TerminalJobs],
        "Option+Right reorders the focused item"
    );
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    cx.simulate_keystrokes("alt-right");
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs, Usage]);
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    let explicit = root.read_with(cx, |root, _| root.settings.action_bar_order.clone());
    cx.simulate_keystrokes("alt-right");
    settle(cx);
    assert_eq!(
        root.read_with(cx, |root, _| root.settings.action_bar_order.clone()),
        explicit
    );
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    for _ in 0..2 {
        cx.simulate_keystrokes("alt-left");
        settle(cx);
        assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    }
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    cx.simulate_keystrokes("alt-left");
    settle(cx);
    assert_eq!(order(cx), [Usage, OpenIn, TerminalJobs]);
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    // Tab reaches the item and its accessible removal button, including clipped cards.
    let mut visited = vec![usage_focus.clone()];
    for selector in ["action-bar-row-open_in", "action-bar-row-terminal_jobs"] {
        cx.simulate_keystrokes("tab");
        settle(cx);
        let remove = cx
            .debug_bounds(if selector.ends_with("open_in") {
                "action-bar-remove-usage"
            } else {
                "action-bar-remove-open_in"
            })
            .unwrap();
        let viewport = cx.debug_bounds("action-bar-order-scroll").unwrap();
        assert!(viewport.contains(&remove.center()));
        cx.simulate_keystrokes("tab");
        settle(cx);
        let focus = cx.update(|window, cx| window.focused(cx)).unwrap();
        assert!(!visited.contains(&focus));
        let id = selector.strip_prefix("action-bar-row-").unwrap();
        assert_eq!(
            focus,
            root.read_with(cx, |root, _| root.action_bar_scroll.items.borrow()[id]
                .clone())
        );
        assert_item_visible(cx, selector, &focus);
        visited.push(focus);
    }
    for (selector, focus) in [
        ("action-bar-row-open_in", &visited[1]),
        ("action-bar-row-usage", &usage_focus),
    ] {
        cx.simulate_keystrokes("shift-tab shift-tab");
        settle(cx);
        assert_item_visible(cx, selector, focus);
    }
    assert!(
        cx.debug_bounds("action-bar-usage-right").is_none(),
        "no visible directional move buttons"
    );
    cx.simulate_keystrokes("alt-right alt-right");
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs, Usage]);
    cx.update(|window, cx| window.blur(cx));
    cx.simulate_resize(gpui::size(px(300.), px(500.)));
    settle(cx);
    let viewport = cx.debug_bounds("action-bar-order-scroll").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(900.), px(0.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    let viewport = cx.debug_bounds("action-bar-order-scroll").unwrap();
    let first = cx.debug_bounds("action-bar-row-open_in").unwrap();
    let middle = cx.debug_bounds("action-bar-row-terminal_jobs").unwrap();
    let last = cx.debug_bounds("action-bar-row-usage").unwrap();
    assert_eq!(first.top(), middle.top());
    assert_eq!(first.top(), last.top(), "narrow preview must not wrap");
    assert!(
        last.right() > viewport.right(),
        "items retain readable widths"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: viewport.center(),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-60.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    assert_eq!(
        cx.debug_bounds("action-bar-row-open_in").unwrap().left(),
        first.left(),
        "vertical page wheel must not scroll the horizontal preview"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(viewport.left() + px(20.), first.center().y),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(-900.), px(0.))),
        touch_phase: gpui::TouchPhase::Moved,
        ..Default::default()
    });
    settle(cx);
    let first_after = cx.debug_bounds("action-bar-row-open_in").unwrap();
    let last_after = cx.debug_bounds("action-bar-row-usage").unwrap();
    assert!(
        first_after.left() < first.left(),
        "horizontal wheel moves preview"
    );
    assert_eq!(first_after.top(), last_after.top());
    assert!(
        viewport.contains(&last_after.center()),
        "scroll exposes final item"
    );
    // The exposed item itself remains the keyboard target after manual scrolling.
    cx.update(|window, cx| usage_focus.focus(window, cx));
    cx.simulate_keystrokes("alt-left");
    settle(cx);
    assert_eq!(order(cx), [OpenIn, Usage, TerminalJobs]);
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    // Remove through the keyboard-accessible nested button, then add at the narrow end.
    cx.simulate_keystrokes("tab enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs]);
    let jobs_focus = root.read_with(cx, |root, _| {
        root.action_bar_scroll.items.borrow()["terminal_jobs"].clone()
    });
    assert_item_visible(cx, "action-bar-row-terminal_jobs", &jobs_focus);
    cx.simulate_keystrokes("alt-right");
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs]);
    let add = cx.debug_bounds("action-bar-add").unwrap().center();
    cx.simulate_click(add, Default::default());
    settle(cx);
    let usage = cx.debug_bounds("menu-item-0").unwrap().center();
    cx.simulate_click(usage, Default::default());
    settle(cx);
    assert_eq!(order(cx), [OpenIn, TerminalJobs, Usage]);
    assert_item_visible(cx, "action-bar-row-usage", &usage_focus);
    // Long localized footer labels must stay inside the narrow card at enlarged scale.
    root.update(cx, |root, cx| {
        root.settings.language = guild_session::Language::Russian;
        root.settings.ui_font_size = 20.;
        cx.notify();
    });
    cx.simulate_resize(gpui::size(px(440.), px(740.)));
    settle(cx);
    let viewport = cx.debug_bounds("action-bar-order-scroll").unwrap();
    let add = cx.debug_bounds("action-bar-add").unwrap();
    let restore = cx.debug_bounds("action-bar-reset").unwrap();
    assert!(
        add.right() <= viewport.right() && restore.right() <= viewport.right(),
        "localized footer stays within card: {add:?} {restore:?} viewport={viewport:?}"
    );
}

fn assert_item_visible(
    cx: &mut gpui::VisualTestContext,
    selector: &'static str,
    focus: &gpui::FocusHandle,
) {
    assert!(
        cx.update(|window, _| focus.is_focused(window)),
        "stable item focus must remain"
    );
    let viewport = cx.debug_bounds("action-bar-order-scroll").unwrap();
    let item = cx.debug_bounds(selector).unwrap();
    assert!(
        item.left() >= viewport.left() && item.right() <= viewport.right(),
        "focused {selector} must be visible: item={item:?}, viewport={viewport:?}"
    );
}

/// The GPUI test host does not automatically dispatch native frame callbacks.
fn settle(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| window.simulate_next_frame(cx));
    cx.run_until_parked();
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_add_menu_ends_with_its_settings_source(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            root.settings_page = Some(super::Section::Appearance);
            root.settings.action_bar_excluded = vec!["usage".into()];
            root
        });
        root.read(cx).focus.clone().focus(window, cx);
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = made.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(850.)));
    settle(cx);
    for transition in ["window close", "search", "category", "scope"] {
        root.update(cx, |root, cx| {
            root.settings_page = Some(super::Section::Appearance);
            root.scoped_settings.targets.clear();
            root.settings_query = "Action Bar".into();
            cx.notify();
        });
        settle(cx);
        // Changing hosts resets search on paint, just as reopening native Settings does.
        root.update(cx, |root, cx| {
            root.settings_query = "Action Bar".into();
            cx.notify();
        });
        settle(cx);
        let return_focus = root.read_with(cx, |root, _| {
            root.action_bar_scroll.items.borrow()["open_in"].clone()
        });
        cx.update(|window, cx| return_focus.focus(window, cx));
        let add = cx.debug_bounds("action-bar-add").unwrap().center();
        cx.simulate_click(add, Default::default());
        settle(cx);
        assert!(
            cx.debug_bounds("menu-item-0").is_some(),
            "menu opened before {transition}, live source {:?}",
            root.read_with(cx, |root, _| root.action_bar_settings_source())
        );
        let stale_target = root.read_with(cx, |root, _| {
            root.context_menu.as_ref().unwrap().target.clone()
        });
        match transition {
            "window close" => cx.simulate_keystrokes("cmd-w"),
            _ => root.update(cx, |root, cx| {
                match transition {
                    "search" => root.settings_query = "no-matching-setting".into(),
                    "category" => {
                        root.settings_query.clear();
                        root.choose_section(
                            super::SettingsHost::Window,
                            super::Section::General,
                            cx,
                        );
                    }
                    "scope" => root.choose_scoped_settings(
                        super::SettingsHost::Window,
                        crate::root::repository_settings::Target::GlobalLsp,
                        cx,
                    ),
                    _ => unreachable!(),
                }
                cx.notify();
            }),
        }
        // An already-dispatched row must not act after its source disappears.
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.run_deed(
                    crate::view::menu::Deed::ActionBarAdd(Usage),
                    &stale_target,
                    window,
                    cx,
                );
            })
        });
        settle(cx);
        assert!(
            cx.debug_bounds("action-bar-add").is_none(),
            "source absent after {transition}"
        );
        assert!(
            root.read_with(cx, |root, _| root.context_menu.is_none()),
            "Add must close after {transition}"
        );
        assert!(
            !cx.update(|window, _| return_focus.is_focused(window)),
            "hidden control cannot retain focus"
        );
        assert!(root.read_with(cx, |root, _| {
            root.settings.action_bar_excluded.contains(&"usage".into())
        }));
    }
    // A project hosts Settings in a real tab, rather than the window page.
    root.update(cx, |root, cx| {
        let project = root.desk.add_project(dir.path());
        root.active_project = Some(project);
        root.open_settings_at(super::Section::Appearance, cx);
    });
    settle(cx);
    root.update(cx, |root, cx| {
        root.settings_query = "Action Bar".into();
        cx.notify();
    });
    settle(cx);
    let source = root.read_with(cx, |root, _| root.active_tab.unwrap());
    let add = cx.debug_bounds("action-bar-add").unwrap().center();
    cx.simulate_click(add, Default::default());
    settle(cx);
    root.update(cx, |root, cx| {
        let project = root.active_project.unwrap();
        let other = root.restore_settings_tab(project, cx);
        root.active_tab = Some(other);
        cx.notify();
    });
    settle(cx);
    assert!(
        root.read_with(cx, |root, _| root.context_menu.is_none()),
        "switching Settings tabs invalidates the original source"
    );
    root.update(cx, |root, cx| {
        root.active_tab = Some(source);
        cx.notify();
    });
    settle(cx);
    root.update(cx, |root, cx| {
        root.settings_query = "Action Bar".into();
        cx.notify();
    });
    settle(cx);
    let add = cx.debug_bounds("action-bar-add").unwrap().center();
    cx.simulate_click(add, Default::default());
    settle(cx);
    cx.simulate_keystrokes("cmd-w");
    settle(cx);
    assert!(
        root.read_with(cx, |root, _| root.context_menu.is_none()),
        "closing Settings tab dismisses Add"
    );
    assert!(root.read_with(cx, |root, _| !root.tabs.iter().any(|tab| tab.id == source)));
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_keyboard_removal_closes_owned_usage_card(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            let project = root.desk.add_project(dir.path());
            root.active_project = Some(project);
            root.open_settings_at(super::Section::Appearance, cx);
            root
        });
        made = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = made.unwrap();
    cx.simulate_resize(gpui::size(px(1200.), px(850.)));
    settle(cx);
    root.update(cx, |root, cx| {
        root.settings_query = "Action Bar".into();
        cx.notify();
    });
    settle(cx);
    let trigger = cx.debug_bounds("titlebar-usage").unwrap().center();
    cx.simulate_click(trigger, Default::default());
    settle(cx);
    assert!(root.read_with(cx, |root, _| root.showing_usage));
    // Establish the item focus precondition without bypassing the removal control.
    let usage_focus = root.read_with(cx, |root, _| {
        root.action_bar_scroll.items.borrow()["usage"].clone()
    });
    cx.update(|window, cx| usage_focus.focus(window, cx));
    settle(cx);
    cx.simulate_keystrokes("tab");
    settle(cx);
    assert!(root.read_with(cx, |root, _| root.showing_usage));
    cx.simulate_keystrokes("enter");
    cx.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse("enter").unwrap(),
    });
    settle(cx);
    assert!(
        root.read_with(cx, |root, _| root
            .settings
            .action_bar_excluded
            .contains(&"usage".into())),
        "keyboard reached removal through open card"
    );
    assert!(cx.debug_bounds("titlebar-usage").is_none());
    assert!(
        !root.read_with(cx, |root, _| root.showing_usage),
        "removing Usage must close its owned card"
    );
    assert!(cx.debug_bounds("action-bar-add").is_some());
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_button_insets_and_removal_centers(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.settings_page = Some(super::Section::Appearance);
            root
        });
        made = Some(root.clone());
        let host = cx.new(|cx| KeyboardHost {
            _changed: cx.observe(&root, |_, _, cx| cx.notify()),
            root,
            focus: cx.focus_handle(),
            search: cx.new(|cx| gpui_component::input::InputState::new(window, cx)),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let root = made.unwrap();
    for language in [
        guild_session::Language::English,
        guild_session::Language::Russian,
    ] {
        for font in [13., 20.] {
            for width in [900., 440.] {
                for disabled in [true, false] {
                    root.update(cx, |root, cx| {
                        root.settings.language = language;
                        root.settings.ui_font_size = font;
                        root.settings.action_bar_excluded = if disabled {
                            vec![]
                        } else {
                            vec!["usage".into()]
                        };
                        cx.notify();
                    });
                    cx.simulate_resize(gpui::size(px(width), px(740.)));
                    settle(cx);
                    let padding = root
                        .read_with(cx, |root, _| px(root.rem_px(crate::theme::space::CARD_GAP)));
                    let height = root.read_with(cx, |root, _| {
                        px(root.rem_px(crate::theme::space::HEAD_BUTTON))
                    });
                    for (button, label) in [
                        ("action-bar-add", "action-bar-add-label"),
                        ("action-bar-reset", "action-bar-reset-label"),
                    ] {
                        let bounds = cx.debug_bounds(button).unwrap();
                        let text = cx.debug_bounds(label).unwrap();
                        let left = text.left() - bounds.left();
                        let right = bounds.right() - text.right();
                        assert!(
                            (left - right).abs() <= px(1.),
                            "balanced {button}: left={left:?}, right={right:?}, {language:?}/{font}/{width}/disabled={disabled}"
                        );
                        assert!(
                            left >= padding && left <= padding + px(2.),
                            "token padding, no phantom content gap"
                        );
                        assert!((bounds.size.height - height).abs() <= px(1.));
                        assert!(bounds.left() >= px(0.) && bounds.right() <= px(width));
                        assert!(text.top() >= bounds.top() && text.bottom() <= bounds.bottom());
                    }
                    for item in if disabled {
                        vec![Usage, OpenIn, TerminalJobs]
                    } else {
                        vec![OpenIn, TerminalJobs]
                    } {
                        let (target_id, glyph_id) = match item {
                            Usage => ("action-bar-remove-usage", "action-bar-remove-glyph-usage"),
                            OpenIn => (
                                "action-bar-remove-open_in",
                                "action-bar-remove-glyph-open_in",
                            ),
                            TerminalJobs => (
                                "action-bar-remove-terminal_jobs",
                                "action-bar-remove-glyph-terminal_jobs",
                            ),
                        };
                        let target = cx.debug_bounds(target_id).unwrap();
                        let glyph = cx.debug_bounds(glyph_id).unwrap();
                        assert!(
                            (target.center().x - glyph.center().x).abs() <= px(1.)
                                && (target.center().y - glyph.center().y).abs() <= px(1.),
                            "centered removal icon {item:?}: {target:?}, {glyph:?}"
                        );
                        assert!(
                            target.contains(&glyph.origin)
                                && target.contains(&glyph.bottom_right())
                        );
                    }
                }
            }
        }
    }
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_empty_launch_fields_without_project_accept_click_and_typing(cx: &mut TestAppContext) {
    use gpui::prelude::*;
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            root.active_project = None;
            root.settings_page = Some(super::Section::Sessions);
            root.settings_query = "launch".into();
            root
        });
        root.read(cx).focus.clone().focus(window, cx);
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    visual.simulate_resize(gpui::size(px(1180.), px(900.)));
    settle(visual);
    assert!(root.read_with(visual, |r, _| r.active_project.is_none()));
    for (id, kind, arguments) in [
        (
            "claude-launch-prefix-input",
            guild_session::HarnessKind::Claude,
            false,
        ),
        (
            "claude-launch-prefix-input-arguments",
            guild_session::HarnessKind::Claude,
            true,
        ),
        (
            "codex-launch-prefix-input",
            guild_session::HarnessKind::Codex,
            false,
        ),
        (
            "codex-launch-prefix-input-arguments",
            guild_session::HarnessKind::Codex,
            true,
        ),
        (
            "opencode-launch-prefix-input",
            guild_session::HarnessKind::OpenCode,
            false,
        ),
        (
            "opencode-launch-prefix-input-arguments",
            guild_session::HarnessKind::OpenCode,
            true,
        ),
    ] {
        root.update(visual, |root, cx| {
            root.settings_query = match kind {
                guild_session::HarnessKind::Claude => "Claude Code",
                guild_session::HarnessKind::Codex => "Codex",
                guild_session::HarnessKind::OpenCode => "OpenCode",
            }
            .into();
            root.settings_scroll.set_offset(Default::default());
            cx.notify();
        });
        settle(visual);
        for fraction in [0.02, 0.5, 0.98] {
            let bounds = visual.debug_bounds(id).unwrap();
            println!("empty field {id}: {bounds:?}");
            visual.simulate_click(
                gpui::point(
                    bounds.left() + bounds.size.width * fraction,
                    bounds.center().y,
                ),
                Default::default(),
            );
            visual.simulate_keystrokes("p a i");
            settle(visual);
            let value = |cx: &mut gpui::VisualTestContext| {
                root.read_with(cx, |r, _| {
                    if arguments {
                        r.settings.launch_arguments.get(kind)
                    } else {
                        r.settings.launch_prefixes.get(kind)
                    }
                    .to_owned()
                })
            };
            assert_eq!(
                value(visual),
                "pai",
                "ordinary input lost at {id}/{fraction}"
            );
            visual.update(|_, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    "--model 'glm value'".into(),
                ))
            });
            visual.simulate_keystrokes("cmd-a cmd-v");
            settle(visual);
            assert_eq!(value(visual), "--model 'glm value'");
            visual.simulate_keystrokes("cmd-z");
            settle(visual);
            assert_eq!(value(visual), "pai");
            visual.simulate_keystrokes("cmd-a backspace");
            settle(visual);
        }
    }
}

#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_codex_independent_permissions_persist(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            root.settings_page = Some(super::Section::Sessions);
            root.settings.launch_arguments.codex = "  --model 'multi word'  ".into();
            root
        });
        root.read(cx).focus.clone().focus(window, cx);
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    visual.simulate_resize(gpui::size(px(1180.), px(900.)));
    settle(visual);
    // Mounting a new Settings host deliberately clears its previous search.
    // Apply this fixture's query after that transition, as a real search would.
    root.update(visual, |root, cx| {
        root.settings_query = "codex_".into();
        cx.notify();
    });
    settle(visual);
    let state = |visual: &mut gpui::VisualTestContext| {
        root.read_with(visual, |r, _| {
            (r.settings.codex_sandbox, r.settings.codex_command_approvals)
        })
    };
    assert_eq!(state(visual), (true, true));
    for (selector, expected) in [
        ("codex-approvals-toggle", (true, false)),
        ("codex-sandbox-toggle", (false, false)),
        ("codex-approvals-toggle", (false, true)),
        ("codex-sandbox-toggle", (true, true)),
    ] {
        let bounds = visual.debug_bounds(selector).unwrap();
        let viewport = root.read_with(visual, |root, _| root.settings_scroll.bounds());
        assert!(
            bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom(),
            "{selector} must be visible before clicking: bounds={bounds:?}, viewport={viewport:?}"
        );
        visual.simulate_click(bounds.center(), gpui::Modifiers::default());
        settle(visual);
        guild_session::flush();
        let saved =
            guild_session::Store::<guild_session::Settings>::new(dir.path().join("settings.json"))
                .load()
                .unwrap();
        assert_eq!(
            (saved.codex_sandbox, saved.codex_command_approvals),
            expected
        );
        assert_eq!(state(visual), expected);
        assert_eq!(saved.launch_arguments.codex, "  --model 'multi word'  ");
    }
    let russian = crate::i18n::Ui(guild_session::Language::Russian);
    assert_eq!(
        russian.text(crate::i18n::Text::CodexSandboxTitle),
        "Ограничивать доступ через sandbox"
    );
    assert_eq!(
        russian.text(crate::i18n::Text::CodexCommandApprovalsTitle),
        "Запрашивать подтверждение команд"
    );
}

/// Tabs & agents → Session list: the Settled threshold steps a day at a
/// time between one day and a month, and is saved with every step.
#[gpui::test]
#[ignore = "run through isolated_action_bar_gestures"]
fn real_settle_after_steps_and_persists(cx: &mut TestAppContext) {
    assert_eq!(std::env::var("GUILD_ACTION_BAR_TEST").as_deref(), Ok("1"));
    cx.executor().allow_parking();
    let dir = tempfile::tempdir().unwrap();
    let mut entity = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(Default::default(), dir.path().into(), cx);
            root.isolated_render = true;
            root.settings_page = Some(super::Section::Sessions);
            root
        });
        root.read(cx).focus.clone().focus(window, cx);
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    visual.simulate_resize(gpui::size(px(1180.), px(900.)));
    settle(visual);
    root.update(visual, |root, cx| {
        root.settings_query = "settle".into();
        cx.notify();
    });
    settle(visual);
    assert!(visual.debug_bounds("settle-after-value").is_some());
    let days = |visual: &mut gpui::VisualTestContext| {
        root.read_with(visual, |r, _| r.settings.settle_after_days)
    };
    assert_eq!(days(visual), 3);
    let value = visual.debug_bounds("settle-after-value").unwrap();
    // The value sits between its minus and its plus.
    let minus = gpui::point(value.left() - px(20.), value.center().y);
    let plus = gpui::point(value.right() + px(20.), value.center().y);
    visual.simulate_click(plus, gpui::Modifiers::default());
    settle(visual);
    assert_eq!(days(visual), 4);
    guild_session::flush();
    let saved =
        guild_session::Store::<guild_session::Settings>::new(dir.path().join("settings.json"))
            .load()
            .unwrap();
    assert_eq!(saved.settle_after_days, 4);
    for _ in 0..10 {
        visual.simulate_click(minus, gpui::Modifiers::default());
        settle(visual);
    }
    assert_eq!(days(visual), 1, "never below a day");
}
