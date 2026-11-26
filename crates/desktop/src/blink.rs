//! The blinking caret.
//!
//! A cursor that does not blink reads as a picture rather than as a place
//! where typing will land — that was the complaint. Every terminal people
//! are used to blinks.
//!
//! The rule is not "toggle on a timer". Typing **restarts** the phase, so
//! the caret is solid under the hand and only starts blinking once the
//! typing stops. Without that the cursor disappears mid-word, and it looks
//! as though the keystroke was lost.
//!
//! The logic is here, away from any window, so that it can be checked by
//! ordinary tests: terminal carets use the same phase rules.

use std::time::{Duration, Instant};

/// Half a blink cycle: the caret is visible for this long, then hidden for
/// the same.
///
/// 500 ms is what macOS uses for its own text caret
/// (`NSTextInsertionPointBlinkPeriod`), and matching it is why Guild's
/// cursor does not look foreign next to the system's.
pub const HALF_CYCLE: Duration = Duration::from_millis(500);

/// A caret's blink phase.
///
/// Holds the moment the phase started rather than a boolean: a boolean
/// would need someone to flip it on a timer, and a missed tick would leave
/// the caret stuck. Time cannot be missed.
#[derive(Debug, Clone, Copy)]
pub struct Caret {
    epoch: Instant,
}

impl Caret {
    /// A caret that has just been typed into: visible, with a full half
    /// cycle ahead of it.
    pub fn new(now: Instant) -> Self {
        Self { epoch: now }
    }

    /// Restart the phase — the person typed.
    ///
    /// The caret becomes visible at once and stays so for a whole half
    /// cycle. Blinking mid-word is what makes a keystroke feel lost.
    pub fn reset(&mut self, now: Instant) {
        self.epoch = now;
    }

    /// Whether the caret is drawn at this moment.
    ///
    /// `blinking` comes from the program: a terminal application asks for
    /// a steady cursor with DECSCUSR, and ignoring that would fight the
    /// program for the look of its own interface.
    pub fn visible(&self, now: Instant, blinking: bool) -> bool {
        if !blinking {
            return true;
        }

        self.phase(now).is_multiple_of(2)
    }

    /// The number of half cycles since the phase started.
    ///
    /// Public so that a caller can notice the moment it changes and repaint
    /// exactly then: repainting every frame for a cursor that changes twice
    /// a second is work nobody asked for.
    pub fn phase(&self, now: Instant) -> u64 {
        (now.saturating_duration_since(self.epoch).as_millis() / HALF_CYCLE.as_millis()) as u64
    }
}

/// When a caret's blink needs the window drawn again.
///
/// **Only when the last paint drew a caret that blinks** — 2026-09-24.
/// Asking on every phase made each terminal tab repaint the whole window
/// twice a second, in a window nobody was typing into: two idle `claude`
/// tabs were 240 notifies and 176 frames a minute. An unfocused tab draws
/// a hollow block that never blinks, a tab behind another is not drawn at
/// all, and a program may hide the cursor or ask for a steady one; none
/// of those pictures changes with the phase.
///
/// The paint says what it drew through a `&self` — painting does not get
/// the view mutably — hence the cell.
#[derive(Debug, Default)]
pub struct Repaint {
    phase: u64,
    blinking: std::cell::Cell<bool>,
}

impl Repaint {
    /// What the paint just drew: `true` for a caret that blinks.
    pub fn drew(&self, blinking: bool) {
        self.blinking.set(blinking);
    }

    /// Whether the picture is out of date at `phase`: it moved since the
    /// last ask and the last paint drew a blinking caret.
    ///
    /// **Asking forgets the paint.** A caret that is not drawn again — its
    /// tab put behind another — costs the one repaint that finds it gone,
    /// and nothing after; the next paint that draws it blinking says so.
    pub fn due(&mut self, phase: u64) -> bool {
        if phase == self.phase {
            return false;
        }
        self.phase = phase;
        self.blinking.take()
    }
}

/// A view's caret: its phase, whether its window lets it blink, and
/// whether the next phase needs a repaint.
///
/// **An inactive window's caret is steady** — the person's decision of
/// 2026-09-24, as common macOS terminals do: blinking in a window nobody
/// is typing into is motion in the corner of the eye, and it cost the
/// window two frames a second for nothing. Coming back to the window
/// restarts the phase, so the caret is there at once.
///
/// The paint asks through a `&self` — painting does not get the view
/// mutably — hence the cells; the view's own clock asks [`Self::due`].
#[derive(Debug)]
pub struct Blink {
    caret: std::cell::Cell<Caret>,
    /// Whether the window was active at the last paint.
    active: std::cell::Cell<bool>,
    repaint: Repaint,
}

impl Blink {
    /// A caret that has just been typed into.
    pub fn new(now: Instant) -> Self {
        Self {
            caret: std::cell::Cell::new(Caret::new(now)),
            active: std::cell::Cell::new(true),
            repaint: Repaint::default(),
        }
    }

