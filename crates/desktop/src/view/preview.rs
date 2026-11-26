//! The preview under a popup's list: the excerpt in the code face, its
//! line numbers in `text.gutter`, the hit's line raised and its marks in
//! brass. Section 6.34.

use gpui::prelude::*;
use gpui::{Context, HighlightStyle, SharedString, div, px, rgb};

use crate::Root;
use crate::theme;

impl Root {
    /// The preview asked for last, when it has landed.
    pub(crate) fn render_preview(&self, _cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let shown = self.preview_shown()?;
        let theme = self.theme;
        // **The editor's look**: its face, its size and its leading, from
        // the same `Look` — `meta` at GPUI's default leading was a smaller,
        // denser copy of the code the preview stands for.
        let look = self.file_look();
        let family: SharedString = look.family.clone();
        let excerpt = &shown.excerpt;

        let rows = excerpt.lines.iter().enumerate().map(|(at, line)| {
            let number = excerpt.first + at as u32;
            let own = number == shown.asked.line;
            // **Marked only on a line that is the hit's own text.**
            let marks = shown
                .asked
                .hit
                .as_ref()
                .filter(|_| own)
                .and_then(|hit| crate::preview::marks(line, hit))
                .unwrap_or(&[]);
            let spans = excerpt.spans.get(at).map(Vec::as_slice).unwrap_or(&[]);
            div()
                .flex()
                .w_full()
                .when(own, |row| row.bg(rgb(theme.surface.raised)))
                .child(
                    div()
                        .flex_none()
                        .w(theme::space::DIFF_NUMBERS)
                        .pr(px(8.0))
                        .flex()
                        .justify_end()
                        .text_color(rgb(theme.text.gutter))
                        .child((number + 1).to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.text.primary))
                        .child(
                            crate::source_snippet::SourceSnippet::new(
                                ("popup-source", at),
                                line.clone(),
                                line_highlights(line, spans, marks, &theme),
                                theme,
                            )
                            .source(&shown.source, shown.offsets.get(at).copied().unwrap_or(0)),
                        ),
                )
        });

        Some(
            div()
                .id("picker-preview")
                .flex_none()
                .h(theme::space::PREVIEW)
                .overflow_hidden()
                .border_t_1()
                .border_color(rgb(theme.surface.divider))
                .bg(rgb(theme.surface.sunken))
                .py(px(4.0))
                .font_family(family)
                .text_size(look.text_size())
                .line_height(look.line_height())
                .flex()
                .flex_col()
                .children(rows)
                .into_any_element(),
        )
    }
}

pub(crate) fn line_highlights(
    line: &str,
    spans: &[crate::syntax::Span],
    marks: &[std::ops::Range<usize>],
    theme: &crate::theme::Theme,
) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> =
        crate::preview::runs(spans, marks, line.len())
            .into_iter()
            .filter_map(|run| {
                let (colour, italic, bold) = match (run.marked, run.paint) {
                    (true, _) => (theme.accent.brass, false, false),
                    (false, Some(paint)) => theme.of_paint(paint),
                    (false, None) => return None,
                };
                Some((
                    run.at,
                    HighlightStyle {
                        color: Some(rgb(colour).into()),
                        font_style: italic.then_some(gpui::FontStyle::Italic),
                        font_weight: bold.then_some(gpui::FontWeight::BOLD),
                        ..Default::default()
                    },
                ))
            })
            .collect();
    highlights
}
