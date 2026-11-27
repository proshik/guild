//! What the column does: which rows there are, and what a click on one
//! means — M12.
//!
//! The model is [`crate::column`], and it is pure; this is the half that
//! needs the window. A row of a worktree nobody has opened becomes a root
//! on its first click, which is the moment its watcher, its git worker and
//! its language servers start — nothing is paid for a worktree a person is
//! only looking at.

use gpui::Context;
use guild_session::ProjectId;

use crate::column::{Group, Row, groups};
use crate::{Root, column};

/// One tab under a worktree's row in the column.
#[derive(Debug, Clone)]
pub(crate) struct ColumnTab {
    pub id: crate::TabId,
    /// The session a click brings forward.
    pub session: guild_session::SessionId,
    /// Whose mark it wears: its agent's, or a terminal's.
    pub harness: guild_session::DefaultTab,
    /// What its strip calls it.
    pub label: String,
    /// Its most urgent pane's status, as its strip shows it.
    pub status: guild_session::status::Status,
    /// Whether it is the tab in front.
    pub front: bool,
}

/// Row drags carry paths, never opened-root IDs or project-header payloads.
#[derive(Debug, Clone)]
pub(crate) struct DragWorktree {
    pub workspace: guild_session::WorkspaceId,
    pub repository: std::path::PathBuf,
    pub path: std::path::PathBuf,
}

