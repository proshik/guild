//! When a program is told the size its terminal has become — M5.14.
//!
//! A held `cmd-alt-arrow` changes a panel some thirty times a second, and
//! every change used to reach the pty at once: `claude` cleared and
//! repainted its whole screen for each, which was the blink. Terminals
//! commonly debounce for 25 ms to half a second; this waits for
//! the hand to pause, and never longer than LONGEST while it does not.
//! Pure: the tab keeps a [`Pending`] and asks [`due`] on its tick.

use std::time::{Duration, Instant};

use super::emulator::GridSize;

/// How long the size must stay put before the program is told.
pub const QUIET: Duration = Duration::from_millis(80);

/// The longest a new size waits while it keeps changing.
pub const LONGEST: Duration = Duration::from_millis(200);

/// A size the grid has been asked for and the program not yet told.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    /// The latest size asked for.
    pub size: GridSize,
    /// When it last changed.
    pub changed: Instant,
    /// When the first size of this gesture was asked for.
    pub since: Instant,
}

/// Ask for `size` at `now`, on top of whatever is pending.
pub fn want(pending: Option<Pending>, size: GridSize, now: Instant) -> Option<Pending> {
    Some(match pending {
        Some(pending) if pending.size == size => pending,
        Some(pending) => Pending {
            size,
            changed: now,
            since: pending.since,
        },
        None => Pending {
            size,
            changed: now,
            since: now,
        },
    })
}

/// Whether the program is told now: the size has been still for QUIET,
/// or the gesture has gone on for LONGEST.
pub fn due(pending: &Pending, now: Instant) -> bool {
    now.duration_since(pending.changed) >= QUIET || now.duration_since(pending.since) >= LONGEST
}
