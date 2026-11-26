//! File-owned image work: the same decoder and animated Img used by Markdown.
use super::*;
use gpui::{Image, ImageFormat, ObjectFit, RenderImage, StyledImage};
use std::sync::Arc;

/// The deliberately bounded set of file previews. Other files retain text handling.
pub fn format(path: &Path) -> Option<ImageFormat> {
    Some(
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "png" => ImageFormat::Png,
            "jpg" | "jpeg" => ImageFormat::Jpeg,
            "webp" => ImageFormat::Webp,
            "gif" => ImageFormat::Gif,
            "ico" => ImageFormat::Ico,
            "svg" => ImageFormat::Svg,
            _ => return None,
        },
    )
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    Fit,
    Scale(f32),
}
impl Zoom {
    pub fn scale(self, intrinsic: (f32, f32), available: (f32, f32)) -> f32 {
        match self {
            Self::Fit => (available.0 / intrinsic.0)
                .min(available.1 / intrinsic.1)
                .clamp(0.000001, 1.),
            Self::Scale(scale) => scale.clamp(0.000001, 8.),
        }
    }
}

#[derive(Clone, Copy)]
enum Adjustment {
    Fit,
    Actual,
    Out,
    In,
}

pub(super) struct Preview {
    pub format: ImageFormat,
    pub source: bool,
    pub zoom: Zoom,
    pub loaded: Option<Result<Arc<RenderImage>, String>>,
    task: Option<gpui::Task<()>>,
    generation: u64,
    /// A disk source is installed only on the UI thread with a window.
    source_read: Option<(FileState, Option<(String, LineEnding)>)>,
    available: std::rc::Rc<std::cell::Cell<gpui::Size<gpui::Pixels>>>,
}
impl Preview {
    pub fn new(format: ImageFormat) -> Self {
        Self {
            format,
            source: false,
            zoom: Zoom::Fit,
            loaded: None,
            task: None,
            generation: 0,
            source_read: None,
            available: Default::default(),
        }
    }
}

/// Read no more than the existing file limit even if the file grows during read.
pub fn bytes(path: &Path) -> Result<Vec<u8>, String> {
    super::read_bytes(path).map_err(|state| match state {
        FileState::TooLarge => "Image exceeds the file size limit".into(),
        FileState::CannotRead(why) => why,
        FileState::Gone => "File no longer exists".into(),
        _ => unreachable!("byte reads do not classify text"),
    })
}

// Bound one RGBA canvas and all retained full-canvas animation frames.
const GIF_CANVAS_LIMIT: u64 = 64 * 1024 * 1024;
const GIF_DECODED_LIMIT: u64 = 256 * 1024 * 1024;

fn validate_gif(bytes: &[u8], canvas_limit: u64, decoded_limit: u64) -> Result<(), String> {
    // Same gif parser as image/GPUI, but metadata-only: no LZW buffers or
    // full-canvas frames. Validate the entire immutable input before GPUI
    // starts allocating. Frame rectangles can be larger than the canvas too.
    let mut options = gif::DecodeOptions::new();
    options.skip_frame_decoding(true);
    let mut decoder = options.read_info(bytes).map_err(|e| e.to_string())?;
    let canvas = u64::from(decoder.width()) * u64::from(decoder.height()) * 4;
    let budget_error = || "GIF exceeds the decoded image memory limit".to_string();
    if canvas == 0 || canvas > canvas_limit {
        return Err(budget_error());
    }
    let mut retained = 0u64;
    while let Some(frame) = decoder.next_frame_info().map_err(|e| e.to_string())? {
        let frame_bytes = u64::from(frame.width) * u64::from(frame.height) * 4;
        retained = retained.checked_add(canvas).ok_or_else(budget_error)?;
        if frame_bytes > canvas_limit || retained > decoded_limit {
            return Err(budget_error());
        }
    }
    Ok(())
}

// App::drop_image(None) misses a currently leased Window. Deferring until
// the outer update returns lets App visit that window as well as other windows.
fn release_atlas(image: Arc<RenderImage>, cx: &mut App) {
    cx.defer(move |cx| cx.drop_image(image, None));
}

impl FileTab {
    pub(super) fn own_image_atlas(cx: &mut Context<Self>) {
        cx.on_release(|tab, cx| {
            if let Some(Ok(image)) = tab.image.as_mut().and_then(|p| p.loaded.take()) {
                release_atlas(image, cx);
            }
        })
        .detach();
    }

