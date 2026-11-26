//! Projects and the sessions inside them.
//!
//! That is how work is arranged: there are several projects, each with its
//! own directory, and sessions belong to a project. Before this, sessions
//! lay in one flat list and the directory came from wherever the
//! application was started — so whatever a person worked on, every session
//! ran in the same place.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::session_id::{SessionId, SessionMode};
use crate::store::Normalise;
use crate::transcript::Usage;
use crate::workspace::{DEFAULT_NAME, ICONS, Refused, Released, Seat, Workspace, WorkspaceId};

/// A project identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(Uuid);

impl ProjectId {
    /// A fresh identifier.
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// An identifier that never occurs.
    ///
    /// Sessions from files written before projects existed carry it: the
    /// field is absent there and `serde` substitutes this value. That is
    /// how [`Desk::normalise`] finds who needs a project.
    fn nil() -> Self {
        Self(Uuid::nil())
    }

    fn is_nil(self) -> bool {
        self.0.is_nil()
    }
}

impl Default for ProjectId {
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// A project — a directory being worked on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The identifier.
    pub id: ProjectId,
    /// The directory.
    pub path: PathBuf,
    /// The display name: the last component of the path.
    pub name: String,
    /// The common git directory of the repository this directory is a
    /// worktree of — M12.
    ///
    /// **A project is an opened root, and a root may be one worktree of
    /// several.** Two projects with the same answer are one repository in
    /// the column. Remembered so that the column is drawn grouped from the
    /// first frame; the git thread confirms it. `None` outside git, and
    /// for a file written before M12 until git has been asked — which is
    /// what `default` is for: a field that made an old file unreadable
    /// would lose every project in it.
    #[serde(default)]
    pub repository: Option<PathBuf>,
}

impl Project {
    /// A project from a directory.
    pub fn new(path: &Path) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            // The root or an odd path: show the whole path — that is
            // more honest than an empty name.
            .unwrap_or_else(|| path.to_string_lossy().into_owned());

        Self {
            id: ProjectId::new(),
            path: path.to_path_buf(),
            name,
            repository: None,
        }
    }
}

/// What runs in a tab.
///
/// A kind, not a mode. A Claude session has a conversation, its id,
/// `--resume` and a cost; a shell has none of that — once closed, there is
/// nowhere to come back to. Folding them into one enum of modes would mean
/// fields that are meaningless for half the tabs, and showing a shell a
/// cost counter and a "resume" button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runs {
    /// A Claude Code session in a terminal.
    Claude(SessionMode),
    /// An OpenCode session in a terminal.
    OpenCode(SessionMode),
    /// A Codex session in a terminal.
    Codex(SessionMode),
    /// An ordinary shell in the project directory.
    Shell,
}

impl Runs {
    /// Is this a Claude session specifically?
    pub fn is_claude(self) -> bool {
        matches!(self, Self::Claude(_))
    }

    /// Is this a session of any harness (not a shell)?
    pub fn is_agent(self) -> bool {
        matches!(self, Self::Claude(_) | Self::OpenCode(_) | Self::Codex(_))
    }

    /// Which harness this session runs, if any.
    pub fn harness(self) -> Option<crate::HarnessKind> {
        match self {
            Self::Claude(_) => Some(crate::HarnessKind::Claude),
            Self::OpenCode(_) => Some(crate::HarnessKind::OpenCode),
            Self::Codex(_) => Some(crate::HarnessKind::Codex),
            Self::Shell => None,
        }
    }
}

