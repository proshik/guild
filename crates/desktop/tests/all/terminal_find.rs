//! A word found in a terminal's history, case and pattern as asked —
//! M5.12, task 8.
//!
//! Every test here is a terminal with no process: a `Term` fed through
//! `vte`'s `Processor`, which is what the pty's reader does with a
//! program's output. A pty is a system-wide resource and none of these
//! rules needs one.

use alacritty_terminal::event::VoidListener;
use alacritty_terminal::grid::{Dimensions as _, Scroll};
use alacritty_terminal::index::{Column, Line, Point};
use alacritty_terminal::term::{Config, Term};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use guild_desktop::terminal::emulator::GridSize;
use guild_desktop::terminal::find::{self, BarKey, Query};

/// A terminal of `columns` by `lines` with a history, and `output` written
/// into it as a program would write it.
fn terminal(columns: usize, lines: usize, output: &str) -> Term<VoidListener> {
    let config = Config {
        scrolling_history: 1000,
        ..Config::default()
    };
    let mut term = Term::new(config, &GridSize::new(columns, lines), VoidListener);
    let mut parser: Processor<StdSyncHandler> = Processor::new();
    parser.advance(&mut term, output.as_bytes());
    term
}

fn query(text: &str, match_case: bool, regex: bool) -> Query {
    Query {
        text: text.to_owned(),
        match_case,
        regex,
    }
}

/// How many matches are on the rows shown.
fn shown(term: &Term<VoidListener>, query: &Query) -> usize {
    let mut search = find::compile(query).expect("the query compiles");
    find::visible(term, &mut search).len()
}

/// **What a person searches for is what just scrolled past**, so the first
/// match is the nearest one above the bottom of what is shown — not the
/// oldest in the history — and *next* goes further back, wrapping at the
/// top to the newest again. Scrolled up, "the bottom" is the bottom of the
/// view, not of the output.
#[test]
fn the_nearest_match_above_the_bottom_is_first_and_next_goes_back() {
    // Eight lines on a five-line screen: the last written is on line 3,
    // the cursor's empty line is 4, and lines 0 to 3 of the output are in
    // the history as lines -4 to -1.
    let output =
        "zero\r\none needle\r\ntwo\r\nthree\r\nfour needle\r\nfive\r\nsix needle\r\nseven\r\n";
    let mut term = terminal(20, 5, output);
    let mut search = find::compile(&query("needle", false, false)).expect("compiles");

    let first = find::next(&term, &mut search, None, true).expect("a first match");
    assert_eq!(
        *first.start(),
        Point::new(Line(2), Column(4)),
        "six, the nearest"
    );
    let second = find::next(&term, &mut search, Some(&first), true).expect("a second");
    assert_eq!(second.start().line, Line(0), "four, further back");
    let third = find::next(&term, &mut search, Some(&second), true).expect("a third");
    assert_eq!(third.start().line, Line(-3), "one, in the history");
    let wrapped = find::next(&term, &mut search, Some(&third), true).expect("wraps");
    assert_eq!(wrapped.start().line, Line(2), "back to the newest");

    // And down again the other way.
    let newer = find::next(&term, &mut search, Some(&third), false).expect("newer");
    assert_eq!(newer.start().line, Line(0));

    // Scrolled three rows up the view ends at line 1, so six is below what
    // is shown and four is the nearest above its bottom.
    term.scroll_display(Scroll::Delta(3));
    assert_eq!(term.grid().display_offset(), 3);
    let from_view = find::next(&term, &mut search, None, true).expect("from the view");
    assert_eq!(from_view.start().line, Line(0));

    // **Coming down with nothing found yet starts at the bottom of the view
    // too**: scrolled up, the first match below it; at the bottom of the
    // output there is none below, and the walk wraps to the oldest.
    let below = find::next(&term, &mut search, None, false).expect("below the view");
    assert_eq!(below.start().line, Line(2), "six, just under the view");
    term.scroll_display(Scroll::Bottom);
    let wrapped_down = find::next(&term, &mut search, None, false).expect("wraps down");
    assert_eq!(wrapped_down.start().line, Line(-3), "one, the oldest");
}

