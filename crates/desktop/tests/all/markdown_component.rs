//! Bounded public-API spike for the selected, unmodified Markdown component.
use gpui::{Context, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_component::text::TextView;

struct Document {
    source: String,
    width: Option<f32>,
}
impl Render for Document {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("document-scroll")
            .size_full()
            .overflow_x_scroll()
            .child(
                div()
                    .h_full()
                    .w_full()
                    .when_some(self.width, |el, width| el.min_w(px(width)))
                    .child(
                        TextView::markdown("document", self.source.clone())
                            .selectable(true)
                            .scrollable(true),
                    ),
            )
    }
}

#[gpui::test]
fn component_draws_readme_and_gfm_without_a_browser(cx: &mut TestAppContext) {
    let source = format!(
        "{}\n{}",
        include_str!("../../../../README.md"),
        include_str!("../fixtures/markdown-preview.md")
    );
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        guild_desktop::editor::apply_theme(&guild_desktop::theme::Theme::default(), 14., cx);
        let view = cx.new(|_| Document {
            source,
            width: None,
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(480.)));
    cx.simulate_event(gpui::ScrollWheelEvent {
        position: gpui::point(px(300.), px(200.)),
        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-600.))),
        ..Default::default()
    });
}

struct LocalImage {
    images: gpui::Entity<guild_desktop::markdown::Images>,
}
impl Render for LocalImage {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        gpui::image_cache(self.images.clone()).child(
            gpui::img(gpui::SharedUri::from("markdown-preview.svg"))
                .debug_selector(|| "local-image".into()),
        )
    }
}
#[gpui::test]
fn local_image_is_decoded_from_the_document_directory(cx: &mut TestAppContext) {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/markdown-preview.md");
    let (_, cx) = cx.add_window_view(|_, cx| LocalImage {
        images: guild_desktop::markdown::Images::new(&path, cx),
    });
    cx.run_until_parked();
    cx.simulate_resize(gpui::size(px(600.), px(300.)));
    let bounds = cx.debug_bounds("local-image").expect("image is drawn");
    assert_eq!(
        bounds.size,
        gpui::size(px(160.), px(64.)),
        "the relative SVG decodes to its intrinsic size"
    );
}

#[gpui::test]
fn rendered_text_can_be_selected_and_copied(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let view = cx.new(|_| Document {
            source: "Copy this native paragraph.".into(),
            width: None,
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(300.)));
    cx.simulate_mouse_down(
        gpui::point(px(1.), px(8.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_move(
        gpui::point(px(500.), px(8.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(
        gpui::point(px(500.), px(8.)),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.simulate_keystrokes("cmd-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().and_then(|c| c.text()),
            Some("Copy this native paragraph.".into())
        )
    });
}

#[gpui::test]
fn wide_table_tail_is_reachable_by_horizontal_scrolling(cx: &mut TestAppContext) {
    let source = format!(
        "| Column |\n| --- |\n| BEGIN_{}TAIL_MARKER |",
        "x".repeat(240)
    );
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let width = guild_desktop::markdown::minimum_columns(&source) as f32 * 14.;
        let view = cx.new(|_| Document {
            source,
            width: Some(width),
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(300.), px(180.)));
    let mut found = false;
    let mut last = String::new();
    for _ in 0..42 {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(150.), px(45.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-100.), px(0.))),
            ..Default::default()
        });
        cx.simulate_mouse_down(
            gpui::point(px(1.), px(45.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(
            gpui::point(px(299.), px(45.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            gpui::point(px(299.), px(45.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("cmd-c");
        let copied = cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .unwrap_or_default();
        last = copied;
        if last.contains("TAIL_MARKER") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "horizontal scroll must reveal the end of the cell; last copied: {last:?}"
    );
}

#[gpui::test]
fn links_open_only_when_clicked(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let view = cx.new(|_| Document {
            source: "[Documentation](https://example.com/docs)".into(),
            width: None,
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(600.), px(300.)));
    assert_eq!(cx.opened_url(), None);
    cx.simulate_click(gpui::point(px(40.), px(8.)), gpui::Modifiers::default());
    assert_eq!(cx.opened_url().as_deref(), Some("https://example.com/docs"));
}

#[gpui::test]
fn wide_code_tail_is_reachable_by_horizontal_scrolling(cx: &mut TestAppContext) {
    let source = format!("```\nBEGIN_{}TAIL_MARKER\n```", "x".repeat(240));
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let width = guild_desktop::markdown::minimum_columns(&source) as f32 * 14.;
        let view = cx.new(|_| Document {
            source,
            width: Some(width),
        });
        gpui_component::Root::new(view, window, cx)
    });
    cx.simulate_resize(gpui::size(px(300.), px(180.)));
    let mut found = false;
    let mut last = String::new();
    for _ in 0..42 {
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: gpui::point(px(150.), px(20.)),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(-100.), px(0.))),
            ..Default::default()
        });
        cx.simulate_mouse_down(
            gpui::point(px(1.), px(20.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_move(
            gpui::point(px(299.), px(20.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            gpui::point(px(299.), px(20.)),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.simulate_keystrokes("cmd-c");
        let copied = cx
            .update(|_, cx| cx.read_from_clipboard().and_then(|c| c.text()))
            .unwrap_or_default();
        last = copied;
        if last.contains("TAIL_MARKER") {
            found = true;
            break;
        }
    }
    assert!(
        found,
        "horizontal scroll must reveal the end of the code line; last copied: {last:?}"
    );
}

#[gpui::test]
fn markdown_palette_follows_theme_switches(cx: &mut TestAppContext) {
    cx.update(|cx| {
        guild_desktop::editor::init(cx);
        for &kind in guild_desktop::theme::Kind::ALL {
            let theme = guild_desktop::theme::Theme::of(kind);
            guild_desktop::editor::apply_theme(&theme, 14., cx);
            let component = gpui_component::Theme::global(cx);
            let base = cx.global::<gpui_kit::base::Theme>();
            assert_eq!(
                base.tokens.colors.background, component.background,
                "Base must leave its startup palette in {kind:?}"
            );
            assert_eq!(
                base.tokens.colors.selection, component.selection,
                "Native text selection must follow the same theme in {kind:?}"
            );
            assert_eq!(
                component.table_head,
                gpui::Hsla::from(gpui::rgb(theme.surface.app)),
                "Markdown header must share the document background in {kind:?}"
            );
            assert_eq!(
                component.table_head_foreground, component.foreground,
                "Markdown header must retain readable document text in {kind:?}"
            );
            assert_eq!(
                component.link,
                gpui::Hsla::from(gpui::rgb(theme.accent.patina)),
                "Markdown links must follow the active palette in {kind:?}"
            );
        }
    });
}
