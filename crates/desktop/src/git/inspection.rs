//! Push's selected commits as combined changes, oldest to newest: each file
//! keeps its first before and last after, renames follow the path, and an
//! added-then-deleted file disappears.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::{GitError, log, run};
use crate::theme::Mark;

/// A file and the two immutable blobs its combined change compares.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub file: log::FileChange,
    /// None when the first selected change added the file.
    pub before: Option<(String, PathBuf)>,
    /// None when the last selected change deleted the file.
    pub after: Option<(String, PathBuf)>,
}

/// `key` is the selected hashes in log order, separated by commas. A
/// single hash is also a key; these come from git's list, never typing.
pub fn read(root: &Path, key: &str) -> Result<Vec<Change>, GitError> {
    let mut merged: BTreeMap<PathBuf, Change> = BTreeMap::new();
    for sha in key.split(',').rev() {
        let bytes = run(
            root,
            &["show", "--name-status", "-z", "--format=", sha, "--"],
        )?;
        let files = log::files_of(&bytes).map_err(|e| GitError::Failed(e.to_string()))?;
        for file in files {
            let prior = file.from.as_ref().unwrap_or(&file.path);
            let mut change = merged.remove(prior).unwrap_or_else(|| Change {
                before: (file.mark != Mark::Added).then(|| (format!("{sha}^"), prior.clone())),
                after: None,
                file: file.clone(),
            });
            change.after =
                (file.mark != Mark::Deleted).then(|| (sha.to_owned(), file.path.clone()));
            let (mark, path, from) = match (&change.before, &change.after) {
                (None, None) => continue,
                (None, Some((_, path))) => (Mark::Added, path.clone(), None),
                (Some((_, path)), None) => (Mark::Deleted, path.clone(), None),
                (Some((_, old)), Some((_, new))) => (
                    Mark::Modified,
                    new.clone(),
                    (old != new).then(|| old.clone()),
                ),
            };
            change.file = log::FileChange {
                path: path.clone(),
                mark,
                from,
            };
            merged.insert(path, change);
        }
    }
    Ok(merged.into_values().collect())
}

/// The existing name-status reader's shape, preserving its public API
/// for the Log and the git worker. A combined rename carries both paths.
pub fn name_status(changes: &[Change]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let mut field = |text: &str| {
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
    };
    for change in changes {
        if let Some(from) = &change.file.from {
            field("R100");
            field(&from.to_string_lossy());
        } else {
            field(change.file.mark.letter());
        }
        field(&change.file.path.to_string_lossy());
    }
    bytes
}
