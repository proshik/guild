//! Open agents in tab/pane order, the status bar's counters and navigation.
//! Harness groups remain available to generic menu introspection; the counters,
//! the popup and the column's Sessions mode read `crate::session_list`.

use gpui::Context;
use guild_session::SessionId;

use crate::agents;
use crate::{Root, Tab};

/// One pane showing an agent: where it is and what it is.
struct AgentPane<'a> {
    tab: &'a Tab,
    body: &'a crate::Body,
    session: SessionId,
    record: &'a guild_session::SessionRecord,
    kind: agents::Agent,
    /// The pane's place in its tab, and how many of the tab's panes show an
    /// agent — two are told apart by place.
    place: usize,
    agents: usize,
}

impl Root {
    /// What kind of agent a tab holds: its session's record, or — a shell
    /// with an agent CLI typed into it, M12.3 — what the terminal runs in
    /// front. `None` for a file, a diff, a shell at its prompt.
    ///
    /// The focused pane's, since M12.8 — the agent in front.
    pub(crate) fn tab_agent(&self, tab: &Tab) -> Option<agents::Agent> {
        self.body_agent(tab.body()?)
    }

    /// [`Root::tab_agent`] for one pane's body.
    pub(crate) fn body_agent(&self, body: &crate::Body) -> Option<agents::Agent> {
        let record = self.desk.session(body.session()?)?;
        agents::kind_of(record.runs).or_else(|| match body {
            crate::Body::Terminal { facts, .. } => {
                facts.harness_in_front().map(agents::Agent::from_harness)
            }
            _ => None,
        })
    }

    /// The agents open in the window, in the order of the tabs.
    ///
    /// **No `App` on purpose**, as `session_tab_name`: the status bar asks
    /// on every frame, and a read of a terminal's entity here tied the
    /// window to every terminal's output. `terminal::facts`.
    ///
    /// Every pane's agent, since M12.8: a split tab holds one per pane.
    pub(crate) fn open_agent_rows(&self) -> Vec<agents::Open> {
        // Every tab's number in its own strip, counted once: asked per tab,
        // `tab_number` walks every tab, and with hundreds of them that was
        // a sixth of a column's frame — 2026-11-23.
        let mut strips = std::collections::HashMap::new();
        let numbers: std::collections::HashMap<crate::TabId, usize> = self
            .tabs
            .iter()
            .map(|tab| {
                let place = strips.entry((tab.seat, tab.area)).or_insert(0);
                *place += 1;
                (tab.id, *place)
            })
            .collect();
        self.open_agent_panes()
            .map(|pane| agents::Open {
                session: pane.session,
                kind: pane.kind,
                // The name the strip gives it — *Session 2* where it has
                // no name of its own, by its place in its own strip,
                // so the list and the strip say the same thing; and
                // *Session 2 · 1* beside another agent of its tab.
                name: self.session_body_name(pane.body).unwrap_or_else(|| {
                    let label = self
                        .ui()
                        .tab_label(pane.record.runs.is_claude(), numbers[&pane.tab.id]);
                    agents::unnamed(&label, pane.place, pane.agents)
                }),
                project: self
                    .desk
                    .project(pane.tab.project)
                    .map(|project| project.name.clone())
                    .unwrap_or_default(),
                workspace: (self.desk.workspaces().len() > 1).then(|| {
                    self.desk
                        .workspace(pane.tab.seat.workspace)
                        .map(|workspace| workspace.name.clone())
                        .unwrap_or_default()
                }),
            })
            .collect()
    }

