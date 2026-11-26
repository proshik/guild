//! Target-selected contract identity. All IO is called off the UI thread.
use super::ContractKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    io::Read,
    path::{Path, PathBuf},
};

/// Digest exact bytes, not a branch name or modification timestamp.
pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One contained artifact, relative to its pinned root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    /// Relative path, preserving resource relationships.
    pub path: PathBuf,
    /// SHA-256 of exact bytes.
    pub sha256: String,
}

/// Selection and immutable identity of the sole acceptance contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PinnedContract {
    /// Selected once from the actual target checkout.
    pub kind: ContractKind,
    /// Canonical checkout, not the launcher's current directory.
    pub target: PathBuf,
    /// Target-relative contract note or change directory.
    pub selection: PathBuf,
    /// Exact artifact identities, sorted by relative path.
    pub artifacts: Vec<Artifact>,
    /// Aggregate identity of kind, selection and artifact manifest.
    pub sha256: String,
    /// Actual installed OpenSpec entry point; absent on lightweight route.
    pub openspec: Option<PathBuf>,
    /// Explicit OpenSpec-only omissions for lightweight runs.
    pub skipped: Vec<String>,
}

pub(super) fn contained(root: &Path, relative: &Path) -> Result<PathBuf, String> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "{}: expected a contained relative path",
            relative.display()
        ));
    }
    let path = root
        .join(relative)
        .canonicalize()
        .map_err(|e| format!("{}: {e}", relative.display()))?;
    if !path.starts_with(root) {
        return Err(format!(
            "{}: resource escapes selected root",
            relative.display()
        ));
    }
    Ok(path)
}

pub(super) fn bounded_read(path: &Path) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes))
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(format!("{}: artifact exceeds 16 MiB", path.display()));
    }
    Ok(bytes)
}

fn walk(
    root: &Path,
    directory: &Path,
    files: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 || files.len() > 4096 {
        return Err("contract resource bounds exceeded".into());
    }
    for entry in
        std::fs::read_dir(directory).map_err(|e| format!("{}: {e}", directory.display()))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        if ty.is_symlink() {
            return Err(format!(
                "{}: contract symlink must be explicitly resolved",
                path.display()
            ));
        }
        if ty.is_dir() {
            walk(root, &path, files, depth + 1)?;
        } else if ty.is_file() {
            files.insert(
                path.strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            );
        } else {
            return Err(format!("{}: unsupported resource type", path.display()));
        }
    }
    Ok(())
}

