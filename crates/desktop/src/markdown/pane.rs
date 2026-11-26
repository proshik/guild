//! Pane-local presentation of an existing, possibly shared FileTab buffer.
use crate::view::tooltip::Hint as _;
use crate::{
    file_tab::{BufferChanged, FileState, FileTab},
    i18n::Text,
    theme,
};
use gpui::{
    App, AppContext, Context, Entity, FocusHandle, Focusable, SharedString, Subscription, Window,
    div, prelude::*, rgb,
};
use gpui_component::{
    ActiveTheme,
    text::{TextView, TextViewState, TextViewStyle},
};
use guild_session::markdown::Mode;
use std::{cell::Cell, rc::Rc};

/// Bound the component's synchronous initial parse; larger files remain editable.
pub const PREVIEW_LIMIT: usize = 256 * 1024;

/// A mode change is persisted by the window, without saving the file itself.
pub struct ModeChanged;

/// One pane's view and a readable state handle for the window's context-free saver.
#[derive(Clone)]
pub struct Presentation {
    /// The native pane rendering the retained file buffer.
    pub view: Entity<Pane>,
    /// Pane-local mode buttons hosted by the area tab strip.
    pub controls: Entity<Controls>,
    mode: Rc<Cell<Mode>>,
}
impl Presentation {
    /// Create presentation for Markdown and standalone diagram source files.
    pub fn for_file(file: &Entity<FileTab>, initial_mode: Mode, cx: &mut App) -> Option<Self> {
        if !guild_session::markdown::has_preview(file.read(cx).relative()) {
            return None;
        }
        // Standalone sources start in Source; persisted pane state is applied by restore.
        let initial_mode =
            if guild_session::markdown::diagram_engine(file.read(cx).relative()).is_some() {
                Mode::Editor
            } else {
                initial_mode
            };
        let mode = Rc::new(Cell::new(initial_mode));
        let view = cx.new(|cx| Pane::new(file.clone(), mode.clone(), cx));
        let controls = cx.new(|cx| Controls {
            pane: view.clone(),
            _subscriptions: vec![
                cx.observe(&view, |_, _, cx| cx.notify()),
                cx.observe(file, |_, _, cx| cx.notify()),
            ],
        });
        Some(Self {
            view,
            controls,
            mode,
        })
    }
    /// Root supplies its window-owned renderer; file-only fixtures need no service.
    pub fn attach_renderer(
        &self,
        renderer: &Entity<crate::diagrams::runtime::Manager>,
        cx: &mut App,
    ) {
        self.view.update(cx, |pane, cx| {
            pane.renderer = Some(renderer.clone());
            pane._subscriptions
                .push(cx.observe(renderer, |pane, _, cx| {
                    if let Some(state) = &pane.state {
                        state.update(cx, |state, cx| state.invalidate_inline_layout(cx));
                    }
                    cx.notify();
                }));
            pane.preview = None;
            cx.notify();
        });
    }
    /// The current pane-local mode, also readable while the root saves its state.
    pub fn mode(&self) -> Mode {
        self.mode.get()
    }
    /// Restore before the pane is drawn; this does not focus or write the file.
    pub fn restore(&self, mode: Mode, cx: &mut App) {
        self.view.update(cx, |pane, cx| {
            pane.mode.set(mode);
            cx.notify();
        });
    }
}