    /// Every pane showing an agent, in the order of the tabs.
    fn open_agent_panes(&self) -> impl Iterator<Item = AgentPane<'_>> + '_ {
        self.tabs
            .iter()
            .flat_map(|tab| {
                let panes = tab.panes.ids();
                let bodies = tab.panes.bodies();
                let agents = bodies
                    .iter()
                    .filter(|(_, body)| self.body_agent(body).is_some())
                    .count();
                bodies.into_iter().map(move |(pane, body)| {
                    let place = panes.iter().position(|id| *id == pane).unwrap_or(0) + 1;
                    (tab, body, place, agents)
                })
            })
            .filter_map(|(tab, body, place, agents)| {
                let session = body.session()?;
                let record = self.desk.session(session)?;
                let kind = self.body_agent(body)?;
                Some(AgentPane {
                    tab,
                    body,
                    session,
                    record,
                    kind,
                    place,
                    agents,
                })
            })
    }

    pub(crate) fn open_agents(&self) -> Vec<agents::Group> {
        agents::groups(self.open_agent_rows())
    }

    /// The session behind the tab in front, when there is one — what the
    /// counter's menu checks against.
    pub(crate) fn front_session(&self) -> Option<SessionId> {
        self.active_tab
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .and_then(Tab::session)
    }

    /// Bring a session's tab to the front, in its project — the counter's
    /// list and `cmd-shift-u` both end here.
    pub(crate) fn go_to_session(&mut self, session: SessionId, cx: &mut Context<Root>) {
        self.go_to_session_in_workspace(session, cx);
    }
}

impl Root {
    /// When a session last did something: what its program reported —
    /// the record's saved time or the board's live transition, the newer —
    /// and only for a record that has neither, the harness's own
    /// transcript time from the last history read (design: a fallback).
    pub(crate) fn session_last(&self, session: SessionId) -> Option<std::time::SystemTime> {
        let saved = self
            .desk
            .session(session)
            .and_then(|record| record.last_activity)
            .map(|ms| std::time::UNIX_EPOCH + std::time::Duration::from_millis(ms));
        [saved, self.board.transitioned_at(session)]
            .into_iter()
            .flatten()
            .max()
            .or_else(|| self.settled_touched(session))
    }

    /// Every open agent once — what it is and what it is doing, nothing
    /// that needs the column's groups: the status bar asks on every frame.
    pub(crate) fn session_states(
        &self,
    ) -> Vec<(crate::agents::Agent, guild_session::status::Status)> {
        // Without the rows' names: the status bar and the column's foot
        // ask on every frame, and a name is the costly part of a row.
        let mut seen = std::collections::HashSet::new();
        self.open_agent_panes()
            .filter(|pane| seen.insert(pane.session))
            .map(|pane| (pane.kind, self.status_of(pane.session)))
            .collect()
    }

    /// Every open agent once, as the model takes it. A workspace's groups
    /// are arranged once per call, not once per session.
    pub(crate) fn session_items(&self) -> Vec<crate::session_list::Item> {
        let mut seen = std::collections::HashSet::new();
        let mut groups = std::collections::HashMap::new();
        self.open_agent_rows()
            .into_iter()
            .filter(|row| seen.insert(row.session))
            .map(|row| crate::session_list::Item {
                key: crate::session_list::Key::Open(row.session),
                agent: row.kind,
                status: self.status_of(row.session),
                project: self
                    .desk
                    .session(row.session)
                    .map(|record| {
                        let groups = groups
                            .entry(record.workspace)
                            .or_insert_with(|| self.column_groups_for(record.workspace));
                        Self::repository_key(record.project, groups, &self.desk)
                    })
                    .unwrap_or_default(),
                last: self.session_last(row.session),
            })
            .collect()
    }

    /// A project's repository, as the filter tells repositories apart: the
    /// column group's key, which two repositories of one name do not share.
    pub(crate) fn repository_key(
        project: guild_session::ProjectId,
        groups: &[crate::column::Group],
        desk: &guild_session::Desk,
    ) -> String {
        groups
            .iter()
            .find(|group| crate::column::opened_in(group).any(|id| id == project))
            .map(|group| group.key.display().to_string())
            .or_else(|| desk.project(project).map(|p| p.path.display().to_string()))
            .unwrap_or_default()
    }

