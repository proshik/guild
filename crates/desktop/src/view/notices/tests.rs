//! The production notification renderer, with an isolated in-memory Root.
use super::*;
use gpui::{Entity, Render, TestAppContext, VisualTestContext, Window, point, px, size};
use std::{cell::Cell, rc::Rc, time::Instant};

struct Host {
    root: Entity<Root>,
    background: Rc<Cell<usize>>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let down = self.background.clone();
        let up = self.background.clone();
        let motion = self.background.clone();
        let wheel = self.background.clone();
        self.root.update(cx, |root, cx| {
            div()
                .relative()
                .size_full()
                .child(
                    div()
                        .id("behind")
                        .size_full()
                        .on_mouse_down(gpui::MouseButton::Left, move |_, _, _| {
                            down.set(down.get() + 1)
                        })
                        .on_mouse_up(gpui::MouseButton::Left, move |_, _, _| up.set(up.get() + 1))
                        .on_mouse_move(move |_, _, _| motion.set(motion.get() + 1))
                        .on_scroll_wheel(move |_, _, _| wheel.set(wheel.get() + 1)),
                )
                .child(root.render_notices(window, cx))
        })
    }
}
fn fixture(
    cx: &mut TestAppContext,
    texts: Vec<(Level, String)>,
) -> (Entity<Root>, Rc<Cell<usize>>, &mut VisualTestContext) {
    assert_eq!(std::env::var("GUILD_NOTICE_TEST").as_deref(), Ok("1"));
    let mut root = None;
    let background = Rc::new(Cell::new(0));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                guild_session::Desk::default(),
                cx,
            );
            root.notices = Default::default();
            for (level, text) in texts {
                root.notices.push(level, text, Instant::now());
            }
            root
        });
        entity.read(cx).focus.clone().focus(window, cx);
        root = Some(entity.clone());
        Host {
            background: background.clone(),
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            root: entity,
        }
    });
    cx.update(|window, _| {
        window.activate_window();
        window.set_rem_size(crate::scale::rem_size(13.0));
    });
    (root.unwrap(), background, cx)
}
fn message() -> String {
    (0..300)
        .map(|line| format!("Строка {line}: предупреждение 🌍\n"))
        .collect()
}
fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("control drawn");
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn long_notice_stack_stays_in_viewport(cx: &mut TestAppContext) {
    let (root, _background, cx) = fixture(cx, vec![(Level::Warning, message())]);
    for count in 1..=3 {
        if count > 1 {
            root.update(cx, |root, cx| {
                root.notices.push(
                    Level::Warning,
                    format!("{count}\n{}", message()),
                    Instant::now(),
                );
                cx.notify();
            });
        }
        for (w, h) in [(1180., 760.), (640., 480.), (320., 240.), (1180., 760.)] {
            cx.simulate_resize(size(px(w), px(h)));
            let mut previous_bottom = px(0.);
            for index in 0..count {
                let card = cx
                    .debug_bounds(["notice-0", "notice-1", "notice-2"][index])
                    .unwrap();
                assert!(
                    card.top() >= px(16.),
                    "card extends above viewport: {card:?}"
                );
                assert!(
                    card.left() >= px(16.) && card.right() <= px(w - 16.),
                    "card width: {card:?}"
                );
                assert!(card.bottom() <= px(h - 16.), "card bottom: {card:?}");
                assert!(card.top() >= previous_bottom, "cards overlap");
                previous_bottom = card.bottom();
                for action in [
                    ["notice-copy-0", "notice-copy-1", "notice-copy-2"],
                    ["notice-close-0", "notice-close-1", "notice-close-2"],
                ] {
                    let b = cx.debug_bounds(action[index]).unwrap();
                    assert!(
                        card.contains(&b.origin)
                            && b.right() <= card.right()
                            && b.bottom() <= card.bottom(),
                        "action outside card"
                    );
                }
            }
        }
    }
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn content_click_does_not_dismiss_and_copy_keeps_focus(cx: &mut TestAppContext) {
    let (root, background, cx) = fixture(cx, vec![(Level::Warning, "Короткое сообщение".into())]);
    cx.simulate_resize(size(px(640.), px(480.)));
    let card = cx.debug_bounds("notice-0").unwrap();
    let target = point(card.left() + px(24.), card.top() + px(20.));
    cx.simulate_mouse_down(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        target + point(px(2.), px(1.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(target, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).notices.visible().count(),
            1,
            "reading is not dismissal"
        )
    });
    assert_eq!(
        background.get(),
        0,
        "pointer gesture reached the background"
    );
    click(cx, "notice-copy-0");
    cx.update(|window, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()),
            Some("Короткое сообщение".into())
        );
        assert!(root.read(cx).focus.is_focused(window));
        assert!(root.read(cx).notices.visible().next().unwrap().1.copied);
    });
    click(cx, "notice-close-0");
    cx.update(|_, cx| assert!(root.read(cx).notices.is_empty()));
}

