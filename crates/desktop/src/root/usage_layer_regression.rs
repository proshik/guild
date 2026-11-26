//! Real usage trigger/card layers, without provider processes or user state.
use super::*;
use gpui::{AppContext, Entity, Render, TestAppContext, div, prelude::*, px, rgb};
use std::{cell::Cell, rc::Rc};

struct Host {
    root: Entity<Root>,
    ink: Rc<Cell<gpui::Hsla>>,
    clicks: Rc<Cell<usize>>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let ink = self.ink.clone();
        let clicks = self.clicks.clone();
        self.root.update(cx, |root, cx| {
            root.viewport = window.viewport_size();
            div()
                .size_full()
                .relative()
                .child(
                    div()
                        .absolute()
                        .top(px(500.))
                        .size(px(100.))
                        .id("underlying-panel")
                        .debug_selector(|| "underlying-panel".into())
                        .on_click(move |_, _, _| clicks.set(clicks.get() + 1)),
                )
                // Append only a paint observer to the production content; use
                // exactly the same trigger layer as render_usage_chip.
                .child(
                    div()
                        .flex()
                        .justify_end()
                        .child(crate::view::usage::usage_chip_layer(
                            root.usage_chip_content(cx).child(gpui::canvas(
                                |_, _, _| (),
                                move |_, _, window, _| ink.set(window.text_style().color),
                            )),
                            root.showing_usage,
                        )),
                )
                .children(root.render_usage(cx))
        })
    }
}
#[test]
fn isolated_real_usage_layers() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::usage::usage_layer_regression::real_chip_hover_toggle_and_outside_click",
            "--exact",
            "--ignored",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[gpui::test]
#[ignore = "isolated_real_usage_layers supplies private state"]
fn real_chip_hover_toggle_and_outside_click(cx: &mut TestAppContext) {
    let ink = Rc::new(Cell::new(gpui::Hsla::default()));
    let clicks = Rc::new(Cell::new(0));
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let entity =
            cx.new(|cx| Root::from_state(Default::default(), guild_session::Desk::default(), cx));
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            root: entity.clone(),
            ink: ink.clone(),
            clicks: clicks.clone(),
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(700.), px(700.)));
    let outside = gpui::point(px(50.), px(550.));
    for missing in [false, true] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.showing_usage = false;
                r.usage.claude = Provider::default();
                r.usage.codex.take(Err(Failure::NotInstalled), "fixture");
                if missing {
                    r.usage.claude.take(Err(Failure::NoCredentials), "fixture");
                } else {
                    r.usage.claude.heard(
                        vec![Window {
                            name: usage::WindowName::Week,
                            left: 88,
                            resets_at: None,
                        }],
                        Instant::now(),
                        "fixture",
                    );
                }
                cx.notify();
            })
        });
        cx.run_until_parked();
        let chip = cx.debug_bounds("titlebar-usage").unwrap();
        cx.simulate_mouse_move(outside, None, Default::default());
        cx.run_until_parked();
        cx.simulate_mouse_move(chip.center(), None, Default::default());
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(249));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("guild-control-tooltip").is_none(),
            "shared delay must elapse"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(1));
        cx.run_until_parked();
        let tip = cx
            .debug_bounds("guild-control-tooltip")
            .expect("closed usage chip must explain its purpose, including unavailable readings");
        assert_eq!(tip.size.height, px(28.));
        cx.simulate_click(chip.center(), Default::default());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| root.read(cx).showing_usage));
        cx.simulate_mouse_move(outside, None, Default::default());
        cx.simulate_mouse_move(chip.center(), None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds("guild-control-tooltip").is_none(),
            "no tooltip over the open usage card"
        );
        let fold = cx.debug_bounds("usage-all").unwrap();
        cx.simulate_click(fold.center(), Default::default());
        cx.run_until_parked();
        assert!(
            cx.update(|_, cx| root.read(cx).showing_usage),
            "card content remains interactive without dismissal"
        );
        cx.simulate_mouse_move(outside, None, Default::default());
        cx.update(|window, _| window.refresh());
        cx.run_until_parked();
        let theme = cx.update(|_, cx| root.read(cx).theme);
        assert_eq!(
            ink.get(),
            rgb(if missing {
                theme.text.faint
            } else {
                theme.text.tertiary
            })
            .into()
        );
        cx.simulate_mouse_move(chip.center(), None, Default::default());
        cx.run_until_parked();
        assert_eq!(
            ink.get(),
            rgb(theme.text.primary).into(),
            "real open-card backdrop must allow chip hover (missing={missing})"
        );
        cx.simulate_mouse_move(outside, None, Default::default());
        cx.run_until_parked();
        assert_eq!(
            ink.get(),
            rgb(if missing {
                theme.text.faint
            } else {
                theme.text.tertiary
            })
            .into()
        );
        cx.simulate_click(chip.center(), Default::default());
        cx.run_until_parked();
        assert!(
            !cx.update(|_, cx| root.read(cx).showing_usage),
            "chip toggles the real open card closed"
        );
        cx.simulate_click(chip.center(), Default::default());
        cx.run_until_parked();
        assert!(cx.update(|_, cx| root.read(cx).showing_usage));
        cx.simulate_click(outside, Default::default());
        cx.run_until_parked();
        assert!(!cx.update(|_, cx| root.read(cx).showing_usage));
        assert_eq!(
            clicks.get(),
            0,
            "outside click must not reach the underlying panel"
        );
    }
    cx.simulate_click(outside, Default::default());
    cx.run_until_parked();
    assert_eq!(
        clicks.get(),
        1,
        "underlying panel is interactive when the card is closed"
    );
}
