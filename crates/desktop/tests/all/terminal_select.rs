//! A selection made by clicks in a terminal: a word, a line, an extension
//! and everything — M5.12, task 10.
//!
//! On a `Term` with no process behind it, fed by the parser a pty would
//! feed — a pty is a system-wide resource, and none of this needs one.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::SelectionType;
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use guild_desktop::terminal::emulator::GridSize;
use guild_desktop::terminal::select::{self, Press};

/// A terminal of `columns` by `rows` that has printed `text`, with a
/// history to scroll into.
fn term_with(columns: usize, rows: usize, text: &str) -> Term<VoidListener> {
    let mut term = Term::new(
        Config::default(),
        &GridSize::new(columns, rows),
        VoidListener,
    );
    let mut parser: Processor<StdSyncHandler> = Processor::new();
    parser.advance(&mut term, text.replace('\n', "\r\n").as_bytes());
    term
}

fn at(line: i32, column: usize) -> Point {
    Point::new(Line(line), Column(column))
}

#[test]
fn a_double_click_takes_the_word_and_a_triple_click_the_line() {
    let mut term = term_with(40, 4, "cargo check --workspace");

    // `check` runs from column 6 to 10; the click lands inside it.
    term.selection = Some(select::selection_at(
        &term,
        at(0, 8),
        Side::Left,
        SelectionType::Semantic,
    ));
    assert_eq!(term.selection_to_string().as_deref(), Some("check"));

    term.selection = Some(select::selection_at(
        &term,
        at(0, 8),
        Side::Left,
        SelectionType::Lines,
    ));
    assert_eq!(
        term.selection_to_string().as_deref(),
        Some("cargo check --workspace\n"),
        "a triple-click is the whole row"
    );
}

#[test]
fn a_drag_after_a_double_click_extends_by_whole_words() {
    let mut term = term_with(40, 4, "alpha beta gamma delta");

    let mut selection = select::selection_at(&term, at(0, 7), Side::Left, SelectionType::Semantic);
    // The middle of `gamma`, which runs from column 11 to 15.
    selection.update(at(0, 13), Side::Left);
    term.selection = Some(selection);

    assert_eq!(
        term.selection_to_string().as_deref(),
        Some("beta gamma"),
        "the drag takes the whole of the word it ends in, not part of it"
    );
}

#[test]
fn select_all_takes_the_history_and_the_screen() {
    let text = (1..=10)
        .map(|n| format!("line-{n}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut term = term_with(20, 3, &text);

    term.selection = Some(select::all(&term));
    let copied = term.selection_to_string().expect("something is selected");

    assert!(
        copied.starts_with("line-1\n"),
        "the oldest line of the history is where it begins: {copied:?}"
    );
    assert!(
        copied.contains("line-10"),
        "and the screen is where it ends: {copied:?}"
    );
}

#[test]
fn a_press_begins_or_extends() {
    use SelectionType::{Lines, Semantic, Simple};

    let table = [
        // click count, shift, a selection there, what the press does
        (1, false, false, Press::Begin(Simple)),
        (1, false, true, Press::Begin(Simple)),
        (2, false, false, Press::Begin(Semantic)),
        (2, false, true, Press::Begin(Semantic)),
        (3, false, false, Press::Begin(Lines)),
        (4, false, false, Press::Begin(Lines)),
        (1, true, true, Press::Extend),
        (2, true, true, Press::Extend),
        // Shift with nothing to extend begins an ordinary selection.
        (1, true, false, Press::Begin(Simple)),
    ];
    for (count, shift, has, expected) in table {
        assert_eq!(
            select::press(count, shift, has),
            expected,
            "click count {count}, shift {shift}, selection {has}"
        );
    }
}

/// A release copies only a selection its own press began or stretched.
///
/// A `cmd`-click on a link begins none, and before this its release copied
/// whatever was selected before — putting an old word back over what the
/// person had copied elsewhere since. Found by review of task 12.
#[test]
fn a_release_copies_only_what_its_own_press_selected() {
    let table = [
        // the press selected, copy on select, something selected, copied
        (true, true, true, true),
        (true, true, false, false),
        (true, false, true, false),
        // A `cmd`-click on a link, or a press past the grid.
        (false, true, true, false),
        (false, true, false, false),
    ];
    for (pressed, copy_on_select, selected, expected) in table {
        assert_eq!(
            select::release_copies(pressed, copy_on_select, || selected),
            expected,
            "pressed {pressed}, copy on select {copy_on_select}, selected {selected}"
        );
    }
}

/// **A move with the left button up ends a drag, over the grid as over the
/// find bar.** A release outside the window goes missing, and the terminal
/// trusted `selecting` alone: with the pointer back over the grid and no
/// button held, every move went on stretching the selection. The find bar
/// (task 9) already ended the drag on such a move and the grid did not —
/// one gesture decided two ways once groups C and D met. The trap *A drag's
/// release can go missing*.
#[test]
fn a_move_with_the_left_button_up_ends_a_drag_and_with_it_held_stretches_it() {
    use select::Drag;
    assert_eq!(select::drag(false, false), Drag::Idle);
    assert_eq!(
        select::drag(false, true),
        Drag::Idle,
        "a hover with a button held is no drag of ours"
    );
    assert_eq!(select::drag(true, true), Drag::Stretch);
    assert_eq!(select::drag(true, false), Drag::Release);
}

/// **Outside the terminal the window hears the drag** — 2026-09-23.
/// GPUI gives an element its moves only while the pointer is over it, so
/// a selection dragged above the grid went deaf and never scrolled.
/// Inside, the element's own handler hears it, and hearing it twice
/// would stretch it twice.
#[test]
fn a_drag_outside_the_terminal_is_heard_by_the_window_only_while_selecting() {
    assert!(select::heard_by_window(true, false));
    assert!(
        !select::heard_by_window(true, true),
        "the element hears it inside"
    );
    assert!(
        !select::heard_by_window(false, false),
        "no selection, nothing to stretch"
    );
}

/// **A program that reads the mouse gets the press** — 2026-09-24. A
/// press always began our own selection, so `claude`'s selecting, its
/// autoscroll and its *Jump to bottom (click)* never heard a thing.
/// Option keeps the press ours.
#[test]
fn a_press_goes_to_a_program_that_reads_the_mouse_unless_option_is_held() {
    use alacritty_terminal::term::TermMode;
    use guild_desktop::terminal::mouse::Button;
    use guild_desktop::terminal::select::{Grab, grab_for};
    let reading = TermMode::MOUSE_DRAG | TermMode::SGR_MOUSE;
    assert_eq!(
        grab_for(reading, false, Button::Left),
        Grab::Report(Button::Left)
    );
    assert_eq!(grab_for(reading, true, Button::Left), Grab::Select);
    assert_eq!(
        grab_for(TermMode::default(), false, Button::Left),
        Grab::Select
    );
}

/// **The edge clock scrolls only a history there is** — the alternate
/// screen has none, and the clock ticked at 12 Hz moving nothing.
#[test]
fn the_edge_scrolls_only_a_history() {
    use alacritty_terminal::term::TermMode;
    use guild_desktop::terminal::select::edge_scrolls;
    assert!(edge_scrolls(TermMode::default(), 500));
    assert!(!edge_scrolls(TermMode::ALT_SCREEN, 500));
    assert!(!edge_scrolls(TermMode::default(), 0));
}