#[test]
fn isolated_notice_window_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::notices::tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NOTICE_TEST", "1")
        .current_dir(state.path())
        .output()
        .unwrap();
    print!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn full_unicode_payload_scrolls_and_copies_without_click_through(cx: &mut TestAppContext) {
    let text = format!(
        "{}\n{}\nLAST LINE: конец 🌍  \n",
        "Ж".repeat(4000),
        message()
    );
    let (root, background, cx) = fixture(cx, vec![(Level::Warning, text.clone())]);
    cx.simulate_resize(size(px(320.), px(240.)));
    let body = cx.debug_bounds("notice-text-0").unwrap();
    let scroll = cx.update(|_, cx| root.read(cx).notice_views.borrow()[&0].scroll.clone());
    assert!(
        scroll.max_offset().y > px(1000.),
        "complete text was laid out, not truncated"
    );
    assert_eq!(
        scroll.max_offset().x,
        px(0.),
        "unbroken Unicode wraps within the content width"
    );
    for delta in [-1_000_000., -100., 1_000_000., 100., -1_000_000.] {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: body.center(),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(delta))),
            ..Default::default()
        });
        let content = cx.debug_bounds("notice-message-0").unwrap();
        if delta < 0. {
            assert!(
                (content.bottom() - body.bottom()).abs() <= px(1.),
                "last line unreachable: {content:?}"
            );
        } else {
            assert!(
                (content.top() - body.top()).abs() <= px(1.),
                "first line unreachable"
            );
        }
        let card = cx.debug_bounds("notice-0").unwrap();
        let copy = cx.debug_bounds("notice-copy-0").unwrap();
        assert!(card.contains(&copy.center()));
    }
    assert_eq!(
        background.get(),
        0,
        "scroll, including limits, reached background"
    );
    click(cx, "notice-copy-0");
    cx.update(|window, cx| {
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()), Some(text));
        assert_eq!(root.read(cx).notices.visible().count(), 1);
        assert!(root.read(cx).focus.is_focused(window));
    });
    click(cx, "notice-close-0");
    cx.update(|_, cx| assert!(root.read(cx).notices.is_empty()));
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn child_control_hover_and_membership_changes_keep_the_right_notice(cx: &mut TestAppContext) {
    let (root, _, cx) = fixture(
        cx,
        vec![
            (Level::Info, "first".into()),
            (Level::Info, "second".into()),
        ],
    );
    cx.simulate_resize(size(px(640.), px(480.)));
    let copy = cx.debug_bounds("notice-copy-1").unwrap().center();
    cx.simulate_event(gpui::MouseMoveEvent {
        position: copy,
        ..Default::default()
    });
    let later = Instant::now() + crate::notify::FADE * 3;
    // GPUI's test platform reports (0,0) at frame start. Dispatch and tick
    // together to exercise actual window pointer truth before the next frame.
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position: copy,
                ..Default::default()
            }),
            cx,
        );
        root.update(cx, |root, cx| {
            root.sync_notice_hover(window, later);
            assert!(root.notices.expire(later));
            cx.notify();
        });
    });
    cx.debug_bounds("notice-1").unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx)
                .notices
                .visible()
                .map(|(_, n)| n.text.as_str())
                .collect::<Vec<_>>(),
            vec!["second"]
        )
    });
    // A close that began on a removed notice cannot close its replacement.
    let close = cx.debug_bounds("notice-close-1").unwrap().center();
    cx.simulate_mouse_down(close, gpui::MouseButton::Left, gpui::Modifiers::default());
    root.update(cx, |root, cx| {
        root.notices.dismiss(1);
        root.notices
            .push(Level::Warning, "replacement".into(), Instant::now());
        cx.notify();
    });
    cx.debug_bounds("notice-2").unwrap();
    cx.simulate_mouse_up(close, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).notices.visible().next().unwrap().1.text,
            "replacement"
        )
    });
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn exiting_window_releases_info_without_a_final_move(cx: &mut TestAppContext) {
    let (root, _, cx) = fixture(cx, vec![(Level::Info, "held".into())]);
    cx.simulate_resize(size(px(640.), px(480.)));
    let copy = cx.debug_bounds("notice-copy-0").unwrap().center();
    cx.simulate_event(gpui::MouseMoveEvent {
        position: copy,
        ..Default::default()
    });
    cx.update(|window, cx| {
        window.dispatch_event(
            gpui::PlatformInput::MouseMove(gpui::MouseMoveEvent {
                position: copy,
                ..Default::default()
            }),
            cx,
        );
        assert!(window.is_window_hovered(), "active window");
        root.update(cx, |root, _| {
            assert!(
                root.notice_views.borrow()[&0].bounds.get().contains(&copy),
                "copy inside measured card"
            );
            root.sync_notice_hover(window, Instant::now());
            assert!(
                !root
                    .notices
                    .expire(Instant::now() + crate::notify::FADE * 2),
                "hover holds notice before exit"
            );
        });
        window.dispatch_event(
            gpui::PlatformInput::MouseExited(gpui::MouseExitEvent {
                position: point(px(641.), copy.y),
                pressed_button: None,
                modifiers: Default::default(),
            }),
            cx,
        );
        assert_eq!(
            window.mouse_position(),
            copy,
            "exit preserves last mouse position"
        );
        let now = Instant::now();
        root.update(cx, |root, _| {
            root.sync_notice_hover(window, now);
            root.sync_notice_hover(window, now + crate::notify::FADE * 2);
            assert!(
                root.notices.expire(now + crate::notify::FADE * 2),
                "exit must release hover despite stale window coordinates"
            );
        });
    });
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn reentry_while_empty_allows_a_new_info_to_be_read(cx: &mut TestAppContext) {
    let (root, _, cx) = fixture(cx, vec![(Level::Info, "held".into())]);
    cx.simulate_resize(size(px(640.), px(480.)));
    let copy = cx.debug_bounds("notice-copy-0").unwrap().center();
    cx.simulate_event(gpui::MouseExitEvent {
        position: point(px(641.), copy.y),
        pressed_button: None,
        modifiers: Default::default(),
    });
    root.update(cx, |root, cx| {
        assert!(
            root.notices
                .expire(Instant::now() + crate::notify::FADE * 2)
        );
        cx.notify();
    });
    assert!(cx.debug_bounds("notice-0").is_none());
    cx.simulate_event(gpui::MouseMoveEvent {
        position: copy,
        ..Default::default()
    });
    root.update(cx, |root, cx| {
        root.notices.push(Level::Info, "new".into(), Instant::now());
        cx.notify();
    });
    let copy = cx.debug_bounds("notice-copy-1").unwrap().center();
    cx.update(|window, cx| {
        // TestWindow reports (0,0) on each frame. A down event reseeds its
        // coordinates without invoking the MouseMove reentry handler.
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                position: copy,
                button: gpui::MouseButton::Left,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }),
            cx,
        );
        root.update(cx, |root, _| {
            root.sync_notice_hover(window, Instant::now());
            assert!(
                !root
                    .notices
                    .expire(Instant::now() + crate::notify::FADE * 2),
                "new info under returned pointer must remain readable"
            );
        });
    });
}