/// Presentation belongs to a pane; save, conflict policy and undo belong to FileTab.
pub struct Pane {
    kind: Option<&'static str>,
    renderer: Option<Entity<crate::diagrams::runtime::Manager>>,
    diagram_blocks: super::diagrams::Views,
    diagram: Option<(usize, Entity<crate::diagrams::runtime::DiagramView>)>,
    file: Entity<FileTab>,
    mode: Rc<Cell<Mode>>,
    focus: FocusHandle,
    images: Entity<super::Images>,
    source: SharedString,
    invalid: bool,
    limited: bool,
    generation: usize,
    columns: usize,
    state: Option<Entity<TextViewState>>,
    image_width: std::sync::Arc<std::sync::atomic::AtomicU32>,
    image_right: std::sync::Arc<std::sync::atomic::AtomicU32>,
    preview: Option<(usize, super::cached::CachedTextView)>,
    scroll: crate::file_scroll::Scroll,
    style: Option<(crate::file_tab::Look, guild_session::Language)>,
    _subscriptions: Vec<Subscription>,
}
impl gpui::EventEmitter<ModeChanged> for Pane {}
impl Pane {
    fn new(file: Entity<FileTab>, mode: Rc<Cell<Mode>>, cx: &mut Context<Self>) -> Self {
        let images = super::Images::new(&file.read(cx).path(), cx);
        let kind = guild_session::markdown::diagram_engine(file.read(cx).relative());
        let changed = cx.subscribe(&file, |this, _, _: &BufferChanged, cx| {
            this.invalid = true;
            cx.notify();
        });
        let look = cx.observe(&file, |pane, file, cx| {
            let kind = guild_session::markdown::diagram_engine(file.read(cx).relative());
            if pane.kind != kind {
                pane.kind = kind;
                pane.diagram = None;
                pane.diagram_blocks.lock().unwrap().clear();
                pane.preview = None;
                pane.state = None;
                pane.invalid = true;
                pane.generation = pane.generation.wrapping_add(1);
            }
            let path = file.read(cx).path();
            let directory = path.parent().unwrap_or(std::path::Path::new("/"));
            if pane.images.read(cx).directory != directory {
                pane.images = super::Images::new(&path, cx);
                pane.generation = pane.generation.wrapping_add(1);
                pane.preview = None;
            }
            cx.notify();
        });
        Self {
            kind,
            renderer: None,
            diagram_blocks: Default::default(),
            diagram: None,
            file,
            mode,
            focus: cx.focus_handle(),
            images,
            source: "".into(),
            invalid: true,
            limited: false,
            generation: 0,
            columns: 0,
            state: None,
            image_width: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1f32.to_bits())),
            image_right: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(1f32.to_bits())),
            preview: None,
            scroll: Default::default(),
            style: None,
            _subscriptions: vec![changed, look],
        }
    }

    /// A mode switch retains the editor entity, selection, undo history and dirt.
    pub fn set_mode(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode.replace(mode) == mode {
            return;
        }
        if mode == Mode::Editor {
            self.diagram = None;
            self.preview = None;
            // Keep native selection/scroll state; release only diagram consumers.
            self.diagram_blocks.lock().unwrap().clear();
            self.invalid = true;
        }
        self.scroll.reset();
        self.refresh(cx);
        if mode == Mode::Preview && !self.limited && *self.file.read(cx).state() == FileState::Text
        {
            self.file
                .update(cx, |file, cx| file.presentation_blur(window, cx));
        }
        window.focus(&self.focus_handle(cx), cx);
        cx.emit(ModeChanged);
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        if !self.invalid || self.mode.get() == Mode::Editor {
            return;
        }
        self.invalid = false;
        self.diagram_blocks.lock().unwrap().clear();
        let Some(editor) = self.file.read(cx).editor() else {
            self.source = "".into();
            self.limited = false;
            return;
        };
        let text = editor.read(cx).text();
        self.limited = text.len() > PREVIEW_LIMIT;
        self.source = if self.limited {
            "".into()
        } else {
            text.to_string().into()
        };
        self.columns = super::minimum_columns(&self.source);
        self.limited |= self.columns > 2048;
        if !self.limited
            && let Some(state) = &self.state
        {
            state.update(cx, |state, cx| state.set_text(&self.source, cx));
        }
        // TextViewState rejects obsolete parse revisions and retains its list and
        // selection across unchanged mode/theme updates. Rebuild only the styles.
        self.generation = self.generation.wrapping_add(1);
    }
}
impl Focusable for Pane {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        if self.mode.get() == Mode::Preview
            && !self.limited
            && (*self.file.read(cx).state() == FileState::Text
                || self.file.read(cx).source_loading())
        {
            self.focus.clone()
        } else {
            self.file.read(cx).focus_handle(cx)
        }
    }
}
impl Render for Pane {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.file.update(cx, |file, cx| file.realise(window, cx));
        let (look, ui) = self.file.read(cx).presentation_style();
        if self
            .style
            .as_ref()
            .is_none_or(|(style, language)| style != &look || *language != ui.0)
        {
            self.generation = self.generation.wrapping_add(1);
            self.style = Some((look.clone(), ui.0));
            if let Some(state) = &self.state {
                state.update(cx, |state, cx| state.invalidate_inline_layout(cx));
            }
        }
        self.refresh(cx);
        let theme = look.theme;
        let mode = self.mode.get();
        let loading = self.file.read(cx).source_loading();
        let usable = *self.file.read(cx).state() == FileState::Text && !self.limited;
        let source_visible = mode != Mode::Preview || !usable;
        if !usable || mode == Mode::Editor {
            self.diagram = None;
            self.diagram_blocks.lock().unwrap().clear();
            self.preview = None;
        }

