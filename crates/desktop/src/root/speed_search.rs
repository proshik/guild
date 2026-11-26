//! List-local input routing. A match becomes the host's one selection —
//! decision 54; the query never filters.
use crate::{Origin, Root, speed_search::Search};
use gpui::{Context, KeyDownEvent, ScrollStrategy};

impl Root {
    /// Rendering also follows model/owner changes, even when an input was disposed
    /// before its blur callback could run. Never take focus or reset a valid query.
    pub(crate) fn settle_speed_search(&mut self) {
        self.settle_sidebar_speed();
        let seat = self.active_seat();
        let project = self.active_project;
        let tree_up = self.tool_is_up(guild_session::Tool::Explorer);
        let commit_up = self.tool_is_up(guild_session::Tool::Commit);
        let log_up = self.tool_is_up(guild_session::Tool::Git)
            && self.git_face() == guild_session::GitFace::Log;
        for (owner, state) in &mut self.trees {
            if Some(*owner) != seat || !tree_up {
                state.speed = None;
            }
        }
        for (owner, state) in &mut self.changes {
            if Some(*owner) != project || !commit_up {
                state.speed = None;
            }
        }
        for (owner, state) in &mut self.logs {
            if Some(*owner) != project || !log_up || state.opened.is_none() {
                state.speed = None;
            }
        }
        if self.tree().is_some_and(|s| s.speed.is_some()) {
            let rows = self.tree_speed_rows();
            let state = self.tree_mut().unwrap();
            if let Some((row, (origin, path))) = refreshed_row(state.speed.as_mut().unwrap(), &rows)
            {
                state.scroll.scroll_to_item(row, ScrollStrategy::Top);
                // A match the refresh replaced is the selection too.
                self.select_tree_path(origin, &path);
            }
        }
        for host in [FileHost::Commit, FileHost::Log] {
            if self.file_speed(host).is_none() {
                continue;
            }
            let rows = self.file_speed_rows(host);
            let row = refreshed_row(self.file_speed_slot(host).unwrap().as_mut().unwrap(), &rows);
            if let Some((row, path)) = row {
                self.select_file_match(host, row, path);
            }
        }
    }

