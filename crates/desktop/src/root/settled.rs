//! The Sessions mode's Settled section in the window: the harnesses'
//! history of the shown workspace's folders, read on a thread the way the
//! start screen reads one folder's, and the transcript times it gives the
//! open sessions that have no saved time of their own.
//!
//! **Read only while the column shows Sessions**, at the start screen's
//! cadence (`start::read_due`), one thread at a time, its caches kept
//! between reads: a read opens transcripts and may run `opencode`, which
//! the window's thread must never wait on.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Instant, SystemTime};

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use gpui::Context;
use guild_session::history::{History, Past, Stores};
use guild_session::{ProjectId, SessionId};

use crate::Root;

/// A conversation a harness remembers in one of the workspace's folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SettledPast {
    /// The project whose folder it ran in.
    pub(crate) project: ProjectId,
    /// What the harness says of it.
    pub(crate) past: Past,
}

/// The history being read for the Settled section, and the last answer.
#[derive(Default)]
pub(crate) struct SettledReading {
    /// The last answer: every conversation the folders asked about were
    /// touched in within the Settled period (`History::since`).
    pub(crate) pasts: Vec<SettledPast>,
    /// The folders the last read was asked for.
    folders: Vec<(ProjectId, PathBuf)>,
    /// When the last read ended — the next is due a while after it, not
    /// after its start, so a slow read is not followed at once by another.
    asked_at: Option<Instant>,
    /// Raised to stop a read whose folders are no longer shown: it gives
    /// up between folders, and the new workspace's read starts at once.
    cancel: Option<Arc<AtomicBool>>,
    /// What the shown page last drew — its lines and their ages — so the
    /// watch loop repaints the cached column when time moves a row
    /// between sections or turns *1m ago* into *2m ago*, and only then.
    pub(crate) drawn: Option<(crate::session_list::Page, Vec<String>)>,
    /// When the clock-only repaint last looked: it looks every few seconds,
    /// not on every tick of the watch loop.
    looked_at: Option<Instant>,
    /// Transcript times of open Claude records with no reported activity
    /// of their own, by conversation id — read whatever their age, apart
    /// from the Settled period, which only bounds the closed ones.
    pub(crate) transcripts: std::collections::HashMap<String, SystemTime>,
    /// The readers' caches between reads — on the thread while one runs.
    history: Option<History>,
    answer: Option<Receiver<Read>>,
}

/// A thread's answer, with the folders it was asked for.
struct Read {
    folders: Vec<(ProjectId, PathBuf)>,
    history: History,
    pasts: Vec<SettledPast>,
    transcripts: std::collections::HashMap<String, SystemTime>,
}

impl Root {
    /// The transcript time the last history read found for an open
    /// session: Claude's by its own id, the others' by the harness's id.
    pub(crate) fn settled_touched(&self, session: SessionId) -> Option<SystemTime> {
        let record = self.desk.session(session)?;
        let harness = record.runs.harness()?;
        let id = Self::past_id(record)?;
        self.settled.transcripts.get(&id).copied().or_else(|| {
            self.settled
                .pasts
                .iter()
                .find(|p| p.past.harness == harness && p.past.id == id)
                .map(|p| p.past.touched)
        })
    }

    /// The id a harness's history knows a record's conversation by.
    pub(crate) fn past_id(record: &guild_session::SessionRecord) -> Option<String> {
        match record.runs {
            guild_session::Runs::Claude(_) => Some(record.id.to_string()),
            _ => record.harness_session.clone(),
        }
    }