impl Root {
    /// The same active checkout the column's keyboard stroke advertises.
    pub(crate) fn copy_worktree_path(&self, cx: &mut Context<Root>) {
        if let Some(project) = self.active_project.and_then(|id| self.desk.project(id)) {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                project.path.display().to_string(),
            ));
        }
    }

    /// Reuse the menu's guard and confirmation; never remove an unknown primary.
    pub(crate) fn remove_active_worktree(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let Some(project) = self.active_project.and_then(|id| self.desk.project(id)) else {
            return;
        };
        let id = project.id;
        let path = project.path.clone();
        let primary = column::is_primary_checkout(&path, project.repository.as_deref());
        self.remove_worktree_asking(path, Some(id), primary, window, cx);
    }

    pub(crate) fn worktree_pinned(
        &self,
        workspace: guild_session::WorkspaceId,
        path: &std::path::Path,
    ) -> bool {
        self.desk
            .workspace(workspace)
            .is_some_and(|item| item.pinned_worktrees.iter().any(|item| item == path))
    }

    pub(crate) fn toggle_worktree_pin(
        &mut self,
        workspace: guild_session::WorkspaceId,
        path: &std::path::Path,
        cx: &mut Context<Root>,
    ) {
        // Re-resolve the current row so stale controls cannot pin a main branch.
        let eligible = self
            .column_groups_for(workspace)
            .iter()
            .flat_map(|group| &group.rows)
            .any(|row| {
                let title = match row {
                    Row::Opened { title, .. } | Row::Unopened { title, .. } => title,
                };
                self.column_row_path(row).as_deref() == Some(path)
                    && !matches!(title.name.as_str(), "main" | "master")
            });
        if eligible
            && serde_json::to_vec(path).is_ok()
            && self.desk.toggle_worktree_pin(workspace, path)
        {
            self.save();
            cx.notify();
        }
    }

    /// The column's repositories in the order they are drawn, groups and
    /// ungrouped ones sharing one list ([`column::arrange`]).
    /// `cmd-1…9`, `ctrl-tab` and a drag's place all count along this list.
    pub(crate) fn column_groups_for(&self, workspace: guild_session::WorkspaceId) -> Vec<Group> {
        self.column_sections_for(workspace)
            .into_iter()
            .flat_map(|section| section.repositories)
            .collect()
    }

    /// The column's sections: each project group with its repositories, and
    /// the ungrouped ones, in the workspace's mode's order.
    pub(crate) fn column_sections_for(
        &self,
        workspace: guild_session::WorkspaceId,
    ) -> Vec<column::Section> {
        let sorted = self.column_repositories_for(workspace);
        let saved = self
            .desk
            .workspace(workspace)
            .map(|holder| holder.project_groups.as_slice())
            .unwrap_or_default();
        column::arrange(sorted, saved)
    }

    /// The column's repositories, from what the workspace remembers and what
    /// git has said, in the workspace's sort order and regardless of groups.
    pub(crate) fn column_repositories_for(
        &self,
        workspace: guild_session::WorkspaceId,
    ) -> Vec<Group> {
        // A project group's folder is drawn under its group's header, never
        // as a repository of its own — GLD-055.
        let holder = self.desk.workspace(workspace);
        let roots: Vec<column::Opened<'_>> = self
            .desk
            .projects_of(workspace)
            .filter(|project| {
                holder.is_none_or(|holder| holder.project_group_of_folder(&project.path).is_none())
            })
            .map(|project| column::Opened {
                project: project.id,
                path: &project.path,
                name: &project.name,
                repository: project.repository.as_deref(),
            })
            .collect();
        let mut found = groups(&roots, &self.worktrees);
        // Nor as a worktree git lists under some repository: a bound folder
        // that is a linked checkout would be drawn twice, the second time
        // offering its removal.
        if let Some(holder) = holder {
            for group in &mut found {
                group.rows.retain(|row| match row {
                    Row::Unopened { path, .. } => holder.project_group_of_folder(path).is_none(),
                    Row::Opened { .. } => true,
                });
            }
            found.retain(|group| !group.rows.is_empty());
        }
        if let Some(workspace) = self.desk.workspace(workspace) {
            for group in &mut found {
                let paths = self.column_row_paths(group);
                let saved = workspace
                    .worktree_order
                    .get(&group.key)
                    .map(Vec::as_slice)
                    .unwrap_or_default();
                let primary = column::main_root(&group.key);
                let ordered =
                    guild_session::worktree_order::apply(&paths, saved, primary.as_deref());
                group.rows.sort_by_key(|row| {
                    self.column_row_path(row)
                        .and_then(|path| ordered.iter().position(|item| item == &path))
                        .unwrap_or(ordered.len())
                });
            }
        }
        if let Some(holder) = self.desk.workspace(workspace) {
            let activity = |row: &Row| {
                self.column_row_path(row)
                    .map(|path| self.desk.project_activity(&path))
                    .unwrap_or_default()
            };
            let active = |group: &Group| self.active_groups.since(&(holder.id, group.key.clone()));
            column::sort_groups(&mut found, holder.project_sort, activity, active);
        }
        // What the person named a project, where they named one. M12.2.
        column::name_groups(&mut found, |key| {
            self.desk.repository_name(key).map(str::to_owned)
        });
        found
    }

    fn column_row_paths(&self, group: &Group) -> Vec<std::path::PathBuf> {
        group
            .rows
            .iter()
            .filter_map(|row| self.column_row_path(row))
            .collect()
    }

    pub(crate) fn column_row_path(&self, row: &Row) -> Option<std::path::PathBuf> {
        match row {
            Row::Opened { project, .. } => {
                self.desk.project(*project).map(|root| root.path.clone())
            }
            Row::Unopened { path, .. } => Some(path.clone()),
        }
    }

    /// Revalidate both identities and current membership at paint and drop time.
    pub(crate) fn worktree_drop_order(
        &self,
        drag: &DragWorktree,
        workspace: guild_session::WorkspaceId,
        repository: &std::path::Path,
        insertion: &guild_session::worktree_order::Insertion,
    ) -> Option<Vec<std::path::PathBuf>> {
        if drag.workspace != workspace
            || drag.repository != repository
            || !self.worktrees.contains_key(repository)
        {
            return None;
        }
        let group = self
            .column_groups_for(workspace)
            .into_iter()
            .find(|group| !group.plain && group.key == repository)?;
        let paths = self.column_row_paths(&group);
        let target = match insertion {
            guild_session::worktree_order::Insertion::Before(path)
            | guild_session::worktree_order::Insertion::After(path) => path,
        };
        if !drag.path.is_dir() || !target.is_dir() {
            return None;
        }
        let primary = column::main_root(repository);
        guild_session::worktree_order::moved(&paths, primary.as_deref(), &drag.path, insertion)
    }

    pub(crate) fn reorder_worktree(
        &mut self,
        drag: &DragWorktree,
        workspace: guild_session::WorkspaceId,
        repository: &std::path::Path,
        insertion: &guild_session::worktree_order::Insertion,
        cx: &mut Context<Root>,
    ) {
        let Some(order) = self.worktree_drop_order(drag, workspace, repository, insertion) else {
            return;
        };
        // Unopened Git paths can retain bytes serde cannot represent. Validate
        // the whole candidate before it can make the desk unsaveable.
        if serde_json::to_vec(&order).is_err() {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::NoticeWorktreeOrderUnsupportedPath)
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        if self.desk.set_worktree_order(workspace, repository, order) {
            self.save();
            cx.notify();
        }
    }

    /// How many paths git has something to say about in a project — the
    /// number a worktree's row carries.
    ///
    /// The count of marked paths rather than of lines: the status map is
    /// what git says about paths, and a row showing line counts would be
    /// showing a number the window does not have.
    pub(crate) fn column_changes(&self, project: ProjectId) -> usize {
        match self.git_status.get(&project) {
            Some(Ok(map)) => map.marked(),
            _ => 0,
        }
    }

    /// Deleting a directory affects every workspace that holds it.
    pub(crate) fn worktree_busy(&self, project: ProjectId) -> bool {
        self.board
            .summary(self.desk.sessions_of_root(project).map(|record| record.id))
            .busy()
    }

    /// The most urgent thing any opened row of a group is doing — what a
    /// folded project's head wears, since its rows are not drawn to say it.
    /// M12.5 decision 18.
    pub(crate) fn column_group_status_in(
        &self,
        workspace: guild_session::WorkspaceId,
        group: &Group,
    ) -> guild_session::status::Status {
        let sessions: Vec<_> = column::opened_in(group)
            .flat_map(|project| {
                self.desk
                    .sessions_of(guild_session::Seat { workspace, project })
                    .map(|record| record.id)
            })
            .collect();
        self.board.summary(sessions).primary()
    }

    /// Fold a project in the column, or open it — a click on its head.
    /// M12.5 decision 18.
    pub(crate) fn toggle_group_fold(&mut self, key: &std::path::Path, cx: &mut Context<Root>) {
        self.desk.toggle_fold(key);
        self.save();
        cx.notify();
    }

    /// Which side of the repository holding `target` a dragged repository
    /// lands on: after it when it comes from above, as the column draws them
    /// now. `None` when either is not drawn, or they are the same.
    fn repository_drop_side(
        &self,
        workspace: guild_session::WorkspaceId,
        projects: &[ProjectId],
        target: ProjectId,
    ) -> Option<bool> {
        let shown = self.column_groups_for(workspace);
        let from = shown
            .iter()
            .position(|group| column::opened_in(group).any(|id| projects.contains(&id)))?;
        let to = shown
            .iter()
            .position(|group| column::opened_in(group).any(|id| id == target))?;
        (from != to).then_some(from < to)
    }

    /// Put a repository's roots on the `after` side of the repository holding
    /// `target`, in the workspace's order.
    fn place_repository(
        &mut self,
        workspace: guild_session::WorkspaceId,
        projects: &[ProjectId],
        target: ProjectId,
        after: bool,
        cx: &mut Context<Root>,
    ) {
        let shown = self.column_groups_for(workspace);
        let Some(from) = shown
            .iter()
            .position(|group| column::opened_in(group).any(|id| projects.contains(&id)))
        else {
            return;
        };
        let Some(to) = shown
            .iter()
            .position(|group| column::opened_in(group).any(|id| id == target))
        else {
            return;
        };
        if from == to {
            return;
        }
        // Git discovery or opening another worktree can change membership
        // while the pointer is held. A stale drag must not move half a group.
        let moving: Vec<_> = column::opened_in(&shown[from]).collect();
        if moving.len() != projects.len() || moving.iter().any(|id| !projects.contains(id)) {
            return;
        }
        let targets: Vec<_> = column::opened_in(&shown[to]).collect();
        let Some(holder) = self.desk.workspace(workspace) else {
            return;
        };
        // Target every root of the repository, even when its remembered roots
        // were interleaved before grouping. Keep all unrelated manual order.
        let mut roots = holder.projects.iter().filter(|id| targets.contains(id));
        let target = if after {
            roots.next_back()
        } else {
            roots.next()
        }
        .copied();
        let Some(target) = target else {
            return;
        };
        if self
            .desk
            .reorder_projects_at(workspace, projects, target, after)
        {
            self.save();
            cx.notify();
        }
    }

    /// Move one repository's open roots and workspace-local state together.
    pub(crate) fn move_repository_to_workspace(
        &mut self,
        from: guild_session::WorkspaceId,
        to: guild_session::WorkspaceId,
        projects: &[ProjectId],
        cx: &mut Context<Root>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.move_repository_to_workspace_without_history(from, to, projects, cx);
        self.history.suspended -= 1;
    }

    fn move_repository_to_workspace_without_history(
        &mut self,
        from: guild_session::WorkspaceId,
        to: guild_session::WorkspaceId,
        projects: &[ProjectId],
        cx: &mut Context<Root>,
    ) {
        self.store_tabs();
        self.store_expanded();
        let paths: Vec<_> = projects
            .iter()
            .filter_map(|project| {
                self.desk
                    .project(*project)
                    .map(|root| (*project, root.path.clone()))
            })
            .collect();
        if !self.desk.move_projects(from, to, projects) {
            return;
        }

        for (project, path) in paths {
            let old = guild_session::Seat {
                workspace: from,
                project,
            };
            let new = guild_session::Seat {
                workspace: to,
                project,
            };
            for tab in self.tabs.iter_mut().filter(|tab| tab.seat == old) {
                tab.seat = new;
            }
            if let Some(tree) = self.trees.remove(&old) {
                self.trees.insert(new, tree);
            }
            self.closed.move_seat(old, new);
            self.stage.moved(old, new);

            let mut state = guild_session::project_state::load(&path);
            let first = self
                .desk
                .workspaces()
                .first()
                .map(|workspace| workspace.id)
                .unwrap_or(to);
            let seat = state.seat(from, first);
            state.set_seat(to, first, seat);
            state.forget_seat(from);
            if let Err(err) = guild_session::project_state::save(&path, &state) {
                tracing::warn!(project = %path.display(), %err, "the moved workspace seat was not saved");
            }
        }
        self.active_project = self
            .desk
            .workspace(self.active_workspace)
            .and_then(|workspace| workspace.front);
        self.come_back();
        self.save();
        self.rebuild_menu(cx);
        cx.notify();
    }

    /// Open the project a root is drawn under, and the project group that
    /// project is in, so that the row in front is never folded away.
    /// Answers whether anything changed.
    pub(crate) fn unfold_group_of(&mut self, project: ProjectId) -> bool {
        let Some(key) = self.desk.project(project).map(|project| {
            column::group_key(&project.path, project.repository.as_deref()).to_path_buf()
        }) else {
            return false;
        };
        let folded = self.desk.is_folded(&key);
        self.desk.unfold(&key);
        let collapsed = self
            .desk
            .expand_project_group_of(self.active_workspace, &key);
        // A group's folder in front opens its group too — GLD-055.
        let path = self
            .desk
            .project(project)
            .map(|project| project.path.clone());
        let folder = path.is_some_and(|path| {
            self.desk
                .expand_project_group_of_folder(self.active_workspace, &path)
        });
        folded || collapsed || folder
    }

    /// Whether the column lists `tab` under its worktree's row: it holds a
    /// session, and its strip draws its face — whose session, if it has
    /// one, has a record. `column_tabs` and `column_tab_counts` both ask
    /// this, so a count is always the number of rows drawn. `known` is
    /// the records' ids when the caller has them gathered.
    fn column_lists_tab(
        &self,
        tab: &crate::Tab,
        known: Option<&std::collections::HashSet<guild_session::SessionId>>,
    ) -> bool {
        tab.sessions().next().is_some()
            && tab.face().is_some_and(|face| {
                face.session().is_none_or(|session| match known {
                    Some(known) => known.contains(&session),
                    None => self.desk.session(session).is_some(),
                })
            })
    }

    /// How many tabs `column_tabs` would list under each seat's row, in
    /// one walk over the tabs and without naming any of them: what an
    /// item's height depends on, asked for every item on every frame.
    pub(crate) fn column_tab_counts(
        &self,
    ) -> std::collections::HashMap<guild_session::Seat, usize> {
        let known = self.desk.sessions().map(|record| record.id).collect();
        let mut counts = std::collections::HashMap::new();
        for tab in self
            .tabs
            .iter()
            .filter(|tab| self.column_lists_tab(tab, Some(&known)))
        {
            *counts.entry(tab.seat).or_insert(0) += 1;
        }
        counts
    }

    /// The tabs listed under an opened worktree's row: its session tabs,
    /// in the strips' order, each as its strip draws it.
    pub(crate) fn column_tabs(
        &self,
        workspace: guild_session::WorkspaceId,
        project: ProjectId,
        cx: &Context<Root>,
    ) -> Vec<ColumnTab> {
        let seat = guild_session::Seat { workspace, project };
        // Only this seat's tabs, which are all `worktree_tabs` keeps: the
        // column asks for every row on every frame.
        let facts: Vec<column::TabFact<crate::TabId>> = self
            .tabs
            .iter()
            .filter(|tab| tab.seat == seat)
            .map(|tab| column::TabFact {
                id: tab.id,
                seat: tab.seat,
                area: tab.area,
                session: self.column_lists_tab(tab, None),
            })
            .collect();
        let areas = self
            .stage
            .areas(seat)
            .map(|areas| areas.ids())
            .unwrap_or_default();
        column::worktree_tabs(&facts, seat, &areas)
            .into_iter()
            .filter_map(|id| {
                let tab = self.tabs.iter().find(|tab| tab.id == id)?;
                // The pane in front of the tab, else its first session:
                // the one a click brings forward.
                let session = tab.session().or_else(|| tab.sessions().next())?;
                let (kind, status, label) = self.column_tab_face(tab, cx)?;
                let harness = match kind {
                    crate::Kind::Session(runs) => match runs {
                        guild_session::Runs::Claude(_) => guild_session::DefaultTab::Claude,
                        guild_session::Runs::OpenCode(_) => guild_session::DefaultTab::OpenCode,
                        guild_session::Runs::Codex(_) => guild_session::DefaultTab::Codex,
                        guild_session::Runs::Shell => guild_session::DefaultTab::Terminal,
                    },
                    // A tab whose face is a file beside a session pane.
                    _ => guild_session::DefaultTab::Terminal,
                };
                Some(ColumnTab {
                    id,
                    session,
                    harness,
                    label,
                    status,
                    front: self.active_workspace == workspace
                        && self.active_project == Some(project)
                        && self.active_tab == Some(id),
                })
            })
            .collect()
    }

    /// Expand a worktree's list of tabs in the column, or collapse it
    /// again; it is collapsed until a person expands it.
    pub(crate) fn toggle_worktree_tabs(
        &mut self,
        workspace: guild_session::WorkspaceId,
        path: &std::path::Path,
        cx: &mut Context<Root>,
    ) {
        if self.desk.workspace(workspace).is_none() {
            return;
        }
        self.desk.toggle_worktree_tabs(workspace, path);
        self.save();
        cx.notify();
    }

    /// Collapse a project group over its repositories, or expand it — a
    /// click on its header.
    pub(crate) fn toggle_project_group(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        cx: &mut Context<Root>,
    ) {
        if self
            .desk
            .workspace(workspace)
            .and_then(|w| w.project_group(id))
            .is_none()
        {
            return;
        }
        self.desk.toggle_project_group(workspace, id);
        self.save();
        cx.notify();
    }

    /// Put a repository into a project group, or with `None` take it out of
    /// the one it is in — the menu's *Move to Group*.
    pub(crate) fn move_repository_to_group(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        to: Option<guild_session::project_group::ProjectGroupId>,
        cx: &mut Context<Root>,
    ) {
        if self.desk.move_to_project_group(workspace, key, to) {
            self.save();
            cx.notify();
        }
    }

    /// Delete a project group; its repositories return to the ungrouped
    /// list, and its folder's root is asked to close as *Close Project*
    /// asks — GLD-055 decision 3. The folder on disk is left as it is.
    pub(crate) fn delete_project_group(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let folder = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group(id))
            .and_then(|group| group.folder.clone());
        if self.desk.delete_project_group(workspace, id) {
            self.save();
            let root = folder.and_then(|folder| {
                self.desk
                    .projects_of(workspace)
                    .find(|project| project.path == folder)
                    .map(|project| project.id)
            });
            if let Some(project) = root {
                self.restore_root_tooling(project, cx);
                if workspace == self.active_workspace {
                    self.ask_to_close_project(project, window, cx);
                }
            }
            cx.notify();
        }
    }

    /// Whether the header's `+` has anything to offer — decision 29: a
    /// project of the workspace outside the group, by the rule
    /// [`Root::group_add_choices`] lists them by, without building the list.
    pub(crate) fn group_can_add(
        &self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
    ) -> bool {
        let Some(holder) = self.desk.workspace(workspace) else {
            return false;
        };
        self.desk.projects_of(workspace).any(|project| {
            holder.project_group_of_folder(&project.path).is_none()
                && holder
                    .project_group_of(guild_session::project_group::repository_key(project))
                    .is_none_or(|group| group.id != id)
        })
    }

    /// The header's `+` — decision 16: the workspace's projects that are not
    /// in the group, members of another group included, in the column's
    /// order, as the menu's choices.
    pub(crate) fn group_add_choices(
        &self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
    ) -> Vec<crate::view::menu::column::AddChoice> {
        let Some(holder) = self.desk.workspace(workspace) else {
            return Vec::new();
        };
        self.column_groups_for(workspace)
            .into_iter()
            .filter_map(|repository| {
                let group = holder.project_group_of(&repository.key);
                if group.is_some_and(|group| group.id == id) {
                    return None;
                }
                Some(crate::view::menu::column::AddChoice {
                    key: repository.key,
                    name: repository.name,
                    group: group.map(|group| group.name.clone()),
                })
            })
            .collect()
    }

    /// A dragged repository let go of on `on`: into a project group, out of
    /// one, and — on another repository — to that repository's place.
    /// `target` is an opened root of the repository dropped on.
    pub(crate) fn drop_repository(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        on: column::DropOn,
        target: Option<ProjectId>,
        cx: &mut Context<Root>,
    ) {
        // Membership is read now, not when the drag began: the menu or a
        // removal may have changed it while the pointer was held.
        let from = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group_of(key))
            .map(|group| group.id);
        let Some(landing) = column::landing(from, &on) else {
            return;
        };
        // The side is the one the person saw: joining a group moves the
        // repository's roots beside the group's, which would read a drop
        // from above as one from below.
        let side = target
            .filter(|_| landing.reorder)
            .and_then(|target| self.repository_drop_side(workspace, projects, target));
        if let Some(to) = landing.group {
            self.move_repository_to_group(workspace, key, to, cx);
        }
        if let (Some(target), Some(after)) = (target, side) {
            self.place_repository(workspace, projects, target, after, cx);
        }
    }

    /// The roots of a top-level item of `workspace`, in its order.
    fn top_roots(
        &self,
        workspace: guild_session::WorkspaceId,
        top: &column::Top,
    ) -> Vec<ProjectId> {
        match top {
            column::Top::Group(group) => self.desk.project_group_roots(workspace, *group),
            column::Top::Repository(key) => self
                .desk
                .projects_of(workspace)
                .filter(|project| guild_session::project_group::repository_key(project) == key)
                .map(|project| project.id)
                .collect(),
        }
    }

    /// Put `moving` roots right before or after a top-level item, as one
    /// block in the workspace's order — which is the manual order, so it
    /// switches the column to Manual, as any drag to a place does.
    fn place_beside_top(
        &mut self,
        workspace: guild_session::WorkspaceId,
        moving: &[ProjectId],
        onto: &column::Top,
        after: bool,
    ) -> bool {
        let targets: Vec<ProjectId> = self
            .top_roots(workspace, onto)
            .into_iter()
            .filter(|id| !moving.contains(id))
            .collect();
        let Some(holder) = self.desk.workspace(workspace) else {
            return false;
        };
        let mut roots = holder.projects.iter().filter(|id| targets.contains(id));
        let target = if after {
            roots.next_back()
        } else {
            roots.next()
        }
        .copied();
        let Some(target) = target else {
            return false;
        };
        self.desk
            .reorder_projects_at(workspace, moving, target, after)
    }

    /// Before a drag places anything under a sort mode, make what the column
    /// shows the manual order — the top level and each group's projects as
    /// they stand — so the item lands where its line was and nothing else
    /// moves. Manual already, this changes nothing.
    fn adopt_shown_order(&mut self, workspace: guild_session::WorkspaceId) {
        let Some(holder) = self.desk.workspace(workspace) else {
            return;
        };
        if holder.project_sort == guild_session::project_sort::Mode::Manual {
            return;
        }
        let held = holder.projects.clone();
        let mut order: Vec<ProjectId> = Vec::with_capacity(held.len());
        let shown = self.column_groups_for(workspace);
        let rest = held.iter().copied();
        for id in shown
            .iter()
            .flat_map(|group| {
                self.top_roots(workspace, &column::Top::Repository(group.key.clone()))
            })
            .chain(rest)
        {
            if !order.contains(&id) {
                order.push(id);
            }
        }
        self.desk.adopt_project_order(workspace, &order);
    }

    /// A dragged project group let go of on the line before or after a
    /// top-level item: its roots go there as one block.
    pub(crate) fn drop_project_group_at(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        onto: column::Top,
        after: bool,
        cx: &mut Context<Root>,
    ) {
        if onto == column::Top::Group(id) {
            return;
        }
        self.adopt_shown_order(workspace);
        let moving = self.desk.project_group_roots(workspace, id);
        if !moving.is_empty() && self.place_beside_top(workspace, &moving, &onto, after) {
            self.save();
            cx.notify();
        }
    }

    /// A dragged repository let go of on the line before or after a
    /// top-level item: it goes there on its own, out of the group it was
    /// in. A payload whose roots are no longer the repository's changes
    /// nothing.
    pub(crate) fn place_repository_at_top(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        onto: column::Top,
        after: bool,
        cx: &mut Context<Root>,
    ) {
        if onto == column::Top::Repository(key.to_path_buf()) {
            return;
        }
        let current = self.top_roots(workspace, &column::Top::Repository(key.to_path_buf()));
        if current.len() != projects.len() || current.iter().any(|id| !projects.contains(id)) {
            return;
        }
        self.adopt_shown_order(workspace);
        let mut changed = self.desk.move_to_project_group(workspace, key, None);
        changed |= self.place_beside_top(workspace, projects, &onto, after);
        if changed {
            self.save();
            cx.notify();
        }
    }

    /// The top-level item a zone of a row is beside: a group's header, an
    /// ungrouped repository, or — for a repository inside a group — its
    /// group.
    fn zone_top(
        row: &column::Listed,
        section: Option<guild_session::project_group::ProjectGroupId>,
    ) -> column::Top {
        match (row, section) {
            (column::Listed::Group(id), _) => column::Top::Group(*id),
            (column::Listed::Repository(_), Some(group)) => column::Top::Group(group),
            (column::Listed::Repository(key), None) => column::Top::Repository(key.clone()),
        }
    }

    /// A dragged repository let go of on a drop row: the zone under the
    /// pointer decides, as [`Root::apply_project_drop`] says.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn drop_project_on_row(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        row: column::Listed,
        section: Option<guild_session::project_group::ProjectGroupId>,
        kind: column::DropRow,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let beside = Self::zone_top(&row, section);
        let zone = self
            .column_drop_zone(workspace, &row, kind, column::Dragged::Project, window)
            .filter(|zone| !self.column_drop_stays(&row, &beside, kind, *zone));
        self.clear_project_drag(cx);
        if let Some(zone) = zone {
            self.apply_project_drop(
                workspace,
                key,
                projects,
                column::DropHint {
                    workspace,
                    row,
                    zone,
                    beside,
                    kind,
                },
                Some(window),
                cx,
            );
        }
    }

    /// A dragged repository let go of between rows: it goes where the line
    /// drawn there says, and nowhere when none is drawn.
    pub(crate) fn drop_project_by_hint(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        cx: &mut Context<Root>,
    ) {
        let hint = self
            .column_drop_hint
            .clone()
            .filter(|hint| hint.workspace == workspace && hint.zone != column::Zone::Middle);
        self.clear_project_drag(cx);
        if let Some(hint) = hint {
            self.apply_project_drop(workspace, key, projects, hint, None, cx);
        }
    }

    /// What a dragged repository let go of at `drop` does. **Only lines
    /// reorder**: one between top-level items puts it there on its own, out
    /// of any group; one between a group's projects puts it there in that
    /// group. A group header's middle takes it into the group without a
    /// word; an ungrouped project's middle asks whether to merge the two
    /// into a group — nothing moves until the person says so.
    fn apply_project_drop(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        drop: column::DropHint,
        window: Option<&mut gpui::Window>,
        cx: &mut Context<Root>,
    ) {
        let after = drop.zone == column::Zone::After;
        match (drop.zone, &drop.row) {
            (column::Zone::Before | column::Zone::After, column::Listed::Repository(member))
                if column::inside_group(drop.kind, drop.zone) =>
            {
                if let column::Top::Group(group) = drop.beside {
                    self.place_repository_in_group(
                        workspace, key, projects, group, member, after, cx,
                    );
                }
            }
            (column::Zone::Before | column::Zone::After, _) => {
                self.place_repository_at_top(workspace, key, projects, drop.beside, after, cx)
            }
            (column::Zone::Middle, column::Listed::Group(id)) => self.drop_repository(
                workspace,
                key,
                projects,
                column::DropOn::Head(*id),
                None,
                cx,
            ),
            (column::Zone::Middle, column::Listed::Repository(on))
                if drop.kind == column::DropRow::Repository && on.as_path() != key =>
            {
                if let Some(window) = window {
                    let at = window.mouse_position();
                    self.open_merge_projects(
                        workspace,
                        on.clone(),
                        key.to_path_buf(),
                        at,
                        window,
                        cx,
                    );
                }
            }
            (column::Zone::Middle, _) => {}
        }
    }

    /// A dragged repository let go of on a line beside a project of a group:
    /// it goes there, in that group — joining it when it was elsewhere. A
    /// payload whose roots are no longer the repository's changes nothing.
    #[allow(clippy::too_many_arguments)]
    fn place_repository_in_group(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        projects: &[ProjectId],
        group: guild_session::project_group::ProjectGroupId,
        member: &std::path::Path,
        after: bool,
        cx: &mut Context<Root>,
    ) {
        if member == key {
            return;
        }
        let current = self.top_roots(workspace, &column::Top::Repository(key.to_path_buf()));
        if current.len() != projects.len() || current.iter().any(|id| !projects.contains(id)) {
            return;
        }
        self.adopt_shown_order(workspace);
        let mut changed = self.desk.move_to_project_group(workspace, key, Some(group));
        changed |= self.place_beside_top(
            workspace,
            projects,
            &column::Top::Repository(member.to_path_buf()),
            after,
        );
        if changed {
            self.save();
            cx.notify();
        }
    }

    /// Merge two ungrouped repositories into a new group named `name`, at
    /// the place of `target`, with `dragged` after it — the question's
    /// answer. Either having left the workspace meanwhile makes nothing.
    pub(crate) fn merge_projects(
        &mut self,
        workspace: guild_session::WorkspaceId,
        target: &std::path::Path,
        dragged: &std::path::Path,
        name: &str,
        cx: &mut Context<Root>,
    ) -> bool {
        let held = |root: &Self, key: &std::path::Path| {
            !root
                .top_roots(workspace, &column::Top::Repository(key.to_path_buf()))
                .is_empty()
        };
        if !held(self, target) || !held(self, dragged) {
            return false;
        }
        let Some(group) = self.desk.create_project_group(workspace, name, target) else {
            return false;
        };
        self.desk
            .move_to_project_group(workspace, dragged, Some(group));
        self.save();
        cx.notify();
        true
    }

    /// A dragged project group let go of between rows, as a repository is.
    pub(crate) fn drop_group_by_hint(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        cx: &mut Context<Root>,
    ) {
        let hint = self
            .column_drop_hint
            .clone()
            .filter(|hint| hint.workspace == workspace);
        self.clear_project_drag(cx);
        if let Some(hint) = hint
            && hint.zone != column::Zone::Middle
        {
            self.drop_project_group_at(
                workspace,
                id,
                hint.beside,
                hint.zone == column::Zone::After,
                cx,
            );
        }
    }

    /// A dragged project group let go of on a drop row: only the lines
    /// between top-level items take it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn drop_group_on_row(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        row: column::Listed,
        section: Option<guild_session::project_group::ProjectGroupId>,
        kind: column::DropRow,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let beside = Self::zone_top(&row, section);
        let zone = self
            .column_drop_zone(workspace, &row, kind, column::Dragged::Group, window)
            .filter(|zone| !self.column_drop_stays(&row, &beside, kind, *zone));
        self.clear_project_drag(cx);
        if let Some(zone @ (column::Zone::Before | column::Zone::After)) = zone {
            self.drop_project_group_at(
                workspace,
                id,
                Self::zone_top(&row, section),
                zone == column::Zone::After,
                cx,
            );
        }
    }

    /// Fold the column to its strip, or open it again — M12.
    ///
    /// A window's choice rather than a project's: it is about how much
    /// room the work gets, which does not change with what is being
    /// worked on. Saved at once, like every other setting the window
    /// changes by a gesture.
    pub(crate) fn toggle_column(&mut self, cx: &mut Context<Root>) {
        self.settings.column_folded = !self.settings.column_folded;
        // Whatever the pointer had brought out is beside the point now:
        // the person has said where the column should be.
        self.column_peeking = false;
        // Brought back by hand, it takes the focus, as a panel opened by
        // hand does. M12.3.
        self.activating_sidebar = !self.settings.column_folded;
        self.save_settings();
        cx.notify();
    }

    /// Switch what the column lists, Projects or Sessions — the title
    /// bar's mode button and `Toggle Session List`. Saved at once, as the
    /// folding beside it is; a search over the other list ends with it.
    pub(crate) fn toggle_column_mode(&mut self, cx: &mut Context<Root>) {
        self.settings.column_mode = self.settings.column_mode.toggled();
        self.sidebar_speed = None;
        self.sidebar_speed_revealed = None;
        self.save_settings();
        cx.notify();
    }

    /// What the column's heading says over `workspace`'s page: the mode,
    /// and in Projects whether the list holds groups too.
    pub(crate) fn column_heading(
        &self,
        workspace: guild_session::WorkspaceId,
    ) -> crate::i18n::Text {
        use crate::i18n::Text;
        if self.settings.column_mode == guild_session::settings::ColumnMode::Sessions {
            return Text::SidebarSessions;
        }
        if self
            .column_sections_for(workspace)
            .iter()
            .any(|section| section.head.is_some())
        {
            Text::SidebarProjectsAndGroups
        } else {
            Text::SidebarProjects
        }
    }

    /// Make a worktree of the repository the root in front belongs to,
    /// on a branch of the name that was typed — M12.
    ///
    /// The folder is `<repository>-<name>` beside the repository, which
    /// is git's own manual's `../hotfix` with the repository's name in
    /// front of it, and the base is `HEAD` of this root, which is what
    /// `git worktree add` uses when it is given none. git refuses a name
    /// or a folder it does not like, in its own words.
    pub(crate) fn create_worktree(&mut self, name: String, cx: &mut Context<Root>) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        if let Err(said) = self.write_worktree(project, &name, None) {
            self.warn(said);
            cx.notify();
            return;
        }
        self.close_branch_popup(cx);
    }

    /// Send the write that makes a worktree of `project`'s repository, and
    /// answer with the write itself — M13.
    ///
    /// **One path for the branch popup and for `guild worktree create`**,
    /// so a worktree made from a shell is made by the same write, beside
    /// the same repository, with the same refusals — decision 4 of M13's
    /// plan. The write comes back rather than being forgotten because the
    /// command line waits for git's answer to exactly this write, and
    /// `Write::args` is the one place its words are spelled.
    ///
    /// **A root with no git thread yet is refused here**, where it used to
    /// be dropped: `run_git_write` does nothing for a project whose worker
    /// has not started, which the popup survived by accident and a command
    /// waiting for an answer would not.
    ///
    /// `base` is where the branch starts, `None` being this root's `HEAD`:
    /// the popup and the `+` give none, `guild worktree create` gives the
    /// repository's main branch or its `--base` — M13.1. **A base that
    /// begins with `-` is refused here**, because git reads the last word
    /// as an option before it reads it as a start point.
    pub(crate) fn write_worktree(
        &mut self,
        project: ProjectId,
        name: &str,
        base: Option<String>,
    ) -> Result<crate::git::Write, String> {
        let name = column::worktree_name(name)?;
        if let Some(base) = base.as_deref().filter(|base| base.starts_with('-')) {
            return Err(format!("{base} is not a branch or a commit to start from"));
        }
        // The repository's own checkout is where its folder goes beside:
        // a worktree of a worktree still belongs next to the repository.
        let identity = self.require_repository_identity(project)?;
        let main = identity
            .main
            .clone()
            .filter(|_| identity.git)
            .ok_or_else(|| self.ui().text(crate::i18n::Text::ScopedNoMain).to_owned())?;
        let prefs = self
            .settings
            .repository_settings
            .get(&identity.key)
            .cloned()
            .unwrap_or_default();
        let path = crate::repository_settings::worktree_destination(&main, name, &prefs)?;
        let base = base.or(prefs.worktree_base);
        if !self.status_workers.contains_key(&project) {
            return Err(
                "git has not answered for that root yet — try again in a moment".to_owned(),
            );
        }
        let write = crate::git::Write::WorktreeAdd {
            path,
            branch: name.to_owned(),
            base,
        };
        self.run_git_write(project, write.clone());
        Ok(write)
    }

    /// Take hold of the column's own border.
    pub(crate) fn grab_column_edge(&mut self, cx: &mut Context<Root>) {
        self.column_edge_drag = true;
        cx.notify();
    }

    /// The border follows the pointer, between the two ends.
    ///
    /// [`column::width`] is the rule and it is tested on its own. Nothing
    /// is written to the disk here: a synced write is twenty milliseconds
    /// and a drag is sixty moves a second — the width is saved once, when
    /// the border is let go of.
    pub(crate) fn drag_column_edge(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Root>,
    ) {
        let width = column::width(f32::from(at.x), self.settings.ui_font_size);
        if (width - self.settings.column_width).abs() > f32::EPSILON {
            self.settings.column_width = width;
            cx.notify();
        }
    }

    /// Put the border back where the design system put it.
    ///
    /// A width dragged to something that turns out wrong has no way back:
    /// the number that was there is written down nowhere a person can
    /// read, and creeping towards it by hand never lands. The panels'
    /// borders have answered a double click this way since M5.7.
    pub(crate) fn reset_column_width(&mut self, cx: &mut Context<Root>) {
        self.settings.column_width = guild_session::Settings::default().column_width;
        self.save_settings();
        cx.notify();
    }

    /// Write the settings out, and say so if the disk refused.
    pub(crate) fn save_settings(&mut self) {
        self.store_settings();
    }

    /// The pointer moved: bring the hidden column out, or let it go.
    ///
    /// [`column::peek`] is the rule and it is tested on its own; this is
    /// the half that needs the window. Nothing is redrawn unless the
    /// answer changed — the pointer moves on every frame a hand is on the
    /// trackpad, and a `notify` per move would redraw the window for
    /// nothing.
    pub(crate) fn follow_column_pointer(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Root>,
    ) {
        // The work-facing half of the centered grip is still part of a
        // peeking column. Keep the overlay while its edge owns a drag.
        if self.column_edge_drag {
            return;
        }
        let reach = if self.column_peeking {
            crate::theme::space::COLUMN_RESIZE_REACH / 2.0
        } else {
            0.0
        };
        let out = column::peek(
            self.settings.column_folded,
            self.column_peeking,
            f32::from(at.x),
            self.settings.column_width + reach,
        );
        if out != self.column_peeking {
            self.column_peeking = out;
            cx.notify();
        }
    }

    /// Pin the column that is only peeking: it stops being hidden.
    ///
    /// The button that does this sits in the panel, and it is the same
    /// button that folds it again when it is pinned — an item that says
    /// what it will do rather than what the panel is.
    pub(crate) fn pin_column(&mut self, cx: &mut Context<Root>) {
        self.column_peeking = false;
        if self.settings.column_folded {
            self.toggle_column(cx);
        }
    }

    /// *New Worktree…* from a repository's head: that repository becomes
    /// the one in front, and the branch popup opens over it.
    ///
    /// **The root is selected first, and that is the whole of it.** A
    /// worktree is made from the `HEAD` of the root in front — decision 4
    /// — so a gesture aimed at a repository that is not in front would
    /// otherwise make its worktree from another repository's commit. The
    /// popup is the same one the column's foot opens, for the same reason
    /// it was chosen there: what is typed is offered as a branch and as a
    /// worktree, and the window keeps its three fields rather than growing
    /// a fourth.
    pub(crate) fn new_worktree_here(
        &mut self,
        project: ProjectId,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        if self.active_project != Some(project) {
            self.select_project(project, cx);
        }
        // **Where it was asked for, and nothing else** — M12.2 task 10. It
        // opened the branch popup under the title bar, which offers what is
        // typed as a branch too; a `+` on a project asks for one thing.
        let at = window.mouse_position();
        self.open_new_worktree(project, at, window, cx);
    }

    /// *Remove Project*: every root of the project leaves the window, and
    /// nothing on the disk is touched — M12.2.
    ///
    /// **One question for all of them, and only when something would be
    /// lost.** A project with no session running and no unsaved file simply
    /// goes; otherwise one sheet names what stops and what is not saved, and
    /// every root closes on the answer — the window asks one question at a
    /// time, and this is one.
    pub(crate) fn remove_project_asking(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        name: String,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        // Resolve the roots again when the deed is taken. The menu is a
        // snapshot rendered a frame earlier; a worktree refresh or a
        // workspace change can make the identifiers captured in its target
        // stale. Remove is about the repository named by the row, so the
        // workspace and its stable repository key are the source of truth.
        let roots: Vec<ProjectId> = self
            .desk
            .projects_of(workspace)
            .filter(|project| {
                column::group_key(&project.path, project.repository.as_deref()) == key
            })
            .map(|project| project.id)
            .collect();
        tracing::info!(
            workspace = %workspace,
            repository = %key.display(),
            roots = roots.len(),
            "Remove Project was chosen"
        );
        // One question at a time: the menu stays live under a sheet, and a
        // removal under a tab's or a project's close sheet would answer that
        // sheet about something already gone.
        if roots.is_empty() {
            tracing::warn!(
                workspace = %workspace,
                repository = %key.display(),
                "Remove Project found no roots in its workspace"
            );
            return;
        }
        if self.asking {
            tracing::warn!(
                workspace = %workspace,
                repository = %key.display(),
                "Remove Project was ignored while another question was open"
            );
            return;
        }
        self.autosave_files(crate::autosave::Trigger::Close, cx);
        let unsaved = roots.iter().any(|root| {
            !self
                .dirty_seat_tabs(
                    guild_session::Seat {
                        workspace,
                        project: *root,
                    },
                    cx,
                )
                .is_empty()
        });
        // Remove Project is always confirmed. Even when no running session
        // or unsaved buffer raises the stakes, taking a whole repository out
        // of a workspace is easy to choose by accident from the compact
        // project menu. `unsaved` adds the one line under the question —
        // since 2026-09-28 the only one: running sessions stop, and their
        // conversations can be resumed, so they are not a loss to warn of.
        let ui = self.ui();
        let answers = [
            gpui::PromptButton::new(ui.text(crate::i18n::Text::MenuRemoveProject)),
            gpui::PromptButton::cancel(ui.text(crate::i18n::Text::Cancel)),
        ];
        self.asking = true;
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            &ui.remove_project_title(&name),
            ui.remove_project_detail(unsaved),
            &answers,
            cx,
        );
        cx.spawn(async move |root, cx| {
            let removed = asked.await == Ok(0);
            let _ = root.update(cx, |root, cx| {
                root.asking = false;
                if removed {
                    for project in roots {
                        root.close_project_in(workspace, project, cx);
                    }
                }
            });
        })
        .detach();
    }

    /// *Remove…*: git takes the checkout off the disk, and it asks first
    /// — M12, decision 6 of its plan.
    ///
    /// **Three things are refused before the question is asked**, in the
    /// order [`column::removal`] puts them: the repository's own checkout,
    /// which git will not remove and which the menu does not offer either;
    /// a worktree an agent is working or waiting in, because losing its
    /// turn is the thing this guard exists to prevent; and one with
    /// changes, which git refuses anyway — but only after the window had
    /// closed its tabs for nothing. Each says why, in the notice.
    ///
    /// The order after the answer matters as much: the root is closed
    /// first, so that its watcher, its servers and its terminals are let
    /// go of before the directory under them goes, and only then is git
    /// told. Which is also why the write cannot run on this root's own
    /// worker — that thread's working directory is the folder about to be
    /// deleted, and the worker goes with the project. A sibling of the
    /// same repository runs it.
    pub(crate) fn remove_worktree_asking(
        &mut self,
        path: std::path::PathBuf,
        project: Option<ProjectId>,
        primary: bool,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        // Already taken off the disk from outside: git has nothing left to
        // remove, and what is left is the root — closed as a project is,
        // asking about unsaved text and a turn in progress. 2026-09-22.
        if let Some(project) = project
            && !primary
            && !column::checkout_at(&path)
        {
            self.ask_to_close_project(project, window, cx);
            return;
        }
        if !self.worktree_removal_allowed(&path, project, primary, cx) {
            return;
        }
        if self.removal_runner(&path).is_none() {
            self.inform(
                self.ui()
                    .text(crate::i18n::Text::WorktreeRemovalUnavailable)
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        let target = self.worktrees.iter().find_map(|(repository, list)| {
            list.iter().find(|row| row.path == path).map(|row| {
                crate::git::worktree_removal::Target {
                    repository: repository.clone(),
                    path: path.clone(),
                    branch: row.branch.clone(),
                    head: row.head.clone(),
                }
            })
        });
        let Some(target) = target else {
            self.inform(
                self.ui()
                    .text(crate::i18n::Text::WorktreeRemovalUnavailable)
                    .to_owned(),
            );
            cx.notify();
            return;
        };
        self.show_worktree_removal(target, window, cx);
    }

    pub(crate) fn worktree_removal_allowed(
        &mut self,
        path: &std::path::Path,
        project: Option<ProjectId>,
        primary: bool,
        cx: &mut Context<Root>,
    ) -> bool {
        let busy = project.is_some_and(|project| self.worktree_busy(project));
        let changed = project.is_some_and(|project| {
            self.column_changes(project) > 0 || !self.dirty_file_tabs(Some(project), cx).is_empty()
        });
        // Whoever locked it said why, and git will not remove it until
        // they say otherwise — M12 task 18.
        let locked = self.lock_on_worktree(path);
        let ui = self.ui();
        match column::removal(primary, busy, locked.is_some(), changed) {
            column::Removal::Primary => return false,
            column::Removal::AgentBusy => {
                let text = ui.text(crate::i18n::Text::NoticeWorktreeBusy).to_owned();
                self.inform(text);
                cx.notify();
                return false;
            }
            column::Removal::Locked => {
                // git's own reason, because whoever wrote it knew what
                // they were holding: a tool locks a checkout with its name
                // and its pid, and that is what tells a person whether to
                // wait or to unlock.
                let said = locked.unwrap_or_default();
                self.inform(ui.worktree_locked(&said));
                cx.notify();
                return false;
            }
            column::Removal::Changed => {
                let text = ui.text(crate::i18n::Text::NoticeWorktreeChanged).to_owned();
                self.inform(text);
                cx.notify();
                return false;
            }
            column::Removal::Ask => {}
        }

        true
    }

    /// An opened root of the same repository that is not the one being
    /// removed — where `git worktree remove` is run from.
    ///
    /// The repository's own checkout first: it is the one worktree that
    /// cannot itself be the subject, so it is the steadiest place to run
    /// the write from. Any other opened sibling will do — git reaches the
    /// whole repository from any of them.
    pub(crate) fn removal_runner(&self, path: &std::path::Path) -> Option<ProjectId> {
        let repository = self
            .desk
            .projects()
            .find(|project| project.path == path)
            .and_then(|project| project.repository.clone());
        let mut fallback = None;
        for project in self.desk.projects() {
            if project.path == path {
                continue;
            }
            // A root of the same repository, by the common git directory
            // git itself printed rather than by where the folders sit.
            if repository.is_some() && project.repository != repository {
                continue;
            }
            if Some(project.path.as_path()) == self.main_root_of(path) {
                return Some(project.id);
            }
            fallback = fallback.or(Some(project.id));
        }
        fallback
    }

    /// The repository's own checkout, for the repository this path belongs
    /// to — git's word, not a guess from the folders.
    fn main_root_of(&self, path: &std::path::Path) -> Option<&std::path::Path> {
        self.worktrees
            .values()
            .find(|list| list.iter().any(|worktree| worktree.path == path))
            .and_then(|list| list.first())
            .map(|worktree| worktree.path.as_path())
    }

    /// Why a worktree is locked, when it is — git's own words.
    pub(crate) fn lock_on_worktree(&self, path: &std::path::Path) -> Option<String> {
        self.worktrees
            .values()
            .flatten()
            .find(|worktree| worktree.path == path)
            .and_then(|worktree| worktree.locked.clone())
    }

    /// What branch git says a worktree is on — the name the sheet shows.
    pub(crate) fn branch_of_worktree(&self, path: &std::path::Path) -> Option<String> {
        self.worktrees
            .values()
            .flatten()
            .find(|worktree| worktree.path == path)
            .and_then(|worktree| worktree.branch.clone())
    }

    /// A click on a row: the root in front, or — for a worktree git lists
    /// that nobody has opened — the root opened first.
    /// Give a root just opened the repository git already lists it in, so
    /// that its row is drawn in that group from the first frame rather
    /// than as a group of its own until its git thread answers.
    pub(crate) fn know_repository(&mut self, project: ProjectId) {
        let Some(path) = self
            .desk
            .project(project)
            .map(|project| project.path.clone())
        else {
            return;
        };
        if let Some(common) = column::listed_in(&self.worktrees, &path) {
            let common = common.to_path_buf();
            self.desk.set_repository(project, Some(common));
        }
    }

    pub(crate) fn choose_row(&mut self, row: &Row, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.choose_row_without_history(row, cx);
        self.history.suspended -= 1;
    }

    fn choose_row_without_history(&mut self, row: &Row, cx: &mut Context<Root>) {
        match row {
            Row::Opened { project, .. } => {
                if self.active_project != Some(*project) {
                    self.select_project(*project, cx);
                }
            }
            Row::Unopened { path, .. } => {
                if !path.is_dir() {
                    // git lists it, but it is not there any more: git
                    // would call it prunable a moment later, and opening
                    // a directory that does not exist is a project of
                    // nothing.
                    let text = self
                        .ui()
                        .text(crate::i18n::Text::NoticeWorktreeGone)
                        .to_owned();
                    self.inform(text);
                    cx.notify();
                    return;
                }
                let project = self.desk.open_in(self.active_workspace, path);
                self.know_repository(project);
                self.active_workspace = self.desk.active_workspace();
                self.adopt_project(project);
                self.select_project(project, cx);
                // Its worker starts on the next tick of the watch loop,
                // and its first ask is the one that fills the group.
                self.save();
            }
        }
    }
}

/// What the column reads of project groups' folders from disk — GLD-055.
///
/// Read on the title bar's clock, never per frame: a branch is a file read
/// (`git::branch`, no process) and a canonical path a system call, and both
/// change only when a person changes them.
#[derive(Debug, Default)]
pub(crate) struct GroupReads {
    read: Option<std::time::Instant>,
    /// Each bound folder's branch; `None` outside git.
    branches: std::collections::HashMap<std::path::PathBuf, Option<String>>,
    /// Folders' and members' checkouts' canonical forms, so a symlinked
    /// path compares as what it points to.
    canonical: std::collections::HashMap<std::path::PathBuf, std::path::PathBuf>,
}

impl Root {
    /// Re-read the bound folders' branches and the canonical paths the
    /// match compares — every [`crate::BRANCH_PROBE`], or `now`.
    pub(crate) fn refresh_group_reads(&mut self, now: bool) {
        if !self.integrations.external() {
            return;
        }
        let due = now
            || self
                .group_reads
                .read
                .is_none_or(|read| read.elapsed() >= crate::BRANCH_PROBE);
        if !due {
            return;
        }
        let mut folders = Vec::new();
        let mut paths = Vec::new();
        for holder in self.desk.workspaces() {
            for group in &holder.project_groups {
                if let Some(folder) = &group.folder {
                    folders.push(folder.clone());
                    paths.push(folder.clone());
                    paths.extend(
                        group
                            .repositories
                            .iter()
                            .map(|key| self.member_checkout(key)),
                    );
                }
            }
        }
        let reads = &mut self.group_reads;
        reads.read = Some(std::time::Instant::now());
        reads.branches = folders
            .into_iter()
            .map(|folder| {
                let branch = crate::git::branch(&folder);
                (folder, branch)
            })
            .collect();
        reads.canonical = paths
            .into_iter()
            .filter_map(|path| {
                let canonical = std::fs::canonicalize(&path).ok()?;
                Some((path, canonical))
            })
            .collect();
    }

    /// What a group folder's row is called — decision 13: its branch when
    /// it is a git repository, its folder's name otherwise.
    pub(crate) fn group_folder_label(&self, folder: &std::path::Path) -> String {
        self.group_reads
            .branches
            .get(folder)
            .cloned()
            .flatten()
            .unwrap_or_else(|| {
                folder
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| folder.display().to_string())
            })
    }

    /// How a bound group stands against the filesystem, compared in the
    /// canonical forms last read.
    pub(crate) fn group_folder_match(
        &self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
    ) -> Option<guild_session::project_group::FolderMatch> {
        let canonical = &self.group_reads.canonical;
        self.desk.workspace(workspace)?.project_group_match(
            id,
            |key| self.member_checkout(key),
            |path| {
                canonical
                    .get(path)
                    .cloned()
                    .unwrap_or_else(|| path.to_path_buf())
            },
        )
    }

    /// A member's main checkout: the first, working entry of git's list of
    /// its worktrees, which is right where the git directory is kept apart
    /// from the checkout; the folder a `.git` directory sits in before git
    /// has answered.
    pub(crate) fn member_checkout(&self, key: &std::path::Path) -> std::path::PathBuf {
        self.worktrees
            .get(key)
            .and_then(|list| list.first())
            .filter(|main| !main.bare && !main.prunable)
            .map(|main| main.path.clone())
            .unwrap_or_else(|| guild_session::project_group::main_checkout(key))
    }

    /// The folder suggested for a group — decision 1.
    pub(crate) fn group_folder_suggestion(
        &self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
    ) -> Option<std::path::PathBuf> {
        let group = self.desk.workspace(workspace)?.project_group(id)?;
        let checkouts: Vec<_> = group
            .repositories
            .iter()
            .map(|key| self.member_checkout(key))
            .collect();
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
        guild_session::project_group::suggest_folder(&checkouts, home.as_deref())
    }

    /// Bind a group to the folder a person gave, or say why not. A path
    /// that does not exist or is not a directory is refused: Guild creates
    /// no folder here. The folder that was bound before has its root asked
    /// to close, as *Close Project* asks — decision 18.
    pub(crate) fn bind_group_folder(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        typed: &str,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) -> Option<ProjectId> {
        let ui = self.ui();
        // As given first — a folder's name may end in a space, and the
        // system panel's answer is kept whole — then without the edges a
        // person typed by accident.
        let given = expand_home(typed);
        let path = if given.exists() {
            given
        } else {
            expand_home(typed.trim())
        };
        if !path.exists() {
            self.warn(ui.group_folder_missing(&path.display().to_string()));
            cx.notify();
            return None;
        }
        if !path.is_dir() {
            self.warn(ui.group_folder_not_a_folder(&path.display().to_string()));
            cx.notify();
            return None;
        }
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let group = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group(id))
            .cloned()?;
        // A folder whose `.git` is a file is a checkout git keeps elsewhere —
        // a linked worktree, a submodule — which removal, auto fast-forward
        // and the worktree list would treat as theirs: a group's folder is
        // never one (decision 26).
        if crate::git::dot_git_is_file(&path) {
            self.warn(ui.text(crate::i18n::Text::GroupFolderLinked).to_owned());
            cx.notify();
            return None;
        }
        // A write a command is still waiting on reports through the root's
        // git worker, which binding stops: wait for it (decision 26).
        if !self.pending_writes.is_empty() {
            self.warn(ui.text(crate::i18n::Text::GroupFolderGitBusy).to_owned());
            cx.notify();
            return None;
        }
        // A member's checkout git lists is one of the group's own — decision
        // 19.
        if column::listed_in(&self.worktrees, &path)
            .is_some_and(|common| group.repositories.iter().any(|key| key == common))
        {
            self.warn(ui.text(crate::i18n::Text::GroupFolderOwnRoot).to_owned());
            cx.notify();
            return None;
        }
        let old = group.folder.clone().filter(|old| *old != path);
        let root = match self.desk.bind_project_group_folder(workspace, id, &path) {
            Ok(root) => root,
            Err(refused) => {
                use guild_session::project_group::FolderRefused;
                let text = match refused {
                    FolderRefused::NoGroup => return None,
                    FolderRefused::OwnRoot => crate::i18n::Text::GroupFolderOwnRoot,
                    FolderRefused::OtherGroup => crate::i18n::Text::GroupFolderOtherGroup,
                };
                self.warn(ui.text(text).to_owned());
                cx.notify();
                return None;
            }
        };
        self.know_repository(root);
        self.adopt_project(root);
        self.drop_group_root_tooling(root, cx);
        self.refresh_group_reads(true);
        self.save();
        self.rebuild_menu(cx);
        let old_root = old.and_then(|old| {
            self.desk
                .projects_of(workspace)
                .find(|project| project.path == old)
                .map(|project| project.id)
        });
        if let Some(project) = old_root {
            self.restore_root_tooling(project, cx);
            if workspace == self.active_workspace {
                self.ask_to_close_project(project, window, cx);
            }
        }
        cx.notify();
        Some(root)
    }

    /// A root bound as a group's folder may have been an ordinary one a
    /// moment ago, with its tooling running and its answers kept: they go
    /// now — its indexes and their rankings, its language servers and their
    /// problems, what its build systems said, and its git status — so a
    /// group root is shown, not indexed, from the moment it is bound.
    pub(crate) fn drop_group_root_tooling(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        self.forget_indexes(project);
        self.stop_servers_of(project);
        self.forget_problems(project);
        self.dependencies.forget(project);
        self.status_workers.remove(&project);
        self.git_status.remove(&project);
        // Started again on the next tick, watching the tree alone.
        self.watchers.remove(&project);
        let without_git: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in without_git {
            view.update(cx, |tab, cx| tab.set_without_git(true, cx));
        }
        cx.notify();
    }

    /// A root that stopped being a group's folder and stays open: watched
    /// again as an ordinary root, its file tabs' git markers back.
    pub(crate) fn restore_root_tooling(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        if self.desk.is_project_group_root(project) {
            return;
        }
        self.watchers.remove(&project);
        let files: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in files {
            view.update(cx, |tab, cx| tab.set_without_git(false, cx));
        }
        cx.notify();
    }

    /// The `+` on a group folder's row, and the group menu's *New Session*:
    /// a session in the group's folder, as the tab launcher's default
    /// opens one. A group with no folder asks for one first — decision 1's
    /// question — and starts the session once it is bound.
    pub(crate) fn new_group_session(
        &mut self,
        workspace: guild_session::WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        let Some(group) = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group(id))
        else {
            return;
        };
        let Some(folder) = group.folder.clone() else {
            self.open_bind_group_folder(workspace, id, true, at, window, cx);
            return;
        };
        match self.open_group_root(workspace, &folder) {
            Ok(project) => {
                if workspace != self.active_workspace {
                    self.select_workspace(workspace, cx);
                }
                self.select_project(project, cx);
                let runs = self.runs_for_default_tab(cx);
                if let Some(kind) = Self::harness_missing(runs) {
                    return self.say_missing(kind, cx);
                }
                if let guild_session::Runs::OpenCode(_) = runs
                    && Self::opencode_floor_missed()
                {
                    // The harness tab's own refusal, said the same way.
                    return self.open_harness_tab(project, runs, window, cx);
                }
                self.open_tab_in_root(project, runs, cx);
                self.refocus(window, cx);
            }
            Err(because) => {
                self.warn(because);
                cx.notify();
            }
        }
    }

    /// A bound group's folder as a root of its workspace: the one already
    /// open, or opened now — the path every session in a group's folder
    /// starts from, the window's and `guild`'s. The folder must still be a
    /// directory.
    pub(crate) fn open_group_root(
        &mut self,
        workspace: guild_session::WorkspaceId,
        folder: &std::path::Path,
    ) -> Result<ProjectId, String> {
        if !folder.is_dir() {
            return Err(format!("there is no folder at {}", folder.display()));
        }
        let opened = self
            .desk
            .projects_of(workspace)
            .find(|project| project.path == folder)
            .map(|project| project.id);
        let project = match opened {
            Some(project) => project,
            None => {
                let project = self.desk.open_in(workspace, folder);
                self.know_repository(project);
                self.adopt_project(project);
                self.save();
                project
            }
        };
        Ok(project)
    }

    /// The header's `+`: put a project of the workspace into the group —
    /// the same deed as *Move to Group*.
    pub(crate) fn add_project_to_group(
        &mut self,
        workspace: guild_session::WorkspaceId,
        key: &std::path::Path,
        id: guild_session::project_group::ProjectGroupId,
        cx: &mut Context<Root>,
    ) {
        self.move_repository_to_group(workspace, key, Some(id), cx);
        self.refresh_group_reads(true);
    }

    /// The roots of groups that went because their last member left —
    /// decision 18: closed at once when nothing in them would be lost,
    /// otherwise left open as ordinary roots for the person to close.
    pub(crate) fn close_released_group_folders(
        &mut self,
        released: Vec<(guild_session::WorkspaceId, std::path::PathBuf)>,
        cx: &mut Context<Root>,
    ) {
        for (workspace, folder) in released {
            let Some(project) = self
                .desk
                .projects_of(workspace)
                .find(|project| project.path == folder)
                .map(|project| project.id)
            else {
                continue;
            };
            // Bound again meanwhile, in this workspace, it stays.
            if self
                .desk
                .workspace(workspace)
                .is_some_and(|holder| holder.project_group_of_folder(&folder).is_some())
            {
                continue;
            }
            // Something running in it — an agent, a Run command, a shell in
            // the Terminal tool — or an unsaved file: left open for the
            // person to close.
            let running = self
                .runs
                .get(&project)
                .is_some_and(|tabs| !tabs.entries().is_empty())
                || self
                    .panel_terminals
                    .get(&project)
                    .is_some_and(|tabs| !tabs.entries().is_empty());
            // Any tab of the seat — a shell running a command, a session,
            // a file — is the person's to close.
            let seat = guild_session::Seat { workspace, project };
            if running
                || self.tabs.iter().any(|tab| tab.seat == seat)
                || self.has_live_session(project, cx)
                || !self.dirty_file_tabs(Some(project), cx).is_empty()
            {
                self.restore_root_tooling(project, cx);
                continue;
            }
            // Its saved sessions and tabs stay in its `.guild/`, whatever
            // *forget on close* says: nothing is lost by this closing.
            self.close_project_keeping_state(workspace, project, cx);
        }
    }
}

