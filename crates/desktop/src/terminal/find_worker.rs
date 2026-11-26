//! Snapshot extraction is bounded under the live mutex; regex traversal owns an
//! isolated grid. No terminal reader or UI callback searches retained history.
use super::{emulator::GridSize, find};
use alacritty_terminal::{
    event::{Event, EventListener},
    grid::Dimensions,
    index::{Column, Direction, Line, Point},
    term::{
        Config, Term,
        search::{Match, RegexIter},
    },
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
struct Silent;
impl EventListener for Silent {
    fn send_event(&self, _: Event) {}
}
pub(super) struct Results {
    pub matches: Vec<Match>,
    pub revision: u64,
}
pub(super) fn search<T: EventListener>(
    live: Arc<std::sync::Mutex<Term<T>>>,
    revision: Arc<AtomicU64>,
    cancel: Arc<AtomicBool>,
    query: find::Query,
) -> Option<Results> {
    let expected = revision.load(Ordering::Acquire);
    let stale = || cancel.load(Ordering::Acquire) || revision.load(Ordering::Acquire) != expected;
    if stale() {
        return None;
    }
    let (history, lines, columns) = {
        let term = live.lock().ok()?;
        (term.history_size(), term.screen_lines(), term.columns())
    };
    let mut rows = Vec::with_capacity(history + lines);
    let mut from = -(history as i32);
    while from < lines as i32 {
        if stale() {
            return None;
        }
        let to = (from + 8).min(lines as i32);
        {
            let term = live.lock().ok()?;
            if stale()
                || term.history_size() != history
                || term.screen_lines() != lines
                || term.columns() != columns
            {
                return None;
            }
            for line in from..to {
                rows.push(term.grid()[Line(line)].clone());
            }
        }
        from = to;
        std::thread::yield_now();
    }
    if stale() {
        return None;
    }
    let mut term = Term::new(
        Config {
            scrolling_history: history,
            ..Default::default()
        },
        &GridSize::new(columns, lines),
        Silent,
    );
    term.grid_mut().initialize_all();
    for (index, row) in rows.into_iter().enumerate() {
        term.grid_mut()[Line(index as i32 - history as i32)] = row;
    }
    let mut regex = find::compile(&query).ok()?;
    let mut matches = Vec::new();
    // A hard line ends the terminal regex input. Keep wrapped cells together
    // and check cancellation between logical lines, including no-match scans.
    let mut line = -(history as i32);
    while line < lines as i32 {
        if stale() {
            return None;
        }
        let start = line;
        while line < lines as i32 - 1
            && term.grid()[Line(line)][Column(columns - 1)]
                .flags
                .contains(alacritty_terminal::term::cell::Flags::WRAPLINE)
        {
            if stale() {
                return None;
            }
            line += 1;
        }
        for found in RegexIter::new(
            Point::new(Line(start), Column(0)),
            Point::new(Line(line), Column(columns - 1)),
            Direction::Right,
            &term,
            &mut regex,
        ) {
            if stale() {
                return None;
            }
            matches.push(found);
        }
        line += 1;
    }
    (!stale()).then_some(Results {
        matches,
        revision: expected,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use alacritty_terminal::{
        event::VoidListener,
        vte::ansi::{Processor, StdSyncHandler},
    };
    #[test]
    fn snapshot_search_keeps_wrapped_unicode_regex_and_case() {
        let mut term = Term::new(
            Config {
                scrolling_history: 100,
                ..Default::default()
            },
            &GridSize::new(8, 3),
            VoidListener,
        );
        let mut parser: Processor<StdSyncHandler> = Processor::new();
        parser.advance(
            &mut term,
            "first\r\n漢字needle\r\nNEEDLE\r\nlast needle".as_bytes(),
        );
        let live = Arc::new(std::sync::Mutex::new(term));
        let revision = Arc::new(AtomicU64::new(7));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = search(
            live.clone(),
            revision.clone(),
            cancel.clone(),
            find::Query {
                text: "needle".into(),
                regex: false,
                match_case: false,
            },
        )
        .unwrap();
        assert_eq!(result.matches.len(), 3);
        assert_eq!(result.revision, 7);
        let result = search(
            live.clone(),
            revision.clone(),
            cancel.clone(),
            find::Query {
                text: "n.*dle".into(),
                regex: true,
                match_case: true,
            },
        )
        .unwrap();
        assert_eq!(result.matches.len(), 2);
        cancel.store(true, Ordering::Release);
        assert!(
            search(
                live,
                revision,
                cancel,
                find::Query {
                    text: "needle".into(),
                    ..Default::default()
                }
            )
            .is_none()
        );
    }
}
