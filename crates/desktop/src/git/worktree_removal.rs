//! The GUI's confirmed identity, followed by two existing guarded git writes.
use std::os::unix::{ffi::OsStrExt, fs::PermissionsExt};
use std::path::{Path, PathBuf};

use super::{GitError, Said, Write, run, worktrees};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub repository: PathBuf,
    pub path: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
}

fn refused(text: impl Into<String>) -> GitError {
    GitError::Failed(text.into())
}

impl Target {
    /// For off-thread readers; the window builds this same identity from its git list.
    pub fn read(root: &Path, path: &Path) -> Result<Self, GitError> {
        let path = path.canonicalize().map_err(|e| refused(e.to_string()))?;
        let row = worktrees::list(root)?
            .into_iter()
            .find(|row| row.path == path)
            .ok_or_else(|| refused("Worktree is no longer listed. Reopen its confirmation."))?;
        Ok(Self {
            repository: worktrees::common_dir(root)?
                .canonicalize()
                .map_err(|e| refused(e.to_string()))?,
            path,
            branch: row.branch,
            head: row.head,
        })
    }

    pub fn can_delete_branch(&self) -> bool {
        self.branch.as_deref().is_some_and(|name| {
            !matches!(name, "main" | "master") && !name.contains(char::REPLACEMENT_CHARACTER)
        }) && self.head.as_deref().is_some_and(|oid| {
            matches!(oid.len(), 40 | 64)
                && oid.bytes().all(|b| b.is_ascii_hexdigit())
                && oid.bytes().any(|b| b != b'0')
        })
    }
}

/// A command-local transaction hook: no repository/global configuration is written.
/// Git's prepared phase holds the ref locks. Read the tip there, rather than
/// trusting branch -d's zero old-OID deletion record. Preserve the original hook.
struct BranchGuard {
    directory: tempfile::TempDir,
    original: PathBuf,
}
impl BranchGuard {
    fn prepare(root: &Path) -> Result<Self, GitError> {
        let version = String::from_utf8_lossy(&run(root, &["--version"])?).into_owned();
        let mut parts = version.split_whitespace().nth(2).unwrap_or("").split('.');
        let major = parts
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let minor = parts
            .next()
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        if (major, minor) < (2, 29) {
            return Err(refused("Safe branch deletion requires Git 2.29 or newer."));
        }
        let original = run(
            root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-path",
                "hooks/reference-transaction",
            ],
        )?;
        let original = PathBuf::from(std::ffi::OsStr::from_bytes(
            original.strip_suffix(b"\n").unwrap_or(&original),
        ));
        let directory = tempfile::tempdir().map_err(|e| refused(e.to_string()))?;
        let script = directory.path().join("reference-transaction");
        std::fs::write(
            &script,
            include_str!("../../assets/hooks/confirm-branch-delete.sh"),
        )
        .map_err(|e| refused(e.to_string()))?;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .map_err(|e| refused(e.to_string()))?;
        Ok(Self {
            directory,
            original,
        })
    }

    fn delete(&self, root: &Path, target: &Target) -> Result<Said, GitError> {
        let name = target
            .branch
            .as_deref()
            .ok_or_else(|| refused("No confirmed branch."))?;
        let reference = format!("refs/heads/{name}");
        let input = self.directory.path().join("transaction-input");
        let marker = self.directory.path().join("confirmed");
        // Reuse the named branch gateway's spelling, including its safe -d guard.
        let args = Write::DeleteBranch(name.into()).args();
        let mut hooks_path = std::ffi::OsString::from("core.hooksPath=");
        hooks_path.push(self.directory.path());
        let output = super::git_command()
            .arg("-c")
            .arg(hooks_path)
            .args(args)
            .current_dir(root)
            .env("GUILD_BRANCH_DELETE_REFERENCE", reference)
            .env(
                "GUILD_BRANCH_DELETE_COMMIT",
                target.head.as_deref().unwrap_or(""),
            )
            .env("GUILD_BRANCH_DELETE_HOOK", &self.original)
            .env("GUILD_BRANCH_DELETE_INPUT", input)
            .env("GUILD_BRANCH_DELETE_MARKER", &marker)
            .output()
            .map_err(|e| refused(e.to_string()))?;
        let said = Said {
            out: String::from_utf8_lossy(&output.stdout).into(),
            err: String::from_utf8_lossy(&output.stderr).into(),
        };
        if !output.status.success() {
            return Err(refused(said.text()));
        }
        if !marker.is_file() {
            return Err(refused(
                "Git did not confirm the guarded branch transaction.",
            ));
        }
        Ok(said)
    }
}

