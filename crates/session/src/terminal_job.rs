//! Terminal job protocol and presentation-independent lifecycle, without a UI or process.
use crate::{ProjectId, SessionId, overlay::Target};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::PathBuf};

mod history;
pub use history::{HistoryContext, JobGroups, Scope};

/// Stable job identity, independent of a tab or overlay.
pub type JobId = uuid::Uuid;
/// A rejected launch or invalid lifecycle operation.
pub type JobError = String;

/// Actions sent through Guild's existing caller-aware control socket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum TerminalCommand {
    /// Probe without starting a process.
    Capabilities,
    /// Reserve and start a literal invocation.
    Start {
        /// Validated process descriptor.
        spec: LaunchSpec,
    },
    /// Observe a stable receipt.
    Status {
        /// Job to observe.
        id: JobId,
    },
    /// Reopen presentation.
    Show {
        /// Job to show.
        id: JobId,
    },
    /// Minimize presentation.
    Hide {
        /// Job to hide.
        id: JobId,
    },
    /// Remove presentation without cancelling.
    Close {
        /// Job whose presentation is closed.
        id: JobId,
    },
    /// Explicit bounded process shutdown.
    Cancel {
        /// Job to cancel.
        id: JobId,
    },
}

/// Typed replies keep process status distinct from transport success.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TerminalReply {
    /// Supported protocol and operations.
    Capabilities {
        /// Protocol version.
        version: u32,
        /// Recognized action names.
        operations: Vec<String>,
    },
    /// A current lifecycle snapshot.
    Job {
        /// Stable receipt, including process completion if known.
        receipt: Box<JobReceipt>,
    },
}

/// Requested fraction of the target pane, in percent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Geometry {
    /// Width, from one to one hundred.
    pub width_percent: u8,
    /// Height, from one to one hundred.
    pub height_percent: u8,
}

/// A literal process invocation. This is never persisted with the receipt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchSpec {
    /// Protocol version.
    pub version: u32,
    /// Caller-scoped idempotency key.
    pub request_id: String,
    /// Executable name or path, not a shell expression.
    pub program: String,
    /// Literal arguments.
    #[serde(default)]
    pub args: Vec<String>,
    /// Explicit absolute working directory.
    pub cwd: PathBuf,
    /// Owned presentation target.
    pub target: Target,
    /// Child-only environment overrides, never included in a receipt.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Optional native card title.
    #[serde(default)]
    pub title: String,
    /// Optional pane-relative geometry.
    #[serde(default)]
    pub geometry: Option<Geometry>,
    /// Explicit request to focus a currently visible target.
    #[serde(default)]
    pub foreground: bool,
    /// Optional read-only archive recovery, persisted separately from argv and env.
    #[serde(
        default,
        rename = "archive_viewer",
        alias = "revmux_archive",
        skip_serializing_if = "Option::is_none"
    )]
    pub revmux_archive: Option<Box<RevmuxArchive>>,
}
impl LaunchSpec {
    /// Reject malformed requests before reserving identity or starting a child.
    pub fn validate(&self) -> Result<(), JobError> {
        if let Some(archive) = &self.revmux_archive {
            archive.validate()?;
        }
        if self.version != 1 {
            return Err("Unsupported terminal protocol version".into());
        }
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.request_id.contains('\0')
        {
            return Err("Terminal request_id must contain 1..128 bytes without NUL".into());
        }
        if self.program.trim().is_empty()
            || self.program.contains('\0')
            || self.args.iter().any(|v| v.contains('\0'))
        {
            return Err("Terminal program must be nonempty and argv must not contain NUL".into());
        }
        if !self.cwd.is_absolute() || self.cwd.as_os_str().as_encoded_bytes().contains(&0) {
            return Err("Terminal cwd must be an absolute path without NUL".into());
        }
        if self.title.len() > 512 || self.title.contains('\0') {
            return Err("Terminal title is invalid".into());
        }
        if let Some(g) = &self.geometry
            && (!(1..=100).contains(&g.width_percent) || !(1..=100).contains(&g.height_percent))
        {
            return Err("Terminal geometry must be 1..100 percent".into());
        }
        for (key, value) in &self.env {
            if key.is_empty()
                || key.contains(['=', '\0'])
                || value.contains('\0')
                || key.starts_with("GUILD_")
            {
                return Err(
                    "Terminal environment contains an invalid or reserved key/value".into(),
                );
            }
        }
        Ok(())
    }
}