        if !source_visible {
            self.file.read(cx).hide_source();
        }
        if !usable && !loading && self.focus.contains_focused(window, cx) {
            window.focus(&self.file.read(cx).focus_handle(cx), cx);
        }
        let mut content = div().flex_1().min_h_0().min_w_0().flex().overflow_hidden();
        if source_visible {
            content = content.child(
                div()
                    .debug_selector(|| "markdown-source".into())
                    .flex_1()
                    .flex_basis(gpui::px(0.))
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .child(self.file.clone()),
            );
        }
        let diagram =
            guild_session::markdown::diagram_engine(self.file.read(cx).relative()).is_some();
        if let Some(renderer) = &self.renderer {
            renderer.update(cx, |manager, cx| manager.restyle(theme, ui, cx));
        }
        if mode != Mode::Editor && usable && diagram {
            let mut preview = div()
                .id("diagram-reading")
                .track_focus(&self.focus)
                .flex_1()
                .min_w_0()
                .h_full()
                .overflow_y_scroll()
                .p(theme::space::PANEL_PADDING);
            if let Some(renderer) = &self.renderer {
                if self
                    .diagram
                    .as_ref()
                    .is_none_or(|(generation, _)| *generation != self.generation)
                {
                    let engine =
                        if guild_session::markdown::diagram_engine(self.file.read(cx).relative())
                            == Some("mermaid")
                        {
                            crate::diagrams::Engine::Mermaid
                        } else {
                            crate::diagrams::Engine::PlantUml
                        };
                    self.diagram = Some((
                        self.generation,
                        crate::diagrams::runtime::DiagramView::new(
                            renderer.clone(),
                            engine,
                            &self.source,
                            cx,
                        ),
                    ));
                }
                preview = preview.child(self.diagram.as_ref().unwrap().1.clone());
            } else {
                preview = preview.child(
                    div()
                        .debug_selector(|| "diagram-resources-unavailable".into())
                        .child(ui.text(Text::DiagramResourcesUnavailable)),
                );
            }
            content = content.child(preview);
        }
        if mode != Mode::Editor && usable && !diagram {
            let style = TextViewStyle {
                highlight_theme: cx.theme().highlight_theme.clone(),
                is_dark: !theme.kind.is_light(),
                table: gpui::StyleRefinement {
                    restrict_scroll_to_axis: Some(true),
                    overflow: gpui::PointRefinement {
                        x: Some(gpui::Overflow::Scroll),
                        y: None,
                    },
                    // A document table shares the page, not the component's
                    // independently themed popup/card surface.
                    ..gpui::StyleRefinement::default()
                        .bg(rgb(theme.surface.app))
                        .rounded_none()
                },
                table_head: gpui::StyleRefinement::default().font_weight(gpui::FontWeight::BOLD),
                table_cell: gpui::StyleRefinement::default().whitespace_nowrap(),
                // Keep source lines intact; only this block scrolls horizontally.
                code_block: gpui::StyleRefinement {
                    restrict_scroll_to_axis: Some(true),
                    overflow: gpui::PointRefinement {
                        x: Some(gpui::Overflow::Scroll),
                        y: None,
                    },
                    ..gpui::StyleRefinement::default()
                        .flex()
                        .flex_col()
                        .items_start()
                        .whitespace_nowrap()
                },
                ..Default::default()
            };
            if self
                .preview
                .as_ref()
                .is_none_or(|(generation, _)| *generation != self.generation)
            {
                let state = self
                    .state
                    .get_or_insert_with(|| cx.new(|cx| TextViewState::markdown(&self.source, cx)));
                let list = state.read(cx).list_state().clone();
                let view = TextView::new(state)
                    .plugin(super::diagrams::Diagrams {
                        manager: self.renderer.clone(),
                        views: self.diagram_blocks.clone(),
                        width: self.image_width.clone(),
                        right: self.image_right.clone(),
                    })
                    .plugin(super::inline_image::InlineImages {
                        images: self.images.clone(),
                        width: self.image_width.clone(),
                        right: self.image_right.clone(),
                    })
                    .plugin(super::html_image::HtmlImages(
                        super::inline_image::InlineImages {
                            images: self.images.clone(),
                            width: self.image_width.clone(),
                            right: self.image_right.clone(),
                        },
                    ))
                    .style(style)
                    .selectable(true)
                    .code_block_actions(move |block, _, _| {
                        let code = block.code();
                        div()
                            .id("copy-code")
                            .debug_selector(|| "markdown-copy-code".into())
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(theme::space::HEAD_BUTTON)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .bg(rgb(theme.surface.panel))
                            .hover(|el| {
                                el.bg(crate::theme::control_hover(theme, theme.surface.panel))
                            })
                            .hint_delay()
                            .tooltip(move |_, cx| {
                                cx.new(|_| {
                                    crate::view::tooltip::Tooltip::text(
                                        ui.text(Text::MenuCopy),
                                        theme,
                                    )
                                })
                                .into()
                            })
                            .child(
                                gpui::svg()
                                    .path("ui/menu-copy.svg")
                                    .size(theme::space::HEAD_ICON)
                                    .text_color(rgb(theme.text.secondary)),
                            )
                            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation()
                            })
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                    code.to_string(),
                                ));
                            })
                    })
                    .scrollable(true)
                    .size_full();
                self.preview = Some((
                    self.generation,
                    super::cached::CachedTextView::new(
                        view,
                        list,
                        self.image_width.clone(),
                        self.image_right.clone(),
                        state.clone(),
                        gpui::Hsla::from(rgb(theme.accent.brass))
                            .opacity(theme::alpha::MARKDOWN_SELECTION),
                    ),
                ));
            }
            let preview = self
                .preview
                .as_ref()
                .expect("preview initialized")
                .1
                .clone();
            content = content.child(
                div()
                    .id("markdown-reading")
                    .debug_selector(|| "markdown-reading".into())
                    .track_focus(&self.focus)
                    .flex_1()
                    .flex_basis(gpui::px(0.))
                    .min_w_0()
                    .h_full()
                    .overflow_hidden()
                    .when(source_visible, |el| {
                        el.border_l_1().border_color(rgb(theme.surface.divider))
                    })
                    .p(theme::space::PANEL_PADDING)
                    .child(
                        gpui::image_cache(self.images.clone()).size_full().child(
                            div()
                                .debug_selector(|| "markdown-document".into())
                                .size_full()
                                .child(self.scroll.wrap(preview)),
                        ),
                    ),
            );
        }
        let warning = if !source_visible {
            self.file.read(cx).warning_text()
        } else {
            None
        };
        div()
            .when(mode == Mode::Preview && loading, |el| {
                el.track_focus(&self.focus)
            })
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .bg(rgb(theme.surface.app))
            .text_color(rgb(theme.text.primary))
            .when(self.limited && mode != Mode::Editor, |el| {
                el.child(
                    div()
                        .flex_none()
                        .p(theme::space::TREE_PADDING)
                        .text_size(theme::size::META)
                        .child(ui.text(Text::MarkdownPreviewLimit)),
                )
            })
            .when_some(warning, |el, text| {
                el.child(
                    div()
                        .flex_none()
                        .p(theme::space::TREE_PADDING)
                        .text_color(rgb(theme.accent.rust))
                        .child(text),
                )
            })
            .child(content)
    }
}

