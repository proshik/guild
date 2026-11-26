//! Staging one shown change: which index lines a write replaces, and with
//! what — pure, decision 55.
//!
//! **The unit is the block on screen, not git's context hunk.** A block is
//! one contiguous non-equal run of [`Document`]'s line diff — what the diff's
//! ribbon and an editor marker show. Two changes one line apart are one
//! `-U3` hunk to git and two blocks here, which is what keeps a staged change
//! the screen no longer shows from riding along with its neighbour (review
//! finding `lenses-3`).
//!
//! **No line numbers leave this module.** The write is the index entry's
//! whole new content, computed here from the exact preimage and swapped in
//! by git only over that preimage (`git::hunk`), so there is nothing for git
//! to place by context at the wrong copy of a repeated block (`lenses-4`).
//!
//! **Bytes are the file's own.** Lines are cut where [`Source`] cuts them and
//! carried with their terminators, so CRLF and a missing final newline are
//! what the file had. A lone carriage return, which [`Source`] counts as a
//! line break and an editor buffer does not, is offered no hunks at all.

use std::ops::Range;

use crate::diff::document::{Block, Document, Source};

/// Which comparison a block was shown in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// `HEAD` against the working text — the diff a Commit row opens, and an
    /// editor's markers.
    Head,
    /// The index against the working text.
    Unstaged,
    /// `HEAD` against the index.
    Staged,
}

/// What a write does to the index.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Puts the shown working lines into the index.
    Stage,
    /// Puts `HEAD`'s lines back into the index.
    Unstage,
}

/// What a shown block offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Offer {
    Stage,
    Unstage,
    /// Disabled: a staged change straddles the block's edge, so no part of
    /// the index belongs to the block alone. The Staged diff can do it.
    Wider,
    /// Disabled: part of the block is in the index and part is not — line
    /// endings included. Staging "the rest" could take out an index line the
    /// screen does not show; the Unstaged or Staged diff can do it.
    Mixed,
    /// Disabled: the block differs from the index only in line endings,
    /// which no diff shows as a change — the whole file is the way.
    LineEndings,
    /// Disabled: git would convert the bytes this write puts in the index —
    /// set by the caller, which knows the file's conversion
    /// ([`Comparisons::replacement`]). Review r3 `bugs+impl-5`.
    Converted,
    /// Nothing to do here: the block is not a change between the index and
    /// either side.
    Nothing,
}

impl Offer {
    /// The enabled action, if any.
    pub fn action(self) -> Option<Action> {
        match self {
            Offer::Stage => Some(Action::Stage),
            Offer::Unstage => Some(Action::Unstage),
            Offer::Wider
            | Offer::Mixed
            | Offer::LineEndings
            | Offer::Converted
            | Offer::Nothing => None,
        }
    }
}

/// The three texts of one file, and where `HEAD`'s lines are in the index.
pub struct Comparisons {
    head: Source,
    index: Source,
    working: Source,
    /// `HEAD` ↔ index blocks, in order.
    staged: Vec<Block>,
    /// `before[i]`: index lines minus `HEAD` lines over `staged[..i]`.
    before: Vec<isize>,
}

impl Comparisons {
    /// `None` when any of the texts has a lone carriage return.
    pub fn new(head: &str, index: &str, working: &str) -> Option<Self> {
        if [head, index, working].into_iter().any(lone_cr) {
            return None;
        }
        let document = Document::new(head, index);
        let staged = document.blocks;
        let mut before = Vec::with_capacity(staged.len() + 1);
        let mut sum = 0isize;
        before.push(0);
        for block in &staged {
            sum += block.new.len() as isize - block.old.len() as isize;
            before.push(sum);
        }
        let [head, index] = document.sources;
        Some(Self {
            head,
            index,
            working: Source::new(working),
            staged,
            before,
        })
    }

    /// What a block shown in `view` offers.
    pub fn offer(&self, view: View, shown: &Block) -> Offer {
        self.plan(view, shown).0
    }

    /// The index entry's new content for the block's action, when it is
    /// available.
    pub fn postimage(&self, view: View, shown: &Block) -> Option<(Action, String)> {
        let (offer, at, with) = self.plan(view, shown);
        let action = offer.action()?;
        let (at, with) = (at?, with?);
        let mut text = self.index.text.to_string();
        text.replace_range(at, with);
        Some((action, text))
    }

    /// The bytes an available offer puts in place of index bytes — what a
    /// caller checks against the file's conversion. The rest of the index is
    /// kept as it is.
    pub fn replacement(&self, view: View, shown: &Block) -> Option<&str> {
        let (offer, _, with) = self.plan(view, shown);
        offer.action()?;
        with
    }

