//! What the agent in a session is doing — M5.10, and since 2026-11-06
//! only what the program says of itself.
//!
//! The program reports its state through OSC 7501, the Program Status
//! Protocol; the terminal keeps the records and the window hands each change
//! of the root record to [`Board::report`], in order. A program that reports
//! nothing has no status: no mark, no unread, no sound, and `unknown` on the
//! wire. The [`Board`] adds only what the window itself knows — whether the
//! person has seen a finished turn.
//!
//! The hook vocabulary below — [`Signal`], [`Status::next`], [`from_hook`] —
//! now moves only the managed-input [`Latch`], which is never displayed.
//!
//! Nothing here is persisted: a restarted window shows nothing until the
//! program reports again.

use serde_json::Value;

use crate::SessionId;

mod latch;
pub use latch::Latch;

/// The tool that asks the person a question.
///
/// Task 0 found it fires `PermissionRequest` as well as `PreToolUse`, so
/// both are read as the question rather than the permission overwriting
/// the reason a moment after the question set it.
pub const QUESTION_TOOL: &str = "AskUserQuestion";

/// Why an agent is waiting for the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// A tool needs permission to run.
    Permission,
    /// The agent asked a question.
    Question,
    /// The program needs the person to sign in.
    Auth,
    /// The program did not say why: the question mark, and no reason.
    Unstated,
}

/// What the agent in a session is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    /// Nothing, as far as anything has said.
    #[default]
    Idle,
    /// In the middle of a turn.
    Working,
    /// Stopped until the person answers.
    Waiting(Why),
    /// The turn is over, and the person has not looked at it yet.
    Done,
    /// An authoritative execution error, retained until a new turn.
    Failed,
}

/// Something that moves a status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    /// A prompt went in: `UserPromptSubmit`.
    PromptSubmitted,
    /// A tool is about to run: `PreToolUse`.
    ToolStarted,
    /// A question is up: [`QUESTION_TOOL`].
    QuestionAsked,
    /// A permission prompt is up: `PermissionRequest`.
    PermissionAsked,
    /// A tool has finished, successfully or with a reported tool error.
    ToolFinished,
    /// An identified response or authoritative resumed execution.
    Resumed,
    /// An authoritative execution error.
    ExecutionFailed,
    /// The turn is over: `Stop` or the harness transcript completion.
    TurnEnded,
    /// A source-authoritative completed turn, including identity-free Codex Stop.
    TurnCompleted,
    /// The session is over: `SessionEnd`.
    SessionEnded,
    /// Esc or Ctrl-C typed into the session's tab.
    ///
    /// Task 0 found that an interrupt fires no hook at all — no `Stop`,
    /// no `PostToolUse` — so the keystroke is the only witness there is.
    Interrupted,
    /// The session's tab is in front of a focused window.
    Seen,
}

/// What a status asks of the person: the dot on a tab.
///
/// Ordered by urgency, so the larger of two is the one to show: an agent
/// that cannot go on outranks one that has merely finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    /// A turn finished that nobody has read.
    Done,
    /// An observed execution error needing attention.
    Failed,
    /// An unanswered request.
    Blocked,
}

impl Status {
    /// The status after `signal` — decision 7 of M5.10's plan, row by row.
    pub fn next(self, signal: Signal) -> Status {
        use Signal::*;
        use Status::*;

        match (self, signal) {
            (_, PromptSubmitted) => Working,
            (_, QuestionAsked) => Waiting(Why::Question),
            (_, PermissionAsked) => Waiting(Why::Permission),
            // A parallel call's PreToolUse can land while another waits,
            // and it answers nothing.
            (Failed, ToolStarted) => Failed,
            (Waiting(why), ToolStarted) => Waiting(why),
            (_, ToolStarted) => Working,
            // PostToolUse is asynchronous, so it can land after Stop: a
            // finished turn stays finished.
            (Working, ToolFinished) => Working,
            (Idle | Done | Failed | Waiting(_), ToolFinished) => self,
            (_, Resumed) => Working,
            (_, ExecutionFailed) => Failed,
            (Failed | Waiting(_), TurnEnded) => self,
            (Failed, TurnCompleted) => Failed,
            (_, TurnCompleted) => Done,
            (_, TurnEnded) => Done,
            (_, SessionEnded) => Idle,
            (Working | Waiting(_), Interrupted) => Idle,
            (Idle | Done | Failed, Interrupted) => self,
            // Looking at a waiting agent does not answer it.
            (Done, Seen) => Idle,
            (_, Seen) => self,
        }
    }

