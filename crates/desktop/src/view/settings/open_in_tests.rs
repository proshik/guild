//! Real Settings fields and controls, with private state and intercepted launches.
use super::*;
use gpui::{EntityInputHandler, TestAppContext, VisualTestContext, px};
use guild_session::settings::OpenInTarget;

#[test]
fn isolated_configured_apps_settings() {
    let state = tempfile::tempdir().unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "view::settings::open_in::tests::configured_apps_settings",
            "--ignored",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env(
            "GUILD_OPEN_IN_TEST_STATE",
            state.path().canonicalize().unwrap(),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
fn frames(cx: &mut VisualTestContext) {
    for _ in 0..4 {
        cx.update(|window, cx| window.simulate_next_frame(cx));
    }
    cx.run_until_parked();
}
fn reveal(
    cx: &mut VisualTestContext,
    root: &Entity<Root>,
    selector: &str,
) -> gpui::Bounds<gpui::Pixels> {
    // GPUI test selectors require static strings; this isolated fixture has bounded queries.
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    frames(cx);
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    let viewport = cx.update(|window, _| window.viewport_size());
    if bounds.bottom() > viewport.height - px(40.) || bounds.top() < px(100.) {
        cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                let offset = root.settings_scroll.offset();
                let delta = if bounds.bottom() > viewport.height - px(40.) {
                    bounds.bottom() - (viewport.height - px(40.))
                } else {
                    bounds.top() - px(100.)
                };
                root.settings_scroll
                    .set_offset(gpui::point(offset.x, offset.y - delta));
                cx.notify();
            })
        });
        frames(cx);
    }
    cx.debug_bounds(selector).unwrap()
}
#[gpui::test]
#[ignore = "isolated_configured_apps_settings supplies private state"]
fn configured_apps_settings(cx: &mut TestAppContext) {
    let allowed = std::path::PathBuf::from(
        std::env::var_os("GUILD_OPEN_IN_TEST_STATE")
            .expect("run isolated_configured_apps_settings"),
    );
    assert_eq!(crate::paths::state_dir().canonicalize().unwrap(), allowed);
    assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").unwrap(), "1");
    struct Flush;
    impl Drop for Flush {
        fn drop(&mut self) {
            guild_session::store::flush();
        }
    }
    let _flush = Flush;
    let finder =
        crate::system::application_bundle(&crate::system::application("com.apple.finder").unwrap())
            .unwrap();
    let terminal = crate::system::application_bundle(
        &crate::system::application("com.apple.Terminal").unwrap(),
    )
    .unwrap();
    let inventory = vec![finder, terminal.clone()];
    let folder = crate::paths::state_dir().join("settings-worktree");
    std::fs::create_dir_all(&folder).unwrap();
    let mut desk = guild_session::Desk::default();
    let project = desk.open_in(desk.active_workspace(), &folder);
    desk.look_at(Some(project), None);
    let mut root = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let entity = cx.new(|cx| Root::diagnostic(desk, crate::paths::state_dir(), cx));
        entity.update(cx, |root, _| {
            root.isolated_render = true;
            root.open_in_launches = Some(vec![]);
            root.open_in_availability =
                Some(["com.mitchellh.ghostty".into(), "com.apple.Terminal".into()].into());
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    let root = root.unwrap();
    cx.simulate_resize(gpui::size(px(1160.), px(900.)));
    frames(cx);
    cx.simulate_keystrokes("cmd-,");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).settings_in_front()),
        Some(super::super::Section::General)
    );
    assert!(
        cx.update(|_, cx| root
            .read(cx)
            .open_in_inventory
            .as_ref()
            .is_some_and(|apps| !apps.is_empty())),
        "Cmd+, loads actual bounded inventory asynchronously"
    );
    cx.update(|_, cx| {
        root.update(cx, |root, cx| {
            root.open_in_inventory = Some(std::sync::Arc::new(inventory));
            cx.notify();
        })
    });
    frames(cx);
    let search = cx.debug_bounds("settings-search").unwrap();
    cx.simulate_click(search.center(), Default::default());
    frames(cx);
    cx.simulate_keystrokes("o p e n space i n");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).settings_query.clone()),
        "open in"
    );
    let down = reveal(cx, &root, "open-in-move-com.apple.finder-true");
    cx.simulate_click(down.center(), Default::default());
    frames(cx);
    cx.simulate_keystrokes("cmd-shift-o");
    frames(cx);
    cx.update(|_,cx|assert!(matches!(&root.read(cx).context_menu.as_ref().unwrap().target,crate::view::menu::Target::OpenIn{apps,..} if apps[0].1=="com.mitchellh.ghostty")));
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).selected_open_in_target()),
        OpenInTarget::App("com.mitchellh.ghostty".into())
    );
    let toggle = reveal(cx, &root, "open-in-enabled-com.mitchellh.ghostty");
    cx.simulate_click(toggle.center(), Default::default());
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).selected_open_in_target()),
        OpenInTarget::Finder
    );
    cx.simulate_click(toggle.center(), Default::default());
    frames(cx);
    let add = reveal(cx, &root, "open-in-add-app");
    let icon = cx
        .debug_bounds("open-in-add-icon")
        .expect("visible plus box");
    let label = cx.debug_bounds("open-in-add-label").expect("label box");
    assert!(icon.size.width > px(0.) && label.size.width > px(0.));
    assert!(icon.right() < label.left(), "horizontal icon then label");
    assert!((icon.center().y - label.center().y).abs() < px(1.));
    assert!(
        ((icon.left() - add.left()) - (add.right() - label.right())).abs() < px(1.),
        "equal edge padding"
    );
    cx.simulate_click(add.center(), Default::default());
    frames(cx);
    let key = cx.update(|_, cx| root.read(cx).settings.open_in_apps.custom[0].id.clone());
    assert!(
        cx.debug_bounds(Box::leak(format!("open-in-name-{key}").into_boxed_str()))
            .unwrap()
            .bottom()
            <= px(900.),
        "Add app reveals the new editable row"
    );
    // Creating an optional name does not take typing focus from the host.
    cx.update(|window, cx| {
        use gpui_component::{WindowExt, input::AnyInputState};
        if let Some(AnyInputState::Input(input)) = window.focused_input(cx) {
            assert_eq!(
                input.read(cx).value().as_str(),
                "open in",
                "new optional name must remain passive"
            );
        }
    });
    let name = reveal(cx, &root, &format!("open-in-name-{key}"));
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_keystrokes("M y space T e r m");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).settings.open_in_apps.custom[0].name.clone()),
        "My Term"
    );
    let choose = reveal(cx, &root, &format!("open-in-choose-{key}"));
    let previous = cx.update(|window, cx| window.focused(cx).unwrap());
    cx.simulate_click(choose.center(), Default::default());
    frames(cx);
    reveal(cx, &root, "open-in-application-picker");
    cx.simulate_keystrokes("down");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).open_in_picker.as_ref().unwrap().selected),
        1
    );
    cx.simulate_keystrokes("escape");
    frames(cx);
    assert!(
        cx.update(
            |window, cx| root.read(cx).open_in_picker.is_none() && previous.is_focused(window)
        ),
        "selector Escape restores the prior input owner"
    );
    let choose = reveal(cx, &root, &format!("open-in-choose-{key}"));
    cx.simulate_click(choose.center(), Default::default());
    frames(cx);
    reveal(cx, &root, "open-in-application-picker");
    cx.simulate_keystrokes("t e r");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root.read(cx).open_in_picker.as_ref().unwrap().query.clone()),
        "ter"
    );
    let input = cx.update(|_, cx| root.read(cx).open_in_picker.as_ref().unwrap().input.clone());
    cx.simulate_keystrokes("cmd-a");
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "Ter", Some(3..3), window, cx)
        })
    });
    frames(cx);
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert!(
        cx.update(|_, cx| root.read(cx).open_in_picker.is_some()),
        "IME confirmation must not select an app prematurely"
    );
    cx.update(|window, cx| {
        input.update(cx, |input, cx| {
            input.replace_text_in_range(None, "Terminal", window, cx);
            input.unmark_text(window, cx);
        })
    });
    frames(cx);
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert!(cx.update(|_, cx| root.read(cx).open_in_picker.is_none()));
    cx.update(|_, cx| {
        let r = root.read(cx);
        let custom = &r.settings.open_in_apps.custom[0];
        assert_eq!(custom.name, "My Term");
        assert_eq!(custom.path.as_ref(), Some(&terminal.path));
        let icons = r.open_in_icons.borrow();
        assert!(
            std::sync::Arc::ptr_eq(
                icons.get(&key).expect("custom icon"),
                icons.get("com.apple.Terminal").expect("builtin icon")
            ),
            "settings/menu/titlebar share the actual path-cached image"
        );
    });
    let name = reveal(cx, &root, &format!("open-in-name-{key}"));
    cx.simulate_click(name.center(), Default::default());
    cx.simulate_keystrokes("cmd-a S h e l l");
    frames(cx);
    guild_session::store::flush();
    let saved = guild_session::Store::<guild_session::Settings>::new(crate::paths::settings_file())
        .load()
        .unwrap();
    assert_eq!(saved.open_in_apps.custom[0].name, "Shell");
    assert_eq!(
        saved.open_in_apps.custom[0].path.as_ref(),
        Some(&terminal.path)
    );
    assert_eq!(
        saved.open_in_apps.keys(&crate::open_in::default_keys())[0],
        "com.mitchellh.ghostty"
    );
    cx.simulate_keystrokes("cmd-shift-o");
    frames(cx);
    let at = cx.update(
        |_, cx| match &root.read(cx).context_menu.as_ref().unwrap().target {
            crate::view::menu::Target::OpenIn { apps, .. } => {
                apps.iter().position(|(_, id)| id == &key).unwrap()
            }
            _ => unreachable!(),
        },
    );
    for _ in 0..at {
        cx.simulate_keystrokes("down");
    }
    cx.simulate_keystrokes("enter");
    frames(cx);
    assert_eq!(
        cx.update(|_, cx| root
            .read(cx)
            .open_in_launches
            .as_ref()
            .unwrap()
            .last()
            .unwrap()
            .0
            .clone()),
        OpenInTarget::App(key.clone())
    );
    let remove = reveal(cx, &root, &format!("open-in-remove-{key}"));
    cx.simulate_click(remove.center(), Default::default());
    frames(cx);
    assert!(cx.update(|_, cx| root.read(cx).settings.open_in_apps.custom.is_empty()));
    assert_eq!(
        cx.update(|_, cx| root.read(cx).selected_open_in_target()),
        OpenInTarget::Finder
    );
    assert!(
        cx.debug_bounds(Box::leak(format!("open-in-choose-{key}").into_boxed_str()))
            .is_none()
    );
    let add = reveal(cx, &root, "open-in-add-app");
    cx.simulate_click(add.center(), Default::default());
    frames(cx);
    let key = cx.update(|_, cx| root.read(cx).settings.open_in_apps.custom[0].id.clone());
    let choose = reveal(cx, &root, &format!("open-in-choose-{key}"));
    cx.simulate_click(choose.center(), Default::default());
    frames(cx);
    assert!(cx.update(|_, cx| root.read(cx).open_in_picker.is_some()));
    let done = cx.debug_bounds("settings-done").unwrap();
    cx.simulate_click(done.center(), Default::default());
    frames(cx);
    assert!(
        cx.update(|_, cx| root.read(cx).settings_in_front().is_none()
            && root.read(cx).open_in_picker.is_none()),
        "closing Settings drops its temporary selector input"
    );
    cx.simulate_keystrokes("cmd-,");
    frames(cx);
    assert!(
        cx.update(|_, cx| root.read(cx).settings_in_front().is_some()
            && root.read(cx).open_in_picker.is_none())
    );
}

#[test]
fn settings_rows_keep_installed_disabled_and_missing_custom_in_saved_order() {
    let mut config = guild_session::open_in::Config::default();
    let missing = config.add_custom();
    config.rename(&missing, "Missing custom".into());
    config.set_enabled("com.apple.Terminal", false);
    config.order = vec![
        "com.apple.Terminal".into(),
        "com.jetbrains.CLion".into(),
        missing.clone(),
        "com.apple.finder".into(),
    ];
    let saved = config.clone();
    let apps = vec![
        crate::open_in::Connection::fixture("Finder", "com.apple.finder"),
        crate::open_in::Connection::fixture("Terminal", "com.apple.Terminal"),
    ];
    assert_eq!(
        settings_app_keys(&config, Some(&apps)),
        vec![
            "com.apple.Terminal".to_owned(),
            missing.clone(),
            "com.apple.finder".to_owned()
        ]
    );
    assert_eq!(
        settings_app_keys(&config, None),
        vec![missing],
        "pending detection shows custom rows without unavailable catalog placeholders"
    );
    assert_eq!(
        config, saved,
        "view filtering must not mutate stored preferences"
    );
}