    /// The offer, the index bytes it replaces and what with.
    ///
    /// **One rule maps a shown block to exactly its written bytes.** In the
    /// Unstaged and Staged diffs the block is already a change of the index.
    /// In the HEAD view the block's `HEAD` lines are found in the index
    /// through `HEAD` ↔ index — a staged insertion at the block's edge is
    /// outside it, a staged change across its edge makes it [`Offer::Wider`]
    /// — and the index bytes there decide, byte for byte: `HEAD`'s bytes
    /// are Stage, the working bytes are Unstage, anything else is
    /// [`Offer::Mixed`]. A write only ever puts one side's shown bytes in
    /// place of that region.
    fn plan(&self, view: View, shown: &Block) -> (Offer, Option<Range<usize>>, Option<&str>) {
        let (offer, at, with) = self.choose(view, shown);
        // **A write never joins its bytes to an index line it keeps**: an
        // unterminated replacement before a kept line, or new bytes after an
        // unterminated kept line, would make two lines one. Review r2.
        if let (Some(at), Some(with)) = (&at, with)
            && self.joins(at, with)
        {
            return (Offer::Mixed, None, None);
        }
        (offer, at, with)
    }

    /// Whether putting `with` in place of the index bytes `at` would run it
    /// into the kept bytes on either side.
    fn joins(&self, at: &Range<usize>, with: &str) -> bool {
        if with.is_empty() {
            return false;
        }
        let text = self.index.text.as_bytes();
        let before = &text[..at.start];
        let after = &text[at.end..];
        (!before.is_empty() && !before.ends_with(b"\n") && !before.ends_with(b"\r"))
            || (!after.is_empty() && !with.ends_with('\n') && !with.ends_with('\r'))
    }

    fn choose(&self, view: View, shown: &Block) -> (Offer, Option<Range<usize>>, Option<&str>) {
        match view {
            View::Unstaged => (
                Offer::Stage,
                Some(span(&self.index, &shown.old)),
                Some(&self.working.text[span(&self.working, &shown.new)]),
            ),
            View::Staged => (
                Offer::Unstage,
                Some(span(&self.index, &shown.new)),
                Some(&self.head.text[span(&self.head, &shown.old)]),
            ),
            View::Head => {
                let Some(rows) = self.index_rows(&shown.old) else {
                    return (Offer::Wider, None, None);
                };
                let at = span(&self.index, &rows);
                let index = &self.index.text[at.clone()];
                let head = &self.head.text[span(&self.head, &shown.old)];
                let working = &self.working.text[span(&self.working, &shown.new)];
                if index == working {
                    if index == head {
                        (Offer::Nothing, None, None)
                    } else {
                        (Offer::Unstage, Some(at), Some(head))
                    }
                } else if index == head {
                    (Offer::Stage, Some(at), Some(working))
                } else if index.replace("\r\n", "\n") == working.replace("\r\n", "\n") {
                    (Offer::LineEndings, None, None)
                } else {
                    (Offer::Mixed, None, None)
                }
            }
        }
    }

    /// The index rows of `HEAD`'s rows `old`: `None` when a staged change
    /// straddles either edge. A staged insertion exactly at an edge of a
    /// non-empty range is outside the rows; an empty range exactly at one is
    /// the insertion itself. Binary search over the ordered blocks.
    fn index_rows(&self, old: &Range<usize>) -> Option<Range<usize>> {
        let straddles = |at: usize, i: usize| {
            self.staged
                .get(i)
                .is_some_and(|b| b.old.start < at && at < b.old.end)
        };
        // The start: every block ending at or before it, insertions at it
        // included, lies before.
        let i = self.staged.partition_point(|b| b.old.end <= old.start);
        // The end: blocks ending before it, and non-empty ones ending at it;
        // an insertion exactly at the end lies after.
        let j = self.staged.partition_point(|b| {
            b.old.end < old.end || (b.old.end == old.end && !b.old.is_empty())
        });
        if straddles(old.start, i) || straddles(old.end, j) {
            return None;
        }
        // An empty range exactly at a staged insertion is that insertion:
        // the shown block is the inserted lines, wholly or partly staged.
        if old.is_empty() && i == j + 1 {
            return Some(self.staged[j].new.clone());
        }
        let start = old.start as isize + self.before[i];
        let end = old.end as isize + self.before[j];
        (start <= end).then_some(start as usize..end as usize)
    }
}
/// The bytes of `rows`, each line with its terminator.
fn span(source: &Source, rows: &Range<usize>) -> Range<usize> {
    let at = |row: usize| {
        source
            .lines
            .get(row)
            .map_or(source.text.len(), |line| line.start)
    };
    at(rows.start)..at(rows.end)
}

/// A carriage return not followed by a line feed.
fn lone_cr(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(at, byte)| *byte == b'\r' && bytes.get(at + 1) != Some(&b'\n'))
}