    /// The person typed: solid again, a whole half cycle ahead.
    pub fn typed(&self, now: Instant) {
        self.caret.set(Caret::new(now));
    }

    /// Every paint of the view says whether its window is active, before
    /// anything is drawn — a paint that then draws no caret has said so
    /// too. A window become active again restarts the phase.
    pub fn seen(&self, active: bool, now: Instant) {
        if active && !self.active.get() {
            self.caret.set(Caret::new(now));
        }
        self.active.set(active);
        self.repaint.drew(false);
    }

    /// Whether the caret is drawn at `now`, when the view `wants` it to
    /// blink — the program asked and the view has the focus. It blinks
    /// only in the active window; otherwise it is steady, and no phase
    /// asks for a repaint.
    pub fn visible(&self, now: Instant, wants: bool) -> bool {
        let blinking = wants && self.active.get();
        self.repaint.drew(blinking);
        self.caret.get().visible(now, blinking)
    }

    /// Next visible blink boundary; a hidden/unpainted caret has no clock.
    pub(crate) fn deadline(&self, now: Instant) -> Option<Instant> {
        if !self.repaint.blinking.get() {
            return None;
        }
        let elapsed = now.saturating_duration_since(self.caret.get().epoch);
        let remaining = HALF_CYCLE.as_nanos() - elapsed.as_nanos() % HALF_CYCLE.as_nanos();
        Some(now + Duration::from_nanos(remaining as u64))
    }

