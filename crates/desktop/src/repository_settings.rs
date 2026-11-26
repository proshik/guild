//! Local repository identity and future worktree preference validation.
use guild_session::repository_settings::RepositorySettings;
use std::path::{Path, PathBuf};
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepositoryIdentity {
    pub key: PathBuf,
    pub main: Option<PathBuf>,
    pub git: bool,
}
pub(crate) fn resolve_repository(root: &Path) -> Result<RepositoryIdentity, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    classify_repository(&root, crate::git::worktrees::common_dir(&root))
}
fn classify_repository(
    root: &Path,
    common: Result<PathBuf, crate::git::GitError>,
) -> Result<RepositoryIdentity, String> {
    match common {
        Ok(common) => {
            let key = common.canonicalize().map_err(|e| e.to_string())?;
            let trees = crate::git::worktrees::list(root).map_err(|e| e.to_string())?;
            let main = trees
                .first()
                .filter(|w| !w.bare && !w.prunable)
                .and_then(|w| w.path.canonicalize().ok());
            Ok(RepositoryIdentity {
                key,
                main,
                git: true,
            })
        }
        Err(error) if error != crate::git::GitError::NoRepository => Err(error.to_string()),
        Err(error) => {
            // A failed Git command is not proof of a directory outside Git.
            // Confirm absence of metadata without hiding permission failures.
            for parent in root.ancestors() {
                match std::fs::symlink_metadata(parent.join(".git")) {
                    Ok(_) => return Err(error.to_string()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
            if root.join("HEAD").exists() && root.join("objects").exists() {
                return Err(error.to_string());
            }
            Ok(RepositoryIdentity {
                key: root.to_path_buf(),
                main: Some(root.to_path_buf()),
                git: false,
            })
        }
    }
}
pub(crate) fn worktree_destination(
    main: &Path,
    name: &str,
    settings: &RepositorySettings,
) -> Result<PathBuf, String> {
    let name = crate::column::worktree_name(name)?;
    if let Some(base) = &settings.worktree_base {
        validate_base(base)?;
    }
    match &settings.worktree_directory {
        None => Ok(crate::column::folder_for(main, name)),
        Some(directory) => {
            validate_directory(directory)?;
            let folder = crate::column::folder_for(main, name);
            // Git lists canonical worktree paths. Keep the writer's pending
            // receipt in that identity too, including user-selected aliases.
            Ok(directory
                .canonicalize()
                .map_err(|e| e.to_string())?
                .join(folder.file_name().ok_or("repository has no folder name")?))
        }
    }
}
pub(crate) fn validate_base(base: &str) -> Result<(), String> {
    if base.trim().is_empty() || base.starts_with('-') || base.chars().any(char::is_control) {
        Err("invalid worktree base".into())
    } else {
        Ok(())
    }
}
pub(crate) fn validate_directory(directory: &Path) -> Result<(), String> {
    use std::os::unix::ffi::OsStrExt;
    if !directory.is_absolute() || !directory.is_dir() {
        return Err("choose an existing absolute directory".into());
    }
    let path =
        std::ffi::CString::new(directory.as_os_str().as_bytes()).map_err(|e| e.to_string())?;
    // SAFETY: a valid terminated path; access performs no filesystem mutation.
    if unsafe { libc::access(path.as_ptr(), libc::W_OK) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    fn git(root: &Path, args: &[&str]) {
        let output = std::process::Command::new("git")
            .arg("-C")
            .arg(root)
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
    fn missing_git_or_failed_query_cannot_become_a_non_git_preference_key() {
        let dir = tempfile::tempdir().unwrap();
        for error in [
            crate::git::GitError::NoGit,
            crate::git::GitError::Failed("fixture permission failure".into()),
        ] {
            assert!(
                classify_repository(dir.path(), Err(error)).is_err(),
                "tool failure must not confirm non-Git identity"
            );
        }
    }

    #[test]
    fn aliases_and_linked_worktrees_have_one_identity() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main");
        std::fs::create_dir(&main).unwrap();
        git(&main, &["init", "-q"]);
        git(
            &main,
            &[
                "-c",
                "user.email=fixture@example.invalid",
                "-c",
                "user.name=Fixture",
                "commit",
                "--allow-empty",
                "-m",
                "fixture",
                "-q",
            ],
        );
        let linked = dir.path().join("linked");
        git(
            &main,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "fixture",
                linked.to_str().unwrap(),
            ],
        );
        let alias = dir.path().join("alias");
        std::os::unix::fs::symlink(&linked, &alias).unwrap();
        let primary = resolve_repository(&main).unwrap();
        assert!(primary.git);
        assert_eq!(primary.key, resolve_repository(&linked).unwrap().key);
        assert_eq!(primary, resolve_repository(&alias).unwrap());
        assert_eq!(primary.main, Some(main.canonicalize().unwrap()));
    }
    #[test]
    fn future_destination_uses_validated_override() {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main");
        let override_dir = dir.path().join("custom");
        std::fs::create_dir(&override_dir).unwrap();
        let settings = RepositorySettings {
            worktree_directory: Some(override_dir.clone()),
            ..Default::default()
        };
        assert_eq!(
            worktree_destination(&main, "topic", &settings).unwrap(),
            override_dir.canonicalize().unwrap().join("main-topic")
        );
        let invalid = RepositorySettings {
            worktree_directory: Some("relative".into()),
            ..Default::default()
        };
        assert!(worktree_destination(&main, "topic", &invalid).is_err());
        assert!(worktree_destination(&main, "--help", &settings).is_err());
    }

    #[test]
    fn destination_alias_matches_the_path_git_will_list_for_settlement() {
        let owned = tempfile::tempdir().unwrap();
        let real = owned.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let alias = owned.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let settings = RepositorySettings {
            worktree_directory: Some(alias),
            ..Default::default()
        };
        assert_eq!(
            worktree_destination(&owned.path().join("main"), "topic", &settings).unwrap(),
            real.canonicalize().unwrap().join("main-topic")
        );
    }

    #[test]
    fn nested_roots_git_files_bare_and_invalid_metadata_are_distinct() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let metadata = dir.path().join("metadata");
        git(
            &root,
            &[
                "init",
                "-q",
                "--separate-git-dir",
                metadata.to_str().unwrap(),
            ],
        );
        let nested = root.join("nested");
        std::fs::create_dir(&nested).unwrap();
        assert!(root.join(".git").is_file());
        assert_eq!(
            resolve_repository(&root).unwrap().key,
            resolve_repository(&nested).unwrap().key
        );
        let bare = dir.path().join("bare");
        std::fs::create_dir(&bare).unwrap();
        git(&bare, &["init", "--bare", "-q"]);
        let identity = resolve_repository(&bare).unwrap();
        assert!(identity.git && identity.main.is_none());
        let plain = dir.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        assert!(!resolve_repository(&plain).unwrap().git);
        std::fs::write(
            plain.join(".git"),
            "gitdir: /guild-fixture-missing-metadata\n",
        )
        .unwrap();
        assert!(
            resolve_repository(&plain).is_err(),
            "broken metadata must not produce a non-Git preference key"
        );
    }

    #[test]
    fn failed_identity_does_not_become_a_directory_key() {
        assert!(resolve_repository(Path::new("/guild-fixture-nonexistent")).is_err());
    }
}