/// Cached controls observe their pane without rebuilding the file buffer.
pub struct Controls {
    pane: Entity<Pane>,
    _subscriptions: Vec<Subscription>,
}
impl Render for Controls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pane = self.pane.read(cx);
        let (look, ui) = pane.file.read(cx).presentation_style();
        let theme = look.theme;
        let mode = pane.mode.get();
        div()
            .flex_none()
            .flex()
            .justify_end()
            .items_center()
            .h(theme::space::LOG_TOOLBAR)
            .gap(theme::space::STATUS_BUTTON_GAP)
            .children(
                [
                    (
                        Mode::Editor,
                        "markdown-editor",
                        "ui/mark-terminal.svg",
                        Text::MarkdownEditor,
                    ),
                    (
                        Mode::Split,
                        "markdown-split",
                        "ui/columns-2.svg",
                        Text::MarkdownSplit,
                    ),
                    (
                        Mode::Preview,
                        "markdown-preview",
                        "ui/workspace-book.svg",
                        Text::MarkdownPreview,
                    ),
                ]
                .into_iter()
                .map(|(value, id, icon, label)| {
                    div()
                        .id(id)
                        .debug_selector(move || id.into())
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(theme::space::HEAD_BUTTON)
                        .rounded(theme::radius::ROW)
                        .cursor_pointer()
                        .bg(rgb(if value == mode {
                            theme.accent.patina_bg
                        } else {
                            theme.surface.panel
                        }))
                        .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                        .child(
                            gpui::svg()
                                .path(icon)
                                .size(theme::space::HEAD_ICON)
                                .text_color(rgb(if value == mode {
                                    theme.accent.patina
                                } else {
                                    theme.text.secondary
                                })),
                        )
                        .hint_delay()
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::view::tooltip::Tooltip::text(ui.text(label), theme))
                                .into()
                        })
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.pane
                                .update(cx, |pane, cx| pane.set_mode(value, window, cx))
                        }))
                }),
            )
    }
}

