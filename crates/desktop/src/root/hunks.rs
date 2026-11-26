//! The window's half of hunk staging — GLD-026, decision 55: a hunk write
//! sent and counted, its outcome said, and a commit held until the status
//! that write produces has landed.
//!
//! **Why a commit waits.** The git thread drains a batch of asks before it
//! walks the status, so a commit planned from the map the panel holds would
//! `git add` a whole file whose first hunk was staged a moment ago (review
//! finding `lenses-5`). A commit pressed while a hunk write has no answer,
//! or has one but no map after it, is remembered and run once that map has
//! landed and the ticks follow it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gpui::Context;

use crate::Root;
use crate::git::Write;
use crate::root::git::HunkOutcome;
use guild_session::ProjectId;

/// The files a hunk write is unanswered for, by absolute path, across every
/// project — what a hunk control asks before it builds a write. A second
/// activation for the file while one is in flight is ignored before any
/// snapshot of it is taken: repeated clicks on a busy git thread would
/// otherwise queue a copy of the file each (review r3 `arch+quality-5`).
#[derive(Debug, Default)]
pub(crate) struct HunksInFlight(HashMap<PathBuf, usize>);

impl gpui::Global for HunksInFlight {}

/// Whether a hunk write for `file`, an absolute path, is unanswered.
pub(crate) fn hunk_in_flight(cx: &gpui::App, file: &Path) -> bool {
    cx.try_global::<HunksInFlight>()
        .is_some_and(|in_flight| in_flight.0.contains_key(file))
}

/// A write for `file` counted as in flight, for the tests of a control.
#[cfg(test)]
pub(crate) fn mark_in_flight_for_test(cx: &mut gpui::App, file: &Path) {
    *cx.default_global::<HunksInFlight>()
        .0
        .entry(file.to_path_buf())
        .or_default() += 1;
}

fn release(cx: &mut gpui::App, file: &Path, count: usize) {
    let in_flight = cx.default_global::<HunksInFlight>();
    if let Some(held) = in_flight.0.get_mut(file) {
        *held = held.saturating_sub(count);
        if *held == 0 {
            in_flight.0.remove(file);
        }
    }
}

/// A project's hunk writes in flight, on one git thread.
///
/// **One barrier** (review r2): a commit pressed while any hunk write of the
/// project is unanswered, or answered with no map after it, waits; when the
/// map after the last answer lands, every path a write changed takes its tick
/// from the index — partial, full or cleared, as the person chose — and only
/// then is the commit planned, or cancelled when any of those writes
/// refused. A flight belongs to the thread it was sent to: a replaced thread
/// answers nothing, so its flight and the commit waiting on it go.
#[derive(Debug)]
pub(crate) struct Flight {
    /// The git thread's serial the writes went to.
    worker: u64,
    /// Sent and not answered yet.
    outstanding: usize,
    /// Answered, and no map after the answer yet.
    awaiting_map: bool,
    /// A commit was pressed meanwhile.
    commit: bool,
    /// A write since the last settled map refused.
    refused: bool,
    /// Paths a write changed since the last settled map.
    written: std::collections::HashSet<std::path::PathBuf>,
    /// The absolute files of the writes not answered yet, as counted in
    /// [`HunksInFlight`].
    files: HashMap<PathBuf, usize>,
}

impl Flight {
    fn new(worker: u64) -> Self {
        Self {
            worker,
            outstanding: 0,
            awaiting_map: false,
            commit: false,
            refused: false,
            written: Default::default(),
            files: HashMap::new(),
        }
    }

    fn busy(&self) -> bool {
        self.outstanding > 0 || self.awaiting_map
    }
}

impl Root {
    /// Send a hunk write and count it until its status lands — counted only
    /// when the thread took it, and refused while a write for the same file
    /// is unanswered.
    pub(crate) fn send_hunk(&mut self, project: ProjectId, write: Write, cx: &mut Context<Self>) {
        // A flight on a thread that went answers nothing: it is let go
        // before its files are asked about.
        self.drop_orphaned_hunks(cx);
        let (Some(worker), Some(hunk), Some(root)) = (
            self.status_workers.get(&project),
            write.hunk(),
            self.desk.project(project).map(|p| p.path.clone()),
        ) else {
            return;
        };
        let file = root.join(&hunk.path);
        if hunk_in_flight(cx, &file) {
            return;
        }
        let serial = worker.serial();
        if !worker.try_run(write) {
            return;
        }
        let flight = self
            .hunk_flights
            .entry(project)
            .or_insert_with(|| Flight::new(serial));
        flight.outstanding += 1;
        *flight.files.entry(file.clone()).or_default() += 1;
        *cx.default_global::<HunksInFlight>()
            .0
            .entry(file)
            .or_default() += 1;
    }

