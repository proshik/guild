//! Workspace-local ordering of repository groups, independent of worktree rows.
use serde::{Deserialize, Serialize};

use crate::status::Status;

/// The selected ordering of repository groups.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Projects with a running agent session first — alive in one of
    /// their rows and used since it was opened ([`Running`]) — in the order
    /// they became active, which its turns, questions and rest do not
    /// change ([`Active`]). Below them, newest started turn first; tools
    /// and a turn's end do not reorder.
    RecentActivity,
    /// Most observed activity first.
    MostActivity,
    /// Latest submitted user request first.
    LastUserMessage,
    /// The workspace's saved project order.
    #[default]
    Manual,
}

impl Mode {
    /// Menu order.
    pub const ALL: [Self; 4] = [
        Self::RecentActivity,
        Self::MostActivity,
        Self::LastUserMessage,
        Self::Manual,
    ];
}

/// Activity observed after this feature was installed, in Unix milliseconds.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Saved")]
pub struct Activity {
    /// Last execution event; focus and view state do not update this.
    pub last_activity: Option<u64>,
    /// Observed started turns, not tools, commits or messages.
    pub turns: u64,
    /// Only a source-confirmed user prompt, never an inferred execution start.
    pub last_user: Option<u64>,
    /// Last turn the program reported beginning (OSC 7501) — a user prompt
    /// or a turn the agent began itself — which Recent activity sorts by.
    /// Its other reports and the prompt hook do not move it, so a working
    /// agent does not reorder the column.
    pub last_turn: Option<u64>,
}

/// `Activity` as stored, telling a missing `last_turn` from a recorded `null`.
#[derive(Deserialize)]
struct Saved {
    last_activity: Option<u64>,
    turns: u64,
    last_user: Option<u64>,
    #[serde(default, deserialize_with = "present")]
    last_turn: Option<Option<u64>>,
}

fn present<'de, D: serde::Deserializer<'de>>(from: D) -> Result<Option<Option<u64>>, D::Error> {
    Option::<u64>::deserialize(from).map(Some)
}

impl From<Saved> for Activity {
    /// State saved before turn starts were kept dates its last turn once by
    /// its last activity, so an update does not drop every project to the
    /// bottom; from then on only a turn start moves it.
    fn from(saved: Saved) -> Self {
        Self {
            last_activity: saved.last_activity,
            turns: saved.turns,
            last_user: saved.last_user,
            last_turn: saved.last_turn.unwrap_or(saved.last_activity),
        }
    }
}

impl Activity {
    /// Combine disjoint worktrees without changing their rows.
    pub fn include(&mut self, other: Self) {
        self.last_activity = self.last_activity.max(other.last_activity);
        self.last_user = self.last_user.max(other.last_user);
        self.last_turn = self.last_turn.max(other.last_turn);
        self.turns = self.turns.saturating_add(other.turns);
    }
}

/// Durable per-session memory of the prompts already dated, so a replayed
/// hook after a restart does not date "last user message" again. The other
/// fields are what the hook-driven turn cursor kept before OSC 7501; they
/// are read and written back unchanged so an older state file round-trips.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub(crate) struct Cursor {
    #[serde(default)]
    started: std::collections::BTreeSet<String>,
    #[serde(default)]
    confirmed: std::collections::BTreeSet<String>,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    current: Option<String>,
    #[serde(default)]
    user_confirmed: bool,
    #[serde(default)]
    runtime: Option<String>,
}

impl crate::Desk {
    /// Activity survives closed sessions and unopened worktree rows.
    pub fn project_activity(&self, path: &std::path::Path) -> Activity {
        self.project_activity.get(path).copied().unwrap_or_default()
    }

    /// What the program reported moved its record: any change is activity,
    /// and a turn that began is one more turn (OSC 7501). Answers whether
    /// anything was recorded.
    pub fn observe_reported_activity(
        &mut self,
        session: crate::SessionId,
        outcome: crate::status::ReportOutcome,
        at: u64,
    ) -> bool {
        if !outcome.changed {
            return false;
        }
        let Some(record) = self
            .session_mut(session)
            .filter(|record| record.runs.is_agent())
        else {
            return false;
        };
        record.last_activity = record.last_activity.max(Some(at));
        let cwd = record.cwd.clone();
        let activity = self.project_activity.entry(cwd).or_default();
        activity.last_activity = activity.last_activity.max(Some(at));
        if outcome.began {
            activity.turns = activity.turns.saturating_add(1);
            activity.last_turn = activity.last_turn.max(Some(at));
        }
        true
    }

