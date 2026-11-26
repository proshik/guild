//! The double Shift detector — M8.9 group A's task 7, decision 8.
//!
//! GPUI turns a lone Shift into a keystroke named `shift` and would match
//! a binding `shift shift` with a pending wait of a second; it also counts
//! a shift-click as a tap, because only a key down tells it otherwise. So
//! the window detects the pair itself, from the modifiers and a count of
//! every key down and every mouse press its ears have heard: **a tap is
//! Shift alone, pressed and let go inside `HOLD` with the count unchanged,
//! and two taps fire when the second press comes inside `GAP` of the first
//! release with the count unchanged between them.**

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use guild_desktop::actions::keymap::Named;
use guild_desktop::double_tap::{DOUBLE_SHIFT, DoubleTap, GAP, HOLD, bound};

/// A hand on the Shift key, with a clock and the ears' count.
struct Hand {
    tap: DoubleTap,
    now: Instant,
    heard: u64,
    fired: usize,
}

impl Hand {
    fn new() -> Self {
        Self {
            tap: DoubleTap::default(),
            now: Instant::now(),
            heard: 0,
            fired: 0,
        }
    }

    fn wait(&mut self, ms: u64) -> &mut Self {
        self.now += Duration::from_millis(ms);
        self
    }

    /// The modifiers changed to Shift alone.
    fn shift(&mut self) -> &mut Self {
        self.feed(true, false)
    }

    /// The modifiers changed to something that is not Shift alone — `cmd`,
    /// or `cmd-shift`.
    fn other(&mut self) -> &mut Self {
        self.feed(false, false)
    }

    /// Every modifier let go.
    fn let_go(&mut self) -> &mut Self {
        self.feed(false, true)
    }

    fn feed(&mut self, shift_alone: bool, none: bool) -> &mut Self {
        if self.tap.modifiers(shift_alone, none, self.heard, self.now) {
            self.fired += 1;
        }
        self
    }

    /// A key down or a mouse press, heard by an ear.
    fn heard(&mut self) -> &mut Self {
        self.heard += 1;
        self
    }

    /// Shift pressed alone and let go after `hold` milliseconds.
    fn tap(&mut self, hold: u64) -> &mut Self {
        self.shift().wait(hold).let_go()
    }
}

#[test]
fn two_taps_inside_the_gap_fire_once() {
    let mut hand = Hand::new();
    hand.tap(80).wait(150).tap(80);
    assert_eq!(hand.fired, 1);
}

#[test]
fn taps_further_apart_do_not() {
    let mut hand = Hand::new();
    hand.tap(80).wait(GAP.as_millis() as u64 + 200).tap(80);
    assert_eq!(hand.fired, 0);
}

/// The count moves between the first release and the second press: a key
/// typed between two Shifts — `shift`, `a`, `shift` — is not a pair.
#[test]
fn a_key_between_taps_breaks_them() {
    let mut hand = Hand::new();
    hand.tap(80).wait(50).heard().wait(50).tap(80);
    assert_eq!(hand.fired, 0);
}

/// **`shift-f7`**: the count moves between a press and its release. A
/// binding takes that key before any key listener hears it, which is why
/// the ear for keys is an interceptor — and two of them at the rhythm of
/// two taps walk a diff backwards, never open a popup.
#[test]
fn a_key_during_a_tap_is_not_a_tap() {
    let mut hand = Hand::new();
    hand.shift().wait(40).heard().wait(40).let_go();
    hand.wait(150);
    hand.shift().wait(40).heard().wait(40).let_go();
    assert_eq!(hand.fired, 0);
}

