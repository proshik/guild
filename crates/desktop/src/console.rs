//! What the window told git, and what git said back.
//!
//! A Git ▸ Console, and the reason for it: Guild has
//! written to repositories since M8.3 and has never shown the commands. A
//! person who has just pressed *Switch* cannot see what ran, and can read
//! git's own words only in a notice that goes away when clicked.
//!
//! **The writes, not every command** — decision 1 of M8.3.4. The window
//! runs `git status` on every batch of file events and `git log` on every
//! change under `.git`; a console carrying those is a console nobody
//! reads. What is recorded is the eight variants of [`crate::git::Write`],
//! which is exactly *what a button did*.
//!
//! Pure: a line is text and a time, and the buffer is a queue with a lid.
//! Nothing here knows about a window.

use std::collections::VecDeque;

/// How many lines are kept.
///
/// Two hundred writes is more than a person makes in a week of work, and
/// a console that grew without a lid would hold a session's whole history
/// in memory for the sake of a scroll nobody reaches the top of.
pub const LINES: usize = 200;

/// One command, and what git said about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Milliseconds since the epoch — the clock, not a duration.
    pub at: i64,
    /// Where it ran, which is the project's root.
    pub root: String,
    /// The words git was given, `git` itself not among them.
    pub argv: Vec<String>,
    /// What git printed, both channels, as it printed it.
    ///
    /// **stderr included, and on success too.** *Switched to branch
    /// 'main'* is stderr, and a console that dropped it would show a
    /// command with no answer under it.
    pub said: String,
    /// Whether git refused.
    pub failed: bool,
}

impl Line {
    /// The heading: the time, the directory, and the command.
    ///
    /// `13:14:21.049: [/path/to/project] git switch main` — down to the
    /// milliseconds, which are what tell two runs of the same
    /// command apart.
    pub fn heading(&self, offset: i64) -> String {
        let local = (self.at + offset * 1000).rem_euclid(86_400_000);
        let (h, m, s, ms) = (
            local / 3_600_000,
            (local / 60_000) % 60,
            (local / 1000) % 60,
            local % 1000,
        );
        format!(
            "{h:02}:{m:02}:{s:02}.{ms:03}: [{}] git {}",
            self.root,
            self.argv.join(" ")
        )
    }
}

/// The lines, oldest first.
#[derive(Debug, Default)]
pub struct Console {
    lines: VecDeque<Line>,
}

impl Console {
    /// Record one, dropping the oldest when the lid is reached.
    pub fn push(&mut self, line: Line) {
        if self.lines.len() == LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(line);
    }

    /// Oldest first, which is the order they are read in: a console is
    /// read downwards and the newest is at the bottom, where the eye
    /// already is.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// How many there are — what the view scrolls to the end of.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Everything in the Console as one text, oldest first — what the
    /// read-only editor shows, so that a selection can cross lines.
    /// 2026-09-23: the lines were a `div` each and nothing could be
    /// copied off them.
    pub fn text(&self, offset: i64) -> String {
        self.lines()
            .map(|line| {
                let said = line.said.trim_end();
                if said.is_empty() {
                    line.heading(offset)
                } else {
                    format!("{}\n{said}", line.heading(offset))
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
