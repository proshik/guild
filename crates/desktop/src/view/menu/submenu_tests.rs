//! Production Explorer context menu; fixture state and synthetic pointer input only.
use super::*;
use crate::Root;
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, point, px};

#[test]
fn isolated_shared_submenu_geometry() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::menu::submenu_tests::",
            "--ignored",
            "--test-threads=1",
            "--nocapture",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_SUBMENU_TEST", "1")
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn mount(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Root>, &mut VisualTestContext) {
    assert_eq!(std::env::var("GUILD_SUBMENU_TEST").as_deref(), Ok("1"));
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    let mut root = None;
    let (_, cx) = cx.add_window_view(|w, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let entity = cx.new(|cx| {
            let mut r = Root::from_state(guild_session::Settings::default(), desk, cx);
            r.active_project = Some(project);
            r.isolated_render = true;
            r.ensure_active_tree(cx);
            r.open_menu(
                Target::TreeRow {
                    path: "src".into(),
                    is_dir: true,
                    expanded: false,
                },
                Anchor::Pointer(point(px(250.), px(80.))),
                w,
                cx,
            );
            r
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, w, cx)
    });
    cx.simulate_resize(gpui::size(px(1000.), px(800.)));
    cx.run_until_parked();
    (dir, root.unwrap(), cx)
}
fn move_to(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect("parent row drawn");
    cx.simulate_mouse_move(bounds.center(), None, Default::default());
}
fn assert_beside(cx: &mut VisualTestContext, selector: &'static str) {
    let row = cx.debug_bounds(selector).unwrap();
    let child = cx
        .debug_bounds("context-menu-child")
        .expect("stationary hover must draw the child without another event");
    assert!(
        (child.top() - row.top()).abs() <= px(1.),
        "child {child:?} must hang at current row {row:?}"
    );
    assert!(
        (child.left() - row.right()).abs() <= px(1.),
        "current row {row:?} owns child {child:?}"
    );
}
#[gpui::test]
#[ignore = "isolated_shared_submenu_geometry supplies private state"]
fn explorer_hover_uses_current_row_without_another_input(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    // Directory menu New is row 0, Open In row 3: production builder, not fake entries.
    for (index, selector) in [
        (0, "menu-submenu-0"),
        (3, "menu-submenu-3"),
        (0, "menu-submenu-0"),
        (3, "menu-submenu-3"),
        (0, "menu-submenu-0"),
    ] {
        move_to(cx, selector);
        root.read_with(cx, |r, _| {
            let open = r.context_menu.as_ref().unwrap();
            assert_eq!(open.lit, Some(index));
            assert_eq!(open.submenu, Some(index));
        });
        assert_beside(cx, selector);
        let (present, absent) = if index == 0 {
            ("popup-label-File…", "popup-label-Reveal in Finder")
        } else {
            ("popup-label-Reveal in Finder", "popup-label-File…")
        };
        assert!(
            cx.debug_bounds(present).is_some() && cx.debug_bounds(absent).is_none(),
            "visible content must belong to the highlighted parent"
        );
    }
}

#[gpui::test]
#[ignore = "isolated_shared_submenu_geometry supplies private state"]
fn pointer_child_entry_normal_row_and_reopening_keep_ownership(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    move_to(cx, "menu-submenu-0");
    assert_beside(cx, "menu-submenu-0");
    let parent = cx.debug_bounds("menu-submenu-0").unwrap();
    cx.simulate_mouse_move(
        point(parent.right() + px(1.), parent.center().y),
        None,
        Default::default(),
    );
    assert_beside(cx, "menu-submenu-0");
    move_to(cx, "menu-sub-item-0");
    root.read_with(cx, |r, _| {
        let open = r.context_menu.as_ref().unwrap();
        assert_eq!(open.lit, Some(0));
        assert_eq!(open.sub_lit, Some(0));
        assert!(open.in_submenu);
    });
    move_to(cx, "menu-item-2");
    assert!(cx.debug_bounds("context-menu-child").is_none());
    root.read_with(cx, |r, _| {
        let o = r.context_menu.as_ref().unwrap();
        assert_eq!(o.lit, Some(2));
        assert_eq!(o.submenu, None);
        assert!(!o.in_submenu);
    });
    cx.simulate_keystrokes("escape");
    assert!(root.read_with(cx, |r, _| r.context_menu.is_none()));
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.open_menu(
                Target::TreeRow {
                    path: "src".into(),
                    is_dir: true,
                    expanded: false,
                },
                Anchor::Pointer(point(px(430.), px(160.))),
                w,
                cx,
            )
        })
    });
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("context-menu-child").is_none(),
        "reopening carries no previous row anchor or child"
    );
    move_to(cx, "menu-submenu-3");
    assert_beside(cx, "menu-submenu-3");
}