    /// Whether the picture is out of date at `now` — [`Repaint::due`].
    pub fn due(&mut self, now: Instant) -> bool {
        let phase = self.caret.get().phase(now);
        self.repaint.due(phase)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_tracks_typing_and_stops_without_another_visible_paint() {
        let now = Instant::now();
        let mut blink = Blink::new(now);
        assert_eq!(blink.deadline(now), None);
        blink.seen(true, now);
        assert!(blink.visible(now, true));
        assert_eq!(blink.deadline(now), Some(now + HALF_CYCLE));
        let typed = now + Duration::from_millis(100);
        blink.typed(typed);
        assert_eq!(blink.deadline(typed), Some(typed + HALF_CYCLE));
        assert!(blink.due(typed + HALF_CYCLE));
        assert_eq!(blink.deadline(typed + HALF_CYCLE), None);
        blink.seen(false, typed + HALF_CYCLE);
        assert!(blink.visible(typed + HALF_CYCLE, true));
        assert_eq!(blink.deadline(typed + HALF_CYCLE), None);
    }

    fn at(caret: &Caret, ms: u64) -> bool {
        caret.visible(caret.epoch + Duration::from_millis(ms), true)
    }

    /// The whole point of resetting on input: right after a keystroke the
    /// caret is visible, not halfway through a dark phase.
    #[test]
    fn a_fresh_caret_is_visible() {
        let caret = Caret::new(Instant::now());

        assert!(at(&caret, 0), "the caret must be visible immediately");
        assert!(at(&caret, 499), "and for the whole first half cycle");
    }

    #[test]
    fn it_alternates_every_half_cycle() {
        let caret = Caret::new(Instant::now());

        assert!(at(&caret, 100), "visible");
        assert!(!at(&caret, 600), "hidden");
        assert!(at(&caret, 1100), "visible again");
        assert!(!at(&caret, 1600), "hidden again");
    }

    /// Typing restarts the phase. Without this the caret goes dark
    /// mid-word and the keystroke looks lost.
    #[test]
    fn typing_makes_the_caret_solid_again() {
        let start = Instant::now();
        let mut caret = Caret::new(start);

        let dark = start + Duration::from_millis(700);
        assert!(!caret.visible(dark, true), "it was in the dark phase");

        caret.reset(dark);

        assert!(
            caret.visible(dark, true),
            "a keystroke must bring the caret back at once"
        );
        assert!(
            caret.visible(dark + Duration::from_millis(400), true),
            "and keep it for a whole half cycle"
        );
    }

    /// A program can ask for a steady cursor. Blinking anyway would mean
    /// fighting it over the look of its own interface.
    #[test]
    fn a_steady_cursor_never_blinks() {
        let caret = Caret::new(Instant::now());
        let later = caret.epoch + Duration::from_millis(700);

        assert!(!caret.visible(later, true), "blinking: dark half");
        assert!(caret.visible(later, false), "steady: always drawn");
    }

    /// The phase counter is what a caller watches to repaint exactly when
    /// the caret flips, rather than every frame.
    #[test]
    fn the_phase_counts_half_cycles() {
        let caret = Caret::new(Instant::now());

        assert_eq!(caret.phase(caret.epoch), 0);
        assert_eq!(caret.phase(caret.epoch + Duration::from_millis(500)), 1);
        assert_eq!(caret.phase(caret.epoch + Duration::from_millis(1200)), 2);
    }

    /// A caret the last paint drew blinking asks for the next phase.
    #[test]
    fn a_blinking_caret_asks_when_the_phase_moves() {
        let mut repaint = Repaint::default();
        repaint.drew(true);

        assert!(!repaint.due(0), "the same phase is the same picture");
        assert!(repaint.due(1), "a new phase is a new picture");
    }

    /// **A caret nobody draws blinking asks for nothing** — 2026-09-24.
    /// Every terminal tab repainted the window twice a second: an
    /// unfocused one draws a hollow block that never blinks, one behind
    /// another tab is not drawn at all, and `claude` hides the cursor.
    #[test]
    fn a_caret_not_drawn_blinking_asks_for_nothing() {
        let mut repaint = Repaint::default();
        assert!(!repaint.due(1), "never drawn: a tab behind another");

        repaint.drew(false);
        assert!(!repaint.due(2), "drawn steady, hollow or not at all");
    }

    /// **Asking forgets the paint**, so a caret that stops being drawn —
    /// its tab put behind another — stops asking after the one repaint
    /// that finds it gone, and one drawn again asks again.
    #[test]
    fn a_caret_not_drawn_again_stops_asking() {
        let mut repaint = Repaint::default();
        repaint.drew(true);
        assert!(repaint.due(1));

        assert!(!repaint.due(2), "no paint since: nobody to repaint for");

        repaint.drew(true);
        assert!(repaint.due(3), "drawn again, blinking again");
    }

    fn ms(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    /// In the active window a wanted caret blinks, and each phase asks.
    #[test]
    fn a_caret_in_the_active_window_blinks() {
        let start = Instant::now();
        let mut blink = Blink::new(start);
        blink.seen(true, start);

        assert!(blink.visible(ms(start, 100), true));
        assert!(!blink.visible(ms(start, 600), true), "the dark half");
        assert!(blink.due(ms(start, 1100)), "the next phase asks");
    }

    /// **An inactive window's caret is steady** — the person's decision
    /// of 2026-09-24, as common macOS terminals do: drawn in the dark
    /// half too, and no phase asks for a repaint.
    #[test]
    fn an_inactive_window_draws_a_steady_caret_and_asks_for_nothing() {
        let start = Instant::now();
        let mut blink = Blink::new(start);
        blink.seen(false, start);

        assert!(
            blink.visible(ms(start, 600), true),
            "drawn in the dark half"
        );
        assert!(!blink.due(ms(start, 1100)), "no repaint for a steady caret");
        assert!(!blink.due(ms(start, 1600)));
    }

    /// **Coming back starts from the visible phase.** The window made
    /// active again restarts the phase, so the caret is there at once
    /// for a whole half cycle, rather than wherever the old phase was.
    #[test]
    fn coming_back_to_the_window_starts_from_the_visible_phase() {
        let start = Instant::now();
        let mut blink = Blink::new(start);
        blink.seen(true, start);
        blink.seen(false, ms(start, 100));

        let back = ms(start, 700);
        blink.seen(true, back);
        assert!(blink.visible(back, true), "the old phase was dark here");
        assert!(blink.visible(ms(start, 1199), true), "a whole half cycle");
        assert!(!blink.visible(ms(start, 1300), true), "and then it blinks");
        assert!(blink.due(ms(start, 1300)), "asking again");
    }

    /// Staying active is not coming back: the phase runs on.
    #[test]
    fn a_paint_in_a_window_still_active_keeps_the_phase() {
        let start = Instant::now();
        let blink = Blink::new(start);
        blink.seen(true, start);
        blink.seen(true, ms(start, 700));

        assert!(!blink.visible(ms(start, 700), true), "still the dark half");
    }

    /// A view that does not want a blink — an unfocused terminal — draws
    /// its caret steady in either window, and asks for nothing.
    #[test]
    fn a_caret_not_wanted_blinking_is_steady_anywhere() {
        let start = Instant::now();
        let mut blink = Blink::new(start);
        blink.seen(true, start);

        assert!(blink.visible(ms(start, 600), false));
        assert!(!blink.due(ms(start, 1100)));
    }

    /// Typing makes it solid again, whichever window.
    #[test]
    fn typing_restarts_the_blink() {
        let start = Instant::now();
        let blink = Blink::new(start);
        blink.seen(true, start);
        blink.typed(ms(start, 700));

        assert!(blink.visible(ms(start, 700), true));
    }

    /// Time can be read as slightly earlier than the epoch on a clock that
    /// is not monotonic across threads; that must not panic.
    #[test]
    fn a_moment_before_the_epoch_is_not_a_panic() {
        let caret = Caret::new(Instant::now());

        assert!(caret.visible(caret.epoch - Duration::from_millis(50), true));
    }
}
