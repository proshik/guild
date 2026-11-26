//! The spans of a text: which stretch of which line is painted with what.
//!
//! This is the half of highlighting that is ours. The editor component
//! paints a file in a tab and cannot reach anything else — it has no diff
//! view, which is why M8.2 drew one — so the diff, Annotate and a
//! commit's files are coloured from here.
//!
//! **The two engines agree by construction, not by care.** They read the
//! same grammar out of [`crate::grammars`], the same query with the same
//! captures renamed, and the same table in [`crate::highlight`]; and the
//! rules for a capture inside another are copied from the component
//! rather than invented, because a different rule is the same file
//! looking different in two places. There are **two** of those rules and
//! the first copy took only one: `unique_styles` lets a strictly-inner
//! capture win, and `match_styles` before it keeps the **earlier** name
//! when two captures cover exactly the same bytes — which is what makes
//! `(comment) @comment @spell` a comment rather than a spell-check
//! annotation.
//!
//! **Injections are part of that agreement and not a refinement of it.**
//! HTML's own highlights query is thirteen lines; everything that makes
//! an HTML file look coloured is `#set! injection.language` handing
//! `<script>` to javascript and `<style>` to css, and the component
//! follows those in a tab. So a module that did not would colour a page
//! in a tab and leave it grey in a diff, in Annotate and in a commit's
//! files — and the sentence above would be false for the one kind of
//! file where it matters most. A region is parsed **in place**, through
//! `set_included_ranges`, so its nodes carry offsets into the original
//! text and there is no arithmetic to get wrong; only a static `#set!`
//! names the language, because that is the only form the component
//! resolves; and a grammar reaches only what it declares in `injects`,
//! which is the list the component resolves against.
//!
//! Pure: no window, no component, no entity. It costs a parse and a
//! query — 5.2 ms and 4.0 ms for three thousand lines, measured in task 0
//! of M8.1.1 — so it is called once per version of a text, off the
//! window's thread, and never per frame. **An injection is another parse
//! and another query over the region alone**: on a 3,011-line page whose
//! script and style blocks are two thirds of it, 12.5 ms against 3.4 ms
//! for the same page with nothing followed — nine milliseconds to colour
//! two thousand lines that had no colour at all. Measured by
//! `the_cost_of_following_an_injection` below, which is where the number
//! is kept honest.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::{Arc, LazyLock, Mutex};

use tree_sitter::{Query, QueryCursor, StreamingIterator};

use crate::grammars::Grammar;
use crate::highlight::{Paint, paint_of};

/// A run of one paint inside a line.
///
/// The runs of a line cover it from end to end: a stretch nothing claims
/// is a span with no paint, not a gap. A view walks them and needs no
/// arithmetic of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    /// Where it is **within its own line**, in bytes.
    pub at: Range<usize>,
    /// What it is painted with, or `None` for ordinary text.
    pub paint: Option<Paint>,
}

/// The spans of a text, one row per line.
///
/// An empty row means *draw this line plain*: it is what a text with no
/// grammar answers, and it lets a caller skip the work rather than walk
/// one span the width of the line.
pub fn spans(text: &str, grammar: &str) -> Vec<Vec<Span>> {
    let lines = line_ranges(text);
    let Some(painted) = whole(text, grammar) else {
        return vec![Vec::new(); lines.len()];
    };
    lines.iter().map(|line| cut(&painted, line)).collect()
}

/// How far an injection may reach.
///
/// HTML hands `<script>` to javascript and `<style>` to css, and neither
/// hands anything on; markdown hands its inline grammar one level; svelte
/// reaches typescript, which reaches nothing. Two is what the deepest
/// file here needs. The cap is not for them — it is because a query is
/// free to say that a grammar injects itself, and a text is not allowed
/// to take for ever because one does.
const DEPTH: usize = 3;

