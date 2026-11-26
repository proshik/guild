//! Real titlebar controls and named dispatch, with an intercepted launch boundary.
use super::*;
use gpui::{
    AppContext, Context, Entity, Focusable, IntoElement, Render, TestAppContext, div, prelude::*,
    px,
};
use guild_session::settings::OpenInTarget;

struct Host {
    root: Entity<Root>,
    input: Entity<gpui_component::input::InputState>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.input.clone();
        self.root.update(cx, |root, cx| {
            let control = root.render_open_in_trigger(cx);
            root.bind_projects(
                div()
                    .size_full()
                    .key_context("guild")
                    .track_focus(&root.focus)
                    .child(control)
                    .child(gpui_component::input::Input::new(&input)),
                cx,
            )
        })
    }
}

#[test]
fn isolated_selected_app_controls_and_dispatch() {
    let state = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::menu::open_in::tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[gpui::test]
#[ignore = "isolated_selected_app_controls_and_dispatch supplies private state"]
fn selected_app_controls_and_dispatch(cx: &mut TestAppContext) {
    let state = crate::paths::state_dir();
    let folder = state.join("active worktree");
    std::fs::create_dir_all(&folder).unwrap();
    let mut desk = guild_session::Desk::default();
    let project = desk.open_in(desk.active_workspace(), &folder);
    desk.look_at(Some(project), None);
    let mut root = None;
    let mut input = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let field = cx.new(|cx| gpui_component::input::InputState::new(window, cx));
        input = Some(field.clone());
        let entity = cx.new(|cx| Root::diagnostic(desk, state.clone(), cx));
        entity.update(cx, |root, _| {
            root.open_in_launches = Some(vec![]);
            root.open_in_availability = Some(["dev.zed.Zed".to_owned()].into());
            root.open_in_apps = Some(vec![
                crate::open_in::Connection::fixture("Finder", "com.apple.finder"),
                crate::open_in::Connection::fixture("Zed", "dev.zed.Zed"),
            ]);
        });
        root = Some(entity.clone());
        let host = cx.new(|cx| Host {
            root: entity.clone(),
            input: field,
            _changed: cx.observe(&entity, |_, _, cx| cx.notify()),
        });
        gpui_component::Root::new(host, window, cx)
    });
    let root = root.unwrap();
    let input = input.unwrap();
    cx.run_until_parked();
    let primary = cx.debug_bounds("titlebar-open-selected").unwrap();
    let selector = cx.debug_bounds("titlebar-open-selector").unwrap();
    let control = cx.debug_bounds("titlebar-open-in").unwrap();
    assert_eq!(control.size.height, px(24.));
    assert!(
        primary.right() <= selector.left(),
        "targets must not overlap"
    );
    assert!(primary.size.width >= px(22.) && selector.size.width >= px(22.));
    for bounds in [primary, selector] {
        cx.simulate_mouse_move(gpui::point(px(200.), px(100.)), None, Default::default());
        cx.simulate_mouse_move(bounds.center(), None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(250));
        cx.run_until_parked();
        assert_eq!(
            cx.debug_bounds("guild-control-tooltip")
                .expect("both closed split controls have a compact hint")
                .size
                .height,
            px(28.)
        );
    }
    cx.update(|window, cx| input.read(cx).focus_handle(cx).focus(window, cx));
    cx.run_until_parked();
    cx.simulate_mouse_down(
        primary.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    assert!(
        cx.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window)),
        "primary mouse-down must retain editor input focus"
    );
    cx.simulate_mouse_up(
        primary.center(),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.run_until_parked();
    cx.simulate_keystrokes("a");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| input.read(cx).value().to_string()),
        "a",
        "ordinary input must still reach the editor after an external-open click"
    );
    cx.simulate_keystrokes("cmd-o b");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|_, cx| input.read(cx).value().to_string()),
        "ab",
        "Cmd+O must retain ordinary input focus too"
    );
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).open_in_launches.as_ref().unwrap(),
            &vec![(OpenInTarget::Finder, folder.clone()); 2]
        );
    });
    cx.dispatch_action(crate::actions::SelectOpenApp);
    cx.run_until_parked();
    assert!(cx.update(|_, cx| matches!(root.read(cx).context_menu.as_ref().map(|m| &m.target), Some(Target::OpenIn { folder: path, .. }) if path == &folder)));
    // An opened menu suppresses both hints even after the shared delay.
    for bounds in [primary, selector] {
        cx.simulate_mouse_move(gpui::point(px(200.), px(100.)), None, Default::default());
        cx.simulate_mouse_move(bounds.center(), None, Default::default());
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(300));
        cx.run_until_parked();
        assert!(cx.debug_bounds("guild-control-tooltip").is_none());
    }
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let target = root.context_menu.as_ref().unwrap().target.clone();
            let at = match &target {
                Target::OpenIn { apps, .. } => {
                    apps.iter().position(|(_, id)| id == "dev.zed.Zed").unwrap()
                }
                _ => unreachable!(),
            };
            root.run_open_in_deed(OpenInDeed::App(at), &target, cx);
            assert_eq!(
                root.settings.open_in_target,
                OpenInTarget::App("dev.zed.Zed".into())
            );
            root.run_open_in_deed(OpenInDeed::CopyPath, &target, cx);
            assert_eq!(
                root.settings.open_in_target,
                OpenInTarget::App("dev.zed.Zed".into())
            );
            root.close_context_menu(window, cx);
            root.focus.focus(window, cx);
        })
    });
    cx.dispatch_action(crate::actions::OpenSelectedApp);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let launches = root.read(cx).open_in_launches.as_ref().unwrap();
        assert_eq!(
            &launches[2..],
            &vec![(OpenInTarget::App("dev.zed.Zed".into()), folder.clone()); 2]
        );
        guild_session::store::flush();
        let saved =
            guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
                .load()
                .unwrap();
        assert_eq!(
            saved.open_in_target,
            OpenInTarget::App("dev.zed.Zed".into())
        );
    });
    let next_folder = state.join("another worktree");
    std::fs::create_dir_all(&next_folder).unwrap();
    cx.update(|window, cx| {
        root.update(cx, |root, cx| {
            let project = root
                .desk
                .open_in(root.desk.active_workspace(), &next_folder);
            root.active_project = Some(project);
            root.focus.focus(window, cx);
        })
    });
    cx.dispatch_action(crate::actions::OpenSelectedApp);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            root.read(cx).open_in_launches.as_ref().unwrap().last(),
            Some(&(OpenInTarget::App("dev.zed.Zed".into()), next_folder))
        )
    });
    // The OS availability boundary changes after the first cached render.
    // No installed app is removed and every external launch remains intercepted.
    for trigger in ["click", "shortcut", "selector"] {
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.settings.open_in_target = OpenInTarget::App("dev.zed.Zed".into());
                root.open_in_apps = Some(vec![
                    crate::open_in::Connection::fixture("Finder", "com.apple.finder"),
                    crate::open_in::Connection::fixture("Zed", "dev.zed.Zed"),
                ]);
                root.open_in_availability = Some(Default::default());
                assert_eq!(
                    root.selected_open_in_target(),
                    OpenInTarget::App("dev.zed.Zed".into()),
                    "cached selection precedes interaction-time validation"
                );
                root.focus.focus(window, cx);
                cx.notify();
            })
        });
        cx.run_until_parked();
        match trigger {
            "click" => cx.simulate_click(primary.center(), Default::default()),
            "shortcut" => cx.simulate_keystrokes("cmd-o"),
            "selector" => cx.simulate_keystrokes("cmd-shift-o"),
            _ => unreachable!(),
        }
        cx.run_until_parked();
        cx.update(|window, cx| root.update(cx, |root, cx| {
            assert_eq!(root.selected_open_in_target(), OpenInTarget::Finder, "{trigger} must stop displaying a removed app");
            assert_eq!(root.settings.open_in_target, OpenInTarget::Finder, "fallback must become the current selection");
            assert!(!root.open_in_apps.as_ref().unwrap().iter().any(|row| row.id == "dev.zed.Zed"));
            if trigger == "selector" {
                assert!(matches!(root.context_menu.as_ref().map(|menu| &menu.target), Some(Target::OpenIn { apps, .. }) if apps.iter().all(|(_, id)| id == "com.apple.finder")));
                root.close_context_menu(window, cx);
            } else {
                assert_eq!(root.open_in_launches.as_ref().unwrap().last().unwrap().0, OpenInTarget::Finder);
            }
        }));
    }
    guild_session::store::flush();
    assert_eq!(
        guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
            .load()
            .unwrap()
            .open_in_target,
        OpenInTarget::Finder,
        "the interaction-time fallback is persisted too"
    );
}