/// Process lifecycle; presentation has a separate state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    /// Reserved before spawning.
    Starting,
    /// Child is supervised.
    Running,
    /// Shutdown has been requested but not completed.
    Cancelling,
    /// Child and owned group have completed.
    Exited,
    /// No child could be started.
    FailedToStart,
    /// A previous application lifetime did not record completion.
    Interrupted,
}
impl JobState {
    /// No more execution can occur under this identity.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Exited | Self::FailedToStart | Self::Interrupted)
    }
}

/// Visibility does not control process lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Presentation {
    /// Expanded view.
    Shown,
    /// Minimized view.
    Hidden,
    /// No current overlay record.
    #[default]
    Closed,
}

/// Persistable and public job metadata without process arguments or environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobReceipt {
    /// Protocol version.
    pub version: u32,
    /// Stable identity.
    pub id: JobId,
    /// Original automated owner; native reattachment does not replace it.
    pub owner: SessionId,
    /// Original project.
    pub project: ProjectId,
    /// Caller request identity.
    pub request_id: String,
    /// Native display title.
    pub title: String,
    /// Reservation time in Unix milliseconds; unknown for legacy receipts.
    #[serde(default)]
    pub started_at_ms: Option<u64>,
    /// Explicit user deletion, distinct from a buffer lost on restart.
    #[serde(default)]
    pub output_deleted: bool,
    /// Saved review location; legacy and ordinary terminal jobs have none.
    #[serde(
        default,
        rename = "archive_viewer",
        alias = "revmux_archive",
        skip_serializing_if = "Option::is_none"
    )]
    pub revmux_archive: Option<Box<RevmuxArchive>>,
    /// Immutable trusted launch provenance; absent on legacy receipts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_context: Option<Box<LaunchContext>>,
    /// Process state.
    pub state: JobState,
    /// Optional current target; absent after owner removal.
    pub target: Option<Target>,
    /// Presentation state.
    pub presentation: Presentation,
    /// Process exit code, if reported.
    pub exit_code: Option<u32>,
    /// Terminating signal, if reported.
    pub signal: Option<String>,
    /// Cancellation source, distinct from the exit code.
    pub cancellation: Option<String>,
    /// A sanitized lifecycle/storage error.
    pub error: Option<String>,
}

/// Original labels and identities captured by the window, never by a producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchContext {
    /// Resolved conversation at reservation, separate from the process owner.
    pub conversation: Option<SessionId>,
    /// Original conversation label, if available.
    pub session_label: Option<String>,
    /// Original root path, separate from the program's working directory.
    pub root_path: Option<PathBuf>,
    /// Original worktree display name.
    pub root_label: Option<String>,
    /// Git common directory from the asynchronous worker, if known.
    pub repository: Option<PathBuf>,
    /// Original repository display name.
    pub repository_label: Option<String>,
    /// Last observed original-root branch, not freshly verified HEAD.
    pub branch: Option<String>,
    /// Literal program cwd from the validated invocation.
    pub program_cwd: PathBuf,
}
impl LaunchContext {
    /// Remove terminal controls from optional display labels without resolving paths.
    pub fn sanitized(mut self) -> Self {
        for label in [
            &mut self.session_label,
            &mut self.root_label,
            &mut self.repository_label,
            &mut self.branch,
        ] {
            *label = label.take().and_then(|value| {
                let value: String = value
                    .chars()
                    .filter(|c| !c.is_control())
                    .take(512)
                    .collect();
                (!value.trim().is_empty()).then_some(value)
            });
        }
        self
    }
}