/// The spans of the whole text, in its own offsets, or `None` when there
/// is nothing to parse with.
fn whole(text: &str, grammar: &str) -> Option<Vec<Span>> {
    let grammar = crate::grammars::of(grammar)?;
    if grammar.highlights.is_empty() {
        return None;
    }
    // In the query's own order, which is what decides an overlap.
    let mut found: Vec<(Range<usize>, Option<Paint>)> = Vec::new();
    paint_with(text, &grammar, None, 0, &mut found);
    Some(flatten(0..text.len(), found))
}

/// One grammar's captures over `within` — the whole text when that is
/// `None` — and then the captures of everything it injects.
///
/// The injected ones are pushed **after**, so the rule that a later
/// capture covering the same stretch wins puts a `<style>` block's own
/// colours above whatever HTML said about the region.
///
/// **There is no byte shift.** `set_included_ranges` parses a region *in
/// place*: the parser is given the whole text and the ranges to look at,
/// and every node it produces carries an offset into the original. The
/// plan expected a slice and an addition, which is the form that goes
/// wrong quietly; this form has nothing to get wrong.
fn paint_with(
    text: &str,
    grammar: &Grammar,
    within: Option<&[tree_sitter::Range]>,
    depth: usize,
    found: &mut Vec<(Range<usize>, Option<Paint>)>,
) {
    let language = (grammar.language)();
    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }
    if let Some(ranges) = within
        && parser.set_included_ranges(ranges).is_err()
    {
        return;
    }
    let Some(tree) = parser.parse(text.as_bytes(), None) else {
        return;
    };

    if let Some(query) = compiled(grammar, &language, Which::Highlights) {
        let mut last: Option<Range<usize>> = None;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.captures(&query, tree.root_node(), text.as_bytes());
        while let Some((one, _)) = matches.next() {
            for capture in one.captures {
                let name = &query.capture_names()[capture.index as usize];
                let at = capture.node.byte_range();
                // **The same bytes twice is an annotation, not a second
                // colour.** The vendored queries mark a node for the spell
                // checker by capturing it again — `(comment) @comment
                // @spell` — and taking the later capture greys the
                // comment. The component's `match_styles` keeps the
                // previous name when the ranges are equal, and only
                // `unique_styles` after it lets a strictly-inner capture
                // win; this is the half of that rule the first copy
                // missed. `last` is this pass's own, so a region's first
                // capture cannot inherit from the wrapper above it.
                let paint = match (&last, found.last()) {
                    (Some(previous), Some((_, paint))) if *previous == at => *paint,
                    // **Asked with the grammar**, M8.10: a grammar may
                    // paint a capture its own way, and an injected
                    // region is asked with the grammar it was handed to.
                    _ => paint_of(grammar.name, name),
                };
                last = Some(at.clone());
                found.push((at, paint));
            }
        }
    }

    if depth + 1 >= DEPTH {
        return;
    }
    for (name, region) in injected(text, grammar, &language, &tree) {
        let Some(inner) = crate::grammars::of(name) else {
            continue;
        };
        if inner.highlights.is_empty() {
            continue;
        }
        paint_with(text, &inner, Some(&region), depth + 1, found);
    }
}