#[cfg(test)]
mod scroll_tests;

#[cfg(test)]
mod diagram_relocation_tests {
    fn wait(cx: &mut gpui::VisualTestContext, selector: &'static str) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(20));
            cx.run_until_parked();
            if cx.debug_bounds(selector).is_some() {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    use super::*;
    #[gpui::test]
    fn renamed_diagram_selects_the_new_engine_without_replacing_its_buffer(
        cx: &mut gpui::TestAppContext,
    ) {
        let state = tempfile::tempdir().unwrap();
        std::fs::write(state.path().join("flow.mmd"), "flowchart LR\n A-->B").unwrap();
        let mut kept = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    state.path(),
                    std::path::Path::new("flow.mmd"),
                    crate::file_tab::Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    crate::i18n::Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let manager = cx.new(|cx| {
                crate::diagrams::runtime::Manager::new(
                    Ok(crate::diagrams::Resources::from_directory(
                        state.path().join("missing bundle"),
                    )),
                    cx,
                )
            });
            let pane = Presentation::for_file(&file, Mode::Preview, cx).unwrap();
            pane.attach_renderer(&manager, cx);
            pane.restore(Mode::Preview, cx);
            kept = Some(file);
            gpui_component::Root::new(pane.view, window, cx)
        });
        cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(800.)));
        wait(cx, "diagram-error-mermaid");
        assert!(cx.debug_bounds("diagram-error-mermaid").is_some());
        let file = kept.unwrap();
        let editor = cx.update(|window, cx| {
            let editor = file.read(cx).editor().unwrap().entity_id();
            file.update(cx, |file, cx| file.relocating(true, window, cx));
            std::fs::rename(
                state.path().join("flow.mmd"),
                state.path().join("flow.puml"),
            )
            .unwrap();
            file.update(cx, |file, cx| {
                file.relocate(std::path::Path::new("flow.puml"), window, cx)
            });
            editor
        });
        wait(cx, "diagram-error-plantuml");
        assert!(
            cx.debug_bounds("diagram-error-plantuml").is_some(),
            "renaming switches the preview engine even with an unchanged buffer"
        );
        assert!(cx.debug_bounds("diagram-error-mermaid").is_none());
        cx.update(|_, cx| assert_eq!(file.read(cx).editor().unwrap().entity_id(), editor));
    }
}

