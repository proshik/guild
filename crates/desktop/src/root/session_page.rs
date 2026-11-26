//! The column's Sessions mode in the window: what a workspace's page lists,
//! in what order, which row is chosen, and what a row does — the model is
//! `crate::session_list`; this is where the window's tabs, records and the
//! Settled history are turned into its items.

use gpui::{Context, Window};
use guild_session::WorkspaceId;
use guild_session::settings::ColumnMode;

use crate::Root;
use crate::session_list::{Item, Key, Line, Listing, Page, Urgency};

impl Root {
    /// Whether the column lists sessions.
    pub(crate) fn sessions_mode(&self) -> bool {
        self.settings.column_mode == ColumnMode::Sessions
    }

    /// The open agents of `workspace`, as the model takes them.
    fn open_items_of(&self, workspace: WorkspaceId) -> Vec<Item> {
        self.session_items()
            .into_iter()
            .filter(|item| match &item.key {
                Key::Open(id) => self
                    .desk
                    .session(*id)
                    .is_some_and(|record| record.workspace == workspace),
                Key::Closed { .. } => false,
            })
            .collect()
    }

    /// The conversations `workspace`'s folders remember that no open tab
    /// holds, as the model takes them.
    fn closed_items_of(&self, workspace: WorkspaceId) -> Vec<Item> {
        let projects = self
            .desk
            .workspace(workspace)
            .map(|w| w.projects.clone())
            .unwrap_or_default();
        let held: Vec<(guild_session::HarnessKind, String)> = self
            .desk
            .sessions()
            .filter(|record| record.open)
            .filter_map(|record| Some((record.runs.harness()?, Self::past_id(record)?)))
            .collect();
        let mut seen = std::collections::HashSet::new();
        let groups = self.column_groups_for(workspace);
        self.settled
            .pasts
            .iter()
            .filter(|p| projects.contains(&p.project))
            .filter(|p| !held.contains(&(p.past.harness, p.past.id.clone())))
            .filter(|p| seen.insert((p.past.harness, p.past.id.clone())))
            .map(|p| Item {
                key: Key::Closed {
                    harness: p.past.harness,
                    id: p.past.id.clone(),
                },
                agent: crate::agents::Agent::from_harness(p.past.harness),
                status: guild_session::status::Status::Idle,
                project: Self::repository_key(p.project, &groups, &self.desk),
                last: Some(p.past.touched),
            })
            .collect()
    }

    /// What `workspace`'s Sessions page lists, through the filter.
    pub(crate) fn session_listing(&self, workspace: WorkspaceId) -> Listing {
        crate::session_list::listing(
            self.open_items_of(workspace),
            self.closed_items_of(workspace),
            std::time::SystemTime::now(),
            std::time::Duration::from_secs(
                u64::from(self.settings.settle_after_days) * 24 * 60 * 60,
            ),
            &self.session_filter,
        )
    }

    /// What `workspace`'s Sessions page draws: the expanded sections in
    /// the scrolled list, the collapsed ones and Settled at the foot — as
    /// the window's folds say, one choice for every workspace.
    pub(crate) fn session_page(&self, workspace: WorkspaceId) -> Page {
        crate::session_list::page(
            self.session_listing(workspace),
            &self.settings.session_folds,
        )
    }

    /// The rows a person can choose on the shown page, in order: no row
    /// of a collapsed section, and Settled's only while it is open.
    pub(crate) fn session_rows(&self) -> Vec<Key> {
        self.session_page(self.active_workspace).rows()
    }

    /// Collapse a status section into the stack at the column's foot, or
    /// expand it back into its place; kept for every workspace.
    pub(crate) fn toggle_session_section(&mut self, urgency: Urgency, cx: &mut Context<Root>) {
        self.settings.session_folds.toggle(urgency.into());
        self.save_settings();
        cx.notify();
    }

    /// Open Settled, or close it; kept for every workspace.
    pub(crate) fn toggle_settled(&mut self, cx: &mut Context<Root>) {
        let folds = &mut self.settings.session_folds;
        folds.settled_open = !folds.settled_open;
        self.save_settings();
        cx.notify();
    }

    /// The row drawn as chosen: the arrows' or the search's while it is
    /// still listed, else the session in front while it is — never a row a
    /// collapsed section or a closed Settled hides, which Enter would open
    /// unseen.
    pub(crate) fn session_chosen(&self) -> Option<Key> {
        let rows = self.session_rows();
        self.session_cursor
            .clone()
            .or_else(|| self.front_session().map(Key::Open))
            .filter(|key| rows.contains(key))
            .or_else(|| {
                self.front_session()
                    .map(Key::Open)
                    .filter(|key| rows.contains(key))
            })
    }

