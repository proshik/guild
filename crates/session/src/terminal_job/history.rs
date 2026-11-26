//! Pure original-identity history selection. Display names and targets never filter.
use super::JobReceipt;
use crate::{ProjectId, SessionId};
use std::{collections::HashMap, path::PathBuf};

/// The four deliberately selected areas of retained launch history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The focused resolved conversation.
    Session,
    /// The selected original root.
    Worktree,
    /// The repository shared by the selected root and its sibling worktrees.
    Project,
    /// Every retained receipt, including orphaned and unknown history.
    All,
}
impl Scope {
    /// Menu order, from narrow to wide.
    pub const ALL: [Self; 4] = [Self::Session, Self::Worktree, Self::Project, Self::All];
    /// Whether the opening context can select this area.
    pub fn available(self, context: &HistoryContext) -> bool {
        match self {
            Self::Session => context.conversation.is_some(),
            Self::Worktree | Self::Project => context.project.is_some(),
            Self::All => true,
        }
    }
    /// Default resolved once per opening, without silently widening empty history.
    pub fn initial(context: &HistoryContext) -> Self {
        if context.conversation.is_some() {
            Self::Session
        } else if context.project.is_some() {
            Self::Worktree
        } else {
            Self::All
        }
    }
    /// Explicit wider action, also valid when no repository has been discovered.
    pub fn wider(self) -> Option<Self> {
        match self {
            Self::Session => Some(Self::Worktree),
            Self::Worktree => Some(Self::Project),
            Self::Project => Some(Self::All),
            Self::All => None,
        }
    }
}

/// Immutable popup opening identities plus current exact-root repository discoveries.
#[derive(Debug, Clone, Default)]
pub struct HistoryContext {
    /// Focused conversation; never a launch-owner alias map.
    pub conversation: Option<SessionId>,
    /// Root selected when the popup opened.
    pub project: Option<ProjectId>,
    /// Repository discoveries for extant original root IDs only.
    pub repositories: HashMap<ProjectId, PathBuf>,
    /// Successfully discovered checkout paths, keyed by common git directory.
    pub worktrees: HashMap<PathBuf, std::collections::HashSet<PathBuf>>,
}
impl HistoryContext {
    /// Match original receipt identity; missing repository means unknown, not non-git.
    pub fn contains(&self, scope: Scope, receipt: &JobReceipt) -> bool {
        match scope {
            Scope::All => true,
            Scope::Session => self.conversation.is_some_and(|id| {
                receipt
                    .launch_context
                    .as_ref()
                    .and_then(|c| c.conversation)
                    .unwrap_or(receipt.owner)
                    == id
            }),
            Scope::Worktree => self.project == Some(receipt.project),
            Scope::Project => {
                if self.project == Some(receipt.project) {
                    return true;
                }
                let selected = self.project.and_then(|id| self.repositories.get(&id));
                let original = receipt
                    .launch_context
                    .as_ref()
                    .and_then(|c| c.repository.as_ref())
                    .or_else(|| self.repositories.get(&receipt.project));
                selected.zip(original).is_some_and(|(a, b)| a == b)
            }
        }
    }
    /// A successful repository snapshot can prove absence, never session closure.
    fn deleted_original_worktree(&self, receipt: &JobReceipt) -> bool {
        let Some(context) = receipt.launch_context.as_ref() else {
            return false;
        };
        let Some(root) = context.root_path.as_ref() else {
            return false;
        };
        let repository = context
            .repository
            .as_ref()
            .or_else(|| self.repositories.get(&receipt.project));
        let Some(checkouts) = repository.and_then(|repo| self.worktrees.get(repo)) else {
            return false;
        };
        // Raw project paths are retained in receipts. Resolving `..` lexically
        // could cross a symlink; a relative path also has no established base.
        // Either side being unresolved means this snapshot cannot prove absence.
        let comparable = |path: &std::path::Path| {
            path.is_absolute()
                && !path
                    .components()
                    .any(|part| part == std::path::Component::ParentDir)
        };
        if !comparable(root) || checkouts.iter().any(|checkout| !comparable(checkout)) {
            return false;
        }
        // A project may be opened below a checkout, including nested history
        // whose original checkout cannot be distinguished from its ancestor.
        !checkouts.iter().any(|checkout| root.starts_with(checkout))
    }

    /// Group by lifecycle, then original time descending, unknown last and stable ID.
    pub fn select<'a>(&self, scope: Scope, receipts: &'a [JobReceipt]) -> JobGroups<'a> {
        let (mut completed, mut ongoing): (Vec<_>, Vec<_>) = receipts
            .iter()
            .filter(|r| self.contains(scope, r))
            .filter(|r| !r.state.is_terminal() || !self.deleted_original_worktree(r))
            .partition(|r| r.state.is_terminal());
        for group in [&mut ongoing, &mut completed] {
            group.sort_by(|a, b| b.started_at_ms.cmp(&a.started_at_ms).then(a.id.cmp(&b.id)));
        }
        JobGroups { ongoing, completed }
    }
}
/// Selected rows; completion moves a row without changing its original chronology.
pub struct JobGroups<'a> {
    /// Starting, running and cancelling launches.
    pub ongoing: Vec<&'a JobReceipt>,
    /// Exited, failed and interrupted launches.
    pub completed: Vec<&'a JobReceipt>,
}
