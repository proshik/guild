//! The picker: what every popup's list is chosen with — pure.
//!
//! Decision 1 of M8.9. Recent Projects, Recent Files, the run list, the
//! branch popup and the font list each kept a query, a lit row, the
//! arrows, a settle and the pointer for themselves, and learned the same
//! rules five times over — two of them wrapping at the bottom and three
//! clamping. This is those rules once:
//!
//! - **A list with rows keeps one lit**, or Enter answers nothing.
//! - **The pointer moves the one highlight**; a hover tint beside it is a
//!   second highlight.
//! - **The arrows wrap** (decision 3). A popup of fifty rows has no end a
//!   person is heading for; the docked lists — the Log, the Commit panel,
//!   Search — keep `pick::step`, which clamps.
//! - **A heading is a row that is never lit** (decision 4): stepped over,
//!   settled below, deaf to the pointer.
//! - **A prefilled query is selected whole**, so the first letter
//!   replaces it — `edit::apply_selected`'s rule, M5.11's decision 5.
//!
//! What a popup draws, where it hangs and what taking a row does are the
//! window's, in `root::picker` and `view::picker`. What a key *means* is
//! here, in [`key`], so that `tab` cannot be a letter in one popup and a
//! tab switch in the next.

use gpui::Keystroke;

use crate::edit::{self, Edit};

/// What a row of a list is, as far as lighting it goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lit {
    /// Something to take.
    Row,
    /// A title over a section — drawn at a row's height, never lit.
    Heading,
}

/// The two ends of a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Jump {
    First,
    Last,
}

/// How a row was taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take {
    /// Return.
    Enter,
    /// `cmd-enter`: the row, somewhere other than where Return puts it —
    /// Find in Files' *Open in Find Window*.
    Aside,
    /// A click on the row.
    Click,
}

/// What a keystroke means to a popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    /// Escape.
    Close,
    /// Return, or `cmd-enter`.
    Take(Take),
    /// An arrow: one row, down for positive.
    Step(isize),
    /// A page key: one page, down for positive.
    Page(isize),
    /// `home` or `end`.
    Jump(Jump),
    /// Tab — the next face of a popup that has faces.
    NextTab,
    /// Shift-Tab.
    PreviousTab,
    /// `alt-up` and `alt-down`: through what was searched before.
    History(isize),
    /// The line being typed, by `desktop::edit`'s rules.
    Edit(Edit),
    /// Nothing a popup reads — a shortcut on its way somewhere else.
    Other,
}

/// What this keystroke means in any popup.
///
/// The keys a list reads come first and only unmodified — `cmd-down` is
/// not a step — and everything else is the line's, which is
/// `desktop::edit`: the shell's rules, and `cmd-v`.
pub fn key(stroke: &Keystroke) -> Key {
    let held = &stroke.modifiers;
    let shortcut = held.platform || held.control || held.function;

    match stroke.key.as_str() {
        "escape" if !shortcut => Key::Close,
        // `cmd-enter` and nothing held beside it: `cmd-shift-enter` is a
        // shortcut of its own, not a second spelling of *aside*.
        "enter" if held.platform && !held.control && !held.shift && !held.alt && !held.function => {
            Key::Take(Take::Aside)
        }
        "enter" if !shortcut => Key::Take(Take::Enter),
        "up" if held.alt && !shortcut => Key::History(-1),
        "down" if held.alt && !shortcut => Key::History(1),
        "up" if !shortcut => Key::Step(-1),
        "down" if !shortcut => Key::Step(1),
        "pageup" if !shortcut => Key::Page(-1),
        "pagedown" if !shortcut => Key::Page(1),
        "home" if !shortcut => Key::Jump(Jump::First),
        "end" if !shortcut => Key::Jump(Jump::Last),
        "tab" if held.shift && !shortcut => Key::PreviousTab,
        "tab" if !shortcut => Key::NextTab,
        _ => edit::edit(stroke).map_or(Key::Other, Key::Edit),
    }
}

/// How many rows a page key moves: the rows in view less one, so the row
/// at the bottom of one view is at the top of the next.
///
/// At least one, and five before there is anything to measure, which is
/// only true before the first frame.
pub fn page(view: f32, row: f32) -> isize {
    const UNMEASURED: isize = 5;
    if view <= 0.0 || row <= 0.0 {
        return UNMEASURED;
    }
    ((view / row).floor() as isize - 1).max(1)
}

/// A popup's list, as far as choosing from it goes.
#[derive(Debug, Default, Clone)]
pub struct Picker {
    query: String,
    /// The lit row, counted in the list as it is drawn — headings
    /// included.
    lit: Option<usize>,
    /// The query was put there, not typed, and is selected whole.
    prefilled: bool,
    /// The opening row has been applied. **Once**: reapplied on every
    /// keystroke it would fight the query, which Recent Files met.
    settled: bool,
}

