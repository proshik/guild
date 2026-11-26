use gpui::{TestAppContext, div, prelude::*, px};
use guild_desktop::{
    source_snippet::SourceSnippet,
    theme::{Kind, Theme},
};
struct Snippet {
    theme: Theme,
    text: String,
    identity: String,
}
impl gpui::Render for Snippet {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .font_family("Menlo")
            .text_size(px(16.))
            .child(
                SourceSnippet::new("source-code", self.text.clone(), vec![], self.theme)
                    .identity(self.identity.clone()),
            )
    }
}
#[gpui::test]
fn snippet_click_paints_whole_unicode_mentions_in_light(cx: &mut TestAppContext) {
    let (_view, cx) = cx.add_window_view(|_, _| Snippet {
        identity: "source-a".into(),
        theme: Theme::of(Kind::Light),
        text: "имя имя имя_2 Имя".into(),
    });
    cx.run_until_parked();
    cx.simulate_click(gpui::point(px(4.), px(8.)), Default::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let color: gpui::Hsla = Theme::of(Kind::Light).identifier_occurrence().into();
        assert_eq!(
            window
                .painted_quads()
                .iter()
                .filter(|q| q.background.as_solid() == Some(color))
                .count(),
            2
        );
    });
}

// Shared route: the source stays read-only and leaves activation/focus to its host.
#[gpui::test]
fn snippet_bounds_drag_replacement_and_limit_recovery(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Snippet {
        identity: "source-a".into(),
        theme: Theme::of(Kind::Dark),
        text: "a;".repeat(4097),
    });
    let at = gpui::point(px(3.), px(8.));
    cx.run_until_parked();
    cx.simulate_click(at, Default::default());
    assert_fill(cx, Theme::of(Kind::Dark), 0);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.text = "a a".into();
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.simulate_click(at, Default::default());
    assert_fill(cx, Theme::of(Kind::Dark), 2);
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.theme = Theme::of(Kind::Light);
            cx.notify();
        })
    });
    assert_fill(cx, Theme::of(Kind::Light), 2);
    cx.simulate_mouse_down(at, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(
        gpui::point(px(500.), px(200.)),
        Some(gpui::MouseButton::Left),
        Default::default(),
    );
    assert_fill(cx, Theme::of(Kind::Light), 0);
    cx.simulate_mouse_up(
        gpui::point(px(500.), px(200.)),
        gpui::MouseButton::Left,
        Default::default(),
    );
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.text = "b b".into();
            cx.notify();
        })
    });
    assert_fill(cx, Theme::of(Kind::Light), 0);
    cx.simulate_click(at, Default::default());
    assert_fill(cx, Theme::of(Kind::Light), 2);
    cx.simulate_click(gpui::point(px(400.), px(8.)), Default::default());
    assert_fill(cx, Theme::of(Kind::Light), 0);
}
fn assert_fill(cx: &mut gpui::VisualTestContext, theme: Theme, expected: usize) {
    cx.run_until_parked();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
        let color: gpui::Hsla = theme.identifier_occurrence().into();
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

#[gpui::test]
fn equal_text_source_identity_changes_clear_but_repaints_keep_occurrences(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(|_, _| Snippet {
        theme: Theme::of(Kind::Light),
        text: "word word".into(),
        identity: "project-a:file:revision-1".into(),
    });
    cx.run_until_parked();
    let at = gpui::point(px(4.), px(8.));
    cx.simulate_click(at, Default::default());
    assert_fill(cx, Theme::of(Kind::Light), 2);
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    assert_fill(cx, Theme::of(Kind::Light), 2);
    for identity in [
        "project-b:file:revision-1",
        "project-b:other-file:revision-1",
        "project-b:other-file:revision-2",
    ] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.identity = identity.into();
                cx.notify();
            })
        });
        assert_fill(cx, Theme::of(Kind::Light), 0);
        cx.simulate_click(at, Default::default());
        assert_fill(cx, Theme::of(Kind::Light), 2);
    }
    cx.simulate_resize(gpui::size(px(45.), px(200.)));
    cx.run_until_parked();
    assert!(
        cx.debug_bounds("source-snippet").unwrap().size.height < px(30.),
        "source result stays one line when clipped"
    );
}
