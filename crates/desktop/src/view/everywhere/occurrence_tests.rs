use super::*;
use crate::root::{
    everywhere::Everywhere,
    picker::{Holds, Scroll},
};
use gpui::{AppContext, Entity, Render, TestAppContext};
struct Host {
    root: Entity<Root>,
    _changed: gpui::Subscription,
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.root.update(cx, |root, _| {
            div().size_full().child(root.render_everywhere_hit(0))
        })
    }
}
#[gpui::test]
fn equal_indexed_everywhere_results_clear_for_file_and_generation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.txt", "b.txt"] {
        std::fs::write(dir.path().join(name), "word word").unwrap();
    }
    let mut owner = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let project = desk.open_in(desk.active_workspace(), dir.path());
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
        root.update(cx, |root, cx| {
            root.theme = crate::theme::Theme::of(crate::theme::Kind::Light);
            root.active_project = Some(project);
            let mut popup = Everywhere::default();
            popup.among = crate::everywhere::Among::Text;
            root.open_picker(
                Holds::Everywhere(Box::new(popup)),
                Some(project),
                Scroll::uniform(),
                window,
                cx,
            );
        });
        owner = Some(root.clone());
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
    for (index, mask) in ["a.txt", "b.txt", "b.txt"].into_iter().enumerate() {
        cx.update(|_, cx| {
            owner.update(cx, |root, cx| {
                let Holds::Everywhere(popup) = &mut root.picker.as_mut().unwrap().holds else {
                    panic!()
                };
                popup.text.restart(
                    dir.path(),
                    crate::search::Query {
                        text: "word".into(),
                        mask: mask.into(),
                        ..Default::default()
                    },
                );
                cx.notify();
            })
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            cx.run_until_parked();
            let ready = cx.update(|_, cx| {
                owner.update(cx, |root, cx| {
                    let Holds::Everywhere(popup) = &mut root.picker.as_mut().unwrap().holds else {
                        panic!()
                    };
                    let moved = popup.text.take();
                    let ready = !popup.text.running() && popup.text.hits().len() == 1;
                    if moved {
                        cx.notify();
                    }
                    ready
                })
            });
            if ready {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        assert_fill(cx, 0);
        let bounds = cx.debug_bounds("source-snippet").unwrap();
        cx.simulate_click(
            gpui::point(bounds.left() + px(3.), bounds.center().y),
            Default::default(),
        );
        assert_fill(cx, 2);
        cx.update(|_, cx| owner.update(cx, |_, cx| cx.notify()));
        assert_fill(cx, 2);
        eprintln!("Everywhere source replacement {index} ready: {mask}");
    }
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
