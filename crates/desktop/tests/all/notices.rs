//! What the window says when something did not work.
//!
//! Out here rather than beside the code because every one of these goes
//! in through the public door — which makes them a check on that door as
//! well as on the behaviour behind it.

use std::time::{Duration, Instant};

use guild_desktop::notify::*;

/// The point of the whole module: a failure stays until it is
/// acknowledged. A message about an unsaved keymap that goes away on
/// its own is a message nobody read.
#[test]
fn a_notice_waits_to_be_dismissed() {
    let mut notices = Notices::default();
    notices.push(
        Level::Error,
        "the keymap was not saved".to_owned(),
        Instant::now(),
    );

    assert_eq!(notices.visible().count(), 1);
    assert!(!notices.is_empty(), "nothing takes it away but a person");
    assert!(
        !notices.expire(Instant::now() + Duration::from_secs(3600)),
        "an error does not fade"
    );

    notices.dismiss(0);

    assert!(notices.is_empty());
}

/// `save` runs whenever anything changes, so a failing disk raises the
/// same line every few seconds. Stacking them papers over the window
/// within a minute.
#[test]
fn the_same_text_does_not_stack() {
    let mut notices = Notices::default();

    for _ in 0..10 {
        notices.push(
            Level::Error,
            "state was not saved".to_owned(),
            Instant::now(),
        );
    }

    assert_eq!(
        notices.visible().count(),
        1,
        "one line, however often it fires"
    );
}

/// Several different failures at once must not grow without limit: a
/// stack that covers the window hides the thing being fixed.
#[test]
fn only_the_newest_few_are_shown() {
    let mut notices = Notices::default();
    for n in 0..(VISIBLE + 4) {
        notices.push(Level::Error, format!("failure {n}"), Instant::now());
    }

    assert_eq!(notices.visible().count(), VISIBLE);
    let texts: Vec<_> = notices.visible().map(|(_, n)| n.text.clone()).collect();
    assert!(
        texts.contains(&format!("failure {}", VISIBLE + 3)),
        "the newest is kept: {texts:?}"
    );
    assert!(
        !texts.contains(&"failure 0".to_owned()),
        "the oldest goes: {texts:?}"
    );
}

/// Closing one must not take its neighbours.
#[test]
fn dismissing_one_leaves_the_others() {
    let mut notices = Notices::default();
    notices.push(Level::Info, "first".to_owned(), Instant::now());
    notices.push(Level::Info, "second".to_owned(), Instant::now());

    notices.dismiss(0);

    let left: Vec<_> = notices.visible().map(|(_, n)| n.text.clone()).collect();
    assert_eq!(left, vec!["second".to_owned()]);
}

/// A stale index must not panic.
#[test]
fn dismissing_something_that_is_gone_is_not_a_panic() {
    let mut notices = Notices::default();
    notices.dismiss(7);
    assert!(notices.is_empty());
}

/// **Information goes by itself, and a warning waits** — the person,
/// 2026-09-22: *That worktree's folder is gone* in the error's red, and
/// staying there, said something had broken when nothing had. What only
/// informs leaves after [`FADE`]; what asks for a decision stays.
#[test]
fn information_fades_and_a_warning_waits() {
    let start = Instant::now();
    let mut notices = Notices::default();
    notices.push(Level::Info, "committed".to_owned(), start);
    notices.push(Level::Warning, "git refused".to_owned(), start);

    assert!(!notices.expire(start + FADE - Duration::from_millis(1)));
    assert_eq!(notices.visible().count(), 2, "not yet");

    assert!(notices.expire(start + FADE), "something went");
    let left: Vec<_> = notices.visible().map(|(_, n)| n.level).collect();
    assert_eq!(left, vec![Level::Warning]);
}

/// A notice being read — the pointer on it, about to copy it — does not
/// leave under the pointer, and its time starts again when the pointer
/// goes.
#[test]
fn a_notice_under_the_pointer_stays_and_counts_again_when_left() {
    let start = Instant::now();
    let mut notices = Notices::default();
    notices.push(Level::Info, "no server for Go".to_owned(), start);

    notices.hover(0, true, start + Duration::from_secs(1));
    assert!(!notices.expire(start + FADE * 3), "held while hovered");

    let left = start + FADE * 3;
    notices.hover(0, false, left);
    assert!(!notices.expire(left + FADE - Duration::from_millis(1)));
    assert!(notices.expire(left + FADE));
    assert!(notices.is_empty());
}

/// The same line raised again is still one line, and it is news again:
/// its time starts over, and a more serious level wins.
#[test]
fn the_same_text_again_starts_its_time_over() {
    let start = Instant::now();
    let mut notices = Notices::default();
    notices.push(Level::Info, "same".to_owned(), start);
    let again = start + FADE - Duration::from_millis(10);
    notices.push(Level::Info, "same".to_owned(), again);

    assert!(
        !notices.expire(start + FADE),
        "counted from the second time"
    );
    assert_eq!(notices.visible().count(), 1);

    notices.push(Level::Error, "same".to_owned(), again);
    assert_eq!(
        notices.visible().map(|(_, n)| n.level).collect::<Vec<_>>(),
        vec![Level::Error]
    );
    assert!(
        !notices.expire(again + FADE * 10),
        "an error now, and it waits"
    );
}

/// Copying says it happened, on the notice that was copied and no other.
#[test]
fn a_copied_notice_says_so() {
    let mut notices = Notices::default();
    notices.push(Level::Error, "one".to_owned(), Instant::now());
    notices.push(Level::Error, "two".to_owned(), Instant::now());

    assert_eq!(notices.copy(1), Some("two".to_owned()));
    let copied: Vec<_> = notices.visible().map(|(_, n)| n.copied).collect();
    assert_eq!(copied, vec![false, true]);
    assert_eq!(notices.copy(9), None, "a stale index copies nothing");
}

#[test]
fn evicted_notice_actions_cannot_target_its_replacement() {
    let mut notices = Notices::default();
    let now = Instant::now();
    notices.push(Level::Warning, "original".into(), now);
    let original = notices.visible().next().unwrap().0;
    for n in 0..VISIBLE {
        notices.push(Level::Warning, format!("replacement {n}"), now);
    }
    assert_eq!(
        notices.copy(original),
        None,
        "stale action must not copy a neighbour"
    );
    notices.dismiss(original);
    assert_eq!(notices.visible().count(), VISIBLE);
}