pub(super) fn write(root: &Path, target: &Target, delete_branch: bool) -> Result<Said, GitError> {
    // Existing write argv is UTF-8. Never turn an exact PathBuf into the
    // replacement-character spelling of a different checkout.
    if target.path.to_str().is_none() {
        return Err(refused(
            "Worktree path cannot be represented safely by the Git write gateway.",
        ));
    }
    let current = Target::read(root, &target.path)?;
    if current != *target {
        return Err(refused(
            "Worktree branch or commit changed. Reopen its confirmation.",
        ));
    }
    if worktrees::list(root)?
        .first()
        .is_some_and(|row| row.path == target.path)
    {
        return Err(refused("The primary checkout cannot be removed."));
    }
    if delete_branch && !target.can_delete_branch() {
        return Err(refused(
            "This branch cannot be deleted (main, master or unavailable commit).",
        ));
    }
    // Qualify hook support before removing any folder; keep its temporary files
    // owned until the synchronous branch child and its transaction hooks exit.
    let guard = delete_branch
        .then(|| BranchGuard::prepare(root))
        .transpose()?;
    let mut said = super::write_one(
        root,
        &Write::WorktreeRemoveConfirmed {
            target: target.clone(),
            delete_branch,
        },
    )?;
    if let Some(guard) = guard {
        match guard.delete(root, target) {
            Ok(branch) => {
                said.out.push_str(&branch.out);
                said.err.push_str(&branch.err);
            }
            Err(error) => {
                return Err(refused(format!(
                    "Worktree removed; branch deletion was not confirmed: {error}"
                )));
            }
        }
    }
    Ok(said)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn git(root: &Path, args: &[&str]) -> String {
        let out = super::super::git_command()
            .current_dir(root)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
        String::from_utf8(out.stdout).unwrap().trim().into()
    }
    #[test]
    fn branch_guard_rejects_a_new_merged_tip_or_symbolic_ref_after_worktree_removal() {
        for packed in [false, true] {
            for symbolic in [false, true] {
                let dir = tempfile::tempdir().unwrap();
                let root = dir.path();
                git(root, &["init", "-b", "main"]);
                git(root, &["commit", "--allow-empty", "-m", "base"]);
                let before = git(root, &["rev-parse", "HEAD"]);
                git(root, &["branch", "feature"]);
                git(root, &["commit", "--allow-empty", "-m", "new merged tip"]);
                let after = git(root, &["rev-parse", "HEAD"]);
                if packed {
                    git(root, &["pack-refs", "--all"]);
                }
                let target = Target {
                    repository: root.join(".git"),
                    path: root.join("already-removed"),
                    branch: Some("feature".into()),
                    head: Some(before),
                };
                let guard = BranchGuard::prepare(root).unwrap();
                // Exact inter-command gap: the worktree has gone, but an
                // external process changes the confirmed branch before -d.
                if symbolic {
                    git(
                        root,
                        &["symbolic-ref", "refs/heads/feature", "refs/heads/main"],
                    );
                } else {
                    git(root, &["update-ref", "refs/heads/feature", &after]);
                }
                let error = guard.delete(root, &target).unwrap_err();
                assert_eq!(git(root, &["rev-parse", "refs/heads/feature"]), after);
                assert_eq!(git(root, &["rev-parse", "refs/heads/main"]), after);
                assert!(
                    error.to_string().contains(if symbolic {
                        "symbolic"
                    } else {
                        "commit changed"
                    }),
                    "{error}"
                );
            }
        }
    }
}