/// A tab record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRecord {
    /// The identifier; it is also the `claude` conversation id.
    pub id: SessionId,
    /// Which project it belongs to.
    ///
    /// `default` is there for files written before projects existed: the
    /// field is absent, and the session gets one at load time from its own
    /// directory.
    #[serde(default)]
    pub project: ProjectId,
    /// Which workspace owns this tab and conversation.
    #[serde(default)]
    pub workspace: WorkspaceId,
    /// The tab title.
    pub title: String,
    /// The directory the session **ran** in.
    ///
    /// Stored separately from the project directory on purpose: a project
    /// can be moved, but the session ran where it ran, and `--resume`
    /// looks for the transcript at the old path.
    pub cwd: PathBuf,
    /// What runs in the tab.
    ///
    /// `default` is there for files written before shells existed: they
    /// had no kind, only a `mode` field. Every such record is a Claude
    /// session, and the kind is filled in at load time from
    /// `legacy_mode`.
    #[serde(default = "claude_terminal")]
    pub runs: Runs,
    /// The mode from a file in the previous format.
    ///
    /// For migration only: after it the field is empty and never reaches
    /// new files. Two sources of truth about a tab's kind cannot be kept —
    /// they would drift apart.
    #[serde(default, rename = "mode", skip_serializing_if = "Option::is_none")]
    legacy_mode: Option<SessionMode>,
    /// What the session cost in total.
    ///
    /// Legacy saved cost, retained for older builds and used only when
    /// the transcript has not reported a cumulative cost.
    pub cost_usd: f64,
    /// What this conversation is about, from its own transcript.
    ///
    /// Not the terminal's title: `claude` writes an OSC title that belongs
    /// to another conversation as often as to this one. See
    /// [`crate::transcript::name`].
    #[serde(default)]
    pub topic: Option<String>,
    /// What a person named the tab — M12.1.
    ///
    /// **It wins over the topic and over the program's title**: a tab named
    /// *login bug* must not turn into whatever `claude` decides the
    /// conversation is about a minute later. `None` is no name, and the
    /// tab is called what it would be called anyway.
    #[serde(default)]
    pub name: Option<String>,
    /// What the session spent, read from its transcript.
    ///
    /// Includes token usage and an optional cumulative cost.
    #[serde(default)]
    pub usage: Usage,
    /// How much of the transcript has been read.
    ///
    /// Kept between runs so that coming back does not re-read a file that
    /// reaches megabytes. If the file turns out shorter than this — it was
    /// replaced or truncated — the reader starts again from the beginning.
    #[serde(default)]
    pub transcript_offset: u64,
    /// Whether at least one turn happened.
    #[serde(default)]
    pub had_turn: bool,
    /// Whether it is currently shown as a tab.
    #[serde(default = "yes")]
    pub open: bool,
    /// Started by the `guild` command with a task to do — M13.
    ///
    /// **Kept across a restart, because provenance is not a mood of the
    /// window.** A session started with a task may not itself start another
    /// with a task — decision 5 of M13's plan — and that must still hold
    /// after the window is closed and opened again, or the rule is one
    /// restart away from not existing. `default` so that every record
    /// written before M13 reads as what it was: started by a person.
    #[serde(default)]
    pub task: bool,
    /// The harness's own session id, once it has one — M14.
    ///
    /// Guild assigns its own `id`: only Claude takes it (`--session-id`).
    /// OpenCode and Codex generate theirs, and this is where the real id is
    /// kept when the window learns it, so a reopened project can resume the
    /// actual conversation (`--session`, `resume`) rather than open fresh.
    /// `None` until the first turn reports it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub harness_session: Option<String>,
    /// Explicit per-session model/effort choices; old records retain their defaults.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_selection: Option<crate::harness::LaunchSelection>,
    /// Permission policy actually used by this native session; absent on old records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_permission: Option<crate::PermissionMode>,
    /// Proven assignment link; authority is still checked against the workflow journal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_run: Option<crate::workflow::RunId>,
    /// When its program last reported a change of state, in Unix
    /// milliseconds — what the column's Sessions mode ages a row by and
    /// settles an idle tab by. The status board keeps nothing across a
    /// restart, so the record does. `None` on a record written before, or
    /// one that never reported.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_activity: Option<u64>,
}

fn yes() -> bool {
    true
}

fn claude_terminal() -> Runs {
    Runs::Claude(SessionMode::Terminal)
}

impl SessionRecord {
    /// Cumulative transcript cost takes precedence over the legacy saved cost.
    pub fn spent_usd(&self) -> Option<f64> {
        self.usage
            .cost_usd
            .or_else(|| (self.cost_usd > 0.0).then_some(self.cost_usd))
    }

    /// Create a record for a new tab.
    pub fn new(title: String, seat: impl Into<Seat>, cwd: PathBuf, runs: Runs) -> Self {
        let seat = seat.into();
        Self {
            id: SessionId::new(),
            project: seat.project,
            workspace: seat.workspace,
            title,
            cwd,
            runs,
            legacy_mode: None,
            cost_usd: 0.0,
            topic: None,
            name: None,
            usage: Usage::default(),
            transcript_offset: 0,
            had_turn: false,
            open: true,
            // A person opens a tab; only the command line marks one as a
            // task, after it has made the record.
            task: false,
            harness_session: None,
            launch_selection: None,
            launch_permission: None,
            workflow_run: None,
            last_activity: None,
        }
    }

    /// Whether there is a conversation to come back to — what a closed
    /// record is kept for, and what the Sessions panel's Reopen promises.
    ///
    /// Each harness by its own witness: Claude's transcript probe marks a
    /// turn, while OpenCode and Codex report their own id after one.
    pub fn has_conversation(&self) -> bool {
        match self.runs {
            Runs::Claude(_) => self.had_turn,
            Runs::OpenCode(_) | Runs::Codex(_) => self.harness_session.is_some(),
            Runs::Shell => false,
        }
    }
}

