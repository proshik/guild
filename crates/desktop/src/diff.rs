//! The hunks between two texts, and the rows a list draws them as.
//!
//! Pure: two strings in, rows out. `similar` does the diff — patience,
//! three lines of context, the shape `git diff` prints — and this module
//! turns its changes into rows with line numbers on both sides, which is
//! all the view needs to know. Decision 2 and 6 of the M8.2 plan.

pub mod document;

use std::ops::Range;

#[cfg(test)]
thread_local! {
    pub(crate) static UI_COMPUTATIONS: std::cell::Cell<[usize; 2]> = const { std::cell::Cell::new([0, 0]) };
}
#[cfg(test)]
pub(crate) fn note_computation(stage: usize) {
    UI_COMPUTATIONS.with(|counts| {
        let mut value = counts.get();
        value[stage] += 1;
        counts.set(value);
    });
}

use similar::{Algorithm, ChangeTag, TextDiff};

/// How many lines of context stand on each side of a change: `git`'s
/// three.
pub const CONTEXT: usize = 3;

/// Past this a file's diff is not computed and the row says so: a diff
/// of four megabytes is not read, and computing it is a wait.
pub const DIFF_LIMIT: u64 = 4 * 1024 * 1024;

/// One row of a diff as the list draws it. Line numbers are one-based,
/// the way a gutter shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Row {
    /// `@@ -old_start,old_len +new_start,new_len @@`.
    Header {
        old_start: usize,
        old_len: usize,
        new_start: usize,
        new_len: usize,
    },
    /// A line both sides have.
    Context {
        old: usize,
        new: usize,
        text: String,
    },
    /// A line only the new side has.
    Added { new: usize, text: String },
    /// A line only the old side had.
    Removed { old: usize, text: String },
    /// The line above had no newline at its end — `git`'s
    /// `\ No newline at end of file`.
    NoNewline,
}

/// One hunk: a run of changes with their context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// The old side's lines, zero-based and half-open, context included.
    pub old: Range<usize>,
    /// The new side's.
    pub new: Range<usize>,
    /// The rows, header first.
    pub rows: Vec<Row>,
}

/// The hunks between `old` and `new`, by line, with [`CONTEXT`] lines on
/// each side. Line endings do not count as a change: both sides are
/// read with `\n` whatever they ended with on disk.
pub fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    hunks_with_deadline(
        old,
        new,
        std::time::Instant::now() + std::time::Duration::from_millis(150),
    )
}

pub(crate) fn hunks_with_deadline(old: &str, new: &str, deadline: std::time::Instant) -> Vec<Hunk> {
    let old = old.replace("\r\n", "\n");
    let new = new.replace("\r\n", "\n");
    let diff = TextDiff::configure()
        .algorithm(Algorithm::Patience)
        .deadline(deadline)
        .diff_lines(old.as_str(), new.as_str());

    diff.grouped_ops(CONTEXT)
        .into_iter()
        .map(|group| {
            let old_range = group.first().map(|op| op.old_range().start).unwrap_or(0)
                ..group.last().map(|op| op.old_range().end).unwrap_or(0);
            let new_range = group.first().map(|op| op.new_range().start).unwrap_or(0)
                ..group.last().map(|op| op.new_range().end).unwrap_or(0);
            let mut rows = vec![Row::Header {
                old_start: unified_start(&old_range),
                old_len: old_range.len(),
                new_start: unified_start(&new_range),
                new_len: new_range.len(),
            }];
            for op in &group {
                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches('\n').to_owned();
                    rows.push(match change.tag() {
                        ChangeTag::Equal => Row::Context {
                            old: change.old_index().map_or(0, |i| i + 1),
                            new: change.new_index().map_or(0, |i| i + 1),
                            text,
                        },
                        ChangeTag::Delete => Row::Removed {
                            old: change.old_index().map_or(0, |i| i + 1),
                            text,
                        },
                        ChangeTag::Insert => Row::Added {
                            new: change.new_index().map_or(0, |i| i + 1),
                            text,
                        },
                    });
                    if change.missing_newline() {
                        rows.push(Row::NoNewline);
                    }
                }
            }
            Hunk {
                old: old_range,
                new: new_range,
                rows,
            }
        })
        .collect()
}

/// The one-based start `git` prints for a range — and `0` for an empty
/// one, as `git` prints `-0,0` for a file that was empty.
fn unified_start(range: &Range<usize>) -> usize {
    if range.is_empty() {
        range.start
    } else {
        range.start + 1
    }
}

/// Every row of every hunk, in order.
pub fn rows(hunks: &[Hunk]) -> Vec<Row> {
    hunks
        .iter()
        .flat_map(|hunk| hunk.rows.iter().cloned())
        .collect()
}

/// How many lines were added and how many removed — the `+n −m` of a
/// file's header.
pub fn counts(hunks: &[Hunk]) -> (usize, usize) {
    hunks
        .iter()
        .flat_map(|hunk| hunk.rows.iter())
        .fold((0, 0), |(added, removed), row| match row {
            Row::Added { .. } => (added + 1, removed),
            Row::Removed { .. } => (added, removed + 1),
            _ => (added, removed),
        })
}

/// One row of a side-by-side diff: the two columns aligned the way every
/// side-by-side viewer draws them. Decision 11 of M8.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pair {
    /// A hunk's `@@` row, across both halves.
    Header {
        old_start: usize,
        old_len: usize,
        new_start: usize,
        new_len: usize,
    },
    /// The same line on both sides.
    Context {
        old: usize,
        new: usize,
        text: String,
    },
    /// A removed line on the left, an added line on the right, or one of
    /// them with the other half empty.
    Changed {
        old: Option<(usize, String)>,
        new: Option<(usize, String)>,
    },
    /// The line above had no newline at its end.
    NoNewline,
}

/// The pairs from the hunks' rows: a run of removed lines followed by a
/// run of added ones is zipped, first with first, and whatever is left
/// over sits alone with an empty half. A context line, a header or the
/// no-newline note ends a run, so the pairing never reaches across one.
pub fn sides(hunks: &[Hunk]) -> Vec<Pair> {
    let mut out = Vec::new();
    let mut removed: Vec<(usize, String)> = Vec::new();
    let mut added: Vec<(usize, String)> = Vec::new();
    let flush = |out: &mut Vec<Pair>, removed: &mut Vec<_>, added: &mut Vec<_>| {
        let mut olds = removed.drain(..);
        let mut news = added.drain(..);
        loop {
            match (olds.next(), news.next()) {
                (None, None) => break,
                (old, new) => out.push(Pair::Changed { old, new }),
            }
        }
    };
    for row in hunks.iter().flat_map(|hunk| &hunk.rows) {
        match row {
            Row::Removed { old, text } => removed.push((*old, text.clone())),
            Row::Added { new, text } => added.push((*new, text.clone())),
            other => {
                flush(&mut out, &mut removed, &mut added);
                out.push(match other {
                    Row::Header {
                        old_start,
                        old_len,
                        new_start,
                        new_len,
                    } => Pair::Header {
                        old_start: *old_start,
                        old_len: *old_len,
                        new_start: *new_start,
                        new_len: *new_len,
                    },
                    Row::Context { old, new, text } => Pair::Context {
                        old: *old,
                        new: *new,
                        text: text.clone(),
                    },
                    Row::NoNewline => Pair::NoNewline,
                    Row::Removed { .. } | Row::Added { .. } => unreachable!("handled above"),
                });
            }
        }
    }
    flush(&mut out, &mut removed, &mut added);
    out
}