/// **The shift-click decision 8 exists for**: two shift-clicks in the tree
/// to select a range. The mouse ear counts the press, during a tap or
/// between two.
#[test]
fn a_press_of_the_mouse_between_taps_breaks_them() {
    let mut hand = Hand::new();
    // Shift held over a click, twice.
    hand.shift().wait(30).heard().wait(30).let_go();
    hand.wait(100);
    hand.shift().wait(30).heard().wait(30).let_go();
    assert_eq!(hand.fired, 0, "a click during each");

    let mut hand = Hand::new();
    hand.tap(60).wait(60).heard().wait(60).tap(60);
    assert_eq!(hand.fired, 0, "a click between");
}

#[test]
fn a_shift_held_past_the_hold_is_not_a_tap() {
    let mut hand = Hand::new();
    hand.tap(HOLD.as_millis() as u64 + 100).wait(100).tap(80);
    assert_eq!(hand.fired, 0, "the first held too long");

    let mut hand = Hand::new();
    hand.tap(80).wait(100).tap(HOLD.as_millis() as u64 + 100);
    assert_eq!(hand.fired, 0, "the second held too long");
}

/// `cmd-shift` pressed and let go is a chord that went nowhere, not a Shift
/// — whichever went down first and whichever came up first.
#[test]
fn shift_with_another_modifier_is_not_a_tap() {
    let mut hand = Hand::new();
    // Shift, then cmd with it, then cmd up, then Shift up.
    hand.shift()
        .wait(20)
        .other()
        .wait(20)
        .shift()
        .wait(20)
        .let_go();
    hand.wait(100);
    hand.shift()
        .wait(20)
        .other()
        .wait(20)
        .shift()
        .wait(20)
        .let_go();
    assert_eq!(hand.fired, 0, "cmd joined a held Shift");

    let mut hand = Hand::new();
    // Cmd first, Shift with it, cmd up, Shift up.
    hand.other()
        .wait(20)
        .other()
        .wait(20)
        .shift()
        .wait(20)
        .let_go();
    hand.wait(100);
    hand.other()
        .wait(20)
        .other()
        .wait(20)
        .shift()
        .wait(20)
        .let_go();
    assert_eq!(hand.fired, 0, "Shift joined a held cmd");

    let mut hand = Hand::new();
    hand.tap(60).wait(60);
    hand.other().wait(20).let_go();
    hand.wait(60).tap(60);
    assert_eq!(hand.fired, 0, "a cmd tapped between two Shifts");
}

#[test]
fn a_third_tap_starts_a_new_pair() {
    let mut hand = Hand::new();
    hand.tap(60).wait(100).tap(60);
    assert_eq!(hand.fired, 1);
    hand.wait(100).tap(60);
    assert_eq!(hand.fired, 1, "the third is the first of a new pair");
    hand.wait(100).tap(60);
    assert_eq!(hand.fired, 2, "and the fourth completes it");
}

/// **A stroke recorded for `SearchEverywhere` replaces the double Shift**,
/// which is `effective()`'s rule — an override replaces the default —
/// applied to a default that is not a binding.
#[test]
fn a_recorded_stroke_replaces_the_double_shift() {
    let mut overrides = BTreeMap::new();
    assert!(bound(&overrides), "nothing recorded: the double Shift");
    overrides.insert(Named::CloseTab, "cmd-j".to_owned());
    assert!(bound(&overrides), "a stroke for something else");
    overrides.insert(Named::SearchEverywhere, "cmd-alt-j".to_owned());
    assert!(!bound(&overrides), "a stroke of its own");
    assert_eq!(DOUBLE_SHIFT, "shift shift");
}

/// **A release lost outside the window does not eat the next tap.** Shift
/// let go over another application sends the window nothing, so the press
/// it heard is still under way when Shift next goes down alone; a press
/// older than `HOLD` could never have been a tap, so the new one starts
/// afresh — and a double Shift right after it fires, rather than a first
/// tap being spent on the stale press.
#[test]
fn a_release_lost_outside_the_window_does_not_eat_the_next_tap() {
    let mut hand = Hand::new();
    hand.shift().wait(2_000);
    hand.tap(80).wait(150).tap(80);
    assert_eq!(hand.fired, 1);
}
