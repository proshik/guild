//! Named sections of one workspace's column.
//!
//! A project group gathers repositories under a header of its own. It is a
//! workspace's and no other's, and it **exists only while it holds a
//! repository**: it is made with its first member, and the member that
//! leaves last — moved to another group, taken out of the group, or taken
//! out of the workspace — takes the group with it. Deleting a group only
//! returns its repositories to the ungrouped list; nothing is closed.
//!
//! A member is kept by the repository's key — the common git directory, or
//! the directory itself outside git — the same key a repository's name and
//! fold are kept by, so every worktree of one repository is in the same
//! group, including one opened after the group was made. A group keeps only
//! *which* repositories; **where it stands is the workspace's own project
//! order**: the column draws a group where its first member comes in that
//! order — or, under a sort mode, where its most active member does — so
//! groups and ungrouped repositories share one list. Every deed here keeps
//! a group's roots beside each other in `projects`, so a group does not jump
//! when a repository joins it.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::workspace::{Workspace, WorkspaceId};
use crate::{Desk, Project};

/// A stable project group identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectGroupId(Uuid);

impl ProjectGroupId {
    /// Make a fresh identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for ProjectGroupId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for ProjectGroupId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One named section of a workspace's column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGroup {
    /// Its stable identifier.
    pub id: ProjectGroupId,
    /// Its person-visible name, trimmed and never blank.
    pub name: String,
    /// The repositories' keys; never empty while the group exists.
    pub repositories: Vec<PathBuf>,
    /// Whether its header is collapsed over its repositories.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub collapsed: bool,
    /// The directory the group is bound to — GLD-055. Sessions started for
    /// the group run there; it is opened as a root of the workspace, held
    /// apart from the members: never one of them, and never what keeps the
    /// group alive. `None` until the person binds one; a state file written
    /// before groups had folders loads without it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
}

/// Why a folder was not bound to a group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderRefused {
    /// The workspace holds no such group.
    NoGroup,
    /// The folder is a root of a group's member — this group's, or another
    /// group's in any workspace — which would make one directory both a
    /// group's folder and a project of a group.
    OwnRoot,
    /// Another group of the workspace is bound to the same folder.
    OtherGroup,
}

/// Where a member's main checkout lies against its group's folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberMatch {
    /// The repository's key.
    pub key: PathBuf,
    /// Its main checkout: the folder a `.git` directory sits in, or the
    /// directory itself outside git.
    pub checkout: PathBuf,
    /// Whether the checkout is a direct child of the group's folder.
    pub inside: bool,
}

/// How a bound group stands against the filesystem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderMatch {
    /// The group's folder.
    pub folder: PathBuf,
    /// Each member, in the group's order.
    pub members: Vec<MemberMatch>,
}

impl FolderMatch {
    /// Whether every member's main checkout is a direct child of the folder.
    pub fn matches(&self) -> bool {
        self.members.iter().all(|member| member.inside)
    }

    /// How many members are inside the folder.
    pub fn inside(&self) -> usize {
        self.members.iter().filter(|member| member.inside).count()
    }
}

/// A repository's main checkout, from its key: the folder its `.git`
/// directory sits in, or the key itself — a directory outside git, or a
/// bare repository, which has no other.
pub fn main_checkout(key: &Path) -> PathBuf {
    if key.file_name().is_some_and(|name| name == ".git")
        && let Some(parent) = key.parent()
    {
        return parent.to_path_buf();
    }
    key.to_path_buf()
}

/// The folder suggested when a group is bound — decision 1: the direct
/// parent of the members' main checkouts, only when **every** member has
/// the same one and it is neither the home directory nor the filesystem
/// root. Nothing otherwise, and nothing for a group with no members.
pub fn suggest_folder(checkouts: &[PathBuf], home: Option<&Path>) -> Option<PathBuf> {
    let mut parents = checkouts.iter().map(|checkout| checkout.parent());
    let first = parents.next()??;
    if parents.any(|parent| parent != Some(first)) {
        return None;
    }
    // The filesystem root has no parent of its own.
    if first.parent().is_none() || home == Some(first) {
        return None;
    }
    Some(first.to_path_buf())
}

