//! Shared immutable source surface: one vertical list, three horizontal positions.
use crate::view::tooltip::Hint as _;
use crate::{
    diff::document::{DisplayRow, intraline},
    diff_tab::{DiffEvent, DiffTab, HitLine, Line},
    theme,
};
use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, ContentMask, Context, DispatchPhase, Focusable, Font, MouseButton,
    SharedString, TextRun, Window, canvas, div, point, px, rgb, size,
};
use gpui_component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
    scroll::{Scrollbar, ScrollbarHandle, ScrollbarMode},
};
use gpui_kit::TestSupportExt;
use guild_session::DiffView;

impl DiffTab {
    pub(super) fn prepare_surface(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.viewer.hits.borrow_mut().clear();
        self.refresh_occurrences();
        // Establish the focused source's actual Loading/Unavailable state before
        // measuring it, rather than resizing its gutter on the next wheel event.
        self.ensure_authors_for(self.current_file(), cx);
        self.prepare_number_width(window);
        self.prepare_authors(window);
        let width = f32::from(self.viewer.viewport.size.width).max(1.);
        let rem = f32::from(window.rem_size());
        let gutter = f32::from(self.viewer.number_width)
            + rem * theme::space::TREE_GAP.0 * 2.
            + f32::from(Self::blame_strip_width(window));
        let desired = |side: usize| {
            gutter
                + self.look().font_size * 4.
                + if self.authors.enabled[side] {
                    f32::from(self.authors.widths[side])
                } else {
                    0.
                }
        };
        let minimum = (desired(0) / width).clamp(0.2, 0.5);
        let maximum =
            1. - ((desired(1) + rem * theme::space::DIFF_CONNECTOR.0) / width).clamp(0.2, 0.5);
        self.viewer.minimum_ratio = minimum;
        self.viewer.maximum_ratio = maximum;
        // Unified has no column divider. Its annotation budget must not
        // overwrite the side-by-side ratio retained for a later mode switch.
        if self.mode() == DiffView::SideBySide {
            self.viewer.ratio = self
                .viewer
                .ratio
                .clamp(self.viewer.minimum_ratio, self.viewer.maximum_ratio);
        }
        let key = format!(
            "{}:{}:{:?}",
            self.look().family,
            self.look().font_size,
            self.look().theme.kind
        );
        if self.viewer.width_key != key {
            self.viewer.width_key = key;
            for h in &self.viewer.horizontal {
                h.0.borrow_mut().width = px(0.);
            }
        }
        // Full-line widths are measured by visible source cells and retained
        // for this source/font generation. Offscreen folds schedule no work.
    }