    /// The folders the Settled section reads: the shown workspace's
    /// projects.
    fn settled_folders(&self) -> Vec<(ProjectId, PathBuf)> {
        self.desk
            .workspace(self.active_workspace)
            .map(|workspace| {
                workspace
                    .projects
                    .iter()
                    .filter_map(|id| Some((*id, self.desk.project(*id)?.path.clone())))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The history's answer, if one has come, and a new read when one is
    /// due — from the watch loop, beside `poll_start`. A repaint only when
    /// the answer changed what the column would draw.
    pub(crate) fn poll_settled(&mut self, cx: &mut Context<Root>) {
        if let Some(answer) = &self.settled.answer {
            match answer.try_recv() {
                Ok(read) => {
                    self.settled.answer = None;
                    self.settled.cancel = None;
                    self.settled.asked_at = Some(Instant::now());
                    self.settled.history = Some(read.history);
                    if read.folders == self.settled.folders
                        && (read.pasts != self.settled.pasts
                            || read.transcripts != self.settled.transcripts)
                    {
                        self.settled.pasts = read.pasts;
                        self.settled.transcripts = read.transcripts;
                        cx.notify();
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    // A superseded read gives up without a word; any other
                    // end without an answer starts the next read cold.
                    if !self
                        .settled
                        .cancel
                        .as_ref()
                        .is_some_and(|cancel| cancel.load(Ordering::Relaxed))
                    {
                        tracing::warn!(
                            "settled sessions: the history read ended without an answer"
                        );
                    }
                    self.settled.answer = None;
                    self.settled.cancel = None;
                }
            }
        }
        let shown = self.settings.column_mode == guild_session::settings::ColumnMode::Sessions
            && (!self.settings.column_folded || self.column_peeking);
        if !shown {
            return;
        }
        let folders = self.settled_folders();
        if folders != self.settled.folders {
            // Another workspace: its answer is read at once, a read still
            // going for the last one stops, and the last one's pasts are
            // not shown under this one's name meanwhile.
            self.settled.folders = folders.clone();
            self.settled.asked_at = None;
            if let Some(cancel) = self.settled.cancel.take() {
                cancel.store(true, Ordering::Relaxed);
                self.settled.answer = None;
            }
            if !self.settled.pasts.is_empty() {
                self.settled.pasts.clear();
                cx.notify();
            }
        }
        self.repaint_when_time_moves(cx);
        let since = self.settled.asked_at.map(|at| at.elapsed());
        if !crate::start::read_due(since, self.settled.answer.is_some(), self.window_focused) {
            return;
        }
        self.settled.asked_at = Some(Instant::now());
        let mut history = self.settled.history.take().unwrap_or_default();
        let since = SystemTime::now()
            - std::time::Duration::from_secs(
                u64::from(self.settings.settle_after_days) * 24 * 60 * 60,
            );
        // Open Claude records that never reported: their transcript's time
        // is their age, read by id whatever the period.
        let legacy: Vec<(String, PathBuf)> = self
            .desk
            .sessions()
            .filter(|record| {
                record.open
                    && record.runs.is_claude()
                    && record.last_activity.is_none()
                    && self.board.transitioned_at(record.id).is_none()
            })
            .map(|record| (record.id.to_string(), record.cwd.clone()))
            .collect();
        let cancel = Arc::new(AtomicBool::new(false));
        self.settled.cancel = Some(cancel.clone());
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            let path = guild_session::login_path::path_or_inherited();
            let stores = Stores::home(path.into());
            let mut pasts = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for (project, folder) in &folders {
                if cancel.load(Ordering::Relaxed) {
                    return;
                }
                if !seen.insert(folder.clone()) {
                    continue;
                }
                pasts.extend(
                    history
                        .since(&stores, folder, since)
                        .into_iter()
                        .map(|past| SettledPast {
                            project: *project,
                            past,
                        }),
                );
            }
            let transcripts = legacy
                .into_iter()
                .filter_map(|(id, cwd)| {
                    let file = stores
                        .claude
                        .join(guild_session::history::claude::folder_name(&cwd))
                        .join(format!("{id}.jsonl"));
                    let touched = std::fs::metadata(file).and_then(|m| m.modified()).ok()?;
                    Some((id, touched))
                })
                .collect();
            let _ = send.send(Read {
                folders,
                history,
                pasts,
                transcripts,
            });
        });
        self.settled.answer = Some(receive);
    }

    /// Repaint the shown Sessions page when what it would draw has moved
    /// with the clock alone — a row crossing into Settled, an age word —
    /// as the start screen does for its ages. A minute's change at most.
    fn repaint_when_time_moves(&mut self, cx: &mut Context<Root>) {
        if self
            .settled
            .looked_at
            .is_some_and(|at| at.elapsed() < std::time::Duration::from_secs(5))
        {
            return;
        }
        self.settled.looked_at = Some(Instant::now());
        let workspace = self.active_workspace;
        let page = self.session_page(workspace);
        let now = std::time::SystemTime::now();
        let ui = self.ui();
        let ages = page
            .rows()
            .iter()
            .map(|key| crate::agents::relative_time(self.session_row_last(key), now, ui))
            .collect();
        let picture = Some((page, ages));
        if self.settled.drawn != picture {
            let first = self.settled.drawn.is_none();
            self.settled.drawn = picture;
            if !first {
                cx.notify();
            }
        }
    }
}