/// The key a repository is kept by: its common git directory, or the
/// directory itself outside git — the column's repository key.
pub fn repository_key(project: &Project) -> &Path {
    project.repository.as_deref().unwrap_or(&project.path)
}

impl Workspace {
    /// The group bound to a folder.
    pub fn project_group_of_folder(&self, folder: &Path) -> Option<&ProjectGroup> {
        self.project_groups
            .iter()
            .find(|group| group.folder.as_deref() == Some(folder))
    }

    /// How a bound group stands against the filesystem: each member's main
    /// checkout, and whether it is a direct child of the folder. `resolve`
    /// turns a path into the form both sides are compared in — the window
    /// hands the filesystem's canonical one, so a symlinked path compares
    /// as what it points to. `None` for a group without a folder.
    ///
    /// `checkout_of` names a member's main checkout by its key — the window
    /// asks git's worktree list, since a repository with its git directory
    /// kept apart has a key that is no checkout at all; [`main_checkout`] is
    /// the answer without one.
    pub fn project_group_match(
        &self,
        id: ProjectGroupId,
        checkout_of: impl Fn(&Path) -> PathBuf,
        resolve: impl Fn(&Path) -> PathBuf,
    ) -> Option<FolderMatch> {
        let group = self.project_group(id)?;
        let folder = group.folder.clone()?;
        let resolved = resolve(&folder);
        let members = group
            .repositories
            .iter()
            .map(|key| {
                let checkout = checkout_of(key);
                let inside = resolve(&checkout).parent() == Some(resolved.as_path());
                MemberMatch {
                    key: key.clone(),
                    checkout,
                    inside,
                }
            })
            .collect();
        Some(FolderMatch { folder, members })
    }

    /// The group holding a repository, by its key.
    pub fn project_group_of(&self, key: &Path) -> Option<&ProjectGroup> {
        self.project_groups
            .iter()
            .find(|group| group.repositories.iter().any(|known| known == key))
    }

    /// A group by identifier.
    pub fn project_group(&self, id: ProjectGroupId) -> Option<&ProjectGroup> {
        self.project_groups.iter().find(|group| group.id == id)
    }
}

/// Take a repository out of every group, and the groups it emptied with it.
/// The folder of a group that went is put in `released`: its root is the
/// window's to close.
fn forget(groups: &mut Vec<ProjectGroup>, key: &Path, released: &mut Vec<PathBuf>) -> bool {
    let mut changed = false;
    for group in groups.iter_mut() {
        let before = group.repositories.len();
        group.repositories.retain(|known| known != key);
        changed |= group.repositories.len() != before;
    }
    released.extend(
        groups
            .iter()
            .filter(|group| group.repositories.is_empty())
            .filter_map(|group| group.folder.clone()),
    );
    groups.retain(|group| !group.repositories.is_empty());
    changed
}

impl Desk {
    pub(crate) fn release_group_folders(&mut self, workspace: WorkspaceId, folders: Vec<PathBuf>) {
        self.released_group_folders
            .extend(folders.into_iter().map(|folder| (workspace, folder)));
    }

    /// The folders of groups that went because their last member left, each
    /// with its workspace, since they were last taken. Their roots are the
    /// window's to close — decision 18.
    pub fn take_released_group_folders(&mut self) -> Vec<(WorkspaceId, PathBuf)> {
        std::mem::take(&mut self.released_group_folders)
    }