    /// A prompt the person submitted, by the harness's own
    /// `UserPromptSubmit` hook: the only witness of authorship there is —
    /// OSC 7501 says a turn began, not who began it. Once per prompt or turn
    /// it names, across restarts; OpenCode's execution start is never one.
    pub fn confirm_user_prompt(
        &mut self,
        session: crate::SessionId,
        payload: &serde_json::Value,
        at: u64,
    ) -> bool {
        if payload["hook_event_name"] != "UserPromptSubmit" {
            return false;
        }
        let Some(record) = self.session(session).filter(|record| {
            record.runs.is_agent() && record.runs.harness() != Some(crate::HarnessKind::OpenCode)
        }) else {
            return false;
        };
        let path = record.cwd.clone();
        let conversation = record
            .harness_session
            .clone()
            .unwrap_or_else(|| session.to_string());
        let turn = ["turn_id", "prompt_id"]
            .into_iter()
            .find_map(|key| payload[key].as_str());
        if let Some(turn) = turn {
            let cursor = self
                .activity_cursors
                .entry(session.to_string())
                .or_default();
            if !cursor.confirmed.insert(format!("{conversation}:{turn}")) {
                return false;
            }
        }
        let activity = self.project_activity.entry(path).or_default();
        activity.last_user = activity.last_user.max(Some(at));
        true
    }
}

/// Which agent sessions keep their project in Recent activity's active
/// block: the ones running — alive, its tab open and its program not
/// exited, and used since it was opened: it has worked, waited for the
/// person or finished a turn. Rest between turns keeps it running, a
/// finished turn already seen included; a failure, an exit and a closed
/// tab end it, and only its next turn makes it running again.
///
/// In memory only: after a restart a session runs again once it reports
/// a turn.
#[derive(Debug, Clone)]
pub struct Running<S> {
    used: std::collections::HashSet<S>,
}

impl<S> Default for Running<S> {
    fn default() -> Self {
        Self {
            used: std::collections::HashSet::new(),
        }
    }
}

impl<S: std::hash::Hash + Eq + Copy> Running<S> {
    /// Every agent session the window holds open on this pass: whether it
    /// is alive, and its status now. A session left out is gone. Answers
    /// the sessions running.
    pub fn update(
        &mut self,
        sessions: impl IntoIterator<Item = (S, bool, Status)>,
    ) -> std::collections::HashSet<S> {
        let mut used = std::collections::HashSet::new();
        for (session, alive, status) in sessions {
            let keeps = match status {
                Status::Working | Status::Waiting(_) | Status::Done => true,
                Status::Idle => self.used.contains(&session),
                Status::Failed => false,
            };
            if alive && keeps {
                used.insert(session);
            }
        }
        self.used = used;
        self.used.clone()
    }
}

/// When each active project became active, by the window's own counter.
///
/// A project is active while one of its sessions is [`Running`]. A key is
/// given its moment on the pass it turns up after being absent and loses
/// it on the pass it is absent again, so what its sessions do while one
/// runs keeps its place, and a project that stops and comes back goes to
/// the end. Keys new in one pass share a moment: their order is left to
/// the caller's stable sort. A counter rather than a clock, like the
/// status board's, and never persisted — after a restart the programs
/// report again and their projects take places in that order.
#[derive(Debug, Clone)]
pub struct Active<K> {
    since: std::collections::HashMap<K, u64>,
    clock: u64,
}

impl<K> Default for Active<K> {
    fn default() -> Self {
        Self {
            since: std::collections::HashMap::new(),
            clock: 0,
        }
    }
}

impl<K: std::hash::Hash + Eq> Active<K> {
    /// The keys active now, in any order and with repeats; answers whether
    /// any key became active or stopped being active.
    pub fn update(&mut self, active: impl IntoIterator<Item = K>) -> bool {
        let now: std::collections::HashSet<K> = active.into_iter().collect();
        let before = self.since.len();
        self.since.retain(|key, _| now.contains(key));
        let mut changed = self.since.len() != before;
        let moment = self.clock + 1;
        for key in now {
            if let std::collections::hash_map::Entry::Vacant(slot) = self.since.entry(key) {
                slot.insert(moment);
                changed = true;
            }
        }
        if changed {
            self.clock = moment;
        }
        changed
    }

    /// When `key` became active; `None` while it is not.
    pub fn since(&self, key: &K) -> Option<u64> {
        self.since.get(key).copied()
    }
}
