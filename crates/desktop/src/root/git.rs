//! What the window does with git: a status walk per project on a thread
//! of its own, asked for on events and never waited for, and the map it
//! answers with, which the tree and the Changes panel draw from. Since
//! M8.3 the same thread carries out the writes: a write is followed by a
//! walk, so the map that shows it is the next thing the window hears,
//! and a write git refuses comes back in git's words and leaves the last
//! map standing.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use gpui::Context;
use guild_session::ProjectId;

use crate::Root;
mod shared;
use crate::git::blame::{self, Run};
use crate::git::branches::{self, Branch};
use crate::git::log::{self, FileChange};
use crate::git::stashes::{self, Stash};
use crate::git::status::{StatusMap, parse};
use crate::git::{self, GitError, Write};
pub(crate) use shared::Coordinator as WorktreeReads;
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

/// What the thread is asked: a walk, a write and then a walk, a page of
/// the log, or one commit's files.
pub(crate) enum Ask {
    #[cfg(feature = "native-preview")]
    DiagnosticFence(mpsc::Sender<()>),
    Common(mpsc::Sender<Result<PathBuf, GitError>>),
    SharedWorktrees {
        counted: Counted,
        full: bool,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        reply: mpsc::Sender<shared::ReadReply>,
    },
    Walk,
    UpdateSources,
    UpdateBranch(
        git::branch_update::Request,
        std::sync::Arc<std::sync::atomic::AtomicBool>,
        InFlight,
    ),
    /// Auto fast-forward's hand-off: the checkout this root is, moved on
    /// this thread so no other write of it interleaves. `clean` is the
    /// unsaved-buffer flag the window keeps current; `cancelled` is the
    /// run's own token, read immediately before the write. `done` is the
    /// answer's own channel: the window holds it past the project's life,
    /// so a worker dropped after the hand-off still says how it ended — or,
    /// dropping the ask unrun, that it never wrote.
    FastForward {
        handoff: git::auto_ff::Handoff,
        clean: std::sync::Arc<std::sync::atomic::AtomicBool>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        done: mpsc::Sender<(git::auto_ff::Landed, Vec<crate::console::Line>)>,
    },
    Run(Write),
    /// One serial, stop-on-failure preparation; the request owns its message file.
    PreparedCommit {
        stage: Vec<PathBuf>,
        unstage: Vec<PathBuf>,
        message: tempfile::NamedTempFile,
    },
    Log {
        epoch: u64,
        scope: crate::git::Scope,
        filter: crate::git::Filter,
        skip: usize,
        n: usize,
    },
    RevealCommit(super::log::Navigation),
    Files(String),
    /// Which branches contain a commit — the footer's *In N branches*.
    Contains(String),
    Blame(PathBuf),
    InlineBlame {
        entity: gpui::EntityId,
        generation: u64,
        path: PathBuf,
    },
    Branches,
    /// What is in the stash, for the branch popup's other tab.
    Stashes,
    /// What a push would send and where it could go — the Push tab's
    /// four questions asked as one. The remote is the one chosen in the
    /// head, which decides the range when there is no upstream. M8.8.4.
    Push {
        remote: Option<String>,
        target: Option<String>,
    },
    /// The repository this root is in, and every worktree of it — M12.
    /// Both halves from one ask: the column groups by the common dir and
    /// draws the rows git lists, and two asks would be two answers free
    /// to disagree about the same repository.
    ///
    /// **`full: false` is the cadence's ask** — 2026-09-24. It runs `git
    /// worktree list` and nothing else, counts the times and the lines
    /// again only when that list is not the one counted last, and answers
    /// nothing when it is: see [`WORKTREES_EVERY`]. `full: true` counts
    /// whatever the list says, and always answers.
    #[cfg(test)]
    Worktrees {
        full: bool,
    },
}

/// What it answers: a map, a write git refused, or a commit that landed
/// — with its short hash and summary, read right after it.
pub(crate) enum Answer {
    Map(Result<StatusMap, GitError>),
    UpdateSources(Result<git::branch_update::Snapshot, GitError>),
    BranchUpdated(git::branch_update::Receipt),
    Refused(Write, GitError),
    /// A stash drop whose entry is no longer listed, so nothing was run —
    /// git refused nothing, which is why this is not a [`GitError`]: that
    /// enum is matched with no wildcard in four places. Decision 17 of
    /// M5.11.
    StashGone(Write),
    /// How a hunk write ended — decision 55. Sent for every hunk write,
    /// in place of [`Answer::Refused`]: the window waits for it before a
    /// commit is planned.
    Hunk(Write, HunkOutcome),
    Committed {
        short: String,
        summary: String,
    },
    /// A folder project's commit refused under the checkout's turn: these
    /// paths are staged outside the folder. Nothing was committed.
    CommitOutside(Vec<PathBuf>),
    Log(super::log::PageReply),
    RevealCommit(super::log::RevealReply),
    Files {
        sha: String,
        files: Result<Vec<FileChange>, GitError>,
    },
    Contains {
        sha: String,
        branches: Result<Vec<String>, GitError>,
    },
    Blame {
        path: PathBuf,
        runs: Result<Vec<Run>, GitError>,
    },
    InlineBlame {
        entity: gpui::EntityId,
        generation: u64,
        path: PathBuf,
        runs: Result<Vec<Run>, GitError>,
    },
    Branches(Result<Vec<Branch>, GitError>),
    Stashes(Result<Vec<Stash>, GitError>),
    /// What a push would send and where. Sent for an [`Ask::Push`] and
    /// again after every [`Write::Push`], refused or not: what the tab
    /// shows is only true until the push happens, and a list that
    /// emptied itself on the window's guess rather than on git's answer
    /// would be a lie whenever git refused. M8.8.4.
    Push(Result<crate::git::Ahead, GitError>),
    /// One write ran, and this is what git said about it — the Console's
    /// line. Sent for every write, refused or not: a command that
    /// succeeded silently is still a command that ran.
    Ran {
        at: i64,
        argv: Vec<String>,
        said: String,
        failed: bool,
    },
    /// The repository this root is in, and its worktrees — M12. One
    /// answer, because the column needs both to draw a group at all.
    #[cfg(test)]
    Worktrees(Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError>),
}

/// How a hunk write ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HunkOutcome {
    Written,
    /// Its entry, disk or `HEAD` was no longer what it was computed from:
    /// nothing was written.
    Stale,
    /// git would store it converted: nothing was written.
    Converted,
    /// Past the size hunks are computed for: nothing was written.
    WholeFile,
    /// git refused it, in its words, with the entry still the preimage.
    Refused(GitError),
    /// Written, and `HEAD`'s blob moved meanwhile: a commit landed between
    /// the check and the write, which git cannot exclude.
    HeadMoved,
    /// `apply` reported success and the entry could not be read back: the
    /// index has most likely changed. Review r4.
    Unverified,
    /// `apply` ran past its deadline and the entry could not be read back:
    /// whether it was written is not known. Review r5.
    Unconfirmed,
}

/// What the commit barrier reads from one take, in the order it came: a
/// hunk write's outcome, or a status map — usable or not. A failed map
/// between an answer and a usable one is news on its own (review r4
/// `bugs+impl-1`), so the take keeps them all.
#[derive(Debug)]
pub(crate) enum HunkEvent {
    Outcome(Write, HunkOutcome),
    Map(bool),
}

/// What `take` found waiting: the newest map, and every refusal since
/// the last take — older maps behind the newest are dropped, because the
/// newest is the one that is true; refusals are not, because each is a
/// piece of news.
#[derive(Default)]
pub(crate) struct Taken {
    pub update_sources: Option<Result<git::branch_update::Snapshot, GitError>>,
    pub branch_updated: Option<git::branch_update::Receipt>,
    pub map: Option<Result<StatusMap, GitError>>,
    pub refused: Vec<(Write, GitError)>,
    /// Every drop since the last take whose entry had gone. A `Vec` for
    /// the reason refusals are one: each is a piece of news.
    pub stash_gone: Vec<Write>,
    pub outside: Vec<Vec<PathBuf>>,
    /// Every hunk write that ended and every map, in order.
    pub hunk_events: Vec<HunkEvent>,
    pub committed: Vec<(String, String)>,
    pub logs: Vec<super::log::PageReply>,
    pub reveals: Vec<super::log::RevealReply>,
    pub files: Vec<(String, Result<Vec<FileChange>, GitError>)>,
    pub contains: Vec<(String, Result<Vec<String>, GitError>)>,
    pub blames: Vec<(PathBuf, Result<Vec<Run>, GitError>)>,
    pub inline_blames: Vec<InlineBlameReply>,
    pub branches: Option<Result<Vec<Branch>, GitError>>,
    pub stashes: Option<Result<Vec<Stash>, GitError>>,
    /// The newest push read; older ones behind it are dropped, for the
    /// reason older maps are.
    pub push: Option<Result<crate::git::Ahead, GitError>>,
    /// Every write since the last take, in the order they ran. A `Vec`
    /// for the reason refusals are one: each is a piece of news.
    pub ran: Vec<crate::console::Line>,
    /// The newest answer about this repository's worktrees; older ones
    /// behind it are dropped, for the reason older maps are — the newest
    /// is the one that is true. M12.
    pub worktrees: Option<Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError>>,
    /// The thread has gone: its end of the answers channel was dropped
    /// while the worker still lives, which `serve` never does on its own —
    /// a panic did it, or a spawn that failed. Everything it sent before
    /// is still above. Decision 19 of M5.11.
    pub gone: bool,
}
pub(crate) struct InlineBlameReply {
    entity: gpui::EntityId,
    generation: u64,
    path: PathBuf,
    runs: Result<Vec<Run>, GitError>,
}

/// What the window does with a worker whose thread has gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterDeath {
    /// Start a fresh thread, which asks again for whatever was waiting.
    Restart,
    /// Leave it dead until the project is opened again.
    Leave,
}

/// A first death is started again; any later one is left. A panic on an
/// odd repository state need not repeat, and one that does would be a
/// restart on every tick of the watch loop. Decision 19 of M5.11.
fn after_death(restarts: u8) -> AfterDeath {
    if restarts == 0 {
        AfterDeath::Restart
    } else {
        AfterDeath::Leave
    }
}

/// One project's git thread: asked with a nudge or a write, answers
/// with a map. Asks that pile up while a walk runs collapse into one
/// walk — after every write among them has been carried out, in order.
pub(crate) struct StatusWorker {
    serial: u64,
    asks: mpsc::Sender<Ask>,
    answers: mpsc::Receiver<Answer>,
    /// Where its git runs, for the Console's heading. Kept here rather
    /// than sent with every answer: it is the same string every time.
    root: String,
    /// How many threads before this one died for this project.
    restarts: u8,
    /// Its thread has gone for good: said once, and restarted by nothing
    /// until the project is opened again.
    dead: bool,
}

impl StatusWorker {
    #[cfg(feature = "native-preview")]
    pub(crate) fn diagnostic_fence(&self) -> mpsc::Receiver<()> {
        let (tx, rx) = mpsc::channel();
        let _ = self.asks.send(Ask::DiagnosticFence(tx));
        rx
    }

    /// Start a thread for the project at `root`, and ask it once.
    pub(crate) fn start(root: PathBuf) -> Self {
        let shown = root.to_string_lossy().into_owned();
        let (asks_tx, asks_rx) = mpsc::channel::<Ask>();
        let (answers_tx, answers_rx) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("guild-git-status".into())
            .spawn(move || serve(&root, &asks_rx, &answers_tx));
        // A spawn that failed dropped the closure and the answering end
        // with it, so the first `take` reads it as a death — one route for
        // both ways a thread can be missing.
        if let Err(err) = spawned {
            tracing::warn!(%err, "the git thread could not be started");
        }
        let _ = asks_tx.send(Ask::Walk);
        Self {
            serial: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            asks: asks_tx,
            answers: answers_rx,
            root: shown,
            restarts: 0,
            dead: false,
        }
    }

