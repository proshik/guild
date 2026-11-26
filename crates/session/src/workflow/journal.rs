//! Strict, serialized run receipts. A reservation grants at most one effect attempt.
use super::{AttemptId, RunId, digest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Caller identity and full immutable operation context, excluding credentials.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    /// Stable caller-selected retry key, scoped to the run.
    pub request_id: String,
    /// Compare-and-set revision, retained even on retries after a lost reply.
    pub expected_revision: u64,
    /// Complete trusted caller/operation/source/payload context.
    pub context: serde_json::Value,
}
impl Mutation {
    /// Validate the bounded request before any reservation.
    pub fn new(id: &str, revision: u64, context: serde_json::Value) -> Result<Self, String> {
        let value = Self {
            request_id: id.into(),
            expected_revision: revision,
            context,
        };
        value.fingerprint()?;
        Ok(value)
    }
    fn fingerprint(&self) -> Result<String, String> {
        if self.request_id.is_empty()
            || self.request_id.len() > 128
            || self.request_id.contains('\0')
            || !self.context.is_object()
        {
            return Err(
                "workflow.request: bounded key and complete object context required".into(),
            );
        }
        let bytes = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("workflow.request: exceeds 1 MiB".into());
        }
        Ok(digest(&bytes))
    }
}
/// One atomic committed event, chained to its exact predecessor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventReceipt {
    /// Persisted format version.
    pub version: u32,
    /// Stable process identity.
    pub run: RunId,
    /// Revision before this receipt.
    pub preceding_revision: u64,
    /// Committed revision.
    pub revision: u64,
    /// Digest of the preceding receipt, or genesis header.
    pub preceding_digest: String,
    /// Stable effect identity; resolutions refer to the original reservation.
    pub attempt: AttemptId,
    /// Present for a reservation; absent for a resolution.
    pub request: Option<Mutation>,
    /// Full request fingerprint for reservations, original fingerprint for resolutions.
    pub fingerprint: String,
    /// Actual correlated facts. A reservation contains no completion claim.
    pub observation: Option<serde_json::Value>,
    /// Digest of all other fields.
    pub sha256: String,
}
impl EventReceipt {
    fn hash(&self) -> Result<String, String> {
        let mut unsigned = self.clone();
        unsigned.sha256.clear();
        Ok(digest(
            &serde_json::to_vec(&unsigned).map_err(|e| e.to_string())?,
        ))
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    version: u32,
    run: RunId,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    version: u32,
    run: RunId,
    revision: u64,
    sha256: String,
}
fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    use std::io::Write;
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("workflow.storage: record exceeds 4 MiB".into());
    }
    // The general settings Store temporarily moves primary to backup. A strict
    // workflow reader cannot use that fallback, so replace primary directly.
    // The kernel claim serializes writers; unique scratch files retain failed
    // attempts without overwriting a prior lifetime's incomplete write.
    let parent = path
        .parent()
        .ok_or("workflow.storage: record parent unavailable")?;
    let directory = File::open(parent).map_err(|e| e.to_string())?;
    let mut name = path
        .file_name()
        .ok_or("workflow.storage: record name unavailable")?
        .to_os_string();
    name.push(format!(".{}.json.tmp", uuid::Uuid::new_v4()));
    let scratch = parent.join(name);
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(&scratch)
        .map_err(|e| format!("workflow.storage scratch: {e}"))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    std::fs::rename(scratch, path).map_err(|e| e.to_string())?;
    directory.sync_all().map_err(|e| e.to_string())
}
fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let meta = std::fs::symlink_metadata(path)
        .map_err(|e| format!("workflow.storage {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "workflow.storage {}: expected regular primary file",
            path.display()
        ));
    }
    let bytes = super::contract::bounded_read(path)?;
    let value = super::json::read(&bytes)?;
    serde_json::from_value(value).map_err(|e| format!("workflow.storage {}: {e}", path.display()))
}
/// Kernel-held lifetime claim. Lock files are never deleted or replaced on release.
struct Claim {
    file: File,
    path: PathBuf,
}
impl Claim {
    fn acquire(path: PathBuf) -> Result<Self, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&path)
            .map_err(|e| format!("workflow.claim: {e}"))?;
        // The kernel owns lifetime qualification; a stored PID alone never permits takeover.
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(
                "workflow.claim: another coordinator owns this run, or its lifetime is uncertain"
                    .into(),
            );
        }
        let claim = Self { file, path };
        claim.verify()?;
        Ok(claim)
    }
    fn verify(&self) -> Result<(), String> {
        let held = self.file.metadata().map_err(|e| e.to_string())?;
        let named = std::fs::symlink_metadata(&self.path).map_err(|e| e.to_string())?;
        if !named.is_file() || (held.dev(), held.ino()) != (named.dev(), named.ino()) {
            return Err("workflow.claim: coordinator claim path changed; mutation blocked".into());
        }
        Ok(())
    }
}
impl Drop for Claim {
    fn drop(&mut self) {
        // A concurrent fork may retain this open file description until exec.
        // Release this owner's lock now, rather than extending its lifetime to
        // an unrelated child's pre_exec. Closing our descriptor alone is not
        // sufficient for flock while such an inherited duplicate exists.
        unsafe { libc::flock(self.file.as_raw_fd(), libc::LOCK_UN) };
    }
}
/// Short admission lock serializing the active-run lookup and creation for one origin.
/// The lock file survives release; kernel ownership, never its PID, grants admission.
pub struct OriginAdmission {
    _claim: Claim,
}
impl OriginAdmission {
    /// Durable attribution survives partial run creation and invalid frozen data.
    /// A torn attribution blocks only this origin; never erase uncertain bytes.
    pub fn runs(&self) -> Result<Vec<RunId>, String> {
        use std::io::{Read, Seek, SeekFrom};
        self._claim.verify()?;
        let mut file = &self._claim.file;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 4 * 1024 * 1024 || (!bytes.is_empty() && !bytes.ends_with(b"\n")) {
            return Err(
                "workflow.origin: incomplete or oversized attribution; reconcile this origin"
                    .into(),
            );
        }
        bytes
            .split(|b| *b == b'\n')
            .filter(|s| !s.is_empty())
            .map(|s| {
                std::str::from_utf8(s)
                    .map_err(|e| e.to_string())?
                    .parse()
                    .map_err(|e| format!("workflow.origin: invalid attribution: {e}"))
            })
            .collect()
    }
    /// Reserve identity before creating any run files while holding the kernel lock.
    pub fn reserve(&self, run: RunId) -> Result<(), String> {
        use std::io::{Seek, SeekFrom, Write};
        if self.runs()?.contains(&run) {
            return Ok(());
        }
        let mut file = &self._claim.file;
        file.seek(SeekFrom::End(0)).map_err(|e| e.to_string())?;
        writeln!(file, "{run}").map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        File::open(
            self._claim
                .path
                .parent()
                .ok_or("workflow.origin: parent missing")?,
        )
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())
    }
    /// Acquire inside the canonical repository's workflow-runs directory.
    pub fn acquire(directory: &Path, origin: crate::SessionId) -> Result<Self, String> {
        Ok(Self {
            _claim: Claim::acquire(directory.join(format!(".origin-{origin}.lock")))?,
        })
    }
}
/// Validated read-only history, without a coordinator claim or execution rights.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct History {
    /// Stable process identity.
    pub run: RunId,
    /// Last contiguous committed revision.
    pub revision: u64,
    /// Original events including unresolved attempts.
    pub receipts: Vec<EventReceipt>,
}
/// One run's exclusive writer. Read-only consumers should inspect retained receipts separately.
/// A failed write poisons this lifetime; re-open only to reconcile committed facts.
pub struct Journal {
    directory: PathBuf,
    claim: Option<Claim>,
    snapshot: Snapshot,
    receipts: Vec<EventReceipt>,
    error: Option<String>,
}
impl Journal {
    /// Create only a genuinely new run directory. Existing/partial storage is never reset.
    pub fn create(directory: &Path, run: RunId) -> Result<Self, String> {
        std::fs::create_dir(directory).map_err(|e| format!("workflow.storage create: {e}"))?;
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let claim = Claim::acquire(directory.join("coordinator.lock"))?;
        std::fs::create_dir(directory.join("events")).map_err(|e| e.to_string())?;
        let header = Header { version: 1, run };
        save(&directory.join("header.json"), &header)?;
        let snapshot = Snapshot {
            version: 1,
            run,
            revision: 0,
            sha256: digest(&serde_json::to_vec(&header).map_err(|e| e.to_string())?),
        };
        save(&directory.join("state.json"), &snapshot)?;
        File::open(
            directory
                .parent()
                .ok_or("workflow.storage: missing parent")?,
        )
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
        Ok(Self {
            directory,
            claim: Some(claim),
            snapshot,
            receipts: Vec::new(),
            error: None,
        })
    }
    /// Replay contiguous committed events, validating the primary snapshot without backup fallback.
    /// Unresolved reservations remain unknown and never grant launch permission again.
    pub fn open(directory: &Path) -> Result<Self, String> {
        let directory = directory.canonicalize().map_err(|e| e.to_string())?;
        let claim = Claim::acquire(directory.join("coordinator.lock"))?;
        Self::replay(directory, Some(claim))
    }
    /// Inspect valid committed history while another window owns execution.
    /// Concurrent incomplete writes return a gap rather than an older backup.
    pub fn inspect(directory: &Path) -> Result<History, String> {
        let journal = Self::replay(directory.canonicalize().map_err(|e| e.to_string())?, None)?;
        Ok(History {
            run: journal.snapshot.run,
            revision: journal.revision(),
            receipts: journal.receipts,
        })
    }
    fn replay(directory: PathBuf, claim: Option<Claim>) -> Result<Self, String> {
        let header: Header = read(&directory.join("header.json"))?;
        let saved: Snapshot = read(&directory.join("state.json"))?;
        if header.version != 1 || saved.version != 1 || header.run != saved.run {
            return Err("workflow.storage: unsupported or mismatched run identity".into());
        }
        let genesis = digest(&serde_json::to_vec(&header).map_err(|e| e.to_string())?);
        let mut journal = Self {
            directory,
            claim,
            snapshot: Snapshot {
                version: 1,
                run: header.run,
                revision: 0,
                sha256: genesis,
            },
            receipts: Vec::new(),
            error: None,
        };
        let events = journal.directory.join("events");
        if !std::fs::symlink_metadata(&events)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            return Err("workflow.storage: events directory unavailable".into());
        }
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&events).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| "workflow.storage: invalid event name")?;
            // An unrenamed scratch file is not a committed receipt.
            if name.ends_with(".json.tmp") {
                continue;
            }
            if name.len() != 25
                || !name.ends_with(".json")
                || !name.as_bytes()[..20].iter().all(u8::is_ascii_digit)
            {
                return Err(
                    "workflow.storage: unexpected event/backup; reconciliation required".into(),
                );
            }
            names.push(name);
        }
        names.sort();
        if names.len() > 100_000 {
            return Err("workflow.storage: exceeds 100000 receipts".into());
        }
        let mut snapshot_matched = saved.revision == 0 && saved.sha256 == journal.snapshot.sha256;
        for name in names {
            let receipt: EventReceipt = read(&events.join(&name))?;
            if name != format!("{:020}.json", journal.snapshot.revision + 1) {
                return Err("workflow.storage: noncontiguous events".into());
            }
            journal.validate(&receipt)?;
            journal.apply(receipt);
            if saved.revision == journal.snapshot.revision {
                snapshot_matched = saved.sha256 == journal.snapshot.sha256;
            }
        }
        if !snapshot_matched {
            return Err(
                "workflow.storage: primary snapshot does not match committed history".into(),
            );
        }
        Ok(journal)
    }
    /// Last committed revision, including replayed events after a lagging snapshot.
    pub fn revision(&self) -> u64 {
        self.snapshot.revision
    }
    /// Actual retained events; no authored process-state inference.
    pub fn receipts(&self) -> &[EventReceipt] {
        &self.receipts
    }
    /// Reservations requiring reconciliation before any new effect may be admitted.
    pub fn unresolved(&self) -> Vec<EventReceipt> {
        let resolved: BTreeSet<_> = self
            .receipts
            .iter()
            .filter(|r| r.observation.is_some())
            .map(|r| r.attempt)
            .collect();
        self.receipts
            .iter()
            .filter(|r| r.request.is_some() && !resolved.contains(&r.attempt))
            .cloned()
            .collect()
    }
    /// Persist before an external effect. Only true admits one attempt in this lifetime.
    pub fn reserve(&mut self, request: &Mutation) -> Result<(EventReceipt, bool), String> {
        self.healthy()?;
        let fingerprint = request.fingerprint()?;
        if let Some(old) = self.receipts.iter().find(|r| {
            r.request
                .as_ref()
                .is_some_and(|r| r.request_id == request.request_id)
        }) {
            if old.fingerprint != fingerprint {
                return Err("workflow.request: key context conflict".into());
            }
            return Ok((old.clone(), false));
        }
        self.expected(request.expected_revision)?;
        if !self.unresolved().is_empty() {
            return Err("workflow.effect: unresolved reservation requires reconciliation".into());
        }
        let receipt = self.append(AttemptId::new(), Some(request.clone()), fingerprint, None)?;
        Ok((receipt, true))
    }
    /// Record observed effect facts; never replay the original invocation.
    /// Same-attempt identical retries coalesce even if their reply was lost.
    pub fn resolve(
        &mut self,
        attempt: AttemptId,
        expected_revision: u64,
        observation: serde_json::Value,
    ) -> Result<EventReceipt, String> {
        self.healthy()?;
        if !observation.is_object() {
            return Err("workflow.observation: object required".into());
        }
        if let Some(old) = self
            .receipts
            .iter()
            .find(|r| r.attempt == attempt && r.observation.is_some())
        {
            if old.observation.as_ref() != Some(&observation) {
                return Err("workflow.observation: attempt context conflict".into());
            }
            return Ok(old.clone());
        }
        self.expected(expected_revision)?;
        let reservation = self
            .receipts
            .iter()
            .find(|r| r.attempt == attempt && r.request.is_some())
            .ok_or("workflow.observation: no reserved attempt")?;
        self.append(
            attempt,
            None,
            reservation.fingerprint.clone(),
            Some(observation),
        )
    }
    fn healthy(&self) -> Result<(), String> {
        if let Some(error) = &self.error {
            return Err(error.clone());
        }
        self.claim
            .as_ref()
            .ok_or("workflow.claim: inspection has no mutation authority")?
            .verify()?;
        let saved: Snapshot = read(&self.directory.join("state.json"))?;
        if saved.run != self.snapshot.run || saved.version != 1 || saved.revision > self.revision()
        {
            return Err("workflow.storage: primary changed during coordination".into());
        }
        let expected = if saved.revision == 0 {
            let header: Header = read(&self.directory.join("header.json"))?;
            digest(&serde_json::to_vec(&header).map_err(|e| e.to_string())?)
        } else {
            let receipt = self
                .receipts
                .get((saved.revision - 1) as usize)
                .ok_or("workflow.storage: unknown primary revision")?;
            let actual: EventReceipt = read(
                &self
                    .directory
                    .join("events")
                    .join(format!("{:020}.json", saved.revision)),
            )?;
            if &actual != receipt {
                return Err("workflow.storage: committed event changed".into());
            }
            receipt.sha256.clone()
        };
        if saved.sha256 != expected {
            return Err("workflow.storage: primary digest changed".into());
        }
        Ok(())
    }
    fn expected(&self, expected: u64) -> Result<(), String> {
        if self.revision() != expected {
            return Err(format!(
                "workflow.revision: expected {expected}, current {}",
                self.revision()
            ));
        }
        Ok(())
    }
    fn validate(&self, receipt: &EventReceipt) -> Result<(), String> {
        if receipt.version != 1
            || receipt.run != self.snapshot.run
            || receipt.preceding_revision != self.revision()
            || receipt.revision
                != self
                    .revision()
                    .checked_add(1)
                    .ok_or("workflow.revision: exhausted")?
            || receipt.preceding_digest != self.snapshot.sha256
            || receipt.hash()? != receipt.sha256
        {
            return Err("workflow.storage: receipt revision/digest chain differs".into());
        }
        match (&receipt.request, &receipt.observation) {
            (Some(request), None)
                if request.expected_revision == self.revision()
                    && request.fingerprint()? == receipt.fingerprint =>
            {
                if self.receipts.iter().any(|r| {
                    r.attempt == receipt.attempt
                        || r.request
                            .as_ref()
                            .is_some_and(|r| r.request_id == request.request_id)
                }) || !self.unresolved().is_empty()
                {
                    return Err("workflow.storage: duplicate or unreconciled reservation".into());
                }
            }
            (None, Some(observation)) if observation.is_object() => {
                if !self
                    .unresolved()
                    .iter()
                    .any(|r| r.attempt == receipt.attempt && r.fingerprint == receipt.fingerprint)
                {
                    return Err("workflow.storage: resolution has no matching reservation".into());
                }
            }
            _ => return Err("workflow.storage: invalid event facts".into()),
        }
        Ok(())
    }
    fn apply(&mut self, receipt: EventReceipt) {
        self.snapshot.revision = receipt.revision;
        self.snapshot.sha256 = receipt.sha256.clone();
        self.receipts.push(receipt);
    }
    fn append(
        &mut self,
        attempt: AttemptId,
        request: Option<Mutation>,
        fingerprint: String,
        observation: Option<serde_json::Value>,
    ) -> Result<EventReceipt, String> {
        if self.receipts.len() >= 100_000 {
            return Err("workflow.storage: exceeds 100000 receipts".into());
        }
        let mut receipt = EventReceipt {
            version: 1,
            run: self.snapshot.run,
            preceding_revision: self.revision(),
            revision: self
                .revision()
                .checked_add(1)
                .ok_or("workflow.revision: exhausted")?,
            preceding_digest: self.snapshot.sha256.clone(),
            attempt,
            request,
            fingerprint,
            observation,
            sha256: String::new(),
        };
        receipt.sha256 = receipt.hash()?;
        self.validate(&receipt)?;
        let result = (|| {
            let path = self
                .directory
                .join("events")
                .join(format!("{:020}.json", receipt.revision));
            if path.try_exists().map_err(|e| e.to_string())? {
                return Err("workflow.storage: event already exists".into());
            }
            save(&path, &receipt)?;
            let snapshot = Snapshot {
                version: 1,
                run: receipt.run,
                revision: receipt.revision,
                sha256: receipt.sha256.clone(),
            };
            save(&self.directory.join("state.json"), &snapshot)
        })();
        if let Err(error) = result {
            let error = format!("workflow.storage: write uncertain; effects blocked: {error}");
            self.error = Some(error.clone());
            return Err(error);
        }
        self.apply(receipt.clone());
        Ok(receipt)
    }
}

#[cfg(test)]
mod claim_lifetime_tests {
    use super::*;
    #[test]
    fn dropping_owner_releases_claim_despite_inherited_descriptor() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("claim");
        let owner = Claim::acquire(path.clone()).unwrap();
        // fork inherits the same open file description until exec; CLOEXEC does
        // not release locks while a concurrent child is still in pre_exec.
        let inherited = owner.file.try_clone().unwrap();
        drop(owner);
        let next = Claim::acquire(path).expect("owner drop must release its lifetime claim");
        drop(inherited);
        next.verify().unwrap();
        assert!(
            Claim::acquire(next.path.clone()).is_err(),
            "a live next owner remains exclusive"
        );
    }
}
