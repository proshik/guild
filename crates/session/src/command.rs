//! What the `guild` command asks the window, and what the window says
//! back — M13.
//!
//! **The vocabulary lives here because both ends must mean the same thing
//! by it.** The window is a GPUI application and the command is a binary
//! that must start in two milliseconds; they share these types and
//! nothing else, which is decision 1 of M13's plan. A field added on one
//! side and forgotten on the other is the whole class of bug this module
//! exists to make impossible.
//!
//! The wire is JSON over the window's own Unix socket, one request and
//! one answer, and the shapes are tagged: a command Guild does not know
//! is a command from a newer `guild` binary, and it is refused by name
//! rather than mis-read as a neighbour.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Configured development process through this window's sole coordinator.
    Workflow {
        /// Typed version-one operation.
        request: crate::workflow::WorkflowCommand,
        /// Calling session, resolved against actual window-owned bindings.
        caller: Option<String>,
    },
    /// Explicit terminal program controls, separate from literal overlays.
    Terminal {
        /// Typed action and descriptor.
        request: crate::terminal_job::TerminalCommand,
        /// Calling session, checked by the window.
        caller: Option<String>,
    },

    /// Manage a presentation over the calling session; never controls a process.
    Overlay {
        /// Open, show, hide, close or list.
        operation: String,
        /// Stable presentation identity (empty for list).
        id: String,
        /// Heading on open; ignored by other operations.
        title: String,
        /// Literal text on open; no executable content.
        text: String,
        /// Session asking, resolved against the window's own records.
        caller: Option<String>,
    },
    /// What the window is holding: its projects, their worktrees, and
    /// which of them is in front.
    ///
    /// **The one that reads and cannot break anything**, which is what
    /// makes it the first: a person or an agent can run it to find out
    /// whether there is a window listening at all.
    Status,
    /// Every worktree of every repository the window has a root in, as git
    /// lists them — opened or not.
    WorktreeList,
    /// Make a checkout a root the window holds.
    ///
    /// **Not a creation.** A create that finds the worktree already there
    /// refuses and sends the caller to `open` instead, and that is the
    /// right split — a name that exists is never a reason to
    /// invent another. This is the column's unopened row, clicked.
    WorktreeOpen {
        /// The checkout's folder.
        path: PathBuf,
        /// The workspace to add the root to. Absent on the pre-M12.5 wire.
        #[serde(default)]
        workspace: Option<String>,
        /// The session asking, used to inherit its workspace.
        #[serde(default)]
        caller: Option<String>,
    },
    /// Make a new worktree on a new branch of that name, beside the
    /// repository — the branch popup's own path.
    ///
    /// **Answered when git has answered**, not when the write was queued:
    /// an agent that is told *created* goes on to start a session in the
    /// folder, and a folder that is not there yet is a session that fails
    /// in a way nothing explains. git's refusal comes back in git's words.
    WorktreeCreate {
        /// A folder inside a root of the repository to make it from —
        /// usually wherever the caller is standing.
        from: PathBuf,
        /// The name: the branch as typed, and the folder's suffix.
        name: String,
        /// Where the new branch starts — `--base`. Absent, the window
        /// starts it at the repository's main branch, and at `from`'s
        /// `HEAD` only when there is none — M13.1; absent on the old wire,
        /// which meant `HEAD`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
        /// The task a session is started with in the new worktree once git
        /// has made it — M13.1, decision 3: a worktree made for a task and a
        /// session doing it are one command, so the second cannot be
        /// forgotten.
        /// `None` makes the worktree alone.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// The session asking, from `GUILD_SESSION`, as on
        /// [`Command::SessionNew`]: decision 5 of M13 governs the task.
        #[serde(default)]
        caller: Option<String>,
        /// Bring the session to the front, as on [`Command::SessionNew`].
        #[serde(default)]
        front: bool,
        /// The workspace to put the session in, as on
        /// [`Command::SessionNew`].
        #[serde(default)]
        workspace: Option<String>,
        /// Which harness runs the task — absent means Claude Code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<crate::HarnessKind>,
    },
    /// Remove a worktree the window's column lists — M13 task 8.
    ///
    /// **The window's own guards, and three a sheet does not need.** The
    /// primary, an agent at work, a lock and changes are refused as the
    /// column refuses them; a session a person opened and a file with
    /// unsaved text are refused because an agent may not close what the
    /// person is using; and a session started with a task may not remove a
    /// worktree at all. What git deletes without a word — the ignored files
    /// — is named, and removed only when `delete_ignored` says they may go.
    WorktreeRemove {
        /// The worktree's folder.
        path: PathBuf,
        /// Whether the ignored files in it may be deleted with it.
        delete_ignored: bool,
        /// The session asking, from its `GUILD_SESSION`; `None` from a shell
        /// Guild did not start.
        caller: Option<String>,
    },
    /// Start an agent session in a worktree, in a tab of the window —
    /// the point of M13: the person sees the agent another agent started.
    SessionNew {
        /// The checkout it runs in. Made a root first if it is not one.
        worktree: PathBuf,
        /// What it is asked first. **A session with a prompt is a task**,
        /// and decision 5 of M13's plan governs who may start one.
        prompt: Option<String>,
        /// The session asking, from `GUILD_SESSION` — `None` when the
        /// command was typed by a person rather than run by an agent.
        caller: Option<String>,
        /// Bring it to the front. Off by default: an agent that starts
        /// three sessions must not pull the person away three times.
        front: bool,
        /// The workspace to put the session in. Absent on the old wire.
        #[serde(default)]
        workspace: Option<String>,
        /// Which harness to run — absent means Claude Code, which is the
        /// default and what the old wire says.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<crate::HarnessKind>,
    },
    /// Start an agent session in a project group's folder — GLD-055.
    ///
    /// **A variant of its own, not an optional field on `SessionNew`**:
    /// `Command` does not deny unknown fields, so a window built before it
    /// would drop a `group` field and start the session in `worktree`.
    /// This way an older window refuses it by name. A door to the same deed
    /// as the `+` on the folder's row; it never binds a folder.
    SessionNewInGroup {
        /// The group, by its name as the column shows it.
        group: String,
        /// What it is asked first; a session with a prompt is a task.
        prompt: Option<String>,
        /// The session asking, from `GUILD_SESSION`.
        caller: Option<String>,
        /// Bring it to the front.
        front: bool,
        /// The workspace whose groups are searched.
        #[serde(default)]
        workspace: Option<String>,
        /// Which harness to run; absent means Claude Code.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        harness: Option<crate::HarnessKind>,
    },
    /// How project groups stand against the filesystem — GLD-055: the read
    /// the column's match line does.
    GroupCheck {
        /// One group by name; every group of the workspace when absent.
        #[serde(default)]
        group: Option<String>,
        /// The workspace whose groups are read.
        #[serde(default)]
        workspace: Option<String>,
        /// The session asking, from `GUILD_SESSION`.
        #[serde(default)]
        caller: Option<String>,
    },
    /// Bring a closed session back into its tab — M13.3.
    ///
    /// **A door to the Sessions panel's Reopen**, `reopen_with`: the
    /// session's own harness resumes its own conversation, and the record
    /// keeps what it was — a task comes back a task. Behind the person's
    /// work unless `front`. Refused when the caller is a task and the
    /// session is one (decision 35: bringing a task back is starting it),
    /// and when the conversation is open in a tab or held by a process.
    SessionResume {
        /// The session, by the id `guild agent list` prints.
        session: String,
        /// The session asking, from its `GUILD_SESSION`.
        #[serde(default)]
        caller: Option<String>,
        /// Bring its tab to the front.
        #[serde(default)]
        front: bool,
    },
    /// Reconcile a managed task for safe same-conversation startup or qualified Codex input.
    SessionResumeTask {
        /// Window-owned task record.
        session: String,
        /// Stable controller-scoped idempotency key.
        request_id: String,
        /// Validated startup text.
        prompt: String,
        /// None retains the external operator route.
        caller: Option<String>,
        /// Bring the tab forward only when explicitly requested.
        front: bool,
    },
    /// Read a durable startup continuation receipt without replaying it.
    SessionContinuationStatus {
        /// Window-owned task record.
        session: String,
        /// Original key for receipt lookup; absent requests read-only runtime preflight.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
        /// Same controller namespace as the start request.
        caller: Option<String>,
    },
    /// List the desk's named workspaces.
    WorkspaceList,
    /// Add a named workspace.
    WorkspaceCreate {
        /// Its person-visible name.
        name: String,
        /// An optional named Lucide icon.
        icon: Option<String>,
        /// Select it after creation.
        select: bool,
    },
    /// Select a workspace by its stable identifier.
    WorkspaceSelect {
        /// The identifier as printed by [`Command::WorkspaceList`].
        workspace: String,
    },
    /// Every session the window holds, with what its agent is doing.
    AgentList,
    /// Type a message into a task's prompt and press Return — M13 task 9.
    ///
    /// **Answered when the turn has begun** — the session's program reports
    /// `working` through OSC 7501 — so a `guild agent wait` straight after
    /// does not return on the turn before.
    AgentSend {
        /// The session to send to.
        session: String,
        /// What to type — any length, any number of lines.
        text: String,
        /// The session asking, from its `GUILD_SESSION`.
        caller: Option<String>,
    },
    /// End a session the command line started.
    ///
    /// **Only a task.** A session a person opened is theirs, and an agent
    /// that could close it could close the conversation the person is in
    /// the middle of; a session started with a task is the command line's
    /// to end — in whatever tab it was continued, decision 52. The tab
    /// closes and its process goes with it.
    AgentStop {
        /// The session, by its id.
        session: String,
    },
}