    /// A worker with no thread: the test holds the answering end and the
    /// asking one, and `take` drains what it sends as it would a thread's.
    #[cfg(test)]
    pub(crate) fn fake(root: &Path) -> (Self, mpsc::Sender<Answer>, mpsc::Receiver<Ask>) {
        let (asks_tx, asks_rx) = mpsc::channel::<Ask>();
        let (answers_tx, answers_rx) = mpsc::channel();
        let worker = Self {
            serial: {
                static NEXT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(1 << 40);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
            asks: asks_tx,
            answers: answers_rx,
            root: root.to_string_lossy().into_owned(),
            restarts: 0,
            dead: false,
        };
        (worker, answers_tx, asks_rx)
    }

    /// Ask for a fresh map. Never blocks; the answer comes to `take`.
    pub(crate) fn ask(&self) {
        let _ = self.asks.send(Ask::Walk);
    }

    /// Refresh the read-only source picker on the project's Git worker.
    pub(crate) fn update_sources(&self) {
        let _ = self.asks.send(Ask::UpdateSources);
    }

    pub(crate) fn update_branch(
        &self,
        request: git::branch_update::Request,
        clean: std::sync::Arc<std::sync::atomic::AtomicBool>,
        in_flight: InFlight,
    ) -> bool {
        self.asks
            .send(Ask::UpdateBranch(request, clean, in_flight))
            .is_ok()
    }

    /// Hand a fast-forward of this root's checkout to its thread.
    pub(crate) fn fast_forward(
        &self,
        handoff: git::auto_ff::Handoff,
        clean: std::sync::Arc<std::sync::atomic::AtomicBool>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
        done: mpsc::Sender<(git::auto_ff::Landed, Vec<crate::console::Line>)>,
    ) -> bool {
        self.asks
            .send(Ask::FastForward {
                handoff,
                clean,
                cancelled,
                done,
            })
            .is_ok()
    }

    /// Carry out a write, and walk after it. Never blocks; the map that
    /// shows the write, or git's refusal, comes to `take`.
    pub(crate) fn run(&self, write: Write) {
        let _ = self.try_run(write);
    }

    /// This thread's serial: a replaced thread has another.
    pub(crate) fn serial(&self) -> u64 {
        self.serial
    }

    pub(crate) fn prepare_commit(
        &self,
        stage: Vec<PathBuf>,
        unstage: Vec<PathBuf>,
        message: tempfile::NamedTempFile,
    ) {
        let _ = self.asks.send(Ask::PreparedCommit {
            stage,
            unstage,
            message,
        });
    }

    /// A confirmed GUI write must surface a worker that closed before enqueue.
    pub(crate) fn try_run(&self, write: Write) -> bool {
        self.asks.send(Ask::Run(write)).is_ok()
    }

    /// Ask for a page of the log: `n` commits after `skip`, within the
    /// scope the toolbar is set to.
    pub(crate) fn log(
        &self,
        epoch: u64,
        scope: crate::git::Scope,
        filter: crate::git::Filter,
        skip: usize,
        n: usize,
    ) {
        let _ = self.asks.send(Ask::Log {
            epoch,
            scope,
            filter,
            skip,
            n,
        });
    }
    pub(crate) fn reveal_commit(&self, intent: super::log::Navigation) {
        let _ = self.asks.send(Ask::RevealCommit(intent));
    }

    /// Ask which branches contain a commit.
    pub(crate) fn contains(&self, sha: String) {
        let _ = self.asks.send(Ask::Contains(sha));
    }

    /// Ask for one commit's files.
    pub(crate) fn files(&self, sha: String) {
        let _ = self.asks.send(Ask::Files(sha));
    }

    /// Ask who wrote each line of a file.
    pub(crate) fn blame(&self, path: PathBuf) {
        let _ = self.asks.send(Ask::Blame(path));
    }
    pub(crate) fn inline_blame(&self, entity: gpui::EntityId, generation: u64, path: PathBuf) {
        let _ = self.asks.send(Ask::InlineBlame {
            entity,
            generation,
            path,
        });
    }

    /// Ask for the local branches.
    pub(crate) fn branches(&self) {
        let _ = self.asks.send(Ask::Branches);
    }

    /// Ask what is in the stash.
    pub(crate) fn stashes(&self) {
        let _ = self.asks.send(Ask::Stashes);
    }

    /// Ask what a push would send, to the remote chosen in the head.
    pub(crate) fn push_read(&self, remote: Option<String>, target: Option<String>) {
        let _ = self.asks.send(Ask::Push { remote, target });
    }

    /// Ask which repository this root is in and what its worktrees are.
    /// M12.
    #[cfg(test)]
    pub(crate) fn worktrees(&self) {
        let _ = self.asks.send(Ask::Worktrees { full: true });
    }

    /// The cadence's ask: the list, and the count only if the list moved.
    /// An answer comes only when something did.
    #[cfg(test)]
    pub(crate) fn poll_worktrees(&self) {
        let _ = self.asks.send(Ask::Worktrees { full: false });
    }

    /// Whatever is waiting, and whether the thread is still there to send
    /// more.
    pub(crate) fn take(&self) -> Taken {
        let mut taken = Taken::default();
        loop {
            let answer = match self.answers.try_recv() {
                Ok(answer) => answer,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    taken.gone = true;
                    break;
                }
            };
            match answer {
                Answer::UpdateSources(value) => taken.update_sources = Some(value),
                Answer::BranchUpdated(value) => taken.branch_updated = Some(value),
                Answer::Map(map) => {
                    taken.hunk_events.push(HunkEvent::Map(map.is_ok()));
                    taken.map = Some(map);
                }
                Answer::Hunk(write, outcome) => {
                    taken.hunk_events.push(HunkEvent::Outcome(write, outcome));
                }
                Answer::Refused(write, err) => taken.refused.push((write, err)),
                Answer::StashGone(write) => taken.stash_gone.push(write),
                Answer::CommitOutside(paths) => taken.outside.push(paths),
                Answer::Committed { short, summary } => taken.committed.push((short, summary)),
                Answer::Log(page) => taken.logs.push(page),
                Answer::RevealCommit(reply) => taken.reveals.push(reply),
                Answer::Files { sha, files } => taken.files.push((sha, files)),
                Answer::Contains { sha, branches } => taken.contains.push((sha, branches)),
                Answer::Blame { path, runs } => taken.blames.push((path, runs)),
                Answer::InlineBlame {
                    entity,
                    generation,
                    path,
                    runs,
                } => taken.inline_blames.push(InlineBlameReply {
                    entity,
                    generation,
                    path,
                    runs,
                }),
                Answer::Branches(branches) => taken.branches = Some(branches),
                Answer::Stashes(stashes) => taken.stashes = Some(stashes),
                Answer::Push(ahead) => taken.push = Some(ahead),
                #[cfg(test)]
                Answer::Worktrees(answer) => taken.worktrees = Some(answer),
                Answer::Ran {
                    at,
                    argv,
                    said,
                    failed,
                } => taken.ran.push(crate::console::Line {
                    at,
                    root: self.root.clone(),
                    argv,
                    said,
                    failed,
                }),
            }
        }
        taken
    }
}

/// The thread: wait for an ask, drain the others that arrived meanwhile,
/// carry out the writes among them in order, walk once, answer. Ends
/// when the worker is dropped. A directory that is not a repository, or
/// a machine without git, answers the error to every ask and asks git
/// again each time — the repository may appear.
fn serve(root: &Path, asks: &mpsc::Receiver<Ask>, answers: &mpsc::Sender<Answer>) {
    #[cfg(test)]
    let mut worktrees = Counted::default();
    // A hunk write still waits for a usable map: every walk until one lands
    // goes through the hunk's door, a watcher's included — review r5
    // `arch+quality-2`.
    let mut unsettled = false;
    while let Ok(first) = asks.recv() {
        let mut batch = vec![first];
        while let Ok(more) = asks.try_recv() {
            batch.push(more);
        }
        let mut walk = false;
        // A hunk write's walk settles the commit barrier: it goes through
        // the hunk's door — bounded, no monitor, nothing fetched.
        let hunked = unsettled
            || batch
                .iter()
                .any(|ask| matches!(ask, Ask::Run(write) if write.hunk().is_some()));
        for ask in batch {
            let answer = match ask {
                #[cfg(feature = "native-preview")]
                Ask::DiagnosticFence(reply) => {
                    let _ = reply.send(());
                    continue;
                }
                Ask::Common(reply) => {
                    let result = crate::git::worktrees::common_dir(root).and_then(|p| {
                        p.canonicalize().map_err(|e| {
                            GitError::Failed(format!("canonical common Git directory: {e}"))
                        })
                    });
                    let _ = reply.send(result);
                    continue;
                }
                Ask::SharedWorktrees {
                    mut counted,
                    full,
                    cancelled,
                    reply,
                } => {
                    let result = if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        None
                    } else {
                        counted.read(root, full, Some(&cancelled))
                    };
                    if !cancelled.load(std::sync::atomic::Ordering::Acquire) {
                        let _ = reply.send((counted, result));
                    }
                    continue;
                }
                Ask::UpdateSources => {
                    Answer::UpdateSources(git::branch_update::Snapshot::read(root))
                }
                Ask::UpdateBranch(request, clean, _in_flight) => {
                    walk = true;
                    let result = git::branch_update::execute_recorded(
                        root,
                        &request,
                        || clean.load(std::sync::atomic::Ordering::Acquire),
                        |write, result| report_ran(answers, write, result),
                    );
                    Answer::BranchUpdated(result)
                }
                Ask::FastForward {
                    handoff,
                    clean,
                    cancelled,
                    done,
                } => {
                    walk = true;
                    // Only for this root's own checkout: a root in a folder
                    // of another checkout is not one of its roots.
                    let mine = git::checkout_of(root);
                    if mine.is_none() || mine != handoff.worktree.canonicalize().ok() {
                        let _ = done.send((
                            git::auto_ff::Landed::Skipped(git::auto_ff::Reason::Refused),
                            Vec::new(),
                        ));
                        continue;
                    }
                    // Never a notice: decision 21's auto fast-forward is
                    // silent. Its Console line goes with the answer on
                    // `done`, which outlives this root: a root closed
                    // after the merge would drop it with `answers`.
                    let shown = root.to_string_lossy().into_owned();
                    let mut ran = Vec::new();
                    let landed = git::auto_ff::fast_forward_checkout(
                        root,
                        &handoff.branch,
                        &handoff.expected,
                        &handoff.oid,
                        || clean.load(std::sync::atomic::Ordering::Acquire),
                        &cancelled,
                        |write, result| ran.push(console_line(&shown, write, result)),
                    );
                    let _ = done.send((landed, ran));
                    continue;
                }
                Ask::Walk => {
                    walk = true;
                    continue;
                }
                Ask::Run(write) => {
                    walk = true;
                    if execute_write(root, write, answers).is_err() {
                        return;
                    }
                    continue;
                }
                Ask::PreparedCommit {
                    stage,
                    unstage,
                    message,
                } => {
                    walk = true;
                    let mut writes = Vec::new();
                    if !stage.is_empty() {
                        writes.push(Write::Stage(stage));
                    }
                    if !unstage.is_empty() {
                        writes.push(Write::Unstage(unstage));
                    }
                    writes.push(Write::Commit {
                        message_file: message.path().to_path_buf(),
                    });
                    // The preparation and the commit in one turn of the
                    // checkout, and a folder's scope asked again in it
                    // before the commit — review r3 `arch+quality-2`.
                    let held = git::turn(root);
                    for write in writes {
                        if matches!(write, Write::Commit { .. }) {
                            match git::staged_outside(root) {
                                Ok(paths) if paths.is_empty() => {}
                                Ok(paths) => {
                                    if answers.send(Answer::CommitOutside(paths)).is_err() {
                                        return;
                                    }
                                    break;
                                }
                                Err(err) => {
                                    if answers.send(Answer::Refused(write, err)).is_err() {
                                        return;
                                    }
                                    break;
                                }
                            }
                        }
                        match execute_write_in(root, write, answers, held.is_some()) {
                            Ok(true) => {}
                            Ok(false) => break,
                            Err(()) => return,
                        }
                    }
                    // message owns the file through every write and releases it on all outcomes.
                    continue;
                }
                Ask::Contains(sha) => Answer::Contains {
                    branches: git::contains(root, &sha),
                    sha,
                },
                Ask::Log {
                    epoch,
                    scope,
                    filter,
                    skip,
                    n,
                } => Answer::Log(super::log::PageReply {
                    epoch,
                    skip,
                    commits: git::log(root, &scope, &filter, skip, n).and_then(|bytes| {
                        log::parse(&bytes).map_err(|e| GitError::Failed(e.to_string()))
                    }),
                }),
                Ask::RevealCommit(intent) => {
                    let commit = git::log(
                        root,
                        &git::Scope::Named(intent.sha.clone()),
                        &git::Filter::default(),
                        0,
                        1,
                    )
                    .and_then(|bytes| {
                        log::parse(&bytes).map_err(|e| GitError::Failed(e.to_string()))
                    })
                    .and_then(|commits| {
                        commits
                            .into_iter()
                            .find(|c| c.sha == intent.sha)
                            .ok_or_else(|| {
                                GitError::Failed(format!("Commit {} is unavailable", intent.sha))
                            })
                    });
                    let files = if commit.is_ok() {
                        git::commit_files(root, &intent.sha).and_then(|bytes| {
                            log::files_of(&bytes).map_err(|e| GitError::Failed(e.to_string()))
                        })
                    } else {
                        Ok(Vec::new())
                    };
                    Answer::RevealCommit(super::log::RevealReply {
                        intent,
                        commit,
                        files,
                    })
                }
                Ask::Files(sha) => Answer::Files {
                    files: git::commit_files(root, &sha).and_then(|bytes| {
                        log::files_of(&bytes).map_err(|err| GitError::Failed(err.to_string()))
                    }),
                    sha,
                },
                Ask::Branches => Answer::Branches(git::branches(root).and_then(|bytes| {
                    branches::parse(&bytes).map_err(|err| GitError::Failed(err.to_string()))
                })),
                Ask::Blame(path) => Answer::Blame {
                    runs: git::blame(root, &path).and_then(|bytes| {
                        blame::parse(&bytes).map_err(|err| GitError::Failed(err.to_string()))
                    }),
                    path,
                },
                Ask::InlineBlame {
                    entity,
                    generation,
                    path,
                } => Answer::InlineBlame {
                    entity,
                    generation,
                    runs: git::blame(root, &path).and_then(|bytes| {
                        blame::parse(&bytes).map_err(|error| GitError::Failed(error.to_string()))
                    }),
                    path,
                },
                Ask::Stashes => Answer::Stashes(git::stashes(root).and_then(|bytes| {
                    stashes::parse(&bytes).map_err(|err| GitError::Failed(err.to_string()))
                })),
                Ask::Push { remote, target } => {
                    Answer::Push(ahead(root, remote.as_deref(), target.as_deref()))
                }
                #[cfg(test)]
                Ask::Worktrees { full } => match worktrees.read(root, full, None) {
                    Some(answer) => Answer::Worktrees(answer),
                    None => continue,
                },
            };
            if answers.send(answer).is_err() {
                return;
            }
        }
        // A page of the log changes nothing; only a walk or a write
        // wants the map read again.
        if !walk {
            continue;
        }
        let answer = git::toplevel(root).and_then(|top| {
            // A project that is a subdirectory of its repository gets
            // git's paths relative to the repository; the tree's are
            // relative to the project.
            let prefix = root
                .canonicalize()
                .ok()
                .and_then(|canonical| canonical.strip_prefix(&top).ok().map(Path::to_path_buf))
                .unwrap_or_default();
            let bytes = if hunked {
                git::hunk::status(root)?
            } else {
                git::status(root)?
            };
            let entries = parse(&bytes).map_err(|err| GitError::Failed(err.to_string()))?;
            // What lies outside the folder is dropped, except what is
            // staged there, which a commit from here would take along.
            let (inside, elsewhere) = crate::git::status::within(entries, &prefix);
            Ok(StatusMap::from_entries(inside).with_staged_elsewhere(elsewhere))
        });
        unsettled = hunked && answer.is_err();
        // Publish identity before its status map, so normal commits, switches
        // and watched external changes cannot leave the Update button stale.
        if answers
            .send(Answer::UpdateSources(git::branch_update::Snapshot::read(
                root,
            )))
            .is_err()
        {
            return;
        }
        if answers.send(Answer::Map(answer)).is_err() {
            return;
        }
    }
}

/// Shared execution for individual writes and serial Commit preparation.
/// Ok(false) stops only preparation; a disconnected owner terminates the worker.
/// What a test runs in one repository between a hunk write's checks and
/// the write — a commit from outside, landing in the window git cannot close.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) static BETWEEN_CHECKS_AND_WRITE: std::sync::Mutex<
    Option<(PathBuf, Box<dyn Fn() + Send>)>,
> = std::sync::Mutex::new(None);

fn execute_write(root: &Path, write: Write, answers: &mpsc::Sender<Answer>) -> Result<bool, ()> {
    execute_write_in(root, write, answers, false)
}

