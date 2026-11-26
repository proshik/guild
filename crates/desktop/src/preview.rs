//! The code around a lit row, painted by `desktop::syntax` rather than by
//! the editor component — M8.9, decision 13. An `InputState` takes the
//! focus when its caret is set and is a tab stop, so a preview built from
//! one would take the keys away from the popup it sits in.
//!
//! What can be said without a window: the excerpt centred on a line, cut
//! from a text painted **whole** — so a block comment opened above the
//! window is still a comment — and the marks drawn only on a line that is
//! the hit's own text. The asking is `root::preview`, the drawing
//! `view::preview`.

use std::ops::Range;

use crate::syntax::{self, Span};

/// How many bytes of one line a preview draws: the Search tool's row
/// limit, because a minified line is as unreadable in one as in the other.
pub const LINE_LIMIT: usize = crate::search::LINE_LIMIT;

/// The lines around one line of a text, with their spans.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Excerpt {
    /// The zero-based number of the first line.
    pub first: u32,
    /// The lines, without their line breaks, each cut to [`LINE_LIMIT`].
    pub lines: Vec<String>,
    /// Each line's spans, in bytes of that line as it is in `lines`. An
    /// empty row is a line drawn plain.
    pub spans: Vec<Vec<Span>>,
}

/// The `2 × around + 1` lines around zero-based `line`, **sliding at the
/// ends** so that a hit near the top or the bottom still has a full window,
/// painted with `grammar` when there is one and the text is within the
/// file tab's highlight limit.
pub fn excerpt(text: &str, grammar: Option<&str>, line: u32, around: usize) -> Excerpt {
    let ranges = syntax::line_ranges(text);
    let Some(last) = ranges.len().checked_sub(1) else {
        return Excerpt::default();
    };
    let wanted = 2 * around + 1;
    let line = (line as usize).min(last);
    let mut first = line.saturating_sub(around);
    let end = (first + wanted).min(ranges.len());
    first = end.saturating_sub(wanted);

    // **Painted whole, cut after.** A parse of the window alone would read
    // the inside of a block comment opened above it as code.
    let painted = grammar
        .filter(|_| text.len() as u64 <= crate::file_tab::HIGHLIGHT_LIMIT)
        .map(|grammar| syntax::spans(text, grammar));

    let mut lines = Vec::with_capacity(end - first);
    let mut spans = Vec::with_capacity(end - first);
    for at in first..end {
        let whole = &text[ranges[at].clone()];
        let whole = whole.strip_suffix('\r').unwrap_or(whole);
        let kept = cut(whole);
        let row = painted
            .as_ref()
            .and_then(|painted| painted.get(at))
            .map(|row| within(row, kept.len()))
            .unwrap_or_default();
        lines.push(kept.to_owned());
        spans.push(row);
    }
    Excerpt {
        first: first as u32,
        lines,
        spans,
    }
}

/// The ranges to mark on a preview line: the hit's own, **and only when the
/// line is the hit's text** — `SidePaint::of_line`'s rule. A file changed
/// under the popup has moved its lines, and marks on whatever is there now
/// would put brass on words that never matched.
pub fn marks<'a>(line: &str, hit: &'a (String, Vec<Range<usize>>)) -> Option<&'a [Range<usize>]> {
    (line == hit.0).then_some(hit.1.as_slice())
}

/// A line cut to [`LINE_LIMIT`] bytes, on a character boundary.
fn cut(line: &str) -> &str {
    if line.len() <= LINE_LIMIT {
        return line;
    }
    let mut end = LINE_LIMIT;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

/// A line's spans as far as `len` bytes: past it dropped, across it cut.
/// `len` is a character boundary, and so is every span's start, so what is
/// left starts and ends on characters.
fn within(row: &[Span], len: usize) -> Vec<Span> {
    row.iter()
        .filter(|span| span.at.start < len)
        .map(|span| Span {
            at: span.at.start..span.at.end.min(len),
            paint: span.paint,
        })
        .filter(|span| span.at.start < span.at.end)
        .collect()
}

/// One stretch of a preview line: its bytes, its colour, and whether a
/// match is marked on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Where it is in the line, in bytes.
    pub at: Range<usize>,
    /// What the grammar painted it, if anything.
    pub paint: Option<crate::highlight::Paint>,
    /// Whether it is inside a match.
    pub marked: bool,
}

