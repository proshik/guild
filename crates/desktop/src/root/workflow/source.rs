//! Source capture uses the existing read-only Git gateway, never a second writer.
use guild_session::workflow::{SourceFile, SourceIdentity, SourceLayer, digest};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    ffi::OsStr,
    io::Read,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    path::{Path, PathBuf},
};

/// Capture twice: concurrent byte/index changes are a named wait, never mixed evidence.
pub(super) fn capture(root: &Path, base: Option<&str>) -> Result<SourceIdentity, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("workflow.source: {e}"))?;
    let first = once(&root, base, 0, &mut BTreeSet::new())?;
    let second = once(&root, base, 0, &mut BTreeSet::new())?;
    if first != second {
        return Err(
            "workflow.source: checkout changed during capture; retry after current writes finish"
                .into(),
        );
    }
    Ok(second)
}
fn once(
    root: &Path,
    base: Option<&str>,
    depth: usize,
    seen: &mut BTreeSet<PathBuf>,
) -> Result<SourceIdentity, String> {
    if depth > 16 || !seen.insert(root.to_path_buf()) {
        return Err("workflow.source: cyclic/deep submodule checkout".into());
    }
    let git =
        |args: &[&str]| crate::git::run(root, args).map_err(|e| format!("workflow.source: {e}"));
    let common = crate::git::worktrees::common_dir(root);
    let mut files = Vec::new();
    let result = if let Err(error) = common {
        if root.ancestors().any(|p| p.join(".git").exists()) {
            return Err(format!("workflow.source: {error}"));
        }
        walk(root, root, &mut files, 0)?;
        SourceIdentity::unversioned(root.to_path_buf(), files)
    } else {
        let head = String::from_utf8(git(&["rev-parse", "--verify", "HEAD"])?)
            .map_err(|e| e.to_string())?
            .trim()
            .to_owned();
        let baseline = match base {
            Some(base) => String::from_utf8(git(&[
                "rev-parse",
                "--verify",
                &format!("{base}^{{commit}}"),
            ])?)
            .map_err(|e| e.to_string())?
            .trim()
            .to_owned(),
            None => head.clone(),
        };
        let index = git(&["ls-files", "--stage", "-z"])?;
        for entry in index.split(|b| *b == 0).filter(|e| !e.is_empty()) {
            let separator = entry
                .iter()
                .position(|b| *b == b'\t')
                .ok_or("workflow.source: malformed index row")?;
            let header = std::str::from_utf8(&entry[..separator]).map_err(|e| e.to_string())?;
            let parts: Vec<_> = header.split(' ').collect();
            if parts.len() != 3 || parts[2] != "0" {
                return Err(
                    "workflow.source: unresolved index merge entries require reconciliation".into(),
                );
            }
            let mode = u32::from_str_radix(parts[0], 8).map_err(|e| e.to_string())?;
            let relative = PathBuf::from(OsStr::from_bytes(&entry[separator + 1..]));
            contained(&relative)?;
            if run_data(&relative) {
                continue;
            }
            if mode == 0o160000 {
                let subroot = root.join(&relative).canonicalize().map_err(|e| {
                    format!("workflow.source submodule {}: {e}", relative.display())
                })?;
                if !subroot.starts_with(root) {
                    return Err("workflow.source: submodule escapes checkout".into());
                }
                let source = once(&subroot, None, depth + 1, seen)?;
                files.push(SourceFile {
                    path: relative,
                    layer: SourceLayer::Submodule,
                    sha256: digest(
                        &serde_json::to_vec(&(parts[1], source)).map_err(|e| e.to_string())?,
                    ),
                    mode,
                });
            } else {
                let size = String::from_utf8(git(&["cat-file", "-s", parts[1]])?)
                    .map_err(|e| e.to_string())?
                    .trim()
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                if size > 16 * 1024 * 1024 {
                    return Err(format!(
                        "workflow.source: index blob {} exceeds bounded Git output capture (16 MiB); full evidence withheld",
                        relative.display()
                    ));
                }
                let bytes = git(&["cat-file", "blob", parts[1]])?;
                files.push(SourceFile {
                    path: relative.clone(),
                    layer: SourceLayer::Index,
                    sha256: digest(&bytes),
                    mode,
                });
                observe(root, &relative, SourceLayer::Working, &mut files)?;
            }
            if files.len() > 100_000 {
                return Err("workflow.source: exceeds 100000 file/layer entries".into());
            }
        }
        for entry in git(&["ls-files", "--others", "--exclude-standard", "-z"])?
            .split(|b| *b == 0)
            .filter(|e| !e.is_empty())
        {
            let relative = PathBuf::from(OsStr::from_bytes(entry));
            contained(&relative)?;
            if !run_data(&relative) {
                observe(root, &relative, SourceLayer::Untracked, &mut files)?;
            }
        }
        SourceIdentity::new(root.to_path_buf(), baseline, head, files)
    };
    seen.remove(root);
    result
}
fn run_data(path: &Path) -> bool {
    path.starts_with(".guild/workflow-runs")
}
fn contained(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("workflow.source: noncontained source path".into());
    }
    Ok(())
}
fn observe(
    root: &Path,
    relative: &Path,
    layer: SourceLayer,
    files: &mut Vec<SourceFile>,
) -> Result<(), String> {
    let path = root.join(relative);
    if let Some(parent) = path.parent()
        && let Ok(parent) = parent.canonicalize()
        && !parent.starts_with(root)
    {
        return Err(format!(
            "workflow.source {}: ancestor link escapes checkout",
            relative.display()
        ));
    }
    let metadata = match std::fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && layer == SourceLayer::Working => {
            files.push(SourceFile {
                path: relative.into(),
                layer: SourceLayer::Removed,
                sha256: digest(b""),
                mode: 0,
            });
            return Ok(());
        }
        Err(e) => return Err(format!("workflow.source {}: {e}", relative.display())),
    };
    let sha256 = if metadata.file_type().is_symlink() {
        digest(
            std::fs::read_link(&path)
                .map_err(|e| e.to_string())?
                .as_os_str()
                .as_bytes(),
        )
    } else if metadata.is_file() {
        let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        format!("{:x}", hash.finalize())
    } else {
        return Err(format!(
            "workflow.source {}: special/nonfile resource",
            relative.display()
        ));
    };
    files.push(SourceFile {
        path: relative.into(),
        layer,
        sha256,
        mode: metadata.mode() & 0o177777,
    });
    if files.len() > 100_000 {
        return Err("workflow.source: exceeds 100000 file/layer entries".into());
    }
    Ok(())
}
fn walk(
    root: &Path,
    directory: &Path,
    files: &mut Vec<SourceFile>,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("workflow.source: unversioned directory depth exceeds 64".into());
    }
    for entry in std::fs::read_dir(directory).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let relative = path.strip_prefix(root).map_err(|e| e.to_string())?;
        if run_data(relative) || relative.starts_with(".git") {
            continue;
        }
        if std::fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .is_dir()
        {
            walk(root, &path, files, depth + 1)?;
        } else {
            observe(root, relative, SourceLayer::Working, files)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn git(root: &std::path::Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .current_dir(root)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    #[test]
    fn actual_dirty_index_untracked_and_executable_bytes_change_source_at_same_head() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-b", "main"]);
        git(root, &["config", "user.name", "Fixture"]);
        git(root, &["config", "user.email", "fixture@example.invalid"]);
        std::fs::write(root.join("code"), "first").unwrap();
        git(root, &["add", "code"]);
        git(root, &["commit", "-m", "fixture baseline"]);
        let before = capture(root, None).unwrap();
        std::fs::write(root.join("code"), "dirty").unwrap();
        let dirty = capture(root, None).unwrap();
        assert_eq!(before.commit, dirty.commit);
        assert_ne!(before.sha256, dirty.sha256);
        git(root, &["add", "code"]);
        let indexed = capture(root, None).unwrap();
        assert_ne!(dirty.sha256, indexed.sha256);
        std::fs::write(root.join("new"), "untracked").unwrap();
        let untracked = capture(root, None).unwrap();
        assert_ne!(indexed.sha256, untracked.sha256);
        std::fs::set_permissions(root.join("code"), std::fs::Permissions::from_mode(0o755))
            .unwrap();
        let executable = capture(root, None).unwrap();
        assert_ne!(untracked.sha256, executable.sha256);
        std::fs::create_dir_all(root.join(".guild/workflow-runs/retained")).unwrap();
        std::fs::write(
            root.join(".guild/workflow-runs/retained/state.json"),
            "own retained run",
        )
        .unwrap();
        let own_state = capture(root, None).unwrap();
        assert_eq!(executable.sha256, own_state.sha256);
        std::fs::remove_file(root.join("code")).unwrap();
        let removed = capture(root, None).unwrap();
        assert_ne!(removed.sha256, own_state.sha256);
    }
}