impl Picker {
    /// What has been typed.
    pub fn query(&self) -> &str {
        &self.query
    }

    /// The lit row, if there is one.
    pub fn lit(&self) -> Option<usize> {
        self.lit
    }

    /// Put a query in the field, selected whole: the first letter typed
    /// replaces it and every rub empties it.
    pub fn prefill(&mut self, text: String) {
        self.prefilled = !text.is_empty();
        self.query = text;
        self.lit = None;
    }

    /// A key typed into the field.
    ///
    /// **What was lit was lit in the old list**, so nothing is lit after
    /// an edit until [`Self::settle`] lights the first row of the new
    /// one. An unresolved paste changes nothing — the caller resolves it
    /// against the clipboard with `edit::pasted` first.
    pub fn edit(&mut self, change: Edit) {
        if change == Edit::Paste {
            return;
        }
        if std::mem::take(&mut self.prefilled) {
            edit::apply_selected(&mut self.query, change);
        } else {
            edit::apply(&mut self.query, change);
        }
        self.lit = None;
    }

    pub fn set_query(&mut self, query: String) {
        self.query = query;
        self.prefilled = false;
        self.lit = None;
    }
    pub fn is_prefilled(&self) -> bool {
        self.prefilled
    }

    /// Move the highlight by `by` rows, headings stepped over.
    ///
    /// **It wraps from an end, and a longer move stops at one.** One row
    /// below the last is the first; a page down from near the bottom is
    /// the bottom, and the next page down is the top. From nothing, down
    /// lights the first row and up the last.
    pub fn step(&mut self, by: isize, rows: &[Lit]) {
        let selectable = selectable(rows);
        let Some(last) = selectable.len().checked_sub(1) else {
            self.lit = None;
            return;
        };
        let at = self
            .lit
            .and_then(|lit| nearest(rows, lit))
            .and_then(|lit| selectable.iter().position(|row| *row == lit));
        let to = match at {
            None if by < 0 => last,
            None => 0,
            Some(at) if by > 0 && at == last => 0,
            Some(at) if by < 0 && at == 0 => last,
            Some(at) => (at as isize + by).clamp(0, last as isize) as usize,
        };
        self.lit = Some(selectable[to]);
    }

    /// Light the first or the last row.
    pub fn jump(&mut self, to: Jump, rows: &[Lit]) {
        let selectable = selectable(rows);
        self.lit = match to {
            Jump::First => selectable.first().copied(),
            Jump::Last => selectable.last().copied(),
        };
    }

    /// Put the highlight where the list can carry it, after whatever just
    /// changed the list.
    ///
    /// The first time there are rows, on `opening` — a popup's own rule,
    /// or the first row — and never again. After that the lit row while it
    /// is still a row, the nearest row when it is not, and the first row
    /// when nothing is lit; nothing when there are no rows at all.
    pub fn settle(&mut self, rows: &[Lit], opening: usize) {
        let Some(first) = rows.iter().position(|row| *row == Lit::Row) else {
            self.lit = None;
            return;
        };
        if !self.settled {
            self.settled = true;
            self.lit = nearest(rows, opening);
            return;
        }
        self.lit = Some(self.lit.and_then(|lit| nearest(rows, lit)).unwrap_or(first));
    }

    /// The pointer moved over a row: that row is lit now.
    ///
    /// Answers whether anything changed — the pointer crosses a row many
    /// times a second, and a redraw for a highlight already in place is
    /// work nobody asked for. A row past the end is the last row: the
    /// list can narrow under a pointer that has not moved. A heading
    /// changes nothing.
    pub fn point_at(&mut self, row: usize, rows: &[Lit]) -> bool {
        let Some(last) = rows.len().checked_sub(1) else {
            return false;
        };
        let row = row.min(last);
        if rows[row] == Lit::Heading || self.lit == Some(row) {
            return false;
        }
        self.lit = Some(row);
        true
    }
}

/// The indices of the rows that can be lit.
fn selectable(rows: &[Lit]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| **row == Lit::Row)
        .map(|(index, _)| index)
        .collect()
}

/// The row that stands for `at`: itself when it is a row, the next row
/// after it when it is a heading, the one before it when nothing comes
/// after — and the last row for an index past the end.
fn nearest(rows: &[Lit], at: usize) -> Option<usize> {
    let last = rows.len().checked_sub(1)?;
    let at = at.min(last);
    (at..rows.len())
        .find(|index| rows[*index] == Lit::Row)
        .or_else(|| (0..at).rev().find(|index| rows[*index] == Lit::Row))
}
