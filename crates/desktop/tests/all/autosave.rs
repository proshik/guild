//! The autosave table, every row of it, through the public door: the
//! decisions that write a person's file on a timer are the ones a test
//! must own.

use guild_desktop::autosave::{Facts, PAUSE, Refusal, Trigger, Verdict, should_write};

fn facts(trigger: Trigger) -> Facts {
    Facts {
        enabled: true,
        dirty: true,
        is_text: true,
        changed_on_disk: false,
        last_write_failed: false,
        writable: true,
        trigger,
    }
}

/// Every trigger writes an ordinary dirty buffer.
#[test]
fn every_trigger_writes_a_dirty_buffer() {
    for trigger in [
        Trigger::Pause,
        Trigger::Blur,
        Trigger::Behind,
        Trigger::Deactivate,
        Trigger::Commit,
        Trigger::Close,
    ] {
        assert_eq!(should_write(facts(trigger)), Verdict::Write, "{trigger:?}");
    }
}

/// Off is off, for every trigger: M8.1's behaviour comes back whole.
#[test]
fn off_never_writes() {
    for trigger in [
        Trigger::Pause,
        Trigger::Blur,
        Trigger::Close,
        Trigger::Commit,
    ] {
        assert_eq!(
            should_write(Facts {
                enabled: false,
                ..facts(trigger)
            }),
            Verdict::Refuse(Refusal::Off)
        );
    }
}

/// Nothing to write, nothing written: a write is a watcher batch and a
/// status walk.
#[test]
fn a_clean_buffer_is_not_written() {
    assert_eq!(
        should_write(Facts {
            dirty: false,
            ..facts(Trigger::Pause)
        }),
        Verdict::Refuse(Refusal::Clean)
    );
}

/// A tab that is not text has no buffer.
#[test]
fn what_is_not_text_is_not_written() {
    assert_eq!(
        should_write(Facts {
            is_text: false,
            ..facts(Trigger::Close)
        }),
        Verdict::Refuse(Refusal::NotText)
    );
}

/// The one refusal that costs work if it is wrong: a file changed on
/// disk under a dirty buffer is a question for the person, and no
/// trigger answers it — not the pause, not a close, not a commit.
#[test]
fn a_conflict_is_never_written_over() {
    for trigger in [
        Trigger::Pause,
        Trigger::Blur,
        Trigger::Behind,
        Trigger::Deactivate,
        Trigger::Commit,
        Trigger::Close,
    ] {
        assert_eq!(
            should_write(Facts {
                changed_on_disk: true,
                ..facts(trigger)
            }),
            Verdict::Refuse(Refusal::Conflict),
            "{trigger:?}"
        );
    }
}

/// A failed write is not retried every pause — a full disk would be
/// asked three times a second — but a blur, a close or a commit tries
/// again: a different moment, and the disk may be back.
#[test]
fn a_failed_write_waits_for_a_reason_to_try_again() {
    let failed = |trigger| Facts {
        last_write_failed: true,
        ..facts(trigger)
    };
    assert_eq!(
        should_write(failed(Trigger::Pause)),
        Verdict::Refuse(Refusal::FailedBefore)
    );
    for trigger in [
        Trigger::Blur,
        Trigger::Close,
        Trigger::Commit,
        Trigger::Deactivate,
    ] {
        assert_eq!(should_write(failed(trigger)), Verdict::Write, "{trigger:?}");
    }
}

/// A dependency's source is read in a tab and never written: no trigger
/// writes into `~/.cargo`, and a clean one is still `Clean`, so closing a
/// clean dependency tab asks nothing — the row comes after `Clean` for
/// that reason.
#[test]
fn a_file_that_is_not_the_projects_is_never_written() {
    for trigger in [
        Trigger::Pause,
        Trigger::Blur,
        Trigger::Behind,
        Trigger::Deactivate,
        Trigger::Commit,
        Trigger::Close,
    ] {
        assert_eq!(
            should_write(Facts {
                writable: false,
                ..facts(trigger)
            }),
            Verdict::Refuse(Refusal::ReadOnly),
            "{trigger:?}"
        );
        assert_eq!(
            should_write(Facts {
                writable: false,
                dirty: false,
                ..facts(trigger)
            }),
            Verdict::Refuse(Refusal::Clean),
            "{trigger:?}"
        );
    }
}

/// The pause is short enough to read as immediate and long enough that a
/// word is one write.
#[test]
fn the_pause_is_a_fraction_of_a_second() {
    assert!(PAUSE.as_millis() >= 200 && PAUSE.as_millis() <= 500);
}
