//! The diff tab's body: the rows of `DiffTab`, on `uniform_list` like the
//! tree, in the terminal's face — section 6.10 of the design system.

mod center;
mod files;
mod gutter;
mod surface;

use crate::view::tooltip::Hint as _;

use gpui::prelude::*;
use gpui::{ClickEvent, Context, Focusable, SharedString, Window, div, px, rgb, uniform_list};

use crate::diff::{Pair, Row};
use crate::diff_tab::{DiffEvent, DiffTab, FileDiff, HunkKind, Line, Painted, State, totals};
use crate::i18n::{Text, Ui};
use crate::{icons, theme};
use gpui_kit::TestSupportExt;

impl Render for DiffTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.viewer.focus_requested {
            self.focus_handle(cx).focus(window, cx);
            self.viewer.focus_requested = false;
        }
        self.row_px = f32::from(window.rem_size()) * theme::space::TREE_ROW.0;
        let look = self.look().clone();
        let theme = look.theme;
        let ui = self.ui();
        let count = self.lines().len();
        let focus = self.focus_handle(cx);
        // The processor receives this already leased DiffTab. Borrow its retained
        // models there; only a visible row's lightweight metadata is cloned.
        let mode = self.mode();
        self.prepare_surface(window, cx);
        let toolbar = self.render_inspection_toolbar(cx);
        let picker = self.render_file_picker(window, cx);
        let revisions = self.render_revisions(cx);
        let horizontal = self.render_horizontal_bars(window);
        let authors_menu = self.render_authors_menu(window, cx);
        let gestures = self.surface_gestures(cx);
        let bar = self.render_marks(self.lines(), window, cx);

        div()
            .id("diff")
            .track_focus(&focus)
            .key_context(
                if self.viewer.resizing
                    || self.viewer.selecting
                    || self.file_picker.is_some()
                    || self.authors_menu.is_some()
                {
                    "diff-gesture"
                } else {
                    "diff-window"
                },
            )
            .on_action(cx.listener(|this, _: &crate::actions::NextChange, _, cx| {
                this.step_change(true, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::actions::StageHunk, _, cx| {
                this.hunk_action_on_current(crate::hunk::Action::Stage, cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::actions::UnstageHunk, _, cx| {
                this.hunk_action_on_current(crate::hunk::Action::Unstage, cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|this, _: &crate::actions::PreviousChange, _, cx| {
                    this.step_change(false, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::NextDiffFile, _, cx| {
                    this.step_file(true, cx);
                    cx.stop_propagation();
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::PreviousDiffFile, _, cx| {
                    this.step_file(false, cx);
                    cx.stop_propagation();
                }),
            )
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface.app))
            .on_action(cx.listener(|this, _: &crate::actions::Copy, _, cx| {
                this.copy_text(cx);
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::actions::SelectAll, _, cx| {
                this.select_all_text(cx);
                cx.stop_propagation();
            }))
            .on_key_down(cx.listener(|this, event, window, cx| this.surface_key(event, window, cx)))
            .child(toolbar)
            .child(revisions)
            .child(
                div()
                    .id("diff-body")
                    .debug_selector(|| "diff-body".into())
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .child(gestures)
                    .child(
                        uniform_list(
                            "diff-rows",
                            count,
                            cx.processor(
                                move |this: &mut DiffTab,
                                      range: std::ops::Range<usize>,
                                      window,
                                      cx| {
                                    range
                                        .map(|index| {
                                            let line = this.lines()[index].clone();
                                            let files = this.files();
                                            match &line {
                                                Line::Source { file, row } => this
                                                    .render_source_row(
                                                        index, *file, row, window, cx,
                                                    ),
                                                Line::Summary => {
                                                    render_summary(index, files, &look, ui)
                                                }
                                                Line::Listed { file } => render_listed(
                                                    index,
                                                    &files[*file],
                                                    *file,
                                                    &look,
                                                    cx,
                                                ),
                                                Line::File { file }
                                                | Line::Hunk { file, .. }
                                                | Line::Note { file } => render_line(
                                                    index,
                                                    &line,
                                                    &files[*file],
                                                    this.paint(&files[*file].path).as_deref(),
                                                    &look,
                                                    ui,
                                                    mode,
                                                    cx,
                                                ),
                                                Line::Pair { file, pair } => render_pair(
                                                    index,
                                                    pair,
                                                    &files[*file],
                                                    this.paint(&files[*file].path).as_deref(),
                                                    &look,
                                                    ui,
                                                    mode,
                                                    cx,
                                                ),
                                            }
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .track_scroll(&self.scroll)
                        .size_full(),
                    )
                    .child(bar)
                    .test_support(),
            )
            .child(horizontal)
            .children(picker)
            .children(authors_menu)
    }
}

impl DiffTab {
    /// *Apply* and *Refuse*, on a proposal tab only. M8.6.
    ///
    /// **The agent is blocked until one of them is pressed** — that is
    /// what `openDiff` does — so the pair says so, and neither is drawn
    /// anywhere else: on an ordinary diff there is nobody waiting.
    fn render_answers(&self, ui: Ui, cx: &mut Context<DiffTab>) -> Vec<gpui::AnyElement> {
        if !matches!(self.base(), guild_session::Base::Proposal) {
            return Vec::new();
        }
        let theme = self.look().theme;
        let answer = |id: &'static str, label: Text, hint: Text, accepting: bool| {
            let (ink, ground) = if accepting {
                (theme.accent.brass, theme.accent.brass_bg)
            } else {
                (theme.text.secondary, theme.surface.raised)
            };
            div()
                .id(id)
                .flex()
                .flex_none()
                .items_center()
                .h(theme::space::HEAD_BUTTON)
                .px(theme::space::TREE_GAP)
                .rounded(theme::radius::ROW)
                .bg(rgb(ground))
                .cursor_pointer()
                .text_size(theme::size::META)
                .text_color(rgb(ink))
                .child(ui.text(label))
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::text(ui.text(hint), theme))
                        .into()
                })
                .on_click(cx.listener(move |_this, _event, _window, cx| {
                    cx.stop_propagation();
                    cx.emit(if accepting {
                        DiffEvent::Accept
                    } else {
                        DiffEvent::Reject
                    });
                }))
                .into_any_element()
        };
        vec![
            answer(
                "proposal-reject",
                Text::RejectProposal,
                Text::RejectProposalHint,
                false,
            ),
            answer(
                "proposal-accept",
                Text::AcceptProposal,
                Text::AcceptProposalHint,
                true,
            ),
        ]
    }

    /// Source-backed block ranges beside, never over, the scrollbar lane.
    fn render_marks(
        &self,
        lines: &[Line],
        window: &Window,
        cx: &mut Context<DiffTab>,
    ) -> gpui::AnyElement {
        let theme = self.look().theme;
        let ranges = self.overview_ranges();
        let height = if self.viewer.viewport.size.height > px(0.) {
            self.viewer.viewport.size.height
        } else {
            window.viewport_size().height
        };
        let minimum =
            f32::from(theme::space::MARK.to_pixels(window.rem_size())) / f32::from(height).max(1.);
        let lane_width = theme::space::MARK_BAR.to_pixels(window.rem_size());
        let marks = ranges.into_iter().enumerate().map(|(index, range)| {
            let (top, length) = crate::diff_tab::overview_span(&range.rows, lines.len(), minimum);
            let colour = match range.kind {
                HunkKind::Added => theme.accent.green,
                HunkKind::Removed => theme.accent.rust,
                HunkKind::Changed => theme.accent.brass,
            };
            div()
                .id(("diff-mark", index))
                .debug_selector(move || format!("diff-overview-mark-{index}"))
                .absolute()
                .left_0()
                .top(gpui::relative(top))
                .h(gpui::relative(length))
                .w_full()
                .bg(rgb(colour))
                .cursor_pointer()
                .hint(
                    format!("{} {}", self.viewer_label("Change", "Изменение"), index + 1),
                    theme,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.scroll_to_row(range.rows.start, cx);
                }))
        });
        div()
            .id("diff-marks")
            .flex()
            .flex_none()
            .w(theme::space::MARK_BAR * 2.)
            .h_full()
            .bg(rgb(theme.surface.panel))
            .child(
                div()
                    .id("diff-overview-stripe")
                    .occlude()
                    .debug_selector(|| "diff-overview-stripe".into())
                    .relative()
                    .overflow_hidden()
                    .w(theme::space::MARK_BAR)
                    .h_full()
                    .flex_none()
                    .children(marks),
            )
            .child(
                div()
                    .id("diff-scrollbar-lane")
                    .debug_selector(|| "diff-scrollbar-lane".into())
                    .relative()
                    .w(theme::space::MARK_BAR)
                    .h_full()
                    .flex_none()
                    .child(
                        gpui_component::scroll::Scrollbar::vertical(&self.scroll)
                            .id("diff-vertical")
                            .viewport_from_layout()
                            .mode(gpui_component::scroll::ScrollbarMode::Always)
                            .styles(|s| {
                                s.track(|s| s.width(lane_width))
                                    .thumb(|s| s.bg(rgb(theme.text.gutter)))
                            }),
                    )
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.viewer.active_change = None;
                            this.viewer.file_anchor = None;
                            cx.notify();
                        }),
                    ),
            )
            .into_any_element()
    }
}

/// The working tree's first row: how many files, and the totals.
fn render_summary(
    index: usize,
    files: &[FileDiff],
    look: &crate::file_tab::Look,
    ui: Ui,
) -> gpui::AnyElement {
    let theme = look.theme;
    let (added, removed) = totals(files);
    div()
        .id(("diff-row", index))
        .w_full()
        .h(theme::space::TREE_ROW)
        .flex()
        .items_center()
        .gap(theme::space::TREE_GAP)
        .px(theme::space::TREE_PADDING)
        .font_family(look.family.clone())
        .text_size(px(look.font_size))
        .text_color(rgb(theme.text.secondary))
        // An empty list says there is nothing, in words, rather than
        // counting to zero.
        .child(if files.is_empty() {
            ui.text(Text::DiffNoChanges).to_owned()
        } else {
            ui.diff_files(files.len())
        })
        .child(div().flex_1())
        .when(!files.is_empty(), |this| {
            this.child(
                div()
                    .text_color(rgb(theme.accent.green))
                    .child(format!("+{added}")),
            )
            .child(
                div()
                    .text_color(rgb(theme.accent.rust))
                    .child(format!("\u{2212}{removed}")),
            )
        })
        .into_any_element()
}

/// One file in the list at the top: its icon, its path, its counts; a
/// click scrolls to its hunks, a double click opens the file.
fn render_listed(
    index: usize,
    file: &FileDiff,
    file_index: usize,
    look: &crate::file_tab::Look,
    cx: &mut Context<DiffTab>,
) -> gpui::AnyElement {
    let theme = look.theme;
    let name = file
        .path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = icons::for_file(&name, icons::Palette::from(theme.kind));
    let (added, removed) = file.counts;
    let path = file.path.clone();
    div()
        .id(("diff-row", index))
        .debug_selector(move || format!("diff-listed-{index}"))
        .w_full()
        .h(theme::space::TREE_ROW)
        .flex()
        .items_center()
        .gap(theme::space::TREE_GAP)
        .pl(theme::space::TREE_PADDING + theme::space::TREE_INDENT)
        .pr(theme::space::TREE_PADDING)
        .font_family(look.family.clone())
        .text_size(px(look.font_size))
        .cursor_pointer()
        .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.card)))
        .child(icons::icon(&stem).flex_none())
        .child(
            div()
                .flex_1()
                .overflow_hidden()
                .text_color(rgb(theme.text.primary))
                .child(file.path.display().to_string()),
        )
        .when(added > 0, |this| {
            this.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.accent.green))
                    .child(format!("+{added}")),
            )
        })
        .when(removed > 0, |this| {
            this.child(
                div()
                    .flex_none()
                    .text_color(rgb(theme.accent.rust))
                    .child(format!("\u{2212}{removed}")),
            )
        })
        .on_click(cx.listener(move |this, event: &ClickEvent, _window, cx| {
            if event.click_count() >= 2 {
                cx.emit(DiffEvent::OpenAt {
                    path: path.clone(),
                    line: 1,
                });
            } else {
                this.choose_file(file_index, cx);
            }
        }))
        .into_any_element()
}