#[cfg(test)]
mod diagram_geometry_tests {
    use super::*;
    use crate::diagrams::{
        Engine, Resources,
        runtime::{Key, Manager, Output},
    };
    #[gpui::test]
    #[ignore = "requires explicit prepared bundled resources"]
    fn repeated_fences_fit_independently_and_settle_without_draw_nudges(
        cx: &mut gpui::TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let resources = Resources::from_directory(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/diagram-runtime"),
        );
        let source = "flowchart LR\n A[\"Широкий исходный текст\"]-->B[\"Повторённая диаграмма\"]-->C[\"Предпросмотр\"]";
        let quoted = source
            .lines()
            .map(|line| format!("> {line}"))
            .collect::<Vec<_>>()
            .join("\n");
        std::fs::write(
            temp.path().join("repeated.md"),
            format!("```mermaid\n{source}\n```\n\n> ```mermaid\n{quoted}\n> ```"),
        )
        .unwrap();
        let mut held = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    temp.path(),
                    std::path::Path::new("repeated.md"),
                    crate::file_tab::Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    crate::i18n::Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let manager = cx.new(|cx| Manager::new(Ok(resources), cx));
            let pane = Presentation::for_file(&file, Mode::Preview, cx).unwrap();
            pane.attach_renderer(&manager, cx);
            held = Some((pane.clone(), manager));
            gpui_component::Root::new(pane.view, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(700.), gpui::px(600.)));
        visual.run_until_parked();
        let (pane, manager) = held.unwrap();
        let views = visual.update(|_, cx| {
            pane.view
                .read(cx)
                .diagram_blocks
                .lock()
                .unwrap()
                .values()
                .cloned()
                .collect::<Vec<_>>()
        });
        assert_eq!(
            views.len(),
            2,
            "identical fences need independent native layout views"
        );
        assert_ne!(views[0].entity_id(), views[1].entity_id());
        let notifications = Rc::new(Cell::new(0));
        let _watches = visual.update(|_, cx| {
            views
                .iter()
                .map(|view| {
                    let notifications = notifications.clone();
                    cx.observe(view, move |_, _| {
                        notifications.set(notifications.get() + 1);
                        assert!(
                            notifications.get() < 32,
                            "fence geometry must settle, not redraw indefinitely"
                        );
                    })
                })
                .collect::<Vec<_>>()
        });
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            visual
                .executor()
                .advance_clock(std::time::Duration::from_millis(20));
            visual.run_until_parked();
            if visual.update(|_, cx| {
                manager.update(cx, |m, _| {
                    matches!(
                        m.output(&Key::new(Engine::Mermaid, source)),
                        Output::Image(_)
                    )
                })
            }) {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let selectors = views
            .iter()
            .map(|view| {
                &*Box::leak(format!("diagram-instance-{}", view.entity_id()).into_boxed_str())
            })
            .collect::<Vec<&'static str>>();
        let mut bounds = selectors
            .iter()
            .map(|selector| visual.debug_bounds(selector).unwrap())
            .collect::<Vec<_>>();
        bounds.sort_by(|a, b| f32::from(a.left()).total_cmp(&f32::from(b.left())));
        let page_right = visual.update(|_, cx| {
            f32::from_bits(
                pane.view
                    .read(cx)
                    .image_right
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
        });
        assert!(
            bounds[0].size.width > bounds[1].size.width + gpui::px(8.),
            "blockquote must fit its narrower width independently: {bounds:?}"
        );
        let logical = visual.update(|_, cx| {
            manager.update(cx, |m, _| {
                match m.output(&Key::new(Engine::Mermaid, source)) {
                    Output::Image(frame) => frame.logical,
                    _ => panic!("render completed"),
                }
            })
        });
        for bound in &bounds {
            assert!(f32::from(bound.right()) <= page_right + 1.);
            let expected =
                f32::from(bound.size.width) * f32::from(logical.height) / f32::from(logical.width);
            assert!((f32::from(bound.size.height) - expected).abs() < 1.);
        }
        let settled = notifications.get();
        eprintln!("repeated fences settled: bounds={bounds:?}, notifications={settled}");
        visual.run_until_parked();
        assert_eq!(notifications.get(), settled);
        let mut final_bounds = selectors
            .iter()
            .map(|selector| visual.debug_bounds(selector).unwrap())
            .collect::<Vec<_>>();
        final_bounds.sort_by(|a, b| f32::from(a.left()).total_cmp(&f32::from(b.left())));
        assert_eq!(
            final_bounds, bounds,
            "each occurrence keeps stable independent bounds"
        );
        assert_eq!(
            notifications.get(),
            settled,
            "bounds reads must not restart redraws"
        );
    }
    #[gpui::test]
    #[ignore = "requires explicit prepared bundled resources"]
    fn hidden_fence_completion_and_eviction_remeasure_existing_state(
        cx: &mut gpui::TestAppContext,
    ) {
        let temp = tempfile::tempdir().unwrap();
        let resources = Resources::from_directory(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/diagram-runtime"),
        );
        let source = "flowchart TD\n A-->B";
        let text = format!(
            "{}\n\n```mermaid\n{source}\n```\n\nEnd.",
            (0..40)
                .map(|i| format!("Paragraph {i} stays selectable."))
                .collect::<Vec<_>>()
                .join("\n\n")
        );
        std::fs::write(temp.path().join("doc.md"), text).unwrap();
        let mut held = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let file = cx.new(|cx| {
                FileTab::open(
                    temp.path(),
                    std::path::Path::new("doc.md"),
                    crate::file_tab::Look {
                        family: "monospace".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    crate::i18n::Ui(guild_session::Language::English),
                    window,
                    cx,
                )
            });
            let manager = cx.new(|cx| Manager::new(Ok(resources), cx));
            let pane = Presentation::for_file(&file, Mode::Preview, cx).unwrap();
            pane.attach_renderer(&manager, cx);
            held = Some((pane.clone(), manager));
            gpui_component::Root::new(pane.view, window, cx)
        });
        visual.simulate_resize(gpui::size(gpui::px(700.), gpui::px(230.)));
        let (pane, manager) = held.unwrap();
        let tick = |visual: &mut gpui::VisualTestContext| {
            visual
                .executor()
                .advance_clock(std::time::Duration::from_millis(20));
            visual.run_until_parked();
            visual.update(|w, cx| w.draw(cx).clear(cx));
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        let state = visual.update(|_, cx| pane.view.read(cx).state.clone().unwrap());
        let identity = state.entity_id();
        let list = visual.update(|_, cx| state.read(cx).list_state().clone());
        visual.update(|_, cx| {
            list.scroll_to_end();
            cx.notify(pane.view.entity_id());
        });
        tick(visual);
        assert!(visual.debug_bounds("diagram-pending-mermaid").is_some());
        visual.update(|_, cx| {
            list.scroll_to(gpui::ListOffset {
                item_ix: 5,
                offset_in_item: gpui::px(3.),
            });
            state.update(cx, |s, cx| s.select_all(cx));
            cx.notify(pane.view.entity_id());
        });
        tick(visual);
        let selected = visual.update(|_, cx| state.read(cx).selected_text());
        assert!(!selected.is_empty());
        let offset = list.logical_scroll_top();
        assert!(offset.item_ix > 0);
        assert_eq!(
            list.item_is_below_viewport(list.item_count() - 2),
            Some(true)
        );
        let height = || list.max_offset_for_scrollbar().y + list.viewport_bounds().size.height;
        let pending = height();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
        loop {
            tick(visual);
            if visual.update(|_, cx| {
                manager.update(cx, |m, _| {
                    matches!(
                        m.output(&Key::new(Engine::Mermaid, source)),
                        Output::Image(_)
                    )
                })
            }) {
                break;
            }
            assert!(std::time::Instant::now() < until);
        }
        tick(visual);
        let complete = height();
        assert_ne!(
            complete, pending,
            "hidden completion changes document extent before visiting row"
        );
        // Fill the bounded cache with large real diagrams to evict the hidden fence.
        for i in 0..10 {
            let large = format!(
                "%%{{init: {{'themeVariables': {{'fontSize':'950px'}}}}}}%%\nflowchart TD\n A[A]\n%% distinct {i}"
            );
            let key = Key::new(Engine::Mermaid, &large);
            visual.update(|_, cx| manager.update(cx, |m, cx| m.acquire(key.clone(), cx)));
            let until = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                tick(visual);
                let output = visual.update(|_, cx| manager.update(cx, |m, _| m.output(&key)));
                if !matches!(output, Output::Pending) {
                    assert!(
                        matches!(output, Output::Image(_)),
                        "large fixture must render"
                    );
                    break;
                }
                assert!(std::time::Instant::now() < until);
            }
        }
        tick(visual);
        assert!(visual.update(|_, cx| manager.update(cx, |m, _| matches!(
            m.output(&Key::new(Engine::Mermaid, source)),
            Output::Error(_)
        ))));
        assert_ne!(
            height(),
            complete,
            "hidden eviction remeasures its image into the error geometry"
        );
        visual.update(|_, cx| {
            assert_eq!(
                pane.view.read(cx).state.as_ref().unwrap().entity_id(),
                identity
            );
            assert_eq!(state.read(cx).selected_text(), selected);
        });
        assert_eq!(list.logical_scroll_top().item_ix, offset.item_ix);
        assert_eq!(
            list.logical_scroll_top().offset_in_item,
            offset.offset_in_item
        );
    }
}