/// Everything the desktop knows: projects and their sessions.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Desk {
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) project_activity: std::collections::BTreeMap<PathBuf, crate::project_sort::Activity>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub(crate) activity_cursors: std::collections::BTreeMap<String, crate::project_sort::Cursor>,
    #[serde(default)]
    projects: Vec<Project>,
    /// `alias` is there for files in the previous format: the list was
    /// called `records`. Without it an old file would read as empty and a
    /// person's sessions would vanish silently — exactly what must not
    /// happen.
    #[serde(default, alias = "records")]
    sessions: Vec<SessionRecord>,
    /// What the person was looking at when the window was closed.
    ///
    /// Part of the state rather than of the interface: coming back and
    /// hunting for the right tab again is not the same as coming back to
    /// work.
    #[serde(default)]
    active_project: Option<ProjectId>,
    #[serde(default)]
    active_session: Option<SessionId>,
    /// Directories opened before, newest first.
    ///
    /// Kept apart from `projects` because it outlives them: a project
    /// removed from the desk is exactly the one a person then wants
    /// to reopen, and looking the path up by hand a second time is the
    /// annoyance "Open Recent" exists to remove.
    #[serde(default)]
    recent: Vec<PathBuf>,
    /// What a person named a project — M12.2, *Rename Project*.
    ///
    /// By the repository's common git directory, or by the directory itself
    /// outside git, so every worktree's group agrees on it; the folder's own
    /// name is what a project is called otherwise.
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    names: std::collections::BTreeMap<PathBuf, String>,
    /// The projects a person folded in the column — M12.5, decision 18.
    ///
    /// By the same key as `names`, so every worktree's group agrees; the
    /// repository's and not a desk's: folding belongs to the project.
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    folded: std::collections::BTreeSet<PathBuf>,
    /// Named sets of roots, in switcher order.
    #[serde(default)]
    pub(crate) workspaces: Vec<Workspace>,
    /// The workspace shown when the desk was saved.
    #[serde(default)]
    active_workspace: Option<WorkspaceId>,
    /// Folders of project groups that went because their last member left,
    /// waiting for the window to close their roots — GLD-055 decision 18.
    /// Never saved: a root left open by a restart is an ordinary root.
    #[serde(skip)]
    pub(crate) released_group_folders: Vec<(WorkspaceId, PathBuf)>,
    /// The seats whose records this run has read from their root's own
    /// file — GLD-060. [`crate::persist::save`] replaces a seat's records
    /// on disk with memory's only for these: a seat added since — a root
    /// opened again in a workspace that had closed it — or one whose
    /// folder was away when the desk was read has records memory has
    /// never seen. Never saved: what a run has read is that run's.
    #[serde(skip)]
    pub(crate) loaded: std::collections::HashSet<Seat>,
    /// The records this run removed — GLD-060, review r1. A seat read
    /// late must not bring one back from a copy its file still has: a
    /// record the desk held from the global file while its folder was
    /// away, removed before the folder came back. Never saved.
    #[serde(skip)]
    pub(crate) removed: std::collections::HashSet<SessionId>,
    /// The seats of a root whose folder was away at the save that wrote
    /// this file and which that run had read — GLD-060, review r2. Their
    /// records in this file are the whole of them, so the next run takes
    /// them from here and not from the older copy beside the root, which a
    /// removal made while the folder was away never reached. Written by
    /// [`crate::persist::save`] alone; empty, and absent from the file,
    /// for every root whose folder was there.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) read_away: Vec<(WorkspaceId, ProjectId)>,
}

/// How many directories "Open Recent" keeps.
///
/// A menu one has to read through is no faster than the file dialogue it
/// replaces.
const RECENT_LIMIT: usize = 10;

impl Desk {
    /// Every workspace, in switcher order.
    pub fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    /// A workspace by identifier.
    pub fn workspace(&self, id: WorkspaceId) -> Option<&Workspace> {
        self.workspaces.iter().find(|workspace| workspace.id == id)
    }

    /// Remember a validated path order without opening roots or changing the active seat.
    pub fn set_worktree_order(
        &mut self,
        workspace: WorkspaceId,
        repository: &Path,
        order: Vec<std::path::PathBuf>,
    ) -> bool {
        let Some(workspace) = self.workspaces.iter_mut().find(|item| item.id == workspace) else {
            return false;
        };
        if workspace.worktree_order.get(repository) == Some(&order) {
            return false;
        }
        workspace
            .worktree_order
            .insert(repository.to_path_buf(), order);
        true
    }

    /// Toggle a workspace-local pin without opening or selecting the checkout.
    pub fn toggle_worktree_pin(&mut self, workspace: WorkspaceId, path: &Path) -> bool {
        let Some(workspace) = self.workspaces.iter_mut().find(|item| item.id == workspace) else {
            return false;
        };
        if let Some(index) = workspace
            .pinned_worktrees
            .iter()
            .position(|item| item == path)
        {
            workspace.pinned_worktrees.remove(index);
        } else {
            workspace.pinned_worktrees.push(path.to_path_buf());
        }
        true
    }

    /// Whether a worktree's list of tabs is collapsed in a workspace's
    /// column: every list is, until a person expands it.
    pub fn worktree_tabs_folded(&self, workspace: WorkspaceId, path: &Path) -> bool {
        self.workspace(workspace).is_none_or(|holder| {
            !holder
                .unfolded_worktree_tabs
                .iter()
                .any(|item| item == path)
        })
    }

    /// Expand a worktree's list of tabs in one workspace's column, or
    /// collapse it again. Answers whether it is collapsed now; a workspace
    /// the desk does not hold changes nothing and answers `false`.
    pub fn toggle_worktree_tabs(&mut self, workspace: WorkspaceId, path: &Path) -> bool {
        let Some(holder) = self.workspaces.iter_mut().find(|item| item.id == workspace) else {
            return false;
        };
        if let Some(at) = holder
            .unfolded_worktree_tabs
            .iter()
            .position(|item| item == path)
        {
            holder.unfolded_worktree_tabs.remove(at);
            true
        } else {
            holder.unfolded_worktree_tabs.push(path.to_path_buf());
            false
        }
    }