/// Select the existing OpenSpec route or one explicit lightweight note. This
/// function creates no files, runs no CLI/schema/archive action and loads no skill.
/// Missing tools/artifacts and ambiguous selection are actionable waiting gaps.
pub fn pin_contract(
    target: &Path,
    selection: Option<&Path>,
    cli: Option<&Path>,
) -> Result<PinnedContract, String> {
    let target = target
        .canonicalize()
        .map_err(|e| format!("target {}: {e}", target.display()))?;
    if !target.is_dir() {
        return Err("target: checkout directory is unavailable".into());
    }
    let openspec = target.join("openspec");
    let kind = if openspec.try_exists().map_err(|e| e.to_string())? {
        if !openspec.is_dir() {
            return Err("openspec: present but not a readable directory".into());
        }
        ContractKind::Openspec
    } else {
        ContractKind::Lightweight
    };
    let mut files = BTreeSet::new();
    let (selection, program, skipped) = match kind {
        ContractKind::Lightweight => {
            let selected = selection.ok_or("contract: select one lightweight task note")?;
            let path = contained(&target, selected)?;
            if !path.is_file() || bounded_read(&path)?.is_empty() {
                return Err("contract: lightweight task note must be a nonempty file".into());
            }
            files.insert(selected.to_path_buf());
            (
                selected.to_path_buf(),
                None,
                vec![
                    "OpenSpec preparation".into(),
                    "OpenSpec schema and delta validation".into(),
                    "OpenSpec archive".into(),
                ],
            )
        }
        ContractKind::Openspec => {
            let cli = cli.ok_or("openspec: installed CLI is unavailable; dependent work waits")?;
            let cli = cli
                .canonicalize()
                .map_err(|e| format!("openspec executable: {e}"))?;
            use std::os::unix::fs::PermissionsExt;
            if !cli.is_file()
                || std::fs::metadata(&cli)
                    .map_err(|e| e.to_string())?
                    .permissions()
                    .mode()
                    & 0o111
                    == 0
            {
                return Err("openspec executable: not executable".into());
            }
            let selected = if let Some(path) = selection {
                path.to_path_buf()
            } else {
                let changes = openspec.join("changes");
                let candidates = std::fs::read_dir(&changes)
                    .map_err(|e| format!("openspec/changes: {e}"))?
                    .filter_map(Result::ok)
                    .filter(|e| e.file_name() != "archive" && e.path().is_dir())
                    .map(|e| PathBuf::from("openspec/changes").join(e.file_name()))
                    .collect::<Vec<_>>();
                if candidates.len() != 1 {
                    return Err(format!(
                        "openspec/changes: select one matching change ({} candidates)",
                        candidates.len()
                    ));
                }
                candidates[0].clone()
            };
            if !selected.starts_with("openspec/changes")
                || selected.components().count() != 3
                || selected.ends_with("archive")
            {
                return Err("contract.selection: expected openspec/changes/CHANGE".into());
            }
            let path = contained(&target, &selected)?;
            for name in ["proposal.md", "design.md", "tasks.md", "acceptance.md"] {
                let relative = selected.join(name);
                let artifact = contained(&target, &relative)?;
                if !artifact.is_file() || bounded_read(&artifact)?.is_empty() {
                    return Err(format!(
                        "{}: missing nonempty accepted artifact",
                        relative.display()
                    ));
                }
                files.insert(relative);
            }
            let specs = path.join("specs");
            if !specs.is_dir() {
                return Err(format!(
                    "{}: accepted delta specs unavailable",
                    specs.display()
                ));
            }
            walk(&target, &specs, &mut files, 0)?;
            if !files.iter().any(|p| p.starts_with(selected.join("specs"))) {
                return Err("contract.specs: no accepted delta specifications".into());
            }
            for relative in ["openspec/config.yaml", "openspec/README.md"] {
                if target.join(relative).is_file() {
                    files.insert(relative.into());
                }
            }
            (selected, Some(cli), Vec::new())
        }
    };
    let artifacts = files
        .into_iter()
        .map(|path| {
            let bytes = bounded_read(&contained(&target, &path)?)?;
            Ok(Artifact {
                path,
                sha256: digest(&bytes),
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let sha256 =
        digest(&serde_json::to_vec(&(kind, &selection, &artifacts)).map_err(|e| e.to_string())?);
    Ok(PinnedContract {
        kind,
        target,
        selection,
        artifacts,
        sha256,
        openspec: program,
        skipped,
    })
}

impl PinnedContract {
    /// Check that original bytes still match. The durable snapshot remains
    /// historical when this check fails; never repin changed bytes in place.
    pub fn verify(&self) -> Result<(), String> {
        if self.kind == ContractKind::Openspec {
            let relative = self.selection.join("specs");
            let directory = contained(&self.target, &relative)?;
            let mut present = BTreeSet::new();
            walk(&self.target, &directory, &mut present, 0)?;
            let expected = self
                .artifacts
                .iter()
                .filter(|artifact| artifact.path.starts_with(&relative))
                .map(|artifact| artifact.path.clone())
                .collect::<BTreeSet<_>>();
            if present != expected {
                return Err("contract.specs: accepted artifact set changed".into());
            }
        }
        for artifact in &self.artifacts {
            let path = contained(&self.target, &artifact.path)?;
            if digest(&bounded_read(&path)?) != artifact.sha256 {
                return Err(format!(
                    "{}: contract resource changed",
                    artifact.path.display()
                ));
            }
        }
        Ok(())
    }
}

/// Run-owned copy of exact contract bytes, independent of mutable source files.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContractSnapshot {
    /// Original accepted identity and target provenance.
    pub contract: PinnedContract,
    /// Content-addressed, sealed directory containing target-relative artifacts.
    pub directory: PathBuf,
}

impl ContractSnapshot {
    /// Check saved bytes and the entire saved artifact set without reading source.
    pub fn verify(&self) -> Result<(), String> {
        let expected_digest = digest(
            &serde_json::to_vec(&(
                self.contract.kind,
                &self.contract.selection,
                &self.contract.artifacts,
            ))
            .map_err(|e| e.to_string())?,
        );
        if expected_digest != self.contract.sha256 {
            return Err("contract snapshot: manifest identity changed".into());
        }
        let root = self.directory.canonicalize().map_err(|e| e.to_string())?;
        if root != self.directory || !root.is_dir() {
            return Err("contract snapshot: canonical directory unavailable".into());
        }
        let mut present = BTreeSet::new();
        walk(&root, &root, &mut present, 0)?;
        let expected = self
            .contract
            .artifacts
            .iter()
            .map(|a| a.path.clone())
            .collect::<BTreeSet<_>>();
        if present != expected || expected.len() != self.contract.artifacts.len() {
            return Err("contract snapshot: artifact set changed".into());
        }
        for artifact in &self.contract.artifacts {
            if digest(&bounded_read(&contained(&root, &artifact.path)?)?) != artifact.sha256 {
                return Err(format!(
                    "{}: saved contract resource changed",
                    artifact.path.display()
                ));
            }
        }
        Ok(())
    }
}

impl PinnedContract {
    /// Publish a sealed run-owned copy. Existing copies are verified, never repaired
    /// from current source. Partial writes remain distinguishable from publication.
    pub fn snapshot(&self, destination: &Path) -> Result<ContractSnapshot, String> {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let expected = digest(
            &serde_json::to_vec(&(self.kind, &self.selection, &self.artifacts))
                .map_err(|e| e.to_string())?,
        );
        if expected != self.sha256 {
            return Err("contract: manifest identity changed".into());
        }
        std::fs::create_dir_all(destination).map_err(|e| e.to_string())?;
        let destination = destination.canonicalize().map_err(|e| e.to_string())?;
        let snapshot = ContractSnapshot {
            contract: self.clone(),
            directory: destination.join(&self.sha256),
        };
        if snapshot.directory.try_exists().map_err(|e| e.to_string())? {
            snapshot.verify()?;
            return Ok(snapshot);
        }
        self.verify()?;
        let staging = destination.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
        std::fs::create_dir(&staging).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&staging, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
        let mut directories = BTreeSet::from([staging.clone()]);
        for artifact in &self.artifacts {
            let bytes = bounded_read(&contained(&self.target, &artifact.path)?)?;
            if digest(&bytes) != artifact.sha256 {
                return Err(format!(
                    "{}: contract changed during snapshot",
                    artifact.path.display()
                ));
            }
            let path = staging.join(&artifact.path);
            let parent = path.parent().ok_or("contract artifact has no parent")?;
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            let mut ancestor = parent;
            while ancestor.starts_with(&staging) {
                directories.insert(ancestor.to_path_buf());
                let Some(next) = ancestor.parent() else { break };
                ancestor = next;
            }
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|e| e.to_string())?;
            file.set_permissions(std::fs::Permissions::from_mode(0o400))
                .map_err(|e| e.to_string())?;
        }
        ContractSnapshot {
            contract: self.clone(),
            directory: staging.clone(),
        }
        .verify()?;
        for directory in directories.iter().rev() {
            std::fs::set_permissions(directory, std::fs::Permissions::from_mode(0o500))
                .map_err(|e| e.to_string())?;
            std::fs::File::open(directory)
                .and_then(|f| f.sync_all())
                .map_err(|e| e.to_string())?;
        }
        std::fs::rename(&staging, &snapshot.directory).map_err(|e| e.to_string())?;
        std::fs::File::open(&destination)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        snapshot.verify()?;
        Ok(snapshot)
    }
}