/// [`execute_write`], `in_turn` when the caller holds the checkout's turn.
fn execute_write_in(
    root: &Path,
    write: Write,
    answers: &mpsc::Sender<Answer>,
    in_turn: bool,
) -> Result<bool, ()> {
    // A hunk write holds the checkout's turn across its checks, the write
    // and the HEAD post-check: another root's thread in the same checkout
    // cannot commit between them. Review r1 `adversarial-5`.
    let held = (!in_turn && write.hunk().is_some())
        .then(|| git::turn(root))
        .flatten();
    let in_turn = in_turn || held.is_some();
    // Stash identity is settled before execution; a vanished entry produces no command.
    let write = match git::settle(root, write) {
        git::Settled::Ready(write) => write,
        git::Settled::Stale(write) => {
            answers
                .send(Answer::Hunk(write, HunkOutcome::Stale))
                .map_err(|_| ())?;
            return Ok(false);
        }
        git::Settled::Converted(write) => {
            answers
                .send(Answer::Hunk(write, HunkOutcome::Converted))
                .map_err(|_| ())?;
            return Ok(false);
        }
        git::Settled::WholeFile(write) => {
            answers
                .send(Answer::Hunk(write, HunkOutcome::WholeFile))
                .map_err(|_| ())?;
            return Ok(false);
        }
        git::Settled::StashGone(write) => {
            answers.send(Answer::StashGone(write)).map_err(|_| ())?;
            return Ok(false);
        }
        // A hunk's refusal reaches the barrier like every other outcome.
        git::Settled::Failed(write, err) if write.hunk().is_some() => {
            answers
                .send(Answer::Hunk(write, HunkOutcome::Refused(err)))
                .map_err(|_| ())?;
            return Ok(false);
        }
        git::Settled::Failed(write, err) => {
            answers.send(Answer::Refused(write, err)).map_err(|_| ())?;
            return Ok(false);
        }
    };
    #[cfg(test)]
    if write.hunk().is_some()
        && let Some((at, hook)) = &*BETWEEN_CHECKS_AND_WRITE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        && at.as_path() == root
    {
        hook();
    }
    let done = if in_turn {
        git::write_in_turn(root, &write)
    } else {
        git::write(root, &write)
    };
    let said = match &done {
        Ok(said) => said.text(),
        Err(GitError::Failed(words)) => words.clone(),
        Err(other) => other.to_string(),
    };
    // Console precedes refusal/success and records only commands actually attempted.
    answers
        .send(Answer::Ran {
            at: now_millis(),
            argv: write.args(),
            said,
            failed: done.is_err(),
        })
        .map_err(|_| ())?;
    // A refused push still refreshes Push Commits and clears its busy state.
    if let Write::Push {
        remote, refspec, ..
    } = &write
    {
        let target = refspec
            .split_once(':')
            .map(|(_, target)| target)
            .unwrap_or(refspec);
        answers
            .send(Answer::Push(ahead(root, Some(remote), Some(target))))
            .map_err(|_| ())?;
    }
    if let Some(hunk) = write.hunk() {
        // Classified by reading the entry again, never by git's words,
        // which follow the person's locale.
        let outcome = match done {
            Err(err) if git::hunk::unverified(&err) => HunkOutcome::Unverified,
            Err(err) if git::hunk::unconfirmed(&err) => HunkOutcome::Unconfirmed,
            Err(err) if git::hunk::unanswered(&err) => HunkOutcome::Refused(err),
            Err(err) => match git::hunk::stale(root, hunk) {
                Ok(true) => HunkOutcome::Stale,
                _ => HunkOutcome::Refused(err),
            },
            // Only an Unstage puts `HEAD`'s bytes back, so only it can undo
            // a commit landing between its checks and its write; a check
            // that cannot answer is said as the same warning.
            Ok(_) if matches!(write, Write::UnstageHunk(_)) => {
                match git::hunk::head_moved(root, hunk) {
                    Ok(false) => HunkOutcome::Written,
                    _ => HunkOutcome::HeadMoved,
                }
            }
            Ok(_) => HunkOutcome::Written,
        };
        let wrote = matches!(
            outcome,
            HunkOutcome::Written
                | HunkOutcome::HeadMoved
                | HunkOutcome::Unverified
                | HunkOutcome::Unconfirmed
        );
        answers.send(Answer::Hunk(write, outcome)).map_err(|_| ())?;
        return Ok(wrote);
    }
    match done {
        Err(err) => {
            answers.send(Answer::Refused(write, err)).map_err(|_| ())?;
            Ok(false)
        }
        Ok(_) if matches!(write, Write::Commit { .. }) => {
            let (short, summary) = git::last_commit(root).unwrap_or_default();
            answers
                .send(Answer::Committed { short, summary })
                .map_err(|_| ())?;
            Ok(true)
        }
        Ok(_) => Ok(true),
    }
}

/// What the thread remembers about the worktrees between two asks — the
/// cheap poll of 2026-09-24.
#[derive(Default)]
pub(crate) struct Counted {
    /// The repository's git directory, asked once: a root's common dir
    /// does not change for the life of its thread. Not kept while git
    /// refuses — a folder may become a repository.
    common: Option<PathBuf>,
    /// The list as git last printed it, before any time or line was
    /// counted into it; `None` until one was read, and after a refusal.
    listed: Option<Vec<crate::git::worktrees::Worktree>>,
    /// The repository's `config` as it stood when last counted — its
    /// modification time and length. A remote's URL or a branch's upstream
    /// changed there moves no list, and is news all the same (GLD-063,
    /// code review r1, docs+tests-5).
    config: Option<(Option<std::time::SystemTime>, u64)>,
}

impl Counted {
    /// Both halves or neither: a common dir without a list draws a group
    /// with no rows, and a list without one has no group to belong to.
    /// M12.
    ///
    /// **Nothing, for a poll whose list is the one counted last.** The
    /// times and the lines follow from the heads and the working trees;
    /// a head that moved, a branch, a row more or less is a different list
    /// and is counted again, and an edit that moves only a row's `+N −M`
    /// waits for a full ask — the trade-off [`WORKTREES_EVERY`] records.
    fn read(
        &mut self,
        root: &Path,
        full: bool,
        cancelled: Option<&std::sync::atomic::AtomicBool>,
    ) -> Option<Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError>> {
        let common = match &self.common {
            Some(common) => common.clone(),
            None => match crate::git::worktrees::common_dir(root) {
                Ok(common) => self.common.insert(common).clone(),
                Err(err) => return Some(Err(err)),
            },
        };
        let list = match crate::git::worktrees::list(root) {
            Ok(list) => list,
            Err(err) => {
                self.listed = None;
                return Some(Err(err));
            }
        };
        let config = std::fs::metadata(common.join("config"))
            .ok()
            .map(|meta| (meta.modified().ok(), meta.len()));
        if !full && self.listed.as_ref() == Some(&list) && self.config == config {
            return None;
        }
        self.listed = Some(list.clone());
        self.config = config;
        Some(Ok((common, counted(root, list, cancelled))))
    }
}

/// A list with its times and lines counted in.
fn counted(
    root: &Path,
    mut list: Vec<crate::git::worktrees::Worktree>,
    cancelled: Option<&std::sync::atomic::AtomicBool>,
) -> Vec<crate::git::worktrees::Worktree> {
    // When each head was committed, in one call for the whole list — M12
    // task 16. A time is a nicety and the rows are not: a repository that
    // will not describe its commits still has worktrees, so a refusal
    // here leaves every `committed` at `None` and nothing else.
    if cancelled.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
        return list;
    }
    let heads: Vec<String> = list.iter().filter_map(|tree| tree.head.clone()).collect();
    if let Ok(times) = crate::git::worktrees::times(root, &heads) {
        for tree in &mut list {
            tree.committed = tree
                .head
                .as_ref()
                .and_then(|head| times.iter().find(|(sha, _)| sha == head).map(|(_, at)| *at));
        }
    }
    // The lines each drawn worktree added and removed since the main
    // worktree's branch — M12.4. The first record is the main worktree in
    // every list git prints; on no branch there is no base, and no row
    // gets numbers. A refusal is `None` for that row and nothing else.
    if let Some(base) = list.first().and_then(|main| main.branch.clone()) {
        for tree in list.iter_mut().filter(|tree| tree.shown()) {
            if cancelled.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
                break;
            }
            tree.lines = crate::git::worktrees::lines(&tree.path, &base).ok();
        }
    }
    // Where each branch would be pushed and what a push would send — the
    // MR's key and `↑N`, GLD-063. Refs only; a refusal leaves every row
    // without them and nothing else.
    if !cancelled.is_some_and(|c| c.load(std::sync::atomic::Ordering::Acquire)) {
        let _ = crate::git::worktrees::destinations(root, &mut list);
        // The main branch, by the rule a new worktree starts from, is never
        // asked for an MR: marked here, off the window's thread. A refusal
        // marks nothing, and the branch is asked as any other.
        if let Some(main) = crate::git::main_branch(root) {
            for tree in &mut list {
                tree.main_branch = tree.branch.as_deref() == Some(main.as_str());
            }
        }
    }
    list
}

/// The Push tab's four questions, asked on the git thread as one.
///
/// **The order matters and the failures do not stop it.** A repository
/// with no remote answers an empty list and a branch with no upstream
/// answers `None`, and both are states the tab draws rather than errors
/// — the whole point of the tab is the branch nobody has pushed yet.
/// What does stop it is git having no answer at all: no repository, no
/// git, or a `log` that failed, which the tab shows as a note.
///
/// The remote decides the range only when there is no upstream, and a
/// repository with no remote has no range to ask for at all: an empty
/// list of commits with the empty list of remotes says everything the
/// head needs.
fn ahead(
    root: &Path,
    remote: Option<&str>,
    target: Option<&str>,
) -> Result<crate::git::Ahead, GitError> {
    let branch = git::head_branch(root)?;
    let remotes = git::remotes(root)?;
    let upstream = git::upstream(root)?;
    if let Some(target) = target {
        git::run(root, &["check-ref-format", &format!("refs/heads/{target}")])?;
    }
    // The remote the range is measured against: the one chosen, then
    // the upstream's, then the first git printed.
    let against = remote
        .filter(|name| remotes.iter().any(|known| known == name))
        .or_else(|| {
            upstream
                .as_deref()
                .and_then(crate::git::split_upstream)
                .map(|(remote, _)| remote)
        })
        .map(str::to_owned)
        .or_else(|| remotes.first().cloned());
    let commits = match &against {
        None => Vec::new(),
        Some(against) => {
            // **The upstream counts only on its own remote.** A person
            // who chose a second remote in the head is asking what
            // *that* one is missing, and `origin/main..HEAD` would
            // answer about the first — which reads as a push about to
            // send three commits and sends thirty.
            let tracked = upstream.as_deref().filter(|upstream| {
                crate::git::split_upstream(upstream)
                    .is_some_and(|(remote, _)| remote == against.as_str())
            });
            let edited = target.map(|target| format!("refs/remotes/{against}/{target}"));
            let range = match edited.as_deref() {
                Some(reference) => git::run(root, &["rev-parse", "--verify", reference])
                    .ok()
                    .map(|_| reference),
                None => tracked,
            };
            let bytes = if edited.is_some() && range.is_none() {
                // A new destination publishes this branch's history,
                // even when another remote branch already holds it.
                match git::log(
                    root,
                    &git::Scope::Head,
                    &git::Filter::default(),
                    0,
                    crate::root::push::PAGE,
                ) {
                    Err(GitError::Failed(message))
                        if message.contains("does not have any commits") =>
                    {
                        Vec::new()
                    }
                    other => other?,
                }
            } else {
                git::ahead(root, range, against, crate::root::push::PAGE)?
            };
            log::parse(&bytes).map_err(|err| GitError::Failed(err.to_string()))?
        }
    };
    Ok(crate::git::Ahead {
        branch,
        remotes,
        upstream,
        commits,
    })
}

/// How often the worktrees are asked for while the window is the one
/// being looked at — M12.
///
/// **Three seconds for the list alone, since 2026-09-24.** M12's task 0
/// measured one `git worktree list` at about seven milliseconds and called
/// the cadence free; M12.4 then added a `show` and a `merge-base` and a
/// `diff` per worktree to the same ask, and every open root asked it. With
/// eighteen projects open a focused window with every agent idle spent
/// 10–20 % of a core, and as much again in git, on about thirty forks a
/// second — no one call heavy, the count was the cost, and fork in a
/// 200 MB threaded process is not cheap either.
///
/// So the cadence's ask is [`Ask::Worktrees`] with `full: false`: one
/// process per root, and the times and the lines counted again only when
/// the list moved — a worktree added or removed, a head or a branch moved,
/// a lock taken. A root whose disk moved is counted on the next tick,
/// and the window coming back, a write and a `guild` command count every
/// root at once. Measured on twenty roots of twelve fixture repositories,
/// debug builds: 140 git processes a round before, 20 after — 47 a second
/// against 7, and the window's own CPU 8 % against 2.5 %.
///
/// **Since 2026-10-30:** Root admits canonical common Git directories and
/// schedules one list per repository wave on an existing StatusWorker. The
/// history above prices the old per-root scheduler; enrichment is shared too.
/// A dirty subscriber or explicit refresh still requires current counts.
///
/// **The price, accepted**: an agent editing a worktree that is not open
/// as a root moves that row's `+N −M` on the disk and not in the list, so
/// the row catches up at the next activation, commit or move of its head
/// rather than within three seconds.
const WORKTREES_EVERY: std::time::Duration = std::time::Duration::from_secs(3);

/// The workers the root keeps, one per open project.
pub(crate) type StatusWorkers = HashMap<ProjectId, StatusWorker>;

/// What each project's worker last answered: the map, or why there is
/// none — not a repository, no git.
pub(crate) type GitAnswers = HashMap<ProjectId, Result<StatusMap, GitError>>;