    pub(super) fn render_inspection_toolbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.look().theme;
        let targets = self.change_targets();
        let current = self.current_change().map_or(0, |i| i + 1);
        let (file, total) = self
            .stepping()
            .unwrap_or((self.current_file(), self.files().len()));
        let unified = self.mode() == DiffView::Unified;
        let mut toolbar = div()
            .id("diff-toolbar")
            .debug_selector(|| "diff-toolbar".into())
            .flex_none()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme::space::TREE_GAP / 2.)
            .min_h(theme::space::DIFF_TOOLBAR)
            .min_w_0()
            .px(theme::space::TREE_PADDING)
            .bg(rgb(t.surface.panel))
            .border_b_1()
            .border_color(rgb(t.surface.divider));
        for (id, label, icon, forward) in [
            (
                "diff-previous",
                self.viewer_label("Previous change", "Предыдущее изменение"),
                "ui/arrow-up.svg",
                false,
            ),
            (
                "diff-next",
                self.viewer_label("Next change", "Следующее изменение"),
                "ui/arrow-down.svg",
                true,
            ),
        ] {
            toolbar = toolbar.child(
                diff_icon_button(id, label, icon, t, false, cx)
                    .disabled(!self.can_step_change(forward))
                    .on_click(cx.listener(move |this, _, _, cx| this.step_change(forward, cx))),
            );
        }
        toolbar = toolbar.child(
            div()
                .id("diff-change-count")
                .w(theme::space::HEAD_BUTTON * 2.)
                .flex_none()
                .text_size(theme::size::META)
                .text_color(rgb(t.text.secondary))
                .whitespace_nowrap()
                .hint(
                    self.viewer_label("Current / total changes", "Текущее / всего изменений"),
                    t,
                )
                .child(format!("{current}/{}", targets.len())),
        );
        toolbar = toolbar.children(self.render_stage_button(cx));
        for (id, label, icon, forward, enabled) in [
            (
                "diff-file-previous",
                self.viewer_label("Previous file", "Предыдущий файл"),
                "icons/chevron-left.svg",
                false,
                file > 0,
            ),
            (
                "diff-file-next",
                self.viewer_label("Next file", "Следующий файл"),
                "icons/chevron-right.svg",
                true,
                file + 1 < total,
            ),
        ] {
            toolbar = toolbar.child(
                diff_icon_button(id, label, icon, t, false, cx)
                    .disabled(!enabled)
                    .on_click(cx.listener(move |this, _, _, cx| this.step_file(forward, cx))),
            );
        }
        let files_label = self.viewer_label("Choose inspection file", "Выбрать файл сравнения");
        toolbar = toolbar.child(
            Button::new("diff-file-counter")
                .debug_selector(|| "diff-file-counter".into())
                .label(format!(
                    "{}/{}",
                    if total == 0 { 0 } else { file + 1 },
                    total
                ))
                .accessibility_label(files_label)
                .when(self.file_picker.is_none(), |b| b.hint(files_label, t))
                .w(theme::space::HEAD_BUTTON * 2.)
                .small()
                .flex_none()
                .custom(super::super::controls::ghost(
                    t,
                    self.file_picker.is_some(),
                    cx,
                ))
                .disabled(total == 0)
                .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                    let at = event.position()
                        + point(px(0.), window.rem_size() * theme::space::HEAD_BUTTON.0 / 2.);
                    this.open_file_picker(at, window, cx);
                })),
        );
        toolbar = toolbar.child(
            diff_icon_button(
                "diff-edit",
                self.viewer_label("Open workspace source", "Открыть исходный файл"),
                "ui/menu-rename.svg",
                t,
                false,
                cx,
            )
            .disabled(
                self.files().is_empty()
                    || self
                        .files()
                        .iter()
                        .all(|f| f.state == crate::diff_tab::State::Loading),
            )
            .on_click(cx.listener(|this, _, _, cx| this.reveal_source(cx))),
        );
        toolbar = toolbar.children(self.render_answers(self.ui(), cx));
        let mut modes = div()
            .ml_auto()
            .flex()
            .flex_none()
            .gap(theme::space::TREE_GAP / 2.);
        for (id, label, icon, active) in [
            (
                "diff-side-by-side",
                self.viewer_label("Side by side", "Рядом"),
                "ui/columns-2.svg",
                !unified,
            ),
            (
                "diff-unified",
                self.viewer_label("Unified", "Вместе"),
                "ui/tool-stop.svg",
                unified,
            ),
        ] {
            modes = modes.child(
                diff_icon_button(id, label, icon, t, active, cx)
                    .toggled(active)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !active {
                            this.toggle_mode(cx);
                        }
                    })),
            );
        }
        toolbar.child(modes).into_any_element()
    }

    pub(super) fn render_revisions(&self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.look().theme;
        let labels = self.revision_labels(self.current_file());
        let fallback = self
            .files()
            .get(self.current_file())
            .map(|file| file.path.display().to_string())
            .unwrap_or_default();
        let header = |side: usize| {
            let full = labels[side].clone();
            let (revision, path) = full
                .split_once(':')
                .map_or((full.as_str(), fallback.as_str()), |(r, p)| (r, p));
            let hash = revision.trim_end_matches('^');
            let is_hash = hash.len() >= 7 && hash.bytes().all(|byte| byte.is_ascii_hexdigit());
            let revision = compact_revision(revision);
            let label = if side == 0 {
                self.viewer_label("Before", "До")
            } else {
                self.viewer_label("After", "После")
            };
            div()
                .id(("diff-revision", side))
                .debug_selector(move || format!("diff-revision-{side}"))
                .hint(full.clone(), t)
                .flex()
                .items_center()
                .gap(theme::space::TREE_GAP)
                .h_full()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .px(theme::space::TREE_PADDING)
                .text_size(theme::size::META)
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(t.text.secondary))
                        .child(label),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(if is_hash {
                            t.accent.patina
                        } else {
                            t.text.secondary
                        }))
                        .when(is_hash, |revision| {
                            revision.font_family(self.look().family.clone())
                        })
                        .child(revision),
                )
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(t.text.secondary))
                        .child(path.to_owned()),
                )
        };
        let unified = self.mode() == DiffView::Unified;
        let ratio = if unified { 0.5 } else { self.viewer.ratio };
        div()
            .id("diff-revisions")
            .debug_selector(|| "diff-revisions".into())
            .flex_none()
            .flex()
            .items_center()
            .h(theme::space::TREE_ROW)
            .bg(rgb(t.surface.sunken))
            .border_b_1()
            .border_color(rgb(t.surface.divider))
            .pr(theme::space::MARK_BAR * 2.)
            .child(header(0).w(gpui::relative(ratio)).flex_none())
            .when(!unified, |row| {
                row.child(
                    Button::new("diff-divider-keyboard")
                        .label("↔")
                        .w(theme::space::DIFF_CONNECTOR)
                        .min_w_0()
                        .px_0()
                        .accessibility_label(format!(
                            "{} · {}%",
                            self.viewer_label(
                                "Column divider, use arrow keys",
                                "Разделитель колонок, используйте стрелки"
                            ),
                            (ratio * 100.).round()
                        ))
                        .hint(
                            self.viewer_label(
                                "Column divider, use arrow keys",
                                "Разделитель колонок, используйте стрелки",
                            ),
                            t,
                        )
                        .small()
                        .custom(super::super::controls::ghost(t, false, cx))
                        .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                            let delta = match event.keystroke.key.as_str() {
                                "left" => -0.025,
                                "right" => 0.025,
                                _ => return,
                            };
                            this.viewer.ratio = (this.viewer.ratio + delta)
                                .clamp(this.viewer.minimum_ratio, this.viewer.maximum_ratio);
                            cx.stop_propagation();
                            cx.notify();
                        })),
                )
            })
            .child(header(1).flex_1())
            .into_any_element()
    }

    pub(super) fn render_horizontal_bars(&self, window: &Window) -> AnyElement {
        let t = self.look().theme;
        let unified = self.mode() == DiffView::Unified;
        let bar = |index: usize| {
            let base = self.viewer.number_width * if index == 2 { 2. } else { 1. }
                + window.rem_size() * theme::space::TREE_GAP.0 * 2.
                + if index == 2 {
                    px(0.)
                } else {
                    Self::blame_strip_width(window)
                };
            let authors = if index == 2 {
                self.authors_width(0, true, window) + self.authors_width(1, true, window)
            } else {
                self.authors_width(index, false, window)
            };
            div()
                .when(index == 0, |bar| bar.pr(base + authors))
                .when(index != 0, |bar| bar.pl(base + authors))
                .relative()
                .h(theme::space::MARK_BAR)
                .min_w_0()
                .child(
                    div().relative().size_full().child(
                        Scrollbar::horizontal(&self.viewer.horizontal[index])
                            .id(("diff-horizontal", index))
                            .mode(ScrollbarMode::Always)
                            .viewport_from_layout()
                            .styles(|s| {
                                s.thumb(|s| s.bg(rgb(t.text.gutter)))
                                    .track(|s| s.bg(rgb(t.surface.panel).into()))
                            }),
                    ),
                )
        };
        let mut row = div()
            .pr(theme::space::MARK_BAR * 2.)
            .flex_none()
            .flex()
            .h(theme::space::MARK_BAR)
            .bg(rgb(t.surface.panel));
        if unified {
            row = row.child(bar(2).flex_1());
        } else {
            row = row
                .child(bar(0).w(gpui::relative(self.viewer.ratio)).flex_none())
                .child(div().flex_none().w(theme::space::DIFF_CONNECTOR))
                .child(bar(1).flex_1());
        }
        row.into_any_element()
    }

    pub(super) fn render_source_row(
        &mut self,
        index: usize,
        file: usize,
        row: &DisplayRow,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.look().theme;
        match row {
            DisplayRow::NoNewline { side } => div()
                .w_full()
                .bg(rgb(t.surface.app))
                .h(theme::space::TREE_ROW)
                .text_size(theme::size::META)
                .text_color(rgb(t.text.faint))
                .child(format!(
                    "{} · {}",
                    if *side == 0 {
                        self.viewer_label("Before", "До")
                    } else {
                        self.viewer_label("After", "После")
                    },
                    self.viewer_label(
                        "No newline at end of file",
                        "Нет перевода строки в конце файла"
                    )
                ))
                .into_any_element(),
            DisplayRow::Fold { equal, count } => {
                let equal = *equal;
                // Non-source rows own an opaque surface over the shared
                // gutter underlay, so divider rules cannot cross their labels.
                div()
                    .w_full()
                    .h(theme::space::TREE_ROW)
                    .bg(rgb(t.surface.app))
                    .child(
                        Button::new(("diff-fold", index))
                            .label(format!(
                                "⋯ {} {count}",
                                self.viewer_label(
                                    "Expand unchanged lines:",
                                    "Раскрыть строки без изменений:"
                                )
                            ))
                            .accessibility_label(self.viewer_label(
                                "Expand full unchanged context",
                                "Раскрыть полный неизменённый контекст",
                            ))
                            .hint(
                                self.viewer_label(
                                    "Expand full unchanged context",
                                    "Раскрыть полный неизменённый контекст",
                                ),
                                t,
                            )
                            .custom(super::super::controls::ghost(t, false, cx))
                            .small()
                            .w_full()
                            .h(theme::space::TREE_ROW)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.expand_context(file, equal, cx)
                            })),
                    )
                    .into_any_element()
            }
            DisplayRow::Text { old, new, block } => {
                if self.mode() == DiffView::Unified {
                    let side = usize::from(new.is_some());
                    let line = if side == 1 { *new } else { *old };
                    self.source_half(index, file, side, line, *block, true, window, cx)
                } else {
                    let left = self.source_half(index, file, 0, *old, *block, false, window, cx);
                    let right = self.source_half(index, file, 1, *new, *block, false, window, cx);
                    let connector = self.connector(file, *old, *new, *block, cx);
                    div()
                        .flex()
                        .w_full()
                        .h(theme::space::TREE_ROW)
                        .child(
                            div()
                                .w(gpui::relative(self.viewer.ratio))
                                .min_w_0()
                                .h_full()
                                .child(left),
                        )
                        .child(connector)
                        .child(div().flex_1().min_w_0().h_full().child(right))
                        .into_any_element()
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn source_half(
        &mut self,
        index: usize,
        file: usize,
        side: usize,
        line: Option<usize>,
        block: Option<usize>,
        unified: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_authors_for(file, cx);
        let t = self.look().theme;
        let Some(doc) = self
            .files()
            .get(file)
            .and_then(|f| self.documents.get(&f.path))
            .cloned()
        else {
            return div().into_any_element();
        };
        let old_context = if unified {
            match &self.lines()[index] {
                Line::Source {
                    row:
                        DisplayRow::Text {
                            old: Some(old),
                            new: Some(_),
                            ..
                        },
                    ..
                } => Some(*old),
                _ => None,
            }
        } else {
            None
        };
        let text = line.map(|n| doc.sources[side].line(n)).unwrap_or("");
        let changed = block.is_some() && line.is_some();
        let paint = self.center_row_paint(index, side);
        let bg = paint.fill;
        let marker = if changed {
            if side == 0 { "−" } else { "+" }
        } else {
            ""
        };
        let spans = line
            .and_then(|n| {
                self.paint(&self.files()[file].path).and_then(|p| {
                    let paint = if side == 0 { &p.old } else { &p.new };
                    paint.of_line(n + 1, text).map(|s| s.to_vec())
                })
            })
            .unwrap_or_default();
        let mut marks = Vec::new();
        if let (Some(block), Some(line)) = (block, line) {
            let b = &doc.blocks[block];
            let offset = line - if side == 0 { b.old.start } else { b.new.start };
            if offset < b.old.len().min(b.new.len()) {
                let key = (file, block, offset);
                let spans = self.viewer.refinements.entry(key).or_insert_with(|| {
                    intraline(
                        doc.sources[0].line(b.old.start + offset),
                        doc.sources[1].line(b.new.start + offset),
                    )
                });
                marks = spans[side].clone();
                // Round refinement out to complete composed characters.
                let boundaries = crate::system::grapheme_boundaries(text);
                for mark in &mut marks {
                    mark.start = boundaries
                        .iter()
                        .copied()
                        .take_while(|b| *b <= mark.start)
                        .last()
                        .unwrap_or(0);
                    mark.end = boundaries
                        .iter()
                        .copied()
                        .find(|b| *b >= mark.end)
                        .unwrap_or(text.len());
                }
            }
        }
        // GLD-026: unified has no ribbon; the block's hunk action takes the
        // marker's place at its first row.
        let control = (unified && block.is_some())
            .then(|| {
                let b = block?;
                let geometry = doc.blocks.get(b)?;
                let (old, new) = match &self.lines()[index] {
                    Line::Source {
                        row: DisplayRow::Text { old, new, .. },
                        ..
                    } => (*old, *new),
                    _ => return None,
                };
                first_row(geometry, old, new)
                    .then(|| {
                        self.hunk_control(
                            "diff-hunk-unified",
                            file,
                            b,
                            theme::space::TREE_GAP * 2.,
                            cx,
                        )
                    })
                    .flatten()
            })
            .flatten();
        let shaped = shape(text, &spans, &self.look().clone(), window);
        let width = shaped.width;
        let horizontal = if unified { 2 } else { side };
        {
            let mut h = self.viewer.horizontal[horizontal].0.borrow_mut();
            h.width = h.width.max(width + px(self.look().font_size));
        }
        let scroll = self.viewer.horizontal[horizontal].clone();
        let hits = self.viewer.hits.clone();
        let selection = self.viewer.selection.clone();
        let occurrence_side = if old_context.is_some() {
            selection.as_ref().map_or(side, |s| s.side)
        } else {
            side
        };
        let occurrence_line = if occurrence_side == side {
            line
        } else {
            old_context
        };
        let occurrences = self
            .viewer
            .occurrences
            .as_ref()
            .filter(|o| o.selection.path == self.files()[file].path)
            .and_then(|o| {
                occurrence_line
                    .and_then(|line| doc.sources[occurrence_side].lines.get(line))
                    .map(|line| crate::source_occurrences::within(&o.ranges[occurrence_side], line))
            })
            .unwrap_or_default();
        let path = self.files()[file].path.clone();
        let row_height = px(self.row_px);
        let font_size = self.look().font_size;
        let text_canvas = canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, cx| {
                let mut h = scroll.0.borrow_mut();
                h.bounds = bounds;
                h.offset = h.offset.min((h.width - bounds.size.width).max(px(0.)));
                let offset = h.offset;
                drop(h);
                if let Some(line) = line {
                    hits.borrow_mut().push(HitLine {
                        file,
                        side,
                        line,
                        bounds,
                        shaped: shaped.clone(),
                        horizontal,
                    });
                }
                if let Some(line) = old_context {
                    hits.borrow_mut().push(HitLine {
                        file,
                        side: 0,
                        line,
                        bounds,
                        shaped: shaped.clone(),
                        horizontal,
                    });
                }
                let origin = point(bounds.left() - offset, bounds.top());
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for range in &occurrences {
                        let start = shaped.x_for_index(range.start);
                        let end = shaped.x_for_index(range.end);
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                point(origin.x + start, origin.y),
                                size((end - start).max(px(1.)), row_height),
                            ),
                            t.identifier_occurrence(),
                        ));
                    }
                    for mark in &marks {
                        let x = shaped.x_for_index(mark.start);
                        let end = shaped.x_for_index(mark.end);
                        let color = if side == 0 {
                            t.accent.rust
                        } else {
                            t.accent.green
                        };
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                point(origin.x + x, origin.y),
                                size((end - x).max(px(1.)), row_height),
                            ),
                            rgb(color).opacity(theme::alpha::DIFF_INTRALINE),
                        ));
                    }
                    if let Some(sel) = &selection
                        && sel.path == path
                        && let Some(selected_line) = if sel.side == side {
                            line
                        } else if sel.side == 0 {
                            old_context
                        } else {
                            None
                        }
                        && let Some(source) = doc.sources.get(sel.side)
                        && let Some(range) = source.lines.get(selected_line)
                    {
                        let selected = sel.range();
                        if selected.start < range.end && selected.end > range.start {
                            let start = selected.start.saturating_sub(range.start).min(range.len());
                            let end = selected.end.saturating_sub(range.start).min(range.len());
                            let x = shaped.x_for_index(start);
                            let end = shaped.x_for_index(end);
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(origin.x + x, origin.y),
                                    size((end - x).max(px(2.)), row_height),
                                ),
                                rgb(t.accent.patina).opacity(theme::alpha::DIFF_SELECTION),
                            ));
                        }
                    }
                    let _ = shaped.paint(
                        point(origin.x, origin.y + (row_height - px(font_size * 1.2)) / 2.),
                        px(font_size * 1.2),
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                });
            },
        )
        .size_full();
        let code = div()
            .debug_selector(move || format!("diff-source-{side}-{index}"))
            .id((
                if side == 0 {
                    "diff-old-code"
                } else {
                    "diff-new-code"
                },
                index,
            ))
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .cursor(gpui::CursorStyle::IBeam)
            .child(text_canvas)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| this.begin_selection(event, window, cx)),
            )
            .test_support();
        let marker = div()
            .flex_none()
            .w(theme::space::TREE_GAP * 2.)
            .flex()
            .items_center()
            .text_color(rgb(if side == 0 {
                t.accent.rust
            } else {
                t.accent.green
            }))
            .map(|this| match control {
                Some(control) => this.child(control),
                None => this.child(marker),
            });
        let mut row = div()
            .id((if side == 0 { "diff-left" } else { "diff-right" }, index))
            .flex()
            .h(theme::space::TREE_ROW)
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .bg(rgb(bg))
            .font_family(self.look().family.clone())
            .text_size(px(font_size))
            .whitespace_nowrap()
            .relative()
            .child({
                let annotation = if unified {
                    px(0.)
                } else {
                    self.authors_width(side, false, window)
                };
                canvas(
                    |bounds, _, _| bounds,
                    move |bounds, _, window, _| {
                        let start = if !unified && side == 1 {
                            bounds.left() + annotation
                        } else {
                            bounds.left()
                        };
                        let end = if !unified && side == 0 {
                            bounds.right() - annotation
                        } else {
                            bounds.right()
                        };
                        if paint.top {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(start, bounds.top()),
                                    size((end - start).max(px(0.)), px(1.)),
                                ),
                                rgb(paint.edge),
                            ));
                        }
                        if paint.bottom {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(start, bounds.bottom() - px(1.)),
                                    size((end - start).max(px(0.)), px(1.)),
                                ),
                                rgb(paint.edge),
                            ));
                        }
                    },
                )
                .absolute()
                .size_full()
            });
        if unified {
            let (old, new) = match &self.lines()[index] {
                Line::Source {
                    row: DisplayRow::Text { old, new, .. },
                    ..
                } => (*old, *new),
                _ => (None, None),
            };
            for (side, line) in [(0, old), (1, new)] {
                if self.authors.enabled[side] {
                    row = row.child(self.authors_cell(
                        index,
                        file,
                        side,
                        line,
                        self.authors_width(side, true, window),
                        cx,
                    ));
                }
            }
            row.child(self.numbers_cell(index, file, 0, old, cx))
                .child(self.numbers_cell(index, file, 1, new, cx))
                .child(marker)
                .child(code)
                .into_any_element()
        } else if side == 0 {
            row = row
                .child(code)
                .child(marker)
                .child(self.blame_click_strip(index, 0, cx))
                .child(self.numbers_cell(index, file, 0, line, cx));
            if self.authors.enabled[0] {
                row = row.child(self.authors_cell(
                    index,
                    file,
                    0,
                    line,
                    self.authors_width(0, false, window),
                    cx,
                ));
            }
            row.into_any_element()
        } else {
            if self.authors.enabled[1] {
                row = row.child(self.authors_cell(
                    index,
                    file,
                    1,
                    line,
                    self.authors_width(1, false, window),
                    cx,
                ));
            }
            row.child(self.numbers_cell(index, file, 1, line, cx))
                .child(self.blame_click_strip(index, 1, cx))
                .child(marker)
                .child(code)
                .into_any_element()
        }
    }

    fn connector(
        &self,
        file: usize,
        old: Option<usize>,
        new: Option<usize>,
        block: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.look().theme;
        let geometry = block.and_then(|b| {
            self.documents
                .get(&self.files()[file].path)
                .map(|d| d.blocks[b].clone())
        });
        // GLD-026: the block's hunk action, at its first row.
        let control = block
            .zip(geometry.as_ref())
            .filter(|(_, b)| first_row(b, old, new))
            .and_then(|(b, _)| {
                self.hunk_control("diff-hunk", file, b, theme::space::MENU_ICON, cx)
            });
        div()
            .flex_none()
            .w(theme::space::DIFF_CONNECTOR)
            .h_full()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            // Transparent ribbons share the viewport gutter's background.
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        if let Some(b) = &geometry {
                            let row = old
                                .map(|n| n - b.old.start)
                                .or_else(|| new.map(|n| n - b.new.start))
                                .unwrap_or(0);
                            let y = bounds.top() - bounds.size.height * row as f32;
                            let shape = super::center::ribbon(
                                b.old.len(),
                                b.new.len(),
                                bounds.size.height,
                                bounds.size.width,
                            );
                            let mut path = gpui::PathBuilder::fill();
                            path.move_to(point(bounds.left(), y));
                            path.line_to(point(bounds.right(), y));
                            path.line_to(point(bounds.right(), y + shape.new_end));
                            path.cubic_bezier_to(
                                point(bounds.left(), y + shape.old_end),
                                point(bounds.left() + shape.control_new.x, y + shape.control_new.y),
                                point(bounds.left() + shape.control_old.x, y + shape.control_old.y),
                            );
                            path.close();
                            let colors = theme::diff_center::of(t);
                            let color = if b.old.is_empty() {
                                colors.added
                            } else if b.new.is_empty() {
                                colors.removed
                            } else {
                                colors.modified
                            };
                            if let Ok(path) = path.build() {
                                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                                    window.paint_path(path, rgb(color))
                                });
                            }
                        }
                    },
                )
                .size_full(),
            )
            .relative()
            .children(control.map(|control| {
                div()
                    .absolute()
                    .inset_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(control)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    this.viewer.resizing = true;
                    this.viewer.selecting = false;
                    window.focus(&this.focus_handle(cx), cx);
                    this.resize_at(event.position, cx);
                    cx.stop_propagation();
                }),
            )
            .into_any_element()
    }

    pub(super) fn surface_gestures(&self, cx: &mut Context<Self>) -> AnyElement {
        let owner = cx.entity().downgrade();
        canvas(
            |bounds, _, _| bounds,
            move |bounds, _, window, app| {
                let _ = owner.update(app, |this, cx| {
                    let content_bounds = Bounds::new(
                        bounds.origin,
                        size(
                            (bounds.size.width - window.rem_size() * theme::space::MARK_BAR.0 * 2.)
                                .max(px(0.)),
                            bounds.size.height,
                        ),
                    );
                    if this.viewer.viewport != content_bounds {
                        this.viewer.viewport = content_bounds;
                        let owner = cx.entity().downgrade();
                        cx.defer(move |cx| {
                            let _ = owner.update(cx, |_, cx| cx.notify());
                        });
                    }
                });
                let _ = owner.update(app, |this, _| {
                    let rem = window.rem_size();
                    let width = this.viewer.viewport.size.width;
                    let number = this.viewer.number_width;
                    if this.mode() == DiffView::SideBySide {
                        let center = bounds.left() + width * this.viewer.ratio;
                        let old = center
                            - number
                            - this.authors_width(0, false, window)
                            - Self::blame_strip_width(window);
                        let new = center
                            + theme::space::DIFF_CONNECTOR.to_pixels(rem)
                            + number
                            + this.authors_width(1, false, window)
                            + Self::blame_strip_width(window);
                        // The center is one viewport-height gutter, not a
                        // sequence of independently shaded connector cells.
                        window.paint_quad(gpui::fill(
                            Bounds::new(
                                point(old, bounds.top()),
                                size((new - old).max(px(0.)), bounds.size.height),
                            ),
                            rgb(this.look().theme.surface.app),
                        ));
                        for x in [old, new - px(1.)] {
                            window.paint_quad(gpui::fill(
                                Bounds::new(
                                    point(x, bounds.top()),
                                    size(px(1.), bounds.size.height),
                                ),
                                rgb(this.look().theme.surface.divider),
                            ));
                        }
                    }
                });
                let down = owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, app| {
                    if phase == DispatchPhase::Capture
                        && event.button == MouseButton::Left
                        && bounds.contains(&event.position)
                    {
                        let _ = down.update(app, |this, cx| {
                            if this.file_picker.is_some() || this.authors_menu.is_some() {
                                return;
                            }
                            if event.position.x
                                >= this.viewer.viewport.right()
                                    + window.rem_size() * theme::space::MARK_BAR.0
                            {
                                this.viewer.active_change = None;
                                this.viewer.file_anchor = None;
                                cx.notify();
                            }
                        });
                    }
                });
                let moved = owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, window, app| {
                    if phase != DispatchPhase::Capture {
                        return;
                    }
                    let _ = moved.update(app, |this, cx| {
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.viewer.resizing = false;
                            this.viewer.selecting = false;
                            return;
                        }
                        if !window.is_window_active() {
                            this.viewer.resizing = false;
                            this.viewer.selecting = false;
                            return;
                        }
                        if this.viewer.resizing {
                            this.resize_at(event.position, cx);
                            cx.stop_propagation();
                        } else if this.viewer.selecting {
                            this.drag_selection(event.position, cx);
                            this.schedule_selection_scroll(window, cx);
                            cx.stop_propagation();
                        }
                    });
                });
                let up = owner.clone();
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, app| {
                    if phase != DispatchPhase::Capture || event.button != MouseButton::Left {
                        return;
                    }
                    let _ = up.update(app, |this, cx| {
                        if this.viewer.resizing {
                            this.resize_at(event.position, cx);
                        }
                        if this.viewer.selecting {
                            this.drag_selection(event.position, cx);
                        }
                        this.viewer.resizing = false;
                        this.viewer.selecting = false;
                    });
                });
                let wheel = owner.clone();
                window.on_mouse_event(move |event: &gpui::ScrollWheelEvent, phase, _, app| {
                    if phase != DispatchPhase::Capture || !bounds.contains(&event.position) {
                        return;
                    }
                    let _ = wheel.update(app, |this, cx| {
                        if this.file_picker.is_some() || this.authors_menu.is_some() {
                            return;
                        }
                        let delta = event.delta.pixel_delta(px(this.row_px));
                        let dx = if event.modifiers.shift && delta.x == px(0.) {
                            delta.y
                        } else {
                            delta.x
                        };
                        if delta.y != px(0.) && !event.modifiers.shift {
                            this.viewer.active_change = None;
                            this.viewer.file_anchor = None;
                            cx.notify();
                        }
                        if dx != px(0.) {
                            let index = if this.mode() == DiffView::Unified {
                                2
                            } else {
                                usize::from(
                                    event.position.x
                                        > this.viewer.viewport.left()
                                            + this.viewer.viewport.size.width * this.viewer.ratio,
                                )
                            };
                            let handle = &this.viewer.horizontal[index];
                            let mut offset = handle.offset();
                            offset.x += dx;
                            handle.set_offset(offset);
                            if !event.modifiers.shift && delta.y != px(0.) {
                                let base = &this.scroll.0.borrow().base_handle;
                                let mut offset = base.offset();
                                offset.y =
                                    (offset.y + delta.y).min(px(0.)).max(-base.max_offset().y);
                                base.set_offset(offset);
                            }
                            cx.stop_propagation();
                            cx.notify();
                        }
                    });
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .into_any_element()
    }

    fn selection_scroll_pointer(&self) -> Option<gpui::Point<gpui::Pixels>> {
        if !self.viewer.selecting {
            return None;
        }
        let pointer = self.viewer.pointer?;
        let side = self.viewer.selection.as_ref().map_or(1, |s| s.side);
        let h = if self.mode() == DiffView::Unified {
            2
        } else {
            side
        };
        let bounds = self.viewer.horizontal[h].0.borrow().bounds;
        (!self.viewer.viewport.contains(&pointer)
            || pointer.x < bounds.left()
            || pointer.x > bounds.right())
        .then_some(pointer)
    }

    fn schedule_selection_scroll(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.viewer.edge_pending || self.selection_scroll_pointer().is_none() {
            return;
        }
        self.viewer.edge_pending = true;
        let owner = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let _ = owner.update(cx, |this, cx| {
                this.viewer.edge_pending = false;
                if !window.is_window_active() {
                    this.viewer.selecting = false;
                    return;
                }
                // Re-read and requalify the latest position. A pointer that
                // returned inside before this frame must not replay its edge.
                let Some(pointer) = this.selection_scroll_pointer() else {
                    return;
                };
                if !crate::system::primary_button_down() {
                    this.viewer.selecting = false;
                    return;
                }
                let before = this.scroll.0.borrow().base_handle.offset();
                let horizontal = this.viewer.horizontal.each_ref().map(|h| h.offset());
                this.drag_selection(pointer, cx);
                if before != this.scroll.0.borrow().base_handle.offset()
                    || horizontal != this.viewer.horizontal.each_ref().map(|h| h.offset())
                {
                    this.schedule_selection_scroll(window, cx);
                }
            });
        });
    }

    pub(super) fn reveal_source(&self, cx: &mut Context<Self>) {
        let file = self.current_file();
        let Some(f) = self.files().get(file) else {
            return;
        };
        let selected = self
            .viewer
            .selection
            .as_ref()
            .filter(|sel| sel.path == f.path)
            .and_then(|sel| {
                let doc = self.documents.get(&f.path)?;
                let row = doc.sources[sel.side]
                    .lines
                    .partition_point(|r| r.start <= sel.head)
                    .saturating_sub(1);
                Some(if sel.side == 0 {
                    doc.new_line_for_old(row) + 1
                } else {
                    row + 1
                })
            });
        let line = selected.unwrap_or_else(|| {
            self.viewer
                .active_change
                .filter(|(i, _)| *i == file)
                .and_then(|(_, b)| self.documents.get(&f.path).and_then(|d| d.blocks.get(b)))
                .map_or(self.first_change_line(), |b| b.new.start + 1)
        });
        cx.emit(DiffEvent::OpenAt {
            path: f.path.clone(),
            line: line.max(1),
        });
    }

    pub(super) fn surface_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        if self.authors_menu.is_some() {
            self.authors_menu_key(event, window, cx);
            return;
        }
        if self.file_picker.is_some() {
            self.file_picker_key(event, window, cx);
            return;
        }
        if key == "enter" {
            // Controls activate on KeyUp. Stop only bubbling: keep that default
            // activation while isolating the enclosing Push window's shortcut.
            cx.stop_propagation();
            return;
        }
        if key == "escape" {
            if self.viewer.resizing || self.viewer.selecting {
                self.viewer.resizing = false;
                self.viewer.selecting = false;
                cx.stop_propagation();
                cx.notify();
            }
            return;
        }
        if mods.platform && key == "c" {
            self.copy_text(cx);
            cx.stop_propagation();
            return;
        }
        if mods.platform && key == "a" {
            self.select_all_text(cx);
            cx.stop_propagation();
            return;
        }
        if !matches!(key, "left" | "right" | "up" | "down" | "home" | "end") {
            return;
        }
        if !self.focus_handle(cx).is_focused(window) {
            return;
        }
        let Some(sel) = self.viewer.selection.as_ref() else {
            return;
        };
        let Some(doc) = self.documents.get(&sel.path) else {
            return;
        };
        let source = &doc.sources[sel.side];
        let boundaries = crate::system::grapheme_boundaries(&source.text);
        let row = source
            .lines
            .partition_point(|r| r.start <= sel.head)
            .saturating_sub(1);
        let line = source.lines.get(row).cloned().unwrap_or(0..0);
        let next = match key {
            "left" if mods.platform => line.start,
            "right" if mods.platform => line.end,
            "left" => boundaries
                .iter()
                .copied()
                .take_while(|b| *b < sel.head)
                .last()
                .unwrap_or(0),
            "right" => boundaries
                .iter()
                .copied()
                .find(|b| *b > sel.head)
                .unwrap_or(source.text.len()),
            "home" => {
                if mods.platform {
                    0
                } else {
                    line.start
                }
            }
            "end" => {
                if mods.platform {
                    source.text.len()
                } else {
                    line.end
                }
            }
            "up" | "down" => {
                let target = if key == "up" {
                    row.saturating_sub(1)
                } else {
                    (row + 1).min(source.lines.len().saturating_sub(1))
                };
                let desired = source
                    .lines
                    .get(target)
                    .map_or(sel.head, |r| r.start + (sel.head - line.start).min(r.len()));
                boundaries
                    .iter()
                    .copied()
                    .take_while(|b| *b <= desired)
                    .last()
                    .unwrap_or(0)
            }
            _ => sel.head,
        };
        if let Some(sel) = self.viewer.selection.as_mut() {
            sel.head = next;
            if !mods.shift {
                sel.anchor = next;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}

fn shape(
    text: &str,
    spans: &[crate::syntax::Span],
    look: &crate::file_tab::Look,
    window: &mut Window,
) -> gpui::ShapedLine {
    let runs = syntax_segments(text.len(), spans.iter())
        .into_iter()
        .map(|span| {
            let paint = span.paint;
            let (color, italic, bold) = paint.map(|p| look.theme.of_paint(p)).unwrap_or((
                look.theme.text.primary,
                false,
                false,
            ));
            TextRun {
                len: span.at.len(),
                font: Font {
                    family: look.family.clone(),
                    weight: if bold {
                        gpui::FontWeight::BOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    },
                    style: if italic {
                        gpui::FontStyle::Italic
                    } else {
                        gpui::FontStyle::Normal
                    },
                    ..Default::default()
                },
                color: rgb(color).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }
        })
        .collect::<Vec<_>>();
    window.text_system().shape_line(
        SharedString::from(text.to_owned()),
        px(look.font_size),
        &runs,
        None,
    )
}

/// Retain the full identity in the tooltip while giving the path room in chrome.
fn compact_revision(identity: &str) -> String {
    let (revision, path) = identity
        .split_once(':')
        .map_or((identity, None), |(r, p)| (r, Some(p)));
    let hex = revision.trim_end_matches('^');
    let revision = if hex.len() > 7 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        format!(
            "{}{}",
            &hex[..7],
            if revision.ends_with('^') { "^" } else { "" }
        )
    } else {
        revision.to_owned()
    };
    path.map_or_else(|| revision.clone(), |path| format!("{revision}:{path}"))
}

