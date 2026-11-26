//! Application-owned terminal jobs and their durable, sanitized receipts.
use crate::terminal::view::{self, Look, TerminalTab};
use gpui::{App, Entity};
use guild_session::{
    ProjectId, SessionId,
    store::{Normalise, Store},
    terminal_job::{
        Book, JobError, JobId, JobReceipt, JobState, LaunchContext, LaunchSpec, Presentation,
        Reservation,
    },
};
use std::{collections::HashMap, path::PathBuf};

#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipts {
    jobs: Vec<JobReceipt>,
}
impl Normalise for Receipts {
    fn normalise(&mut self) {}
}

/// Resolve an active caller and constrain an optional target to its actual pane.
pub fn authorize(
    desk: &guild_session::Desk,
    caller: Option<&str>,
    target: Option<&guild_session::overlay::Target>,
    pane: Option<guild_session::PaneId>,
) -> Result<(SessionId, ProjectId), JobError> {
    let owner: SessionId = caller
        .ok_or("Run terminal commands inside a Guild session")?
        .parse()
        .map_err(|_| "Invalid calling session")?;
    let record = desk
        .session(owner)
        .ok_or("Calling session is not in this window")?;
    if !record.open {
        return Err("Calling session is closed".into());
    }
    if let Some(target) = target {
        let pane = pane.ok_or("Calling session no longer has a terminal pane")?;
        if !matches!(target, guild_session::overlay::Target::Session(id) if *id == owner)
            && !matches!(target, guild_session::overlay::Target::Pane(id) if *id == pane)
        {
            return Err("Terminal target must belong to the calling session".into());
        }
    }
    Ok((owner, record.project))
}

/// Capture only window-owned records and a branch observation supplied by its git cache.
/// This function performs no filesystem or git reads.
pub fn launch_context(
    desk: &guild_session::Desk,
    conversation: SessionId,
    project: ProjectId,
    cwd: &std::path::Path,
    observed_branch: Option<&str>,
) -> LaunchContext {
    let root = desk.project(project);
    let session = desk.session(conversation);
    let repository = root.and_then(|p| p.repository.clone());
    let repository_label = repository.as_ref().map(|key| {
        desk.repository_name(key)
            .map(str::to_owned)
            .unwrap_or_else(|| crate::column::repository_name(key))
    });
    LaunchContext {
        conversation: Some(conversation),
        session_label: session.and_then(|s| {
            s.name
                .clone()
                .or_else(|| s.topic.clone())
                .or_else(|| (!s.title.is_empty()).then(|| s.title.clone()))
        }),
        root_path: root.map(|p| p.path.clone()),
        root_label: root.map(|p| p.name.clone()),
        repository,
        repository_label,
        branch: observed_branch.map(str::to_owned),
        program_cwd: cwd.to_path_buf(),
    }
    .sanitized()
}

/// Native output availability, separate from process exit and presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputState {
    /// Interactive child; an empty screen is normal.
    Live,
    /// Completed screen or scrollback has text.
    Retained,
    /// A retained completed terminal has no displayable text.
    Empty,
    /// The user explicitly deleted this output.
    Deleted,
    /// No buffer survived, or the program never started.
    Unavailable,
}

struct ReceiptOverlay {
    revision: u64,
    error: String,
    rows: std::sync::Arc<Vec<JobReceipt>>,
}