/// What the window answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "answer", rename_all = "snake_case")]
pub enum Answer {
    /// Durable workflow state or a named execution gap.
    Workflow {
        /// Typed protocol response.
        reply: crate::workflow::WorkflowReply,
    },
    /// Process launch knowledge, not a model execution acknowledgement.
    SessionContinuation {
        /// Durable request-scoped receipt.
        receipt: Box<crate::resume_handoff::Receipt>,
    },
    /// Read-only task readiness, separate from historical activity or delivery.
    SessionContinuationStatus {
        /// Exact window-owned identity and observed limitation.
        observation: Box<crate::resume_handoff::Observation>,
    },
    /// Terminal job state, not a review verdict.
    Terminal {
        /// Typed protocol response.
        reply: crate::terminal_job::TerminalReply,
    },
    /// Presentation records owned by the calling session.
    Overlays {
        /// Includes minimized presentations.
        overlays: Vec<crate::overlay::Overlay>,
    },
    /// It was done, and there is nothing to say about it.
    Done,
    /// It was not done, in words a person can act on.
    ///
    /// **Not an exit code with a number.** Every refusal in this window
    /// already has a sentence — git's own words, or the guard's — and a
    /// command that turned those into `1` would be throwing away the only
    /// part an agent can do anything with.
    Refused {
        /// Why, in the words the window would have shown a person.
        because: String,
    },
    /// The window, as [`Command::Status`] asks for it.
    Status {
        /// Every root the window has open.
        projects: Vec<Project>,
    },
    /// The worktrees, as [`Command::WorktreeList`] asks for them.
    Worktrees {
        /// Every worktree git lists, the primary of each repository first.
        worktrees: Vec<Worktree>,
    },
    /// A root the window now holds, as opening one answers.
    Opened {
        /// Its folder.
        path: PathBuf,
    },
    /// A worktree git has made, as [`Command::WorktreeCreate`] answers.
    Created {
        /// Its folder.
        path: PathBuf,
        /// The branch it is on.
        branch: String,
        /// What the branch started from; `None` is the `HEAD` it was made
        /// from — and every answer from a window before M13.1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        base: Option<String>,
        /// The session started in it with its task, when one was asked
        /// for — M13.1.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        session: Option<String>,
    },
    /// A message a session has begun a turn on, as [`Command::AgentSend`]
    /// answers.
    Sent {
        /// The session.
        session: String,
    },
    /// A worktree git has removed, as [`Command::WorktreeRemove`] answers —
    /// sent once the column's own list no longer has it.
    Removed {
        /// The folder that is gone.
        path: PathBuf,
    },
    /// The sessions, as [`Command::AgentList`] asks for them.
    Agents {
        /// Every session the window holds.
        agents: Vec<Agent>,
    },
    /// A session the window has started, as [`Command::SessionNew`] answers.
    SessionStarted {
        /// Its identifier — the one `--session-id` was given, and what
        /// `guild agent read|wait|stop` will take.
        session: String,
        /// The checkout it runs in.
        worktree: PathBuf,
    },
    /// A closed session back in a tab, as [`Command::SessionResume`]
    /// answers.
    Resumed {
        /// Its identifier, unchanged.
        session: String,
        /// The checkout its tab is in.
        worktree: PathBuf,
    },
    /// Project groups against the filesystem, as [`Command::GroupCheck`]
    /// answers.
    Groups {
        /// The groups read, in the order the workspace keeps them — the
        /// order they were made, not the column's, which a sort mode moves.
        groups: Vec<GroupCheck>,
    },
    /// Named workspaces in switcher order.
    Workspaces {
        /// The rows the switcher holds.
        workspaces: Vec<WorkspaceRow>,
    },
    /// A workspace was added.
    WorkspaceCreated {
        /// Its stable identifier.
        id: String,
        /// Its trimmed person-visible name.
        name: String,
    },
    /// A workspace was selected.
    WorkspaceSelected {
        /// Its stable identifier.
        id: String,
    },
}

