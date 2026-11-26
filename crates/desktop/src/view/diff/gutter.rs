//! Fixed per-source gutters and the opt-in authors context menu.
use super::*;
use crate::diff_tab::{AuthorData, AuthorWidth};
use crate::i18n::Text;
use gpui::{MouseButton, Pixels, Point, anchored, deferred, point};
use gpui_kit::TestSupportExt;

impl DiffTab {
    /// One width for the full projection, independent of visible/folded rows.
    pub(super) fn prepare_number_width(&mut self, window: &Window) {
        let digits = self
            .documents
            .values()
            .flat_map(|doc| doc.sources.iter())
            .map(|source| source.lines.len())
            .max()
            .unwrap_or(1)
            .max(1)
            .to_string()
            .len();
        let font = gpui::Font {
            family: self.look().family.clone(),
            ..window.text_style().font()
        };
        let size = px(self.look().font_size);
        let padding = theme::space::CARD_GAP.to_pixels(window.rem_size());
        let key = format!("{font:?}:{size:?}:{padding:?}:{digits}");
        if self.viewer.number_key != key {
            // Respect non-tabular code fonts too: any source digit must fit.
            let digit = (0..=9)
                .map(|digit| {
                    crate::file_tab::gutter::text_width(
                        &digit.to_string(),
                        size,
                        font.clone(),
                        window,
                    )
                })
                .fold(px(0.), Pixels::max);
            self.viewer.number_width = px(f32::from(digit * digits as f32 + padding).ceil());
            self.viewer.number_key = key;
        }
    }
    pub(super) fn blame_strip_width(window: &Window) -> Pixels {
        theme::space::TREE_GAP.to_pixels(window.rem_size()) * 2.
    }
    pub(super) fn blame_click_strip(
        &self,
        index: usize,
        side: usize,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = self.look().theme;
        let label = format!(
            "{} · {}",
            self.ui().text(Text::MenuAnnotateWithBlame),
            self.viewer_label(
                if side == 0 { "Before" } else { "After" },
                if side == 0 { "До" } else { "После" }
            )
        );
        div()
            .id((
                if side == 0 {
                    "diff-blame-strip-old"
                } else {
                    "diff-blame-strip-new"
                },
                index,
            ))
            .debug_selector(move || {
                format!(
                    "diff-blame-strip-{}-{index}",
                    if side == 0 { "old" } else { "new" }
                )
            })
            .w(theme::space::TREE_GAP * 2.)
            .h_full()
            .flex_none()
            .bg(rgb(self.center_row_paint(index, side).fill))
            .when(side == 0, |strip| strip.border_l_1())
            .when(side == 1, |strip| strip.border_r_1())
            .border_color(rgb(t.surface.divider))
            .cursor_pointer()
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .hint(label, t)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(
                cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_authors_menu(side, event.position(), window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_authors_menu(side, event.position, window, cx);
                }),
            )
            .test_support()
            .into_any_element()
    }
    /// Match FileTab's measured UI metadata, cached per source and font. A
    /// multi-file projection keeps one width per side, so scrolling rows cannot
    /// move its text clips or the common horizontal track.
    pub(super) fn prepare_authors(&mut self, window: &Window) {
        let mut font = window.text_style().font();
        font.weight = gpui::FontWeight::NORMAL;
        let size = theme::size::META.to_pixels(window.rem_size());
        let padding = theme::space::CARD_GAP.to_pixels(window.rem_size());
        let cap = theme::space::ANNOTATE_COLUMN.to_pixels(window.rem_size());
        let ui = self.ui();
        let measure =
            |label: &str| crate::file_tab::gutter::text_width(label, size, font.clone(), window);
        let pending = (measure(ui.text(Text::LogLoading)) + padding * 2.).min(cap);
        self.authors.font = Some(font.clone());
        self.authors.widths = [pending; 2];
        let dirty = self
            .authors
            .entries
            .iter()
            .filter(|(_, entry)| entry.width.is_none())
            .map(|((path, _), _)| path.clone())
            .collect::<std::collections::HashSet<_>>();
        for path in &dirty {
            let times = self
                .authors
                .entries
                .iter()
                .filter(|((file, _), _)| file == path)
                .flat_map(|(_, entry)| match &entry.data {
                    AuthorData::Ready(lines) => lines.as_slice(),
                    _ => &[],
                })
                .flatten()
                .map(|line| line.time);
            self.authors
                .ranks
                .insert(path.clone(), crate::annotate_tab::revision_shades(times));
        }
        let mut measured = [false; 2];
        for ((path, side), entry) in &mut self.authors.entries {
            if !self.authors.enabled[*side] {
                continue;
            }
            let valid = entry.width.as_ref().is_some_and(|cached| {
                cached.font == font
                    && cached.size == size
                    && cached.padding == padding
                    && cached.language == ui.0
            });
            if !valid {
                let labels = match &entry.data {
                    AuthorData::Ready(lines) => lines
                        .iter()
                        .map(|line| {
                            line.as_ref()
                                .map(crate::git::blame::Line::compact)
                                .unwrap_or_else(|| ui.text(Text::AnnotateNotCommitted).to_owned())
                        })
                        .collect::<std::collections::HashSet<_>>(),
                    AuthorData::Loading => [ui.text(Text::LogLoading).to_owned()].into(),
                    AuthorData::Unavailable(_) => {
                        [ui.text(Text::BlameUnavailable).to_owned()].into()
                    }
                };
                let width = (labels
                    .iter()
                    .map(|label| measure(label))
                    .fold(px(0.), Pixels::max)
                    + padding * 2.)
                    .min(cap);
                entry.width = Some(AuthorWidth {
                    font: font.clone(),
                    size,
                    padding,
                    language: ui.0,
                    width,
                    shades: self.authors.ranks.get(path).cloned().unwrap_or_default(),
                });
            }
            if dirty.contains(path) {
                entry.width.as_mut().unwrap().shades =
                    self.authors.ranks.get(path).cloned().unwrap_or_default();
            }
            let width = entry.width.as_ref().unwrap().width;
            self.authors.widths[*side] = if measured[*side] {
                self.authors.widths[*side].max(width)
            } else {
                width
            };
            measured[*side] = true;
        }
    }
    pub(super) fn authors_width(&self, side: usize, unified: bool, window: &Window) -> Pixels {
        if !self.authors.enabled[side] {
            return px(0.);
        }
        let rem = window.rem_size();
        let width = self.viewer.viewport.size.width;
        let base = self.viewer.number_width * if unified { 2. } else { 1. }
            + rem * theme::space::TREE_GAP.0 * 2.
            + if unified {
                px(0.)
            } else {
                Self::blame_strip_width(window)
            };
        let available = if unified {
            width
        } else if side == 0 {
            width * self.viewer.ratio
        } else {
            width * (1. - self.viewer.ratio) - rem * theme::space::DIFF_CONNECTOR.0
        };
        let desired = self.authors.widths[side];
        let room = (available - base - px(self.look().font_size * 4.)).max(px(0.));
        if unified {
            let total = (0..2)
                .filter(|side| self.authors.enabled[*side])
                .map(|side| self.authors.widths[side])
                .sum::<Pixels>();
            desired * (f32::from(room) / f32::from(total).max(1.)).min(1.)
        } else {
            desired.min(room)
        }
    }
    pub(super) fn numbers_cell(
        &self,
        index: usize,
        file: usize,
        side: usize,
        line: Option<usize>,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = self.look().theme;
        let path = self.files()[file].path.clone();
        let source_line = line
            .map(|line| {
                if side == 0 {
                    self.documents
                        .get(&path)
                        .map_or(line, |doc| doc.new_line_for_old(line))
                } else {
                    line
                }
            })
            .unwrap_or(0)
            + 1;
        div()
            .id((
                if side == 0 {
                    "diff-numbers-old"
                } else {
                    "diff-numbers-new"
                },
                index,
            ))
            .debug_selector(move || {
                format!(
                    "diff-numbers-{}-{index}",
                    if side == 0 { "old" } else { "new" }
                )
            })
            .w(self.viewer.number_width)
            .h_full()
            .flex_none()
            .overflow_hidden()
            .text_right()
            .when(
                side == 1 && self.mode() == guild_session::DiffView::SideBySide,
                |cell| cell.text_left(),
            )
            .px(theme::space::CARD_GAP / 2.)
            .bg(rgb(self.center_row_paint(index, side).fill))
            .when(
                side == 0 && self.mode() == guild_session::DiffView::Unified,
                |cell| cell.border_l_1(),
            )
            .when(
                side == 1 && self.mode() == guild_session::DiffView::Unified,
                |cell| cell.border_r_1(),
            )
            .border_color(rgb(t.surface.divider))
            .text_color(rgb(theme::diff_center::of(t).number))
            .child(line.map(|line| (line + 1).to_string()).unwrap_or_default())
            .hint(
                self.viewer_label(
                    "Right-click: Annotate with Git Blame",
                    "Правая кнопка: Аннотировать с помощью Git Blame",
                ),
                t,
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_authors_menu(side, event.position, window, cx);
                }),
            )
            .when(line.is_some(), |cell| {
                cell.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |_, event: &gpui::MouseDownEvent, _, cx| {
                        if event.click_count == 2 {
                            cx.emit(DiffEvent::OpenAt {
                                path: path.clone(),
                                line: source_line,
                            });
                            cx.stop_propagation();
                        }
                    }),
                )
            })
            .test_support()
            .into_any_element()
    }
    pub(super) fn authors_cell(
        &self,
        index: usize,
        file: usize,
        side: usize,
        line: Option<usize>,
        width: Pixels,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let t = self.look().theme;
        let mut label = String::new();
        let mut tip = self
            .viewer_label(
                "Right-click: Annotate with Git Blame",
                "Правая кнопка: Аннотировать с помощью Git Blame",
            )
            .to_owned();
        let colors = theme::diff_center::of(t);
        let mut background = colors.blame[2];
        let ink = colors.metadata;
        let entry = self
            .authors
            .entries
            .get(&(self.files()[file].path.clone(), side));
        if let Some(line) = line {
            match entry.map(|entry| &entry.data) {
                Some(AuthorData::Ready(lines)) => {
                    if let Some(Some(author)) = lines.get(line) {
                        label = author.compact();
                        tip = author.tooltip();
                        background = match entry
                            .and_then(|entry| entry.width.as_ref())
                            .and_then(|width| width.shades.get(&author.time))
                        {
                            Some(0) => colors.blame[0],
                            Some(1) => colors.blame[1],
                            _ => colors.blame[2],
                        };
                    } else {
                        label = self.ui().text(Text::AnnotateNotCommitted).to_owned();
                    }
                }
                Some(AuthorData::Unavailable(error)) => {
                    label = self.ui().text(Text::BlameUnavailable).to_owned();
                    tip = error.clone();
                }
                _ => label = self.ui().text(Text::LogLoading).to_owned(),
            }
        }
        div()
            .id((
                if side == 0 {
                    "diff-authors-old"
                } else {
                    "diff-authors-new"
                },
                index,
            ))
            .debug_selector(move || format!("diff-authors-{side}-{index}"))
            .w(width)
            .h_full()
            .flex_none()
            .overflow_hidden()
            .min_w_0()
            .flex()
            .items_center()
            .bg(rgb(background))
            .when_some(self.authors.font.as_ref(), |cell, font| {
                cell.font_family(font.family.clone())
            })
            .text_size(theme::size::META)
            .font_weight(gpui::FontWeight::NORMAL)
            .text_color(rgb(ink))
            .child(
                div()
                    .min_w_0()
                    .mx(theme::space::CARD_GAP)
                    .truncate()
                    .child(label),
            )
            .hint_delay()
            .tooltip(move |_, cx| {
                cx.new(|_| crate::view::tooltip::Tooltip::details(tip.clone(), t))
                    .into()
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_authors_menu(side, event.position, window, cx);
                }),
            )
            .test_support()
            .into_any_element()
    }
    fn open_authors_menu(
        &mut self,
        side: usize,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_picker = None;
        self.viewer.selecting = false;
        self.viewer.resizing = false;
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.authors_menu = Some((side, at, focus));
        cx.notify();
    }
    fn close_authors_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.authors_menu = None;
        self.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    pub(super) fn authors_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((side, _, _)) = &self.authors_menu else {
            return;
        };
        let side = *side;
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" => self.close_authors_menu(window, cx),
            "enter" | "space" => {
                self.toggle_authors(side, cx);
                self.close_authors_menu(window, cx);
            }
            _ => {}
        }
    }
    pub(super) fn render_authors_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (side, at, focus) = self.authors_menu.as_ref()?;
        let side = *side;
        let t = self.look().theme;
        let label = self.ui().text(Text::MenuAnnotateWithBlame);
        let row = crate::view::popup::row(
            "diff-authors-toggle",
            crate::view::popup::Row {
                label: label.into(),
                mark: None,
                icons: false,
                check: Some(self.authors.enabled[side]),
                stroke: None,
                loses: false,
                submenu: false,
                lit: false,
                disabled: false,
                trailing: None,
            },
            t,
        )
        .debug_selector(|| "diff-authors-toggle".into())
        .on_click(cx.listener(move |this, _, window, cx| {
            cx.stop_propagation();
            this.toggle_authors(side, cx);
            this.close_authors_menu(window, cx);
        }));
        let card = crate::view::popup::card(t)
            .id("diff-authors-menu")
            .debug_selector(|| "diff-authors-menu".into())
            .track_focus(focus)
            .key_context("diff-authors-menu")
            .child(row)
            .on_key_down(
                cx.listener(|this, event, window, cx| this.authors_menu_key(event, window, cx)),
            );
        Some(
            div()
                .children([
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .w(window.viewport_size().width)
                                .h(window.viewport_size().height)
                                .occlude()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_authors_menu(window, cx);
                                    }),
                                ),
                        ),
                    )
                    .into_any_element(),
                    deferred(
                        anchored()
                            .position(*at)
                            .snap_to_window_with_margin(px(8.))
                            .child(card),
                    )
                    .into_any_element(),
                ])
                .into_any_element(),
        )
    }
}
