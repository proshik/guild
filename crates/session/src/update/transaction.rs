//! Journaled replacement on one destination volume. No UI or network dependency.
mod fs;
mod handoff;
pub use handoff::{ArmToken, Handoff, run_helper};
mod startup;
mod store;
use super::{Identity, bundle};
use serde::{Deserialize, Serialize};
pub use startup::Startup;
use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
pub use store::Installed;
use uuid::Uuid;
/// Durable transaction state; prepared/previous paths are derived, never read as arbitrary deletion paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// The journal exists; adoption of the validated payload may still be pending.
    Preparing,
    /// Validated payload is durably retained beside the destination.
    Prepared,
    /// Previous app has moved to its unique retained sibling.
    OldRetained,
    /// New app occupies the destination; launch has not yet been requested.
    NewInPlace,
    /// A fresh instance was requested; its first layout is unacknowledged.
    LaunchRequested,
    /// The exact new instance acknowledged startup, or the owner explicitly kept it.
    Acknowledged,
    /// Automatic completion stopped with owned recovery files retained.
    RecoveryRequired,
    /// Previous app is back at the destination.
    Restored,
    /// A transaction was cancelled before replacement.
    Aborted,
}
/// Sanitized transaction failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Failure {
    /// Malformed or unsupported input.
    Invalid,
    /// A previously pinned file or directory no longer matches.
    Changed,
    /// Another transaction or unexpected destination prevents safe progress.
    Conflict,
    /// A filesystem publication or move failed.
    Storage,
    /// The current owner cancelled work.
    Cancelled,
    /// A crash boundary was injected; recovery must inspect actual files.
    Interrupted,
    /// The native launch request failed.
    Launch,
    /// No valid first-layout acknowledgement arrived within the bounded wait.
    MissingAck,
    /// The acknowledgement does not identify this new instance.
    InvalidAck,
}
/// Fingerprint of actual app bytes, distinct from release ordering.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    /// Artifact UUID, absent only for legacy metadata.
    pub build_id: Option<Uuid>,
    /// Full source revision.
    pub commit: String,
    /// Fingerprint of both executables and the signed resource envelope.
    pub seal: String,
}
/// One owned transaction receipt. No caller-supplied backup/preparation paths.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    schema: u32,
    id: Uuid,
    nonce: Uuid,
    arm_nonce: Uuid,
    parent_pid: u32,
    destination: PathBuf,
    parent: fs::DirectoryId,
    previous: Proof,
    next: Proof,
    selection: Identity,
    phase: Phase,
    failure: Option<Failure>,
}
impl std::fmt::Debug for Receipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Receipt")
            .field("id", &self.id)
            .field("phase", &self.phase)
            .field("destination", &self.destination)
            .finish_non_exhaustive()
    }
}
impl Receipt {
    /// Stable transaction identity.
    pub fn id(&self) -> Uuid {
        self.id
    }
    /// Last durably published phase.
    pub fn phase(&self) -> Phase {
        self.phase
    }
    /// Exact canonical installed app path.
    pub fn destination(&self) -> &Path {
        &self.destination
    }
    /// Exact selected provenance.
    pub fn selection(&self) -> &Identity {
        &self.selection
    }
    /// Manual recovery may run only from a native-validated artifact in this transaction.
    pub fn verify_recovery_executor(
        &self,
        executable: &Path,
        cancel: &AtomicBool,
    ) -> Result<(), Failure> {
        let actual = executable.canonicalize().map_err(|_| Failure::Changed)?;
        let slots = [
            self.destination.clone(),
            self.previous_path(),
            self.prepared_path(),
            self.failed_path(),
        ];
        let app = slots
            .iter()
            .find(|app| {
                app.join("Contents/MacOS/guild-desktop")
                    .canonicalize()
                    .ok()
                    .as_ref()
                    == Some(&actual)
            })
            .ok_or(Failure::Changed)?;
        let proof = Native.proof(app, cancel)?;
        if proof != self.previous && proof != self.next {
            return Err(Failure::Changed);
        }
        Ok(())
    }
    /// Unique owned sibling holding the validated payload before replacement.
    pub fn prepared_path(&self) -> PathBuf {
        self.sibling("prepared")
    }
    /// Unique owned sibling retained until acknowledgement.
    pub fn previous_path(&self) -> PathBuf {
        self.sibling("previous")
    }
    /// Unique owned sibling retaining a failed new payload for recovery.
    pub fn failed_path(&self) -> PathBuf {
        self.sibling("failed")
    }
    fn sibling(&self, role: &str) -> PathBuf {
        let name = self
            .destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        self.destination
            .with_file_name(format!(".{name}.guild-update-{}.{role}.app", self.id))
    }
}
/// Explicit recovery never infers whether a possibly live new app may be killed.
#[derive(Debug, Clone, Copy)]
pub enum Recovery {
    /// Restore only proven previous bytes after the current app has exited.
    RestorePrevious,
    /// Explicitly retain proven new bytes and retire owned recovery copies.
    KeepCurrent,
}
/// Failed admission with the exact owned receipt, when journal publication began.
#[derive(Debug)]
pub struct BeginFailure {
    /// Sanitized failure reason.
    pub failure: Failure,
    /// Owned transaction identity; never adopt an unrelated active pointer after failure.
    pub receipt: Option<Box<Receipt>>,
}
/// One user-owned state root; only its private update/transactions records are touched.
pub struct Store {
    state: PathBuf,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Boundary {
    JournalWritten,
    ActiveWritten,
    PayloadRetained,
    /// Validated payload is durably retained beside the destination.
    Prepared,
    OldMoved,
    /// Previous app has moved to its unique retained sibling.
    OldRetained,
    NewMoved,
    /// New app occupies the destination; launch has not yet been requested.
    NewInPlace,
    /// A fresh instance was requested; its first layout is unacknowledged.
    LaunchRequested,
    /// The exact new instance acknowledged startup, or the owner explicitly kept it.
    Acknowledged,
}
struct Admission<'a> {
    destination: &'a Path,
    staged: &'a Path,
    identity: &'a Identity,
    parent_pid: u32,
}
trait Platform {
    fn proof(&self, path: &Path, cancel: &AtomicBool) -> Result<Proof, Failure>;
    fn launch(&self, receipt: &Receipt, old: bool, state: &Path) -> Result<(), Failure>;
}
struct Native;
mod native;
#[cfg(test)]
mod tests;