/// One project group against the filesystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupCheck {
    /// The group's name.
    pub name: String,
    /// The workspace it is in, by id.
    pub workspace: String,
    /// The folder it is bound to; `None` for a group without one.
    pub folder: Option<PathBuf>,
    /// Whether every member's main checkout is a direct child of the
    /// folder; `None` without a folder.
    pub matches: Option<bool>,
    /// Each member.
    pub members: Vec<GroupMember>,
}

/// One member of a project group against its folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    /// Its main checkout.
    pub checkout: PathBuf,
    /// Whether that is a direct child of the group's folder; `None` without
    /// a folder.
    pub inside: Option<bool>,
}

/// One workspace as the command line sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceRow {
    /// Stable identifier.
    pub id: String,
    /// Person-visible name.
    pub name: String,
    /// Optional named icon.
    pub icon: Option<String>,
    /// Whether it is in front.
    pub selected: bool,
    /// Roots held by this workspace, in sidebar order.
    pub projects: Vec<PathBuf>,
}

/// One session, as the command line sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    /// Its id — what `--session-id` was given.
    pub session: String,
    /// The harness the window launched; absent from older windows.
    #[serde(default)]
    pub harness: Option<crate::HarnessKind>,
    /// Exact harness conversation, resolved by the window, never by cwd.
    #[serde(default)]
    pub conversation: Option<String>,
    /// The root it runs in.
    pub worktree: PathBuf,
    /// The workspace owning the session.
    #[serde(default)]
    pub workspace: String,
    /// What its agent is doing: `idle`, `working`, `waiting`, `done`, `failed`, or
    /// `unknown` when its program has not reported its state (OSC 7501).
    pub status: String,
    /// Why a `waiting` session waits — `permission`, `question` or `auth` —
    /// only when its program said so (OSC 7501 `kind`); absent otherwise,
    /// and from a window built before it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    /// Whether a turn has happened in it at all.
    pub turned: bool,
    /// Started by the command line with a task.
    pub task: bool,
    /// Shown as a tab now, rather than closed and kept for `--resume`.
    pub open: bool,
    /// A terminal whose process has ended while its tab is open — M5.12.
    ///
    /// **`serde(default)` so that a `guild` built after M5.12 reads a
    /// window built before it**, which sends no such field; absent reads as
    /// alive. The other direction needs nothing: serde ignores a field it
    /// does not know, and this type does not deny unknown fields.
    #[serde(default)]
    pub exited: bool,
    /// Held by a tab of this window. A project's state is shared by every
    /// window that opens its folder, so an `open` record may be open in
    /// another window, or in a workspace this window does not have.
    /// **Defaults to `true`** for a window built before it, which held every
    /// open record it listed in a tab or in a restore about to make one.
    #[serde(default = "held_by_default")]
    pub tab: bool,
}