/// Retains processes and grids independently of any overlay record.
pub struct LiveJobs {
    book: Book,
    terminals: HashMap<JobId, Entity<TerminalTab>>,
    store: Store<Receipts>,
    load_error: Option<String>,
    storage_error: Option<String>,
    overlay_builds: std::cell::Cell<usize>,
    overlay: std::cell::RefCell<Option<ReceiptOverlay>>,
}
impl LiveJobs {
    /// Read receipts without relaunching. Unreadable state blocks new launches:
    /// falling back to an older reservation file could duplicate a paid run.
    pub fn load(path: PathBuf) -> Self {
        let loaded = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Receipts>(&bytes)
                .map_err(|_| "Terminal receipts are unreadable; launches are disabled".to_owned())
                .and_then(|r| Book::restore(r.jobs)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Book::default()),
            Err(_) => Err("Terminal receipts could not be read; launches are disabled".into()),
        };
        let (book, load_error) = match loaded {
            Ok(book) => (book, None),
            Err(error) => (Book::default(), Some(error)),
        };
        Self {
            book,
            terminals: HashMap::new(),
            store: Store::new(path),
            load_error,
            storage_error: None,
            overlay: Default::default(),
            overlay_builds: Default::default(),
        }
    }
    /// A stable status snapshot; persistence failures are never reported as success.
    pub fn receipt(&self, id: &JobId) -> Option<JobReceipt> {
        let mut receipt = self.book.get(id)?.clone();
        if let Some(error) = &self.storage_error {
            receipt.error = Some(error.clone());
        }
        Some(receipt)
    }
    /// All retained metadata, for native recovery controls.
    pub fn receipts(&self) -> Vec<JobReceipt> {
        self.receipt_snapshot().as_ref().clone()
    }
    /// Shared metadata for idle polling/render; overlay only changes on real errors.
    pub fn receipt_snapshot(&self) -> std::sync::Arc<Vec<JobReceipt>> {
        let Some(error) = &self.storage_error else {
            return self.book.receipt_snapshot();
        };
        let mut cache = self.overlay.borrow_mut();
        if let Some(previous) = cache.as_ref()
            && previous.revision == self.book.revision()
            && &previous.error == error
        {
            return previous.rows.clone();
        }
        self.overlay_builds.set(self.overlay_builds.get() + 1);
        let mut rows = self.book.receipt_snapshot().as_ref().clone();
        for row in &mut rows {
            row.error = Some(error.clone());
        }
        let rows = std::sync::Arc::new(rows);
        *cache = Some(ReceiptOverlay {
            revision: self.book.revision(),
            error: error.clone(),
            rows: rows.clone(),
        });
        rows
    }
    /// Ordered and error-overlay materializations, for finite owner diagnostics.
    #[doc(hidden)]
    pub fn snapshot_build_counts(&self) -> (usize, usize) {
        (self.book.snapshot_build_count(), self.overlay_builds.get())
    }
    fn storage_error(&mut self, error: Option<String>) {
        if self.storage_error != error {
            self.storage_error = error;
            *self.overlay.get_mut() = None;
        }
    }

    /// Open only the archived viewer, reusing an existing live viewer for this job.
    pub fn open_archive(
        &mut self,
        id: &JobId,
        target: guild_session::overlay::Target,
        look: Look,
        cx: &mut App,
    ) -> Result<JobReceipt, JobError> {
        self.poll(cx);
        let receipt = self
            .book
            .archive_source(id)
            .cloned()
            .ok_or("No saved archive for this job")?;
        let spec = receipt.archive_view(target)?;
        let prefix = format!("archive-view-{}-", receipt.id);
        if let Some(viewer) = self.receipt_snapshot().iter().find(|r| {
            r.owner == receipt.owner
                && r.project == receipt.project
                && r.request_id.starts_with(&prefix)
                && !r.state.is_terminal()
        }) {
            return Ok(viewer.clone());
        }
        self.start_in(
            receipt.owner,
            receipt.project,
            &spec,
            receipt.launch_context.as_deref().cloned(),
            look,
            cx,
        )
    }

    /// Whether this row can recover its own or its linked original archive.
    pub fn can_open_archive(&self, id: &JobId) -> bool {
        self.book.archive_source(id).is_some()
    }

    /// Retained terminal output, including after process completion.
    pub fn terminal(&self, id: &JobId) -> Option<Entity<TerminalTab>> {
        self.terminals.get(id).cloned()
    }

    /// Explain missing content without guessing a transcript from job metadata.
    pub fn output_state(&self, id: &JobId, cx: &App) -> Option<OutputState> {
        let receipt = self.book.get(id)?;
        Some(if receipt.output_deleted {
            OutputState::Deleted
        } else if let Some(terminal) = self.terminals.get(id) {
            if !receipt.state.is_terminal() {
                OutputState::Live
            } else if terminal.read(cx).has_text() {
                OutputState::Retained
            } else {
                OutputState::Empty
            }
        } else {
            OutputState::Unavailable
        })
    }

    pub(crate) fn views(&self) -> impl Iterator<Item = Entity<TerminalTab>> + '_ {
        self.terminals.values().cloned()
    }
    /// The storage issue the window must surface.
    pub fn error(&self) -> Option<&str> {
        self.load_error.as_deref().or(self.storage_error.as_deref())
    }
    fn persist(&mut self) -> Result<(), JobError> {
        let result = self
            .store
            .save(&Receipts {
                jobs: self.book.receipts(),
            })
            .map_err(|_| {
                "Terminal receipt could not be saved; the in-memory job remains supervised"
                    .to_owned()
            });
        self.storage_error(result.as_ref().err().cloned());
        result
    }
    /// Reserve and persist before launching once. An identical retry only reads.
    pub fn start(
        &mut self,
        owner: SessionId,
        project: ProjectId,
        spec: &LaunchSpec,
        look: Look,
        cx: &mut App,
    ) -> Result<JobReceipt, JobError> {
        self.start_in(owner, project, spec, None, look, cx)
    }
    /// Start with trusted immutable context, preserving old metadata on retries.
    pub fn start_in(
        &mut self,
        owner: SessionId,
        project: ProjectId,
        spec: &LaunchSpec,
        context: Option<LaunchContext>,
        look: Look,
        cx: &mut App,
    ) -> Result<JobReceipt, JobError> {
        if let Some(error) = &self.load_error {
            return Err(error.clone());
        }
        let reservation = self.book.reserve_in(owner, project, spec, context)?;
        let id = *reservation.job_id();
        if matches!(reservation, Reservation::Existing(_)) {
            return Ok(self.receipt(&id).expect("reserved"));
        }
        if self.persist().is_err() {
            let error = "Receipt reservation could not be saved; no process was started".to_owned();
            self.book.transition(&id, JobState::FailedToStart)?;
            self.book.get_mut(&id).expect("reserved").error = Some(error.clone());
            self.storage_error(Some(error.clone()));
            return Err(error);
        }
        let mut command = portable_pty::CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        // A job is not another Guild agent session. It cannot inherit a caller
        // identity from the shell which started the desktop itself.
        for (key, _) in std::env::vars_os() {
            if key.to_string_lossy().starts_with("GUILD_") {
                command.env_remove(key);
            }
        }
        if let Err(error) =
            crate::network::apply_pty(&mut command, guild_session::network::Scope::Tools)
        {
            self.book.transition(&id, JobState::FailedToStart)?;
            self.book.get_mut(&id).expect("reserved").error = Some(error.to_string());
            let _ = self.persist();
            return Err(error.to_string());
        }
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        match view::open_job(command, &spec.cwd, look, cx) {
            Ok(terminal) => {
                self.terminals.insert(id, terminal);
                self.book.transition(&id, JobState::Running)?;
            }
            Err(_) => {
                self.book.transition(&id, JobState::FailedToStart)?;
                self.book.get_mut(&id).expect("reserved").error =
                    Some("Terminal program could not be started".into());
            }
        }
        let _ = self.persist();
        Ok(self.receipt(&id).expect("reserved"))
    }
    /// Collect completion without waiting. Finished archive viewers dismiss once;
    /// their receipts and terminal output remain available for explicit reopening.
    pub fn poll(&mut self, cx: &mut App) -> bool {
        let mut changed = false;
        for (id, terminal) in &self.terminals {
            if self.book.get(id).is_none_or(|r| r.state.is_terminal()) {
                continue;
            }
            if let Some(result) = terminal.read(cx).job_completion() {
                complete(&mut self.book, id, result);
                changed = true;
            }
        }
        if changed {
            let _ = self.persist();
        }
        changed
    }
    /// Request cancellation without waiting on the UI thread. Repeated calls are safe.
    pub fn cancel(&mut self, id: &JobId, reason: &str, cx: &mut App) -> Result<(), JobError> {
        self.poll(cx);
        let receipt = self.book.get(id).ok_or("No terminal job has that ID")?;
        if receipt.state.is_terminal() || receipt.state == JobState::Cancelling {
            return Ok(());
        }
        let receipt = self.book.get_mut(id).expect("checked");
        receipt.state = JobState::Cancelling;
        receipt.cancellation = Some(reason.into());
        if let Some(terminal) = self.terminals.get(id) {
            terminal.update(cx, |t, _| t.cancel_job());
        }
        let _ = self.persist();
        Ok(())
    }
    /// Update presentation without releasing the process or its original owner.
    pub fn present(
        &mut self,
        id: &JobId,
        target: Option<guild_session::overlay::Target>,
        presentation: Presentation,
    ) -> Result<(), JobError> {
        let receipt = self.book.get(id).ok_or("No terminal job has that ID")?;
        if receipt.target == target && receipt.presentation == presentation {
            return if self.storage_error.is_some() {
                self.persist()
            } else {
                Ok(())
            };
        }
        let receipt = self.book.get_mut(id).expect("checked");
        receipt.target = target;
        receipt.presentation = presentation;
        self.persist()
    }
    /// Release only a completed grid, preserving durable metadata.
    pub fn release(&mut self, id: &JobId) -> Result<(), JobError> {
        if !self
            .book
            .get(id)
            .ok_or("No terminal job has that ID")?
            .state
            .is_terminal()
        {
            return Err("A running terminal job cannot be released".into());
        }
        let receipt = self.book.get(id).expect("checked");
        if receipt.output_deleted
            && receipt.target.is_none()
            && receipt.presentation == Presentation::Closed
            && !self.terminals.contains_key(id)
        {
            return if self.storage_error.is_some() {
                self.persist()
            } else {
                Ok(())
            };
        }
        self.terminals.remove(id);
        let receipt = self.book.get_mut(id).expect("checked");
        receipt.output_deleted = true;
        receipt.target = None;
        receipt.presentation = Presentation::Closed;
        self.persist()
    }
    /// Quit finalization owns plain metadata/completion slots off UI. App is
    /// never re-entered from a quit future; final receipts use the normal wire.
    pub(crate) fn finish_shutdown(
        &mut self,
        cx: &App,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()>>> {
        let closing = crate::transport::finish_closing();
        if self.book.is_empty() {
            return Box::pin(closing);
        }
        let completions: Vec<_> = self
            .terminals
            .iter()
            .filter_map(|(id, t)| t.read(cx).job_completion_handle().map(|c| (*id, c)))
            .collect();
        let mut book = std::mem::take(&mut self.book);
        *self.overlay.get_mut() = None;
        let store = std::mem::replace(&mut self.store, Store::new(PathBuf::new()));
        let executor = cx.background_executor().clone();
        Box::pin(async move {
            closing.await;
            executor
                .spawn(async move {
                    for (id, slot) in completions {
                        if let Some(result) = slot.lock().unwrap().clone() {
                            complete(&mut book, &id, result);
                        }
                    }
                    if let Err(error) = store.save(&Receipts {
                        jobs: book.receipts(),
                    }) {
                        tracing::warn!(%error,"terminal receipts did not finish before quit");
                    }
                    guild_session::flush();
                })
                .await;
        })
    }
    /// Cancel/revoke all immediately; finish_shutdown awaits reapers and writes
    /// final completion receipts without re-entering the application.
    pub fn shutdown(&mut self, cx: &mut App) {
        for id in self.terminals.keys().copied().collect::<Vec<_>>() {
            let _ = self.cancel(&id, "Guild shutdown", cx);
        }
        for terminal in self.terminals.values() {
            terminal.update(cx, |t, cx| t.hang_up_for_quit(cx));
        }
        self.poll(cx);
        if !self.book.is_empty() {
            let _ = self.persist();
        }
    }
}