    /// How many hunk writes of a project are unanswered.
    #[cfg(test)]
    pub(crate) fn hunks_outstanding_for_test(&self, project: ProjectId) -> usize {
        self.hunk_flights
            .get(&project)
            .map_or(0, |flight| flight.outstanding)
    }

    /// A write counted as in flight on the project's current thread, for the
    /// tests that drive the barrier without a git write.
    #[cfg(test)]
    pub(crate) fn hunk_in_flight_for_test(&mut self, project: ProjectId) {
        let serial = self.status_workers[&project].serial();
        self.hunk_flights
            .entry(project)
            .or_insert_with(|| Flight::new(serial))
            .outstanding += 1;
    }

    /// Whether a hunk write of the project is unanswered, or answered with
    /// no map after it — a commit pressed now is held.
    pub(crate) fn hunks_pending(&self, project: ProjectId) -> bool {
        self.hunk_flights.get(&project).is_some_and(Flight::busy)
    }

    /// The commit's barrier: `true` when the commit is held for a hunk
    /// write and will run, or be cancelled, when its status lands.
    pub(crate) fn hold_commit_for_hunks(&mut self, project: ProjectId) -> bool {
        match self.hunk_flights.get_mut(&project) {
            Some(flight) if flight.busy() => {
                flight.commit = true;
                true
            }
            _ => false,
        }
    }

    /// Flights whose git thread is not the project's thread any more: none
    /// of their answers will come. Each goes, and a commit waiting on one is
    /// cancelled and said. Every poll.
    pub(crate) fn drop_orphaned_hunks(&mut self, cx: &mut Context<Self>) {
        let orphaned: Vec<ProjectId> = self
            .hunk_flights
            .iter()
            .filter(|(project, flight)| {
                self.status_workers.get(project).map(|w| w.serial()) != Some(flight.worker)
            })
            .map(|(project, _)| *project)
            .collect();
        for project in orphaned {
            self.forget_hunks(project, cx);
        }
    }

    fn cancel_held_commit(&mut self) {
        let text = self
            .ui()
            .text(crate::i18n::Text::NoticeHunkCommitCancelled)
            .to_owned();
        self.warn(text);
    }