    /// What this status asks of the person, if anything.
    pub fn need(self) -> Option<Need> {
        match self {
            Status::Waiting(_) => Some(Need::Blocked),
            Status::Done => Some(Need::Done),
            Status::Failed => Some(Need::Failed),
            Status::Idle | Status::Working => None,
        }
    }

    /// Whether the agent is in the middle of a turn.
    pub fn working(self) -> bool {
        self == Status::Working
    }

    /// How urgent this is where several statuses fold into one: a
    /// project's tab, the Dock. Blocked outranks done outranks working.
    pub fn urgency(self) -> u8 {
        match self {
            Status::Idle => 0,
            Status::Working => 1,
            Status::Done => 2,
            Status::Failed => 3,
            Status::Waiting(_) => 4,
        }
    }
}

/// The session a hook's payload is about, and what it signals.
///
/// `None` for a payload without a session the window could have started,
/// and for a hook nobody asked for — a newer CLI, or one the person's own
/// settings add beside ours. Neither is an error.
pub fn from_hook(payload: &Value) -> Option<(SessionId, Signal)> {
    let session = payload.get("session_id")?.as_str()?.parse().ok()?;
    let tool = payload.get("tool_name").and_then(Value::as_str);
    let signal = match (payload.get("hook_event_name")?.as_str()?, tool) {
        ("UserPromptSubmit", _) => Signal::PromptSubmitted,
        ("PreToolUse" | "PermissionRequest", Some(QUESTION_TOOL)) => Signal::QuestionAsked,
        ("PreToolUse", _) => Signal::ToolStarted,
        ("PermissionRequest", _) => Signal::PermissionAsked,
        ("PostToolUse" | "PostToolUseFailure", _) => Signal::ToolFinished,
        ("Stop", _) => Signal::TurnEnded,
        ("SessionEnd", _) => Signal::SessionEnded,
        _ => return None,
    };
    Some((session, signal))
}

/// Why a program that reported `blocked` waits — OSC 7501's `kind`, only
/// when the program said it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A tool or action needs the person's permission.
    Permission,
    /// The program asked the person a question.
    Question,
    /// The program needs the person to sign in.
    Auth,
}

/// What a program reported of itself through OSC 7501: its root record's
/// state. `clear` is no record at all, so it is not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reported {
    /// Nothing in progress: `idle`.
    Idle,
    /// A turn in progress: `working`.
    Working,
    /// Stopped until the person acts: `blocked`, with its `kind` if said.
    Blocked(Option<Kind>),
    /// A turn finished: `done`.
    Done,
    /// A turn failed: `error`.
    Error,
}

/// What one report did to a session.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportOutcome {
    /// The record changed.
    pub changed: bool,
    /// A turn began: the record became `working` from outside a turn.
    pub began: bool,
    /// A turn ended: after `working`, the record became `done`, `idle` or
    /// `error`.
    pub ended: bool,
}

/// The waiting reason a reported `kind` shows; none said is none invented.
fn why(kind: Option<Kind>) -> Why {
    match kind {
        Some(Kind::Permission) => Why::Permission,
        Some(Kind::Question) => Why::Question,
        Some(Kind::Auth) => Why::Auth,
        None => Why::Unstated,
    }
}

