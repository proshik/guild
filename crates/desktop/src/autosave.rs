//! When a buffer may be written without anybody asking.
//!
//! All of the policy, none of the mechanism: the timer, the blur and the
//! write itself are the file tab's; every *decision* is [`should_write`], a
//! pure function over a record of facts, so that the whole table is a test.
//! The reason is this: it writes a person's files on a timer, and its
//! refusals are the difference between an editor and something that silently
//! destroys work.
//!
//! What autosave refuses, and why each one:
//!
//! | fact | refuses | because |
//! |---|---|---|
//! | the setting off | always | M8.1's behaviour: the dot and the sheet |
//! | not dirty | always | nothing to write, and a write is a watcher batch |
//! | not text | always | there is no buffer to write |
//! | not the project's file | always | a dependency's source is read in a tab and never written; after *not dirty*, so a clean one is never the tab a close asks about |
//! | changed on disk under the buffer | always | the line above the editor is a person's unanswered question, and a timer must not answer it for them |
//! | the last write failed | on the pause | the pause would retry a full disk every 300 ms; the next change, a blur or a close tries again |

/// What set autosave off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// No key for [`PAUSE`] after the last change.
    Pause,
    /// The editor lost the focus.
    Blur,
    /// The tab went behind another.
    Behind,
    /// The window stopped being the active one.
    Deactivate,
    /// A commit is about to read the disk.
    Commit,
    /// The tab, the project or the window is closing.
    Close,
}

/// What is known about the buffer at that moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Facts {
    pub enabled: bool,
    pub dirty: bool,
    pub is_text: bool,
    pub changed_on_disk: bool,
    pub last_write_failed: bool,
    /// The file is the project's own. A dependency's source, opened from
    /// External Libraries or by `f12`, is not — M5.11's decision 7.
    pub writable: bool,
    pub trigger: Trigger,
}

/// Why autosave did not write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Off,
    Clean,
    NotText,
    ReadOnly,
    Conflict,
    FailedBefore,
}

/// The decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Write,
    Refuse(Refusal),
}

/// How long after the last key a buffer is written: long enough that a
/// word typed is one write and not five, short enough to be *immediate*
/// to the person and to the watcher's hundred-millisecond batches.
pub const PAUSE: std::time::Duration = std::time::Duration::from_millis(300);

/// The table.
pub fn should_write(facts: Facts) -> Verdict {
    if !facts.enabled {
        return Verdict::Refuse(Refusal::Off);
    }
    if !facts.is_text {
        return Verdict::Refuse(Refusal::NotText);
    }
    if !facts.dirty {
        return Verdict::Refuse(Refusal::Clean);
    }
    if !facts.writable {
        return Verdict::Refuse(Refusal::ReadOnly);
    }
    if facts.changed_on_disk {
        return Verdict::Refuse(Refusal::Conflict);
    }
    if facts.last_write_failed && facts.trigger == Trigger::Pause {
        return Verdict::Refuse(Refusal::FailedBefore);
    }
    Verdict::Write
}