    /// When a row's session last did something: an open one's activity, a
    /// closed one's transcript time.
    pub(crate) fn session_row_last(&self, key: &Key) -> Option<std::time::SystemTime> {
        match key {
            Key::Open(id) => self.session_last(*id),
            closed => self.settled_past(closed).map(|p| p.past.touched),
        }
    }

    /// The SettledPast a closed row stands for.
    pub(crate) fn settled_past(&self, key: &Key) -> Option<&crate::root::settled::SettledPast> {
        let Key::Closed { harness, id } = key else {
            return None;
        };
        self.settled
            .pasts
            .iter()
            .find(|p| p.past.harness == *harness && &p.past.id == id)
    }

    /// What a row is called: an open one as the popup names it; a closed
    /// one by the name a person gave its record, else by the harness's own
    /// title for it.
    pub(crate) fn session_row_name(&self, key: &Key) -> String {
        match key {
            Key::Open(id) => self
                .open_agent_rows()
                .into_iter()
                .find(|row| row.session == *id)
                .map(|row| row.name)
                .unwrap_or_default(),
            Key::Closed { harness, id } => self
                .desk
                .sessions()
                .find(|record| {
                    record.runs.harness() == Some(*harness)
                        && Self::past_id(record).as_deref() == Some(id.as_str())
                })
                .and_then(|record| record.name.clone())
                .or_else(|| self.settled_past(key).map(|p| p.past.title.clone()))
                .unwrap_or_default(),
        }
    }

    /// Move the chosen row one down or up, wrapping, and show it.
    pub(crate) fn step_session_cursor(&mut self, down: bool, cx: &mut Context<Root>) {
        let rows = self.session_rows();
        if rows.is_empty() {
            return;
        }
        let at = self
            .session_chosen()
            .and_then(|key| rows.iter().position(|row| *row == key));
        let next = match at {
            Some(at) if down => (at + 1) % rows.len(),
            Some(at) => (at + rows.len() - 1) % rows.len(),
            None if down => 0,
            None => rows.len() - 1,
        };
        self.session_cursor = Some(rows[next].clone());
        self.reveal_session_row();
        cx.notify();
    }

    /// Scroll the chosen row into view: in the list by its place among the
    /// list's lines, in Settled by its place among Settled's rows.
    pub(crate) fn reveal_session_row(&mut self) {
        let Some(key) = self.session_cursor.clone() else {
            return;
        };
        let page = self.session_page(self.active_workspace);
        if let Some(at) = page
            .list
            .iter()
            .position(|line| *line == Line::Row(key.clone()))
        {
            self.session_scroll.scroll_to_item(at);
        } else if let Some(at) = page
            .settled
            .iter()
            .flat_map(|zone| zone.rows.iter())
            .position(|row| *row == key)
        {
            self.settled_scroll.scroll_to_item(at);
        }
    }

    /// What choosing a row does: an open session's tab comes to the front;
    /// a closed one is continued in its project, the start screen's way.
    pub(crate) fn activate_session(
        &mut self,
        key: Key,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        self.session_cursor = None;
        match key {
            Key::Open(id) => {
                self.go_to_session(id, cx);
                self.focusing_front = true;
                self.refocus(window, cx);
            }
            closed @ Key::Closed { .. } => {
                let Some(found) = self.settled_past(&closed).cloned() else {
                    return;
                };
                // Its own project goes in front once it can start: the start
                // screen's path opens the tab in the project it is given,
                // and the strip, the tools and the saved selection follow
                // the one in front.
                let from = crate::root::start::ResumeFrom {
                    into: None,
                    select: true,
                };
                self.resume_past(found.project, found.past, from, window, cx);
            }
        }
        cx.notify();
    }

    /// Show Projects with the row's worktree chosen.
    pub(crate) fn show_session_in_projects(&mut self, key: Key, cx: &mut Context<Root>) {
        if self.sessions_mode() {
            self.toggle_column_mode(cx);
        }
        match key {
            Key::Open(id) => {
                self.go_to_session(id, cx);
            }
            closed @ Key::Closed { .. } => {
                if let Some(project) = self.settled_past(&closed).map(|p| p.project) {
                    self.select_project(project, cx);
                }
            }
        }
        cx.notify();
    }

    /// The tab that holds an open session.
    pub(crate) fn tab_of_session(&self, id: guild_session::SessionId) -> Option<crate::TabId> {
        self.tabs
            .iter()
            .find(|tab| tab.sessions().any(|held| held == id))
            .map(|tab| tab.id)
    }
}
