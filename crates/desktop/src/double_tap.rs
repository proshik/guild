//! A double Shift, detected rather than bound — M8.9, decision 8.
//!
//! **Not a binding**, for two reasons read in GPUI's source: a lone Shift
//! becomes a keystroke named `shift`, and a binding `shift shift` would be
//! matched as a pending sequence that holds the first Shift for a second;
//! and GPUI counts a shift-click as a tap, because only a key down tells it
//! a Shift was not alone. Two shift-clicks in the tree to select a range
//! would open Search Everywhere.
//!
//! So the window listens with three ears — the modifiers on the root, an
//! interceptor that hears every key down (a key a binding takes never
//! reaches a key listener: `shift-f7` pressed twice to walk a diff back
//! would be two lone taps to the root), and a window listener for every
//! mouse press (the root's own hitbox is not hovered under an occluding
//! card). The last two only count, into `Root.heard`; this is the pure
//! detector the modifiers ear hands that count to, and it fires on **two
//! taps of Shift alone with nothing heard in between**.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::actions::keymap::Named;

/// The most time from the first tap's release to the second tap's press.
pub const GAP: Duration = Duration::from_millis(300);

/// The longest Shift may be held and still be a tap rather than a hand
/// resting on it.
pub const HOLD: Duration = Duration::from_millis(300);

/// How the default is spelled where a stroke is printed — a sequence of two
/// strokes, which `actions::printed` turns into `⇧⇧`.
pub const DOUBLE_SHIFT: &str = "shift shift";

/// Whether the double Shift opens Search Everywhere: **not once a stroke
/// has been recorded for it**, which is `actions::effective`'s rule — an
/// override replaces the default — for a default that is not a binding.
pub fn bound(overrides: &BTreeMap<Named, String>) -> bool {
    !overrides.contains_key(&Named::SearchEverywhere)
}

/// Shift going down, and what was true then.
#[derive(Debug, Clone, Copy)]
struct Down {
    /// When it went down alone.
    at: Instant,
    /// The ears' count then.
    heard: u64,
    /// Another modifier was down with it at some point: whatever this is,
    /// it is a chord and not a tap, until every modifier is let go.
    spoiled: bool,
}

/// The detector's state.
#[derive(Debug, Default)]
pub struct DoubleTap {
    /// The press under way, if one is.
    down: Option<Down>,
    /// The last tap, waiting for a second: when it was let go, and the
    /// ears' count then.
    tapped: Option<(Instant, u64)>,
}

impl DoubleTap {
    /// The modifiers changed. `shift_alone` is Shift and nothing else;
    /// `none` is every modifier up; `heard` is how many key downs and mouse
    /// presses the ears have counted so far. Answers whether this completes
    /// a pair.
    ///
    /// **A tap** is Shift pressed alone and let go inside [`HOLD`] with the
    /// count unchanged — a key or a press while it is held makes it a
    /// shifted stroke or a shift-click. **A pair** is a second tap pressed
    /// inside [`GAP`] of the first one's release with the count unchanged
    /// between them. A pair that fires is spent, so a third tap starts a
    /// new one.
    pub fn modifiers(&mut self, shift_alone: bool, none: bool, heard: u64, now: Instant) -> bool {
        if shift_alone {
            // Shift alone after a chord let go of its other key is still
            // the chord's: the press under way keeps what it was. **An
            // unspoiled press older than the hold is a release that never
            // arrived** — Shift let go over another application sends the
            // window nothing — and could not have been a tap anyway, so
            // this press starts afresh rather than being judged from it.
            // A spoiled one is kept: a chord held long is still a chord.
            let stale = self
                .down
                .is_some_and(|down| !down.spoiled && now.duration_since(down.at) > HOLD);
            if self.down.is_none() || stale {
                self.down = Some(Down {
                    at: now,
                    heard,
                    spoiled: false,
                });
            }
            return false;
        }
        if !none {
            // Another modifier: a chord, whichever key went down first.
            match &mut self.down {
                Some(down) => down.spoiled = true,
                None => {
                    self.down = Some(Down {
                        at: now,
                        heard,
                        spoiled: true,
                    })
                }
            }
            self.tapped = None;
            return false;
        }

        let Some(down) = self.down.take() else {
            return false;
        };
        let tap = !down.spoiled && now.duration_since(down.at) <= HOLD && heard == down.heard;
        if !tap {
            self.tapped = None;
            return false;
        }
        let pair = self.tapped.take().is_some_and(|(released, then)| {
            down.at.duration_since(released) <= GAP && down.heard == then
        });
        if !pair {
            self.tapped = Some((now, heard));
        }
        pair
    }
}
