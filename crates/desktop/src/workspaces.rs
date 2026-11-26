//! Pure workspace routing shared by the command-line door and the window.

use std::path::Path;

use guild_session::WorkspaceId;

/// The workspace a command means, or the actionable reason it is ambiguous.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meant {
    /// Route the command here.
    Workspace(WorkspaceId),
    /// Refuse it in words suitable for the command line.
    Refused(String),
}

/// The next place in a workspace cycle. A one-item cycle stays put.
pub fn stepped(count: usize, current: usize, forward: bool) -> usize {
    if count < 2 {
        return current.min(count.saturating_sub(1));
    }
    if forward {
        (current + 1) % count
    } else {
        (current + count - 1) % count
    }
}

/// Resolve workspace context in the order of M13.1 decision 1, which
/// amends M12.5 decision 17: `asked`; the one workspace holding the
/// repository, whoever asks; among several holders, the caller's; for a
/// repository nobody holds, the caller's or the front one.
///
/// **The repository's workspace before the caller's**, because that is
/// where the person looks for a session in it: an agent in one workspace
/// starting a task in another workspace's repository put its tab where
/// nobody was looking.
pub fn workspace_for(
    asked: Option<WorkspaceId>,
    caller: Option<WorkspaceId>,
    holders_of_repository: &[WorkspaceId],
    front: WorkspaceId,
    names: &[(WorkspaceId, String)],
    path: &Path,
) -> Meant {
    if let Some(id) = asked {
        return Meant::Workspace(id);
    }
    match holders_of_repository {
        [id] => Meant::Workspace(*id),
        [] => Meant::Workspace(caller.unwrap_or(front)),
        many => {
            if let Some(id) = caller.filter(|id| many.contains(id)) {
                return Meant::Workspace(id);
            }
            let labels = many
                .iter()
                .map(|id| {
                    names
                        .iter()
                        .find(|(known, _)| known == id)
                        .map(|(_, name)| name.clone())
                        .unwrap_or_else(|| id.to_string())
                })
                .collect::<Vec<_>>()
                .join(", ");
            Meant::Refused(format!(
                "{} belongs to multiple workspaces ({labels}); pass --workspace",
                path.display()
            ))
        }
    }
}