/// A line of `len` bytes as runs that cover it once each, in order: cut
/// wherever a span or a mark begins or ends, so a mark is painted over the
/// colours it crosses and they go on either side of it.
///
/// Marks past `len` are dropped — a line cut to [`LINE_LIMIT`] draws no
/// mark it no longer holds. Syntax spans follow the ordered, disjoint output
/// of `syntax::spans`; search marks may overlap or arrive out of order.
pub fn runs(spans: &[Span], marks: &[Range<usize>], len: usize) -> Vec<Run> {
    #[cfg(test)]
    begin_span_pass(spans.len());
    let mut cuts = vec![0, len];
    for range in spans.iter().map(|span| &span.at).chain(marks) {
        cuts.push(range.start.min(len));
        cuts.push(range.end.min(len));
    }
    cuts.sort_unstable();
    cuts.dedup();
    // Syntax spans are ordered by the syntax engine. Sort search marks once
    // so overlapping/nested marks keep their union semantics without rescans.
    let mut marks = marks.to_vec();
    marks.sort_unstable_by_key(|range| range.start);
    let mut span_index = 0;
    let mut mark_index = 0;
    cuts.windows(2)
        .map(|pair| pair[0]..pair[1])
        .filter(|at| at.start < at.end)
        .map(|at| {
            while spans.get(span_index).is_some_and(|span| {
                #[cfg(test)]
                span_visit();
                span.at.end <= at.start
            }) {
                span_index += 1;
            }
            while marks
                .get(mark_index)
                .is_some_and(|mark| mark.end <= at.start)
            {
                mark_index += 1;
            }
            Run {
                paint: spans
                    .get(span_index)
                    .filter(|span| {
                        #[cfg(test)]
                        span_visit();
                        span.at.start <= at.start && at.end <= span.at.end
                    })
                    .and_then(|span| span.paint),
                marked: marks
                    .get(mark_index)
                    .is_some_and(|mark| mark.start <= at.start && at.end <= mark.end),
                at,
            }
        })
        .collect()
}

#[cfg(test)]
std::thread_local! {
    static AUDIT_SPANS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static SPAN_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PASS_VISITS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PASS_LIMIT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static TOTAL_LIMIT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static PASSES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
#[cfg(test)]
fn begin_span_pass(spans: usize) {
    PASS_VISITS.set(0);
    PASS_LIMIT.set(spans * 8 + 8);
    if AUDIT_SPANS.get() {
        TOTAL_LIMIT.set(TOTAL_LIMIT.get() + PASS_LIMIT.get());
        PASSES.set(PASSES.get() + 1);
    }
}
#[cfg(test)]
fn span_visit() {
    if AUDIT_SPANS.get() {
        SPAN_VISITS.set(SPAN_VISITS.get() + 1);
        PASS_VISITS.set(PASS_VISITS.get() + 1);
        assert!(
            PASS_VISITS.get() <= PASS_LIMIT.get(),
            "Annotate syntax preparation exceeded its linear per-pass span-visit budget"
        );
    }
}

#[cfg(test)]
mod occurrence_preparation_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    // The renderer's private work counter guards computational work, not a
    // machine-dependent latency threshold. No test-only production API is exposed.
    #[gpui::test]
    fn dense_annotate_source_preparation_visits_syntax_spans_linearly(cx: &mut TestAppContext) {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                AUDIT_SPANS.set(false);
            }
        }
        let _reset = Reset;
        SPAN_VISITS.set(0);
        AUDIT_SPANS.set(true);
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let tab = cx.new(|cx| {
                crate::annotate_tab::AnnotateTab::new(
                    "dense.js".into(),
                    crate::file_tab::Look {
                        family: "Menlo".into(),
                        font_size: 14.,
                        theme: Default::default(),
                        autosave: false,
                    },
                    crate::i18n::Ui(guild_session::Language::English),
                    cx,
                )
            });
            tab.update(cx, |tab, cx| {
                tab.set_runs(
                    Ok(vec![crate::git::blame::Run {
                        sha: "a".repeat(40),
                        author: "fixture".into(),
                        time: 1,
                        summary: "dense".into(),
                        first_line: 1,
                        original_line: 1,
                        filename: "dense.js".into(),
                        lines: vec!["0;".repeat(50_000)],
                    }]),
                    cx,
                )
            });
            owner = Some(tab.clone());
            gpui_component::Root::new(tab, window, cx)
        });
        cx.run_until_parked();
        let owner = owner.unwrap();
        cx.update(|window,cx|{
            let spans=owner.read(cx).spans()[0].len();assert!(spans>=50_000,"real JavaScript syntax must be dense");
            SPAN_VISITS.set(0);TOTAL_LIMIT.set(0);PASSES.set(0);
            owner.update(cx,|_,cx|cx.notify());window.draw(cx).clear(cx);
            assert!(SPAN_VISITS.get()>0,"exercise actual Annotate preparation before any click");
            assert!(SPAN_VISITS.get()<=TOTAL_LIMIT.get());
            eprintln!("Annotate bytes=100000 syntax_spans={spans} preparation_passes={} span_visits={} budget={}",PASSES.get(),SPAN_VISITS.get(),TOTAL_LIMIT.get());
        });
    }
}