/// Whether a display row is a block's first: its old line is the block's
/// first, or, for a pure insertion, its new line is.
fn first_row(block: &crate::diff::document::Block, old: Option<usize>, new: Option<usize>) -> bool {
    if block.old.is_empty() {
        new == Some(block.new.start)
    } else {
        old == Some(block.old.start)
    }
}

impl DiffTab {
    /// A block's Stage or Unstage control — GLD-026: `head-plus` to stage,
    /// `head-hide` to unstage, faint and saying why when it is disabled.
    fn hunk_control(
        &self,
        id: &'static str,
        file: usize,
        block: usize,
        icon: gpui::Rems,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let t = self.look().theme;
        let ui = self.ui();
        let (action, available) = self.hunk_offer(file, block)?;
        let token = self.hunk_token(file, block);
        let (path, label) = match action {
            crate::hunk::Action::Stage => ("ui/head-plus.svg", crate::i18n::Text::StageHunk),
            crate::hunk::Action::Unstage => ("ui/head-hide.svg", crate::i18n::Text::UnstageHunk),
        };
        let enabled = available.is_ok();
        let hint = ui.text(available.err().unwrap_or(label)).to_owned();
        Some(
            div()
                .id((id, file * 100_000 + block))
                .debug_selector(move || format!("{id}-{file}-{block}"))
                .flex()
                .items_center()
                .justify_center()
                .rounded(theme::radius::STATUS_BUTTON)
                .child(
                    gpui::svg()
                        .path(path)
                        .size(icon)
                        .text_color(rgb(if enabled {
                            t.text.secondary
                        } else {
                            t.text.faint
                        })),
                )
                .hint(hint, t)
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .when(enabled, |this| {
                    this.cursor_pointer()
                        .hover(|s| s.bg(rgb(t.surface.raised)))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            if let Some(token) = &token {
                                this.stage_or_unstage(token, cx);
                            }
                        }))
                })
                .into_any_element(),
        )
    }

    /// The toolbar's button for the current change — GLD-026. Absent where
    /// this diff stages nothing.
    fn render_stage_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        crate::diff_tab::hunk_view_of(self.base()).filter(|_| self.inspection_key().is_none())?;
        let t = self.look().theme;
        let ui = self.ui();
        let current = self.current_hunk();
        let offer = current.and_then(|(file, block)| self.hunk_offer(file, block));
        let (path, label) = match offer.as_ref().map(|(a, _)| *a) {
            Some(crate::hunk::Action::Unstage) => {
                ("ui/head-hide.svg", crate::i18n::Text::UnstageHunk)
            }
            _ => ("ui/head-plus.svg", crate::i18n::Text::StageHunk),
        };
        let reason = match &offer {
            Some((_, Err(why))) => Some(*why),
            _ => None,
        };
        let enabled = matches!(offer, Some((_, Ok(()))));
        let token = current.and_then(|(file, block)| self.hunk_token(file, block));
        let hint = ui.text(reason.unwrap_or(label)).to_owned();
        Some(
            diff_icon_button("diff-stage-change", &hint, path, t, false, cx)
                .disabled(!enabled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(token) = &token {
                        this.stage_or_unstage(token, cx);
                    }
                }))
                .into_any_element(),
        )
    }
}

