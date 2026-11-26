use super::*;
use gpui::{AppContext, TestAppContext, px, size};

#[test]
fn isolated_stable_diff_window_transitions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "view::diff_window::tests::stable_diff_window_transitions",
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
#[ignore = "isolated_stable_diff_window_transitions owns private state"]
fn stable_diff_window_transitions(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let folder = tempfile::tempdir().unwrap();
    let mut root = None;
    let mut satellite = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        crate::actions::install(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), folder.path());
        let owner = cx.new(|cx| Root::from_state(Default::default(), desk, cx));
        let view = cx.new(|cx| {
            DiffWindow::new(
                owner.downgrade(),
                Showing {
                    project,
                    folder: folder.path().to_path_buf(),
                    sha: "fixture-revision".into(),
                    files: vec![
                        "a/first.txt".into(),
                        "b/second.txt".into(),
                        "c/third.txt".into(),
                    ],
                    index: 0,
                },
                Default::default(),
                cx,
            )
        });
        root = Some(owner);
        satellite = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(size(px(1000.), px(650.)));
    cx.update(|window, _| window.activate_window());
    let satellite = satellite.unwrap();
    let tab = cx.update(|_, cx| satellite.read(cx).tab.clone());
    tab.update(cx, |tab, cx| {
        if tab.mode() != guild_session::DiffView::Unified {
            tab.toggle_mode(cx);
        }
        tab.viewer.ratio = 0.63;
        tab.toggle_authors(0, cx);
    });
    cx.run_until_parked();
    let before = cx.debug_bounds("diff-file-next").unwrap();
    cx.simulate_click(before.center(), Default::default());
    cx.run_until_parked();
    let after = cx.debug_bounds("diff-file-next").unwrap();
    cx.update(|_, cx| {
        let view = satellite.read(cx);
        assert_eq!(view.showing.index, 1);
        assert_eq!(
            view.tab.entity_id(),
            tab.entity_id(),
            "file stepping must reuse the diff entity"
        );
        assert_eq!(view.tab.read(cx).mode(), guild_session::DiffView::Unified);
        assert_eq!(view.tab.read(cx).viewer.ratio, 0.63);
        assert_eq!(view.tab.read(cx).authors.enabled, [true, false]);
    });
    assert_eq!(before, after, "file controls must not move during loading");
    let counter = cx.debug_bounds("diff-file-counter").unwrap();
    cx.simulate_click(counter.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("diff-file-picker").is_some());
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(
        cx.cx.windows().len(),
        1,
        "picker Escape must not close the actual satellite"
    );
    cx.simulate_click(counter.center(), Default::default());
    cx.run_until_parked();
    let choice = cx.debug_bounds("diff-file-choice-2").unwrap();
    cx.simulate_click(choice.center(), Default::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        let view = satellite.read(cx);
        assert_eq!(view.showing.index, 2);
        assert_eq!(view.showing.sha, "fixture-revision");
        assert_eq!(view.tab.entity_id(), tab.entity_id());
        assert_eq!(view.tab.read(cx).stepping(), Some((2, 3)));
        assert_eq!(view.tab.read(cx).inspection_paths, view.showing.files);
        assert_eq!(view.tab.read(cx).mode(), guild_session::DiffView::Unified);
        assert_eq!(view.tab.read(cx).viewer.ratio, 0.63);
        assert_eq!(view.tab.read(cx).authors.enabled, [true, false]);
    });
    cx.simulate_click(counter.center(), Default::default());
    cx.simulate_keystrokes("home down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(satellite.read(cx).showing.index, 0));
    tab.update(cx, |_, cx| {
        cx.emit(DiffEvent::SelectFile {
            index: 2,
            key: "superseded-inspection".into(),
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            satellite.read(cx).showing.index,
            0,
            "a stale popup cannot select a file in another revision"
        )
    });
    drop(root);
}
