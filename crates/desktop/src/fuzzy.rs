//! A list found by its scattered letters — pure.
//!
//! **Every popup's query goes through here** (decision 5 of M8.9): Recent
//! Projects, Recent Files, the run list, the branch popup, the font list,
//! and the indexes of Search Everywhere. Before this each of them narrowed
//! by a substring of its own, and `gdsk` found nothing in
//! `guild/crates/desktop`.
//!
//! The matcher is `nucleo-matcher`, with three choices made here
//! once rather than at each caller:
//!
//! - **Case is ignored.** Smart case would make `ORCA` miss `orca`, and
//!   nobody holds shift to search.
//! - **The query is literal.** `Pattern::parse`'s fzf syntax would make `!`
//!   and `^` mean something in a field where a person types file names;
//!   `Pattern::new` splits on spaces and nothing else, and every word has
//!   to match.
//! - **A mark is a character**, never a byte: the crate is built without
//!   `unicode-segmentation`, which would make an index a byte offset for
//!   one path and a grapheme for the next. [`spans`] turns characters
//!   into the byte ranges a painter needs — the trap *a span is a byte
//!   range, and a run must start on a character boundary*.
//!
//! An empty query keeps each list's own order, because that order —
//! recency, git's, the recipe file's — is the answer to *nothing typed*.

use std::ops::Range;

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// What kind of text is being matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A path: a match that starts after a `/` scores above one that does
    /// not, which is what makes `srcrootrecent` find `src/root/recent.rs`.
    Path,
    /// A name with no separators of its own — a branch, a face, an action.
    Name,
}

/// A match: how good it is, and which characters it took.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    pub score: u32,
    /// `char` indices into the candidate, ascending and unique.
    pub marks: Vec<u32>,
}

fn config(shape: Shape) -> Config {
    match shape {
        Shape::Path => Config::DEFAULT.match_paths(),
        Shape::Name => Config::DEFAULT,
    }
}

fn pattern(query: &str) -> Pattern {
    Pattern::new(
        query,
        CaseMatching::Ignore,
        Normalization::Smart,
        AtomKind::Fuzzy,
    )
}

fn found_in(
    pattern: &Pattern,
    candidate: &str,
    matcher: &mut Matcher,
    buffer: &mut Vec<char>,
) -> Option<Found> {
    let mut marks = Vec::new();
    let score = pattern.indices(Utf32Str::new(candidate, buffer), matcher, &mut marks)?;
    // Each word appends its own positions, unsorted, and two words can
    // take the same letter.
    marks.sort_unstable();
    marks.dedup();
    Some(Found { score, marks })
}

/// Whether `query` finds `candidate`, and how.
///
/// A query of nothing but spaces finds everything with a score of zero
/// and no marks.
pub fn find(query: &str, candidate: &str, shape: Shape) -> Option<Found> {
    let mut matcher = Matcher::new(config(shape));
    found_in(&pattern(query), candidate, &mut matcher, &mut Vec::new())
}

/// The items `query` finds, best first, as their indices into `items`.
///
/// Ties keep the list's order — the sort is stable — and an empty query
/// answers every item in the list's own order. One matcher for the whole
/// call: it holds the scratch space a match needs.
pub fn rank<T>(
    query: &str,
    items: &[T],
    text: impl Fn(&T) -> &str,
    shape: Shape,
) -> Vec<(usize, Found)> {
    let pattern = pattern(query);
    let mut matcher = Matcher::new(config(shape));
    let mut buffer = Vec::new();
    let mut found: Vec<(usize, Found)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            found_in(&pattern, text(item), &mut matcher, &mut buffer).map(|found| (index, found))
        })
        .collect();
    if !pattern.atoms.is_empty() {
        found.sort_by_key(|(_, found)| std::cmp::Reverse(found.score));
    }
    found
}

/// The byte ranges of `text` that `marks` name, neighbours merged.
///
/// Every range starts and ends on a character boundary, so a painter can
/// slice with it. A mark past the end of the text is ignored rather than
/// followed — a row and its marks can come from two moments.
pub fn spans(text: &str, marks: &[u32]) -> Vec<Range<usize>> {
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut marks = marks.iter().copied().peekable();
    for (index, (byte, character)) in text.char_indices().enumerate() {
        let Some(&next) = marks.peek() else { break };
        if next as usize != index {
            continue;
        }
        marks.next();
        let end = byte + character.len_utf8();
        match ranges.last_mut() {
            Some(last) if last.end == byte => last.end = end,
            _ => ranges.push(byte..end),
        }
    }
    ranges
}