    /// Bind a group to a folder, opening the folder as a root of the
    /// workspace, and answer that root. A folder that is a root of one of
    /// the group's own repositories, or another group's folder, is refused.
    /// Whether the folder exists is the caller's to check: this crate does
    /// not read the disk.
    pub fn bind_project_group_folder(
        &mut self,
        workspace: WorkspaceId,
        id: ProjectGroupId,
        folder: &Path,
    ) -> Result<crate::ProjectId, FolderRefused> {
        let holder = self.workspace(workspace).ok_or(FolderRefused::NoGroup)?;
        let group = holder.project_group(id).ok_or(FolderRefused::NoGroup)?;
        if holder
            .project_groups
            .iter()
            .any(|other| other.id != id && other.folder.as_deref() == Some(folder))
        {
            return Err(FolderRefused::OtherGroup);
        }
        // A directory is a group's folder or a project of a group, never
        // both: in any workspace, a root at this path whose repository is a
        // member of some group refuses it, and so does this group's own
        // members' main checkout.
        let member_root = self.workspaces.iter().any(|other| {
            self.projects_of(other.id).any(|project| {
                project.path == folder && other.project_group_of(repository_key(project)).is_some()
            })
        });
        if member_root
            || group
                .repositories
                .iter()
                .any(|key| main_checkout(key) == folder)
        {
            return Err(FolderRefused::OwnRoot);
        }
        if let Some(group) = self
            .holder_mut(workspace)
            .and_then(|holder| holder.project_groups.iter_mut().find(|g| g.id == id))
        {
            group.folder = Some(folder.to_path_buf());
        }
        Ok(self.open_in(workspace, folder))
    }

    /// The workspace and group whose folder a root is. **A directory's
    /// role, not a seat's** — decision 23: a path bound as a group's folder
    /// in any workspace, open there or not, makes the root a group root
    /// wherever it is open.
    pub fn project_group_root(
        &self,
        project: crate::ProjectId,
    ) -> Option<(WorkspaceId, ProjectGroupId)> {
        let path = &self.project(project)?.path;
        self.bound_folder(path)
    }

    /// The workspace and group bound to a folder, in any workspace.
    pub fn bound_folder(&self, folder: &Path) -> Option<(WorkspaceId, ProjectGroupId)> {
        self.workspaces.iter().find_map(|holder| {
            holder
                .project_group_of_folder(folder)
                .map(|group| (holder.id, group.id))
        })
    }

    /// Whether a root is a project group's folder — shown, not indexed:
    /// no file or symbol index, no language server and no git status worker
    /// run over it (decisions 7 and 9).
    pub fn is_project_group_root(&self, project: crate::ProjectId) -> bool {
        self.project_group_root(project).is_some()
    }

    fn holder_mut(&mut self, workspace: WorkspaceId) -> Option<&mut Workspace> {
        self.workspaces
            .iter_mut()
            .find(|holder| holder.id == workspace)
    }

    /// Whether a workspace holds a root of the repository kept by `key`.
    /// A group's folder root does not count: it is never what holds a
    /// member, so never what keeps a group alive.
    fn holds_repository(&self, workspace: WorkspaceId, key: &Path) -> bool {
        self.projects_of(workspace).any(|project| {
            repository_key(project) == key && self.bound_folder(&project.path).is_none()
        })
    }

    /// Whether `key` is the repository of a directory bound as a group's
    /// folder in any workspace: such a repository is never admitted as a
    /// member (decision 23).
    fn is_folder_repository(&self, key: &Path) -> bool {
        self.projects().any(|project| {
            repository_key(project) == key && self.bound_folder(&project.path).is_some()
        })
    }

    /// Make a group named `name` with the repository kept by `key` as its
    /// first member, taking the repository out of the group it was in.
    /// A blank name, or a repository the workspace does not hold, is refused.
    pub fn create_project_group(
        &mut self,
        workspace: WorkspaceId,
        name: &str,
        key: &Path,
    ) -> Option<ProjectGroupId> {
        let name = name.trim();
        if name.is_empty()
            || !self.holds_repository(workspace, key)
            || self.is_folder_repository(key)
        {
            return None;
        }
        let mut released = Vec::new();
        let holder = self.holder_mut(workspace)?;
        forget(&mut holder.project_groups, key, &mut released);
        self.release_group_folders(workspace, released);
        let holder = self.holder_mut(workspace)?;
        // A group made now stands where its repository does; nothing older
        // is drawn by the creation-order rule any more.
        holder.groups_in_order = true;
        let id = ProjectGroupId::new();
        holder.project_groups.push(ProjectGroup {
            id,
            name: name.to_owned(),
            repositories: vec![key.to_path_buf()],
            collapsed: false,
            folder: None,
        });
        self.gather_project_groups(workspace);
        Some(id)
    }

