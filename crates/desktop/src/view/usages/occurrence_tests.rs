//! Host routing is private; all assertions exercise the rendered source paths.
use super::*;
use crate::root::preview::{Asked, Slot};
use gpui::{AppContext, Entity, Render, TestAppContext};
struct Host {
    root: Entity<Root>,
    mode: u8,
    clicks: std::rc::Rc<std::cell::Cell<usize>>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, _: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;
        let clicks = self.clicks.clone();
        self.root.update(cx, |root, cx| {
            let hit = crate::search::Hit {
                path: "sample.txt".into(),
                line: 1,
                column: 0,
                cut: false,
                text: "имя имя имя_2".into(),
                ranges: std::iter::once(0..6).collect(),
            };
            let content = match mode {
                0 => root
                    .render_preview(cx)
                    .unwrap_or_else(|| div().into_any_element()),
                1 => root.render_find_preview(),
                2 => crate::view::search::painted(&hit, "fixture-search".into(), root.theme),
                _ => crate::view::search::painted_bold(&hit, "fixture-usages".into(), root.theme),
            };
            div()
                .id("source-host")
                .size_full()
                .flex()
                .flex_col()
                .font_family("Menlo")
                .text_size(px(14.))
                .child(content)
                .on_click(move |_, _, _| clicks.set(clicks.get() + 1))
        })
    }
}
#[gpui::test]
fn popup_find_and_search_hosts_keep_interactive_source_and_activation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("sample.txt"), "имя имя_2\nимя Имя").unwrap();
    let mut owner = None;
    let mut host_owner = None;
    let clicks = std::rc::Rc::new(std::cell::Cell::new(0));
    let (host, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
        root.update(cx, |root, cx| {
            root.active_project = Some(project);
            root.theme = theme::Theme::of(theme::Kind::Light);
            let asked = Asked {
                project,
                origin: crate::Origin::Project,
                path: "sample.txt".into(),
                line: 0,
                hit: None,
            };
            root.ask_preview_in(Slot::Popup, Some(asked.clone()), 5, cx);
            root.ask_preview_in(Slot::Find, Some(asked), 5, cx);
        });
        owner = Some(root.clone());
        let host = cx.new(|cx| {
            let changed = cx.observe(&root, |_, _, cx| cx.notify());
            Host {
                root,
                mode: 0,
                clicks: clicks.clone(),
                _changed: changed,
            }
        });
        host_owner = Some(host.clone());
        gpui_component::Root::new(host, window, cx)
    });
    cx.run_until_parked();
    // The host entity is inside the toolkit root; retain the real Root for lifecycle checks.
    let _ = host;
    let owner = owner.unwrap();
    let host_owner = host_owner.unwrap();
    for mode in 0..4 {
        cx.update(|_, cx| {
            host_owner.update(cx, |host, cx| {
                host.mode = mode;
                cx.notify();
            })
        });
        cx.run_until_parked();
        let bounds = cx.debug_bounds("source-snippet").expect("host source");
        cx.simulate_click(
            point(bounds.left() + px(3.), bounds.center().y),
            Default::default(),
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            let color: gpui::Hsla = theme::Theme::of(theme::Kind::Light)
                .identifier_occurrence()
                .into();
            assert_eq!(
                window
                    .painted_quads()
                    .iter()
                    .filter(|q| q.background.as_solid() == Some(color))
                    .count(),
                2,
                "host mode {mode}"
            );
        });
        assert_eq!(
            clicks.get(),
            mode as usize + 1,
            "source handling must leave activation bubbling"
        );
    }
    cx.update(|_, cx| {
        host_owner.update(cx, |host, cx| {
            host.mode = 0;
            cx.notify();
        })
    });
    cx.update(|_, cx| owner.update(cx, |root, cx| root.ask_preview(None, cx)));
    cx.run_until_parked();
    assert!(cx.debug_bounds("source-snippet").is_none());
}
