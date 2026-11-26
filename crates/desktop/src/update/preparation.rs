//! One immutable selected artifact and final-copy validation for both sources.
//! Keep ownership, blocking IO and cleanup on the updater's background worker.
use super::{
    Build,
    control::{Artifact, Identity, Origin},
    remote::Acquired,
};
use guild_session::update::{BuildInfo, Channel, Preferences, Source, bundle};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
/// Sanitized common failure; paths and tool diagnostics stay private.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    /// Not a completed valid compatible Guild payload.
    Invalid,
    /// The selected artifact changed before its final copy could be validated.
    Changed,
    /// Work was cancelled.
    Cancelled,
    /// Insufficient available space for the preparation copy.
    Space,
    /// An owned copy/directory operation failed.
    Storage,
}
#[derive(Clone)]
struct Snapshot {
    info: Option<BuildInfo>,
    build: Build,
    identity: Identity,
}
/// Selected bytes and proof; dispose only on the updater worker.
pub struct Candidate {
    source: PathBuf,
    snapshot: Snapshot,
    _download: Option<Acquired>,
}
/// Final validated owned copy. No replacement has started yet.
pub struct Prepared {
    directory: tempfile::TempDir,
    bundle: PathBuf,
    snapshot: Snapshot,
}
impl Candidate {
    /// Inspect exactly the selected Local completion directory, never a checkout.
    pub fn local(
        preferences: &Preferences,
        default_stage: &Path,
        cancel: &AtomicBool,
    ) -> Result<Self, Failure> {
        cancelled(cancel)?;
        if preferences.source != Source::Local {
            return Err(Failure::Invalid);
        }
        let directory = preferences
            .local_directory
            .as_deref()
            .unwrap_or(default_stage);
        if !directory.is_absolute() {
            return Err(Failure::Invalid);
        }
        let source = directory.join("Guild.app");
        regular_app(&source)?;
        let resources = source.join("Contents/Resources");
        let info = if present(&directory.join("guild-local-ready.json"))?
            || present(&resources.join("build-info.json"))?
        {
            Some(bundle::local(directory).map_err(|_| Failure::Invalid)?.1)
        } else {
            None
        };
        let build = Build::of_bundle(&source).ok_or(Failure::Invalid)?;
        if info.is_none()
            && !build
                .tag
                .as_deref()
                .is_some_and(|tag| Channel::Nightly.accepts(tag, true, false))
        {
            return Err(Failure::Invalid);
        }
        let seal = bundle::seal(&source, cancel).map_err(|_| classify(cancel, Failure::Invalid))?;
        bundle::validate(&source, info.as_ref(), cancel)
            .map_err(|_| classify(cancel, Failure::Invalid))?;
        if bundle::seal(&source, cancel).map_err(|_| classify(cancel, Failure::Invalid))? != seal {
            return Err(Failure::Changed);
        }
        let artifact = info
            .as_ref()
            .map(|info| Artifact::Build(info.build_id))
            .unwrap_or_else(|| Artifact::LegacySeal(seal.clone()));
        let identity = Identity {
            artifact,
            origin: Origin::Local {
                directory: preferences.local_directory.clone(),
                seal,
            },
        };
        Ok(Self {
            source,
            snapshot: Snapshot {
                info,
                build,
                identity,
            },
            _download: None,
        })
    }
    /// Adopt a fully authenticated owned payload and its extraction-time seal.
    pub fn remote(acquired: Acquired) -> Result<Self, Failure> {
        let identity = acquired.candidate().identity().ok_or(Failure::Invalid)?;
        let info = acquired.candidate().manifest.build.clone();
        let build = Build::from(&info);
        Ok(Self {
            source: acquired.bundle().to_path_buf(),
            snapshot: Snapshot {
                info: Some(info),
                build,
                identity,
            },
            _download: Some(acquired),
        })
    }
    /// Worker-only source for bounded release-note reads.
    pub(crate) fn source(&self) -> &Path {
        &self.source
    }
    /// Worker-only authenticated acquisition retained by a Remote selection.
    pub(crate) fn download(&self) -> Option<&Acquired> {
        self._download.as_ref()
    }
    /// Exact identity, independent of product ordering and build timestamp.
    pub fn identity(&self) -> &Identity {
        &self.snapshot.identity
    }
    /// New metadata is absent for an explicitly identified legacy replacement.
    pub fn info(&self) -> Option<&BuildInfo> {
        self.snapshot.info.as_ref()
    }
    /// Display metadata for provenance/release-note surfaces.
    pub fn build(&self) -> &Build {
        &self.snapshot.build
    }
    /// Bounded completion metadata only. This preserves an unchanged Local
    /// confirmation; it never replaces final native seal validation at prepare.
    pub(crate) fn local_unchanged(
        &self,
        preferences: &Preferences,
        default_stage: &Path,
    ) -> Result<bool, Failure> {
        if preferences.source != Source::Local || self._download.is_some() {
            return Err(Failure::Invalid);
        }
        let directory = preferences
            .local_directory
            .as_deref()
            .unwrap_or(default_stage);
        if self.source != directory.join("Guild.app") {
            return Ok(false);
        }
        regular_app(&self.source)?;
        if let Some(expected) = &self.snapshot.info {
            let (_, actual) = bundle::local(directory).map_err(|_| Failure::Invalid)?;
            Ok(&actual == expected)
        } else {
            if present(&directory.join("guild-local-ready.json"))?
                || present(&self.source.join("Contents/Resources/build-info.json"))?
            {
                return Ok(false);
            }
            Ok(Build::of_bundle(&self.source).as_ref() == Some(&self.snapshot.build))
        }
    }
    /// Make and validate the same final copy for both sources. Never touches the
    /// installed app. The runtime must recheck its ticket after this returns.
    pub fn prepare(&self, root: &Path, cancel: &AtomicBool) -> Result<Prepared, Failure> {
        cancelled(cancel)?;
        regular_app(&self.source).map_err(|_| Failure::Changed)?;
        if let Origin::Local { .. } = &self.snapshot.identity.origin {
            let directory = self.source.parent().ok_or(Failure::Invalid)?;
            let current = match &self.snapshot.info {
                Some(expected) => bundle::local(directory)
                    .map(|(_, actual)| actual == *expected)
                    .unwrap_or(false),
                None => {
                    !present(&directory.join("guild-local-ready.json"))?
                        && !present(&self.source.join("Contents/Resources/build-info.json"))?
                        && Build::of_bundle(&self.source).as_ref() == Some(&self.snapshot.build)
                }
            };
            if !current {
                return Err(Failure::Changed);
            }
        }
        let expected_seal = match &self.snapshot.identity.origin {
            Origin::Local { seal, .. } => seal,
            Origin::Remote { .. } => self._download.as_ref().ok_or(Failure::Invalid)?.seal(),
        };
        let size = bundle::expanded_size(&self.source, cancel)
            .map_err(|_| classify(cancel, Failure::Changed))?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root)
            .map_err(|_| Failure::Storage)?;
        bundle::ensure_space(root, size).map_err(|_| Failure::Space)?;
        let directory = tempfile::Builder::new()
            .prefix(".guild-prepare-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root)
            .map_err(|_| Failure::Storage)?;
        let app = directory.path().join("Guild.app");
        bundle::copy(&self.source, &app, cancel).map_err(|_| classify(cancel, Failure::Storage))?;
        bundle::validate(&app, self.snapshot.info.as_ref(), cancel)
            .map_err(|_| classify(cancel, Failure::Changed))?;
        if Build::of_bundle(&app).as_ref() != Some(&self.snapshot.build)
            || bundle::seal(&app, cancel).map_err(|_| classify(cancel, Failure::Changed))?
                != *expected_seal
        {
            return Err(Failure::Changed);
        }
        // Completion must still identify the selected Local publication after copy.
        if let Some(expected) = &self.snapshot.info
            && matches!(&self.snapshot.identity.origin, Origin::Local { .. })
            && bundle::local(self.source.parent().ok_or(Failure::Invalid)?)
                .map(|(_, actual)| actual != *expected)
                .unwrap_or(true)
        {
            return Err(Failure::Changed);
        }
        cancelled(cancel)?;
        Ok(Prepared {
            directory,
            bundle: app,
            snapshot: self.snapshot.clone(),
        })
    }
}
impl Prepared {
    /// Owned validated app for the journaled swap.
    pub fn bundle(&self) -> &Path {
        &self.bundle
    }
    /// Owned directory; retaining Prepared retains its copy.
    pub fn work_dir(&self) -> &Path {
        self.directory.path()
    }
    /// New metadata; legacy explicitly lacks an embedded UUID.
    pub fn info(&self) -> Option<&BuildInfo> {
        self.snapshot.info.as_ref()
    }
    /// Display metadata shared with legacy About/release-note surfaces.
    pub fn build(&self) -> &Build {
        &self.snapshot.build
    }
    /// Final artifact identity to recheck before helper launch.
    pub fn identity(&self) -> &Identity {
        &self.snapshot.identity
    }
}
fn present(path: &Path) -> Result<bool, Failure> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Failure::Invalid),
    }
}
fn regular_app(app: &Path) -> Result<(), Failure> {
    let metadata = fs::symlink_metadata(app).map_err(|_| Failure::Invalid)?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(Failure::Invalid);
    }
    Ok(())
}
fn cancelled(cancel: &AtomicBool) -> Result<(), Failure> {
    if cancel.load(Ordering::Acquire) {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}
fn classify(cancel: &AtomicBool, failure: Failure) -> Failure {
    if cancel.load(Ordering::Acquire) {
        Failure::Cancelled
    } else {
        failure
    }
}