    /// Put the repository kept by `key` into an existing group, or with
    /// `None` take it out of the one it is in. A group the move empties is
    /// gone. Answers whether anything changed.
    pub fn move_to_project_group(
        &mut self,
        workspace: WorkspaceId,
        key: &Path,
        to: Option<ProjectGroupId>,
    ) -> bool {
        if to.is_some()
            && (!self.holds_repository(workspace, key) || self.is_folder_repository(key))
        {
            return false;
        }
        let Some(holder) = self.holder_mut(workspace) else {
            return false;
        };
        let current = holder.project_group_of(key).map(|group| group.id);
        if current == to {
            return false;
        }
        if let Some(to) = to
            && holder.project_group(to).is_none()
        {
            return false;
        }
        let mut released = Vec::new();
        forget(&mut holder.project_groups, key, &mut released);
        if let Some(to) = to
            && let Some(group) = holder
                .project_groups
                .iter_mut()
                .find(|group| group.id == to)
        {
            group.repositories.push(key.to_path_buf());
            self.gather_project_group(workspace, to, key);
        }
        self.release_group_folders(workspace, released);
        self.gather_project_groups(workspace);
        true
    }

    /// Make `order` — the same roots, rearranged — the workspace's manual
    /// order and switch it to Manual: what a sort mode shows becomes what
    /// the person arranges from. Anything but a rearrangement of the roots
    /// the workspace holds is refused.
    pub fn adopt_project_order(
        &mut self,
        workspace: WorkspaceId,
        order: &[crate::ProjectId],
    ) -> bool {
        let Some(holder) = self.holder_mut(workspace) else {
            return false;
        };
        let given: std::collections::HashSet<_> = order.iter().collect();
        let held: std::collections::HashSet<_> = holder.projects.iter().collect();
        if order.len() != holder.projects.len() || given != held {
            return false;
        }
        holder.projects = order.to_vec();
        holder.project_sort = crate::project_sort::Mode::Manual;
        self.gather_project_groups(workspace);
        true
    }

    /// The roots of `workspace` whose repository is a member of `id`, in
    /// the workspace's order.
    pub fn project_group_roots(
        &self,
        workspace: WorkspaceId,
        id: ProjectGroupId,
    ) -> Vec<crate::ProjectId> {
        let Some(group) = self.workspace(workspace).and_then(|w| w.project_group(id)) else {
            return Vec::new();
        };
        self.projects_of(workspace)
            .filter(|project| {
                group
                    .repositories
                    .iter()
                    .any(|known| known == repository_key(project))
            })
            .map(|project| project.id)
            .collect()
    }

    /// Put the roots of the repository kept by `key` right after the last
    /// root of the group's other members, so the group stays where it
    /// stood rather than following the newcomer.
    fn gather_project_group(&mut self, workspace: WorkspaceId, id: ProjectGroupId, key: &Path) {
        let (joining, others): (Vec<_>, Vec<_>) = self
            .project_group_roots(workspace, id)
            .into_iter()
            .partition(|project| {
                self.project(*project)
                    .is_some_and(|project| repository_key(project) == key)
            });
        let Some(holder) = self.holder_mut(workspace) else {
            return;
        };
        let Some(last) = holder
            .projects
            .iter()
            .rposition(|project| others.contains(project))
        else {
            return;
        };
        let anchor = holder.projects[last];
        holder.projects.retain(|project| !joining.contains(project));
        let at = holder
            .projects
            .iter()
            .position(|project| *project == anchor)
            .map_or(holder.projects.len(), |at| at + 1);
        holder.projects.splice(at..at, joining);
    }