fn complete(book: &mut Book, id: &JobId, result: Result<portable_pty::ExitStatus, String>) {
    let Some(receipt) = book.get_mut(id) else {
        return;
    };
    match result {
        Ok(status) => {
            receipt.state = JobState::Exited;
            receipt.exit_code = Some(status.exit_code());
            receipt.signal = status.signal().map(str::to_owned);
            if receipt.is_archive_view()
                && receipt.signal.is_none()
                && receipt.cancellation.is_none()
            {
                receipt.presentation = Presentation::Closed;
            }
        }
        Err(_) => {
            receipt.state = JobState::Interrupted;
            receipt.error = Some("Terminal process cleanup failed".into());
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn overlay_and_metadata_snapshots_invalidate_without_rebuilding_equal_presentation() {
        let dir = tempfile::tempdir().unwrap();
        let mut jobs = LiveJobs::load(dir.path().join("receipts.json"));
        let owner = SessionId::new();
        let spec:LaunchSpec=serde_json::from_value(serde_json::json!({"version":1,"request_id":"snapshot","program":"/bin/true","cwd":"/tmp","target":{"kind":"session","id":owner}})).unwrap();
        let id = *jobs
            .book
            .reserve(owner, ProjectId::new(), &spec)
            .unwrap()
            .job_id();
        let original = jobs.receipt_snapshot();
        assert_eq!(original[0].state, JobState::Starting);
        jobs.present(&id, original[0].target.clone(), original[0].presentation)
            .unwrap();
        assert!(std::sync::Arc::ptr_eq(&original, &jobs.receipt_snapshot()));
        jobs.storage_error(Some("owned storage error".into()));
        let failed = jobs.receipt_snapshot();
        assert_eq!(failed[0].error.as_deref(), Some("owned storage error"));
        for _ in 0..5 {
            assert!(std::sync::Arc::ptr_eq(&failed, &jobs.receipt_snapshot()));
        }
        jobs.book.get_mut(&id).unwrap().title = "new title".into();
        let changed = jobs.receipt_snapshot();
        assert_eq!(changed[0].title, "new title");
        assert_eq!(failed[0].title, original[0].title);
        jobs.storage_error(None);
        assert!(jobs.receipt_snapshot()[0].error.is_none());
        jobs.book.transition(&id, JobState::FailedToStart).unwrap();
        jobs.release(&id).unwrap();
        let released = jobs.receipt_snapshot();
        assert!(released[0].output_deleted);
        jobs.release(&id).unwrap();
        assert!(std::sync::Arc::ptr_eq(&released, &jobs.receipt_snapshot()));
        let restored = LiveJobs::load(dir.path().join("receipts.json"));
        assert_eq!(restored.receipts(), jobs.receipts());
    }
}

#[cfg(test)]
mod quit_tests {
    use super::*;
    #[gpui::test]
    fn final_shutdown_receipt_is_persisted_without_reentering_app(cx: &mut gpui::TestAppContext) {
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("jobs.json");
        let mut jobs = LiveJobs::load(path.clone());
        let owner = SessionId::new();
        let project = ProjectId::new();
        let spec:LaunchSpec=serde_json::from_value(serde_json::json!({"version":1,"request_id":"quit","program":"/bin/sleep","args":["15"],"cwd":dir.path(),"target":{"kind":"session","id":owner}})).unwrap();
        let receipt = cx.update(|cx| {
            jobs.start(
                owner,
                project,
                &spec,
                Look::of(&Default::default(), Default::default()),
                cx,
            )
            .unwrap()
        });
        let finish = cx.update(|cx| {
            jobs.shutdown(cx);
            jobs.finish_shutdown(cx)
        });
        cx.foreground_executor().block_test(finish);
        let saved: Receipts = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved.jobs.len(), 1);
        let final_receipt = &saved.jobs[0];
        assert_eq!(final_receipt.id, receipt.id);
        assert_eq!(final_receipt.owner, owner);
        assert_eq!(final_receipt.project, project);
        assert_eq!(final_receipt.state, JobState::Exited);
        assert_eq!(
            final_receipt.cancellation.as_deref(),
            Some("Guild shutdown")
        );
        assert!(final_receipt.exit_code.is_some());
    }
}