    pub(crate) fn source_loading(&self) -> bool {
        self.editor.is_none()
            && (self.loading.pending()
                || self.pending.is_some()
                || self.image.as_ref().is_some_and(|p| {
                    matches!(p.format, ImageFormat::Svg)
                        && (p.loaded.is_none() || p.source_read.is_some())
                }))
    }
    pub(super) fn invalidate_image_source(&mut self, cx: &mut Context<Self>) {
        let Some(p) = self
            .image
            .as_mut()
            .filter(|p| matches!(p.format, ImageFormat::Svg))
        else {
            return;
        };
        // An edit revokes disk authority even if the person saves before the
        // outstanding read completes and the buffer becomes clean again.
        p.generation += 1;
        p.task = None;
        p.source_read = None;
        if !p.source {
            self.load_image(false, cx);
        }
    }
    /// Navigation and Find must place their caret in visible source.
    pub(crate) fn reveal_image_source(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.image {
            p.source = matches!(p.format, ImageFormat::Svg);
            if p.source {
                cx.notify();
            }
        }
    }
    pub(super) fn load_image(&mut self, from_disk: bool, cx: &mut Context<Self>) {
        let path = self.path();
        let Some(preview) = self.image.as_mut() else {
            return;
        };
        preview.generation += 1;
        let generation = preview.generation;
        if let Some(Ok(image)) = preview.loaded.take() {
            release_atlas(image, cx);
        }
        preview.source_read = None;
        let format = preview.format;
        let svg = matches!(format, ImageFormat::Svg);
        let source = (!from_disk && svg).then(|| {
            self.editor
                .as_ref()
                .map(|e| e.read(cx).text().to_string())
                .or_else(|| self.pending.clone())
                .unwrap_or_else(|| self.clean.clone())
        });
        let renderer = cx.svg_renderer();
        let work = cx.background_executor().spawn(async move {
            let bytes = source
                .map(String::into_bytes)
                .map(Ok)
                .unwrap_or_else(|| bytes(&path));
            let text = if svg && from_disk {
                Some(match &bytes {
                    Ok(bytes) => match Text::of(bytes) {
                        Text::Source { text, line_ending } => {
                            (FileState::Text, Some((text, line_ending)))
                        }
                        Text::Binary => (FileState::NotText, None),
                    },
                    Err(why) => (FileState::CannotRead(why.clone()), None),
                })
            } else {
                None
            };
            let decoded = bytes.and_then(|bytes| {
                if matches!(format, ImageFormat::Gif) {
                    validate_gif(&bytes, GIF_CANVAS_LIMIT, GIF_DECODED_LIMIT)?;
                }
                Image::from_bytes(format, bytes)
                    .to_image_data(renderer)
                    .map_err(|e| e.to_string())
            });
            (text, decoded)
        });
        self.image.as_mut().unwrap().task = Some(cx.spawn(async move |tab, cx| {
            let (source, decoded) = work.await;
            let _ = tab.update(cx, |tab, cx| {
                let Some(preview) = tab.image.as_mut() else {
                    return;
                };
                if preview.generation != generation {
                    return;
                }
                preview.loaded = Some(decoded);
                preview.source_read = source;
                cx.notify();
            });
        }));
        cx.notify();
    }