    /// Keep each top-level item's roots in one block of `projects`, at the
    /// place of its first root — a group's, with each of its repositories'
    /// roots together inside it, and an ungrouped repository's. A root that
    /// arrived later — a worktree opened after the group was made, one whose
    /// repository git named afterwards, one moved in from another workspace —
    /// moves up beside the others. Nothing drawn moves, since a group and a
    /// repository each stand at their first root; what it keeps right is the
    /// block a dragged item is placed against, and an item's place when its
    /// first root is closed. Only a workspace with groups is touched.
    pub(crate) fn gather_project_groups(&mut self, workspace: WorkspaceId) {
        let Some(at) = self.workspaces.iter().position(|w| w.id == workspace) else {
            return;
        };
        let holder = &self.workspaces[at];
        if holder.project_groups.is_empty() {
            return;
        }
        let keys: std::collections::HashMap<crate::ProjectId, &Path> = holder
            .projects
            .iter()
            .filter_map(|id| self.project(*id))
            .map(|project| (project.id, repository_key(project)))
            .collect();
        let group_of: std::collections::HashMap<&Path, usize> = holder
            .project_groups
            .iter()
            .enumerate()
            .flat_map(|(at, group)| {
                group
                    .repositories
                    .iter()
                    .map(move |key| (key.as_path(), at))
            })
            .collect();
        // Each top-level item's repositories in the order they first come,
        // and each repository's roots in theirs.
        #[derive(PartialEq, Eq, Hash, Clone, Copy)]
        enum Top<'a> {
            Group(usize),
            Repository(&'a Path),
        }
        let mut tops: Vec<Top<'_>> = Vec::new();
        let mut members: std::collections::HashMap<Top<'_>, Vec<&Path>> = Default::default();
        let mut roots: std::collections::HashMap<&Path, Vec<crate::ProjectId>> = Default::default();
        let mut unknown = Vec::new();
        for project in &holder.projects {
            let Some(key) = keys.get(project).copied() else {
                unknown.push(*project);
                continue;
            };
            let top = group_of
                .get(key)
                .map_or(Top::Repository(key), |group| Top::Group(*group));
            let listed = members.entry(top).or_insert_with(|| {
                tops.push(top);
                Vec::new()
            });
            let held = roots.entry(key).or_insert_with(|| {
                listed.push(key);
                Vec::new()
            });
            held.push(*project);
        }
        let mut gathered = Vec::with_capacity(holder.projects.len());
        for top in &tops {
            for key in &members[top] {
                gathered.extend(&roots[key]);
            }
        }
        // A root the desk does not know is normalisation's to drop.
        gathered.extend(unknown);
        self.workspaces[at].projects = gathered;
    }

    /// State written before groups shared the project order drew them all
    /// above the ungrouped repositories, in creation order. Put each such
    /// workspace's `projects` into that shape once — every group's roots in
    /// creation order, then the rest, each keeping its own order — so it
    /// looks as it did until the person moves something.
    pub(crate) fn put_groups_in_order(&mut self) {
        for at in 0..self.workspaces.len() {
            let holder = &self.workspaces[at];
            if holder.groups_in_order || holder.project_groups.is_empty() {
                continue;
            }
            let rank = |project: &crate::ProjectId| {
                self.project(*project)
                    .and_then(|project| {
                        let key = repository_key(project);
                        holder
                            .project_groups
                            .iter()
                            .position(|group| group.repositories.iter().any(|known| known == key))
                    })
                    .unwrap_or(holder.project_groups.len())
            };
            let mut projects = holder.projects.clone();
            // Stable: each group's roots and the ungrouped keep their order.
            projects.sort_by_key(rank);
            let holder = &mut self.workspaces[at];
            holder.projects = projects;
            holder.groups_in_order = true;
        }
        let workspaces: Vec<_> = self.workspaces.iter().map(|w| w.id).collect();
        for workspace in workspaces {
            self.gather_project_groups(workspace);
        }
    }