/// A saved review archive, independent of the original review invocation.
/// The historical Rust name remains source-compatible; JSON uses `archive_viewer`.
/// Both wire spellings decode into one field, so serde rejects duplicates (even null).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevmuxArchive {
    /// Absolute round directory containing the saved report and events.
    pub directory: PathBuf,
    /// Absolute executable supporting literal `view --archive DIRECTORY` arguments.
    pub program: PathBuf,
}
impl RevmuxArchive {
    /// Reject paths which cannot be passed literally to the viewer.
    pub fn validate(&self) -> Result<(), JobError> {
        for path in [&self.directory, &self.program] {
            if !path.is_absolute() || path.as_os_str().as_encoded_bytes().contains(&0) {
                return Err("Archive viewer paths must be absolute and contain no NUL".into());
            }
        }
        if self.program.file_name().is_none() {
            return Err("Archive viewer executable must name a file".into());
        }
        Ok(())
    }
}
impl JobReceipt {
    /// Recognize the internal viewer request namespace independently of display titles.
    /// This is presentation metadata, never an authorization check.
    pub fn is_archive_view(&self) -> bool {
        self.archive_source_id().is_some()
    }

    /// Original launch encoded by the internal viewer namespace, including legacy receipts.
    /// This is a relationship hint; callers must check original owner and project.
    pub fn archive_source_id(&self) -> Option<JobId> {
        let request = self.request_id.strip_prefix("archive-view-")?;
        let original = request.get(..36).and_then(|id| JobId::parse_str(id).ok())?;
        let viewer = request.get(36..)?.strip_prefix('-')?;
        (original != self.id && JobId::parse_str(viewer).is_ok()).then_some(original)
    }

    /// Build a new read-only viewer, never the original review command.
    pub fn archive_view(&self, target: Target) -> Result<LaunchSpec, JobError> {
        if !self.state.is_terminal() {
            return Err("The review is still running; show its live output".into());
        }
        let archive = self
            .revmux_archive
            .as_ref()
            .ok_or("No saved archive viewer for this job")?;
        archive.validate()?;
        Ok(LaunchSpec {
            version: 1,
            request_id: format!("archive-view-{}-{}", self.id, JobId::new_v4()),
            program: archive.program.to_string_lossy().into_owned(),
            args: vec![
                "view".into(),
                "--archive".into(),
                archive.directory.to_string_lossy().into_owned(),
            ],
            cwd: archive.directory.clone(),
            target,
            env: BTreeMap::new(),
            title: format!(
                "{} (archive)",
                archive
                    .program
                    .file_name()
                    .expect("validated executable")
                    .to_string_lossy()
            ),
            geometry: Some(Geometry {
                width_percent: 90,
                height_percent: 90,
            }),
            foreground: true,
            revmux_archive: None,
        })
    }
}

/// Whether a caller won the right to spawn or recovered an existing request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reservation {
    /// New reservation; persist before spawning.
    New(JobId),
    /// Existing job; never spawn again.
    Existing(JobId),
}
impl Reservation {
    /// Identity shared by the response and subsequent controls.
    pub fn job_id(&self) -> &JobId {
        match self {
            Self::New(id) | Self::Existing(id) => id,
        }
    }
}