/// A path as the bind question's field shows it: under the home directory
/// with `~` in its place, as a person writes one — and reads back through
/// [`expand_home`].
pub(crate) fn shown_path(path: &std::path::Path) -> String {
    match std::env::var_os("HOME").map(std::path::PathBuf::from) {
        Some(home) if path.starts_with(&home) && path != home => {
            format!("~/{}", path.strip_prefix(&home).unwrap_or(path).display())
        }
        _ => path.display().to_string(),
    }
}

/// A typed path with a leading `~` meaning the home directory, as a person
/// writes one.
fn expand_home(typed: &str) -> std::path::PathBuf {
    match (typed.strip_prefix('~'), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            std::path::PathBuf::from(home).join(rest.trim_start_matches('/'))
        }
        _ => std::path::PathBuf::from(typed),
    }
}

/// GLD-055 in the window: a group root is shown, not indexed. The maps
/// these read are `Root`'s own, so the test lives beside them rather than in
/// `tests/all/`, which reaches only the public surface.
#[cfg(test)]
mod group_folder_tests {
    use crate::Root;
    use gpui::{AppContext, TestAppContext};
    use guild_session::Desk;

    #[test]
    fn isolated_group_root_is_shown_not_indexed() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::column::group_folder_tests::a_",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_GROUP_ROOT_FIXTURE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("2 passed"),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "isolated_group_root_is_shown_not_indexed owns global persistence paths"]
    fn a_group_root_is_shown_not_indexed(cx: &mut TestAppContext) {
        assert!(std::env::var_os("GUILD_GROUP_ROOT_FIXTURE").is_some());
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let acme = dir.path().join("acme");
        let backend_path = acme.join("backend");
        let frontend_path = acme.join("frontend");
        let other_path = dir.path().join("other");
        for path in [&backend_path, &frontend_path, &other_path] {
            std::fs::create_dir_all(path.join("src")).unwrap();
        }
        std::fs::write(acme.join("AGENTS.md"), "# acme\n").unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let backend = desk.open_in(workspace, &backend_path);
        desk.open_in(workspace, &frontend_path);
        desk.open_in(workspace, &other_path);
        let id = desk
            .create_project_group(workspace, "acme", &backend_path)
            .unwrap();
        desk.move_to_project_group(workspace, &frontend_path, Some(id));
        let group_root = desk
            .bind_project_group_folder(workspace, id, &acme)
            .unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let entity = cx.new(|cx| Root::diagnostic(desk, dir.path().join("diagnostic"), cx));
            entity.update(cx, |root, cx| {
                // No git status worker for the group root; the members keep theirs.
                root.poll_git(cx);
                assert!(!root.status_workers.contains_key(&group_root));
                assert!(root.status_workers.contains_key(&backend));

                // No file or symbol index, however it is asked.
                root.rank_files(group_root, "agents", false, cx);
                root.rank_symbols(group_root, "main", false, cx);
                assert!(!root.has_file_index(group_root));
                assert!(!root.has_symbol_index(group_root));
                // Nothing to wait for: the pickers say nothing was found,
                // never that it is indexing.
                assert!(root.index_ready(group_root) && root.symbols_ready(group_root));
                root.rank_files(backend, "main", false, cx);
                assert!(root.has_file_index(backend));

                // No language server is permitted over it.
                assert!(!root.lsp_permitted(group_root, "rust"));

                // Its tree is read when it is in front.
                root.select_project(group_root, cx);
                root.ensure_active_tree(cx);
                assert!(root.tree().is_some());

                // The column draws it under its header, never as a repository.
                let keys: Vec<_> = root
                    .column_groups_for(workspace)
                    .into_iter()
                    .map(|group| group.key)
                    .collect();
                assert!(!keys.contains(&acme), "{keys:?}");
                let head = root
                    .column_sections_for(workspace)
                    .into_iter()
                    .find_map(|section| section.head)
                    .unwrap();
                assert_eq!(head.folder.as_deref(), Some(acme.as_path()));
                assert_eq!(head.count, 2);

                // Both members sit directly in the folder.
                let found = root.group_folder_match(workspace, id).unwrap();
                assert!(found.matches());

                // `+` offers exactly the projects not in the group.
                let offered: Vec<_> = root
                    .group_add_choices(workspace, id)
                    .into_iter()
                    .map(|choice| choice.key)
                    .collect();
                assert_eq!(offered, std::slice::from_ref(&other_path));
                root.add_project_to_group(workspace, &other_path, id, cx);
                assert!(root.group_add_choices(workspace, id).is_empty());
                assert!(!root.group_folder_match(workspace, id).unwrap().matches());

                // A member's file belongs to the member, AGENTS.md to the group.
                assert_eq!(
                    root.project_of(&backend_path.join("src/main.rs"))
                        .map(|(p, _)| p),
                    Some(backend)
                );
                assert_eq!(
                    root.project_of(&acme.join("AGENTS.md")).map(|(p, _)| p),
                    Some(group_root)
                );

                // The last member leaving takes the group; its root closes,
                // and the folder stays on disk.
                for key in [&backend_path, &frontend_path, &other_path] {
                    root.move_repository_to_group(workspace, key, None, cx);
                }
                let released = root.desk.take_released_group_folders();
                assert_eq!(released, [(workspace, acme.clone())]);
                root.close_released_group_folders(released, cx);
                assert!(
                    !root
                        .desk
                        .projects_of(workspace)
                        .any(|project| project.id == group_root)
                );
                assert!(acme.join("AGENTS.md").exists());
            });
        });
    }

    #[gpui::test]
    #[ignore = "isolated_group_root_is_shown_not_indexed owns global persistence paths"]
    fn a_bound_open_root_loses_its_tooling_and_a_member_s_worktree_is_refused(
        cx: &mut TestAppContext,
    ) {
        assert!(std::env::var_os("GUILD_GROUP_ROOT_FIXTURE").is_some());
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let acme = std::fs::canonicalize(dir.path()).unwrap().join("acme");
        let backend_path = acme.join("backend");
        std::fs::create_dir_all(backend_path.join("src")).unwrap();
        std::fs::write(backend_path.join("src/main.rs"), "fn main() {}\n").unwrap();
        let git = |args: &[&str]| {
            let done = std::process::Command::new("git")
                .args(["-c", "user.email=t@t", "-c", "user.name=t"])
                .args(args)
                .current_dir(&backend_path)
                .output()
                .unwrap();
            assert!(
                done.status.success(),
                "{}",
                String::from_utf8_lossy(&done.stderr)
            );
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["add", "."]);
        git(&["commit", "-qm", "init"]);
        // A linked worktree the window has never been told of.
        git(&["worktree", "add", "-q", "-b", "side", "../backend-side"]);
        let common = std::fs::canonicalize(backend_path.join(".git")).unwrap();
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let backend = desk.open_in(workspace, &backend_path);
        desk.set_repository(backend, Some(common));
        let folder = desk.open_in(workspace, &acme);
        let id = desk
            .create_project_group(workspace, "acme", &backend_path.join(".git"))
            .unwrap();
        let mut entity = None;
        let state = dir.path().join("diagnostic");
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let root = cx.new(|cx| Root::diagnostic(desk, state, cx));
            entity = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        let root = entity.unwrap();
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                // An ordinary root, indexed and watched by git.
                root.poll_git(cx);
                root.rank_files(folder, "agents", false, cx);
                assert!(root.has_file_index(folder));
                assert!(root.status_workers.contains_key(&folder));

                // A member's linked worktree is one of the group's own.
                let side = acme.join("backend-side");
                assert_eq!(
                    root.bind_group_folder(workspace, id, &side.display().to_string(), window, cx),
                    None
                );

                // Bound, it loses what it ran as an ordinary root.
                let bound =
                    root.bind_group_folder(workspace, id, &acme.display().to_string(), window, cx);
                assert_eq!(bound, Some(folder));
                assert!(!root.has_file_index(folder));
                assert!(!root.status_workers.contains_key(&folder));
                root.poll_git(cx);
                assert!(!root.status_workers.contains_key(&folder));
                assert!(root.status_workers.contains_key(&backend));
            })
        });
    }
}