/// **Case is the toggle's, not `RegexSearch`'s smart case.** Smart case
/// alone would make a lowercase query insensitive and an uppercase one
/// sensitive, so each is checked against the toggle in the direction smart
/// case would not go.
#[test]
fn match_case_off_finds_either_case_and_on_finds_only_its_own() {
    let term = terminal(40, 3, "Needle needle NEEDLE");

    assert_eq!(shown(&term, &query("needle", false, false)), 3);
    assert_eq!(shown(&term, &query("needle", true, false)), 1);
    assert_eq!(
        shown(&term, &query("Needle", false, false)),
        3,
        "an uppercase letter does not turn case on"
    );
    assert_eq!(shown(&term, &query("Needle", true, false)), 1);
    assert_eq!(shown(&term, &query("NEEDLE", true, true)), 1);
}

/// With regex off a query is the text it is: `a.b` is a dot, and every
/// other metacharacter is itself too.
#[test]
fn regex_off_takes_the_query_as_it_is_written() {
    let term = terminal(40, 3, "axb a.b (x|y)* [z]");

    assert_eq!(shown(&term, &query("a.b", false, false)), 1);
    assert_eq!(shown(&term, &query("a.b", false, true)), 2);
    assert_eq!(shown(&term, &query("(x|y)*", false, false)), 1);
    assert_eq!(shown(&term, &query("[z]", false, false)), 1);
    assert_eq!(
        find::pattern(&query(r"a\.b+?^$#&-~{}", true, false)),
        r"(?-i)a\\\.b\+\?\^\$\#\&\-\~\{\}"
    );
    assert_eq!(find::pattern(&query("a.b", false, true)), "(?i)a.b");
}

/// A pattern that will not compile says why in one line — the bar has one
/// line to say it in — and the same text with regex off is fine.
#[test]
fn a_pattern_that_will_not_compile_is_one_line() {
    let Err(error) = find::compile(&query("a(", false, true)) else {
        panic!("an unclosed group compiles");
    };
    assert!(!error.is_empty());
    assert!(!error.contains('\n'), "more than one line: {error:?}");
    assert!(error.contains("unclosed"), "does not say why: {error:?}");

    assert!(find::compile(&query("a(", false, false)).is_ok());
}

/// A word the terminal wrapped over the edge is one word to a person, and
/// one match to the find.
#[test]
fn a_match_across_a_wrapped_line_is_one_match() {
    let term = terminal(10, 4, "xxxxxxxneedle");
    let mut search = find::compile(&query("needle", false, false)).expect("compiles");

    let matches = find::visible(&term, &mut search);
    assert_eq!(matches.len(), 1, "{matches:?}");
    assert_eq!(*matches[0].start(), Point::new(Line(0), Column(7)));
    assert_eq!(*matches[0].end(), Point::new(Line(1), Column(2)));
}

/// **A match wrapped in from above the top row or from below the bottom
/// row is on screen**, because part of it is: `visible` follows the
/// wrapped line past the edge of the view. The line here starts a hundred
/// and fifty rows above the word, past `WRAPPED_REACH`, so the walk begins
/// at its reach rather than at the line's start — and still finds it.
///
/// A pin: `visible` was written this way in the same commit as the first
/// wrapped-line test, which a search of the screen's rows alone would also
/// have passed, since that word lay entirely on them.
#[test]
fn a_match_wrapped_in_from_above_or_below_the_view_is_shown() {
    // Ten columns: 1507 `x`s put the word at column 7 of the 151st row of
    // one wrapped line, so it runs over into the next row. Ten short lines
    // after it keep it in the history.
    let mut output = "x".repeat(1507);
    output.push_str("needle\r\n");
    for n in 0..10 {
        output.push_str(&format!("tail {n}\r\n"));
    }
    let mut term = terminal(10, 4, &output);
    let mut search = find::compile(&query("needle", false, false)).expect("compiles");
    let word = find::next(&term, &mut search, None, true).expect("the word");
    let (start, end) = (word.start().line.0, word.end().line.0);
    assert_eq!(end, start + 1, "wraps: {word:?}");
    assert_eq!(word.start().column, Column(7));

    // Its tail on the top row, its head above it.
    let offset = |top: i32| usize::try_from(-top).expect("in the history");
    term.scroll_display(Scroll::Delta(offset(end) as i32));
    assert_eq!(-(term.grid().display_offset() as i32), end);
    assert_eq!(find::visible(&term, &mut search), vec![word.clone()]);

    // Its head on the bottom row, its tail below it.
    term.scroll_display(Scroll::Bottom);
    let bottom_on_start = term.screen_lines() as i32 - 1 - start;
    term.scroll_display(Scroll::Delta(bottom_on_start));
    assert_eq!(term.grid().display_offset() as i32, bottom_on_start);
    assert_eq!(find::visible(&term, &mut search), vec![word.clone()]);

    // One row further up the word is below the screen and not shown.
    term.scroll_display(Scroll::Delta(1));
    assert_eq!(find::visible(&term, &mut search), vec![]);
}