/// The regions this grammar hands to another, by the name it hands them
/// to.
///
/// **Only a static `#set! injection.language`.** A `@injection.language`
/// capture takes the name from a word in the file — svelte's `lang="ts"`,
/// php's heredoc tag — and the component resolves neither, so following
/// one here would colour a file in a tab and differently in a diff. That
/// is the disagreement this module exists to make impossible, and it is
/// worth more than the two cases.
///
/// A name is taken only if the grammar **declares** it in `injects`,
/// which is the same list the component resolves against, and the
/// declared name is what the region is parsed with — so a query naming a
/// grammar nobody registered contributes nothing rather than an error.
///
/// `#set! injection.combined` means the regions of a pattern are one
/// text: several `preBuild = '' … ''` blocks are one shell script, not
/// five. Without it each region is parsed on its own.
fn injected(
    text: &str,
    grammar: &Grammar,
    language: &tree_sitter::Language,
    tree: &tree_sitter::Tree,
) -> Vec<(&'static str, Vec<tree_sitter::Range>)> {
    if grammar.injections.is_empty() || grammar.injects.is_empty() {
        return Vec::new();
    }
    let Some(query) = compiled(grammar, language, Which::Injections) else {
        return Vec::new();
    };
    let content = query
        .capture_names()
        .iter()
        .position(|name| *name == "injection.content");
    let Some(content) = content else {
        return Vec::new();
    };

    let mut apart: Vec<(&'static str, Vec<tree_sitter::Range>)> = Vec::new();
    let mut combined: Vec<(&'static str, Vec<tree_sitter::Range>)> = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
    while let Some(one) = matches.next() {
        let settings = query.property_settings(one.pattern_index);
        let named = settings
            .iter()
            .find(|property| &*property.key == "injection.language")
            .and_then(|property| property.value.as_deref());
        let Some(named) = named else {
            continue;
        };
        let Some(name) = grammar.injects.iter().find(|known| **known == named) else {
            continue;
        };
        let is_combined = settings
            .iter()
            .any(|property| &*property.key == "injection.combined");
        let regions: Vec<tree_sitter::Range> = one
            .captures
            .iter()
            .filter(|capture| capture.index as usize == content)
            .map(|capture| capture.node.range())
            .collect();
        if regions.is_empty() {
            continue;
        }
        if is_combined {
            match combined.iter_mut().find(|(known, _)| known == name) {
                Some((_, gathered)) => gathered.extend(regions),
                None => combined.push((name, regions)),
            }
        } else {
            for region in regions {
                apart.push((name, vec![region]));
            }
        }
    }
    apart.extend(combined);
    apart
}

/// One paint per stretch, by the component's rule: the **last** capture
/// that covers a stretch wins it.
///
/// That is what puts a string's escape above the string it is inside,
/// and it is copied from `unique_styles` rather than reasoned out afresh.
///
/// **Written as a byte painted in order rather than as a piece asking
/// who covers it.** The first version cut the text at every capture
/// boundary and searched the captures backwards for each piece, which is
/// the rule stated literally and is `O(pieces × captures)`: on a
/// 3,000-line page it was **311 ms**, against 12 ms for the parse and the
/// query that produced the captures. Nothing showed it while a file was
/// four thousand captures; following injections made the same page
/// thirty-two thousand and the diff, which is rebuilt on every keystroke,
/// would have taken a third of a second per letter.
///
/// The two are the same answer. Every capture boundary is a cut, so a
/// capture either contains a piece whole or misses it entirely — *the
/// last capture covering the piece* and *the last capture covering each
/// of its bytes* cannot differ. Painting them in order and joining the
/// neighbours costs the bytes the captures span instead.
fn flatten(total: Range<usize>, found: Vec<(Range<usize>, Option<Paint>)>) -> Vec<Span> {
    if total.start >= total.end {
        return Vec::new();
    }
    let mut painted: Vec<Option<Paint>> = vec![None; total.end - total.start];
    for (at, paint) in found {
        let from = at.start.clamp(total.start, total.end) - total.start;
        let to = at.end.clamp(total.start, total.end) - total.start;
        if from < to {
            painted[from..to].fill(paint);
        }
    }

    let mut spans: Vec<Span> = Vec::new();
    for (offset, paint) in painted.into_iter().enumerate() {
        let at = total.start + offset;
        match spans.last_mut() {
            Some(last) if last.paint == paint => last.at.end = at + 1,
            _ => spans.push(Span {
                at: at..at + 1,
                paint,
            }),
        }
    }
    spans
}

/// The part of the text's spans that belongs to one line, in that line's
/// own offsets.
fn cut(painted: &[Span], line: &Range<usize>) -> Vec<Span> {
    painted
        .iter()
        .filter(|span| span.at.start < line.end && span.at.end > line.start)
        .map(|span| Span {
            at: span.at.start.max(line.start) - line.start..span.at.end.min(line.end) - line.start,
            paint: span.paint,
        })
        .filter(|span| span.at.start < span.at.end)
        .collect()
}

/// Where each line is in the text, its line break excluded. An empty text
/// has no lines; a text of one line break has one, and it is empty.
///
/// Public because a caller that keeps the spans usually keeps the text
/// beside them, and cutting it twice by two rules is how they come apart.
pub fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    let mut at = 0;
    for line in text.split_inclusive('\n') {
        let end = at + line.len() - usize::from(line.ends_with('\n'));
        lines.push(at..end);
        at += line.len();
    }
    lines
}

/// Which of a grammar's two queries is wanted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Which {
    /// What is painted, with the captures renamed on the way in.
    Highlights,
    /// What is handed to another grammar. **Not renamed**: its captures
    /// are `injection.content` and `injection.language`, which are not
    /// colours, and the component does not rename them either.
    Injections,
}

