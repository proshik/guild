//! Shared conversation identity, restoration and launch provenance.

use crate::desk::Runs;
use std::path::PathBuf;

/// A new conversation or the continuation of an existing one.
///
/// There is an identifier in both cases, because we assign it: the only
/// difference is the flag it is passed to `claude` with. While the id was
/// learned from the first event, "new" also meant "we do not know which" —
/// and too much depended on that distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conversation {
    /// A new conversation with an assigned id: `--session-id`.
    New(String),
    /// A return to an existing one: `--resume`.
    Resume(String),
}

/// What can be done with a saved session.
///
/// There are three cases rather than two, and the third one cost us a dead
/// tab. `claude` creates the transcript file as soon as a session starts,
/// before the first message. It considers such an id taken (`Error:
/// Session ID … is already in use`), yet there is nothing to continue
/// (`No conversation found with session ID`). Verified with both flags on
/// a live run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Continuation {
    /// There is a conversation — continue with `--resume`.
    Resume,
    /// The id is free — start with `--session-id`.
    Start,
    /// The id is taken but there is no conversation: nothing to continue
    /// and nothing to start. The record is dead; forget it and create a
    /// new session.
    Impossible,
}

/// What to do with a saved session.
///
/// The disk decides, not our "a turn happened" flag: the flag is set while
/// the application is watching, and the window may have been closed
/// earlier.
pub fn continuation(conversation_exists: bool, id_taken: bool) -> Continuation {
    match (conversation_exists, id_taken) {
        (true, _) => Continuation::Resume,
        (false, true) => Continuation::Impossible,
        (false, false) => Continuation::Start,
    }
}

/// How a saved record comes back into a tab — after a restart, or when a
/// closed one is reopened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resumption {
    /// Run the harness with this conversation.
    Continue(Conversation),
    /// A shell: a new one in the same directory. There is nothing to
    /// continue — the process did not outlive its tab.
    Fresh,
    /// The harness identity was never recorded; keep the tab without launching.
    Unidentified,
    /// Nothing can revive it: forget the record.
    Forget,
}

/// How the record `id`, running `runs`, comes back.
///
/// **Each harness is asked by its own id.** Claude's record id *is* its
/// conversation, and Claude's transcripts decide through `claude`, which
/// is only called for a Claude record. OpenCode and Codex generate their
/// own id. A missing identity cannot prove that no conversation happened:
/// keep the record and let the window explain why it cannot resume.
pub fn resumption(
    runs: Runs,
    id: &str,
    harness_session: Option<&str>,
    claude: impl FnOnce() -> Continuation,
) -> Resumption {
    match runs {
        Runs::Claude(_) => match claude() {
            Continuation::Resume => Resumption::Continue(Conversation::Resume(id.to_owned())),
            Continuation::Start => Resumption::Continue(Conversation::New(id.to_owned())),
            Continuation::Impossible => Resumption::Forget,
        },
        Runs::OpenCode(_) | Runs::Codex(_) => match harness_session.filter(|id| !id.is_empty()) {
            Some(theirs) => Resumption::Continue(Conversation::Resume(theirs.to_owned())),
            None => Resumption::Unidentified,
        },
        Runs::Shell => Resumption::Fresh,
    }
}

impl Conversation {
    /// The conversation id.
    pub fn id(&self) -> &str {
        match self {
            Self::New(id) | Self::Resume(id) => id,
        }
    }

    /// The command-line flag it is passed with.
    pub fn flag(&self) -> &'static str {
        match self {
            Self::New(_) => "--session-id",
            Self::Resume(_) => "--resume",
        }
    }
}

/// The variable a session's hooks find the window's socket by — M5.10.
pub const HOOK_SOCKET_VAR: &str = "GUILD_HOOK_SOCKET";

/// The variable the `guild` command finds the window by — M13.
///
/// **The same socket, a second name.** The hook script and the command
/// line are two readers with two futures: the script is `include_str!`'d
/// into the binary and speaks one shape of request, while the command is
/// a program a person also types. A single name shared by both would make
/// either one's change the other's problem, and the day the socket splits
/// in two, only one of these moves.
pub const SOCKET_VAR: &str = "GUILD_SOCKET";

/// The variable a session knows its own identity by — M13.
///
/// **What lets the window tell who is asking.** `guild session new` from
/// inside a session carries this, and the window checks the record it
/// names: a session that was itself started with a task may not start
/// another — decision 5 of M13's plan. It is the
/// same identifier `--session-id` was given, so there is nothing to look up
/// and nothing to get wrong.
pub const SESSION_VAR: &str = "GUILD_SESSION";

/// The variable Guild's status line finds the person's own by — the
/// `statusLine.command` of their settings, run after the plan's limits are
/// tapped, with the same input. Set only when they have one.
pub const USER_STATUSLINE_VAR: &str = "GUILD_USER_STATUSLINE";

/// What a `claude` is launched with so that its hooks reach the window —
/// M5.10.
///
/// `--settings` adds hooks **beside** the person's own rather than in
/// place of them — measured by M5.10's task 0 — and the variable names the
/// socket they post to. A `claude` Guild did not start has neither, which
/// is the whole of the guard against a worker agent's hooks being read as
/// its parent's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hooks {
    /// The settings file naming the hooks.
    pub settings: PathBuf,
    /// The same hooks, and Guild's status line, which taps the plan's
    /// limits on their way to the person's own.
    pub statusline_settings: PathBuf,
    /// The socket the hooks post to.
    pub socket: PathBuf,
    /// The person's own status line, when this launch found one — the
    /// tap is given only to a person who has a status line to chain to,
    /// so nobody gets a status row they did not have.
    pub user_statusline: Option<String>,
    /// Where a Codex or OpenCode launch writes its agent's status door
    /// before the process starts — GLD-078: an agent never run here gets
    /// its door at its first launch from Guild, not at the window's start.
    /// `None` writes nothing: a window that may not touch the agents'
    /// global files (`GUILD_NO_GLOBAL_HOOKS`, a diagnostic window, a test).
    pub doors: Option<crate::harness::AgentDirs>,
}

impl Hooks {
    /// The arguments that add the hooks: `--settings <file>` — the tap's
    /// file when there is a status line of the person's to chain to.
    pub fn args(&self) -> [std::ffi::OsString; 2] {
        let settings = if self.user_statusline.is_some() {
            &self.statusline_settings
        } else {
            &self.settings
        };
        ["--settings".into(), settings.clone().into_os_string()]
    }
}
