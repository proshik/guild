//! Explicit bounded payload acquisition; the controller alone publishes accepted results.
use super::{Acquired, Candidate, Error, Progress, Provider, Request, check_cancel};
use guild_session::update::{self, bundle};
use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
impl Provider {
    /// Authenticate, download and validate into an owned temporary directory.
    /// Every result/progress event still needs the caller's current operation ticket.
    pub fn download(
        &self,
        candidate: &Candidate,
        cache_root: &Path,
        cancel: &AtomicBool,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<Acquired, Error> {
        check_cancel(cancel)?;
        let manifest = self.authenticate(candidate)?;
        let deadline = Instant::now() + Duration::from_secs(20 * 60);
        let signature = self.asset(candidate.signature_id, 4096, cancel, deadline)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(cache_root)
            .map_err(|_| Error::Storage)?;
        bundle::ensure_space(cache_root, manifest.bytes).map_err(|_| Error::Space)?;
        let directory = tempfile::Builder::new()
            .prefix(".guild-download-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(cache_root)
            .map_err(|_| Error::Storage)?;
        let path = directory.path().join("archive.zip");
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .mode(0o600)
            .open(&path)
            .map_err(|_| Error::Storage)?;
        let endpoint = format!(
            "/repos/{}/releases/assets/{}",
            self.repository, candidate.archive_id
        );
        let request = Request {
            path: &endpoint,
            limit: manifest.bytes,
            asset: true,
            timeout: deadline
                .checked_duration_since(Instant::now())
                .ok_or(Error::Network)?,
        };
        progress(Progress::Downloading {
            bytes: 0,
            total: manifest.bytes,
        });
        let actual = self.fetch(&request, cancel, &mut file, &mut |bytes| {
            progress(Progress::Downloading {
                bytes,
                total: manifest.bytes,
            })
        })?;
        if actual != manifest.bytes {
            return Err(Error::Verification);
        }
        file.sync_all().map_err(|_| Error::Storage)?;
        drop(file);
        check_cancel(cancel)?;
        progress(Progress::Verifying);
        self.finish_download(candidate, directory, signature, cancel)
    }
    pub(super) fn authenticate(&self, candidate: &Candidate) -> Result<update::Manifest, Error> {
        let manifest = update::verify_manifest(
            &candidate.manifest_bytes,
            &candidate.manifest_signature,
            &self.keys,
        )
        .map_err(|_| Error::Verification)?;
        if manifest != candidate.manifest
            || manifest.repository != self.repository
            || candidate.archive_id == 0
            || candidate.signature_id == 0
            || candidate.release_id == 0
        {
            return Err(Error::Invalid);
        }
        Ok(manifest)
    }
    pub(super) fn verify_payload(
        &self,
        candidate: &Candidate,
        path: &Path,
        signature: &[u8],
        cancel: &AtomicBool,
    ) -> Result<(), Error> {
        let manifest = self.authenticate(candidate)?;
        let reader = Cancellable {
            input: bundle::open_regular(path).map_err(|_| Error::Storage)?,
            cancel,
        };
        let actual = update::verify_archive(reader, signature, &self.keys).map_err(|_| {
            if cancel.load(Ordering::Acquire) {
                Error::Cancelled
            } else {
                Error::Verification
            }
        })?;
        if actual.0 != manifest.bytes || !actual.1.eq_ignore_ascii_case(&manifest.sha256) {
            return Err(Error::Verification);
        }
        check_cancel(cancel)?;
        Ok(())
    }
    pub(super) fn finish_download(
        &self,
        candidate: &Candidate,
        directory: tempfile::TempDir,
        signature: Vec<u8>,
        cancel: &AtomicBool,
    ) -> Result<Acquired, Error> {
        let manifest = self.authenticate(candidate)?;
        let path = directory.path().join("archive.zip");
        self.verify_payload(candidate, &path, &signature, cancel)?;
        let app = update::archive::extract(
            &path,
            &directory.path().join("extracted"),
            &manifest,
            cancel,
        )
        .map_err(|error| match error {
            update::archive::Failure::Invalid => Error::Verification,
            update::archive::Failure::Space => Error::Space,
            update::archive::Failure::Io => Error::Storage,
            update::archive::Failure::Cancelled => Error::Cancelled,
        })?;
        update::publisher::keys(&app.join("Contents/Resources/update-keys.json"))
            .map_err(|_| Error::Verification)?;
        check_cancel(cancel)?;
        // Only validated bytes survive in this owned result. There is no active cache
        // publication here: a cancelled/stale consumer drops the entire directory.
        for (name, bytes) in [
            ("guild-update.json", candidate.manifest_bytes.as_slice()),
            (
                "guild-update.json.sig",
                candidate.manifest_signature.as_slice(),
            ),
            ("archive.sig", signature.as_slice()),
        ] {
            bundle::write_new(&directory.path().join(name), bytes, true)
                .map_err(|_| Error::Storage)?;
        }
        check_cancel(cancel)?;
        let seal = bundle::seal(&app, cancel).map_err(|_| {
            if cancel.load(Ordering::Acquire) {
                Error::Cancelled
            } else {
                Error::Verification
            }
        })?;
        Ok(Acquired {
            seal,
            directory,
            bundle: app,
            candidate: candidate.clone(),
        })
    }
}
struct Cancellable<'a, R> {
    input: R,
    cancel: &'a AtomicBool,
}
impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(std::io::Error::other("update cancelled"));
        }
        self.input.read(buffer)
    }
}