/// Every session's status in one place, and who needs the person first.
///
/// The window's one answer to what the agents are doing, moved only by
/// OSC 7501 reports ([`Board::report`]) and by the person seeing a tab
/// ([`Board::seen`]).
#[derive(Debug, Default)]
pub struct Board {
    /// Only sessions that are not idle: an idle one is simply absent.
    entries: std::collections::HashMap<SessionId, Entry>,
    /// Bumped on every change. *Longest waiting* is the smallest `since`
    /// — a counter rather than a clock, so that two changes in one tick
    /// still have an order and a test can say what it means.
    clock: u64,
    /// Wall-clock time of the last observed state transition, and its place
    /// among the transitions observed; never persisted. The place breaks the
    /// ties a burst of reports in one clock tick makes.
    transitioned: std::collections::HashMap<SessionId, (std::time::SystemTime, u64)>,
    /// Bumped on every recorded transition.
    transitions: u64,
    /// First completion of the current turn; survives Seen removing its entry.
    completed: std::collections::HashMap<SessionId, u64>,
    /// Each session's root OSC 7501 record, as last reported.
    reported: std::collections::HashMap<SessionId, Reported>,
    /// Sessions whose record has been `working` since the last turn ended.
    in_turn: std::collections::HashSet<SessionId>,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    status: Status,
    since: u64,
}

/// How many sessions are in each state a person can see.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Waiting for the person.
    pub blocked: usize,
    /// Finished a turn nobody has read.
    pub done: usize,
    /// Turns in progress.
    pub working: usize,
    /// Observed execution errors.
    pub failed: usize,
}

/// First completions still unread at the end of a status pass.
pub type DoneCompletions = std::collections::HashSet<(SessionId, u64)>;

/// Whether this pass contains a completion absent from the previous pass.
pub fn should_chime(previous: &DoneCompletions, current: &DoneCompletions) -> bool {
    current.iter().any(|entry| !previous.contains(entry))
}

impl Board {
    /// A change of a session's root OSC 7501 record, in the order the
    /// program reported it; `None` when the record is gone.
    pub fn report(&mut self, session: SessionId, record: Option<Reported>) -> ReportOutcome {
        let before = match record {
            Some(record) => self.reported.insert(session, record),
            None => self.reported.remove(&session),
        };
        if before == record {
            return ReportOutcome::default();
        }
        let mut outcome = ReportOutcome {
            changed: true,
            ..ReportOutcome::default()
        };
        let after = match record {
            None | Some(Reported::Idle) => Status::Idle,
            Some(Reported::Working) => Status::Working,
            Some(Reported::Blocked(kind)) => Status::Waiting(why(kind)),
            // A `done` is unread until seen: a change into it is a new
            // completion, a repeat is not (the equality above).
            Some(Reported::Done) => Status::Done,
            Some(Reported::Error) => Status::Failed,
        };
        self.transitions += 1;
        self.transitioned
            .insert(session, (std::time::SystemTime::now(), self.transitions));
        if after == Status::Idle {
            self.entries.remove(&session);
        } else {
            self.clock += 1;
            self.entries.insert(
                session,
                Entry {
                    status: after,
                    since: self.clock,
                },
            );
        }
        if after == Status::Done {
            self.completed.insert(session, self.clock);
        } else {
            self.completed.remove(&session);
        }
        if record.is_none() {
            self.transitioned.remove(&session);
        }
        match record {
            Some(Reported::Working) => {
                outcome.began = self.in_turn.insert(session);
            }
            Some(Reported::Blocked(_)) => {}
            Some(Reported::Done | Reported::Idle | Reported::Error) => {
                outcome.ended = self.in_turn.remove(&session);
            }
            None => {
                self.in_turn.remove(&session);
            }
        }
        outcome
    }

