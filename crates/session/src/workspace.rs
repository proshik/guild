//! Named sets of project roots.

use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::ProjectId;

/// Maximum number of workspaces that may be newly created. Loading never truncates.
pub const CREATION_LIMIT: usize = 5;

/// The name given to the workspace made from state written before M12.5.
pub const DEFAULT_NAME: &str = "Default";

/// The icons offered by the workspace icon picker.
pub const ICONS: &[&str] = &[
    "sun",
    "moon",
    "brain",
    "flask",
    "target",
    "globe",
    "terminal",
    "sparkles",
    "shield",
    "bell",
    "lightbulb-sparkle",
    "palette",
    "tree",
    "folder",
    "wand",
    "git-branch",
    "keyboard",
    "monitor",
    "history",
    "pin",
    "star",
    "heart",
    "flag",
    "bookmark",
    "rocket",
    "coffee",
    "briefcase",
    "home",
    "book",
    "music",
    "camera",
    "gamepad",
    "dumbbell",
    "plane",
    "pizza",
    "skull",
    "zap",
    "layers",
];

/// A stable workspace identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WorkspaceId(Uuid);

impl WorkspaceId {
    /// Make a fresh identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
    /// The migration sentinel used by records without a workspace.
    pub fn nil() -> Self {
        Self(Uuid::nil())
    }
    /// Whether this is the migration sentinel.
    pub fn is_nil(self) -> bool {
        self.0.is_nil()
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::nil()
    }
}
impl fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}
impl FromStr for WorkspaceId {
    type Err = uuid::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(value).map(Self)
    }
}

/// A named, ordered set of roots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Its stable identifier.
    pub id: WorkspaceId,
    /// Its person-visible name.
    pub name: String,
    /// An optional Lucide icon name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Repository ordering; manual root order stays in `projects`.
    #[serde(default)]
    pub project_sort: crate::project_sort::Mode,
    /// Roots in manual sidebar order.
    #[serde(default)]
    pub projects: Vec<ProjectId>,
    /// Manual linked-worktree paths per repository, including unopened checkouts.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub worktree_order: std::collections::HashMap<std::path::PathBuf, Vec<std::path::PathBuf>>,
    /// Worktrees shown in the workspace’s Pinned section, in pin order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pinned_worktrees: Vec<std::path::PathBuf>,
    /// Worktrees whose list of tabs a person expanded in the column; every
    /// other worktree's is collapsed. State written while lists were
    /// expanded by default named the collapsed ones instead, under
    /// `folded_worktree_tabs`: those are collapsed by the default now, so
    /// that key is read past and no longer written.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unfolded_worktree_tabs: Vec<std::path::PathBuf>,
    /// Named sections of the column — this workspace's alone. See
    /// [`crate::project_group`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_groups: Vec<crate::project_group::ProjectGroup>,
    /// Whether the groups stand among the ungrouped repositories in
    /// `projects`' order. State written before they could drew every group
    /// above the ungrouped, in creation order, and lacks this: loading it
    /// puts `projects` into that shape once and sets it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub groups_in_order: bool,
    /// The root last shown in this workspace.
    #[serde(default)]
    pub front: Option<ProjectId>,
}

/// One root as held by one workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Seat {
    /// The workspace holding the root.
    pub workspace: WorkspaceId,
    /// The root being held.
    pub project: ProjectId,
}

impl From<(WorkspaceId, ProjectId)> for Seat {
    fn from((workspace, project): (WorkspaceId, ProjectId)) -> Self {
        Self { workspace, project }
    }
}

// Transitional source compatibility for callers which have not yet gained
// workspace context. `Desk::add_session` resolves the nil workspace to the
// desk's active workspace before the record enters memory.
impl From<ProjectId> for Seat {
    fn from(project: ProjectId) -> Self {
        Self {
            workspace: WorkspaceId::nil(),
            project,
        }
    }
}

/// Why a workspace operation was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// The only workspace cannot be deleted.
    Last,
    /// Creation capacity is exhausted; existing saved workspaces are retained.
    Capacity,
    /// The trimmed name was empty.
    BlankName,
    /// The requested icon is not in [`ICONS`].
    UnknownIcon,
}

impl fmt::Display for Refused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Capacity => "At most five workspaces can be created.",
            Self::Last => "The last workspace can't be deleted.",
            Self::BlankName => "A workspace name cannot be blank.",
            Self::UnknownIcon => "That workspace icon is not available.",
        })
    }
}

/// Whether leaving a workspace released the underlying root too.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Released {
    /// Another workspace still holds the root.
    Kept,
    /// No workspace holds the root any longer.
    Released,
}