    /// The selected workspace. A normalised desk always has one.
    pub fn active_workspace(&self) -> WorkspaceId {
        self.active_workspace.unwrap_or_else(WorkspaceId::nil)
    }

    /// Whether a new workspace can be submitted now. Legacy state may exceed this limit.
    pub fn can_add_workspace(&self) -> bool {
        self.workspaces.len() < crate::workspace::CREATION_LIMIT
    }

    /// Add a workspace at the end of the switcher.
    pub fn add_workspace(
        &mut self,
        name: &str,
        icon: Option<&str>,
    ) -> Result<WorkspaceId, Refused> {
        let name = name.trim();
        if name.is_empty() {
            return Err(Refused::BlankName);
        }
        if icon.is_some_and(|icon| !ICONS.contains(&icon)) {
            return Err(Refused::UnknownIcon);
        }
        if !self.can_add_workspace() {
            return Err(Refused::Capacity);
        }
        let id = WorkspaceId::new();
        self.workspaces.push(Workspace {
            id,
            name: name.to_owned(),
            icon: icon.map(str::to_owned),
            projects: Vec::new(),
            project_sort: Default::default(),
            worktree_order: Default::default(),
            pinned_worktrees: Vec::new(),
            unfolded_worktree_tabs: Vec::new(),
            project_groups: Vec::new(),
            groups_in_order: false,
            front: None,
        });
        if self.active_workspace.is_none() {
            self.active_workspace = Some(id);
        }
        Ok(id)
    }

    /// Rename a workspace, refusing an absent id or blank name.
    pub fn rename_workspace(&mut self, id: WorkspaceId, name: &str) -> bool {
        let name = name.trim();
        if name.is_empty() {
            return false;
        }
        let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
        else {
            return false;
        };
        workspace.name = name.to_owned();
        true
    }

