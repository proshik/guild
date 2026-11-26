//! When a terminal's program is told its new size — M5.14, 2026-09-24.
//!
//! A held `cmd-alt-arrow` told the program some thirty sizes a second,
//! and `claude` cleared and repainted its whole screen for each: the
//! blink the person saw while a panel was stretched. A size now goes out
//! once the hand has paused, or every LONGEST while it has not.

use std::time::{Duration, Instant};

use guild_desktop::terminal::emulator::GridSize;
use guild_desktop::terminal::resize::{LONGEST, QUIET, due, want};

#[test]
fn a_size_is_told_once_the_hand_pauses() {
    let t0 = Instant::now();
    let first = want(None, GridSize::new(80, 24), t0).expect("a size is wanted");
    assert!(!due(&first, t0));
    assert!(!due(&first, t0 + QUIET - Duration::from_millis(1)));
    assert!(due(&first, t0 + QUIET));
}

#[test]
fn a_continuous_gesture_still_gets_a_size_every_longest() {
    let t0 = Instant::now();
    let mut held = want(None, GridSize::new(80, 24), t0).expect("a size is wanted");
    let mut now = t0;
    let mut columns = 80;
    while now + Duration::from_millis(30) < t0 + LONGEST {
        now += Duration::from_millis(30);
        columns += 1;
        held = want(Some(held), GridSize::new(columns, 24), now).expect("still wanted");
        assert!(!due(&held, now), "the hand is still moving at {now:?}");
    }
    assert!(due(&held, t0 + LONGEST), "but not for longer than LONGEST");
    assert_eq!(
        held.size,
        GridSize::new(columns, 24),
        "the latest size is the one told"
    );
}