fn held_by_default() -> bool {
    true
}

/// Why a waiting session waits, on the wire, when its program said why.
pub fn kind_word(status: crate::status::Status) -> Option<&'static str> {
    use crate::status::{Status, Why};
    match status {
        Status::Waiting(Why::Permission) => Some("permission"),
        Status::Waiting(Why::Question) => Some("question"),
        Status::Waiting(Why::Auth) => Some("auth"),
        _ => None,
    }
}

/// The word for a status on the wire — M13.
///
/// Words and not the enum, because a caller in any language reads them and
/// a new state inside the window must not break a `guild` built before it.
pub fn status_word(status: crate::status::Status) -> &'static str {
    use crate::status::Status;
    match status {
        Status::Idle => "idle",
        Status::Working => "working",
        Status::Waiting(_) => "waiting",
        Status::Done => "done",
        Status::Failed => "failed",
    }
}

/// Whether a session a caller is waiting on has finished what it was doing —
/// M13, for `guild agent wait`.
///
/// **`idle` alone is not finished.** A session just started with a prompt
/// reads `unknown` or `idle` until its program reports its first turn, so waiting for
/// *not working* returns before the agent has done anything. Finished is a
/// turn that ended (`done`), an agent that stopped to ask the person
/// (`waiting` — nothing more will happen without them), an observed failure
/// (`failed`), or `idle` once a turn
/// has happened, which is a `done` the person has already looked at.
pub fn settled(status: &str, turned: bool) -> bool {
    match status {
        "done" | "waiting" | "failed" => true,
        "idle" => turned,
        _ => false,
    }
}