impl Root {
    /// A worker for every open project and none for a closed one; then
    /// whatever they have answered — into the trees and the Changes
    /// panel, and a refusal into a notice.
    pub(crate) fn poll_git(&mut self, cx: &mut Context<Self>) {
        // A project group's folder gets none — GLD-055 decision 9: the
        // worker feeds only git marks, Changes and the worktree list, and a
        // group root is shown, not indexed. One bound after its worker
        // started loses it here.
        let projects: Vec<(ProjectId, PathBuf)> = self
            .desk
            .projects()
            .filter(|project| !self.desk.is_project_group_root(project.id))
            .map(|project| (project.id, project.path.clone()))
            .collect();
        self.status_workers
            .retain(|id, _| projects.iter().any(|(open, _)| open == id));
        self.worktrees.retain(|common, _| {
            self.desk
                .projects()
                .any(|p| p.repository.as_ref() == Some(common))
        });
        let mut started = Vec::new();
        for (id, path) in &projects {
            if !self.status_workers.contains_key(id) {
                self.status_workers
                    .insert(*id, StatusWorker::start(path.clone()));
                started.push(*id);
            }
        }
        for project in started {
            self.git_thread_started(project, cx);
        }
        // Every few seconds after that, while the window is where the
        // person is looking: a worktree added in a terminal is somebody
        // else's write, and nothing tells the window about it. M12.
        let due = self
            .worktrees_asked
            .is_none_or(|asked| asked.elapsed() >= WORKTREES_EVERY);
        if due && self.window_focused {
            self.poll_worktrees();
        }
        let mut updates = Vec::new();
        let mut sources = Vec::new();
        let mut landed = Vec::new();
        let mut refused = Vec::new();
        let mut stash_gone = Vec::new();
        let mut outside = Vec::new();
        let mut committed = Vec::new();
        let mut logs = Vec::new();
        let mut reveals = Vec::new();
        let mut files = Vec::new();
        let mut blames = Vec::new();
        let mut inline_blames = Vec::new();
        let mut branch_lists = Vec::new();
        let mut stash_lists = Vec::new();
        let mut ran = Vec::new();
        let mut contains = Vec::new();
        let mut pushes = Vec::new();
        let mut worktree_lists = Vec::new();
        let mut gone = Vec::new();
        self.drop_orphaned_hunks(cx);
        let mut hunk_news = Vec::new();
        for (id, worker) in &self.status_workers {
            let taken = worker.take();
            if !taken.hunk_events.is_empty() {
                hunk_news.push((*id, taken.hunk_events));
            }
            if let Some(value) = taken.update_sources {
                sources.push((*id, value));
            }
            if let Some(value) = taken.branch_updated {
                updates.push((*id, value));
            }
            if taken.gone && !worker.dead {
                gone.push(*id);
            }
            if let Some(answer) = taken.map {
                self.git_status.insert(*id, answer);
                landed.push(*id);
            }
            refused.extend(taken.refused.into_iter().map(|r| (*id, r)));
            stash_gone.extend(taken.stash_gone.into_iter().map(|_| *id));
            outside.extend(taken.outside);
            committed.extend(taken.committed.into_iter().map(|c| (*id, c)));
            logs.extend(taken.logs.into_iter().map(|l| (*id, l)));
            reveals.extend(taken.reveals.into_iter().map(|r| (*id, r)));
            files.extend(taken.files.into_iter().map(|f| (*id, f)));
            contains.extend(taken.contains.into_iter().map(|c| (*id, c)));
            blames.extend(taken.blames.into_iter().map(|b| (*id, b)));
            inline_blames.extend(taken.inline_blames.into_iter().map(|b| (*id, b)));
            if let Some(branches) = taken.branches {
                branch_lists.push((*id, branches));
            }
            if let Some(stashes) = taken.stashes {
                stash_lists.push((*id, stashes));
            }
            if let Some(ahead) = taken.push {
                pushes.push((*id, ahead));
            }
            if let Some(answer) = taken.worktrees {
                worktree_lists.push((*id, answer));
            }
            ran.extend(taken.ran.into_iter().map(|line| (*id, line)));
        }
        for (project, value) in sources {
            if let Ok(snapshot) = &value
                && let Some(default) = &snapshot.default
                && !self
                    .settings
                    .branch_update_sources
                    .contains_key(&snapshot.root)
            {
                self.settings
                    .branch_update_sources
                    .insert(snapshot.root.clone(), default.clone());
                self.store_settings();
            }
            self.update_sources.insert(project, value);
            self.held_update_sources_landed(project, cx);
            if self.active_project == Some(project)
                && self.branch_face() == Some(crate::root::branches::Face::Sources)
            {
                self.settle_picker();
            }
            cx.notify();
        }
        for (project, result) in updates {
            self.updating_branches.remove(&project);
            let description = self.ui().branch_update_receipt(&result);
            self.settings.remember_branch_update(result.clone());
            self.store_settings();
            self.notices.push_terminal(
                if result.failed() {
                    crate::notify::Level::Warning
                } else {
                    crate::notify::Level::Info
                },
                description,
                project,
                result.root,
                std::time::Instant::now(),
            );
            self.refresh_branches(project);
            cx.notify();
        }
        for project in &gone {
            self.updating_branches.remove(project);
            self.held_updates.remove(project);
            self.forget_hunks(*project, cx);
        }
        // A thread that has gone says so, once, and is started again once.
        // Its start asks again for whatever was waiting on it — the trap of
        // a tab restored before its thread, met from the other end.
        // Decision 19 of M5.11.
        for project in &gone {
            self.git_thread_gone(*project, cx);
        }
        // The Console's lines, filed under the project whose git ran
        // them. Before the refusals, so that a person who reads the
        // notice and switches to the Console finds the command already
        // there.
        for (project, line) in &ran {
            // Completion is a new boundary for this repository, even on refusal.
            self.worktree_reads
                .request_project(&self.status_workers, *project);
            // A push or a switch from Guild asks its MRs again (GLD-063).
            if super::forge::asks_again(line) {
                self.forge_wrote(*project);
            }
        }
        for (project, line) in ran {
            // A `guild` command may be holding a connection open for
            // exactly this write — M13.
            self.answer_pending_write(&line);
            self.consoles.entry(project).or_default().push(line);
            if self.active_project == Some(project) {
                self.show_console_end();
            }
        }
        // Auto fast-forward after the Console's lines, so a hand-off's write
        // is filed before its outcome is handled — decision 21.
        self.poll_auto_ff(std::time::Instant::now(), cx);
        for (project, result) in stash_lists {
            self.stashes_landed(project, result, cx);
        }
        for (project, result) in branch_lists {
            self.branches_landed(project, result, cx);
        }
        for (project, (path, result)) in blames {
            self.blame_landed(project, &path, result, cx);
        }
        for (
            project,
            InlineBlameReply {
                entity,
                generation,
                path,
                runs: result,
            },
        ) in inline_blames
        {
            let view = self
                .tabs
                .iter()
                .filter(|t| t.project == project)
                .flat_map(crate::Tab::bodies)
                .find_map(|body| match body {
                    crate::Body::File {
                        tab,
                        origin: crate::Origin::Project,
                        ..
                    } if tab.entity_id() == entity => Some(tab.clone()),
                    _ => None,
                });
            if let Some(view) = view {
                view.update(cx, |file, cx| {
                    file.blame_landed(generation, &path, result, cx)
                });
            }
        }
        for (project, page) in logs {
            self.scoped_log_landed(project, page, cx);
        }
        for (project, reply) in reveals {
            self.reveal_commit_landed(project, reply, cx);
        }
        for (project, (sha, branches)) in contains {
            self.log_contains_landed(project, sha, branches);
        }
        // **Refusals before the lists that follow them**: a refused push
        // is answered by a re-read as well, and `push_landed` must see the
        // refusal first or it would take the push for one that landed and
        // close Push Commits. M8.3.6.
        for (project, (write, err)) in &refused {
            tracing::warn!(?write, %err, "git refused a write");
            // git's own words, without our wrapper's: `GitError::Failed`
            // prints "git failed: …" for a log line, and the notice
            // already says who refused. Both together read as a stutter.
            let words = match &err {
                GitError::Failed(words) => words.clone(),
                other => other.to_string(),
            };
            // **A refused push is said in Push Commits** — M8.3.6 — where
            // the person pressed it, and in a notice only when that window
            // is not up.
            if matches!(write, Write::Push { .. }) {
                self.push_refused(*project, words.clone(), cx);
                if self
                    .satellite(crate::windows::Satellite::Push, cx)
                    .is_some()
                {
                    continue;
                }
            }
            let words = if words == "Unsupported non-UTF-8 path" {
                self.ui()
                    .text(crate::i18n::Text::CommitUnsupportedPath)
                    .to_owned()
            } else if words == "Path contains NUL" {
                self.ui().text(crate::i18n::Text::CommitPathNul).to_owned()
            } else {
                words
            };
            let text = self.ui().notice_git_refused(&words);
            self.warn(text);
        }
        for (project, ahead) in pushes {
            self.push_landed(project, ahead, cx);
        }
        for (project, (sha, result)) in files {
            // **Both readers, from one ask.** The Log and the Push tab
            // open commits from the same list in the same format, so a
            // second `Ask::Files` would be a second process for an
            // answer already in hand — and two maps of the same commit
            // free to disagree.
            self.push_files_landed(project, &sha, &result, cx);
            self.log_files_landed(project, sha, result, cx);
        }
        for (project, (short, summary)) in committed {
            self.commit_landed(project, &short, &summary, cx);
        }
        for project in &landed {
            self.refresh_tree_diffs(*project, cx);
            self.refresh_changes(*project, cx);
        }
        // After the ticks follow the map: a commit held for a hunk write is
        // planned from the status that write produced. GLD-026.
        for (project, events) in hunk_news {
            self.hunks_landed(project, events, cx);
        }
        // A drop whose entry had gone: nothing was dropped, and the list
        // the person was looking at is not the list there is, so it is
        // asked for again. Decision 17 of M5.11.
        for paths in outside {
            let paths: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
            let text = self.ui().commit_refused_outside(&paths);
            self.warn(text);
        }
        for project in &stash_gone {
            let text = self.ui().text(crate::i18n::Text::NoticeStashGone);
            self.inform(text.to_owned());
            self.refresh_branches(*project);
        }
        // What git says about the repositories: which one a root belongs
        // to, and the rows its group draws. **A failure leaves what
        // stands** — a column that blanked itself whenever git was busy
        // would be worse than one a few seconds behind. M12.
        let mut regrouped = false;
        let mut answered = Vec::new();
        // Resolve shared replies only after writes and worker replacements have
        // requested their newer epochs in this polling turn.
        worktree_lists.extend(self.worktree_reads.poll(&self.status_workers));
        for (project, answer) in worktree_lists {
            let Ok((common, list)) = answer else {
                continue;
            };
            // The common key was canonicalized by the worker; Git prints absolute
            // canonical checkout paths in its porcelain list. No identity IO here.
            let main = list
                .first()
                .filter(|w| !w.bare && !w.prunable)
                .map(|w| w.path.clone());
            self.scoped_settings.identities.insert(
                project,
                Ok(crate::repository_settings::RepositoryIdentity {
                    key: common.clone(),
                    main,
                    git: true,
                }),
            );
            answered.push(common.clone());
            if self.desk.set_repository(project, Some(common.clone())) {
                // Remembered, so the next window draws the column grouped
                // from its first frame rather than a second later.
                self.save();
                regrouped = true;
            }
            if self.worktrees.get(&common) != Some(&list) {
                self.worktrees.insert(common.clone(), list);
                regrouped = true;
            }
            // The list after a push or a switch made from Guild is the one
            // the merge requests wait for: the coordinator drops a list read
            // before the write (GLD-063, code review r1, bugs+impl-5).
            self.forge_snapshot_landed(&common);
        }

        // **A worktree removed outside the window is closed by it** — a tool
        // outside Guild removing worktrees left the column drawing rows for
        // paths that no longer exist. Only on git's own answer for the
        // repository and a folder with no checkout in it, and only when
        // nothing in any seat would be lost: `column::removed_outside` says
        // why each of those is there. What it keeps, a person closes from the
        // row, with the usual questions.
        let removed: Vec<ProjectId> = self
            .desk
            .projects()
            .filter(|project| {
                project
                    .repository
                    .as_ref()
                    .is_some_and(|common| answered.contains(common))
            })
            .filter(|project| {
                let list = project
                    .repository
                    .as_ref()
                    .and_then(|common| self.worktrees.get(common))
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                crate::column::removed_outside(
                    list,
                    &project.path,
                    !self.dirty_file_tabs(Some(project.id), cx).is_empty(),
                    self.has_live_session(project.id, cx),
                )
            })
            .map(|project| project.id)
            .collect();
        for project in removed {
            tracing::info!(?project, "a worktree removed outside the window is closed");
            for seat in self.desk.holders(project) {
                self.close_project_in(seat, project, cx);
            }
            regrouped = true;
        }

        // A `guild` command held until the list it will be read back from
        // agrees — M13.
        if regrouped {
            self.answer_settled(cx);
        }

        if !landed.is_empty()
            || !refused.is_empty()
            || !stash_gone.is_empty()
            || !gone.is_empty()
            || regrouped
        {
            cx.notify();
        }
    }

    /// A project's git thread has just started: the asks that were waiting
    /// on it are made now.
    ///
    /// A log or an annotate tab restored before the thread existed asked
    /// nobody, and so did a thread that died with their asks in it: the
    /// thread's first asks are theirs.
    fn git_thread_started(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        self.refresh_source_git_baselines(project, cx);
        self.refresh_log(project);
        self.refresh_blames(project, cx);
        // Push in a dock asks at the first render, which can be before
        // this thread — its entry is made and its ask dropped. M12.3.
        if self.push.knows(project) {
            self.ask_push(project);
        }
        // And the column's: a group is drawn from what git lists, so a
        // root's first ask is the one that fills it. M12.
        self.worktree_reads
            .request_project(&self.status_workers, project);
        // What a person is looking at, asked of a thread that may have
        // died with the ask in it: the branch popup's two lists, and an
        // opened commit's files and branches in the Log and in Push. Each
        // only when its answer never landed, so a first start asks nothing
        // twice. Found by the review of M5.11's task 21.
        if self.branch_popup_open() && self.active_project == Some(project) {
            self.refresh_branches(project);
        }
        let Some(worker) = self.status_workers.get(&project) else {
            return;
        };
        if let Some(state) = self.logs.get(&project)
            && let Some(sha) = &state.opened
        {
            if !state.files.contains_key(sha) {
                worker.files(sha.clone());
            }
            if !sha.contains(',') && !state.contains.contains_key(sha) {
                worker.contains(sha.clone());
            }
        }
        if let Some(state) = self.push.of(project)
            && let Some(sha) = &state.opened
            && !state.files.contains_key(sha)
        {
            worker.files(sha.clone());
        }
    }

    /// A project's git thread has gone: started once more, or left dead
    /// and said so. Decision 19 of M5.11.
    ///
    /// **Said, because nothing else would say it.** A dead thread answers
    /// nothing, so the marks, the Commit panel and the Log stood on their
    /// last answer and every write went nowhere — with a line in a log
    /// nobody opens.
    fn git_thread_gone(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(open) = self.desk.project(project) else {
            return;
        };
        let (path, name) = (open.path.clone(), open.name.clone());
        let Some(worker) = self.status_workers.get_mut(&project) else {
            return;
        };
        tracing::warn!(project = %name, restarts = worker.restarts, "the git thread has gone");
        let text = match after_death(worker.restarts) {
            AfterDeath::Restart => {
                let mut fresh = StatusWorker::start(path);
                fresh.restarts = worker.restarts.saturating_add(1);
                *worker = fresh;
                self.git_thread_started(project, cx);
                self.ui().notice_git_restarted(&name)
            }
            AfterDeath::Leave => {
                worker.dead = true;
                self.ui().notice_git_stopped(&name)
            }
        };
        self.fail(text);
    }

