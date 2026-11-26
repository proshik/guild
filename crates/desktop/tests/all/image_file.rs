//! Image files share the ordinary FileTab and preserve editable SVG buffers.
use gpui::{AppContext, Entity, Focusable, TestAppContext, VisualTestContext, px};
use guild_desktop::{
    file_tab::{
        FileTab, Look,
        image::{self, Zoom},
    },
    i18n::Ui,
    theme::Theme,
};
use std::path::Path;

struct Fixture<'a> {
    dir: tempfile::TempDir,
    file: Entity<FileTab>,
    cx: &'a mut VisualTestContext,
    name: &'static str,
}
fn mount<'a>(cx: &'a mut TestAppContext, name: &'static str, bytes: &[u8]) -> Fixture<'a> {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(name), bytes).unwrap();
    let mut made = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        guild_desktop::editor::init(cx);
        let file = cx.new(|cx| {
            FileTab::open(
                dir.path(),
                Path::new(name),
                Look {
                    family: "monospace".into(),
                    font_size: 14.,
                    theme: Theme::default(),
                    autosave: false,
                },
                Ui(guild_session::Language::English),
                window,
                cx,
            )
        });
        window.focus(&file.read(cx).focus_handle(cx), cx);
        made = Some(file.clone());
        gpui_component::Root::new(file, window, cx)
    });
    cx.simulate_resize(gpui::size(px(900.), px(600.)));
    cx.run_until_parked();
    Fixture {
        dir,
        file: made.unwrap(),
        cx,
        name,
    }
}
impl Fixture<'_> {
    fn click(&mut self, id: &'static str) {
        let bounds = self.cx.debug_bounds(id).expect("image control");
        self.cx
            .simulate_click(bounds.center(), gpui::Modifiers::default());
        self.cx.run_until_parked();
    }
    fn text(&mut self) -> String {
        self.cx.update(|_, cx| {
            self.file
                .read(cx)
                .editor()
                .unwrap()
                .read(cx)
                .text()
                .to_string()
        })
    }
    fn disk_changed(&mut self, bytes: &[u8]) {
        std::fs::write(self.dir.path().join(self.name), bytes).unwrap();
        self.cx
            .update(|window, cx| self.file.update(cx, |f, cx| f.disk_changed(window, cx)));
        self.cx.run_until_parked();
    }
}
const SVG: &[u8] = include_bytes!("../fixtures/markdown-preview.svg");
const PNG: &[u8] = include_bytes!("../fixtures/markdown-raster.png");

#[gpui::test]
fn svg_opens_in_preview_with_source_controls(cx: &mut TestAppContext) {
    let f = mount(cx, "picture.svg", SVG);
    assert!(
        f.cx.debug_bounds("image-source").is_some(),
        "SVG must offer source without replacing its file tab"
    );
    assert!(
        f.cx.debug_bounds("image-fit").is_some(),
        "SVG must open in image preview"
    );
    f.cx.update(|_, cx| {
        let file = f.file.read(cx);
        assert!(!file.image_source_visible());
        assert!(file.image_dimensions().is_some());
        assert!(file.editor().is_some(), "SVG retains the XML editor");
    });
}

#[gpui::test]
fn source_switch_preserves_dirty_editor_and_save(cx: &mut TestAppContext) {
    let mut f = mount(cx, "picture.svg", SVG);
    f.click("image-source");
    let editor =
        f.cx.update(|_, cx| f.file.read(cx).editor().unwrap().clone());
    let edited = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"33\" height=\"17\"><text>Привет</text></svg>";
    f.cx.simulate_keystrokes("cmd-a");
    f.cx.simulate_input(edited);
    f.cx.run_until_parked();
    assert!(f.cx.update(|_, cx| f.file.read(cx).is_dirty()));
    f.click("image-source");
    assert!(f.cx.update(|_, cx| f.file.read(cx).is_dirty()));
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_dimensions()),
        Some((33, 17))
    );
    f.disk_changed(b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2\" height=\"2\"/>");
    assert_eq!(f.text(), edited, "watcher must preserve dirty SVG source");
    f.cx.simulate_input("must not edit hidden SVG input");
    assert_eq!(f.text(), edited);
    f.click("image-source");
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).editor().unwrap().clone()),
        editor
    );
    assert_eq!(f.text(), edited);
    f.cx.update(|_, cx| f.file.update(cx, |f, cx| f.save(cx).unwrap()));
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join(f.name)).unwrap(),
        edited
    );
    assert!(!f.cx.update(|_, cx| f.file.read(cx).is_dirty()));
}

#[gpui::test]
fn raster_fit_zoom_reload_error_and_recovery(cx: &mut TestAppContext) {
    let mut f = mount(cx, "picture.PNG", PNG);
    assert!(f.cx.debug_bounds("image-source").is_none());
    assert!(f.cx.update(|_, cx| f.file.read(cx).editor().is_none()));
    let original = f.cx.update(|_, cx| f.file.read(cx).image_dimensions());
    assert!(original.is_some());
    f.click("image-actual");
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_zoom()),
        Some(Zoom::Scale(1.))
    );
    f.click("image-in");
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_zoom()),
        Some(Zoom::Scale(1.25))
    );
    f.click("image-out");
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_zoom()),
        Some(Zoom::Scale(1.))
    );
    f.click("image-fit");
    f.cx.simulate_resize(gpui::size(px(360.), px(240.)));
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_zoom()),
        Some(Zoom::Fit)
    );
    f.disk_changed(b"broken image");
    assert!(f.cx.update(|_, cx| f.file.read(cx).image_error().is_some()));
    assert!(f.cx.debug_bounds("file-image").is_none());
    f.disk_changed(PNG);
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_dimensions()),
        original
    );
}