/// How a wait on a session stands — M5.12, for `guild agent wait`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waited {
    /// What it was doing is over: [`settled`].
    Finished,
    /// Its tab was closed, so nothing more will happen in it.
    Closed,
    /// Its process ended while its tab is open.
    Exited,
    /// Still at it: ask again.
    Going,
}

/// Where a wait on this agent stands.
///
/// **Only an open tab with a live process can finish** — decision 16 of
/// M5.12. A closed tab is [`Waited::Closed`] even after a finished turn,
/// because closing marks the turn as soon as the transcript holds a record,
/// and `idle` with `turned` would read as a result nobody wrote; an ended
/// process is [`Waited::Exited`] whatever the board last heard, because no
/// hook will ever move it again. Closed is asked first: a closed tab's
/// process has ended too, and *closed* is the word for why.
pub fn waited(agent: &Agent) -> Waited {
    if !agent.open {
        Waited::Closed
    } else if agent.exited {
        Waited::Exited
    } else if settled(&agent.status, agent.turned) {
        Waited::Finished
    } else {
        Waited::Going
    }
}

/// One worktree, as the command line sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Worktree {
    /// Its folder.
    pub path: PathBuf,
    /// The repository it belongs to, by its common git directory.
    pub repository: PathBuf,
    /// Its branch; `None` when detached.
    pub branch: Option<String>,
    /// The repository's own checkout, which git will not remove.
    pub primary: bool,
    /// Whether the window holds it as a root.
    pub opened: bool,
    /// Why it is locked, when it is — git's own words.
    pub locked: Option<String>,
}

/// One open root, as the command line sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// Its folder.
    pub path: PathBuf,
    /// What the window calls it.
    pub name: String,
    /// The repository it belongs to, by the git directory its worktrees
    /// share; `None` for a directory outside git.
    pub repository: Option<PathBuf>,
    /// Whether it is the root in front.
    pub active: bool,
    /// How many sessions it holds.
    pub sessions: usize,
    /// Workspaces holding this root.
    #[serde(default)]
    pub workspaces: Vec<String>,
}

