//! The production usage card, isolated from providers and saved user state.
use super::*;
use gpui::{AppContext, Entity, Render, TestAppContext, Window};
use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

struct Host {
    root: Entity<Root>,
    _changed: gpui::Subscription,
}

impl Render for Host {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            root.viewport = window.viewport_size();
            div()
                .size_full()
                .child(div().flex().justify_end().child(root.render_usage_chip(cx)))
                .children(root.render_usage(cx))
        })
    }
}

#[test]
fn isolated_usage_card_regression() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::usage::tests::regression::provider_rows_follow_focus_without_repeating_primary",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
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
#[ignore = "run through isolated_usage_card_regression"]
fn provider_rows_follow_focus_without_repeating_primary(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut root = None;
    let mut tabs = Vec::new();
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut root = Root::from_state(Default::default(), desk, cx);
            root.active_project = Some(project);
            root.showing_usage = true;
            root.usage_all_open = true;
            let seat = Seat {
                workspace: root.active_workspace,
                project,
            };
            // Session records select the real focused provider without launching CLIs.
            for runs in [
                Runs::Codex(SessionMode::Terminal),
                Runs::Claude(SessionMode::Terminal),
                Runs::OpenCode(SessionMode::Terminal),
                Runs::Shell,
            ] {
                let session = root.desk.add_session(SessionRecord::new(
                    "Usage fixture".into(),
                    seat,
                    dir.path().into(),
                    runs,
                ));
                tabs.push(root.adopt(
                    seat,
                    project,
                    crate::Body::Broken {
                        session,
                        reason: "No process in the usage fixture".into(),
                    },
                    cx,
                ));
            }
            root
        });
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
            root: entity,
        });
        gpui_component::Root::new(host, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(700.), px(700.)));
    let selectors = [
        "usage-secondary-Claude Code",
        "usage-secondary-OpenCode",
        "usage-secondary-Codex",
    ];
    // Before any session, the existing initial front is Claude.
    for (tab, expected, absent) in [
        (None, Agent::ClaudeCode, 0),
        (Some(tabs[0]), Agent::Codex, 2),
        (Some(tabs[1]), Agent::ClaudeCode, 0),
        (Some(tabs[2]), Agent::OpenCode, 1),
        (Some(tabs[3]), Agent::OpenCode, 1),
        (None, Agent::OpenCode, 1),
    ] {
        cx.update(|_, cx| {
            root.update(cx, |r, cx| {
                r.active_tab = tab;
                cx.notify();
            })
        });
        cx.run_until_parked();
        assert_eq!(cx.update(|_, cx| root.read(cx).usage_front()), expected);
        let primary = [
            "usage-primary-Claude Code",
            "usage-primary-OpenCode",
            "usage-primary-Codex",
        ][absent];
        assert!(cx.debug_bounds(primary).is_some());
        for (index, selector) in selectors.iter().enumerate() {
            assert_eq!(
                cx.debug_bounds(selector).is_none(),
                index == absent,
                "{expected:?}: the primary provider must not repeat in {selector}"
            );
        }
        let remaining: Vec<_> = selectors
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != absent)
            .map(|(_, selector)| cx.debug_bounds(selector).unwrap().top())
            .collect();
        assert!(
            remaining[0] < remaining[1],
            "other providers keep their order"
        );
    }
    let fold = cx.debug_bounds("usage-all").unwrap();
    cx.simulate_click(fold.center(), Default::default());
    cx.run_until_parked();
    assert!(
        selectors
            .iter()
            .all(|selector| cx.debug_bounds(selector).is_none())
    );
    cx.simulate_click(fold.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds(selectors[1]).is_none());
    assert!(cx.debug_bounds(selectors[0]).is_some());
    assert!(cx.debug_bounds(selectors[2]).is_some());
}

#[gpui::test]
fn chip_hover_brightens_foreground_and_restores_on_leave(cx: &mut TestAppContext) {
    struct ChipHost {
        ink: std::rc::Rc<std::cell::Cell<gpui::Hsla>>,
        theme: Theme,
        open: bool,
        missing: bool,
    }
    impl Render for ChipHost {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let ink = self.ink.clone();
            div().size_full().flex().items_start().child(
                usage_chip_frame(self.theme, self.open, self.missing)
                    .child(if self.missing { "-" } else { "88%" })
                    .child(gpui::canvas(
                        |_, _, _| (),
                        move |_, _, window, _| {
                            ink.set(window.text_style().color);
                        },
                    )),
            )
        }
    }
    let theme = Theme::dark();
    let ink = std::rc::Rc::new(std::cell::Cell::new(gpui::Hsla::default()));
    let ink_clone = ink.clone();
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let entity = cx.new(|_| ChipHost {
            ink: ink_clone,
            theme,
            open: false,
            missing: false,
        });
        host = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    cx.simulate_resize(gpui::size(px(300.), px(150.)));
    let bounds = cx.debug_bounds("titlebar-usage").unwrap();
    cx.simulate_mouse_move(gpui::point(px(250.), px(100.)), None, Default::default());
    // Deliver a fresh frame after the test platform's initial hover at (0, 0).
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
    assert_eq!(ink.get(), rgb(theme.text.tertiary).into());
    for (open, missing) in [(false, false), (true, false), (false, true), (true, true)] {
        cx.update(|_, cx| {
            host.as_ref().unwrap().update(cx, |h, cx| {
                h.open = open;
                h.missing = missing;
                cx.notify();
            })
        });
        cx.simulate_mouse_move(bounds.center(), None, Default::default());
        cx.run_until_parked();
        assert_eq!(
            ink.get(),
            rgb(theme.text.primary).into(),
            "hover brightens the foreground even while the card is open"
        );
        cx.simulate_mouse_move(gpui::point(px(250.), px(100.)), None, Default::default());
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
    }
}

#[gpui::test]
fn chip_hover_fill_is_distinct_in_every_palette(cx: &mut TestAppContext) {
    struct Chip(Theme);
    impl Render for Chip {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .child(usage_chip_frame(self.0, false, false).child("88%"))
        }
    }
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let view = cx.new(|_| Chip(Theme::dark()));
        host = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    let host = host.unwrap();
    cx.simulate_resize(gpui::size(px(300.), px(140.)));
    for theme in Theme::all() {
        cx.update(|_, cx| {
            host.update(cx, |host, cx| {
                host.0 = theme;
                cx.notify();
            })
        });
        cx.simulate_mouse_move(gpui::point(px(250.), px(100.)), None, Default::default());
        cx.run_until_parked();
        let bounds = cx.debug_bounds("titlebar-usage").unwrap();
        cx.simulate_mouse_move(bounds.center(), None, Default::default());
        cx.run_until_parked();
        let fill = cx.update(|window, _| {
            window
                .painted_quads()
                .into_iter()
                .filter(|quad| quad.bounds == bounds.scale(window.scale_factor()))
                .filter_map(|quad| quad.background.as_solid())
                .rfind(|color| color.a > 0.)
                .unwrap()
        });
        let fill: u32 = u32::from(gpui::Rgba::from(fill)) >> 8;
        assert!(
            theme::contrast(theme.surface.card, fill) >= 1.2,
            "hover disappears in {:?}: {:x} vs {:x}",
            theme.kind,
            theme.surface.card,
            fill
        );
        assert!(
            theme::contrast(theme.text.primary, fill) >= theme::LEGIBLE,
            "ink lost in {:?}: {:x} on {:x}, contrast {}",
            theme.kind,
            theme.text.primary,
            fill,
            theme::contrast(theme.text.primary, fill)
        );
    }
}
