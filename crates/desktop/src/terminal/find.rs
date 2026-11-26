//! A word found in a terminal's history — M5.12, task 8.
//!
//! `alacritty_terminal`'s own `RegexSearch` over the grid the terminal
//! already holds, with case and pattern decided here rather than by its
//! smart case: a pure query-to-pattern function, the walk from the bottom
//! of what is shown, and the keys of the bar. Decision 9 of the plan.
//!
//! Global count and navigation snapshots live in `find_worker`; only visible
//! highlighting is searched on the live grid. Query text remains owned by Find.
//!
//! Query helpers take the `Term` they read. Snapshot navigation additionally
//! owns the grid guard so revision validation and viewport movement are atomic
//! with respect to the parser. Tests use terminals with no process.

use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Boundary, Column, Direction, Line, Point, Side};
use alacritty_terminal::term::Term;
use alacritty_terminal::term::search::{Match, RegexIter, RegexSearch};

/// What the bar is asked to find.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Query {
    /// What was typed.
    pub text: String,
    /// `Aa`: a letter matches only its own case.
    pub match_case: bool,
    /// `.*`: the text is a regular expression rather than itself.
    pub regex: bool,
}

/// `regex-syntax`'s metacharacters — its `is_meta_character`, which every
/// escaped one must be, or the parser refuses the escape.
const META: &str = r"\.+*?()|[]{}^$#&-~";

/// The pattern `RegexSearch` is built from.
///
/// **Case is said inline, always**, because `RegexSearch::new` decides it
/// by smart case — insensitive unless the pattern has an uppercase letter —
/// and the toggle has to win both ways. An inline flag does: task 0 found
/// `(?i)Needle` three times over `Needle needle NEEDLE` and `(?-i)needle`
/// once. **Regex off escapes every metacharacter of `regex-syntax`**, by
/// hand, so that nothing new enters the graph for a list of eighteen.
pub fn pattern(query: &Query) -> String {
    let flag = if query.match_case { "(?-i)" } else { "(?i)" };
    if query.regex {
        return format!("{flag}{}", query.text);
    }
    let mut escaped = String::with_capacity(flag.len() + query.text.len() * 2);
    escaped.push_str(flag);
    for c in query.text.chars() {
        if META.contains(c) {
            escaped.push('\\');
        }
        escaped.push(c);
    }
    escaped
}

/// The search for a query, or why the pattern will not compile — **in one
/// line**, because the bar has one line to say it in.
///
/// `BuildError` says only *error building NFA*; the reason is the parser's
/// error at the end of its sources, and that one is written over several
/// lines with the pattern and a caret under it. What is kept is its
/// `error:` sentence — *unclosed group* — and the pattern is left out,
/// because the one shown would carry the case flag the person never typed.
pub fn compile(query: &Query) -> Result<RegexSearch, String> {
    RegexSearch::new(&pattern(query)).map_err(|error| {
        let mut deepest: &dyn std::error::Error = error.as_ref();
        while let Some(source) = deepest.source() {
            deepest = source;
        }
        one_line(&deepest.to_string())
    })
}

/// A message of many lines as one: the `error:` line's sentence if there
/// is one, the last line with anything on it if not.
fn one_line(message: &str) -> String {
    let mut lines = message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    if let Some(sentence) = lines.clone().find_map(|line| line.strip_prefix("error:")) {
        return sentence.trim().to_owned();
    }
    lines.next_back().unwrap_or(message).to_owned()
}

/// The next match, going back (`older`) or down.
///
/// **With no `from`, from the bottom of what is shown** — the nearest match
/// above it when going back, because what a person looks for is what just
/// scrolled past; the first one below it when coming down. With a `from`,
/// the match beyond it, not overlapping it. Both **wrap at the ends**, as
/// `search_next` does: past the oldest line is the newest again.
pub fn next<T>(
    term: &Term<T>,
    search: &mut RegexSearch,
    from: Option<&Match>,
    older: bool,
) -> Option<Match> {
    let bottom = Point::new(
        Line(term.screen_lines() as i32 - 1 - term.grid().display_offset() as i32),
        term.last_column(),
    );
    if older {
        let origin = from.map_or(bottom, |from| from.start().sub(term, Boundary::None, 1));
        // `Side::Right`: a match counts as before the origin when its end
        // is, so the one just left is never found again.
        term.search_next(search, origin, Direction::Left, Side::Right, None)
    } else {
        let origin = from.map_or(bottom, |from| from.end().add(term, Boundary::None, 1));
        term.search_next(search, origin, Direction::Right, Side::Left, None)
    }
}

/// How far past the rows shown a line wrapped over them is followed for a
/// match — alacritty's own limit for the same walk. A program that wrote
/// ten thousand rows without a newline would otherwise make every frame
/// search all of them.
const WRAPPED_REACH: i32 = 100;