#[gpui::test]
#[ignore = "isolated_selected_app_controls_and_dispatch owns private state"]
fn keyboard_selector_starts_at_first_choice_and_returns_editor_focus(cx: &mut TestAppContext) {
    let folder = crate::paths::state_dir().join("keyboard-worktree");
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join("note.txt"), "ordinary editor input").unwrap();
    let mut desk = guild_session::Desk::default();
    let project = desk.open_in(desk.active_workspace(), &folder);
    desk.look_at(Some(project), None);
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
        entity.update(cx, |root, cx| {
            root.isolated_render = true;
            root.open_in_launches = Some(vec![]);
            root.open_in_availability =
                Some(["com.mitchellh.ghostty".into(), "dev.zed.Zed".into()].into());
            root.open_in_apps = Some(vec![
                crate::open_in::Connection::fixture("Finder", "com.apple.finder"),
                crate::open_in::Connection::fixture("Ghostty", "com.mitchellh.ghostty"),
                crate::open_in::Connection::fixture("Zed", "dev.zed.Zed"),
            ]);
            root.settings.open_in_target = OpenInTarget::Terminal;
            root.open_file(std::path::Path::new("note.txt"), window, cx);
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    let frames = |cx: &mut gpui::VisualTestContext| {
        for _ in 0..4 {
            cx.update(|window, cx| window.simulate_next_frame(cx));
        }
        cx.run_until_parked();
    };
    frames(cx);
    let editor_focus = cx.update(|_, cx| {
        root.read(cx)
            .active_body()
            .unwrap()
            .focus_handle(cx)
            .unwrap()
    });
    assert!(cx.update(|window, _| editor_focus.is_focused(window)));
    cx.simulate_keystrokes("cmd-shift-o");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).context_menu.as_ref().unwrap().lit),
        Some(1),
        "keyboard opening immediately selects Finder, the first enabled choice, not the saved app"
    );
    cx.simulate_keystrokes("enter");
    frames(cx);
    cx.update(|window, cx| {
        assert!(root.read(cx).context_menu.is_none());
        assert_eq!(
            root.read(cx).open_in_launches.as_ref().unwrap().last(),
            Some(&(OpenInTarget::Finder, folder.clone()))
        );
        assert!(editor_focus.is_focused(window));
    });
    cx.simulate_keystrokes("cmd-shift-o down enter");
    frames(cx);
    cx.update(|window, cx| {
        assert_eq!(
            root.read(cx).open_in_launches.as_ref().unwrap().last(),
            Some(&(
                OpenInTarget::App("com.mitchellh.ghostty".into()),
                folder.clone()
            ))
        );
        assert!(editor_focus.is_focused(window));
    });
    cx.simulate_keystrokes("cmd-shift-o escape");
    frames(cx);
    assert!(cx.update(
        |window, cx| root.read(cx).context_menu.is_none() && editor_focus.is_focused(window)
    ));
    let chevron = cx.debug_bounds("titlebar-open-selector").unwrap();
    cx.simulate_click(chevron.center(), Default::default());
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).context_menu.as_ref().unwrap().lit),
        None,
        "mouse opening keeps existing hover semantics"
    );
    cx.simulate_keystrokes("escape");
    frames(cx);
    assert!(cx.update(
        |window, cx| root.read(cx).context_menu.is_none() && editor_focus.is_focused(window)
    ));
}