    pub(super) fn realise_image_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.image.as_mut().and_then(|p| p.source_read.take());
        let Some((state, text)) = source else {
            return;
        };
        // Typing while an external read was outstanding always wins.
        if self.dirty {
            self.changed_on_disk = true;
            return;
        }
        self.state = state;
        if let Some((text, ending)) = text {
            self.line_ending = ending;
            self.clean = text.clone();
            if let Some(editor) = &self.editor {
                let focused = window.focused(cx);
                let position = editor.read(cx).cursor_position();
                editor.update(cx, |editor, cx| {
                    editor.set_value(text, window, cx);
                    editor.set_cursor_position(position, window, cx);
                });
                if let Some(handle) = focused {
                    window.focus(&handle, cx);
                } else {
                    window.blur(cx);
                }
            } else {
                self.pending = Some(text);
            }
        } else {
            self.discard_local_search(cx);
            self.smart_selection = None;
            self.editor = None;
            self._changes = None;
            self.pending = None;
            self.clean.clear();
        }
        if let Some(search) = &self.local_search {
            search.update(cx, |search, cx| search.source_reloaded(cx));
        }
        self.changed_on_disk = false;
        self.refresh_git_baseline(cx);
        self.refresh_blame(cx);
        cx.emit(BufferChanged);
    }

    pub fn image_zoom(&self) -> Option<Zoom> {
        self.image.as_ref().map(|p| p.zoom)
    }
    pub fn image_dimensions(&self) -> Option<(u32, u32)> {
        let image = self.image.as_ref()?.loaded.as_ref()?.as_ref().ok()?;
        let size = image.size(0);
        // GPUI intentionally rasterizes SVG at 2x for smoothing. Its public
        // scale constant is shared with Img; raster dimensions are not SVG units.
        let factor = if matches!(self.image.as_ref()?.format, ImageFormat::Svg) {
            gpui::SMOOTH_SVG_SCALE_FACTOR
        } else {
            1.
        };
        Some((
            (size.width.0 as f32 / factor).round().max(1.) as u32,
            (size.height.0 as f32 / factor).round().max(1.) as u32,
        ))
    }
    pub fn image_error(&self) -> Option<&str> {
        self.image
            .as_ref()?
            .loaded
            .as_ref()?
            .as_ref()
            .err()
            .map(String::as_str)
    }
    pub fn image_source_visible(&self) -> bool {
        self.image.as_ref().is_some_and(|p| p.source)
    }

    pub(super) fn image_controls(&self, cx: &mut Context<Self>) -> Option<gpui::Div> {
        let preview = self.image.as_ref()?;
        let theme = self.look.theme;
        let mut controls = div()
            .flex()
            .flex_wrap()
            .items_center()
            .flex_none()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .px(theme::space::PANEL_PADDING)
            .min_h(theme::space::PANEL_HEAD)
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.secondary));
        for (id, text, adjustment) in [
            ("image-fit", self.ui.text(Label::ImageFit), Adjustment::Fit),
            ("image-actual", "100%", Adjustment::Actual),
            ("image-out", "−", Adjustment::Out),
            ("image-in", "+", Adjustment::In),
        ] {
            controls = controls.child(
                div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .cursor_pointer()
                    .px(theme::space::BUTTON_SIDE)
                    .h(theme::space::HEAD_BUTTON)
                    .flex()
                    .items_center()
                    .rounded(theme::radius::ROW)
                    .hover(|s| s.bg(rgb(theme.surface.raised)))
                    .child(text)
                    .on_click(cx.listener(move |tab, _, _, cx| {
                        let current = tab.image_scale();
                        if let Some(p) = &mut tab.image {
                            p.zoom = match adjustment {
                                Adjustment::Out => Zoom::Scale((current / 1.25).max(0.000001)),
                                Adjustment::In => Zoom::Scale((current * 1.25).min(8.)),
                                Adjustment::Fit => Zoom::Fit,
                                Adjustment::Actual => Zoom::Scale(1.),
                            };
                        }
                        cx.notify();
                    })),
            );
        }
        if let Some((w, h)) = self.image_dimensions() {
            controls = controls.child(format!("{w} × {h} · {:.0}%", self.image_scale() * 100.));
        }
        if matches!(preview.format, ImageFormat::Svg) {
            let label = if preview.source {
                Label::MarkdownPreview
            } else {
                Label::ImageSource
            };
            controls = controls.child(
                div()
                    .id("image-source")
                    .debug_selector(|| "image-source".into())
                    .cursor_pointer()
                    .px(theme::space::BUTTON_SIDE)
                    .child(self.ui.text(label))
                    .on_click(cx.listener(|tab, _, window, cx| {
                        let source = !tab.image.as_ref().unwrap().source;
                        if !source {
                            tab.presentation_blur(window, cx);
                        }
                        tab.image.as_mut().unwrap().source = source;
                        if source {
                            if let Some(editor) = &tab.editor {
                                window.focus(&editor.read(cx).focus_handle(cx), cx);
                            }
                        } else {
                            if tab.editor.is_some() || tab.pending.is_some() {
                                tab.load_image(false, cx);
                            }
                            window.focus(&tab.focus, cx);
                        }
                        cx.notify();
                    })),
            );
        }
        Some(controls)
    }
    fn image_scale(&self) -> f32 {
        let p = self.image.as_ref().unwrap();
        let (w, h) = self.image_dimensions().unwrap_or((1, 1));
        let available = p.available.get();
        p.zoom.scale(
            (w as f32, h as f32),
            (f32::from(available.width), f32::from(available.height)),
        )
    }
    pub(super) fn image_body(&self, _cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let p = self.image.as_ref()?;
        if p.source && (self.editor.is_some() || p.loaded.is_some()) {
            return None;
        }
        let content = match &p.loaded {
            None => div()
                .child(self.ui.text(Label::ImageLoading))
                .into_any_element(),
            Some(Err(why)) => div()
                .p(theme::space::PANEL_PADDING)
                .child(self.ui.text(Label::ImageCannotRead))
                .child(div().child(why.clone()))
                .into_any_element(),
            Some(Ok(image)) => {
                let (w, h) = self.image_dimensions().unwrap();
                let scale = self.image_scale();
                gpui::img(image.clone())
                    .id("file-image")
                    .debug_selector(|| "file-image".into())
                    .when(p.zoom == Zoom::Fit, |img| {
                        img.w_full()
                            .h_full()
                            .max_w(px(w as f32))
                            .max_h(px(h as f32))
                            .object_fit(ObjectFit::Contain)
                    })
                    .when(p.zoom != Zoom::Fit, |img| {
                        img.w(px(w as f32 * scale)).h(px(h as f32 * scale))
                    })
                    .flex_none()
                    .into_any_element()
            }
        };
        let measured = p.available.clone();
        let (w, h) = self.image_dimensions().unwrap_or((0, 0));
        let scale = self.image_scale();
        Some(
            div()
                .id("image-viewport")
                .track_focus(&self.focus)
                .debug_selector(|| "image-viewport".into())
                .relative()
                .size_full()
                .overflow_scroll()
                .child(
                    div()
                        .when(p.zoom == Zoom::Fit, |this| this.size_full())
                        .when(p.zoom != Zoom::Fit, |this| {
                            this.min_w_full()
                                .min_h_full()
                                .w(px(w as f32 * scale))
                                .h(px(h as f32 * scale))
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(content),
                )
                .child(
                    gpui::canvas(
                        move |bounds, window, _| {
                            if measured.replace(bounds.size) != bounds.size {
                                // Like Markdown's inline images: refresh after learning
                                // current layout, rather than painting one stale resize.
                                window.refresh();
                            }
                        },
                        |_, _, _, _| {},
                    )
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full(),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    // The frame list is private to the file presentation, not a public file API.
    use super::*;
    use gpui::{AppContext, TestAppContext};
    #[test]
    fn gif_budgets_precede_canvas_and_aggregate_allocation() {
        let normal = include_bytes!("../../tests/fixtures/markdown-animation.gif");
        assert!(validate_gif(normal, GIF_CANVAS_LIMIT, GIF_DECODED_LIMIT).is_ok());
        let canvas = u64::from(u16::from_le_bytes([normal[6], normal[7]]))
            * u64::from(u16::from_le_bytes([normal[8], normal[9]]))
            * 4;
        assert!(validate_gif(normal, canvas - 1, GIF_DECODED_LIMIT).is_err());
        assert!(
            validate_gif(normal, canvas, canvas).is_err(),
            "two frames cannot fit a one-frame budget"
        );
        assert!(validate_gif(normal, canvas, canvas * 2).is_ok());
        // Two 1x1 updates still retain two complete 2x2 output canvases.
        // This is metadata-only: no decoder allocation is needed for the check.
        let mut partial = normal.to_vec();
        partial[6..10].copy_from_slice(&[2, 0, 2, 0]);
        assert!(validate_gif(&partial, 16, 16).is_err());
        assert!(validate_gif(&partial, 16, 32).is_ok());
        // Merely change the logical dimensions; never allocate this canvas.
        let mut huge = normal.to_vec();
        huge[6..10].copy_from_slice(&[255, 255, 255, 255]);
        assert!(validate_gif(&huge, GIF_CANVAS_LIMIT, GIF_DECODED_LIMIT).is_err());
    }
    #[gpui::test]
    fn file_decoder_retains_gif_animation_frames(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("animation.gif"),
            include_bytes!("../../tests/fixtures/markdown-animation.gif"),
        )
        .unwrap();
        let file = cx.update(|cx| {
            cx.new(|cx| {
                FileTab::restore(
                    dir.path(),
                    Path::new("animation.gif"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: theme::Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    cx,
                )
            })
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let p = file.read(cx).image.as_ref().unwrap();
            assert_eq!(
                p.loaded.as_ref().unwrap().as_ref().unwrap().frame_count(),
                2
            );
        });
    }
    #[gpui::test]
    fn newer_svg_buffer_supersedes_outstanding_decode(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let file = cx.update(|cx| {
            cx.new(|cx| {
                FileTab::restore(
                    dir.path(),
                    Path::new("image.svg"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: theme::Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    cx,
                )
            })
        });
        cx.update(|cx| {
            file.update(cx, |file, cx| {
                assert!(
                    file.image.as_ref().unwrap().loaded.is_none(),
                    "opening must not decode synchronously"
                );
                file.pending = Some(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"31\" height=\"17\"/>".into(),
                );
                file.load_image(false, cx);
                file.pending = Some(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"11\" height=\"7\"/>".into(),
                );
                file.load_image(false, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(file.read(cx).image_dimensions(), Some((11, 7))));
    }
    struct Slot(Option<gpui::Entity<FileTab>>);
    impl gpui::Render for Slot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().size_full().children(self.0.clone())
        }
    }
    #[gpui::test]
    fn painted_atlas_entries_are_released_on_reload_and_last_owner(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("image.png"),
            include_bytes!("../../tests/fixtures/markdown-raster.png"),
        )
        .unwrap();
        let mut made = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    dir.path(),
                    Path::new("image.png"),
                    Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: theme::Theme::default(),
                        autosave: false,
                    },
                    Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let slot = cx.new(|_| Slot(Some(file.clone())));
            made = Some((file, slot.clone()));
            gpui_component::Root::new(slot, window, cx)
        });
        visual.run_until_parked();
        let (file, slot) = made.unwrap();
        let mut second_context = visual.cx.clone();
        let mut second_slot = None;
        let (_, other) = second_context.add_window_view(|window, cx| {
            let slot = cx.new(|_| Slot(Some(file.clone())));
            second_slot = Some(slot.clone());
            gpui_component::Root::new(slot, window, cx)
        });
        other.run_until_parked();
        let second_slot = second_slot.unwrap();
        let old = visual.update(|window, cx| {
            let image = file
                .read(cx)
                .image
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .clone();
            assert!(
                window.has_image_atlas_entry(&image),
                "the public platform atlas must contain the painted image"
            );
            image
        });
        other.update(|window, _| {
            assert!(
                window.has_image_atlas_entry(&old),
                "the shared image is painted in a second atlas"
            )
        });
        visual.update(|window, cx| file.update(cx, |file, cx| file.reload(window, cx)));
        visual.run_until_parked();
        let current = visual.update(|window, cx| {
            assert!(
                !window.has_image_atlas_entry(&old),
                "reload must remove the prior GPU allocation, not just its CPU Arc"
            );
            let image = file
                .read(cx)
                .image
                .as_ref()
                .unwrap()
                .loaded
                .as_ref()
                .unwrap()
                .as_ref()
                .unwrap()
                .clone();
            assert!(window.has_image_atlas_entry(&image));
            image
        });
        other.update(|window, _| {
            assert!(
                !window.has_image_atlas_entry(&old),
                "replacement must also release other windows"
            );
            assert!(
                window.has_image_atlas_entry(&current),
                "the replacement is painted in both windows before final cleanup"
            );
        });
        other.update(|_, cx| {
            second_slot.update(cx, |slot, cx| {
                slot.0 = None;
                cx.notify();
            })
        });
        other.run_until_parked();
        visual.update(|_, cx| {
            slot.update(cx, |slot, cx| {
                slot.0 = None;
                cx.notify();
            })
        });
        visual.run_until_parked();
        visual.update(|window, _| {
            assert!(
                window.has_image_atlas_entry(&current),
                "an external FileTab owner still retains this image"
            )
        });
        visual.update(|_, _| drop(file));
        visual.run_until_parked();
        visual.update(|window, _| {
            assert!(
                !window.has_image_atlas_entry(&current),
                "last release inside a current Window update must free its atlas"
            )
        });
        other.update(|window, _| {
            assert!(
                !window.has_image_atlas_entry(&current),
                "final owner release must clean every painted window"
            )
        });
    }
}