/// One row, section 6.10: a file's header on the panel's surface, an
/// `@@` row in faint, and the lines with two number columns, the marker
/// and the text on their backgrounds.
#[allow(clippy::too_many_arguments)]
fn render_line(
    index: usize,
    line: &Line,
    file: &FileDiff,
    paint: Option<&Painted>,
    look: &crate::file_tab::Look,
    ui: Ui,
    mode: guild_session::DiffView,
    cx: &mut Context<DiffTab>,
) -> gpui::AnyElement {
    let theme = look.theme;
    let family: SharedString = look.family.clone();
    let size = px(look.font_size);
    let row = div()
        .id(("diff-row", index))
        .w_full()
        .h(theme::space::TREE_ROW)
        .flex()
        .items_center()
        .font_family(family.clone())
        .text_size(size)
        .whitespace_nowrap();

    match line {
        // Drawn by their own functions; never reach here.
        Line::Source { .. } | Line::Summary | Line::Listed { .. } | Line::Pair { .. } => {
            row.into_any_element()
        }
        Line::File { .. } => {
            let name = file
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stem = icons::for_file(&name, icons::Palette::from(theme.kind));
            let (added, removed) = file.counts;
            // The view's name at the far right is the button to the
            // other: the tab's own, the setting being the next tab's.
            let switch_hint = if ui.0 == guild_session::Language::Russian {
                "Переключить вид сравнения"
            } else {
                "Switch diff view"
            };
            let switch = div()
                .id(("diff-view", index))
                .debug_selector(move || format!("diff-inline-mode-{index}"))
                .aria_label(switch_hint)
                .hint(switch_hint, theme)
                .flex_none()
                .pl(theme::space::TREE_GAP)
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.faint))
                .cursor_pointer()
                .child(ui.text(match mode {
                    guild_session::DiffView::SideBySide => Text::DiffSideBySide,
                    guild_session::DiffView::Unified => Text::DiffUnified,
                }))
                .on_click(cx.listener(|this, _event, _window, cx| {
                    cx.stop_propagation();
                    this.toggle_mode(cx);
                }));
            row.gap(theme::space::TREE_GAP)
                .px(theme::space::TREE_PADDING)
                .bg(rgb(theme.surface.panel))
                .child(icons::icon(&stem).flex_none())
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_color(rgb(theme.text.primary))
                        .child(file.path.display().to_string()),
                )
                .when(added > 0, |this| {
                    this.child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.accent.green))
                            .child(format!("+{added}")),
                    )
                })
                .when(removed > 0, |this| {
                    this.child(
                        div()
                            .flex_none()
                            .text_color(rgb(theme.accent.rust))
                            .child(format!("\u{2212}{removed}")),
                    )
                })
                .child(switch)
                .test_support()
                .into_any_element()
        }
        Line::Note { .. } => {
            let text: String = match &file.state {
                State::Loading => ui.text(Text::DiffLoading).to_owned(),
                State::NoChanges => ui.text(Text::DiffNoChanges).to_owned(),
                State::Binary => ui.text(Text::DiffBinary).to_owned(),
                State::TooLarge => ui.text(Text::DiffTooLarge).to_owned(),
                State::NoRepository => ui.text(Text::DiffNoRepository).to_owned(),
                State::NoGit => ui.text(Text::DiffNoGit).to_owned(),
                State::Failed(why) => why.clone(),
                State::Hunks | State::Added | State::Deleted => String::new(),
            };
            row.bg(rgb(theme.surface.app))
                .px(theme::space::PANEL_PADDING)
                .text_color(rgb(theme.text.faint))
                .child(text)
                .into_any_element()
        }
        Line::Hunk { row: hunk_row, .. } => {
            let path = file.path.clone();
            let numbers = |old: Option<usize>, new: Option<usize>| {
                let column = |n: Option<usize>| {
                    div()
                        .flex_none()
                        .w(theme::space::DIFF_NUMBERS)
                        .pr(theme::space::TREE_GAP)
                        .text_color(rgb(theme.text.gutter))
                        .child(match n {
                            Some(n) => format!("{n:>4}"),
                            None => String::new(),
                        })
                };
                (column(old), column(new))
            };
            let (bg, marker, marker_colour, text_colour, old, new, text, line_at) = match hunk_row {
                Row::Header {
                    old_start,
                    old_len,
                    new_start,
                    new_len,
                } => (
                    theme.surface.app,
                    "",
                    theme.text.faint,
                    theme.text.faint,
                    None,
                    None,
                    format!("@@ -{old_start},{old_len} +{new_start},{new_len} @@"),
                    *new_start,
                ),
                Row::Context { old, new, text } => (
                    theme.surface.app,
                    "",
                    theme.text.secondary,
                    theme.text.primary,
                    Some(*old),
                    Some(*new),
                    text.clone(),
                    *new,
                ),
                Row::Added { new, text } => (
                    theme.diff.added,
                    "+",
                    theme.accent.green,
                    theme.text.primary,
                    None,
                    Some(*new),
                    text.clone(),
                    *new,
                ),
                Row::Removed { old, text } => (
                    theme.diff.removed,
                    "\u{2212}",
                    theme.accent.rust,
                    theme.text.primary,
                    Some(*old),
                    None,
                    text.clone(),
                    *old,
                ),
                Row::NoNewline => (
                    theme.surface.app,
                    "",
                    theme.text.faint,
                    theme.text.faint,
                    None,
                    None,
                    "\\ No newline at end of file".to_owned(),
                    0,
                ),
            };
            let (old_column, new_column) = numbers(old, new);
            // The new side where there is one — a context line is the
            // same text on both — and the old side for what was removed.
            let spans = paint.and_then(|paint| match (old, new) {
                (_, Some(number)) => paint.new.of_line(number, &text),
                (Some(number), None) => paint.old.of_line(number, &text),
                (None, None) => None,
            });
            let text = super::code(text, spans, &theme);
            row.bg(rgb(bg))
                .child(old_column)
                .child(new_column)
                .child(
                    div()
                        .flex_none()
                        .w(theme::space::TREE_GAP * 2.0)
                        .text_color(rgb(marker_colour))
                        .child(marker),
                )
                .child(
                    div()
                        .flex_1()
                        .overflow_hidden()
                        .text_color(rgb(text_colour))
                        .child(text),
                )
                // Decision 8: a double click on a line opens the file at
                // it — the one gesture that leaves the diff.
                .when(line_at > 0, |this| {
                    this.cursor_pointer().on_click(cx.listener(
                        move |_this, event: &ClickEvent, _window, cx| {
                            if event.click_count() == 2 {
                                cx.emit(DiffEvent::OpenAt {
                                    path: path.clone(),
                                    line: line_at,
                                });
                            }
                        },
                    ))
                })
                .into_any_element()
        }
    }
}