/// A match off the screen is brought onto it; one already there is not
/// moved to, because a view that jumps under a match the person can see
/// loses their place for nothing.
#[test]
fn a_match_far_up_is_scrolled_into_view_and_one_on_screen_is_not() {
    let shows = |line: i32, screen: usize, offset: usize| {
        let top = -(offset as i32);
        top <= line && line < top + screen as i32
    };

    // On screen at the bottom of the output, and on screen scrolled up.
    assert_eq!(find::offset_showing(5, 10, 100, 0), 0);
    assert_eq!(find::offset_showing(-15, 10, 100, 20), 20);

    for (line, offset) in [(-50, 0), (-98, 0), (-100, 0), (3, 20), (9, 50), (-1, 0)] {
        let moved = find::offset_showing(line, 10, 100, offset);
        assert!(moved <= 100, "past the history: {moved}");
        assert!(shows(line, 10, moved), "line {line} from {offset}: {moved}");
    }

    // And against a real grid: a match in the history is found and shown.
    let mut output = String::from("needle\r\n");
    for n in 0..60 {
        output.push_str(&format!("line {n}\r\n"));
    }
    let mut term = terminal(20, 10, &output);
    let mut search = find::compile(&query("needle", false, false)).expect("compiles");
    let found = find::next(&term, &mut search, None, true).expect("found far up");
    let offset = find::offset_showing(
        found.start().line.0,
        term.screen_lines(),
        term.history_size(),
        term.grid().display_offset(),
    );
    term.scroll_display(Scroll::Delta(offset as i32));
    assert_eq!(term.grid().display_offset(), offset);
    assert_eq!(find::visible(&term, &mut search).len(), 1);
}

/// Enter goes back, Shift-Enter comes down, Escape closes; everything else
/// is `desktop::edit`'s, typing included.
#[test]
fn the_bar_s_keys() {
    let key = |stroke: &str| find::bar_key(&gpui::Keystroke::parse(stroke).expect("parses"));

    assert_eq!(key("enter"), Some(BarKey::Older));
    assert_eq!(key("shift-enter"), Some(BarKey::Newer));
    assert_eq!(key("escape"), Some(BarKey::Close));
    assert_eq!(key("a"), None);
    assert_eq!(key("backspace"), None);
    assert_eq!(key("cmd-enter"), None);
}

/// **A terminal is told the window's language, through its look**, and the
/// find bar speaks it. Until the merge a `TerminalTab` knew no language and
/// the bar read it off the window's root by downcasting its view — group
/// C's workaround for a type that was group B's. `Look::of` is the one
/// constructor every terminal of the window is opened through, so the
/// language travels where the theme and the font already did.
#[test]
fn a_terminal_s_look_carries_the_window_s_language() {
    use guild_desktop::i18n::{Language, Text};
    use guild_desktop::terminal::view::Look;

    let settings = guild_session::Settings {
        language: Language::Russian,
        ..guild_session::Settings::default()
    };
    let look = Look::of(&settings, guild_desktop::theme::in_effect(&settings, true));
    assert_eq!(look.ui.0, Language::Russian);
    assert_eq!(look.ui.text(Text::FindPlaceholder), "Найти в выводе");
}
