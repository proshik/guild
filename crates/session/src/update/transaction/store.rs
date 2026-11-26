use super::{fs::Directory, *};
use crate::update::{Artifact, Origin, lock::FileLock};
use std::{
    fs as disk,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    sync::atomic::Ordering,
    time::Instant,
};
const LIMIT: usize = 64 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Active {
    schema: u32,
    id: Uuid,
    root: PathBuf,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    schema: u32,
    id: Uuid,
    nonce: Uuid,
    build_id: Uuid,
    executable: PathBuf,
    pid: u32,
}
/// Origin of the last acknowledged installation; never used without matching actual app bytes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Installed {
    /// Receipt format version.
    pub schema: u32,
    /// Acknowledged transaction identity.
    pub transaction: Uuid,
    /// Canonical installed app path.
    pub destination: PathBuf,
    /// Verified actual installed bytes.
    pub proof: Proof,
    /// Selected artifact and source provenance.
    pub identity: Identity,
}
impl Store {
    /// Construct without IO.
    pub fn new(state: PathBuf) -> Self {
        Self { state }
    }
    /// Begin after common final-copy validation. The installed app remains in place.
    pub fn begin(
        &self,
        destination: &Path,
        staged: &Path,
        identity: &Identity,
        parent_pid: u32,
        cancel: &AtomicBool,
    ) -> Result<Receipt, Failure> {
        self.begin_owned(destination, staged, identity, parent_pid, cancel)
            .map_err(|error| error.failure)
    }
    /// Begin retaining exact operation ownership even after an indeterminate IO
    /// failure. A caller may retire only this receipt, never a newly found foreign one.
    pub fn begin_owned(
        &self,
        destination: &Path,
        staged: &Path,
        identity: &Identity,
        parent_pid: u32,
        cancel: &AtomicBool,
    ) -> Result<Receipt, BeginFailure> {
        let mut owned = None;
        self.begin_with(
            Admission {
                destination,
                staged,
                identity,
                parent_pid,
            },
            cancel,
            &Native,
            &mut |_, receipt| {
                owned = Some(Box::new(receipt.clone()));
                Ok(())
            },
        )
        .map_err(|failure| BeginFailure {
            failure,
            receipt: owned,
        })
    }
    /// Read a bounded validated receipt without modifying its contents.
    pub fn load(&self, id: Uuid) -> Result<Receipt, Failure> {
        let root = self.root(false)?;
        let receipt: Receipt = read(&root.join(format!("{id}.json")), LIMIT)?;
        receipt.validate()?;
        if receipt.id != id {
            return Err(Failure::Invalid);
        }
        let parent = Directory::open(receipt.destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        receipt.check_parent(&parent)?;
        Ok(receipt)
    }
    /// Active destination transaction in this state root; a foreign state is never followed.
    pub fn pending(&self, destination: &Path) -> Result<Option<Receipt>, Failure> {
        let parent = Directory::open(destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        let active = active_path(destination);
        if !exists(&active)? {
            return Ok(None);
        }
        parent.revalidate().map_err(|_| Failure::Changed)?;
        let pointer: Active = match read(&active, 4096) {
            Ok(pointer) => pointer,
            Err(error) => {
                parent.revalidate().map_err(|_| Failure::Changed)?;
                // A helper can retire the pointer between existence and open.
                // Existing unreadable/corrupt pointers still fail closed.
                if !exists(&active)? {
                    return Ok(None);
                }
                return Err(error);
            }
        };
        if pointer.schema != 1 || pointer.id.is_nil() || pointer.root != self.root(false)? {
            return Err(Failure::Conflict);
        }
        let receipt = self.load(pointer.id)?;
        if receipt.destination
            != parent
                .path
                .join(destination.file_name().ok_or(Failure::Invalid)?)
        {
            return Err(Failure::Conflict);
        }
        Ok(Some(receipt))
    }
    /// Replace only after authenticated helper handoff and observed parent exit.
    pub fn execute(&self, id: Uuid, cancel: &AtomicBool) -> Result<Receipt, Failure> {
        self.execute_with(id, cancel, &Native, Duration::from_secs(60), &mut |_, _| {
            Ok(())
        })
    }
    /// Retire only an uncommitted preparation after revoking its helper/token.
    /// This is safe while the old GUI remains live; it never replaces that app.
    pub fn abort_prepared(&self, id: Uuid, cancel: &AtomicBool) -> Result<Receipt, Failure> {
        self.abort_prepared_with(id, cancel, &Native)
    }
    pub(super) fn abort_prepared_with(
        &self,
        id: Uuid,
        cancel: &AtomicBool,
        platform: &impl Platform,
    ) -> Result<Receipt, Failure> {
        let mut receipt = self.load(id)?;
        let parent = Directory::open(receipt.destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        let _lock = lock(&parent, &receipt.destination)?;
        self.authorize(&receipt, &parent)?;
        if !matches!(receipt.phase, Phase::Preparing | Phase::Prepared)
            || platform.proof(&receipt.destination, cancel)? != receipt.previous
            || exists(&receipt.previous_path())?
            || exists(&receipt.failed_path())?
        {
            return Err(Failure::Conflict);
        }
        check_optional(&receipt.prepared_path(), &receipt.next, platform, cancel)?;
        remove_owned(
            &parent,
            &receipt.prepared_path(),
            &receipt.next,
            platform,
            cancel,
        )?;
        self.phase(&mut receipt, Phase::Aborted, None)?;
        self.clear_active(&receipt, &parent)?;
        Ok(receipt)
    }
    /// First-layout acknowledgement from this exact loaded build and actual executable.
    pub fn acknowledge(
        &self,
        id: Uuid,
        nonce: Uuid,
        loaded_build: Uuid,
        cancel: &AtomicBool,
    ) -> Result<(), Failure> {
        let executable = std::env::current_exe().map_err(|_| Failure::InvalidAck)?;
        self.acknowledge_with(id, nonce, loaded_build, &executable, cancel, &Native)
    }
    /// Manual recovery assumes the app being replaced has exited. No process is killed.
    pub fn recover(
        &self,
        id: Uuid,
        choice: Recovery,
        cancel: &AtomicBool,
    ) -> Result<Receipt, Failure> {
        self.recover_with(id, choice, cancel, &Native)
    }
    /// Last acknowledged provenance; unknown/corrupt data is preserved and rejected.
    pub fn installed(&self) -> Result<Option<Installed>, Failure> {
        let root = match self.root(false) {
            Ok(root) => root,
            Err(Failure::Invalid) if !self.state.join("update/transactions").exists() => {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };
        let path = root.join("installed.json");
        if !exists(&path)? {
            return Ok(None);
        }
        let installed: Installed = read(&path, LIMIT)?;
        if installed.schema != 1
            || installed.transaction.is_nil()
            || !installed.destination.is_absolute()
        {
            return Err(Failure::Invalid);
        }
        installed.proof.validate()?;
        installed
            .identity
            .validate()
            .map_err(|_| Failure::Invalid)?;
        if !matches_selection(&installed.proof, &installed.identity) {
            return Err(Failure::Invalid);
        }
        Ok(Some(installed))
    }
    pub(super) fn begin_with(
        &self,
        admission: Admission<'_>,
        cancel: &AtomicBool,
        platform: &impl Platform,
        hook: &mut impl FnMut(Boundary, &Receipt) -> Result<(), Failure>,
    ) -> Result<Receipt, Failure> {
        let Admission {
            destination,
            staged,
            identity,
            parent_pid,
        } = admission;
        check_cancel(cancel)?;
        identity.validate().map_err(|_| Failure::Invalid)?;
        if parent_pid == 0 || !destination.is_absolute() || !staged.is_absolute() {
            return Err(Failure::Invalid);
        }
        let metadata = disk::symlink_metadata(destination).map_err(|_| Failure::Invalid)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(Failure::Invalid);
        }
        let destination = destination.canonicalize().map_err(|_| Failure::Invalid)?;
        if !destination
            .file_name()
            .is_some_and(|s| s.to_string_lossy().ends_with(".app"))
        {
            return Err(Failure::Invalid);
        }
        let parent = Directory::open(destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        let _lock = lock(&parent, &destination)?;
        if exists(&active_path(&destination))? {
            return Err(Failure::Conflict);
        }
        let previous = platform.proof(&destination, cancel)?;
        let next = platform.proof(staged, cancel)?;
        previous.validate()?;
        next.validate()?;
        if !matches_selection(&next, identity) {
            return Err(Failure::Changed);
        }
        let root = self.root(true)?;
        // An older updater must not overwrite a future provenance receipt.
        if exists(&root.join("installed.json"))? {
            self.installed()?;
        }
        let mut receipt = Receipt {
            schema: 1,
            id: Uuid::new_v4(),
            nonce: Uuid::new_v4(),
            arm_nonce: Uuid::new_v4(),
            parent_pid,
            destination,
            parent: parent.id,
            previous,
            next,
            selection: identity.clone(),
            phase: Phase::Preparing,
            failure: None,
        };
        for path in [
            receipt.prepared_path(),
            receipt.previous_path(),
            receipt.failed_path(),
        ] {
            if exists(&path)? {
                return Err(Failure::Conflict);
            }
        }
        bundle::write_new(
            &root.join(format!("{}.json", receipt.id)),
            &encode(&receipt)?,
            true,
        )
        .map_err(|_| Failure::Storage)?;
        hook(Boundary::JournalWritten, &receipt)?;
        parent.revalidate().map_err(|_| Failure::Changed)?;
        bundle::write_new(
            &active_path(&receipt.destination),
            &encode(&Active {
                schema: 1,
                id: receipt.id,
                root,
            })?,
            true,
        )
        .map_err(|_| Failure::Conflict)?;
        hook(Boundary::ActiveWritten, &receipt)?;
        check_cancel(cancel)?;
        parent
            .adopt(
                staged,
                receipt
                    .prepared_path()
                    .file_name()
                    .ok_or(Failure::Invalid)?,
            )
            .map_err(|_| Failure::Storage)?;
        parent.sync().map_err(|_| Failure::Storage)?;
        hook(Boundary::PayloadRetained, &receipt)?;
        if platform.proof(&receipt.prepared_path(), cancel)? != receipt.next {
            return Err(Failure::Changed);
        }
        sync_tree(&receipt.prepared_path(), cancel)?;
        self.phase(&mut receipt, Phase::Prepared, None)?;
        hook(Boundary::Prepared, &receipt)?;
        Ok(receipt)
    }
    pub(super) fn execute_with(
        &self,
        id: Uuid,
        cancel: &AtomicBool,
        platform: &impl Platform,
        budget: Duration,
        hook: &mut impl FnMut(Boundary, &Receipt) -> Result<(), Failure>,
    ) -> Result<Receipt, Failure> {
        let mut receipt = self.load(id)?;
        let parent = Directory::open(receipt.destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        let _lock = lock(&parent, &receipt.destination)?;
        self.authorize(&receipt, &parent)?;
        let mut launched = false;
        let result = (|| {
            check_cancel(cancel)?;
            if receipt.phase != Phase::Prepared {
                return Err(Failure::Conflict);
            }
            if exists(&receipt.previous_path())? || exists(&receipt.failed_path())? {
                return Err(Failure::Conflict);
            }
            if platform.proof(&receipt.destination, cancel)? != receipt.previous
                || platform.proof(&receipt.prepared_path(), cancel)? != receipt.next
            {
                return Err(Failure::Changed);
            }
            move_within(&parent, &receipt.destination, &receipt.previous_path())?;
            hook(Boundary::OldMoved, &receipt)?;
            if platform.proof(&receipt.previous_path(), cancel)? != receipt.previous {
                return Err(Failure::Changed);
            }
            self.phase(&mut receipt, Phase::OldRetained, None)?;
            hook(Boundary::OldRetained, &receipt)?;
            check_cancel(cancel)?;
            move_within(&parent, &receipt.prepared_path(), &receipt.destination)?;
            hook(Boundary::NewMoved, &receipt)?;
            if platform.proof(&receipt.destination, cancel)? != receipt.next {
                return Err(Failure::Changed);
            }
            self.phase(&mut receipt, Phase::NewInPlace, None)?;
            hook(Boundary::NewInPlace, &receipt)?;
            let deadline = Instant::now() + budget;
            self.phase(&mut receipt, Phase::LaunchRequested, None)?;
            hook(Boundary::LaunchRequested, &receipt)?;
            platform.launch(&receipt, false, &self.state)?;
            launched = true;
            loop {
                check_cancel(cancel)?;
                if let Some(ack) = self.ack(&receipt)? {
                    if !valid_ack(&ack, &receipt)
                        || platform.proof(&receipt.destination, cancel)? != receipt.next
                    {
                        return Err(Failure::InvalidAck);
                    }
                    self.phase(&mut receipt, Phase::Acknowledged, None)?;
                    hook(Boundary::Acknowledged, &receipt)?;
                    let installed = Installed {
                        schema: 1,
                        transaction: receipt.id,
                        destination: receipt.destination.clone(),
                        proof: receipt.next.clone(),
                        identity: receipt.selection.clone(),
                    };
                    bundle::publish_json(&self.root(false)?.join("installed.json"), &installed)
                        .map_err(|_| Failure::Storage)?;
                    remove_owned(
                        &parent,
                        &receipt.previous_path(),
                        &receipt.previous,
                        platform,
                        cancel,
                    )?;
                    self.clear_active(&receipt, &parent)?;
                    return Ok(receipt.clone());
                }
                if Instant::now() >= deadline {
                    return Err(Failure::MissingAck);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })();
        if let Err(error) = result {
            receipt.failure = Some(error);
            if error == Failure::Interrupted {
                return Err(error);
            }
            // Once launch succeeded, every wait/read/cancellation failure keeps
            // the last usable backup: the new app may already be running.
            // Only failures before successful launch permit automatic restoration.
            if !launched
                && receipt.phase != Phase::Acknowledged
                && !matches!(error, Failure::MissingAck | Failure::InvalidAck)
                && let Ok(restored) = self.restore_locked(&mut receipt, &parent, platform, cancel)
            {
                let _ = platform.launch(&restored, true, &self.state);
                return Err(error);
            }
            // Cleanup failure must not erase verified startup acknowledgement.
            // The previous slot may already be removed; recovery must keep current.
            let phase = if receipt.phase == Phase::Acknowledged {
                Phase::Acknowledged
            } else {
                Phase::RecoveryRequired
            };
            let _ = self.phase(&mut receipt, phase, Some(error));
            return Err(error);
        }
        result
    }
    pub(super) fn recover_with(
        &self,
        id: Uuid,
        choice: Recovery,
        cancel: &AtomicBool,
        platform: &impl Platform,
    ) -> Result<Receipt, Failure> {
        let mut receipt = self.load(id)?;
        let parent = Directory::open(receipt.destination.parent().ok_or(Failure::Invalid)?)
            .map_err(|_| Failure::Changed)?;
        let _lock = lock(&parent, &receipt.destination)?;
        self.authorize(&receipt, &parent)?;
        check_cancel(cancel)?;
        match choice {
            Recovery::RestorePrevious => {
                self.restore_locked(&mut receipt, &parent, platform, cancel)
            }
            Recovery::KeepCurrent => {
                if platform.proof(&receipt.destination, cancel)? != receipt.next {
                    return Err(Failure::Changed);
                }
                check_optional(
                    &receipt.previous_path(),
                    &receipt.previous,
                    platform,
                    cancel,
                )?;
                check_optional(&receipt.prepared_path(), &receipt.next, platform, cancel)?;
                check_optional(&receipt.failed_path(), &receipt.next, platform, cancel)?;
                let installed = Installed {
                    schema: 1,
                    transaction: receipt.id,
                    destination: receipt.destination.clone(),
                    proof: receipt.next.clone(),
                    identity: receipt.selection.clone(),
                };
                bundle::publish_json(&self.root(false)?.join("installed.json"), &installed)
                    .map_err(|_| Failure::Storage)?;
                self.phase(&mut receipt, Phase::Acknowledged, None)?;
                for (path, proof) in [
                    (receipt.previous_path(), &receipt.previous),
                    (receipt.prepared_path(), &receipt.next),
                    (receipt.failed_path(), &receipt.next),
                ] {
                    remove_owned(&parent, &path, proof, platform, cancel)?;
                }
                self.clear_active(&receipt, &parent)?;
                Ok(receipt)
            }
        }
    }
    fn restore_locked(
        &self,
        receipt: &mut Receipt,
        parent: &Directory,
        platform: &impl Platform,
        cancel: &AtomicBool,
    ) -> Result<Receipt, Failure> {
        parent.revalidate().map_err(|_| Failure::Changed)?;
        // Validate every existing derived slot before moving or deleting anything.
        check_optional(
            &receipt.previous_path(),
            &receipt.previous,
            platform,
            cancel,
        )?;
        check_optional(&receipt.prepared_path(), &receipt.next, platform, cancel)?;
        check_optional(&receipt.failed_path(), &receipt.next, platform, cancel)?;
        let destination = if exists(&receipt.destination)? {
            Some(platform.proof(&receipt.destination, cancel)?)
        } else {
            None
        };
        match destination {
            Some(proof)
                if proof == receipt.previous
                    && (proof != receipt.next || !exists(&receipt.previous_path())?) => {}
            Some(proof) if proof == receipt.next => {
                if !exists(&receipt.previous_path())? || exists(&receipt.failed_path())? {
                    return Err(Failure::Conflict);
                }
                move_within(parent, &receipt.destination, &receipt.failed_path())?;
                move_within(parent, &receipt.previous_path(), &receipt.destination)?;
            }
            None if exists(&receipt.previous_path())? => {
                move_within(parent, &receipt.previous_path(), &receipt.destination)?;
            }
            _ => return Err(Failure::Changed),
        }
        if platform.proof(&receipt.destination, cancel)? != receipt.previous {
            return Err(Failure::Changed);
        }
        for path in [receipt.prepared_path(), receipt.failed_path()] {
            remove_owned(parent, &path, &receipt.next, platform, cancel)?;
        }
        // A duplicate old backup is kept rather than guessed safe to erase.
        if exists(&receipt.previous_path())? {
            return Err(Failure::Conflict);
        }
        self.phase(receipt, Phase::Restored, receipt.failure)?;
        self.clear_active(receipt, parent)?;
        Ok(receipt.clone())
    }
    pub(super) fn acknowledge_with(
        &self,
        id: Uuid,
        nonce: Uuid,
        loaded_build: Uuid,
        executable: &Path,
        cancel: &AtomicBool,
        platform: &impl Platform,
    ) -> Result<(), Failure> {
        let receipt = self.load(id)?;
        check_cancel(cancel)?;
        let actual = executable.canonicalize().map_err(|_| Failure::InvalidAck)?;
        let expected = receipt.destination.join("Contents/MacOS/guild-desktop");
        if receipt.phase != Phase::LaunchRequested
            || nonce != receipt.nonce
            || Some(loaded_build) != receipt.next.build_id
            || actual != expected
            || std::process::id() == receipt.parent_pid
            || platform.proof(&receipt.destination, cancel)? != receipt.next
        {
            return Err(Failure::InvalidAck);
        }
        let ack = Ack {
            schema: 1,
            id,
            nonce,
            build_id: loaded_build,
            executable: actual,
            pid: std::process::id(),
        };
        bundle::publish_json(&self.ack_path(id)?, &ack).map_err(|_| Failure::Storage)
    }
    fn ack(&self, receipt: &Receipt) -> Result<Option<Ack>, Failure> {
        let path = self.ack_path(receipt.id)?;
        if exists(&path)? {
            read(&path, 4096).map(Some).map_err(|_| Failure::InvalidAck)
        } else {
            Ok(None)
        }
    }
    fn ack_path(&self, id: Uuid) -> Result<PathBuf, Failure> {
        Ok(self.root(false)?.join(format!("{id}.ack.json")))
    }
    fn phase(
        &self,
        receipt: &mut Receipt,
        phase: Phase,
        error: Option<Failure>,
    ) -> Result<(), Failure> {
        receipt.phase = phase;
        receipt.failure = error;
        bundle::publish_json(
            &self.root(false)?.join(format!("{}.json", receipt.id)),
            receipt,
        )
        .map_err(|_| Failure::Storage)
    }
    fn authorize(&self, receipt: &Receipt, parent: &Directory) -> Result<(), Failure> {
        receipt.check_parent(parent)?;
        let active: Active = read(&active_path(&receipt.destination), 4096)?;
        if active.schema != 1 || active.id != receipt.id || active.root != self.root(false)? {
            return Err(Failure::Conflict);
        }
        Ok(())
    }
    fn clear_active(&self, receipt: &Receipt, parent: &Directory) -> Result<(), Failure> {
        self.authorize(receipt, parent)?;
        disk::remove_file(active_path(&receipt.destination)).map_err(|_| Failure::Storage)?;
        parent.sync().map_err(|_| Failure::Storage)
    }
    fn root(&self, create: bool) -> Result<PathBuf, Failure> {
        if !self.state.is_absolute() {
            return Err(Failure::Invalid);
        }
        let path = self.state.join("update/transactions");
        if create {
            disk::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(&path)
                .map_err(|_| Failure::Storage)?;
        }
        let metadata = disk::symlink_metadata(&path).map_err(|_| Failure::Invalid)?;
        // SAFETY: geteuid has no preconditions and returns the current uid.
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
        {
            return Err(Failure::Invalid);
        }
        path.canonicalize().map_err(|_| Failure::Storage)
    }
}
impl Proof {
    pub(super) fn validate(&self) -> Result<(), Failure> {
        if self.commit.len() != 40
            || !self.commit.bytes().all(|c| c.is_ascii_hexdigit())
            || self.seal.len() != 64
            || !self.seal.bytes().all(|c| c.is_ascii_hexdigit())
            || self.build_id.is_some_and(|id| id.is_nil())
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
impl Receipt {
    pub(super) fn validate(&self) -> Result<(), Failure> {
        if self.schema != 1
            || self.id.is_nil()
            || self.nonce.is_nil()
            || self.arm_nonce.is_nil()
            || self.arm_nonce == self.nonce
            || self.parent_pid == 0
            || !self.destination.is_absolute()
            || !self
                .destination
                .file_name()
                .is_some_and(|s| s.to_str().is_some_and(|s| s.ends_with(".app")))
        {
            return Err(Failure::Invalid);
        }
        self.previous.validate()?;
        self.next.validate()?;
        self.selection.validate().map_err(|_| Failure::Invalid)?;
        if !matches_selection(&self.next, &self.selection) {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
    fn check_parent(&self, parent: &Directory) -> Result<(), Failure> {
        if self.destination.parent() != Some(parent.path.as_path()) || parent.id != self.parent {
            return Err(Failure::Changed);
        }
        parent.revalidate().map_err(|_| Failure::Changed)
    }
}
fn matches_selection(proof: &Proof, identity: &Identity) -> bool {
    let artifact = match &identity.artifact {
        Artifact::Build(id) => Some(*id) == proof.build_id,
        Artifact::LegacySeal(seal) => proof.build_id.is_none() && seal == &proof.seal,
    };
    artifact
        && match &identity.origin {
            Origin::Local { seal, .. } => seal == &proof.seal,
            Origin::Remote { .. } => true,
        }
}
fn valid_ack(ack: &Ack, receipt: &Receipt) -> bool {
    ack.schema == 1
        && ack.id == receipt.id
        && ack.nonce == receipt.nonce
        && Some(ack.build_id) == receipt.next.build_id
        && ack.executable == receipt.destination.join("Contents/MacOS/guild-desktop")
        && ack.pid > 0
        && ack.pid != receipt.parent_pid
}
fn active_path(destination: &Path) -> PathBuf {
    destination.with_file_name(format!(
        ".{}.guild-update.json",
        destination
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
    ))
}
fn lock(parent: &Directory, destination: &Path) -> Result<FileLock, Failure> {
    parent.revalidate().map_err(|_| Failure::Changed)?;
    let name = format!(
        ".{}.guild-update.lock",
        destination
            .file_name()
            .ok_or(Failure::Invalid)?
            .to_string_lossy()
    );
    FileLock::acquire(&parent.path, &name, Duration::from_secs(5)).map_err(|_| Failure::Conflict)
}
fn move_within(parent: &Directory, source: &Path, destination: &Path) -> Result<(), Failure> {
    parent
        .rename(
            source.file_name().ok_or(Failure::Invalid)?,
            destination.file_name().ok_or(Failure::Invalid)?,
        )
        .map_err(|_| Failure::Storage)?;
    parent.sync().map_err(|_| Failure::Storage)
}
fn check_optional(
    path: &Path,
    proof: &Proof,
    platform: &impl Platform,
    cancel: &AtomicBool,
) -> Result<(), Failure> {
    if exists(path)? && &platform.proof(path, cancel)? != proof {
        return Err(Failure::Changed);
    }
    Ok(())
}
fn remove_owned(
    parent: &Directory,
    path: &Path,
    proof: &Proof,
    platform: &impl Platform,
    cancel: &AtomicBool,
) -> Result<(), Failure> {
    if !exists(path)? {
        return Ok(());
    }
    parent.revalidate().map_err(|_| Failure::Changed)?;
    if &platform.proof(path, cancel)? != proof {
        return Err(Failure::Changed);
    }
    disk::remove_dir_all(path).map_err(|_| Failure::Storage)?;
    parent.sync().map_err(|_| Failure::Storage)
}
fn sync_tree(app: &Path, cancel: &AtomicBool) -> Result<(), Failure> {
    bundle::expanded_size(app, cancel).map_err(|_| Failure::Changed)?;
    let mut pending = vec![app.to_path_buf()];
    let mut directories = Vec::new();
    while let Some(path) = pending.pop() {
        check_cancel(cancel)?;
        for entry in disk::read_dir(&path).map_err(|_| Failure::Storage)? {
            let entry = entry.map_err(|_| Failure::Storage)?;
            let kind = entry.file_type().map_err(|_| Failure::Storage)?;
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file() {
                bundle::open_regular(&entry.path())
                    .and_then(|file| {
                        file.sync_all()
                            .map_err(|_| "update file sync failed".into())
                    })
                    .map_err(|_| Failure::Storage)?;
            }
        }
        directories.push(path);
    }
    for path in directories.into_iter().rev() {
        Directory::open(&path)
            .and_then(|dir| dir.sync())
            .map_err(|_| Failure::Storage)?;
    }
    Ok(())
}
fn read<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> Result<T, Failure> {
    serde_json::from_slice(&bundle::read_small(path, limit).map_err(|_| Failure::Invalid)?)
        .map_err(|_| Failure::Invalid)
}
fn encode(value: &impl Serialize) -> Result<Vec<u8>, Failure> {
    serde_json::to_vec_pretty(value).map_err(|_| Failure::Storage)
}
fn exists(path: &Path) -> Result<bool, Failure> {
    match disk::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Failure::Storage),
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), Failure> {
    if cancel.load(Ordering::Acquire) {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}
