//! GLD-080: the empty window's rows are deeds a click reaches — *Open…*
//! first, the same action as *File ▸ Open…* — not only hints for the keys.

use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};

fn empty_window(cx: &mut TestAppContext) -> (Entity<crate::Root>, &mut VisualTestContext) {
    assert_eq!(std::env::var("GUILD_SPEED_SEARCH_TEST").as_deref(), Ok("1"));
    let mut root = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let entity = cx.new(|cx| {
            let mut root = crate::Root::from_state(
                guild_session::Settings {
                    panels: Some(Default::default()),
                    ..Default::default()
                },
                desk,
                cx,
            );
            // Nothing in front: the window draws its empty screen.
            root.active_project = None;
            root
        });
        root = Some(entity.clone());
        gpui_component::Root::new(entity, window, cx)
    });
    visual.update(|window, _| window.activate_window());
    visual.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
    visual.run_until_parked();
    (root.unwrap(), visual)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("no row {selector}"));
    cx.simulate_click(bounds.center(), gpui::Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn the_empty_window_s_rows_answer_a_click(cx: &mut TestAppContext) {
    let (root, cx) = empty_window(cx);
    cx.update(|_, cx| {
        assert!(root.read(cx).active_project.is_none());
        assert!(root.read(cx).settings_page.is_none());
    });
    click(cx, "start-hint-Settings");
    cx.update(|_, cx| assert!(root.read(cx).settings_page.is_some(), "Settings opened"));
}

#[gpui::test]
#[ignore = "run through isolated_speed_search_window_regressions"]
fn the_empty_window_opens_a_folder_from_its_first_row(cx: &mut TestAppContext) {
    let (_root, cx) = empty_window(cx);
    assert!(!cx.did_prompt_for_paths());
    click(cx, "start-hint-Open…");
    assert!(cx.did_prompt_for_paths(), "Open… asks for a folder");
}