    /// Call a group something else; a blank name is refused.
    pub fn rename_project_group(
        &mut self,
        workspace: WorkspaceId,
        id: ProjectGroupId,
        name: &str,
    ) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(group) = self
            .holder_mut(workspace)
            .and_then(|holder| holder.project_groups.iter_mut().find(|g| g.id == id))
        else {
            return false;
        };
        if group.name == name {
            return false;
        }
        group.name = name.to_owned();
        true
    }

    /// Delete a group: its repositories return to the ungrouped list, and
    /// no root is closed.
    pub fn delete_project_group(&mut self, workspace: WorkspaceId, id: ProjectGroupId) -> bool {
        let Some(holder) = self.holder_mut(workspace) else {
            return false;
        };
        let before = holder.project_groups.len();
        holder.project_groups.retain(|group| group.id != id);
        holder.project_groups.len() != before
    }

    /// Collapse a group's header over its repositories, or expand it.
    /// Answers whether it is collapsed now.
    pub fn toggle_project_group(&mut self, workspace: WorkspaceId, id: ProjectGroupId) -> bool {
        let Some(group) = self
            .holder_mut(workspace)
            .and_then(|holder| holder.project_groups.iter_mut().find(|g| g.id == id))
        else {
            return false;
        };
        group.collapsed = !group.collapsed;
        group.collapsed
    }

    /// Expand the group holding the repository kept by `key`, so the row
    /// in front is never hidden. Answers whether anything changed.
    pub fn expand_project_group_of(&mut self, workspace: WorkspaceId, key: &Path) -> bool {
        let Some(group) = self.holder_mut(workspace).and_then(|holder| {
            holder
                .project_groups
                .iter_mut()
                .find(|group| group.repositories.iter().any(|known| known == key))
        }) else {
            return false;
        };
        std::mem::replace(&mut group.collapsed, false)
    }

    /// Expand the group bound to `folder`, so its folder's row in front is
    /// never hidden — GLD-055. Answers whether anything changed.
    pub fn expand_project_group_of_folder(
        &mut self,
        workspace: WorkspaceId,
        folder: &Path,
    ) -> bool {
        let Some(group) = self.holder_mut(workspace).and_then(|holder| {
            holder
                .project_groups
                .iter_mut()
                .find(|group| group.folder.as_deref() == Some(folder))
        }) else {
            return false;
        };
        std::mem::replace(&mut group.collapsed, false)
    }

    /// After a root left a workspace: when no other root of its repository
    /// is left there, the repository leaves its group too.
    pub(crate) fn forget_left_repository(&mut self, workspace: WorkspaceId, key: &Path) {
        if self.holds_repository(workspace, key) {
            return;
        }
        let mut released = Vec::new();
        if let Some(holder) = self.holder_mut(workspace) {
            forget(&mut holder.project_groups, key, &mut released);
        }
        self.release_group_folders(workspace, released);
    }

    /// A root's repository key changed — git answered for a root the desk
    /// knew only as a directory. Its group membership follows the new key
    /// wherever no other root still answers to the old one.
    pub(crate) fn rekey_grouped_repository(&mut self, project: crate::ProjectId, old: &Path) {
        let Some(new) = self
            .project(project)
            .map(|project| repository_key(project).to_path_buf())
        else {
            return;
        };
        if new == old {
            return;
        }
        let holders = self.holders(project);
        for workspace in holders {
            if self.holds_repository(workspace, old) {
                continue;
            }
            let Some(holder) = self.holder_mut(workspace) else {
                continue;
            };
            if holder.project_group_of(old).is_none() {
                continue;
            }
            if holder.project_group_of(&new).is_some() {
                // Another root already put the repository somewhere.
                let mut released = Vec::new();
                forget(&mut holder.project_groups, old, &mut released);
                self.release_group_folders(workspace, released);
                continue;
            }
            // In place, so a group whose only member this was stands on
            // under the new key rather than vanishing.
            for known in holder
                .project_groups
                .iter_mut()
                .flat_map(|group| group.repositories.iter_mut())
                .filter(|known| known.as_path() == old)
            {
                *known = new.clone();
            }
        }
    }
}