/// The queries compiled so far, by the grammar that owns each and which
/// of its two it is.
type Compiled = Mutex<HashMap<(&'static str, Which), Arc<Query>>>;

/// A query is compiled once per grammar and kept: rust's is a hundred
/// patterns, and compiling it per diff would cost more than the parse.
///
/// The cache is asked **before** the parts are joined and the captures
/// renamed, so neither costs anything after the first file of a kind.
fn compiled(
    grammar: &Grammar,
    language: &tree_sitter::Language,
    which: Which,
) -> Option<Arc<Query>> {
    static CACHE: LazyLock<Compiled> = LazyLock::new(|| Mutex::new(HashMap::new()));
    let mut cache = CACHE.lock().ok()?;
    if let Some(query) = cache.get(&(grammar.name, which)) {
        return Some(query.clone());
    }
    let source = match which {
        Which::Highlights => crate::highlight::query_for(grammar.name, &grammar.highlights()),
        Which::Injections => grammar.injections.to_owned(),
    };
    let query = Query::new(language, &source)
        .inspect_err(
            |err| tracing::warn!(grammar = grammar.name, ?which, %err, "the query did not compile"),
        )
        .ok()?;
    let query = Arc::new(query);
    cache.insert((grammar.name, which), query.clone());
    Some(query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::Role;

    /// A page of `lines` lines, most of it script and style, so that the
    /// injected halves are what the measurement is measuring.
    #[cfg(test)]
    fn page(lines: usize) -> String {
        let block = lines / 3;
        let mut text = String::from("<!DOCTYPE html>\n<html>\n<head>\n<style>\n");
        for n in 0..block {
            text.push_str(&format!(".row-{n} {{ color: #112233; margin: {n}px; }}\n"));
        }
        text.push_str("</style>\n</head>\n<body>\n<script>\n");
        for n in 0..block {
            text.push_str(&format!("const value{n} = compute({n}); // a comment\n"));
        }
        text.push_str("</script>\n");
        for n in 0..block {
            text.push_str(&format!("<p class=\"row-{n}\">line {n}</p>\n"));
        }
        text.push_str("</body>\n</html>\n");
        text
    }

    /// **What following an injection costs**, which the plan deferred to
    /// this branch to measure. The diff is rebuilt on every keystroke and
    /// that path already carries a parse, so the number matters.
    ///
    /// Ignored because it is a measurement: it says nothing about
    /// correctness, and a machine under load would fail it as a test.
    /// Run it by hand:
    ///
    /// ```text
    /// cargo test -p guild-desktop --release the_cost_of_following -- --ignored --nocapture
    /// ```
    ///
    /// **Measured on 2026-09-06**, release, on a 3,011-line page whose
    /// script and style blocks are two thirds of it:
    ///
    /// | | |
    /// |---|---|
    /// | the page, injections followed | 12.5 ms |
    /// | the same page, not followed | 3.4 ms |
    /// | of the first: parsing and querying | 12.2 ms |
    /// | of the first: flattening 32,032 captures | 0.2 ms |
    ///
    /// The nine milliseconds are not overhead — they are two thousand
    /// lines of css and javascript being coloured that were not coloured
    /// at all before, and HTML's own thirteen-line query over the same
    /// file is the 3.4 ms and was the *whole* of what a page cost in a
    /// diff. Beside M8.1.1's 9 ms for three thousand lines of Rust it is
    /// the same order, in the same path, for a great deal more colour.
    ///
    /// **The first version of this measurement said 341 ms**, and that
    /// is why `flatten` was rewritten — the note there has it.
    #[test]
    #[ignore = "a measurement, run by hand"]
    fn the_cost_of_following_an_injection() {
        let text = page(3_000);
        let grammar = crate::grammars::of("html").expect("html");
        let runs = 20;

        let mut followed = std::time::Duration::ZERO;
        let mut alone = std::time::Duration::ZERO;
        for _ in 0..runs {
            let at = std::time::Instant::now();
            let mut found = Vec::new();
            paint_with(&text, &grammar, None, 0, &mut found);
            let _ = flatten(0..text.len(), found);
            followed += at.elapsed();

            // The same call one step under the cap, which is the same
            // file with nothing followed.
            let at = std::time::Instant::now();
            let mut found = Vec::new();
            paint_with(&text, &grammar, None, DEPTH - 1, &mut found);
            let _ = flatten(0..text.len(), found);
            alone += at.elapsed();
        }
        let at = std::time::Instant::now();
        let mut found = Vec::new();
        paint_with(&text, &grammar, None, 0, &mut found);
        let captures = found.len();
        let parsing = at.elapsed();
        let at = std::time::Instant::now();
        let _ = flatten(0..text.len(), found);
        let flattening = at.elapsed();
        println!(
            "{} lines: injections followed {:?}, not followed {:?} \
             ({captures} captures: {parsing:?} parsing, {flattening:?} flattening)",
            text.lines().count(),
            followed / runs,
            alone / runs
        );
    }

    /// The rule for an overlap, in its own right: the **last** capture
    /// that covers a stretch wins it.
    ///
    /// Out here rather than in `tests/` because it is the private rule
    /// the two engines have to share, and a case where the two paints
    /// differ is the only way to see it — where they agree, as a string
    /// and its escape do, the runs merge and the win is invisible.
    #[test]
    fn the_last_capture_covering_a_stretch_wins_it() {
        let spans = flatten(
            0..10,
            vec![
                (0..10, Some(Paint::Role(Role::String))),
                (4..6, Some(Paint::Role(Role::Keyword))),
            ],
        );
        assert_eq!(
            spans,
            vec![
                Span {
                    at: 0..4,
                    paint: Some(Paint::Role(Role::String))
                },
                Span {
                    at: 4..6,
                    paint: Some(Paint::Role(Role::Keyword))
                },
                Span {
                    at: 6..10,
                    paint: Some(Paint::Role(Role::String))
                },
            ]
        );
    }

    /// Neighbours of one paint are one run, so a view draws the fewest
    /// pieces it can — and so a stretch nobody claims is a run too.
    #[test]
    fn neighbours_of_one_paint_are_one_run() {
        let spans = flatten(
            0..10,
            vec![
                (0..3, Some(Paint::Role(Role::Comment))),
                (3..6, Some(Paint::Role(Role::Comment))),
            ],
        );
        assert_eq!(
            spans,
            vec![
                Span {
                    at: 0..6,
                    paint: Some(Paint::Role(Role::Comment))
                },
                Span {
                    at: 6..10,
                    paint: None
                },
            ]
        );
    }
}