    /// A session takes over a terminal whose root record already stands —
    /// a tab rebound to another session (M13.3). The record is a fact, not a
    /// transition: no completion, no unread mark, no sound and no turn; a
    /// `done` shows as rest, and a turn in progress goes on.
    pub fn seed(&mut self, session: SessionId, record: Option<Reported>) {
        self.forget(session);
        let Some(record) = record else {
            return;
        };
        self.reported.insert(session, record);
        if record == Reported::Working {
            self.in_turn.insert(session);
        }
        let status = match record {
            Reported::Idle | Reported::Done => return,
            Reported::Working => Status::Working,
            Reported::Blocked(kind) => Status::Waiting(why(kind)),
            Reported::Error => Status::Failed,
        };
        self.clock += 1;
        self.entries.insert(
            session,
            Entry {
                status,
                since: self.clock,
            },
        );
    }

    /// A session's root OSC 7501 record, if it has one.
    pub fn reported(&self, session: SessionId) -> Option<Reported> {
        self.reported.get(&session).copied()
    }

    /// Unread first completions, identified by the board's transition clock.
    pub fn done_completions(&self) -> DoneCompletions {
        self.entries
            .iter()
            .filter_map(|(session, entry)| {
                (entry.status == Status::Done && self.completed.get(session) == Some(&entry.since))
                    .then_some((*session, entry.since))
            })
            .collect()
    }

    /// The session's tab is in front of a focused window: a finished turn
    /// is read and shows as rest; a question or an error stays.
    pub fn seen(&mut self, session: SessionId) -> bool {
        let before = self.status(session);
        let after = before.next(Signal::Seen);
        if after == before {
            return false;
        }
        if after == Status::Idle {
            self.entries.remove(&session);
        } else {
            self.clock += 1;
            self.entries.insert(
                session,
                Entry {
                    status: after,
                    since: self.clock,
                },
            );
        }
        true
    }

    /// Independent activity counts in a supplied scope, once per identity.
    pub fn summary(&self, sessions: impl IntoIterator<Item = SessionId>) -> Activity {
        let mut activity = Activity::default();
        let mut seen = std::collections::HashSet::new();
        for session in sessions {
            if !seen.insert(session) {
                continue;
            }
            match self.status(session) {
                Status::Waiting(_) => activity.waiting += 1,
                Status::Failed => activity.failed += 1,
                Status::Working => activity.working += 1,
                Status::Done => activity.unread += 1,
                Status::Idle => activity.idle += 1,
            }
        }
        activity
    }

    /// What a session is doing; idle if nothing has said otherwise.
    pub fn status(&self, session: SessionId) -> Status {
        self.entries
            .get(&session)
            .map_or(Status::Idle, |entry| entry.status)
    }

