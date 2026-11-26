//! The Problems panel: what every server of this project has said.
//!
//! M8.8.5, and the sixth tool. **A tool and not a popup** — decision 14:
//! the list is already the data, and a tool costs one `Tool` variant and
//! reuses the dock, the status-bar button and the persisted placement
//! that the other five already have, where a popup costs a second
//! occluding surface with its own drag, focus and dismissal rules — all
//! of which this crate has learned the hard way at least twice.
//!
//! The rows are the search panel's, in the same three kinds and the same
//! heights: a directory, a file, and under it the things themselves. It
//! has **no toolbar**, because there is nothing to control — nothing is
//! typed at it and nothing is filtered — which is also why the rule
//! about controls outliving the note that replaces a list has nothing to
//! bite on here.

use gpui::prelude::*;
use gpui::{Context, SharedString, div, rgb, uniform_list};

use crate::Root;
use crate::root::problems::{Problem, Row, Severity, foldable};
use crate::theme;

impl Root {
    /// The panel's body: the rows, or nothing when there are none — the
    /// panel's own placeholder says what the emptiness is.
    pub(crate) fn render_problems(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let (problems, capped) = self.problems();
        let rows = self.problem_rows(&problems);
        if rows.is_empty() {
            return None;
        }
        let folded = self.problems_folded();
        let scroll = self.problems_scroll();
        let count = rows.len();

        Some(
            div()
                .id("problems")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    div().flex_1().min_h_0().child(
                        uniform_list(
                            "problem-rows",
                            count,
                            cx.processor(
                                move |_root, range: std::ops::Range<usize>, _window, cx| {
                                    range
                                        .map(|index| {
                                            row(&rows[index], index, &problems, &folded, theme, cx)
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .track_scroll(&scroll)
                        .size_full(),
                    ),
                )
                // **A count with no note is a lie about the project.**
                // The cap is high enough that nothing has reached it,
                // which is exactly why the line that says it was reached
                // has to exist before anything does.
                .children(capped.then(|| {
                    div()
                        .flex_none()
                        .px(theme::space::TREE_PADDING)
                        .py(theme::space::CARD_INNER_GAP)
                        .text_size(theme::size::EYEBROW)
                        .text_color(rgb(theme.text.faint))
                        .child(self.ui().text(crate::i18n::Text::ProblemsCapped))
                }))
                .into_any_element(),
        )
    }
}

/// One row of the panel, whichever kind it is.
fn row(
    row: &Row,
    index: usize,
    problems: &[Problem],
    folded: &std::collections::HashSet<std::path::PathBuf>,
    theme: crate::theme::Theme,
    cx: &mut Context<Root>,
) -> gpui::AnyElement {
    let depth = match row {
        Row::Directory { depth, .. } | Row::File { depth, .. } | Row::Problem { depth, .. } => {
            *depth
        }
    };
    let line = div()
        .id(SharedString::from(format!("problem-row-{index}")))
        .flex()
        // A row as wide as its content passes for a highlight that stops
        // at the text — M8.1's trap, and the reason this is here.
        .w_full()
        .items_center()
        .gap(theme::space::CARD_GAP)
        .h(theme::space::TREE_ROW)
        .pr(theme::space::TREE_PADDING)
        .pl(theme::space::TREE_PADDING + theme::space::TREE_INDENT * depth as f32)
        .text_size(theme::size::META);

    match row {
        Row::Directory {
            label,
            problems: n,
            worst,
            ..
        }
        | Row::File {
            name: label,
            problems: n,
            worst,
            ..
        } => {
            let file = matches!(row, Row::File { .. });
            let at = foldable(row).expect("a directory and a file both fold");
            let shut = folded.contains(&at);
            line.cursor_pointer()
                .hover(|line| line.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                .child(chevron(shut, theme))
                .children(file.then(|| {
                    crate::icons::icon(&crate::icons::for_file(
                        label,
                        crate::icons::Palette::from(theme.kind),
                    ))
                    .flex_none()
                }))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(if file {
                            theme.text.primary
                        } else {
                            theme.text.secondary
                        }))
                        .child(label.clone()),
                )
                // **The worst severity beneath is a disc, and the count
                // is a plain number.** They were one thing — a count in
                // the worst severity's colour — until the window was
                // driven: *117* in `accent.rust` beside a status-bar
                // button saying *58* reads as two counts of errors that
                // disagree. The button counts errors and warnings, this
                // counts every row drawn beneath, and the disc is the
                // half that says *how bad*, in the same shape and the
                // same colours a problem row uses.
                .child(
                    div()
                        .flex_none()
                        .size(theme::space::LOG_DOT)
                        .rounded_full()
                        .bg(rgb(paint(*worst, theme))),
                )
                .child(
                    div()
                        .flex_none()
                        .text_size(theme::size::EYEBROW)
                        .text_color(rgb(theme.text.faint))
                        .child(n.to_string()),
                )
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.toggle_problems_fold(at.clone(), cx);
                }))
                .into_any_element()
        }
        Row::Problem { at, .. } => {
            let Some(problem) = problems.get(*at) else {
                return line.into_any_element();
            };
            let which = *at;
            line.cursor_pointer()
                .hover(|line| line.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                // A disc rather than a letter or an icon: the four
                // colours are the ones the squiggles in the editor
                // already use, so the panel and the file agree without
                // anybody having to learn a second alphabet.
                .child(
                    div()
                        .flex_none()
                        .size(theme::space::LOG_DOT)
                        .rounded_full()
                        .bg(rgb(paint(problem.severity, theme))),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.gutter))
                        // 0-based from the protocol, 1-based on the
                        // screen — the same arithmetic as the caret's
                        // place in the status bar, and for the same
                        // reason.
                        .child(format!("{}:{}", problem.line + 1, problem.character + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_color(rgb(theme.text.secondary))
                        // **The server's own words, on one line.**
                        // rust-analyzer sends paragraphs; the panel is a
                        // list, and the whole message is one hover away
                        // in the editor's own card.
                        .child(one_line(&problem.message)),
                )
                // Which server said it, when it names itself — *rustc*,
                // *clippy*. Two servers can have opinions about one file
                // and disagree, and this is what tells them apart.
                .children(problem.source.clone().map(|source| {
                    div()
                        .flex_none()
                        .text_size(theme::size::EYEBROW)
                        .text_color(rgb(theme.text.faint))
                        .child(source)
                }))
                .on_click(cx.listener(move |root, _event, window, cx| {
                    cx.stop_propagation();
                    root.open_problem(which, window, cx);
                }))
                .into_any_element()
        }
    }
}

/// The colour a severity is drawn in — section 6.21's four, the same
/// ones the component paints the squiggles with.
fn paint(severity: Severity, theme: crate::theme::Theme) -> u32 {
    match severity {
        Severity::Error => theme.accent.rust,
        Severity::Warning => theme.accent.brass,
        Severity::Info => theme.accent.patina,
        Severity::Hint => theme.text.faint,
    }
}

/// The one line of a message that a row can hold.
///
/// rust-analyzer's are paragraphs — a type mismatch arrives with the
/// expected type, the found type and a note, each on its own line. The
/// first line is the sentence; the rest is the explanation, and the
/// explanation belongs where there is room for it.
fn one_line(message: &str) -> String {
    message
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or(message)
        .trim()
        .to_owned()
}

/// The fold marker: right when shut, down when open — the tree's own, in
/// the tree's own size.
fn chevron(shut: bool, theme: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .size(theme::space::CHEVRON)
        .child(crate::icons::chevron(!shut).text_color(rgb(theme.text.faint)))
        .into_any_element()
}
