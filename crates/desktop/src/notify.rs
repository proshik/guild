//! What the window says in the corner: that something did not work, or
//! that something happened a person should know.
//!
//! Eight places used to report a failure by writing to the log and
//! stopping. From outside that is indistinguishable from the button doing
//! nothing, and the log is in `~/Library/Logs/Guild` — not a place anyone
//! looks while wondering why New Project opened no window.
//!
//! The rules live here rather than in the view for the same reason
//! `guild-session` has no GPUI: deduplication, the cap and the fading are
//! decisions worth checking without an open window.
//!
//! **Three levels, and only information fades** — 2026-09-22. Until then
//! there was one kind, drawn in the error's red and waiting to be
//! dismissed, on the reasoning that only failures need saying. By the
//! time the person asked, more than half of the fifty-odd notices were
//! not failures at all — *Committed*, *That worktree's folder is gone*,
//! *Select a change first* — and every one of them said *something
//! broke*. The levels are the diagnostics' own colours: information in
//! the card's plain colours, a warning in `accent.brass` — git refused,
//! a default is in effect, nothing is lost — and an error in
//! `accent.rust`, for what was not written or could not run. A warning
//! and an error wait for a person; information leaves after [`FADE`],
//! unless the pointer is on it.

use std::time::{Duration, Instant};

/// How many notices are shown at once.
///
/// Beyond this the oldest goes. A stack that grows without limit covers
/// the very thing a person is trying to fix.
pub const VISIBLE: usize = 3;

/// How long a notice that only informs stays once nothing holds it.
pub const FADE: Duration = Duration::from_secs(6);

/// How much a notice matters, which is what it looks like and whether it
/// leaves by itself. Ordered, so that the same line raised again at a
/// more serious level takes that level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// It happened, or it cannot be done from here: nothing is wrong.
    Info,
    /// Refused, or a default is standing in; nothing was lost.
    Warning,
    /// Something was not written, or could not run.
    Error,
}

/// One notice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    /// Process-local identity; removing another notice never changes it.
    id: usize,
    /// What it says.
    pub text: String,
    /// How much it matters.
    pub level: Level,
    /// Its text went to the clipboard from its own button.
    pub copied: bool,
    /// An explicit route to an existing Guild shell; never an automatic command.
    pub terminal: Option<(guild_session::ProjectId, std::path::PathBuf)>,
    /// The pointer is on it, which holds it however old it is.
    hovered: bool,
    /// When its time began: when it was raised, raised again, or left by
    /// the pointer.
    since: Instant,
}

/// The notices the window is showing.
#[derive(Debug, Default)]
pub struct Notices {
    items: Vec<Notice>,
    next_id: usize,
}

impl Notices {
    /// Raise a notice.
    ///
    /// The same text does not stack. `save` runs whenever anything
    /// changes, so a failing disk raises one line every few seconds; ten
    /// copies of it are not ten pieces of news. It is news again, though:
    /// its time starts over, and a more serious level wins.
    pub fn push(&mut self, level: Level, text: String, now: Instant) {
        if let Some(item) = self.items.iter_mut().find(|item| item.text == text) {
            item.level = item.level.max(level);
            item.since = now;
            return;
        }

        let id = self.next_id;
        self.next_id += 1;
        self.items.push(Notice {
            id,
            text,
            level,
            copied: false,
            terminal: None,
            hovered: false,
            since: now,
        });

        // The oldest goes rather than the newest being refused: what just
        // happened is what a person is looking for.
        while self.items.len() > VISIBLE {
            self.items.remove(0);
        }
    }

    pub fn push_terminal(
        &mut self,
        level: Level,
        text: String,
        project: guild_session::ProjectId,
        root: std::path::PathBuf,
        now: Instant,
    ) {
        self.push(level, text.clone(), now);
        if let Some(item) = self.items.iter_mut().find(|item| item.text == text) {
            item.terminal = Some((project, root));
        }
    }

    /// Take away the information whose time is up. Answers whether
    /// anything went, so that the window redraws only then.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.items.len();
        self.items.retain(|item| {
            item.level != Level::Info
                || item.hovered
                || now.saturating_duration_since(item.since) < FADE
        });
        self.items.len() != before
    }

    /// The pointer came onto a notice or left it. Leaving starts its time
    /// again: it was being read until then.
    pub fn hover(&mut self, index: usize, on: bool, now: Instant) {
        if let Some(item) = self.items.iter_mut().find(|item| item.id == index) {
            if item.hovered && !on {
                item.since = now;
            }
            item.hovered = on;
        }
    }

    /// The text to put on the clipboard, marking the notice as copied.
    pub fn copy(&mut self, index: usize) -> Option<String> {
        let item = self.items.iter_mut().find(|item| item.id == index)?;
        item.copied = true;
        Some(item.text.clone())
    }

    /// Take one away — the person closed it.
    ///
    /// An identity that is no longer there is a no-op: another notice may
    /// have displaced the target between press and release.
    pub fn dismiss(&mut self, index: usize) {
        self.items.retain(|item| item.id != index);
    }

    /// What to draw, oldest first, paired with stable process-local identities.
    pub fn visible(&self) -> impl Iterator<Item = (usize, &Notice)> {
        self.items.iter().map(|item| (item.id, item))
    }

    /// Is there anything to draw?
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}
