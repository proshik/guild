//! The annotate tab, drawn: the column, the line number and the line.
//!
//! Section 6.14: each row is the diff row's height; the column at the
//! left is 200 wide and says, on a run's first line, who wrote it and
//! how long ago — `text.secondary` for the newest third of the file's
//! commits, `text.tertiary` for the middle, `text.faint` for the oldest
//! — and nothing on the run's other lines; a hover on the column gives
//! the hash and the summary, a click opens the commit in the Log. A
//! line not committed yet says so in `text.faint`. Then the number in
//! the gutter's colour and the line, plain, in the code face.

use crate::view::tooltip::Hint as _;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, IntoElement, Render, SharedString, Window, div, px, rgb, uniform_list};

use crate::annotate_tab::{AnnotateTab, lines, shades};
use crate::git::GitError;
use crate::i18n::Text;
use crate::syntax::Span;
use crate::{icons, theme};

impl Render for AnnotateTab {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let look = self.look().clone();
        let theme = look.theme;
        let ui = self.ui();
        let family: SharedString = look.family.clone();
        let size = px(look.font_size);
        let focus = self.focus_handle(cx);
        let scroll = self.scroll.clone();

        let name = self
            .path()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let stem = icons::for_file(&name, icons::Palette::from(theme.kind));
        let head = div()
            .flex_none()
            .h(theme::space::PANEL_HEAD)
            .flex()
            .items_center()
            .gap(theme::space::TREE_GAP)
            .px(theme::space::TREE_PADDING)
            .bg(rgb(theme.surface.panel))
            .font_family(family.clone())
            .text_size(size)
            .text_color(rgb(theme.text.primary))
            .child(icons::icon(&stem).flex_none())
            .child(self.path().display().to_string())
            .child(div().flex_1())
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(Text::AnnotateTab)),
            );

        let body: gpui::AnyElement = match self.runs() {
            None => note(ui.text(Text::LogLoading), theme),
            Some(Err(GitError::NoGit)) => note(ui.text(Text::DiffNoGit), theme),
            Some(Err(GitError::NoRepository)) => note(ui.text(Text::DiffNoRepository), theme),
            Some(Err(GitError::Failed(words))) => note(words, theme),
            Some(Ok(runs)) => {
                let runs = Rc::new(runs.clone());
                let rows = Rc::new(lines(&runs));
                let dim = Rc::new(shades(&runs));
                let source = self.source.clone();
                let source_offsets = self.source_offsets.clone();
                // Read here, not in the row: a row is built inside this
                // entity's own update, and reading the entity there is a
                // panic.
                let spans: Rc<Vec<Vec<Span>>> = Rc::new(self.spans().to_vec());
                let count = rows.len();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs() as i64);
                uniform_list(
                    "annotate-rows",
                    count,
                    cx.processor(
                        move |_this: &mut AnnotateTab,
                              range: std::ops::Range<usize>,
                              _window,
                              cx| {
                            range
                                .map(|index| {
                                    let (run_index, offset) = rows[index];
                                    let run = &runs[run_index];
                                    let number = run.first_line + offset;
                                    let colour = match dim[run_index] {
                                        0 => theme.text.secondary,
                                        1 => theme.text.tertiary,
                                        _ => theme.text.faint,
                                    };
                                    let column = div()
                                        .id(("annotate-run", index))
                                        .flex_none()
                                        .w(theme::space::ANNOTATE_COLUMN)
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_size(theme::size::META)
                                        .when(offset == 0, |this| {
                                            if run.is_uncommitted() {
                                                this.text_color(rgb(theme.text.faint))
                                                    .child(ui.text(Text::AnnotateNotCommitted))
                                            } else {
                                                let sha = run.sha.clone();
                                                let tip = format!(
                                                    "{} \u{b7} {}",
                                                    run.short(),
                                                    run.summary
                                                );
                                                this.text_color(rgb(colour))
                                                    .cursor_pointer()
                                                    .child(format!(
                                                        "{} \u{b7} {}",
                                                        run.author,
                                                        ui.relative_time(run.time, now)
                                                    ))
                                                    .hint_delay()
                                                    .tooltip(move |_window, app| {
                                                        app.new(|_cx| {
                                                            crate::view::tooltip::Tooltip::text(
                                                                tip.clone(),
                                                                theme,
                                                            )
                                                        })
                                                        .into()
                                                    })
                                                    .on_click(cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            cx.stop_propagation();
                                                            this.open_commit(sha.clone(), cx);
                                                        },
                                                    ))
                                            }
                                        });
                                    div()
                                        .id(("annotate-row", index))
                                        .w_full()
                                        .h(theme::space::TREE_ROW)
                                        .flex()
                                        .items_center()
                                        .gap(theme::space::TREE_GAP)
                                        .px(theme::space::TREE_PADDING)
                                        .font_family(family.clone())
                                        .text_size(size)
                                        .whitespace_nowrap()
                                        .child(column)
                                        .child(
                                            div()
                                                .flex_none()
                                                .w(theme::space::DIFF_NUMBERS)
                                                .text_color(rgb(theme.text.gutter))
                                                .child(number.to_string()),
                                        )
                                        .child(
                                            div()
                                                .flex_1()
                                                .overflow_hidden()
                                                .text_color(rgb(theme.text.primary))
                                                .child(
                                                    crate::source_snippet::SourceSnippet::new(
                                                        ("annotate-source", index),
                                                        run.lines[offset].clone(),
                                                        super::preview::line_highlights(
                                                            &run.lines[offset],
                                                            spans
                                                                .get(index)
                                                                .map(Vec::as_slice)
                                                                .unwrap_or(&[]),
                                                            &[],
                                                            &theme,
                                                        ),
                                                        theme,
                                                    )
                                                    .source(
                                                        &source,
                                                        source_offsets
                                                            .get(index)
                                                            .copied()
                                                            .unwrap_or(0),
                                                    ),
                                                ),
                                        )
                                })
                                .collect::<Vec<_>>()
                        },
                    ),
                )
                .track_scroll(&scroll)
                .size_full()
                .into_any_element()
            }
        };

        div()
            .id("annotate")
            .track_focus(&focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(theme.surface.app))
            .child(head)
            .child(div().flex_1().min_h_0().child(body))
    }
}

fn note(text: &str, theme: theme::Theme) -> gpui::AnyElement {
    div()
        .p(theme::space::PANEL_PADDING)
        .text_size(theme::size::BODY)
        .text_color(rgb(theme.text.faint))
        .child(text.to_owned())
        .into_any_element()
}