#[gpui::test]
#[ignore = "runs in isolated_notice_window_regressions"]
fn terminal_action_stays_visible_and_only_runs_when_requested(cx: &mut TestAppContext) {
    let (root, _, cx) = fixture(cx, vec![]);
    root.update(cx, |root, cx| {
        for level in [Level::Info, Level::Warning, Level::Error] {
            root.notices.push_terminal(
                level,
                format!("{level:?}\n{}", message()),
                guild_session::ProjectId::new(),
                "/missing-fixture".into(),
                Instant::now(),
            );
        }
        cx.notify();
    });
    for (w, h, scale) in [(320., 240., 13.), (640., 480., 20.)] {
        cx.update(|window, _| {
            window.set_rem_size(crate::scale::rem_size(scale));
            window.refresh();
        });
        cx.simulate_resize(size(px(w), px(h)));
        for name in [
            "notice-terminal-0",
            "notice-terminal-1",
            "notice-terminal-2",
        ] {
            let b = cx.debug_bounds(name).unwrap();
            assert!(
                b.top() >= px(0.) && b.bottom() <= px(h) && b.right() <= px(w),
                "terminal action: {b:?}"
            );
        }
    }
    cx.update(|_, cx| assert_eq!(root.read(cx).notices.visible().count(), 3));
    click(cx, "notice-copy-2");
    cx.update(|_, cx| assert_eq!(root.read(cx).notices.visible().count(), 3));
    click(cx, "notice-terminal-2");
    cx.update(|_, cx| {
        let root = root.read(cx);
        assert!(
            root.notices.visible().any(
                |(_, n)| n.text == root.ui().text(crate::i18n::Text::UpdateTerminalUnavailable)
            ),
            "explicit action follows existing unavailable-project route"
        );
        assert!(root.tabs.is_empty(), "no implicit terminal launch");
    });
}
