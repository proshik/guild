//! Complete source/channel snapshots; no receipt grants installation authority.
//! All methods perform IO off-thread. Fresh discovery and Gate still authorize use.
use super::{Acquired, Candidate, Error, Progress, Provider, check_cancel};
use crate::update::control::Permit;
use guild_session::update::{bundle, lock::FileLock};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    folder: Uuid,
    release_id: u64,
    archive_id: u64,
    signature_id: u64,
    build_id: Uuid,
    sha256: String,
}
impl Receipt {
    fn matches(&self, candidate: &Candidate) -> bool {
        self.schema == 1
            && !self.folder.is_nil()
            && self.release_id == candidate.release_id
            && self.archive_id == candidate.archive_id
            && self.signature_id == candidate.signature_id
            && self.build_id == candidate.manifest.build.build_id
            && self.sha256 == candidate.manifest.sha256
    }
    fn path(&self, scope: &Path) -> PathBuf {
        scope.join(format!("payload-{}", self.folder))
    }
}
/// A prepared owned snapshot, invisible to readers until current-ticket publication.
/// Keep its drop/cleanup and publication off the window thread.
pub struct CacheDraft {
    directory: tempfile::TempDir,
    scope: PathBuf,
    receipt: Receipt,
    candidate: Candidate,
    _lock: FileLock,
}
impl Provider {
    /// Prepare immutable archive bytes without publishing a selected candidate.
    pub fn prepare_cache(
        &self,
        acquired: &Acquired,
        root: &Path,
        cancel: &AtomicBool,
    ) -> Result<CacheDraft, Error> {
        self.authenticate(&acquired.candidate)?;
        check_cancel(cancel)?;
        let scope = self
            .scope(&acquired.candidate, root, true)?
            .ok_or(Error::Storage)?;
        let lock = FileLock::acquire(&scope, ".cache.lock", Duration::from_secs(5))
            .map_err(|_| Error::Storage)?;
        bundle::ensure_space(&scope, acquired.candidate.manifest.bytes)
            .map_err(|_| Error::Space)?;
        let directory = tempfile::Builder::new()
            .prefix(".pending-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&scope)
            .map_err(|_| Error::Storage)?;
        let archive = directory.path().join("archive.zip");
        bundle::copy_archive(
            &acquired.work_dir().join("archive.zip"),
            &archive,
            acquired.candidate.manifest.bytes,
            cancel,
        )
        .map_err(|_| cancelled_or(cancel, Error::Verification))?;
        let signature = bundle::read_small(&acquired.work_dir().join("archive.sig"), 4096)
            .map_err(|_| Error::Verification)?;
        self.verify_payload(&acquired.candidate, &archive, &signature, cancel)?;
        for (name, bytes) in [
            (
                "guild-update.json",
                acquired.candidate.manifest_bytes.as_slice(),
            ),
            (
                "guild-update.json.sig",
                acquired.candidate.manifest_signature.as_slice(),
            ),
            ("archive.sig", signature.as_slice()),
        ] {
            bundle::write_new(&directory.path().join(name), bytes, true)
                .map_err(|_| Error::Storage)?;
        }
        let receipt = Receipt {
            schema: 1,
            folder: Uuid::new_v4(),
            release_id: acquired.candidate.release_id,
            archive_id: acquired.candidate.archive_id,
            signature_id: acquired.candidate.signature_id,
            build_id: acquired.candidate.manifest.build.build_id,
            sha256: acquired.candidate.manifest.sha256.clone(),
        };
        bundle::write_new(
            &directory.path().join("complete.json"),
            &serde_json::to_vec(&receipt).map_err(|_| Error::Storage)?,
            true,
        )
        .map_err(|_| Error::Storage)?;
        check_cancel(cancel)?;
        Ok(CacheDraft {
            directory,
            scope,
            receipt,
            candidate: acquired.candidate.clone(),
            _lock: lock,
        })
    }
    /// Restore only against a fresh successful chosen-channel discovery result.
    /// Authentication, digest, extraction and native validation run again. A cache
    /// cannot prove availability and must never be used after failed discovery.
    pub fn restore(
        &self,
        candidate: &Candidate,
        root: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<Option<Acquired>, Error> {
        self.authenticate(candidate)?;
        check_cancel(cancel)?;
        let Some(scope) = self.scope(candidate, root, false)? else {
            return Ok(None);
        };
        let _lock = FileLock::acquire(&scope, ".cache.lock", Duration::from_secs(5))
            .map_err(|_| Error::Storage)?;
        let Some(receipt) = read_receipt(&scope.join("ready.json"))? else {
            return Ok(None);
        };
        if !receipt.matches(candidate) {
            return Ok(None);
        }
        let saved = receipt.path(&scope);
        if !private_directory(&saved, false)? {
            return Err(Error::Verification);
        }
        if read_receipt(&saved.join("complete.json"))?.as_ref() != Some(&receipt) {
            return Err(Error::Verification);
        }
        for (name, expected) in [
            ("guild-update.json", candidate.manifest_bytes.as_slice()),
            (
                "guild-update.json.sig",
                candidate.manifest_signature.as_slice(),
            ),
        ] {
            let bytes = bundle::read_small(&saved.join(name), guild_session::update::MAX_MANIFEST)
                .map_err(|_| Error::Verification)?;
            if bytes != expected {
                return Ok(None);
            }
        }
        let signature = bundle::read_small(&saved.join("archive.sig"), 4096)
            .map_err(|_| Error::Verification)?;
        bundle::ensure_space(&scope, candidate.manifest.bytes).map_err(|_| Error::Space)?;
        let directory = tempfile::Builder::new()
            .prefix(".restoring-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(&scope)
            .map_err(|_| Error::Storage)?;
        bundle::copy_archive(
            &saved.join("archive.zip"),
            &directory.path().join("archive.zip"),
            candidate.manifest.bytes,
            cancel,
        )
        .map_err(|_| cancelled_or(cancel, Error::Verification))?;
        // No cached extracted app is trusted or reused. Only the authenticated
        // archive is re-extracted into this worker's fresh owned directory.
        progress(Progress::Verifying);
        self.finish_download(candidate, directory, signature, cancel)
            .map(Some)
    }
    fn scope(
        &self,
        candidate: &Candidate,
        root: &Path,
        create: bool,
    ) -> Result<Option<PathBuf>, Error> {
        let (owner, repo) = self.repository.split_once('/').ok_or(Error::Invalid)?;
        let channel = candidate.manifest.build.channel.ok_or(Error::Invalid)?;
        let mut path = root.to_path_buf();
        if !private_directory(&path, create)? {
            return Ok(None);
        }
        for part in ["remote", owner, repo, channel.name()] {
            path.push(part);
            if !private_directory(&path, create)? {
                return Ok(None);
            }
        }
        Ok(Some(path))
    }
}
impl CacheDraft {
    /// Publish after the controller accepted this exact artifact, using revocable
    /// authority without borrowing window state. Check again after filesystem IO;
    /// a cancelled publication loses its owned files and can never authorize install.
    pub fn publish(self, permit: &Permit) -> Result<(), Error> {
        let identity = self.candidate.identity().ok_or(Error::Invalid)?;
        if !permit.pins(&identity) {
            return Err(Error::Cancelled);
        }
        // A damaged hint must not prevent an explicit successful redownload
        // from repairing the cache. Unprovable old ownership is never deleted.
        let old = read_receipt(&self.scope.join("ready.json")).ok().flatten();
        let destination = self.receipt.path(&self.scope);
        // Keep rollback ownership through rename and receipt fsync. Publication
        // failure may leave a stale receipt; readers reject its missing directory.
        fs::rename(self.directory.path(), &destination).map_err(|_| Error::Storage)?;
        let mut rollback = OwnedPath {
            path: destination,
            keep: false,
        };
        if !permit.pins(&identity) {
            return Err(Error::Cancelled);
        }
        bundle::publish_json(&self.scope.join("ready.json"), &self.receipt)
            .map_err(|_| Error::Storage)?;
        if !permit.pins(&identity) {
            return Err(Error::Cancelled);
        }
        rollback.keep = true;
        if let Some(old) = old.filter(|old| {
            old.schema == 1 && !old.folder.is_nil() && old.folder != self.receipt.folder
        }) {
            let path = old.path(&self.scope);
            // Never delete a directory solely because an index named it.
            if private_directory(&path, false).unwrap_or(false)
                && read_receipt(&path.join("complete.json"))
                    .ok()
                    .flatten()
                    .as_ref()
                    == Some(&old)
            {
                let _ = fs::remove_dir_all(path);
            }
        }
        Ok(())
    }
}
struct OwnedPath {
    path: PathBuf,
    keep: bool,
}
impl Drop for OwnedPath {
    fn drop(&mut self) {
        if !self.keep {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
fn read_receipt(path: &Path) -> Result<Option<Receipt>, Error> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Storage),
        Ok(_) => (),
    }
    let bytes = bundle::read_small(path, 4096).map_err(|_| Error::Verification)?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| Error::Verification)
}
fn private_directory(path: &Path, create: bool) -> Result<bool, Error> {
    if create {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|_| Error::Storage)?;
    }
    let metadata = match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(Error::Storage),
        Ok(m) => m,
    };
    // SAFETY: geteuid has no preconditions and returns the current process uid.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != uid
        || metadata.mode() & 0o077 != 0
    {
        return Err(Error::Storage);
    }
    Ok(true)
}
fn cancelled_or(cancel: &AtomicBool, error: Error) -> Error {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        Error::Cancelled
    } else {
        error
    }
}