    /// What one poll brought for a project's hunk writes, in order: each
    /// write's outcome and each status map, usable or not. Called after the
    /// newest map has been applied to the panel.
    pub(crate) fn hunks_landed(
        &mut self,
        project: ProjectId,
        events: Vec<crate::root::git::HunkEvent>,
        cx: &mut Context<Self>,
    ) {
        use crate::root::git::HunkEvent;
        let mut unread = false;
        let root = self.desk.project(project).map(|p| p.path.clone());
        let mut outcomes = Vec::new();
        let usable = matches!(self.git_status.get(&project), Some(Ok(_)));
        let mut flight = self.hunk_flights.get_mut(&project);
        for event in events {
            match event {
                HunkEvent::Outcome(write, outcome) => {
                    if let Some(flight) = flight.as_deref_mut() {
                        flight.outstanding = flight.outstanding.saturating_sub(1);
                        flight.awaiting_map = true;
                        if let (Some(root), Some(hunk)) = (&root, write.hunk()) {
                            let file = root.join(&hunk.path);
                            if let Some(held) = flight.files.get_mut(&file) {
                                *held -= 1;
                                if *held == 0 {
                                    flight.files.remove(&file);
                                }
                                release(cx, &file, 1);
                            }
                        }
                        // Anything but a clean write cancels the commit
                        // waiting: after `HeadMoved` the index may hold the
                        // inverse of the commit that landed (review r3), and
                        // after `Unverified` it may hold the write (r4).
                        if !matches!(outcome, HunkOutcome::Written) {
                            flight.refused = true;
                        }
                        if matches!(
                            outcome,
                            HunkOutcome::Written
                                | HunkOutcome::HeadMoved
                                | HunkOutcome::Unverified
                                | HunkOutcome::Unconfirmed
                        ) && let Some(hunk) = write.hunk()
                        {
                            flight.written.insert(hunk.path.clone());
                        }
                    }
                    outcomes.push((write, outcome));
                }
                HunkEvent::Map(true) => {
                    if let Some(flight) = flight.as_deref_mut() {
                        flight.awaiting_map = false;
                    }
                }
                // The status after the writes could not be read: the ticks
                // wait for a usable one, and a commit waiting cannot be
                // planned from it — reviews r3 and r4 `bugs+impl-2`, even
                // when a usable map follows in the same take (r4
                // `bugs+impl-1`).
                HunkEvent::Map(false) => {
                    if let Some(flight) = flight.as_deref_mut()
                        && flight.awaiting_map
                    {
                        unread |= std::mem::take(&mut flight.commit);
                    }
                }
            }
        }
        // The newest map is the one ticks are read from: one that failed
        // settles nothing.
        if !usable
            && let Some(flight) = flight
            && !flight.busy()
            && (flight.commit || !flight.written.is_empty())
        {
            flight.awaiting_map = true;
            unread |= std::mem::take(&mut flight.commit);
        }
        if unread {
            self.cancel_held_commit();
        }
        let mut refreshed = Vec::new();
        for (write, outcome) in outcomes {
            tracing::info!(?outcome, path = ?write.hunk().map(|h| &h.path), "hunk write");
            if !matches!(outcome, HunkOutcome::Written)
                && let Some(hunk) = write.hunk()
            {
                refreshed.push(hunk.path.clone());
            }
            let text = match outcome {
                HunkOutcome::Written => continue,
                HunkOutcome::Stale => self
                    .ui()
                    .text(crate::i18n::Text::NoticeHunkStale)
                    .to_owned(),
                HunkOutcome::WholeFile => {
                    self.ui().text(crate::i18n::Text::HunkWholeFile).to_owned()
                }
                HunkOutcome::Converted => self
                    .ui()
                    .text(crate::i18n::Text::NoticeHunkConverted)
                    .to_owned(),
                HunkOutcome::HeadMoved => self
                    .ui()
                    .text(crate::i18n::Text::NoticeHunkHeadMoved)
                    .to_owned(),
                HunkOutcome::Unverified => self
                    .ui()
                    .text(crate::i18n::Text::NoticeHunkUnverified)
                    .to_owned(),
                HunkOutcome::Unconfirmed => self
                    .ui()
                    .text(crate::i18n::Text::NoticeHunkUnconfirmed)
                    .to_owned(),
                HunkOutcome::Refused(err) => {
                    let words = match err {
                        crate::git::GitError::Failed(words) => words,
                        other => other.to_string(),
                    };
                    self.ui().notice_git_refused(&words)
                }
            };
            self.warn(text);
        }
        // A refusal is about inputs that moved: what offered the write is
        // asked again, so the same control does not stay enabled.
        for path in refreshed {
            self.refresh_hunk_sources(project, &path, cx);
        }
        // Settled: the map after every answer is in.
        let settled = self
            .hunk_flights
            .get_mut(&project)
            .filter(|flight| !flight.busy())
            .map(|flight| {
                let written = std::mem::take(&mut flight.written);
                let commit = std::mem::take(&mut flight.commit);
                let refused = std::mem::take(&mut flight.refused);
                (written, commit, refused)
            });
        if let Some((written, commit, refused)) = settled {
            self.ticks_follow_index(project, &written);
            if commit {
                if refused {
                    self.cancel_held_commit();
                } else {
                    self.commit_in(project, cx);
                }
            }
        }
        cx.notify();
    }

    /// What offered a refused write about `path` asks again: the project's
    /// diffs showing it, and its file tabs' markers. Review r2.
    fn refresh_hunk_sources(
        &mut self,
        project: ProjectId,
        path: &std::path::Path,
        cx: &mut Context<Self>,
    ) {
        for body in self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
        {
            match body {
                crate::Body::Diff { tab: view, .. } => {
                    view.update(cx, |diff, cx| diff.refetch(path, cx));
                }
                crate::Body::File { tab, .. } if tab.read(cx).relative() == path => {
                    tab.update(cx, |file, cx| file.refresh_git_baseline(cx));
                }
                _ => {}
            }
        }
    }

    /// A git thread that went: nothing it held will be answered.
    pub(crate) fn forget_hunks(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(flight) = self.hunk_flights.remove(&project) else {
            return;
        };
        for (file, count) in &flight.files {
            release(cx, file, *count);
        }
        if flight.commit {
            self.cancel_held_commit();
        }
    }
}
