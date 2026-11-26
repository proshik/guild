//! What the status bar's counter counts and lists — M12.2 task 5. Pure.
//!
//! The number at the bottom right counted a project's session records and
//! opened a page, and nobody could say what it meant. It counts the agents
//! open in the window now, and opens a list of them upwards. **The shape is
//! a list of groups from the start**: there was one kind of agent today, and
//! the person had said Open Code would be a second, listed apart under the
//! same total. M14 makes it three, each harness a kind.

use guild_session::{HarnessKind, Runs, SessionId};

/// A kind of agent a tab can hold — one per harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Agent {
    /// Claude Code in a terminal.
    ClaudeCode,
    /// OpenCode.
    OpenCode,
    /// Codex.
    Codex,
}

impl Agent {
    /// The harness this agent kind is.
    pub fn harness(self) -> HarnessKind {
        match self {
            Self::ClaudeCode => HarnessKind::Claude,
            Self::OpenCode => HarnessKind::OpenCode,
            Self::Codex => HarnessKind::Codex,
        }
    }

    /// The mark its tabs wear — the add menu's choice of the same name.
    pub fn mark(self) -> guild_session::DefaultTab {
        match self {
            Self::ClaudeCode => guild_session::DefaultTab::Claude,
            Self::OpenCode => guild_session::DefaultTab::OpenCode,
            Self::Codex => guild_session::DefaultTab::Codex,
        }
    }

    pub fn from_harness(kind: HarnessKind) -> Self {
        match kind {
            HarnessKind::Claude => Self::ClaudeCode,
            HarnessKind::OpenCode => Self::OpenCode,
            HarnessKind::Codex => Self::Codex,
        }
    }
}

/// One agent open in the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Open {
    /// Its session, which is what its tab is found by.
    pub session: SessionId,
    /// What kind of agent it is.
    pub kind: Agent,
    /// What its tab is called.
    pub name: String,
    /// The root it runs in, by name.
    pub project: String,
    /// Its workspace, only when the desk has more than one.
    pub workspace: Option<String>,
}

/// The agents of one kind, in the order their tabs are in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// The kind.
    pub kind: Agent,
    /// The agents.
    pub rows: Vec<Open>,
}

/// What kind of agent a tab running `runs` holds — `None` for a shell,
/// which is a terminal and not an agent.
pub fn kind_of(runs: Runs) -> Option<Agent> {
    runs.harness().map(Agent::from_harness)
}

/// The open agents as groups: one per kind, the kinds in their declared
/// order, each group in the order it was given.
pub fn groups(open: Vec<Open>) -> Vec<Group> {
    let mut groups: Vec<Group> = Vec::new();
    for agent in open {
        match groups.iter_mut().find(|group| group.kind == agent.kind) {
            Some(group) => group.rows.push(agent),
            None => groups.push(Group {
                kind: agent.kind,
                rows: vec![agent],
            }),
        }
    }
    groups.sort_by_key(|group| group.kind);
    groups
}

/// What an agent with no name of its own is called — its tab's `label`
/// (*Session 8*), and, in a tab whose panes show more than one agent
/// (`agents`), its `pane`'s place in the tab after it: *Session 8 · 2*.
/// The place counts from one in draw order, empty panes and shells
/// included — the place the person sees. Window check 3: two agents of
/// one divided tab were both *Session 8*, and only the dot told them
/// apart. An agent with a name of its own — a topic, a title — keeps it.
pub fn unnamed(label: &str, pane: usize, agents: usize) -> String {
    if agents > 1 {
        format!("{label} · {pane}")
    } else {
        label.to_owned()
    }
}

/// How many agents the groups hold — the number on the counter.
pub fn total(groups: &[Group]) -> usize {
    groups.iter().map(|group| group.rows.len()).sum()
}

/// Which harness `args` is the front of a pty — the first harness whose
/// `detect_in_front` answers, or `None` for a shell or anything else.
pub fn which_in_front(args: &str) -> Option<HarnessKind> {
    HarnessKind::ALL
        .into_iter()
        .find(|&kind| kind.impl_().detect_in_front(args))
}

/// Whether a command line is Claude Code — kept for the call-sites that
/// said exactly that. A shell with `claude` typed in front counts, M12.3.
pub fn is_claude_command(args: &str) -> bool {
    which_in_front(args) == Some(HarnessKind::Claude)
}

/// Relative observed activity, never a transcript age or a logical Board counter.
pub fn relative_time(
    at: Option<std::time::SystemTime>,
    now: std::time::SystemTime,
    ui: crate::i18n::Ui,
) -> String {
    let Some(age) = at.and_then(|at| now.duration_since(at).ok()) else {
        return "—".into();
    };
    ui.ago(age.as_secs().min(i64::MAX as u64) as i64)
}
