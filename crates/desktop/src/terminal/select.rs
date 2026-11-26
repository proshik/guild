//! A selection made by clicks — M5.12, task 10.
//!
//! A double-click is a word, a triple-click a line, a shift-click extends
//! what is selected, and Select All is the history and the screen — each of
//! them `alacritty_terminal`'s own selection kinds. Decision 13 of the plan.
//!
//! **What a word is, is `alacritty_terminal`'s too**: its
//! `SEMANTIC_ESCAPE_CHARS` stop a word at a space, a colon, a quote or a
//! bracket, so a double-click on `src/main.rs:12:5` takes `src/main.rs`.
//! A table of our own would be a second rule for one gesture, and the
//! paint reads the library's.

use alacritty_terminal::grid::Dimensions as _;
use alacritty_terminal::index::{Column, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Term, TermMode};

/// What a press of the left button does to the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Begin a new selection of this kind where the press landed.
    Begin(SelectionType),
    /// Stretch the selection there is to where the press landed.
    Extend,
}

/// What a press does, from the platform's click count, whether shift is
/// held and whether something is selected already.
///
/// **Shift with nothing to extend begins an ordinary selection** rather
/// than doing nothing: a shift-click that answers silence reads as a dead
/// button. A fourth click and beyond stay a line — macOS keeps counting
/// where `alacritty` cycles back, and a hand that clicks once too often
/// expects to keep what it had.
pub fn press(click_count: usize, shift: bool, has_selection: bool) -> Press {
    if shift && has_selection {
        return Press::Extend;
    }
    Press::Begin(match click_count {
        0 | 1 => SelectionType::Simple,
        2 => SelectionType::Semantic,
        _ => SelectionType::Lines,
    })
}

/// Whether a release copies under copy-on-select: only a selection its own
/// press began or stretched, and only one that holds something.
///
/// **A press that selected nothing copies nothing on its release** — a
/// `cmd`-click on a link, a press past the grid. Before, every left press
/// began a selection and the question never came up; once a link's press
/// began none, its release copied the selection left from before, over
/// whatever the person had copied elsewhere since. `selected` is asked last
/// because it builds the selection's text.
pub fn release_copies(
    pressed: bool,
    copy_on_select: bool,
    selected: impl FnOnce() -> bool,
) -> bool {
    pressed && copy_on_select && selected()
}

/// What a move of the pointer does to a selection being dragged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Drag {
    /// No drag is going on.
    Idle,
    /// The left button is held: the selection goes on stretching.
    Stretch,
    /// The left button is up and no release was heard: the drag is over.
    Release,
}

/// What a move does, from whether the terminal is selecting and whether
/// the platform says the left button is held.
///
/// **The move is believed over the release**: a release outside the window
/// goes missing, and every move carries the button that is down — the trap
/// *A drag's release can go missing*. A selection is the left button's, so
/// with it up, whatever else is held, the drag is over. One rule for the
/// grid and for the find bar that hides part of it.
pub fn drag(selecting: bool, left_held: bool) -> Drag {
    match (selecting, left_held) {
        (false, _) => Drag::Idle,
        (true, true) => Drag::Stretch,
        (true, false) => Drag::Release,
    }
}

/// What a held button is doing — 2026-09-24.
///
/// Until then a press was always our selection. A program that reads the
/// mouse — `claude` turns on 1000, 1002, 1003 and 1006 — selects,
/// autoscrolls and copies on its own, and our selection on its alternate
/// screen could reach no further than the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grab {
    /// Nothing is held.
    None,
    /// Our own selection is being made.
    Select,
    /// The program is being told about this button.
    Report(crate::terminal::mouse::Button),
}

/// Whose a press is: the program's when it reads the mouse and option is
/// not held, ours otherwise.
pub fn grab_for(mode: TermMode, bypass: bool, button: crate::terminal::mouse::Button) -> Grab {
    if crate::terminal::mouse::reported(mode, bypass) {
        Grab::Report(button)
    } else {
        Grab::Select
    }
}

/// Whether a selection held past the edge has a history to scroll into:
/// never on the alternate screen, which has none, and not before there
/// is any. The clock ticked there moving nothing.
pub fn edge_scrolls(mode: TermMode, history: usize) -> bool {
    !mode.contains(TermMode::ALT_SCREEN) && history > 0
}

/// Whether a move is the window's to pass on to a selection.
///
/// GPUI hands an element its moves only while the pointer is over it, so
/// a selection dragged above the grid went deaf: the edge clock never
/// started and the history never scrolled — 2026-09-23. Outside, the
/// window hears the move for it; inside, the element's own handler does,
/// and hearing it twice would stretch it twice.
pub fn heard_by_window(selecting: bool, inside: bool) -> bool {
    selecting && !inside
}

/// A selection of `kind` begun at a point, held inside the grid.
///
/// The kind decides how far it reaches when it is turned into a range — a
/// word, a line — so the point is only where the press landed.
pub fn selection_at<T>(term: &Term<T>, point: Point, side: Side, kind: SelectionType) -> Selection {
    let grid = term.grid();
    let line = point
        .line
        .max(grid.topmost_line())
        .min(grid.bottommost_line());
    let column = Column(point.column.0.min(grid.last_column().0));
    Selection::new(kind, Point::new(line, column), side)
}

/// Everything the terminal holds: the history from its oldest line and
/// the screen to its last.
pub fn all<T>(term: &Term<T>) -> Selection {
    let grid = term.grid();
    let mut selection = Selection::new(
        SelectionType::Lines,
        Point::new(grid.topmost_line(), Column(0)),
        Side::Left,
    );
    selection.update(
        Point::new(grid.bottommost_line(), grid.last_column()),
        Side::Right,
    );
    selection
}