    /// Change a workspace icon. Unknown icon names are refused.
    pub fn set_icon(&mut self, id: WorkspaceId, icon: Option<&str>) -> Result<bool, Refused> {
        if icon.is_some_and(|icon| !ICONS.contains(&icon)) {
            return Err(Refused::UnknownIcon);
        }
        let Some(workspace) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.id == id)
        else {
            return Ok(false);
        };
        workspace.icon = icon.map(str::to_owned);
        Ok(true)
    }

    /// Select an existing workspace and restore its front root.
    pub fn select_workspace(&mut self, id: WorkspaceId) -> bool {
        let Some(workspace) = self.workspace(id) else {
            return false;
        };
        let front = workspace.front;
        self.active_workspace = Some(id);
        self.active_project = front;
        true
    }

    /// Open a root in a workspace, reusing the global root for the path.
    pub fn open_in(&mut self, workspace: WorkspaceId, path: &Path) -> ProjectId {
        // A brand-new desk loaded from no file has not passed through a
        // persisted record's normalisation yet. Opening its first project
        // must create the Default workspace here; otherwise the global root
        // exists and its Claude tab opens, but no workspace holds it and the
        // sidebar stays empty.
        let workspace = if self.workspace(workspace).is_some() {
            workspace
        } else if let Some(active) = self
            .active_workspace
            .filter(|active| self.workspace(*active).is_some())
        {
            active
        } else if let Some(first) = self.workspaces.first().map(|workspace| workspace.id) {
            self.active_workspace = Some(first);
            first
        } else {
            self.add_workspace(DEFAULT_NAME, None)
                .expect("the built-in workspace name is valid")
        };
        let project = self.add_project(path);
        if let Some(holder) = self
            .workspaces
            .iter_mut()
            .find(|holder| holder.id == workspace)
        {
            if !holder.projects.contains(&project) {
                holder.projects.push(project);
            }
            holder.front.get_or_insert(project);
        }
        self.gather_project_groups(workspace);
        project
    }

    /// Roots held by a workspace, in its order.
    pub fn projects_of(&self, workspace: WorkspaceId) -> impl Iterator<Item = &Project> {
        let ids = self
            .workspace(workspace)
            .map(|workspace| workspace.projects.as_slice())
            .unwrap_or_default();
        ids.iter().filter_map(|id| self.project(*id))
    }

    /// Change ordering without rewriting the saved manual project order.
    pub fn set_project_sort(
        &mut self,
        workspace: WorkspaceId,
        mode: crate::project_sort::Mode,
    ) -> bool {
        let Some(holder) = self
            .workspaces
            .iter_mut()
            .find(|holder| holder.id == workspace)
        else {
            return false;
        };
        if holder.project_sort == mode {
            return false;
        }
        holder.project_sort = mode;
        true
    }

    /// Put one repository's roots at another repository's place in a workspace.
    ///
    /// `projects` is a group because one repository may have several open
    /// worktrees. Their order inside the group is kept; only the group's place
    /// among its neighbours changes.
    pub fn reorder_projects(
        &mut self,
        workspace: WorkspaceId,
        projects: &[ProjectId],
        target: ProjectId,
    ) -> bool {
        let Some(holder) = self.workspace(workspace) else {
            return false;
        };
        let from = holder.projects.iter().position(|id| projects.contains(id));
        let to = holder.projects.iter().position(|id| *id == target);
        let after = matches!((from, to), (Some(from), Some(to)) if from < to);
        self.reorder_projects_at(workspace, projects, target, after)
    }

    /// Move only this group in the saved manual order. The caller chooses the
    /// side using the displayed order, which can differ under automatic sorting.
    pub fn reorder_projects_at(
        &mut self,
        workspace: WorkspaceId,
        projects: &[ProjectId],
        target: ProjectId,
        after: bool,
    ) -> bool {
        let Some(holder) = self
            .workspaces
            .iter_mut()
            .find(|holder| holder.id == workspace)
        else {
            return false;
        };
        if projects.is_empty()
            || projects.contains(&target)
            || !projects
                .iter()
                .all(|project| holder.projects.contains(project))
        {
            return false;
        }
        if !holder.projects.contains(&target) {
            return false;
        }
        let moving: Vec<_> = holder
            .projects
            .iter()
            .copied()
            .filter(|project| projects.contains(project))
            .collect();
        holder
            .projects
            .retain(|project| !projects.contains(project));
        let target_now = holder
            .projects
            .iter()
            .position(|project| *project == target)
            .unwrap_or(holder.projects.len());
        let at = target_now + usize::from(after);
        holder.projects.splice(at..at, moving);
        holder.project_sort = crate::project_sort::Mode::Manual;
        true
    }

    /// Move roots, their sessions and their place in front from one workspace
    /// to another. The destination must not already hold any of the roots: a
    /// move is not an implicit merge of two independently arranged seats.
    pub fn move_projects(
        &mut self,
        from: WorkspaceId,
        to: WorkspaceId,
        projects: &[ProjectId],
    ) -> bool {
        if from == to || projects.is_empty() {
            return false;
        }
        let Some(from_at) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == from)
        else {
            return false;
        };
        let Some(to_at) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == to)
        else {
            return false;
        };
        if !projects
            .iter()
            .all(|project| self.workspaces[from_at].projects.contains(project))
            || projects
                .iter()
                .any(|project| self.workspaces[to_at].projects.contains(project))
        {
            return false;
        }

        let keys: Vec<PathBuf> = projects
            .iter()
            .filter_map(|project| self.project(*project))
            .map(|project| crate::project_group::repository_key(project).to_path_buf())
            .collect();
        let moving: Vec<_> = self.workspaces[from_at]
            .projects
            .iter()
            .copied()
            .filter(|project| projects.contains(project))
            .collect();
        self.workspaces[from_at]
            .projects
            .retain(|project| !projects.contains(project));
        if self.workspaces[from_at]
            .front
            .is_some_and(|project| projects.contains(&project))
        {
            self.workspaces[from_at].front = self.workspaces[from_at].projects.first().copied();
        }
        self.workspaces[to_at]
            .projects
            .extend(moving.iter().copied());
        self.workspaces[to_at].front.get_or_insert(moving[0]);
        for session in &mut self.sessions {
            if session.workspace == from && projects.contains(&session.project) {
                session.workspace = to;
            }
        }
        // The destination's seat is not read by the move: a workspace that
        // closed the root kept its records beside it, and the next save
        // reads them before it replaces anything of the seat — review r1.
        for project in &moving {
            self.loaded.remove(&Seat {
                workspace: from,
                project: *project,
            });
        }
        if self.active_workspace == Some(from)
            && self
                .active_project
                .is_some_and(|project| projects.contains(&project))
        {
            self.active_project = self.workspaces[from_at].front;
        }
        for key in keys {
            self.forget_left_repository(from, &key);
        }
        self.gather_project_groups(to);
        true
    }

    /// Workspaces holding a root.
    pub fn holders(&self, project: ProjectId) -> Vec<WorkspaceId> {
        self.workspaces
            .iter()
            .filter(|workspace| workspace.projects.contains(&project))
            .map(|workspace| workspace.id)
            .collect()
    }

    /// Remove a root from one workspace.
    pub fn leave(&mut self, workspace: WorkspaceId, project: ProjectId) -> Released {
        let key = self
            .project(project)
            .map(|project| crate::project_group::repository_key(project).to_path_buf());
        if let Some(holder) = self
            .workspaces
            .iter_mut()
            .find(|holder| holder.id == workspace)
        {
            holder.projects.retain(|known| *known != project);
            if holder.front == Some(project) {
                holder.front = holder.projects.first().copied();
            }
        }
        self.sessions
            .retain(|session| session.workspace != workspace || session.project != project);
        self.loaded.remove(&Seat { workspace, project });
        if let Some(key) = key {
            self.forget_left_repository(workspace, &key);
        }
        if self.holders(project).is_empty() {
            self.remove_project(project);
            Released::Released
        } else {
            Released::Kept
        }
    }

    /// Delete a workspace, returning roots released with it.
    pub fn delete_workspace(&mut self, id: WorkspaceId) -> Result<Vec<ProjectId>, Refused> {
        if self.workspaces.len() == 1 {
            return Err(Refused::Last);
        }
        let Some(position) = self
            .workspaces
            .iter()
            .position(|workspace| workspace.id == id)
        else {
            return Ok(Vec::new());
        };
        let projects = self.workspaces[position].projects.clone();
        self.workspaces.remove(position);
        self.sessions.retain(|session| session.workspace != id);
        self.loaded.retain(|seat| seat.workspace != id);
        let mut released = Vec::new();
        for project in projects {
            if self.holders(project).is_empty() {
                self.remove_project(project);
                released.push(project);
            }
        }
        if self.active_workspace == Some(id) {
            let next = self.workspaces[position.min(self.workspaces.len() - 1)].id;
            self.select_workspace(next);
        }
        Ok(released)
    }
    /// The project the person was looking at.
    pub fn active_project(&self) -> Option<ProjectId> {
        self.active_project
    }

    /// The session the person was looking at.
    pub fn active_session(&self) -> Option<SessionId> {
        self.active_session
    }

    /// Remember what the person is looking at.
    pub fn look_at(&mut self, project: Option<ProjectId>, session: Option<SessionId>) {
        self.active_project = project;
        self.active_session = session;
        if let Some(workspace) = self.active_workspace.and_then(|id| {
            self.workspaces
                .iter_mut()
                .find(|workspace| workspace.id == id)
        }) {
            workspace.front = project;
        }
    }

    /// Every project.
    pub fn projects(&self) -> impl Iterator<Item = &Project> {
        self.projects.iter()
    }

    /// A project by its identifier.
    pub fn project(&self, id: ProjectId) -> Option<&Project> {
        self.projects.iter().find(|project| project.id == id)
    }

    /// Add a project and return its identifier.
    ///
    /// The same directory gives the same project: adding it again would
    /// breed twins, and a person would not know which one holds their
    /// sessions.
    pub fn add_project(&mut self, path: &Path) -> ProjectId {
        self.remember_recent(path);

        if let Some(existing) = self.projects.iter().find(|project| project.path == path) {
            return existing.id;
        }

        let project = Project::new(path);
        let id = project.id;
        self.projects.push(project);
        id
    }

    /// Record which repository a project's root belongs to — M12.
    ///
    /// Answers whether anything changed, so the window saves only when git
    /// said something new: it is asked every few seconds and nearly always
    /// says the same.
    pub fn set_repository(&mut self, project: ProjectId, repository: Option<PathBuf>) -> bool {
        match self.projects.iter_mut().find(|known| known.id == project) {
            Some(known) if known.repository != repository => {
                let old = crate::project_group::repository_key(known).to_path_buf();
                known.repository = repository;
                self.rekey_grouped_repository(project, &old);
                for workspace in self.holders(project) {
                    self.gather_project_groups(workspace);
                }
                true
            }
            _ => false,
        }
    }

    /// Directories opened before, newest first.
    pub fn recent(&self) -> impl Iterator<Item = &Path> {
        self.recent.iter().map(PathBuf::as_path)
    }

    /// Put a directory at the head of the recent list.
    ///
    /// Reopening one moves it up rather than adding a twin: a list with
    /// the same directory three times is a list nobody reads.
    fn remember_recent(&mut self, path: &Path) {
        self.recent.retain(|known| known != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(RECENT_LIMIT);
    }

    /// Forget the recent list.
    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// Forget a project together with its sessions.
    ///
    /// The sessions go with it: without a project they are unreachable,
    /// and an invisible record is nothing but a source of confusion in the
    /// counter.
    pub fn remove_project(&mut self, id: ProjectId) {
        self.projects.retain(|project| project.id != id);
        self.sessions.retain(|session| session.project != id);
        self.loaded.retain(|seat| seat.project != id);
        self.forget_missing_selection();
    }

    /// The project's sessions.
    pub fn sessions_of(&self, seat: impl Into<Seat>) -> impl Iterator<Item = &SessionRecord> {
        let seat = seat.into();
        self.sessions.iter().filter(move |session| {
            session.project == seat.project
                && (seat.workspace.is_nil() || session.workspace == seat.workspace)
        })
    }

    /// The sessions in one root, across every workspace.
    pub fn sessions_of_root(&self, project: ProjectId) -> impl Iterator<Item = &SessionRecord> {
        self.sessions
            .iter()
            .filter(move |session| session.project == project)
    }

    /// The project's open sessions — what the tabs show.
    pub fn open_sessions_of(&self, seat: impl Into<Seat>) -> impl Iterator<Item = &SessionRecord> {
        self.sessions_of(seat).filter(|session| session.open)
    }

    /// Every session.
    pub fn sessions(&self) -> impl Iterator<Item = &SessionRecord> {
        self.sessions.iter()
    }

    /// Retain a coordinator-verified assigned-child navigation link.
    pub fn record_workflow_run(&mut self, id: SessionId, run: crate::workflow::RunId) {
        self.with_session(id, |record| record.workflow_run = Some(run));
    }

    /// A session by its identifier.
    pub fn session(&self, id: SessionId) -> Option<&SessionRecord> {
        self.sessions.iter().find(|session| session.id == id)
    }

    /// A session by its identifier, to change in place — the crate's own
    /// bookkeeping; the window changes records through named deeds.
    pub(crate) fn session_mut(&mut self, id: SessionId) -> Option<&mut SessionRecord> {
        self.sessions.iter_mut().find(|session| session.id == id)
    }

    /// Add a session.
    pub fn add_session(&mut self, mut record: SessionRecord) -> SessionId {
        if record.workspace.is_nil() {
            record.workspace = self.active_workspace();
        }
        let id = record.id;
        self.removed.remove(&id);
        self.sessions.push(record);
        id
    }

    /// Close a tab, keeping the session.
    pub fn close_session(&mut self, id: SessionId) {
        self.with_session(id, |session| session.open = false);
    }

    /// Show a closed session as a tab again.
    pub fn reopen_session(&mut self, id: SessionId) {
        self.with_session(id, |session| session.open = true);
    }

    /// Forget a session entirely.
    pub fn remove_session(&mut self, id: SessionId) {
        self.sessions.retain(|session| session.id != id);
        self.removed.insert(id);
        self.forget_missing_selection();
    }

    /// Mark a session as started with a task by the command line — M13.
    pub fn mark_task(&mut self, id: SessionId) {
        self.with_session(id, |session| session.task = true);
    }

    /// Mark that a turn happened.
    pub fn mark_turn(&mut self, id: SessionId) {
        self.with_session(id, |session| session.had_turn = true);
    }

    /// Take the sessions out, leaving the projects behind.
    ///
    /// For the split between what is written globally and what is written
    /// beside each project — see [`crate::persist`]. In memory a desk
    /// is still one thing, because that is what the window addresses.
    pub fn take_sessions(&mut self) -> Vec<SessionRecord> {
        std::mem::take(&mut self.sessions)
    }

    /// Put sessions back, from wherever they were read.
    pub fn absorb_sessions(&mut self, sessions: impl IntoIterator<Item = SessionRecord>) {
        self.sessions.extend(sessions);
    }

    /// Put a root's sessions read after the desk was — GLD-060 — ahead of
    /// the ones memory already holds of it: they were made before those,
    /// and the order is the one a seat from before tab lists restores in.
    pub(crate) fn absorb_earlier(&mut self, project: ProjectId, sessions: Vec<SessionRecord>) {
        let at = self
            .sessions
            .iter()
            .position(|session| session.project == project)
            .unwrap_or(self.sessions.len());
        self.sessions.splice(at..at, sessions);
    }

    /// What a person named the project kept by `key`, if they named it.
    pub fn repository_name(&self, key: &Path) -> Option<&str> {
        self.names.get(key).map(String::as_str)
    }

    /// Name a project as a person typed it — trimmed, and taken away when
    /// nothing is left.
    pub fn rename_repository(&mut self, key: &Path, name: &str) {
        let name = name.trim();
        if name.is_empty() {
            self.names.remove(key);
        } else {
            self.names.insert(key.to_path_buf(), name.to_owned());
        }
    }

    /// Whether the project kept by `key` is folded in the column.
    pub fn is_folded(&self, key: &Path) -> bool {
        self.folded.contains(key)
    }

    /// Fold the project kept by `key`, or open it; answers whether it is
    /// folded now.
    pub fn toggle_fold(&mut self, key: &Path) -> bool {
        if self.folded.remove(key) {
            false
        } else {
            self.folded.insert(key.to_path_buf());
            true
        }
    }

    /// Open the project kept by `key`, whatever it was.
    pub fn unfold(&mut self, key: &Path) {
        self.folded.remove(key);
    }

    /// Name a tab as a person typed it — trimmed, and taken away when
    /// nothing is left.
    pub fn rename_session(&mut self, id: SessionId, name: &str) {
        let name = name.trim();
        let name = (!name.is_empty()).then(|| name.to_owned());
        self.with_session(id, |session| session.name = name);
    }

    /// Record what the conversation turned out to be about.
    pub fn name_session(&mut self, id: SessionId, topic: String) {
        self.with_session(id, |session| session.topic = Some(topic));
    }

    /// Record what was read from a transcript.
    pub fn record_usage(&mut self, id: SessionId, usage: Usage, offset: u64) {
        self.with_session(id, |session| {
            session.usage.absorb(usage);
            session.transcript_offset = offset;
        });
    }

    /// Record the harness's own session id, once the first turn reports it —
    /// M14. What lets a reopened OpenCode or Codex resume its real
    /// conversation.
    pub fn record_harness_session(&mut self, id: SessionId, harness_session: String) {
        self.with_session(id, |session| {
            session.harness_session = Some(harness_session)
        });
    }

    /// Everything every session has spent.
    ///
    /// Tokens add up across sessions. Money does too — but `Usage::absorb`
    /// replaces cost rather than adding it, because within one session
    /// `totalCostUSD` is a running total. So the sum is taken here by
    /// hand, choosing one authoritative cost per record.
    pub fn total_usage(&self) -> Usage {
        let mut total = Usage::default();
        let mut money = 0.0;
        let mut any_money = false;

        for session in &self.sessions {
            let mut tokens = session.usage;
            tokens.cost_usd = None;
            if let Some(cost) = session.spent_usd() {
                money += cost;
                any_money = true;
            }
            total.absorb(tokens);
        }

        total.cost_usd = any_money.then_some(money);
        total
    }

    /// Forget closed sessions there is no returning to.
    ///
    /// Every launch creates a tab; if nobody said a word to it, no turn
    /// happened and `--resume` returns to nothing. Such records would pile
    /// up with every launch and hide the real sessions. Records with money
    /// spent are left alone: a counter that forgets spending is worse than
    /// a long list.
    pub fn forget_unresumable(&mut self) {
        let removed = &mut self.removed;
        self.sessions.retain(|session| {
            // There is nowhere to return to in a closed shell: it has no
            // conversation, while a row in the list would promise
            // otherwise. An agent of any harness is kept for its
            // conversation — not Claude's alone, since M14.
            let resumable =
                session.has_conversation() || (session.runs.is_agent() && session.cost_usd > 0.0);
            let kept = session.open || resumable;
            if !kept {
                removed.insert(session.id);
            }
            kept
        });
        self.forget_missing_selection();
    }

    /// Clear the selection of something that no longer exists.
    ///
    /// Otherwise the window opens pointing at a session that is gone,
    /// which looks like lost work even though there was nothing to lose.
    fn forget_missing_selection(&mut self) {
        if self
            .active_project
            .is_some_and(|id| self.project(id).is_none())
        {
            self.active_project = None;
        }
        if self
            .active_session
            .is_some_and(|id| self.session(id).is_none())
        {
            self.active_session = None;
        }
    }

    /// Mark every session as closed: no process survived the restart.
    pub fn close_everything(&mut self) {
        for session in &mut self.sessions {
            session.open = false;
        }
    }

    /// Put the state in order after loading.
    ///
    /// Called by the store rather than by the caller: forgetting the call
    /// is easy, and the consequence is lost sessions.
    ///
    /// Files written before projects existed keep a flat list of sessions,
    /// each with its own directory. Reading such a file and silently
    /// discarding it would mean losing sessions — the very thing
    /// `--resume` applies to. So a session without a project gets one from
    /// its own directory, and sessions from the same directory share
    /// one.
    pub fn normalise(&mut self) {
        let orphans: Vec<(SessionId, PathBuf)> = self
            .sessions
            .iter()
            .filter(|session| session.project.is_nil())
            .map(|session| (session.id, session.cwd.clone()))
            .collect();

        for (id, cwd) in orphans {
            let project = self.add_project(&cwd);
            self.with_session(id, |session| session.project = project);
        }

        // A session whose project is gone is unreachable: its directory
        // is still known, so the project is restored rather than lost.
        let stray: Vec<(SessionId, PathBuf)> = self
            .sessions
            .iter()
            .filter(|session| !self.projects.iter().any(|p| p.id == session.project))
            .map(|session| (session.id, session.cwd.clone()))
            .collect();

        for (id, cwd) in stray {
            let project = self.add_project(&cwd);
            self.with_session(id, |session| session.project = project);
        }

        // Files written before shells existed keep a mode instead of a
        // kind. Every such record is a Claude session: there were no
        // shells back then.
        for session in &mut self.sessions {
            if let Some(mode) = session.legacy_mode.take() {
                session.runs = Runs::Claude(mode);
            }
        }

        if self.workspaces.is_empty() {
            let id = WorkspaceId::new();
            self.workspaces.push(Workspace {
                id,
                name: DEFAULT_NAME.to_owned(),
                icon: None,
                projects: self.projects.iter().map(|project| project.id).collect(),
                project_sort: Default::default(),
                worktree_order: Default::default(),
                pinned_worktrees: Vec::new(),
                unfolded_worktree_tabs: Vec::new(),
                project_groups: Vec::new(),
                groups_in_order: false,
                front: self.active_project,
            });
            self.active_workspace = Some(id);
        }

        let known: std::collections::HashSet<_> =
            self.projects.iter().map(|project| project.id).collect();
        for workspace in &mut self.workspaces {
            workspace.projects.retain(|project| known.contains(project));
            if workspace
                .front
                .is_some_and(|front| !workspace.projects.contains(&front))
            {
                workspace.front = workspace.projects.first().copied();
            }
        }
        let named: std::collections::HashSet<_> = self
            .workspaces
            .iter()
            .flat_map(|workspace| workspace.projects.iter().copied())
            .collect();
        let unnamed: Vec<_> = self
            .projects
            .iter()
            .map(|project| project.id)
            .filter(|project| !named.contains(project))
            .collect();
        self.workspaces[0].projects.extend(unnamed);
        if self.workspaces[0].front.is_none() {
            self.workspaces[0].front = self.workspaces[0].projects.first().copied();
        }
        if self
            .active_workspace
            .is_none_or(|id| !self.workspaces.iter().any(|workspace| workspace.id == id))
        {
            self.active_workspace = Some(self.workspaces[0].id);
        }
        self.put_groups_in_order();
        let first = self.workspaces[0].id;
        for session in &mut self.sessions {
            if session.workspace.is_nil() {
                session.workspace = first;
            }
        }
        self.active_project = self
            .workspace(self.active_workspace())
            .and_then(|workspace| workspace.front);
    }

    fn with_session<F: FnOnce(&mut SessionRecord)>(&mut self, id: SessionId, change: F) {
        if let Some(session) = self.sessions.iter_mut().find(|session| session.id == id) {
            change(session);
        }
    }
}

impl Normalise for Desk {
    fn normalise(&mut self) {
        Desk::normalise(self);
    }
}