    /// The status bar's counts, for every open agent together.
    pub(crate) fn session_counts(&self) -> crate::session_list::Counts {
        crate::session_list::counts(self.session_states().into_iter().map(|(_, status)| status))
    }

    /// The popup's one column, by urgency.
    pub(crate) fn popup_sessions(&self) -> Vec<SessionId> {
        crate::session_list::popup_order(self.session_items())
            .into_iter()
            .filter_map(|key| match key {
                crate::session_list::Key::Open(id) => Some(id),
                crate::session_list::Key::Closed { .. } => None,
            })
            .collect()
    }

    /// What the counter says on hover: each agent's counts.
    pub(crate) fn session_breakdown(&self) -> String {
        let ui = self.ui();
        crate::session_list::breakdown(self.session_states())
            .into_iter()
            .map(|(agent, counts)| {
                format!(
                    "{}: {}",
                    ui.text(crate::view::menu::agents::kind_text(agent)),
                    ui.session_counts(counts)
                )
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }

    /// Where a session works, as its rows show it — its project's place.
    pub(crate) fn session_place(&self, session: SessionId) -> SessionPlace {
        self.desk
            .session(session)
            .map(|record| self.project_place(record.project, record.workspace))
            .unwrap_or_default()
    }

    /// Where a project of `workspace` is, as a session's row shows it: the
    /// repository's name, tile and icon, the branch, whether it is the
    /// primary checkout, and the lines its branch changed.
    pub(crate) fn project_place(
        &self,
        project: guild_session::ProjectId,
        workspace: guild_session::WorkspaceId,
    ) -> SessionPlace {
        let project = self.desk.project(project);
        let worktree =
            project.and_then(|p| self.worktrees.values().flatten().find(|w| w.path == p.path));
        let branch = worktree.and_then(|w| w.branch.clone()).or_else(|| {
            project
                .filter(|p| Some(p.id) == self.active_project)
                .and(self.branch.clone())
        });
        // Match the sidebar's repository tile and saved icon, including roots
        // whose settings identity has not finished resolving yet.
        let groups = self.column_groups_for(workspace);
        let group = project.and_then(|project| {
            groups
                .iter()
                .find(|group| crate::column::opened_in(group).any(|id| id == project.id))
        });
        let key = project
            .and_then(|p| self.repository_identity(p.id))
            .map(|r| r.key.clone())
            .or_else(|| group.map(|group| group.key.clone()));
        let icon = key
            .as_ref()
            .and_then(|key| self.settings.repository_settings.get(key))
            .and_then(|s| s.icon.as_deref())
            .and_then(crate::icons::workspace_path);
        let row = group.and_then(|group| {
            group.rows.iter().find(|row| {
                matches!(row, crate::column::Row::Opened { project: id, .. }
                    if Some(*id) == project.map(|p| p.id))
            })
        });
        let (primary, lines) = match row {
            Some(crate::column::Row::Opened { title, lines, .. }) => (title.primary, *lines),
            _ => (false, None),
        };
        SessionPlace {
            project: group
                .map(|group| group.name.clone())
                .or_else(|| project.map(|p| p.name.clone()))
                .unwrap_or_default(),
            icon,
            branch,
            primary,
            lines,
        }
    }
}

/// Where a session works, as a row shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SessionPlace {
    /// The repository's name, the tile's letter and colour.
    pub(crate) project: String,
    /// The repository's chosen icon, if any.
    pub(crate) icon: Option<&'static str>,
    /// The worktree's branch.
    pub(crate) branch: Option<String>,
    /// The repository's own checkout: the star.
    pub(crate) primary: bool,
    /// Lines its branch added and removed.
    pub(crate) lines: Option<(u32, u32)>,
}

impl Root {
    /// The status bar's session control: state counters for every open
    /// agent together — a failure only when there is one, then working,
    /// waiting, done and, last and quiet, idle, each hidden at zero — and
    /// nothing at all when no agent is open: there is nothing to list.
    /// Its look does not follow the column's mode.
    pub(crate) fn render_session_summary(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        use crate::theme;
        use crate::view::menu::{Anchor, Target};
        use gpui::{div, prelude::*, rgb};
        use guild_session::status::Status;
        let counts = self.session_counts();
        if counts.is_empty() {
            return div().into_any_element();
        }
        let palette = self.theme;
        let at = self.counter_at.clone();
        let root = cx.entity().downgrade();
        let open = matches!(
            self.context_menu.as_ref().map(|m| &m.target),
            Some(Target::Agents { .. })
        );
        let breakdown = self.session_breakdown();
        let count = |name: &'static str, n: usize, mark: gpui::AnyElement| {
            (n > 0).then(|| {
                div()
                    .debug_selector(move || format!("session-count-{name}"))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(theme::activity::COUNT_GAP)
                    // Idle is the quiet one: its number in its mark's colour.
                    .when(name == "idle", |count| {
                        count.text_color(rgb(crate::view::activity::color(Status::Idle, palette)))
                    })
                    .child(mark)
                    .child(n.to_string())
                    .into_any_element()
            })
        };
        let waiting = crate::icons::toolbar("ui/activity-question.svg")
            .size(theme::activity::GLYPH)
            .text_color(rgb(palette.text.secondary))
            .into_any_element();
        let counters: Vec<gpui::AnyElement> = [
            count(
                "failed",
                counts.failed,
                crate::view::activity::mark(Status::Failed, palette, false, false),
            ),
            count(
                "working",
                counts.working,
                crate::view::activity::mark(Status::Working, palette, true, false),
            ),
            count("waiting", counts.waiting, waiting),
            count(
                "done",
                counts.done,
                crate::view::activity::mark(Status::Done, palette, false, false),
            ),
            // The popup's idle dot, shown rather than left blank.
            count(
                "idle",
                counts.idle,
                crate::view::activity::mark(Status::Idle, palette, false, true),
            ),
        ]
        .into_iter()
        .flatten()
        .collect();
        div()
            .id("session-list")
            .debug_selector(|| "session-list".into())
            .relative()
            .flex()
            .flex_none()
            .min_w_0()
            .max_w_full()
            .items_center()
            .gap(theme::activity::COUNTS_GAP)
            .h(theme::space::STATUS_BUTTON)
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .text_size(theme::size::META)
            .text_color(rgb(palette.text.secondary))
            .when(open, |b| b.bg(rgb(palette.surface.raised)))
            .hover(|b| b.bg(crate::theme::control_hover(palette, palette.surface.panel)))
            .children(counters)
            .when(!open, |b| {
                let text = breakdown.clone();
                use crate::view::tooltip::Hint as _;
                b.hint_delay().tooltip(move |_, app| {
                    app.new(|_| crate::view::tooltip::Tooltip::text(text.clone(), palette))
                        .into()
                })
            })
            // The anchor is the control's own box: placed at its corner, not
            // after its padding — the popup sat one gap to the right.
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        if at.get() != bounds {
                            at.set(bounds);
                            // Footer relayout must also move the Root-owned popup.
                            let root = root.clone();
                            cx.defer(move |cx| {
                                let _ = root.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_click(cx.listener(|root, _, window, cx| {
                cx.stop_propagation();
                if matches!(
                    root.context_menu.as_ref().map(|m| &m.target),
                    Some(Target::Agents { .. })
                ) {
                    root.close_context_menu(window, cx);
                } else {
                    root.open_menu(
                        Target::Agents {
                            groups: root.open_agents(),
                            front: root.front_session(),
                        },
                        Anchor::Above(root.counter_at.get()),
                        window,
                        cx,
                    );
                }
            }))
            .into_any_element()
    }
}