#[gpui::test]
fn clean_svg_reload_updates_retained_source_and_preview(cx: &mut TestAppContext) {
    let mut f = mount(cx, "picture.svg", SVG);
    let next = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"23\" height=\"41\"/>";
    f.disk_changed(next.as_bytes());
    assert_eq!(f.text(), next);
    assert_eq!(
        f.cx.update(|_, cx| f.file.read(cx).image_dimensions()),
        Some((23, 41))
    );
    f.disk_changed(b"invalid svg");
    assert!(f.cx.update(|_, cx| f.file.read(cx).image_error().is_some()));
    f.click("image-source");
    assert_eq!(f.text(), "invalid svg", "invalid XML remains editable");
}

#[gpui::test]
fn neighboring_utf8_text_keeps_its_editor(cx: &mut TestAppContext) {
    let mut f = mount(cx, "notes.txt", "Привет\nordinary text\n".as_bytes());
    assert!(f.cx.debug_bounds("image-fit").is_none());
    assert_eq!(f.text(), "Привет\nordinary text\n");
}

#[test]
fn supported_formats_and_fit_geometry_are_bounded() {
    for name in [
        "x.png", "x.jpg", "x.JPEG", "x.webp", "x.gif", "x.ico", "x.SVG",
    ] {
        assert!(image::format(Path::new(name)).is_some(), "{name}");
    }
    for name in ["x.bmp", "x.tiff", "x.xml", "x.txt", "x.svg.txt"] {
        assert!(image::format(Path::new(name)).is_none(), "{name}");
    }
    assert_eq!(Zoom::Fit.scale((1200., 600.), (300., 300.)), 0.25);
    assert_eq!(Zoom::Fit.scale((100., 50.), (300., 300.)), 1.);
    assert_eq!(Zoom::Scale(1.).scale((1200., 600.), (300., 300.)), 1.);
    assert_eq!(Zoom::Scale(100.).scale((1., 1.), (1., 1.)), 8.);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("too-big.png");
    std::fs::File::create(&path)
        .unwrap()
        .set_len(guild_desktop::file_tab::OPEN_LIMIT + 1)
        .unwrap();
    assert!(image::bytes(&path).unwrap_err().contains("limit"));
    assert!(image::bytes(&dir.path().join("missing.png")).is_err());
}

#[gpui::test]
fn jpeg_webp_and_ico_decode_through_the_file_tab(cx: &mut TestAppContext) {
    for (name, bytes) in [
        (
            "image.jpg",
            include_bytes!("../fixtures/image-preview.jpg").as_slice(),
        ),
        (
            "image.webp",
            include_bytes!("../fixtures/image-preview.webp").as_slice(),
        ),
        (
            "image.ico",
            include_bytes!("../fixtures/image-preview.ico").as_slice(),
        ),
    ] {
        let f = mount(cx, name, bytes);
        assert_eq!(
            f.cx.update(|_, cx| f.file.read(cx).image_dimensions()),
            Some((16, 16)),
            "{name}"
        );
        assert!(f.cx.debug_bounds("file-image").is_some(), "{name}");
    }
}

#[gpui::test]
fn newly_loaded_svg_preview_does_not_focus_its_hidden_editor(cx: &mut TestAppContext) {
    let mut f = mount(cx, "picture.svg", SVG);
    let original = f.text();
    f.cx.simulate_input("hidden typing");
    assert_eq!(
        f.text(),
        original,
        "the asynchronously realised XML editor must stay unfocused in preview"
    );
    f.click("image-source");
    f.cx.simulate_input("visible typing");
    assert_ne!(f.text(), original);
}

#[gpui::test]
fn large_image_fits_and_remains_centered_after_resize(cx: &mut TestAppContext) {
    let mut f = mount(
        cx,
        "large.svg",
        b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1200\" height=\"600\"/>",
    );
    for (w, h) in [(900., 600.), (360., 240.), (900., 600.)] {
        f.cx.simulate_resize(gpui::size(px(w), px(h)));
        f.cx.run_until_parked();
        let viewport = f.cx.debug_bounds("image-viewport").unwrap();
        let image = f.cx.debug_bounds("file-image").unwrap();
        assert!(
            image.size.width <= viewport.size.width,
            "{w}x{h}: image={image:?}, viewport={viewport:?}"
        );
        assert!(image.size.height <= viewport.size.height);
        assert!((f32::from(image.center().x - viewport.center().x)).abs() < 1.);
        assert!((f32::from(image.center().y - viewport.center().y)).abs() < 1.);
    }
    f.cx.simulate_resize(gpui::size(px(100.), px(240.)));
    f.cx.run_until_parked();
    let before = f.cx.debug_bounds("file-image").unwrap().size.width;
    f.click("image-out");
    let after = f.cx.debug_bounds("file-image").unwrap().size.width;
    assert!(
        after < before,
        "zooming out of a sub-10% fit must shrink: {before:?} -> {after:?}"
    );
    f.click("image-actual");
    let image = f.cx.debug_bounds("file-image").unwrap();
    assert_eq!(image.size, gpui::size(px(1200.), px(600.)));
}

#[gpui::test]
fn oversized_gif_reports_preview_error_without_decoding_its_canvas(cx: &mut TestAppContext) {
    let mut bytes = include_bytes!("../fixtures/markdown-animation.gif").to_vec();
    bytes[6..10].copy_from_slice(&[255, 255, 255, 255]);
    let fixture = mount(cx, "oversized.gif", &bytes);
    fixture.cx.update(|_, cx| {
        assert!(
            fixture
                .file
                .read(cx)
                .image_error()
                .unwrap()
                .contains("memory limit")
        );
        assert!(fixture.file.read(cx).image_dimensions().is_none());
    });
}