    fn tree_speed_update(&mut self, backwards: Option<bool>, cx: &mut Context<Self>) {
        let rows = self.tree_speed_rows();
        let mut matched = None;
        if let Some(state) = self.tree_mut()
            && let Some(search) = &mut state.speed
        {
            if let Some(back) = backwards {
                search.step(&rows, back);
            } else {
                search.reconcile(&rows);
            }
            if let Some(row) = search.matched(&rows) {
                state.scroll.scroll_to_item(row.row, ScrollStrategy::Top);
                matched = Some(row.key.clone());
            }
        }
        // The match is the selection — decision 54.
        if let Some((origin, path)) = matched {
            self.select_tree_path(origin, &path);
        }
        cx.notify();
    }
    pub(crate) fn tree_speed_field(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let state = self.tree()?;
        let search = state.speed.as_ref();
        let seat = self.active_seat()?;
        Some(crate::view::speed_search::field(
            search.map(|s| s.query.as_str()),
            &state.focus,
            search.is_some_and(|s| {
                !s.query.is_empty() && s.matched(&self.tree_speed_rows()).is_none()
            }),
            self.theme,
            cx.listener(move |root, value: &String, _, cx| {
                if root.active_seat() != Some(seat) {
                    return;
                }
                if let Some(state) = root.tree_mut() {
                    let current = state
                        .files
                        .focus
                        .clone()
                        .or_else(|| state.selected.clone().map(|p| (Origin::Project, p)));
                    state
                        .speed
                        .get_or_insert_with(|| Search::new("", current))
                        .query = value.clone();
                }
                root.tree_speed_update(None, cx);
            }),
            cx.listener(|root, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "up" | "down" => root.tree_speed_update(Some(event.keystroke.key == "up"), cx),
                    "enter" | "escape" | "tab" => {
                        let rows = root.tree_speed_rows();
                        let key = root
                            .tree()
                            .and_then(|s| s.speed.as_ref())
                            .and_then(|s| s.matched(&rows))
                            .map(|r| r.key.clone());
                        if let Some(state) = root.tree_mut() {
                            state.speed = None;
                            window.focus(&state.focus, cx);
                        }
                        if event.keystroke.key == "enter"
                            && let Some((origin, path)) = key
                        {
                            match origin {
                                Origin::Project => {
                                    let row = root.tree().and_then(|s| {
                                        s.tree
                                            .rows()
                                            .iter()
                                            .find(|r| r.path() == path)
                                            .map(|r| (r.is_dir(), r.chain_root().to_path_buf()))
                                    });
                                    if let Some((dir, fold)) = row {
                                        if dir {
                                            root.tree_click(
                                                &path,
                                                &fold,
                                                true,
                                                Default::default(),
                                                window,
                                                cx,
                                            );
                                        } else {
                                            root.open_file_from(origin, &path, window, cx);
                                        }
                                    }
                                }
                                Origin::External(at) => {
                                    let dir = root
                                        .tree()
                                        .and_then(|s| s.library_trees.get(&at))
                                        .and_then(|t| {
                                            t.rows()
                                                .iter()
                                                .find(|r| r.path() == path)
                                                .map(|r| r.is_dir())
                                        });
                                    if path.as_os_str().is_empty() {
                                        // The displayed root is a candidate before its tree is read.
                                        root.library_click(
                                            at,
                                            &path,
                                            true,
                                            Default::default(),
                                            window,
                                            cx,
                                        );
                                    } else if let Some(dir) = dir {
                                        if dir {
                                            root.library_click(
                                                at,
                                                &path,
                                                true,
                                                Default::default(),
                                                window,
                                                cx,
                                            );
                                        } else {
                                            root.open_file_from(origin, &path, window, cx);
                                        }
                                    }
                                }
                                // The Explorer has no row for a file
                                // outside the project.
                                Origin::Outside => {}
                            }
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            }),
            cx.listener(move |root, _: &(), _, cx| {
                if let Some(state) = root.trees.get_mut(&seat) {
                    state.speed = None;
                }
                cx.notify();
            }),
        ))
    }
}
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileHost {
    Commit,
    Log,
}

impl Root {
    pub(crate) fn file_speed_rows(
        &self,
        host: FileHost,
    ) -> Vec<crate::speed_search::Candidate<std::path::PathBuf>> {
        use crate::speed_search::Candidate;
        match host {
            FileHost::Commit => {
                let Some(state) = self.changes() else {
                    return Vec::new();
                };
                let mut rows = Vec::new();
                let mut line = 0;
                for section in [
                    crate::changes::Section::Changes,
                    crate::changes::Section::Unversioned,
                ] {
                    let group: Vec<_> =
                        state.list.iter().filter(|l| l.section == section).collect();
                    if group.is_empty() {
                        continue;
                    }
                    line += 1;
                    if state.folded.contains(&section) {
                        continue;
                    }
                    for listed in group {
                        if !state.row_visible(&listed.row.path) {
                            continue;
                        }
                        let path = &listed.row.path;
                        rows.push(Candidate::new(
                            path.clone(),
                            path.file_name().unwrap_or_default().to_string_lossy(),
                            line,
                        ));
                        line += 1;
                    }
                }
                rows
            }
            FileHost::Log => self
                .log()
                .map(|state| {
                    state
                        .file_tree
                        .visible()
                        .into_iter()
                        .enumerate()
                        .filter_map(|(i, r)| match r {
                            crate::files_tree::Row::File { path, name, .. } => {
                                Some(Candidate::new(path.clone(), name.clone(), i + 1))
                            }
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
    fn file_speed(&self, host: FileHost) -> Option<&Search<std::path::PathBuf>> {
        match host {
            FileHost::Commit => self.changes()?.speed.as_ref(),
            FileHost::Log => self.log()?.speed.as_ref(),
        }
    }
    fn file_speed_slot(
        &mut self,
        host: FileHost,
    ) -> Option<&mut Option<Search<std::path::PathBuf>>> {
        match host {
            FileHost::Commit => Some(&mut self.changes_mut()?.speed),
            FileHost::Log => Some(&mut self.logs.get_mut(&self.active_project?)?.speed),
        }
    }
    fn file_speed_update(
        &mut self,
        host: FileHost,
        backwards: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        let rows = self.file_speed_rows(host);
        if let Some(Some(search)) = self.file_speed_slot(host) {
            if let Some(back) = backwards {
                search.step(&rows, back);
            } else {
                search.reconcile(&rows);
            }
            let row = search.matched(&rows).map(|r| (r.row, r.key.clone()));
            if let Some((row, path)) = row {
                self.select_file_match(host, row, path);
            }
        }
        cx.notify();
    }

    /// The match is the host's one selection — decision 54 —
    /// and its row is brought into view.
    fn select_file_match(&mut self, host: FileHost, row: usize, path: std::path::PathBuf) {
        match host {
            FileHost::Commit => {
                let Some(state) = self.changes_mut() else {
                    return;
                };
                let visible = state.visible_paths();
                state.selection.click(&path, &visible, false, false);
                state.selected = state
                    .list
                    .iter()
                    .find(|l| l.row.path == path)
                    .map(|l| (l.section, path.clone()));
                state.scroll.scroll_to_item(row, ScrollStrategy::Top);
            }
            FileHost::Log => {
                let Some(state) = self
                    .active_project
                    .and_then(|project| self.logs.get_mut(&project))
                else {
                    return;
                };
                let order = state.file_paths();
                state.file_selection.click(&path, &order, false, false);
                state.file_scroll.scroll_to_item(row);
            }
        }
    }
    pub(crate) fn file_speed_field(
        &self,
        host: FileHost,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let search = self.file_speed(host);
        let focus = match host {
            FileHost::Commit => &self.changes()?.focus,
            FileHost::Log => &self.log()?.file_focus,
        };
        let project = self.active_project?;
        Some(crate::view::speed_search::field(
            search.map(|s| s.query.as_str()),
            focus,
            search.is_some_and(|s| {
                !s.query.is_empty() && s.matched(&self.file_speed_rows(host)).is_none()
            }),
            self.theme,
            cx.listener(move |root, query: &String, _, cx| {
                if root.active_project != Some(project) {
                    return;
                }
                let current = match host {
                    FileHost::Commit => root.changes().and_then(|s| s.selection.focus.clone()),
                    FileHost::Log => root.log().and_then(|s| s.file_selection.focus.clone()),
                };
                if let Some(slot) = root.file_speed_slot(host) {
                    slot.get_or_insert_with(|| Search::new("", current)).query = query.clone();
                }
                root.file_speed_update(host, None, cx);
            }),
            cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        root.file_speed_update(host, Some(event.keystroke.key == "up"), cx)
                    }
                    "enter" | "escape" | "tab" => {
                        let rows = root.file_speed_rows(host);
                        let path = root
                            .file_speed(host)
                            .and_then(|s| s.matched(&rows))
                            .map(|r| r.key.clone());
                        if let Some(slot) = root.file_speed_slot(host) {
                            *slot = None;
                        }
                        let focus = match host {
                            FileHost::Commit => root.changes().map(|s| s.focus.clone()),
                            FileHost::Log => root.log().map(|s| s.file_focus.clone()),
                        };
                        if let Some(focus) = focus {
                            window.focus(&focus, cx);
                        }
                        if event.keystroke.key == "enter"
                            && let Some(path) = path
                        {
                            if host == FileHost::Commit
                                && let Some(state) = root.changes_mut()
                                && state
                                    .list
                                    .iter()
                                    .any(|l| l.row.path == path && l.row.is_dir)
                            {
                                if !state.expanded.remove(&path) {
                                    state.expanded.insert(path.clone());
                                }
                                state
                                    .selection
                                    .click(&path, &state.visible_paths(), false, false);
                                state.settle_selection();
                                root.reveal_changes_selection();
                            }
                            let base = match host {
                                FileHost::Commit => root
                                    .changes()
                                    .filter(|state| {
                                        state.list.iter().any(|row| {
                                            row.row.path == path
                                                && !row.row.is_dir
                                                && !state.folded.contains(&row.section)
                                        })
                                    })
                                    .map(|_| guild_session::Base::Head),
                                FileHost::Log => root
                                    .log()
                                    .and_then(|s| s.opened.clone())
                                    .map(guild_session::Base::Commit),
                            };
                            if let Some(base) = base {
                                root.open_diff(Some(path), base, window, cx);
                            }
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            }),
            cx.listener(move |root, _: &(), _, cx| {
                match host {
                    FileHost::Commit => {
                        if let Some(s) = root.changes.get_mut(&project) {
                            s.speed = None;
                        }
                    }
                    FileHost::Log => {
                        if let Some(s) = root.logs.get_mut(&project) {
                            s.speed = None;
                        }
                    }
                }
                cx.notify();
            }),
        ))
    }
}

/// A refresh reveals a replacement match; ordinary redraws preserve manual scrolling.
fn refreshed_row<K: Clone + Eq>(
    search: &mut Search<K>,
    rows: &[crate::speed_search::Candidate<K>],
) -> Option<(usize, K)> {
    let before = search.current.clone();
    search.reconcile(rows);
    if before == search.current {
        return None;
    }
    search.matched(rows).map(|r| (r.row, r.key.clone()))
}

impl Root {
    /// What the column draws for `workspace`: its list, with a collapsed
    /// group opened while the search finds one of its projects. The view and
    /// the search's rows both count along this one list.
    pub(crate) fn column_items_for(
        &self,
        workspace: guild_session::WorkspaceId,
    ) -> Vec<crate::column::Item> {
        self.column_items_from(workspace, &self.column_sections_for(workspace))
    }

    /// [`Self::column_items_for`] from sections already worked out — the
    /// view's, which needs them for itself too.
    pub(crate) fn column_items_from(
        &self,
        workspace: guild_session::WorkspaceId,
        sections: &[crate::column::Section],
    ) -> Vec<crate::column::Item> {
        let query = self
            .sidebar_speed
            .as_ref()
            .filter(|_| {
                workspace == self.active_workspace
                    && self.sidebar_speed_workspace == Some(workspace)
            })
            .map(|search| search.query.as_str())
            .unwrap_or_default();
        crate::column::items_found(sections, query)
    }

    pub(crate) fn sidebar_speed_rows(
        &self,
    ) -> Vec<crate::speed_search::Candidate<crate::column::Listed>> {
        // `row` is the place among the list's children, which the scroll
        // handle counts in. A group's header is a candidate as a project's
        // head is; a repository is drawn — so found — inside a collapsed
        // group only while the query finds it.
        self.column_items_for(self.active_workspace)
            .into_iter()
            .enumerate()
            .map(|(row, item)| match item {
                crate::column::Item::Repository { group, .. } => {
                    crate::speed_search::Candidate::new(
                        crate::column::Listed::Repository(group.key),
                        group.name,
                        row,
                    )
                }
                crate::column::Item::Head(head) => crate::speed_search::Candidate::new(
                    crate::column::Listed::Group(head.id),
                    head.name,
                    row,
                ),
            })
            .collect()
    }

    fn settle_sidebar_speed(&mut self) {
        if self.sidebar_speed_workspace != Some(self.active_workspace)
            || (self.settings.column_folded && !self.column_peeking)
        {
            self.sidebar_speed = None;
        }
        // The Sessions page's search ends with the page: another mode, a
        // hidden column.
        if !self.sessions_mode() || (self.settings.column_folded && !self.column_peeking) {
            self.session_speed = None;
        }
        // Rows change under a search — a title, a status, a tab closed: its
        // match is found again and is the chosen row, so what is lit is what
        // Enter opens.
        if self.session_speed.is_some() {
            let rows = self.session_speed_rows();
            let matched = self.session_speed.as_mut().and_then(|search| {
                search.reconcile(&rows);
                search.matched(&rows).map(|row| (row.key.clone(), row.row))
            });
            if let Some((key, row)) = matched {
                self.session_cursor = Some(key.clone());
                // A match that moved — another row, or the same one in
                // another section — is brought into view, as a typed one is.
                if self.session_speed_shown.as_ref() != Some(&(key.clone(), row)) {
                    self.session_speed_shown = Some((key, row));
                    self.reveal_session_row();
                }
            }
        } else {
            self.session_speed_shown = None;
        }
        if self.sidebar_speed.is_none() {
            self.sidebar_speed_revealed = None;
            return;
        }
        let rows = self.sidebar_speed_rows();
        let matched = self.sidebar_speed.as_mut().and_then(|search| {
            search.reconcile(&rows);
            search.matched(&rows).map(|r| (r.key.clone(), r.row))
        });
        if matched != self.sidebar_speed_revealed {
            if let Some((_, row)) = &matched {
                self.project_drag
                    .scroll(self.active_workspace)
                    .scroll_to_top_of_item(*row);
                // So that the frame which scrolls draws where it lands.
                self.column_spacers.reveal(self.active_workspace, *row);
            }
            self.sidebar_speed_revealed = matched;
        }
    }

    fn sidebar_speed_update(&mut self, backwards: Option<bool>, cx: &mut Context<Self>) {
        let rows = self.sidebar_speed_rows();
        if let Some(search) = &mut self.sidebar_speed
            && let Some(back) = backwards
        {
            search.step(&rows, back);
        }
        // Query changes also reveal a candidate the user manually scrolled away from.
        self.sidebar_speed_revealed = None;
        self.settle_sidebar_speed();
        cx.notify();
    }

    pub(crate) fn sidebar_speed_field(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let search = self.sidebar_speed.as_ref();
        let workspace = self.active_workspace;
        crate::view::speed_search::field(
            search.map(|s| s.query.as_str()), &self.sidebar_focus,
            search.is_some_and(|s| !s.query.is_empty() && s.matched(&self.sidebar_speed_rows()).is_none()),
            self.theme,
            cx.listener(move |root, value: &String, _, cx| {
                if root.active_workspace != workspace { return; }
                let current = root.column_groups_for(workspace).into_iter()
                    .find(|g| crate::column::opened_in(g).any(|id| Some(id) == root.active_project))
                    .map(|g| crate::column::Listed::Repository(g.key));
                root.sidebar_speed_workspace = Some(workspace);
                root.sidebar_speed.get_or_insert_with(|| Search::new("", current)).query = value.clone();
                root.sidebar_speed_update(None, cx);
            }),
            cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                if root.active_workspace != workspace { return; }
                match event.keystroke.key.as_str() {
                    "up" | "down" => root.sidebar_speed_update(Some(event.keystroke.key == "up"), cx),
                    "enter" | "escape" | "tab" => {
                        let rows = root.sidebar_speed_rows();
                        let key = root.sidebar_speed.as_ref().and_then(|s| s.matched(&rows)).map(|r| r.key.clone());
                        root.sidebar_speed = None;
                        root.sidebar_speed_revealed = None;
                        window.focus(&root.sidebar_focus, cx);
                        // Enter on a group's header opens the group; it
                        // never collapses one, which a click would.
                        if event.keystroke.key == "enter"
                            && let Some(crate::column::Listed::Group(id)) = &key
                            && root.desk.workspace(workspace)
                                .and_then(|holder| holder.project_group(*id))
                                .is_some_and(|group| group.collapsed)
                        {
                            root.toggle_project_group(workspace, *id, cx);
                        }
                        if event.keystroke.key == "enter"
                            && let Some(crate::column::Listed::Repository(key)) = key
                        {
                            let row = root.column_groups_for(workspace).into_iter().find(|g| g.key == key)
                                .and_then(|g| {
                                    g.rows.iter().find(|r| matches!(r, crate::column::Row::Opened { project, .. } if Some(*project) == root.active_project))
                                        .or_else(|| g.rows.iter().find(|r| matches!(r, crate::column::Row::Opened { .. }))).cloned()
                                });
                            if let Some(row) = row {
                                root.choose_row(&row, cx);
                                root.refocus(window, cx);
                            }
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            }),
            cx.listener(move |root, _: &(), _, cx| {
                if root.active_workspace == workspace {
                    root.sidebar_speed = None;
                    root.sidebar_speed_revealed = None;
                    cx.notify();
                }
            }),
        )
    }
}

/// Where Settled's rows start among the Sessions search's places — past
/// any list a column could hold, so no row of the list shares a place with
/// one of Settled's.
const SETTLED_PLACES: usize = 1 << 20;

impl Root {
    /// The Sessions page's rows as the search finds them: by the session's
    /// name and its project's, in the arrows' order — the list's, then an
    /// open Settled's. A collapsed section's rows are not on the page and
    /// are not found. A row's place is its line in the list, or its row in
    /// Settled past [`SETTLED_PLACES`]: a match that moves between the two
    /// scrolled regions is a match that moved, and is brought into view.
    pub(crate) fn session_speed_rows(
        &self,
    ) -> Vec<crate::speed_search::Candidate<crate::session_list::Key>> {
        let page = self.session_page(self.active_workspace);
        let listed = page
            .list
            .iter()
            .enumerate()
            .filter_map(|(at, line)| match line {
                crate::session_list::Line::Row(key) => Some((key.clone(), at)),
                crate::session_list::Line::Section(..) => None,
            });
        let settled = page
            .settled
            .iter()
            .flat_map(|zone| zone.rows.iter().cloned().enumerate())
            .map(|(at, key)| (key, SETTLED_PLACES + at));
        listed
            .chain(settled)
            .map(|(key, place)| {
                let project = match &key {
                    crate::session_list::Key::Open(id) => self.session_place(*id).project,
                    closed => self
                        .settled_past(closed)
                        .map(|p| self.project_place(p.project, self.active_workspace).project)
                        .unwrap_or_default(),
                };
                let label = format!("{} · {project}", self.session_row_name(&key));
                crate::speed_search::Candidate::new(key, label, place)
            })
            .collect()
    }

    /// Move the Sessions search on — a step, or a fresh match of the query —
    /// and make its match the chosen row, as a list's search does.
    fn session_speed_update(&mut self, backwards: Option<bool>, cx: &mut Context<Self>) {
        let rows = self.session_speed_rows();
        let matched = self.session_speed.as_mut().and_then(|search| {
            match backwards {
                Some(back) => {
                    search.step(&rows, back);
                }
                None => {
                    search.reconcile(&rows);
                }
            }
            search.matched(&rows).map(|row| row.key.clone())
        });
        if let Some(key) = matched {
            self.session_cursor = Some(key);
            self.reveal_session_row();
        }
        cx.notify();
    }

    pub(crate) fn session_speed_field(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let search = self.session_speed.as_ref();
        let workspace = self.active_workspace;
        crate::view::speed_search::field(
            search.map(|s| s.query.as_str()),
            &self.sidebar_focus,
            search.is_some_and(|s| {
                !s.query.is_empty() && s.matched(&self.session_speed_rows()).is_none()
            }),
            self.theme,
            cx.listener(move |root, value: &String, _, cx| {
                if root.active_workspace != workspace {
                    return;
                }
                let current = root.session_chosen();
                root.session_speed
                    .get_or_insert_with(|| Search::new("", current))
                    .query = value.clone();
                root.session_speed_update(None, cx);
            }),
            cx.listener(move |root, event: &KeyDownEvent, window, cx| {
                if root.active_workspace != workspace {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "up" | "down" => {
                        root.session_speed_update(Some(event.keystroke.key == "up"), cx)
                    }
                    "enter" | "escape" | "tab" => {
                        // The lit row, which the search's match keeps up to
                        // date — not a match found afresh at the keystroke.
                        let rows = root.session_speed_rows();
                        let found = root
                            .session_speed
                            .as_ref()
                            .is_some_and(|s| s.matched(&rows).is_some());
                        let key = root.session_cursor.clone().filter(|_| found);
                        root.session_speed = None;
                        window.focus(&root.sidebar_focus, cx);
                        if event.keystroke.key == "enter"
                            && let Some(key) = key
                        {
                            root.activate_session(key, window, cx);
                        }
                        cx.notify();
                    }
                    _ => {}
                }
            }),
            cx.listener(move |root, _: &(), _, cx| {
                if root.active_workspace == workspace {
                    root.session_speed = None;
                    cx.notify();
                }
            }),
        )
    }
}