#[gpui::test]
#[ignore = "isolated_shared_submenu_geometry supplies private state"]
fn click_and_keyboard_keep_existing_submenu_actions(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    let row = cx.debug_bounds("menu-submenu-0").unwrap();
    cx.simulate_mouse_down(row.center(), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_up(row.center(), gpui::MouseButton::Left, Default::default());
    assert_beside(cx, "menu-submenu-0");
    assert!(root.read_with(cx, |r, _| r.context_menu.as_ref().unwrap().in_submenu));
    cx.simulate_keystrokes("left");
    assert!(cx.debug_bounds("context-menu-child").is_none());
    cx.simulate_keystrokes("right");
    assert_beside(cx, "menu-submenu-0");
    cx.simulate_keystrokes("escape");
    assert!(root.read_with(cx, |r, _| r.context_menu.is_some()));
    cx.simulate_keystrokes("enter");
    assert_beside(cx, "menu-submenu-0");
    cx.simulate_keystrokes("enter"); // File… opens Guild's existing inline form, writes nothing.
    root.read_with(cx, |r, _| {
        assert!(r.context_menu.is_none());
        assert!(r.tree().unwrap().naming.is_some());
    });
}

#[gpui::test]
#[ignore = "isolated_shared_submenu_geometry supplies private state"]
fn current_row_anchor_follows_scroll_resize_and_window_snap(cx: &mut TestAppContext) {
    let (_dir, root, cx) = mount(cx);
    cx.update(|_, cx| {
        root.update(cx, |r, cx| {
            r.context_menu.as_mut().unwrap().cap = Some(gpui::rems(10.));
            cx.notify();
        })
    });
    cx.run_until_parked();
    move_to(cx, "menu-submenu-3");
    assert_beside(cx, "menu-submenu-3");
    let before = cx.debug_bounds("menu-submenu-3").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: before.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-20.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    let after = cx.debug_bounds("menu-submenu-3").unwrap();
    assert!(
        after.top() < before.top(),
        "the real wheel event must scroll the main list"
    );
    assert_beside(cx, "menu-submenu-3");
    let main = cx.debug_bounds("context-menu-main").unwrap();
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: main.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-500.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    assert!(
        cx.debug_bounds("context-menu-child").is_none(),
        "a fully clipped parent cannot own a visible child"
    );
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: main.center(),
        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(500.))),
        modifiers: Default::default(),
        touch_phase: gpui::TouchPhase::Moved,
    });
    assert_beside(cx, "menu-submenu-3"); // scrolling the owner back restores its child without a pointer move

    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    cx.run_until_parked();
    let viewport = cx.update(|w, _| w.viewport_size());
    cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.open_menu(
                Target::TreeRow {
                    path: "src".into(),
                    is_dir: true,
                    expanded: false,
                },
                Anchor::Pointer(point(viewport.width - px(10.), viewport.height - px(10.))),
                w,
                cx,
            )
        })
    });
    cx.run_until_parked();
    move_to(cx, "menu-submenu-3");
    let parent = cx.debug_bounds("menu-submenu-3").unwrap();
    let child = cx.debug_bounds("context-menu-child").unwrap();
    let expected = point(
        parent
            .right()
            .min(viewport.width - px(MARGIN) - child.size.width)
            .max(px(MARGIN)),
        parent
            .top()
            .min(viewport.height - px(MARGIN) - child.size.height)
            .max(px(MARGIN)),
    );
    assert!(
        (child.left() - expected.x).abs() <= px(1.) && (child.top() - expected.y).abs() <= px(1.),
        "current row geometry is snapped once to the constrained viewport: {parent:?}, {child:?}, {expected:?}"
    );
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    assert_beside(cx, "menu-submenu-3"); // no new input needed to re-anchor after resize
}

#[gpui::test]
#[ignore = "isolated_shared_submenu_geometry supplies private state"]
fn repository_destination_submenu_keeps_intrinsic_width(cx: &mut TestAppContext) {
    let (dir, root, cx) = mount(cx);
    cx.simulate_resize(gpui::size(px(1600.), px(900.)));
    let name = "Workspace with a deliberately long destination name for submenu sizing";
    let index = cx.update(|w, cx| {
        root.update(cx, |r, cx| {
            r.desk.add_workspace(name, None).unwrap();
            let workspace = r.desk.active_workspace();
            let project = r.active_project.unwrap();
            r.open_menu(
                Target::Repository {
                    workspace,
                    projects: vec![project],
                    key: dir.path().into(),
                    folder: dir.path().into(),
                    name: "fixture".into(),
                },
                Anchor::Pointer(point(px(250.), px(80.))),
                w,
                cx,
            );
            r.entries_for_open(r.context_menu.as_ref().unwrap(), cx)
                .iter()
                .position(|e| matches!(e, Entry::Submenu(Text::MenuMoveToWorkspace, _)))
                .unwrap()
        })
    });
    cx.run_until_parked();
    let selector: &'static str = Box::leak(format!("menu-submenu-{index}").into_boxed_str());
    move_to(cx, selector);
    assert_beside(cx, selector);
    let child = cx.debug_bounds("context-menu-child").unwrap();
    assert!(
        child.size.width > px(300.),
        "a long destination must retain its intrinsic menu width, not the 180px minimum: {child:?}"
    );
    let label: &'static str = Box::leak(format!("popup-label-{name}").into_boxed_str());
    assert!(cx.debug_bounds(label).is_some());
}