    /// A CLI status distinguishes silence from an observed idle process.
    pub fn observed_status(&self, session: SessionId) -> &'static str {
        if self.reported.contains_key(&session) {
            crate::command::status_word(self.status(session))
        } else {
            "unknown"
        }
    }

    /// Last observed state change. Restored sessions have no invented timestamp.
    pub fn transitioned_at(&self, session: SessionId) -> Option<std::time::SystemTime> {
        self.transitioned.get(&session).map(|(at, _)| *at)
    }

    /// `open`, the latest transition first; a session with none observed
    /// goes last. Two transitions in one clock tick keep the order they were
    /// observed in.
    pub fn timeline(&self, open: &[SessionId]) -> Vec<SessionId> {
        let mut line = open.to_vec();
        line.sort_by_key(|id| std::cmp::Reverse(self.transitioned.get(id).copied()));
        line
    }

    /// Attention order shared by the navigator and Cmd+Shift+U.
    pub fn attention_order(&self, open: &[SessionId]) -> Vec<SessionId> {
        let mut line: Vec<_> = open
            .iter()
            .filter_map(|&session| {
                let entry = self.entries.get(&session)?;
                Some((entry.status.need()?, entry.since, session))
            })
            .collect();
        line.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        line.dedup_by_key(|(_, _, session)| *session);
        line.into_iter().map(|(_, _, session)| session).collect()
    }

    /// Drop a session that is gone.
    pub fn forget(&mut self, session: SessionId) {
        self.entries.remove(&session);
        self.transitioned.remove(&session);
        self.completed.remove(&session);
        self.reported.remove(&session);
        self.in_turn.remove(&session);
    }

    /// Keep only the sessions `keep` says yes to; answers whether any
    /// went. A terminal session's process is its tab's, so a session
    /// whose tab has closed is nobody's to wait for.
    pub fn retain(&mut self, keep: impl Fn(SessionId) -> bool) -> bool {
        self.transitioned.retain(|session, _| keep(*session));
        self.completed.retain(|session, _| keep(*session));
        self.reported.retain(|session, _| keep(*session));
        self.in_turn.retain(|session| keep(*session));
        let before = self.entries.len();
        self.entries.retain(|session, _| keep(*session));
        self.entries.len() != before
    }

    /// The most urgent status among `sessions` — what a project's tab
    /// shows for all of its sessions at once.
    pub fn most_urgent(&self, sessions: impl IntoIterator<Item = SessionId>) -> Status {
        sessions
            .into_iter()
            .map(|session| self.status(session))
            .max_by_key(|status| status.urgency())
            .unwrap_or_default()
    }

    /// How many are blocked, finished and working.
    pub fn counts(&self) -> Counts {
        let mut counts = Counts::default();
        for entry in self.entries.values() {
            match entry.status.need() {
                Some(Need::Blocked) => counts.blocked += 1,
                Some(Need::Done) => counts.done += 1,
                Some(Need::Failed) => counts.failed += 1,
                None => {}
            }
            if entry.status.working() {
                counts.working += 1;
            }
        }
        counts
    }

    /// The session to go to after `current`, among those with a tab open.
    ///
    /// The blocked first and then the finished, each the longest waiting
    /// first; from the last it wraps to the first, and from a session
    /// that is not in the line it starts at the front. `None` when nobody
    /// with a tab is waiting.
    pub fn next_waiting(
        &self,
        current: Option<SessionId>,
        open: &[SessionId],
    ) -> Option<SessionId> {
        let line = self.attention_order(open);

        let at = current.and_then(|current| line.iter().position(|session| *session == current));
        let next = at.map_or(0, |at| (at + 1) % line.len());
        line.get(next).copied()
    }
}

/// Activity counts are independent of attention navigation's urgency order.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Activity {
    /// Unanswered requests.
    pub waiting: usize,
    /// Observed execution errors.
    pub failed: usize,
    /// Turns in progress.
    pub working: usize,
    /// Finished unread answers.
    pub unread: usize,
    /// Sessions with no current activity.
    pub idle: usize,
}

/// One compact count beside a row's primary mark.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Count {
    /// The state counted, which the window colours like that state's mark.
    pub status: Status,
    /// Its sign and number, `9+` above nine.
    pub text: String,
}

impl Activity {
    /// Waiting, failure, work, unread, then idle.
    pub fn primary(self) -> Status {
        if self.waiting > 0 {
            Status::Waiting(Why::Permission)
        } else if self.failed > 0 {
            Status::Failed
        } else if self.working > 0 {
            Status::Working
        } else if self.unread > 0 {
            Status::Done
        } else {
            Status::Idle
        }
    }

    /// A removal guard independent of primary priority.
    pub fn busy(self) -> bool {
        self.waiting > 0 || self.working > 0
    }

    /// The states that ask for the person, beside the primary mark and not
    /// repeating it: waiting, failed, then unread. Work in progress asks
    /// nothing, so it is never counted here; the exact values, working
    /// included, remain available in disclosure.
    pub fn secondary(self) -> Vec<Count> {
        let primary = self.primary();
        [
            (Status::Waiting(Why::Permission), "?", self.waiting),
            (Status::Failed, "!", self.failed),
            (Status::Done, "●", self.unread),
        ]
        .into_iter()
        .filter(|(status, _, count)| *count > 0 && *status != primary)
        .map(|(status, mark, count)| Count {
            status,
            text: format!(
                "{mark}{}",
                if count > 9 {
                    "9+".into()
                } else {
                    count.to_string()
                }
            ),
        })
        .collect()
    }
}
