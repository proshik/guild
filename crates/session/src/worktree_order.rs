//! Workspace-local worktree ordering without opening a checkout.
use std::path::{Path, PathBuf};

/// An insertion relative to a currently present row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Insertion {
    /// Immediately before this path.
    Before(PathBuf),
    /// Immediately after this path, including the final row.
    After(PathBuf),
}

/// Apply remembered paths to current membership, keeping the primary first.
pub fn apply(current: &[PathBuf], saved: &[PathBuf], primary: Option<&Path>) -> Vec<PathBuf> {
    let mut ordered = Vec::with_capacity(current.len());
    for path in primary
        .into_iter()
        .map(Path::to_path_buf)
        .chain(saved.iter().cloned())
        .chain(current.iter().cloned())
    {
        if current.contains(&path) && !ordered.contains(&path) {
            ordered.push(path);
        }
    }
    ordered
}

/// Return a changed order for a valid linked-row move; stale and no-op drops fail.
pub fn moved(
    current: &[PathBuf],
    primary: Option<&Path>,
    source: &Path,
    insertion: &Insertion,
) -> Option<Vec<PathBuf>> {
    let (target, after) = match insertion {
        Insertion::Before(path) => (path.as_path(), false),
        Insertion::After(path) => (path.as_path(), true),
    };
    if Some(source) == primary || source == target || (!after && Some(target) == primary) {
        return None;
    }
    let mut order = current.to_vec();
    let source_index = order.iter().position(|path| path == source)?;
    let source = order.remove(source_index);
    let target_index = order.iter().position(|path| path == target)?;
    order.insert(target_index + usize::from(after), source);
    (order != current).then_some(order)
}