/// Whether a session may not be started — decision 5 of M13's plan.
///
/// **A session started with a task may not start another with a task.**
/// An agent that may start agents will start them for its own convenience,
/// and parallelism, isolation and *it would be faster* are not the person
/// asking; the window cannot see intent, so it checks provenance. A
/// session without a prompt is not a task — it is a tab a person will type
/// into — and anyone may open one; a caller that is not a task, which
/// includes a person at a shell, may start a task.
///
/// `caller_is_task` is whether the asking session was itself started with
/// a task; a caller the window does not know is not one.
pub fn refuses_session(prompt: bool, caller_is_task: bool) -> bool {
    prompt && caller_is_task
}

/// What a worktree made with its task answers, once the session has been
/// asked for — M13.1.
///
/// A started session joins the worktree's answer. **A refused one says the
/// worktree is there**: the caller was about to be told only *refused*, and
/// would make the worktree again and meet git's *already exists* — while
/// the checkout it asked for sits in the column with nobody in it.
pub fn with_session(
    path: PathBuf,
    branch: String,
    base: Option<String>,
    started: Answer,
) -> Answer {
    match started {
        Answer::SessionStarted { session, .. } => Answer::Created {
            path,
            branch,
            base,
            session: Some(session),
        },
        Answer::Refused { because } => Answer::Refused {
            because: format!(
                "created {} on branch {branch}, but its session did not start: {because} — \
                 start it with `guild session new --worktree {}`",
                path.display(),
                path.display()
            ),
        },
        other => other,
    }
}

/// Shared public CLI capability/discovery guidance for managed terminal sessions.
/// Claude and Codex receive additive SessionStart context; OpenCode receives
/// additive context through a per-launch plugin. This is separate from user
/// tasks and built-in/user/project instructions, and selects no workflow.
pub const SESSION_PROMPT: &str = "Guild exposes public commands through `guild` on PATH. GUILD_SOCKET names the owning window and GUILD_SESSION identifies this Guild session. Read `guild instructions` and `guild help` for public APIs; supported commands offer --json output.";

/// Why a message may not be sent to a session, or `None` — M13 task 9.
///
/// **A send is typing into a prompt**, so it has `agent stop`'s rule and
/// decision 5's: only to a session the command line started, and not from
/// one — a task may not steer another. Whose session it is comes before
/// what state it is in. **And only while the session is reading a prompt**:
/// mid-turn the caller should wait; at a permission or a question a
/// keystroke chooses an option blind, and a permission is the person's to
/// grant; before the first turn the prompt may not be up at all — the trust
/// dialog is there.
///
/// **A session continuing a task's conversation is the task** — decision
/// 52: a tab where a task was resumed, by `guild session resume` or by a
/// person's `/resume`, holds the task's record, and `target_is_task` is
/// asked of that.
pub fn refuses_send(
    target_is_task: bool,
    caller_is_task: bool,
    status: &str,
    turned: bool,
) -> Option<&'static str> {
    if !target_is_task {
        return Some(
            "that session was opened by a person — it is theirs to type into, and the command \
             line speaks only to sessions it started",
        );
    }
    if caller_is_task {
        return Some(
            "a session started with a task may not steer another session — tell the person",
        );
    }
    match status {
        "working" => Some("it is in the middle of a turn — `guild agent wait` it first, then send"),
        "waiting" => {
            Some("it is waiting on a permission or a question — the person answers that in its tab")
        }
        "idle" if !turned => Some("it has not finished its first turn yet — wait for it first"),
        "idle" | "done" | "failed" => None,
        "unknown" => Some(
            "status unavailable: this session has not reported its state (OSC 7501) — \
             `guild agent wait` for its first report, or look at its tab",
        ),
        _ => Some(
            "its current status is unknown — inspect its tab; missing telemetry is not an idle prompt",
        ),
    }
}