    /// Ask every open root which repository it is in and what worktrees
    /// it has, every row counted — M12. Never blocks; the answers come to
    /// `poll_git`. For the moments something outside the list may have
    /// moved: the window coming back, and a `guild` command that will
    /// read the list back.
    pub(crate) fn ask_worktrees(&mut self) {
        self.worktree_reads.request(&self.status_workers, |_| true);
        self.worktrees_stale.clear();
        self.worktrees_asked = Some(std::time::Instant::now());
    }

    /// The cadence's ask — 2026-09-24: every root lists its worktrees,
    /// and only a root whose disk moved since, or whose list did, counts
    /// them again. See [`WORKTREES_EVERY`].
    fn poll_worktrees(&mut self) {
        self.worktree_reads
            .request(&self.status_workers, |project| {
                self.worktrees_stale.contains(&project)
            });
        self.worktrees_stale.clear();
        self.worktrees_asked = Some(std::time::Instant::now());
    }

    /// Drive the production cadence without faking native activation in hidden trials.
    #[cfg(feature = "native-preview")]
    pub(crate) fn diagnostic_worktree_cadence(&mut self) {
        if self
            .worktrees_asked
            .is_some_and(|at| at.elapsed() >= WORKTREES_EVERY)
        {
            self.poll_worktrees();
        }
    }

    /// Ask a project's worker for a fresh map — after a batch from the
    /// watcher, whether it named files or a change under `.git`.
    ///
    /// `moved` says whether that batch touched `HEAD`, the index or a
    /// ref. The marks are asked for either way, because an ordinary write
    /// does change them. **The log is asked only when it is true** — its
    /// first page replaces the list, so asking on every autosave threw
    /// away a page a person had scrolled to and closed the commit they
    /// were reading. A person pressing refresh passes `true`: they asked
    /// for everything.
    pub(crate) fn ask_git(&mut self, project: ProjectId, moved: bool, cx: &mut Context<Self>) {
        if let Some(worker) = self.status_workers.get(&project) {
            worker.ask();
        }
        // An edit moves this root's `+N −M` and not the list, so the
        // cadence's next ask counts it again. M12.4, 2026-09-24.
        self.worktrees_stale.insert(project);
        // A blame of the working tree names the lines not committed yet,
        // so it changes when the file does — every batch, not only a
        // move of `HEAD`.
        self.refresh_blames(project, cx);
        if moved {
            self.refresh_source_git_baselines(project, cx);
            self.refresh_log(project);
            // A push, a commit or a fetch from a terminal moves a ref and
            // changes what is waiting; Push was asked only when it opened,
            // and went on offering commits already on the remote.
            if self.push.knows(project) {
                self.ask_push(project);
            }
        }
    }
}

/// The wall clock, in milliseconds.
///
/// Read on the git thread, at the moment the command finished, rather
/// than when the window notices: the window notices on the watch loop's
/// hundred-millisecond tick, and a console that timed its own polling
/// would say two writes happened together when they did not.
/// A write's Console line, success or failure — decision 22.
/// Says a manual Update branch is still in flight for as long as its ask
/// lives — run, or dropped unrun by a thread that went away — whatever
/// becomes of the root and the worker that asked. Auto fast-forward reads
/// it per repository: closing the root does not stop the fetch.
pub(crate) struct InFlight(std::sync::Arc<std::sync::atomic::AtomicBool>);

impl InFlight {
    /// The ask's token, and the flag the window keeps.
    pub(crate) fn new() -> (Self, std::sync::Arc<std::sync::atomic::AtomicBool>) {
        let flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        (Self(flag.clone()), flag)
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::Release);
    }
}

/// A write's Console line, built where it ran.
pub(crate) fn console_line(
    root: &str,
    write: &Write,
    result: &Result<git::Said, GitError>,
) -> crate::console::Line {
    crate::console::Line {
        at: now_millis(),
        root: root.to_owned(),
        argv: write.args(),
        said: match result {
            Ok(said) => said.text(),
            Err(error) => error.to_string(),
        },
        failed: result.is_err(),
    }
}

fn report_ran(answers: &mpsc::Sender<Answer>, write: &Write, result: &Result<git::Said, GitError>) {
    let said = match result {
        Ok(said) => said.text(),
        Err(error) => error.to_string(),
    };
    let _ = answers.send(Answer::Ran {
        at: now_millis(),
        argv: write.args(),
        said,
        failed: result.is_err(),
    });
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

impl Root {
    /// The handlers for the diffs, the Changes panel and the commit.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_git(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &ShowDiff, window, cx| root.show_diff(window, cx)))
            .on_action(cx.listener(|root, _: &ShowChanges, window, cx| {
                root.open_diff(None, guild_session::Base::Head, window, cx)
            }))
            .on_action(
                cx.listener(|root, _: &crate::actions::Commit, _window, cx| {
                    root.open_commit_message(cx)
                }),
            )
            .on_action(cx.listener(|root, _: &Annotate, window, cx| root.annotate(window, cx)))
            .on_action(
                cx.listener(|root, _: &NextChange, _window, cx| root.step_diff_change(true, cx)),
            )
            .on_action(
                cx.listener(|root, _: &PreviousChange, _window, cx| {
                    root.step_diff_change(false, cx)
                }),
            )
            .on_action(
                cx.listener(|root, _: &RefreshChanges, _window, cx| root.refresh_changes_now(cx)),
            )
            .on_action(cx.listener(|root, _: &ShowSelectedDiff, window, cx| {
                root.show_selected_diff(window, cx)
            }))
            .on_action(cx.listener(|root, _: &TickAllChanges, _window, cx| root.tick_all(true, cx)))
            .on_action(
                cx.listener(|root, _: &UntickAllChanges, _window, cx| root.tick_all(false, cx)),
            )
            .on_action(
                cx.listener(|root, _: &CollapseChanges, _window, cx| root.fold_all(true, cx)),
            )
            .on_action(cx.listener(|root, _: &ExpandChanges, _window, cx| root.fold_all(false, cx)))
            .on_action(cx.listener(|root, _: &ShowLog, _window, cx| root.show_log(cx)))
            // The Commit panel's two writing gestures. Both act on the
            // row the **keyboard** has — an action always reaches the
            // thing in front — which is why neither shows a stroke
            // beside its menu item. M8.8.4.
            .on_action(
                cx.listener(|root, _: &Rollback, window, cx| root.rollback_selected(window, cx)),
            )
            .on_action(cx.listener(|root, _: &AddToVcs, _window, cx| root.add_to_vcs_selected(cx)))
    }
}

#[cfg(test)]
mod tests {
    //! On a real repository under the temporary directory, with the git
    //! every machine this runs on has. Here rather than in `tests/`:
    //! the worker is the window's, and what a caller can reach is the
    //! panel it drives.

    use std::time::{Duration, Instant};

    use super::*;
    use crate::git::status::Kind;

