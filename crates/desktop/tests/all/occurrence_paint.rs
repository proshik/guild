//! Observe actual scene paths handed to GPUI's public headless renderer.
use gpui::{AppContext, PlatformHeadlessRenderer, Render, Styled as _};
use gpui_component::input::{Editor, EditorState};
use guild_desktop::{
    editor::Occurrences,
    theme::{Kind, Theme},
};
use std::{cell::RefCell, rc::Rc, sync::Arc};

struct Recorder {
    paths: Rc<RefCell<Vec<gpui::Background>>>,
    atlas: Arc<gpui::HeadlessAtlas>,
}
impl PlatformHeadlessRenderer for Recorder {
    fn render_scene_to_image(
        &mut self,
        scene: &gpui::Scene,
        size: gpui::Size<gpui::DevicePixels>,
    ) -> anyhow::Result<image::RgbaImage> {
        self.render_scene(scene, size)?;
        // Capture is only a public route to the actual scene; pixels aren't asserted.
        Ok(image::RgbaImage::default())
    }
    fn render_scene(
        &mut self,
        scene: &gpui::Scene,
        _: gpui::Size<gpui::DevicePixels>,
    ) -> anyhow::Result<()> {
        *self.paths.borrow_mut() = scene.paths.iter().map(|path| path.color).collect();
        Ok(())
    }
    fn sprite_atlas(&self) -> Arc<dyn gpui::PlatformAtlas> {
        self.atlas.clone()
    }
}
struct Source(gpui::Entity<EditorState>);
impl Render for Source {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        Editor::new(&self.0).appearance(false).size_full()
    }
}

/// Observe the FileTab's own decorations after real hit-tested mouse events.
#[test]
fn readonly_file_tab_mouse_click_replaces_initial_occurrences_with_all_five_mentions() {
    use guild_desktop::{
        file_tab::{FileTab, Look},
        i18n::Ui,
    };
    let paths = Rc::new(RefCell::new(Vec::new()));
    let recorded = paths.clone();
    let mut cx = gpui::HeadlessAppContext::with_platform(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(()),
        move || {
            Ok(Some(Box::new(Recorder {
                paths: recorded.clone(),
                atlas: Arc::new(gpui::HeadlessAtlas::default()),
            })))
        },
    );
    cx.update(guild_desktop::editor::init);
    let dir = tempfile::tempdir().unwrap();
    let source = "fn calculate_total() {\nlet total = 0;\ntotal;\ntotal;\ntotal;\ntotal;\n}\n";
    std::fs::write(dir.path().join("library.rs"), source).unwrap();
    let theme = Theme::default();
    let mut file = None;
    let window = cx
        .open_window(gpui::size(gpui::px(700.), gpui::px(400.)), |window, cx| {
            let tab = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    std::path::Path::new("library.rs"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme,
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
                .read_only(true)
            });
            file = Some(tab.clone());
            cx.new(|cx| gpui_component::Root::new(tab, window, cx))
        })
        .unwrap();
    cx.run_until_parked();
    let file = file.unwrap();
    let draw = |cx: &mut gpui::HeadlessAppContext| {
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let _ = cx.capture_screenshot(window.into()).unwrap();
    };
    draw(&mut cx);
    let fills = || {
        paths
            .borrow()
            .iter()
            .filter(|paint| paint.as_solid() == Some(theme.identifier_occurrence().into()))
            .count()
    };
    assert_eq!(fills(), 1, "initial fn occurrence");
    let offset = source.find("let total").unwrap() + 5;
    cx.update_window(window.into(), |_, window, cx| {
        let editor = file.read(cx).editor().unwrap();
        assert_eq!(editor.read(cx).selected_range(), 0..0);
        let position = editor
            .read(cx)
            .range_to_bounds(&(offset..offset))
            .unwrap()
            .center();
        window.dispatch_event(
            gpui::PlatformInput::MouseDown(gpui::MouseDownEvent {
                button: gpui::MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.dispatch_event(
            gpui::PlatformInput::MouseUp(gpui::MouseUpEvent {
                button: gpui::MouseButton::Left,
                position,
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
    })
    .unwrap();
    draw(&mut cx);
    cx.update(|cx| {
        let editor = file.read(cx).editor().unwrap();
        let range = editor.read(cx).selected_range();
        assert!(
            range.is_empty() && (offset - 1..offset + 4).contains(&range.start),
            "actual click must move caret into total: {range:?}"
        );
    });
    assert_eq!(
        fills(),
        5,
        "all five total backgrounds, no stale fn background"
    );
}

#[test]
fn applied_editor_occurrence_fill_is_explicit_in_light_and_repainted_on_restyle() {
    let paths = Rc::new(RefCell::new(Vec::new()));
    let recorded = paths.clone();
    let mut cx = gpui::HeadlessAppContext::with_platform(
        Arc::new(gpui::NoopTextSystem::new()),
        Arc::new(()),
        move || {
            Ok(Some(Box::new(Recorder {
                paths: recorded.clone(),
                atlas: Arc::new(gpui::HeadlessAtlas::default()),
            })))
        },
    );
    cx.update(guild_desktop::editor::init);
    let light = Theme::of(Kind::Light);
    let dark = Theme::of(Kind::Dark);
    let mut owner = None;
    let window = cx
        .open_window(gpui::size(gpui::px(600.), gpui::px(240.)), |window, cx| {
            let editor = cx.new(|cx| {
                EditorState::new(window, cx)
                    .line_number(false)
                    .default_value("name name")
            });
            owner = Some(Occurrences::attach(&editor, light, cx));
            let source = cx.new(|_| Source(editor));
            cx.new(|cx| gpui_component::Root::new(source, window, cx))
        })
        .unwrap();
    let owner = owner.unwrap();
    for theme in [light, dark, light] {
        cx.update(|cx| owner.update(cx, |owner, cx| owner.restyle(theme, cx)));
        cx.run_until_parked();
        cx.update_window(window.into(), |_, window, cx| window.draw(cx).clear(cx))
            .unwrap();
        let _ = cx.capture_screenshot(window.into()).unwrap();
        let expected = Some(theme.identifier_occurrence().into());
        assert_eq!(
            paths
                .borrow()
                .iter()
                .filter(|paint| paint.as_solid() == expected)
                .count(),
            2,
            "the editor must paint both mentions in {} using its owner's current Guild color; paths={:?}",
            theme.kind.name(),
            paths.borrow()
        );
    }
}
