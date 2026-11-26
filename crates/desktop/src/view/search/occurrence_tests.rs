use super::*;
use gpui::{AppContext, Entity, Render, TestAppContext};
struct Host {
    root: Entity<Root>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, cx| {
            div()
                .size_full()
                .flex()
                .flex_col()
                .child(root.render_search_results(cx))
        })
    }
}
#[gpui::test]
fn prepared_equal_search_results_do_not_share_occurrences_across_projects(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    for folder in [&a, &b] {
        std::fs::create_dir(folder).unwrap();
        std::fs::write(folder.join("same.txt"), "word word").unwrap();
    }
    let mut owner = None;
    let mut projects = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let first = desk.open_in(desk.active_workspace(), &a);
        let second = desk.open_in(desk.active_workspace(), &b);
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
        root.update(cx, |root, cx| {
            root.theme = crate::theme::Theme::of(crate::theme::Kind::Light);
            for project in [first, second] {
                root.active_project = Some(project);
                root.ensure_search(window, cx);
                root.searches.get_mut(&project).unwrap().query.text = "word".into();
                root.restart_search(project, cx);
            }
            root.active_project = Some(first);
        });
        owner = Some(root.clone());
        projects = Some((first, second));
        let host = cx.new(|cx| {
            let changed = cx.observe(&root, |_, _, cx| cx.notify());
            Host {
                root,
                _changed: changed,
            }
        });
        gpui_component::Root::new(host, window, cx)
    });
    let owner = owner.unwrap();
    let (first, second) = projects.unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        cx.run_until_parked();
        let ready = cx.update(|_, cx| {
            owner.update(cx, |root, cx| {
                root.poll_search(cx);
                [first, second].iter().all(|p| {
                    !root.searches[p].walk.running() && root.searches[p].walk.hits().len() == 1
                })
            })
        });
        if ready {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    cx.run_until_parked();
    let bounds = cx.debug_bounds("source-snippet").unwrap();
    cx.simulate_click(
        gpui::point(bounds.left() + px(3.), bounds.center().y),
        Default::default(),
    );
    assert_fill(cx, 2);
    cx.update(|_, cx| owner.update(cx, |_, cx| cx.notify()));
    assert_fill(cx, 2);
    cx.update(|_, cx| {
        owner.update(cx, |root, cx| {
            root.active_project = Some(second);
            cx.notify();
        })
    });
    assert_fill(cx, 0);
    let bounds = cx.debug_bounds("source-snippet").unwrap();
    cx.simulate_click(
        gpui::point(bounds.left() + px(3.), bounds.center().y),
        Default::default(),
    );
    assert_fill(cx, 2);
    cx.update(|_, cx| owner.update(cx, |root, cx| root.restart_search(second, cx)));
    loop {
        cx.run_until_parked();
        let ready = cx.update(|_, cx| {
            owner.update(cx, |root, cx| {
                root.poll_search(cx);
                !root.searches[&second].walk.running()
            })
        });
        if ready {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    assert_fill(cx, 0);
}
fn assert_fill(cx: &mut gpui::VisualTestContext, expected: usize) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let color: gpui::Hsla = crate::theme::Theme::of(crate::theme::Kind::Light)
            .identifier_occurrence()
            .into();
        assert_eq!(
            window
                .painted_quads()
                .iter()
                .filter(|q| q.background.as_solid() == Some(color))
                .count(),
            expected
        );
    });
}