/// One row of the side-by-side view, section 6.10: two halves of equal
/// width with a hairline between them, each with its own number column,
/// marker and text on its own background — the old side on the left,
/// the new on the right, a changed line facing what replaced it. The
/// `@@` row and the no-newline note span both halves, drawn as the
/// unified view draws them.
#[allow(clippy::too_many_arguments)]
fn render_pair(
    index: usize,
    pair: &Pair,
    file: &FileDiff,
    paint: Option<&Painted>,
    look: &crate::file_tab::Look,
    ui: Ui,
    mode: guild_session::DiffView,
    cx: &mut Context<DiffTab>,
) -> gpui::AnyElement {
    let theme = look.theme;
    let whole = match pair {
        Pair::Header {
            old_start,
            old_len,
            new_start,
            new_len,
        } => Some(Row::Header {
            old_start: *old_start,
            old_len: *old_len,
            new_start: *new_start,
            new_len: *new_len,
        }),
        Pair::NoNewline => Some(Row::NoNewline),
        _ => None,
    };
    if let Some(row) = whole {
        let line = Line::Hunk { file: 0, row };
        return render_line(index, &line, file, paint, look, ui, mode, cx);
    }
    let family: SharedString = look.family.clone();
    let size = px(look.font_size);
    let path = file.path.clone();

    // A half: the number, the marker, the text, on the background the
    // line's kind asks for; empty when nothing faces the other side.
    let half = |id: &'static str,
                is_new: bool,
                number: Option<usize>,
                text: Option<&str>,
                marker: &'static str,
                marker_colour: u32,
                bg: u32,
                line_at: usize,
                cx: &mut Context<DiffTab>| {
        let path = path.clone();
        div()
            .id((id, index))
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .bg(rgb(bg))
            .child(
                div()
                    .flex_none()
                    .w(theme::space::DIFF_NUMBERS)
                    .pr(theme::space::TREE_GAP)
                    .text_color(rgb(theme.text.gutter))
                    .child(number.map(|n| format!("{n:>4}")).unwrap_or_default()),
            )
            .child(
                div()
                    .flex_none()
                    .w(theme::space::TREE_GAP * 2.0)
                    .text_color(rgb(marker_colour))
                    .child(if text.is_some() { marker } else { "" }),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_color(rgb(theme.text.primary))
                    .child({
                        // The left half is always the old side and the
                        // right always the new; each asks its own.
                        let line = text.unwrap_or_default().to_owned();
                        let spans = paint.zip(number).and_then(|(paint, number)| {
                            let side = if is_new { &paint.new } else { &paint.old };
                            side.of_line(number, &line)
                        });
                        super::code(line, spans, &theme)
                    }),
            )
            .when(line_at > 0, |this| {
                this.cursor_pointer().on_click(cx.listener(
                    move |_this, event: &ClickEvent, _window, cx| {
                        if event.click_count() == 2 {
                            cx.emit(DiffEvent::OpenAt {
                                path: path.clone(),
                                line: line_at,
                            });
                        }
                    },
                ))
            })
    };

    let (left, right) = match pair {
        Pair::Context { old, new, text } => (
            half(
                "pair-left",
                false,
                Some(*old),
                Some(text),
                "",
                theme.text.secondary,
                theme.surface.app,
                *new,
                cx,
            ),
            half(
                "pair-right",
                true,
                Some(*new),
                Some(text),
                "",
                theme.text.secondary,
                theme.surface.app,
                *new,
                cx,
            ),
        ),
        Pair::Changed { old, new } => (
            half(
                "pair-left",
                false,
                old.as_ref().map(|(n, _)| *n),
                old.as_ref().map(|(_, t)| t.as_str()),
                "\u{2212}",
                theme.accent.rust,
                if old.is_some() {
                    theme.diff.removed
                } else {
                    theme.surface.app
                },
                old.as_ref().map_or(0, |(n, _)| *n),
                cx,
            ),
            half(
                "pair-right",
                true,
                new.as_ref().map(|(n, _)| *n),
                new.as_ref().map(|(_, t)| t.as_str()),
                "+",
                theme.accent.green,
                if new.is_some() {
                    theme.diff.added
                } else {
                    theme.surface.app
                },
                new.as_ref().map_or(0, |(n, _)| *n),
                cx,
            ),
        ),
        Pair::Header { .. } | Pair::NoNewline => unreachable!("drawn whole above"),
    };

    div()
        .id(("diff-pair", index))
        .w_full()
        .h(theme::space::TREE_ROW)
        .flex()
        .font_family(family)
        .text_size(size)
        .whitespace_nowrap()
        .child(left)
        .child(
            div()
                .flex_none()
                .w(px(1.0))
                .h_full()
                .bg(rgb(theme.surface.divider)),
        )
        .child(right)
        .into_any_element()
}