/// Reservation book. Process descriptors remain in memory only.
#[derive(Default)]
pub struct Book {
    entries: BTreeMap<JobId, (JobReceipt, Option<LaunchSpec>)>,
    revision: u64,
    snapshot: std::sync::Mutex<Option<std::sync::Arc<Vec<JobReceipt>>>>,
    builds: std::sync::atomic::AtomicUsize,
}
impl Book {
    /// Validate and reserve an owner-scoped request.
    pub fn reserve(
        &mut self,
        owner: SessionId,
        project: ProjectId,
        spec: &LaunchSpec,
    ) -> Result<Reservation, JobError> {
        self.reserve_in(owner, project, spec, None)
    }
    /// Reserve once with window-resolved provenance; retries keep the first snapshot.
    pub fn reserve_in(
        &mut self,
        owner: SessionId,
        project: ProjectId,
        spec: &LaunchSpec,
        context: Option<LaunchContext>,
    ) -> Result<Reservation, JobError> {
        spec.validate()?;
        if let Some((receipt, previous)) = self
            .entries
            .values()
            .find(|(r, _)| r.owner == owner && r.request_id == spec.request_id)
        {
            if previous.as_ref() != Some(spec) || receipt.project != project {
                return Err("Terminal request_id was already used with another descriptor or in an earlier application lifetime; inspect the existing job".into());
            }
            return Ok(Reservation::Existing(receipt.id));
        }
        let id = JobId::new_v4();
        let receipt = JobReceipt {
            version: 1,
            id,
            owner,
            project,
            request_id: spec.request_id.clone(),
            title: if spec.title.is_empty() {
                "Terminal".into()
            } else {
                spec.title.clone()
            },
            started_at_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .and_then(|elapsed| u64::try_from(elapsed.as_millis()).ok()),
            output_deleted: false,
            revmux_archive: spec.revmux_archive.clone(),
            launch_context: context.map(|mut context| {
                context.program_cwd = spec.cwd.clone();
                Box::new(context.sanitized())
            }),
            state: JobState::Starting,
            target: Some(spec.target.clone()),
            presentation: Presentation::Closed,
            exit_code: None,
            signal: None,
            cancellation: None,
            error: None,
        };
        self.invalidate();
        self.entries.insert(id, (receipt, Some(spec.clone())));
        Ok(Reservation::New(id))
    }
    /// Read a receipt without changing lifetime.
    pub fn get(&self, id: &JobId) -> Option<&JobReceipt> {
        self.entries.get(id).map(|e| &e.0)
    }
    /// Public receipt snapshot suitable for persistence.
    pub fn receipts(&self) -> Vec<JobReceipt> {
        self.receipt_snapshot().as_ref().clone()
    }
    /// Immutable ordered metadata, shared until a mutation gateway invalidates it.
    pub fn receipt_snapshot(&self) -> std::sync::Arc<Vec<JobReceipt>> {
        let mut cache = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .get_or_insert_with(|| {
                self.builds
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let mut rows: Vec<_> = self.entries.values().map(|e| e.0.clone()).collect();
                rows.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms).then(a.id.cmp(&b.id)));
                std::sync::Arc::new(rows)
            })
            .clone()
    }
    /// Number of ordered materializations, for finite owner diagnostics.
    #[doc(hidden)]
    pub fn snapshot_build_count(&self) -> usize {
        self.builds.load(std::sync::atomic::Ordering::Relaxed)
    }
    /// Metadata revision, advanced by mutable access and real mutations.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Whether any durable job metadata is retained.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    fn invalidate(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        *self
            .snapshot
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    }
    /// Resolve a viewer's original receipt without changing durable history.
    /// Never borrow another owner's or project's archive, or follow viewer chains.
    pub fn archive_source(&self, id: &JobId) -> Option<&JobReceipt> {
        let receipt = self.get(id)?;
        let source = match receipt.archive_source_id() {
            Some(original) => self.get(&original)?,
            None => receipt,
        };
        (source.owner == receipt.owner
            && source.project == receipt.project
            && !source.is_archive_view()
            && source.state.is_terminal()
            && source
                .revmux_archive
                .as_ref()
                .is_some_and(|a| a.validate().is_ok()))
        .then_some(source)
    }

    /// Apply a permitted process-state transition.
    pub fn transition(&mut self, id: &JobId, next: JobState) -> Result<(), JobError> {
        let receipt = self.get(id).ok_or("No terminal job has that ID")?;
        let allowed = receipt.state == next
            || matches!(
                (receipt.state, next),
                (
                    JobState::Starting,
                    JobState::Running | JobState::FailedToStart | JobState::Interrupted
                ) | (
                    JobState::Running,
                    JobState::Cancelling | JobState::Exited | JobState::Interrupted
                ) | (
                    JobState::Cancelling,
                    JobState::Exited | JobState::Interrupted
                )
            );
        if !allowed {
            return Err("Invalid terminal job transition".into());
        }
        if receipt.state != next {
            self.get_mut(id).expect("checked").state = next;
        }
        Ok(())
    }
    /// Update presentation or completion metadata under the live owner's control.
    pub fn get_mut(&mut self, id: &JobId) -> Option<&mut JobReceipt> {
        if self.entries.contains_key(id) {
            self.invalidate();
        }
        self.entries.get_mut(id).map(|e| &mut e.0)
    }
    /// Recover receipts without executing any program.
    pub fn restore(receipts: Vec<JobReceipt>) -> Result<Self, JobError> {
        let mut book = Self::default();
        for mut receipt in receipts {
            if receipt.version != 1
                || book.entries.contains_key(&receipt.id)
                || book
                    .entries
                    .values()
                    .any(|(r, _)| r.owner == receipt.owner && r.request_id == receipt.request_id)
            {
                return Err("Invalid or duplicate persisted terminal job".into());
            }
            if !receipt.state.is_terminal() {
                receipt.state = JobState::Interrupted;
            }
            receipt.presentation = Presentation::Closed;
            receipt.target = None;
            book.entries.insert(receipt.id, (receipt, None));
        }
        Ok(book)
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn repeated_metadata_reads_build_order_once() {
        let mut book = Book::default();
        let owner = SessionId::new();
        let spec:LaunchSpec=serde_json::from_value(serde_json::json!({"version":1,"request_id":"snapshot","program":"/bin/true","cwd":"/tmp","target":{"kind":"session","id":owner}})).unwrap();
        book.reserve(owner, ProjectId::new(), &spec).unwrap();
        let first = book.receipts();
        for _ in 0..5 {
            assert_eq!(book.receipts(), first);
        }
        assert_eq!(
            book.builds.load(std::sync::atomic::Ordering::Relaxed),
            1,
            "unchanged metadata must not be cloned and sorted again to build its ordered snapshot"
        );
    }
    #[test]
    fn mutations_invalidate_sorted_values_but_noop_transitions_do_not() {
        let mut book = Book::default();
        let owner = SessionId::new();
        let project = ProjectId::new();
        for n in 0..3 {
            let spec:LaunchSpec=serde_json::from_value(serde_json::json!({"version":1,"request_id":format!("row-{n}"),"program":"/bin/true","cwd":"/tmp","target":{"kind":"session","id":owner}})).unwrap();
            let id = *book.reserve(owner, project, &spec).unwrap().job_id();
            book.get_mut(&id).unwrap().started_at_ms = Some(n);
        }
        let old = book.receipt_snapshot();
        assert_eq!(
            old.iter()
                .map(|r| r.started_at_ms.unwrap())
                .collect::<Vec<_>>(),
            vec![2, 1, 0]
        );
        let id = old[0].id;
        let rev = book.revision();
        book.transition(&id, JobState::Starting).unwrap();
        assert_eq!(book.revision(), rev);
        assert!(book.transition(&id, JobState::Exited).is_err());
        assert_eq!(book.revision(), rev);
        book.get_mut(&id).unwrap().started_at_ms = Some(0);
        let fresh = book.receipt_snapshot();
        assert_eq!(fresh[0].started_at_ms, Some(1));
        assert_eq!(old[0].started_at_ms, Some(2));
        let restored = Book::restore(book.receipts()).unwrap();
        assert!(
            restored
                .receipt_snapshot()
                .iter()
                .all(|r| r.state == JobState::Interrupted)
        );
        assert_eq!(book.builds.load(std::sync::atomic::Ordering::Relaxed), 2);
    }
}