/// The matches on the rows shown, a match wrapped in from above or below
/// included.
pub fn visible<T>(term: &Term<T>, search: &mut RegexSearch) -> Vec<Match> {
    let offset = term.grid().display_offset() as i32;
    let top = Line(-offset);
    let bottom = Line(term.screen_lines() as i32 - 1 - offset);

    // Bound the wrap walk itself, rather than walking the whole logical line
    // under the live grid lock and only then clamping its answer.
    let mut start = Point::new(top, Column(0));
    while start.line > term.topmost_line()
        && top.0 - start.line.0 < WRAPPED_REACH
        && term.grid()[start.line - 1i32][term.last_column()]
            .flags
            .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
    {
        start.line -= 1;
    }
    let mut end = Point::new(bottom, term.last_column());
    while end.line.0 + 1 < term.screen_lines() as i32
        && end.line.0 - bottom.0 < WRAPPED_REACH
        && term.grid()[end.line][term.last_column()]
            .flags
            .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
    {
        end.line += 1;
    }

    RegexIter::new(start, end, Direction::Right, term, search)
        .filter(|found| found.end().line >= top && found.start().line <= bottom)
        .collect()
}

/// The display offset that puts `line` on screen: **unchanged when it is
/// already there**, and otherwise the one that sets it in the middle of the
/// screen, within the history there is.
///
/// The middle rather than an edge, because a match found at the top row
/// has none of what came before it in view, and that is most of what a
/// person reading a found word wants to see. `line` is the grid's: 0 is
/// the top of the screen at the bottom of the output, the history is
/// negative.
pub fn offset_showing(line: i32, screen_lines: usize, history: usize, offset: usize) -> usize {
    let line = i64::from(line);
    let top = -(offset as i64);
    let bottom = top + screen_lines as i64 - 1;
    if (top..=bottom).contains(&line) {
        return offset;
    }
    (screen_lines as i64 / 2 - line).clamp(0, history as i64) as usize
}

/// A key the bar reads itself rather than as typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarKey {
    /// `enter`: the next match further back.
    Older,
    /// `shift-enter`: the next match further down.
    Newer,
    /// `escape`: close the bar.
    Close,
}

/// What a keystroke in the bar is, when it is not typing: Enter,
/// Shift-Enter and Escape, with nothing else held. Everything else is
/// `desktop::edit`'s.
pub fn bar_key(stroke: &gpui::Keystroke) -> Option<BarKey> {
    let held = &stroke.modifiers;
    if held.platform || held.control || held.alt || held.function {
        return None;
    }
    match stroke.key.as_str() {
        "enter" if held.shift => Some(BarKey::Newer),
        "enter" => Some(BarKey::Older),
        "escape" if !held.shift => Some(BarKey::Close),
        _ => None,
    }
}

/// Apply snapshot coordinates only while holding the same lock as the writer.
pub(crate) fn navigate_snapshot<T: alacritty_terminal::event::EventListener>(
    shared: &std::sync::Mutex<Term<T>>,
    revision: &std::sync::atomic::AtomicU64,
    expected: u64,
    found: Option<&Match>,
) -> Result<bool, ()> {
    let mut term = shared.lock().map_err(|_| ())?;
    if revision.load(std::sync::atomic::Ordering::Acquire) != expected {
        return Err(());
    }
    let now = term.grid().display_offset();
    let offset = found.map_or(now, |found| {
        offset_showing(
            found.start().line.0,
            term.screen_lines(),
            term.history_size(),
            now,
        )
    });
    if offset != now {
        term.scroll_display(alacritty_terminal::grid::Scroll::Delta(
            offset as i32 - now as i32,
        ));
    }
    Ok(offset != now)
}

#[cfg(test)]
mod navigation_revision_tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    };
    #[test]
    fn output_between_initial_validation_and_navigation_does_not_move_viewport() {
        let mut term = Term::new(
            Default::default(),
            &crate::terminal::emulator::GridSize::new(20, 3),
            alacritty_terminal::event::VoidListener,
        );
        let mut parser = alacritty_terminal::vte::ansi::Processor::<
            alacritty_terminal::vte::ansi::StdSyncHandler,
        >::new();
        parser.advance(&mut term, b"one\r\ntwo\r\nthree\r\nfour\r\nfive");
        let shared = Mutex::new(term);
        let revision = AtomicU64::new(1);
        assert_eq!(revision.load(Ordering::Acquire), 1);
        // Deterministically insert the writer's entire critical section after
        // the UI's initial validation and before its navigation lock.
        {
            let mut guard = shared.lock().unwrap();
            parser.advance(&mut *guard, b"\r\nsix");
            revision.fetch_add(1, Ordering::Release);
        }
        let found = Point::new(Line(-2), Column(0))..=Point::new(Line(-2), Column(2));
        assert!(navigate_snapshot(&shared, &revision, 1, Some(&found)).is_err());
        assert_eq!(shared.lock().unwrap().grid().display_offset(), 0);
        assert!(navigate_snapshot(&shared, &revision, 2, Some(&found)).unwrap());
    }
}