    /// A folder of the test's own, gone when the test lets go of it, with
    /// a repository at `repo` inside — inside rather than at its root, so
    /// that a worktree a test adds at `../wt-…` lands in the same folder
    /// and goes with it instead of in `$TMPDIR` itself.
    pub(super) fn scratch(name: &str) -> tempfile::TempDir {
        let folder = tempfile::Builder::new()
            .prefix(&format!("guild-git-worker-{name}-"))
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path().join("repo");
        std::fs::create_dir_all(&dir).unwrap();
        git::run(&dir, &["init", "-q"]).expect("git init");
        // One commit, so that `HEAD` resolves: `restore --staged` puts an
        // index entry back to what `HEAD` has, and refuses on a branch
        // with no commit yet — a limit recorded in the plan. The verb is
        // spelled here for the fixture only; the crate's own commit is
        // `Write::Commit`, task 6.
        let committed = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Guild",
                "-c",
                "user.email=guild@example.invalid",
            ])
            .args(["commit", "-q", "--allow-empty", "-m", "init"])
            .current_dir(&dir)
            .status()
            .expect("git commit");
        assert!(committed.success());
        folder
    }

    /// GLD-063: the worktree on the repository's main branch — by
    /// `git::main_branch`'s rule, `origin/HEAD`'s local branch first — is
    /// marked in the snapshot, off the window's thread, and no other is.
    #[test]
    fn the_snapshot_marks_the_main_branch_s_worktree() {
        let folder = scratch("main-branch");
        let dir = folder.path().join("repo");
        git::run(&dir, &["branch", "-m", "main"]).unwrap();
        let feat = folder.path().join("wt-feat");
        // A fixture's worktree, spelled here only: the crate's own is
        // `Write::WorktreeAdd`.
        let added = std::process::Command::new("git")
            .args(["worktree", "add", "-q", "-b", "feat"])
            .arg(&feat)
            .current_dir(&dir)
            .status()
            .unwrap();
        assert!(added.success());
        let marked = |counted: &mut Counted| {
            let (_, list) = counted.read(&dir, true, None).unwrap().unwrap();
            list.iter()
                .filter(|tree| tree.main_branch)
                .map(|tree| tree.branch.clone().unwrap())
                .collect::<Vec<_>>()
        };
        assert_eq!(marked(&mut Counted::default()), vec!["main".to_owned()]);
        // `origin/HEAD` naming `feat` makes it the main branch instead.
        git::run(&dir, &["update-ref", "refs/remotes/origin/feat", "HEAD"]).unwrap();
        git::run(
            &dir,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/feat",
            ],
        )
        .unwrap();
        assert_eq!(marked(&mut Counted::default()), vec!["feat".to_owned()]);
    }

    #[test]
    fn annotation_reveal_reads_exact_old_commit_without_mutating_git() {
        let folder = scratch("annotation-reveal");
        let dir = folder.path().join("repo");
        git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
        git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
        std::fs::write(dir.join("old.txt"), "original\n").unwrap();
        git::run(&dir, &["add", "old.txt"]).unwrap();
        git::run(&dir, &["commit", "-q", "-m", "original"]).unwrap();
        let sha = String::from_utf8(git::run(&dir, &["rev-parse", "HEAD"]).unwrap())
            .unwrap()
            .trim()
            .to_owned();
        git::run(&dir, &["mv", "old.txt", "new.txt"]).unwrap();
        git::run(&dir, &["commit", "-q", "-m", "rename"]).unwrap();
        let head = git::run(&dir, &["rev-parse", "HEAD"]).unwrap();
        let status = git::run(&dir, &["status", "--porcelain"]).unwrap();
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        worker.reveal_commit(super::super::log::Navigation {
            epoch: 77,
            sha: sha.clone(),
            path: Some("old.txt".into()),
            original_line: Some(1),
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        let reply = loop {
            if let Some(reply) = worker.take().reveals.into_iter().next() {
                break reply;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(reply.intent.epoch, 77);
        assert_eq!(reply.commit.unwrap().sha, sha);
        assert_eq!(reply.files.unwrap()[0].path, Path::new("old.txt"));
        assert_eq!(git::run(&dir, &["rev-parse", "HEAD"]).unwrap(), head);
        assert_eq!(git::run(&dir, &["status", "--porcelain"]).unwrap(), status);
    }

    /// The next map the worker answers, on a clock rather than a count:
    /// a real git takes what it takes, and more under a full `just check`.
    fn next(worker: &StatusWorker) -> Taken {
        let deadline = Instant::now() + Duration::from_secs(20);
        // Accumulated across takes: a commit's answer and the map of the
        // walk after it arrive one after the other, and a take between
        // them must not lose the first.
        let mut all = Taken::default();
        loop {
            let taken = worker.take();
            all.refused.extend(taken.refused);
            all.committed.extend(taken.committed);
            if taken.map.is_some() {
                all.map = taken.map;
            }
            if all.map.is_some() || !all.refused.is_empty() {
                return all;
            }
            assert!(Instant::now() < deadline, "nothing from the worker in 20 s");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A repository on `main` at A, with B one commit ahead on `next`.
    fn fast_forwardable(name: &str) -> (tempfile::TempDir, PathBuf, String, String) {
        let folder = scratch(name);
        let dir = folder.path().join("repo");
        git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
        git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
        git::run(&dir, &["branch", "-q", "-M", "main"]).unwrap();
        let head = |dir: &Path| {
            String::from_utf8(git::run(dir, &["rev-parse", "HEAD"]).unwrap())
                .unwrap()
                .trim()
                .to_owned()
        };
        let a = head(&dir);
        git::run(&dir, &["switch", "-q", "-c", "next"]).unwrap();
        commit(&dir, "b");
        let b = head(&dir);
        git::run(&dir, &["switch", "-q", "main"]).unwrap();
        (folder, dir, a, b)
    }

    fn handed(dir: &Path, a: &str, b: &str) -> git::auto_ff::Handoff {
        git::auto_ff::Handoff {
            worktree: dir.to_path_buf(),
            branch: "main".into(),
            expected: a.into(),
            oid: b.into(),
        }
    }

    fn flag(value: bool) -> std::sync::Arc<std::sync::atomic::AtomicBool> {
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(value))
    }

    /// Hand a fast-forward to the worker and wait on its own answer; the
    /// Console's lines are sent before it, on the worker's channel.
    fn fast_forward(
        worker: &StatusWorker,
        handoff: git::auto_ff::Handoff,
        clean: std::sync::Arc<std::sync::atomic::AtomicBool>,
        cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> (git::auto_ff::Landed, Vec<Vec<String>>) {
        let (answer, done) = mpsc::channel();
        assert!(worker.fast_forward(handoff, clean, cancelled, answer));
        let (landed, ran) = done
            .recv_timeout(Duration::from_secs(20))
            .expect("no fast-forward answer in 20 s");
        (landed, ran.into_iter().map(|line| line.argv).collect())
    }

    /// The hand-off's checks run on the root's own thread, the buffers and
    /// the cancellation last, and only a write that ran reaches the
    /// Console — silently: a fast-forward refused is not a notice.
    #[test]
    fn a_handed_off_fast_forward_is_checked_on_the_roots_thread() {
        let (_folder, dir, a, b) = fast_forwardable("ff-clean");
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        let (landed, ran) = fast_forward(&worker, handed(&dir, &a, &b), flag(true), flag(false));
        assert_eq!(landed, git::auto_ff::Landed::FastForwarded);
        assert_eq!(ran.len(), 1);
        assert!(ran[0].contains(&"--ff-only".to_owned()), "{ran:?}");
        let walked = next(&worker);
        assert!(walked.refused.is_empty());

        let (_folder, dir, a, b) = fast_forwardable("ff-unsaved");
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        let (landed, ran) = fast_forward(&worker, handed(&dir, &a, &b), flag(false), flag(false));
        assert_eq!(
            landed,
            git::auto_ff::Landed::Skipped(git::auto_ff::Reason::Unsaved)
        );
        assert!(ran.is_empty());

        let (_folder, dir, a, b) = fast_forwardable("ff-dirty");
        std::fs::write(dir.join("dirt"), "x").unwrap();
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        let (landed, ran) = fast_forward(&worker, handed(&dir, &a, &b), flag(true), flag(false));
        assert_eq!(
            landed,
            git::auto_ff::Landed::Skipped(git::auto_ff::Reason::Dirty)
        );
        assert!(ran.is_empty());

        // Cancelled while it waited in the queue — the setting turned off
        // after the hand-off: the worker reads the run's own token right
        // before the write, so a token already set writes nothing.
        let (_folder, dir, a, b) = fast_forwardable("ff-cancelled-first");
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        let (landed, ran) = fast_forward(&worker, handed(&dir, &a, &b), flag(true), flag(true));
        assert_eq!(landed, git::auto_ff::Landed::Cancelled);
        assert!(ran.is_empty());
    }

    /// Review r2: an ask for another checkout — a root in a nested
    /// checkout handed the outer one's fast-forward — is refused unrun.
    #[test]
    fn a_fast_forward_for_another_checkout_is_refused() {
        let (_folder, dir, a, b) = fast_forwardable("ff-other");
        let elsewhere = tempfile::tempdir().unwrap();
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        let mut handoff = handed(&dir, &a, &b);
        handoff.worktree = elsewhere.path().to_path_buf();
        let (landed, ran) = fast_forward(&worker, handoff, flag(true), flag(false));
        assert_eq!(
            landed,
            git::auto_ff::Landed::Skipped(git::auto_ff::Reason::Refused)
        );
        assert!(ran.is_empty());
        assert_eq!(
            String::from_utf8(git::run(&dir, &["rev-parse", "HEAD"]).unwrap())
                .unwrap()
                .trim(),
            a
        );
    }

    #[test]
    fn status_walk_refreshes_update_identity_after_a_commit_and_switch() {
        let folder = scratch("update-snapshot");
        let dir = folder.path().join("repo");
        let worker = StatusWorker::start(dir.clone());
        next(&worker);
        worker.run(Write::Create("update-target".into()));
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut snapshot = None;
        loop {
            let taken = worker.take();
            if let Some(Ok(value)) = taken.update_sources {
                snapshot = Some(value);
            }
            if taken.map.is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            snapshot
                .expect("write must refresh update sources")
                .head
                .unwrap()
                .0,
            "refs/heads/update-target"
        );
        std::process::Command::new("git")
            .args([
                "-c",
                "user.name=Guild",
                "-c",
                "user.email=guild@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "external",
            ])
            .current_dir(&dir)
            .output()
            .unwrap();
        worker.ask();
        let mut snapshot = None;
        loop {
            let taken = worker.take();
            if let Some(Ok(value)) = taken.update_sources {
                snapshot = Some(value);
            }
            if taken.map.is_some() {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(20));
        }
        let expected = git::branch_update::Snapshot::read(&dir).unwrap().head;
        assert_eq!(
            snapshot
                .expect("status walk must refresh update sources")
                .head,
            expected
        );
    }

    fn kind_of(map: &StatusMap, path: &str) -> Option<Kind> {
        map.entry(Path::new(path)).map(|entry| entry.kind.clone())
    }

    /// The next worktree answer, on a clock for the reason [`next`] is on
    /// one.
    fn next_worktrees(
        worker: &StatusWorker,
    ) -> Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(answer) = worker.take().worktrees {
                return answer;
            }
            assert!(
                Instant::now() < deadline,
                "no worktrees from the worker in 20 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The next worktree answer holding `n` of them — a write is followed
    /// by a walk of its own, so the list taken right after one may still
    /// be the list from before it. M12.
    fn next_worktrees_after(
        worker: &StatusWorker,
        n: usize,
    ) -> Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            worker.worktrees();
            if let Some(Ok((common, list))) = worker.take().worktrees
                && list.len() == n
            {
                return Ok((common, list));
            }
            assert!(
                Instant::now() < deadline,
                "no list of {n} worktrees from the worker in 20 s"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The next write the worker says git refused.
    fn next_refusal(worker: &StatusWorker) -> GitError {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let mut taken = worker.take();
            if let Some((_, err)) = taken.refused.pop() {
                return err;
            }
            assert!(
                Instant::now() < deadline,
                "no refusal from the worker in 20 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// **The two writes M12 adds, against a real git.** `worktree add`
    /// makes a folder and a branch, and the next list has both; `worktree
    /// remove` without `--force` refuses one with a change in it, in
    /// git's own words, and takes a clean one.
    #[test]
    fn a_worktree_is_added_listed_and_removed_and_a_dirty_one_is_refused() {
        let folder = scratch("worktree-writes");
        let dir = folder.path().join("repo");
        let made = dir.parent().unwrap().join("wt-made");
        let worker = StatusWorker::start(dir.clone());

        worker.run(Write::WorktreeAdd {
            path: made.clone(),
            branch: "made".to_owned(),
            base: None,
        });
        let (_, list) = next_worktrees_after(&worker, 2).expect("git listed them");
        assert!(
            list.iter()
                .any(|worktree| worktree.branch.as_deref() == Some("made")),
            "the worktree git just made is in the next list"
        );
        assert!(made.join(".git").exists(), "and it is on the disk");

        std::fs::write(made.join("dirt.txt"), "x").unwrap();
        worker.run(Write::WorktreeRemove(made.clone()));
        let refused = next_refusal(&worker);
        assert!(
            refused
                .to_string()
                .contains("contains modified or untracked files"),
            "git refuses a worktree with something in it: {refused}"
        );
        assert!(made.exists(), "and it is still there");

        std::fs::remove_file(made.join("dirt.txt")).unwrap();
        worker.run(Write::WorktreeRemove(made.clone()));
        let (_, list) = next_worktrees_after(&worker, 1).expect("git listed them");
        assert!(
            !list
                .iter()
                .any(|worktree| worktree.branch.as_deref() == Some("made")),
            "a clean one is taken"
        );
        assert!(!made.exists());
    }

    /// **What makes two roots one repository, asked of a real git.** The
    /// column groups by the common dir and draws the rows git lists, so
    /// both halves come from one ask: a worktree added in a terminal is
    /// in the next answer without the window being told. M12.
    #[test]
    fn the_worker_answers_the_common_dir_and_every_worktree() {
        let folder = scratch("worktrees");
        let dir = folder.path().join("repo");
        git::run(&dir, &["worktree", "add", "-q", "-b", "feat", "../wt-feat"])
            .expect("git worktree add");

        let worker = StatusWorker::start(dir.clone());
        worker.worktrees();
        let (common, list) = next_worktrees(&worker).expect("git listed them");

        assert_eq!(
            std::fs::canonicalize(&common).unwrap(),
            std::fs::canonicalize(dir.join(".git")).unwrap(),
            "the common dir is the main worktree's .git"
        );
        let branches: Vec<Option<&str>> = list
            .iter()
            .map(|worktree| worktree.branch.as_deref())
            .collect();
        assert_eq!(branches.len(), 2, "the main worktree and the linked one");
        assert!(branches.contains(&Some("feat")));
    }

    /// Every git command the worker ran under the test's folder, in
    /// order, since the last time this was asked — read off `git::run`'s
    /// record. `lines` runs in a linked worktree, whose path git prints
    /// canonical, so the folder is matched both ways.
    fn ran_since(folder: &Path, seen: &mut usize) -> Vec<Vec<String>> {
        let canonical = std::fs::canonicalize(folder).unwrap();
        let all = git::ran_under(folder, &canonical);
        let fresh = all[*seen..].to_vec();
        *seen = all.len();
        fresh
    }

    /// The worktree answer to the asks already made, if there was one:
    /// an ask for the branches goes behind them as a fence, and the
    /// thread answers in the order it was asked.
    fn worktrees_before_fence(
        worker: &StatusWorker,
    ) -> Option<Result<(PathBuf, Vec<crate::git::worktrees::Worktree>), GitError>> {
        worker.branches();
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut worktrees = None;
        loop {
            let taken = worker.take();
            if taken.worktrees.is_some() {
                worktrees = taken.worktrees;
            }
            if taken.branches.is_some() {
                return worktrees;
            }
            assert!(
                Instant::now() < deadline,
                "the fence did not answer in 20 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn feat_of(list: &[crate::git::worktrees::Worktree]) -> &crate::git::worktrees::Worktree {
        list.iter()
            .find(|tree| tree.branch.as_deref() == Some("feat"))
            .expect("the linked worktree is listed")
    }

    /// **The cadence's ask is one process when nothing moved — and says
    /// nothing.** 2026-09-24: eighteen open projects asked every three
    /// seconds for the common dir, the list, the heads' times and a
    /// merge-base and a diff per worktree came to about thirty forks a
    /// second from a focused window with every agent idle. A poll asks
    /// git for the list alone, and counts nothing again unless the list
    /// is not the one it counted last.
    ///
    /// The edit in the linked worktree is the trade-off written down: it
    /// moves that row's `+N −M` and not the list, so the poll does not see
    /// it, and the full ask — activation, a watcher batch, a command — does.
    #[test]
    fn a_poll_with_nothing_moved_lists_and_answers_nothing() {
        let folder = scratch("poll-still");
        let dir = folder.path().join("repo");
        let feat = folder.path().join("wt-feat");
        git::run(&dir, &["worktree", "add", "-q", "-b", "feat", "../wt-feat"])
            .expect("git worktree add");
        std::fs::write(feat.join("a.txt"), "one\n").unwrap();
        git::run(&feat, &["add", "a.txt"]).unwrap();

        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        worker.worktrees();
        let (_, list) = next_worktrees(&worker).expect("git listed them");
        assert_eq!(feat_of(&list).lines, Some((1, 0)), "the full ask counts");

        let mut seen = 0;
        let _ = ran_since(folder.path(), &mut seen);
        worker.poll_worktrees();
        assert!(
            worktrees_before_fence(&worker).is_none(),
            "an unchanged list is no news"
        );
        let ran = ran_since(folder.path(), &mut seen);
        let verbs: Vec<&str> = ran.iter().map(|args| args[0].as_str()).collect();
        assert_eq!(
            verbs,
            ["worktree", "branch"],
            "the list and the fence, nothing else: {ran:?}"
        );

        std::fs::write(feat.join("a.txt"), "one\ntwo\n").unwrap();
        worker.poll_worktrees();
        assert!(
            worktrees_before_fence(&worker).is_none(),
            "an edit is not a move of the list, and the poll does not count it"
        );

        worker.worktrees();
        let (_, list) = worktrees_before_fence(&worker)
            .expect("a full ask always answers")
            .expect("git listed them");
        assert_eq!(feat_of(&list).lines, Some((2, 0)), "and it counts again");
    }

    /// **A list that moved is counted again by the poll.** A commit in a
    /// worktree moves its `HEAD`, and a worktree added in a terminal is a
    /// row more — either is a different list, and the answer carries the
    /// times and the lines of the list it is.
    #[test]
    fn a_poll_counts_again_when_the_list_moved() {
        let folder = scratch("poll-moved");
        let dir = folder.path().join("repo");
        git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
        git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
        let feat = folder.path().join("wt-feat");
        git::run(&dir, &["worktree", "add", "-q", "-b", "feat", "../wt-feat"])
            .expect("git worktree add");

        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        worker.worktrees();
        let (_, list) = next_worktrees(&worker).expect("git listed them");
        assert_eq!(feat_of(&list).lines, Some((0, 0)));

        commit(&feat, "b.txt");
        worker.poll_worktrees();
        let (_, list) = worktrees_before_fence(&worker)
            .expect("a moved HEAD is news")
            .expect("git listed them");
        let moved = feat_of(&list);
        assert_eq!(moved.lines, Some((1, 0)), "counted from the new head");
        assert!(moved.committed.is_some(), "and timed");

        git::run(&dir, &["worktree", "add", "-q", "-b", "more", "../wt-more"])
            .expect("git worktree add");
        worker.poll_worktrees();
        let (_, list) = worktrees_before_fence(&worker)
            .expect("a row more is news")
            .expect("git listed them");
        assert_eq!(list.len(), 3);
        assert!(list.iter().all(|tree| tree.lines.is_some()), "{list:?}");
    }

    /// **A remote's URL changed in `.git/config` is news to the poll**
    /// — GLD-063, code review r1, docs+tests-5: the list and the heads do
    /// not move, and a merge request looked up by the old destination
    /// would stay. The config file's stamp is read with the list, no
    /// process more.
    #[test]
    fn a_remote_url_changed_in_config_is_news() {
        let folder = scratch("poll-remote");
        let dir = folder.path().join("repo");
        git::run(
            &dir,
            &[
                "remote",
                "add",
                "origin",
                "git@gitlab.example.com:team/one.git",
            ],
        )
        .unwrap();
        git::run(&dir, &["worktree", "add", "-q", "-b", "feat", "../wt-feat"])
            .expect("git worktree add");
        let worker = StatusWorker::start(dir.clone());
        settle(&worker);
        worker.worktrees();
        let (_, list) = next_worktrees(&worker).expect("git listed them");
        assert_eq!(
            feat_of(&list).push.as_ref().and_then(|p| p.url.as_deref()),
            Some("git@gitlab.example.com:team/one.git")
        );
        git::run(
            &dir,
            &[
                "remote",
                "set-url",
                "origin",
                "git@gitlab.example.com:team/another.git",
            ],
        )
        .unwrap();
        worker.poll_worktrees();
        let (_, list) = worktrees_before_fence(&worker)
            .expect("a changed remote is news")
            .expect("git listed them");
        assert_eq!(
            feat_of(&list).push.as_ref().and_then(|p| p.url.as_deref()),
            Some("git@gitlab.example.com:team/another.git")
        );
    }

    /// **The measurement `changes::rollback_of` rests on, pinned.**
    ///
    /// `restore --source=HEAD --staged --worktree` puts a modified file
    /// back — and on a path `HEAD` does not have it reads as *there
    /// should be nothing here* and **deletes the file from the disk**.
    /// That second half is the whole reason an added row is sent to
    /// `Write::Unstage` instead, and it is git's behaviour rather than
    /// ours: a test is the only thing that would notice it changing.
    #[test]
    fn a_rollback_restores_a_change_and_would_delete_a_file_head_never_had() {
        let folder = scratch("rollback");
        let dir = folder.path().join("repo");
        std::fs::write(dir.join("kept.txt"), "one\n").unwrap();
        git::run(&dir, &["add", "kept.txt"]).unwrap();
        git::run(&dir, &["commit", "-q", "-m", "kept"]).unwrap();
        std::fs::write(dir.join("kept.txt"), "two\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        git::run(&dir, &["add", "new.txt"]).unwrap();

        let worker = StatusWorker::start(dir.clone());
        let _ = next(&worker);
        worker.run(Write::Rollback(vec![PathBuf::from("kept.txt")]));
        let taken = next(&worker);
        assert!(taken.refused.is_empty(), "{:?}", taken.refused);
        assert_eq!(
            std::fs::read_to_string(dir.join("kept.txt")).unwrap(),
            "one\n",
            "the change is back to what HEAD holds"
        );

        // And the half that must never be reached from the panel.
        worker.run(Write::Rollback(vec![PathBuf::from("new.txt")]));
        let taken = next(&worker);
        assert!(taken.refused.is_empty(), "{:?}", taken.refused);
        assert!(
            !dir.join("new.txt").exists(),
            "git takes this to mean the file should not exist — which is \
             why an added row is unstaged instead"
        );
    }

    /// The next push read the worker answers, on the same clock as
    /// `next` and for the same reason.
    fn next_push(worker: &StatusWorker) -> Result<crate::git::Ahead, GitError> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(ahead) = worker.take().push {
                return ahead;
            }
            assert!(Instant::now() < deadline, "no push read in 20 s");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// The push read **and** the walk that follows a write, both.
    ///
    /// **A test that drives the worker may not run git of its own until
    /// the walk has landed.** A write answers in three pieces — the
    /// Console's line, the re-read, and then the map of the walk — so
    /// `next_push` alone returns while `git status` is still holding
    /// `index.lock`, and the next `git add` in the test fails with
    /// *Another git process seems to be running in this repository*. It
    /// reads as a broken fixture and it is the worker doing exactly what
    /// it is supposed to.
    /// Wait for the walk a freshly started worker makes, and throw away
    /// everything queued behind it.
    ///
    /// **`after_write` can otherwise be satisfied by the wrong walk.**
    /// It returns once it has seen a map *and* the push answer, and the
    /// map it sees may be the start-up walk that was still in flight —
    /// so it returns while the write's own `git status` is running, and
    /// the next `git add` the test makes fails on `index.lock`. That is
    /// a flake that passes on its own and fails under `just check`,
    /// which is the worst kind: caught once in a full run, three
    /// re-runs green.
    fn settle(worker: &StatusWorker) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if worker.take().map.is_some() {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the worker's first walk did not land in 20 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn after_write(worker: &StatusWorker) -> Result<crate::git::Ahead, GitError> {
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut ahead = None;
        let mut walked = false;
        loop {
            let taken = worker.take();
            if taken.push.is_some() {
                ahead = taken.push;
            }
            walked |= taken.map.is_some();
            if walked && let Some(ahead) = ahead {
                return ahead;
            }
            assert!(
                Instant::now() < deadline,
                "the write did not answer in 20 s"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    /// A repository with a bare remote beside it, one commit pushed and
    /// two made since. `-b main` because the default branch name is a
    /// person's git configuration and a test may not depend on it.
    /// In a folder the test holds, at `work` inside it.
    fn scratch_with_remote(name: &str) -> tempfile::TempDir {
        let folder = tempfile::Builder::new()
            .prefix(&format!("guild-git-push-{name}-"))
            .tempdir()
            .expect("a scratch folder");
        let base = folder.path();
        let remote = base.join("remote.git");
        let work = base.join("work");
        std::fs::create_dir_all(&remote).unwrap();
        std::fs::create_dir_all(&work).unwrap();
        git::run(&remote, &["init", "-q", "--bare", "-b", "main"]).expect("git init --bare");
        git::run(&work, &["init", "-q", "-b", "main"]).expect("git init");
        git::run(&work, &["config", "user.name", "Guild"]).unwrap();
        git::run(&work, &["config", "user.email", "guild@example.invalid"]).unwrap();
        git::run(
            &work,
            &["remote", "add", "origin", &remote.to_string_lossy()],
        )
        .unwrap();
        folder
    }

    fn commit(work: &Path, name: &str) {
        std::fs::write(work.join(name), format!("{name}\n")).unwrap();
        git::run(work, &["add", name]).unwrap();
        git::run(work, &["commit", "-q", "-m", name]).unwrap();
    }

    /// **The four reads are one answer, and the range is the upstream's
    /// when there is one.**
    ///
    /// A branch nobody has pushed answers every commit the remote does
    /// not hold, with no upstream and therefore a destination that sets
    /// one. After a push the upstream exists, the range is above it, and
    /// the list is empty — and it is empty because the worker re-read
    /// after the write, not because the window guessed.
    #[test]
    fn a_push_read_answers_the_remote_the_upstream_and_what_is_waiting() {
        let folder = scratch_with_remote("reads");
        let work = folder.path().join("work");
        commit(&work, "one");
        commit(&work, "two");
        let worker = StatusWorker::start(work.clone());
        // The start-up walk, waited for and thrown away: a map still in
        // flight would satisfy `after_write` below in place of the
        // write's own.
        settle(&worker);

        worker.push_read(None, None);
        let ahead = next_push(&worker).expect("a push read");
        assert_eq!(ahead.branch.as_deref(), Some("main"));
        assert_eq!(ahead.remotes, ["origin"]);
        assert_eq!(ahead.upstream, None, "nothing has been pushed yet");
        assert_eq!(
            ahead.commits.len(),
            2,
            "everything the remote does not have"
        );
        let to = ahead.destination(None).expect("somewhere to push");
        assert_eq!(to.remote, "origin");
        assert_eq!(to.onto, "main");
        assert!(to.set_upstream, "the branch has no upstream yet");

        worker.run(crate::git::Write::push(&to));
        let ahead = after_write(&worker).expect("the read after the push");
        assert_eq!(
            ahead.upstream.as_deref(),
            Some("origin/main"),
            "--set-upstream did what it says"
        );
        assert!(
            ahead.commits.is_empty(),
            "nothing waiting: {:?}",
            ahead.commits.iter().map(|c| &c.summary).collect::<Vec<_>>()
        );

        commit(&work, "three");
        worker.push_read(None, None);
        let ahead = next_push(&worker).expect("a push read");
        assert_eq!(ahead.commits.len(), 1, "the range above the upstream");
        assert!(
            !ahead.destination(None).expect("a destination").set_upstream,
            "there is an upstream now"
        );
    }

    #[test]
    fn an_edited_target_reads_its_own_range_without_changing_the_upstream() {
        let folder = scratch_with_remote("edited-target");
        let work = folder.path().join("work");
        commit(&work, "one");
        let worker = StatusWorker::start(work.clone());
        settle(&worker);
        worker.push_read(None, None);
        let first = next_push(&worker).unwrap();
        worker.run(crate::git::Write::push(&first.destination(None).unwrap()));
        after_write(&worker).unwrap();
        commit(&work, "two");
        let to = crate::git::Destination {
            remote: "origin".to_owned(),
            branch: "main".to_owned(),
            onto: "other".to_owned(),
            set_upstream: false,
        };
        worker.run(crate::git::Write::push(&to));
        after_write(&worker).unwrap();
        worker.push_read(Some("origin".to_owned()), Some("main".to_owned()));
        let main = next_push(&worker).unwrap();
        assert_eq!(main.commits.len(), 1);
        worker.push_read(Some("origin".to_owned()), Some("other".to_owned()));
        let other = next_push(&worker).unwrap();
        assert!(
            other.commits.is_empty(),
            "the edited destination already has both commits"
        );
        assert_eq!(
            other.upstream.as_deref(),
            Some("origin/main"),
            "reading never changes git configuration"
        );
        worker.push_read(Some("origin".to_owned()), Some("brand-new".to_owned()));
        assert_eq!(
            next_push(&worker).unwrap().commits.len(),
            2,
            "a new target can publish this branch even when another remote branch holds its commits"
        );
    }

    /// **A push git refuses is re-read all the same**, so the button
    /// comes back and the list still says what is waiting. Refused by
    /// naming a remote that is not there, which is the one refusal a
    /// test can raise without a network.
    #[test]
    fn a_refused_push_still_answers_and_leaves_the_list_standing() {
        let folder = scratch_with_remote("refused");
        let work = folder.path().join("work");
        commit(&work, "one");
        let worker = StatusWorker::start(work.clone());
        settle(&worker);

        worker.run(crate::git::Write::Push {
            remote: "nowhere".to_owned(),
            refspec: "main".to_owned(),
            set_upstream: false,
            tags: crate::git::PushTags::None,
            run_hooks: true,
        });
        let ahead = after_write(&worker).expect("a read after the refusal");
        assert_eq!(
            ahead.commits.len(),
            1,
            "the refusal changed nothing about what is waiting"
        );
    }

    /// A repository with no remote has nowhere to push, which is not the
    /// same as nothing to push: the list is empty because there is no
    /// range to ask for, and the destination is `None` so the head says
    /// so instead of offering a button.
    #[test]
    fn a_repository_with_no_remote_has_nowhere_rather_than_nothing() {
        let folder = scratch("no-remote");
        let dir = folder.path().join("repo");
        let worker = StatusWorker::start(dir.clone());
        worker.push_read(None, None);
        let ahead = next_push(&worker).expect("a push read");
        assert!(ahead.remotes.is_empty());
        assert!(ahead.commits.is_empty());
        assert_eq!(ahead.destination(None), None);
    }

    /// A write is carried out and followed by a walk, so the map that
    /// comes back shows it — staged, then unstaged again.
    #[test]
    fn a_write_is_followed_by_a_walk_that_shows_it() {
        let folder = scratch("stage");
        let dir = folder.path().join("repo");
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        let worker = StatusWorker::start(dir.clone());
        let map = next(&worker).map.expect("the first walk").expect("a map");
        assert_eq!(kind_of(&map, "a.txt"), Some(Kind::Untracked));

        worker.run(Write::Stage(vec![PathBuf::from("a.txt")]));
        let taken = next(&worker);
        assert!(taken.refused.is_empty(), "{:?}", taken.refused);
        let map = taken.map.expect("the walk after the write").expect("a map");
        let entry = map.entry(Path::new("a.txt")).expect("still there");
        assert_eq!(entry.kind, Kind::Added);
        assert!(entry.staged);

        worker.run(Write::Unstage(vec![PathBuf::from("a.txt")]));
        let taken = next(&worker);
        assert!(taken.refused.is_empty(), "{:?}", taken.refused);
        let map = taken.map.expect("the walk after").expect("a map");
        assert_eq!(kind_of(&map, "a.txt"), Some(Kind::Untracked));
    }

    /// A commit takes the index with the message as written — a second
    /// line and a `#` line intact, which is what `-F` and the whitespace
    /// cleanup are for — answers its short hash and summary, and the walk
    /// after it shows nothing staged.
    #[test]
    fn a_commit_lands_with_its_message_intact() {
        let folder = scratch("commit");
        let dir = folder.path().join("repo");
        git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
        git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        let worker = StatusWorker::start(dir.clone());
        let _ = next(&worker);
        worker.run(Write::Stage(vec![PathBuf::from("a.txt")]));
        let _ = next(&worker);

        let message = "Fix the thing\n\n#42 is why, and this line is not a comment\n";
        std::fs::create_dir_all(dir.join(".guild")).unwrap();
        std::fs::write(dir.join(".guild/COMMIT_MESSAGE"), message).unwrap();
        worker.run(Write::Commit {
            message_file: PathBuf::from(".guild/COMMIT_MESSAGE"),
        });
        let taken = next(&worker);
        assert!(taken.refused.is_empty(), "{:?}", taken.refused);
        let (short, summary) = taken.committed.first().cloned().expect("a commit");
        assert_eq!(short.len(), 7, "{short}");
        assert_eq!(summary, "Fix the thing");

        let body = git::run(&dir, &["log", "-1", "--format=%B"]).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&body).trim_end(),
            message.trim_end()
        );
        let map = taken
            .map
            .expect("the walk after the commit")
            .expect("a map");
        assert!(
            map.entry(Path::new("a.txt")).is_none(),
            "nothing left to commit"
        );
    }

    #[test]
    fn prepared_commit_is_serial_and_stops_on_either_preparation_refusal() {
        use std::io::Write as _;
        for failure in [None, Some("stage"), Some("unstage")] {
            let folder = scratch("prepared");
            let dir = folder.path().join("repo");
            git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
            git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
            std::fs::write(dir.join("unrelated.txt"), "unrelated").unwrap();
            git::write(&dir, &Write::Stage(vec!["unrelated.txt".into()])).unwrap();
            std::fs::write(dir.join("included.txt"), "included").unwrap();
            let before = git::run(&dir, &["rev-parse", "HEAD"]).unwrap();
            let worker = StatusWorker::start(dir.clone());
            let _ = next(&worker);
            let mut message = tempfile::NamedTempFile::new().unwrap();
            message
                .write_all(b"Prepared message\n\n# preserved\n")
                .unwrap();
            let message_path = message.path().to_path_buf();
            worker.prepare_commit(
                vec![
                    if failure == Some("stage") {
                        "missing.txt"
                    } else {
                        "included.txt"
                    }
                    .into(),
                ],
                vec![
                    if failure == Some("unstage") {
                        "missing.txt"
                    } else {
                        "unrelated.txt"
                    }
                    .into(),
                ],
                message,
            );
            let mut taken = Taken::default();
            let deadline = Instant::now() + Duration::from_secs(20);
            while taken.map.is_none() {
                let more = worker.take();
                taken.ran.extend(more.ran);
                taken.refused.extend(more.refused);
                taken.committed.extend(more.committed);
                taken.map = more.map;
                assert!(
                    Instant::now() < deadline,
                    "prepared request did not refresh"
                );
                if taken.map.is_none() {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            assert!(taken.map.is_some(), "refresh follows success and refusal");
            assert!(!message_path.exists(), "request releases owned message");
            if let Some(failure) = failure {
                assert_eq!(before, git::run(&dir, &["rev-parse", "HEAD"]).unwrap());
                assert_eq!(taken.refused.len(), 1);
                assert!(taken.refused[0].1.to_string().contains("missing.txt"));
                assert!(taken.committed.is_empty());
                assert!(
                    !taken
                        .ran
                        .iter()
                        .any(|line| line.argv.first().is_some_and(|v| v == "commit")),
                    "no later Commit is executed or logged"
                );
                assert_eq!(taken.ran.len(), if failure == "stage" { 1 } else { 2 });
                assert!(
                    git::run(&dir, &["diff", "--cached", "--name-only"])
                        .unwrap()
                        .ends_with(b"unrelated.txt\n")
                );
            } else {
                assert!(taken.refused.is_empty(), "{:?}", taken.refused);
                assert_eq!(taken.committed.len(), 1);
                assert_eq!(
                    git::run(&dir, &["show", "--pretty=", "--name-only", "HEAD"]).unwrap(),
                    b"included.txt\n"
                );
                assert_eq!(
                    git::run(&dir, &["log", "-1", "--format=%B"]).unwrap(),
                    b"Prepared message\n\n# preserved\n\n"
                );
                assert_eq!(taken.ran.len(), 3);
            }
        }
    }

    #[test]
    fn disconnected_execution_stops_all_queued_writes_and_drops_messages() {
        use std::io::Write as _;
        for prepared in [false, true] {
            let folder = scratch("disconnected-writes");
            let root = folder.path().join("repo");
            git::run(&root, &["config", "user.name", "Guild"]).unwrap();
            git::run(&root, &["config", "user.email", "guild@example.invalid"]).unwrap();
            for name in ["a.txt", "b.txt"] {
                std::fs::write(root.join(name), "data").unwrap();
            }
            let head = git::run(&root, &["rev-parse", "HEAD"]).unwrap();
            let (asks, requests) = mpsc::channel();
            let (answers, receiver) = mpsc::channel();
            let mut files = Vec::new();
            if prepared {
                let mut message = tempfile::NamedTempFile::new().unwrap();
                message.write_all(b"Must not commit\n").unwrap();
                files.push(message.path().to_path_buf());
                asks.send(Ask::PreparedCommit {
                    stage: vec!["a.txt".into()],
                    unstage: vec![],
                    message,
                })
                .unwrap();
            } else {
                asks.send(Ask::Run(Write::Stage(vec!["a.txt".into()])))
                    .unwrap();
            }
            asks.send(Ask::Run(Write::Stage(vec!["b.txt".into()])))
                .unwrap();
            let mut message = tempfile::NamedTempFile::new().unwrap();
            message
                .write_all(b"Later request must not commit\n")
                .unwrap();
            files.push(message.path().to_path_buf());
            asks.send(Ask::PreparedCommit {
                stage: vec![],
                unstage: vec![],
                message,
            })
            .unwrap();
            drop(asks);
            drop(receiver);
            serve(&root, &requests, &answers);
            assert_eq!(
                git::run(&root, &["diff", "--cached", "--name-only"]).unwrap(),
                b"a.txt\n",
                "only the command before the first failed answer can execute"
            );
            assert_eq!(head, git::run(&root, &["rev-parse", "HEAD"]).unwrap());
            assert!(
                files.iter().all(|p| !p.exists()),
                "pending messages drop on termination"
            );
        }
    }

    /// Review r4 `bugs+impl-2`: the status walk that settles a hunk write is
    /// bounded like the write. A FIFO where `.gitattributes` should be, and a
    /// tracked file whose size held but whose stat moved, make `status` open
    /// it and wait; past the deadline the thread answers a failed map — the
    /// barrier cancels — and then answers the next ask.
    #[test]
    fn the_walk_after_a_hunk_write_has_a_deadline() {
        let folder = scratch("hunk-walk-deadline");
        let root = folder.path().join("repo");
        std::fs::write(root.join("f"), "a\nb\n").unwrap();
        git::run(&root, &["add", "f"]).unwrap();
        git::run(
            &root,
            &[
                "-c",
                "user.name=G",
                "-c",
                "user.email=g@e.invalid",
                "commit",
                "-qm",
                "f",
            ],
        )
        .unwrap();
        // Same size, another content and stat: status must read it.
        std::thread::sleep(Duration::from_millis(1100));
        std::fs::write(root.join("f"), "a\nB\n").unwrap();
        let fifo = std::ffi::CString::new(root.join(".gitattributes").to_str().unwrap()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        *git::hunk::DEADLINE_IN_ROOT.lock().unwrap() =
            Some((root.clone(), Duration::from_millis(300)));
        let (asks, requests) = mpsc::channel();
        let (answers, receiver) = mpsc::channel();
        let thread_root = root.clone();
        let thread = std::thread::spawn(move || serve(&thread_root, &requests, &answers));
        asks.send(Ask::Run(Write::StageHunk(git::Hunk {
            path: "f".into(),
            preimage: std::sync::Arc::from(&b"a\nb\n"[..]),
            postimage: std::sync::Arc::from(&b"a\nB\n"[..]),
            disk: None,
            head: std::sync::Arc::from(&b"a\nb\n"[..]),
        })))
        .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        let mut failed = false;
        while !failed {
            let answer = receiver
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .expect("the walk answered");
            failed = matches!(answer, Answer::Map(Err(_)));
        }
        // Review r5 `arch+quality-2`: a walk the watcher queues while the
        // write still awaits a usable map is bounded the same way.
        asks.send(Ask::Walk).unwrap();
        let mut failed_again = false;
        while !failed_again {
            let answer = receiver
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .expect("the recovery walk answered");
            failed_again = matches!(answer, Answer::Map(Err(_)));
        }
        // The thread answers what comes next.
        asks.send(Ask::Contains("HEAD".into())).unwrap();
        let next = loop {
            let answer = receiver
                .recv_timeout(until.saturating_duration_since(Instant::now()))
                .expect("the next ask answered");
            if matches!(answer, Answer::Contains { .. }) {
                break answer;
            }
        };
        assert!(matches!(next, Answer::Contains { .. }));
        *git::hunk::DEADLINE_IN_ROOT.lock().unwrap() = None;
        std::fs::remove_file(root.join(".gitattributes")).unwrap();
        drop(asks);
        thread.join().unwrap();
    }

    /// Review r4 `bugs+impl-4`: the change staged outside the folder is a
    /// gitlink, and `diff.ignoreSubmodules=all` would hide it from the scope
    /// check. It refuses the commit all the same.
    #[test]
    fn a_folder_commit_refuses_a_gitlink_staged_outside_it() {
        use std::io::Write as _;
        let folder = scratch("folder-scope-gitlink");
        let root = folder.path().join("repo");
        git::run(&root, &["config", "user.name", "Guild"]).unwrap();
        git::run(&root, &["config", "user.email", "guild@example.invalid"]).unwrap();
        git::run(&root, &["config", "diff.ignoreSubmodules", "all"]).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/in.txt"), "in\n").unwrap();
        git::run(&root, &["add", "."]).unwrap();
        git::run(&root, &["commit", "-qm", "base"]).unwrap();
        std::fs::write(root.join("sub/in.txt"), "IN\n").unwrap();
        let head = String::from_utf8(git::run(&root, &["rev-parse", "HEAD"]).unwrap()).unwrap();
        git::run(
            &root,
            &[
                "update-index",
                "--add",
                "--cacheinfo",
                &format!("160000,{},vendor", head.trim()),
            ],
        )
        .unwrap();
        let (asks, requests) = mpsc::channel();
        let (answers, receiver) = mpsc::channel();
        let mut message = tempfile::NamedTempFile::new().unwrap();
        message.write_all(b"Only the folder\n").unwrap();
        asks.send(Ask::PreparedCommit {
            stage: vec!["in.txt".into()],
            unstage: vec![],
            message,
        })
        .unwrap();
        drop(asks);
        serve(&root.join("sub"), &requests, &answers);
        let received: Vec<_> = receiver.try_iter().collect();
        assert!(
            received.iter().any(|a| matches!(
                a,
                Answer::CommitOutside(paths) if paths == &[PathBuf::from("vendor")]
            )),
            "refused naming the gitlink"
        );
        assert_eq!(
            head.as_bytes(),
            git::run(&root, &["rev-parse", "HEAD"]).unwrap()
        );
    }

    /// Review r3 `arch+quality-2`: a folder project's prepared commit, when
    /// a change outside the folder was staged after the window planned it —
    /// by another root's hunk write in the same checkout. Its scope is asked
    /// again under the checkout's turn, held through the preparation and the
    /// commit; the commit is refused naming the path, and nothing is
    /// committed.
    #[test]
    fn a_folder_commit_refuses_what_was_staged_outside_it_meanwhile() {
        use std::io::Write as _;
        let folder = scratch("folder-scope");
        let root = folder.path().join("repo");
        git::run(&root, &["config", "user.name", "Guild"]).unwrap();
        git::run(&root, &["config", "user.email", "guild@example.invalid"]).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        std::fs::write(root.join("sub/in.txt"), "in\n").unwrap();
        std::fs::write(root.join("out.txt"), "out\n").unwrap();
        git::run(&root, &["add", "."]).unwrap();
        git::run(&root, &["commit", "-qm", "base"]).unwrap();
        std::fs::write(root.join("sub/in.txt"), "IN\n").unwrap();
        std::fs::write(root.join("out.txt"), "OUT\n").unwrap();
        git::run(&root, &["add", "out.txt"]).unwrap();
        let head = git::run(&root, &["rev-parse", "HEAD"]).unwrap();
        let (asks, requests) = mpsc::channel();
        let (answers, receiver) = mpsc::channel();
        let mut message = tempfile::NamedTempFile::new().unwrap();
        message.write_all(b"Only the folder\n").unwrap();
        asks.send(Ask::PreparedCommit {
            stage: vec!["in.txt".into()],
            unstage: vec![],
            message,
        })
        .unwrap();
        drop(asks);
        serve(&root.join("sub"), &requests, &answers);
        let received: Vec<_> = receiver.try_iter().collect();
        assert!(
            received.iter().any(|a| matches!(
                a,
                Answer::CommitOutside(paths) if paths == &[PathBuf::from("out.txt")]
            )),
            "refused naming the path"
        );
        assert_eq!(head, git::run(&root, &["rev-parse", "HEAD"]).unwrap());
    }

    #[test]
    fn ordinary_refusal_keeps_unrelated_queued_requests_alive() {
        use std::io::Write as _;
        for prepared in [false, true] {
            let folder = scratch("refusal-continues");
            let root = folder.path().join("repo");
            let (asks, requests) = mpsc::channel();
            let (answers, receiver) = mpsc::channel();
            if prepared {
                let mut message = tempfile::NamedTempFile::new().unwrap();
                message.write_all(b"Must not commit\n").unwrap();
                asks.send(Ask::PreparedCommit {
                    stage: vec!["missing.txt".into()],
                    unstage: vec![],
                    message,
                })
                .unwrap();
            } else {
                asks.send(Ask::Run(Write::Stage(vec!["missing.txt".into()])))
                    .unwrap();
            }
            asks.send(Ask::Run(Write::Create("after-refusal".into())))
                .unwrap();
            drop(asks);
            serve(&root, &requests, &answers);
            let received: Vec<_> = receiver.try_iter().collect();
            assert!(
                received
                    .iter()
                    .any(|a| matches!(a, Answer::Refused(Write::Stage(_), _)))
            );
            assert!(
                !received
                    .iter()
                    .any(|a| matches!(a, Answer::Committed { .. }))
            );
            assert_eq!(
                git::run(&root, &["branch", "--show-current"]).unwrap(),
                b"after-refusal\n"
            );
        }
    }

    /// A hook that fails is a refusal in git's words — the hook's own
    /// output among them — and no commit.
    #[test]
    fn a_failing_hook_is_a_refusal_in_gits_words() {
        use std::os::unix::fs::PermissionsExt;
        let folder = scratch("hook");
        let dir = folder.path().join("repo");
        git::run(&dir, &["config", "user.name", "Guild"]).unwrap();
        git::run(&dir, &["config", "user.email", "guild@example.invalid"]).unwrap();
        std::fs::create_dir_all(dir.join(".git/hooks")).unwrap();
        let hook = dir.join(".git/hooks/pre-commit");
        std::fs::write(&hook, "#!/bin/sh\necho 'nope, not today' >&2\nexit 1\n").unwrap();
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(dir.join("a.txt"), "one\n").unwrap();
        std::fs::write(dir.join("msg"), "Try\n").unwrap();
        let worker = StatusWorker::start(dir.clone());
        let _ = next(&worker);
        worker.run(Write::Stage(vec![PathBuf::from("a.txt")]));
        let _ = next(&worker);
        worker.run(Write::Commit {
            message_file: PathBuf::from("msg"),
        });
        let taken = next(&worker);
        let (write, err) = taken.refused.first().expect("a refusal");
        assert!(matches!(write, Write::Commit { .. }));
        assert!(err.to_string().contains("nope, not today"), "{err}");
        assert!(taken.committed.is_empty());
    }

    /// A refused write is news, not a map: it comes back with git's own
    /// words and the write that was refused, and the walk after it still
    /// answers.
    #[test]
    fn a_refused_write_comes_back_in_gits_words() {
        let folder = scratch("refuse");
        let dir = folder.path().join("repo");
        let worker = StatusWorker::start(dir.clone());
        let _ = next(&worker);
        worker.run(Write::Stage(vec![PathBuf::from("nothing-here.txt")]));
        let taken = next(&worker);
        let (write, err) = taken.refused.first().expect("a refusal");
        assert!(matches!(write, Write::Stage(_)));
        assert!(err.to_string().contains("nothing-here.txt"), "{err}");
    }

    /// A worker on channels of the test's own, as `start` would make them
    /// with no thread behind — so what is on the other end is the test's
    /// to keep or to drop.
    fn worker_on(answers: mpsc::Receiver<Answer>) -> (StatusWorker, mpsc::Receiver<Ask>) {
        let (asks, asked) = mpsc::channel();
        let worker = StatusWorker {
            serial: 0,
            asks,
            answers,
            root: "/nowhere".to_owned(),
            restarts: 0,
            dead: false,
        };
        (worker, asked)
    }

    /// **A thread that has gone says so.** Its end of the answers channel
    /// is dropped with it — by a panic, or by a spawn that never ran — and
    /// `take` tells that from a quiet thread by `Disconnected`, after
    /// whatever it sent before it went. Decision 19 of M5.11.
    #[test]
    fn a_worker_whose_thread_has_gone_says_so() {
        let (answering, answers) = mpsc::channel();
        let (worker, _asked) = worker_on(answers);
        answering
            .send(Answer::Stashes(Ok(Vec::new())))
            .expect("the worker is listening");
        drop(answering);
        let taken = worker.take();
        assert!(
            taken.gone,
            "a dropped answering end is a thread that has gone"
        );
        assert!(
            taken.stashes.is_some(),
            "what it said before it went still lands"
        );
    }

    /// A thread with nothing to say is not a dead one: the window polls
    /// ten times a second and a walk takes longer than that.
    #[test]
    fn a_quiet_worker_is_not_a_dead_one() {
        let (answering, answers) = mpsc::channel::<Answer>();
        let (worker, _asked) = worker_on(answers);
        assert!(!worker.take().gone);
        drop(answering);
        assert!(worker.take().gone);
    }

    /// **Once, and then left.** A first death is started again, because a
    /// panic on one odd repository state need not repeat; a second is left
    /// dead until the project is opened again, because a panic that
    /// repeats would otherwise be a restart on every tick.
    #[test]
    fn a_thread_is_started_again_once() {
        assert_eq!(after_death(0), AfterDeath::Restart);
        assert_eq!(after_death(1), AfterDeath::Leave);
        assert_eq!(after_death(u8::MAX), AfterDeath::Leave);
    }
}

#[cfg(test)]
mod shared_tests;