fn diff_icon_button(
    id: &'static str,
    label: &str,
    icon: &'static str,
    theme: theme::Theme,
    active: bool,
    cx: &mut Context<DiffTab>,
) -> Button {
    Button::new(id)
        .debug_selector(move || id.into())
        .icon(gpui_component::Icon::default().path(icon))
        .accessibility_label(label.to_owned())
        .hint(label.to_owned(), theme)
        .small()
        .w(theme::space::HEAD_BUTTON)
        .flex_none()
        .custom(super::super::controls::ghost(theme, active, cx))
}

/// Project ordered syntax spans to styled runs, retaining plain gaps.
fn syntax_segments<'a>(
    len: usize,
    spans: impl Iterator<Item = &'a crate::syntax::Span>,
) -> Vec<crate::syntax::Span> {
    let mut out = Vec::new();
    let mut at = 0;
    // syntax::spans guarantees ordered disjoint ranges. Walk once, including
    // plain gaps, instead of restarting a covering-span search for each run.
    for span in spans {
        let start = span.at.start.max(at).min(len);
        let end = span.at.end.min(len);
        if start > at {
            out.push(crate::syntax::Span {
                at: at..start,
                paint: None,
            });
            at = start;
        }
        if end > start {
            out.push(crate::syntax::Span {
                at: start..end,
                paint: span.paint,
            });
            at = end;
        }
        if at == len {
            break;
        }
    }
    if at < len {
        out.push(crate::syntax::Span {
            at: at..len,
            paint: None,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    #[test]
    fn many_syntax_spans_are_consumed_in_linear_work_with_plain_gaps() {
        for count in [256, 1024, 4096, 32768] {
            let spans = (0..count)
                .map(|i| crate::syntax::Span {
                    at: i * 2..i * 2 + 1,
                    paint: Some(crate::highlight::Paint::Strong),
                })
                .collect::<Vec<_>>();
            let visits = Cell::new(0usize);
            let runs = syntax_segments(
                count * 2,
                spans.iter().inspect(|_| visits.set(visits.get() + 1)),
            );
            assert_eq!(runs.len(), count * 2);
            assert_eq!(runs[0].at, 0..1);
            assert_eq!(runs[1].at, 1..2);
            assert!(runs[1].paint.is_none());
            assert_eq!(runs.last().unwrap().at.end, count * 2);
            eprintln!(
                "{count} source spans: {} visits, {} runs",
                visits.get(),
                runs.len()
            );
            assert!(
                visits.get() <= count * 4 + 4,
                "{count} spans consumed {} times",
                visits.get()
            );
        }
    }
}
