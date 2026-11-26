//! Native inspection of actual workflow snapshots. View lifetime owns no effects.
use crate::{Body, Root, TabId};
use gpui::Context;
use guild_session::panes::PaneId;
use guild_session::{
    Seat, SessionId,
    workflow::{OriginBinding, RunId, WorkflowReply},
};
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Presentation {
    #[default]
    None,
    Background,
    Select(u64),
    Recover(u64),
}

#[derive(Default)]
pub(crate) struct Views {
    pub(crate) runs: HashMap<RunId, Snapshot>,
    pending: HashSet<RunId>,
    pub(super) epoch: u64,
    pub(super) closed_at: HashMap<SessionId, u64>,
    // Unloaded restored views are resolved when their existing snapshot arrives.
    closing: Vec<ClosingView>,
    last_poll: Option<Instant>,
}
struct ClosingView {
    tab: TabId,
    pane: PaneId,
    run: RunId,
    origins: HashSet<SessionId>,
}

#[derive(Default)]
pub(crate) struct Snapshot {
    pub(crate) origin_hint: Option<SessionId>,
    pub(crate) value: Option<serde_json::Value>,
    pub(crate) revision: Option<u64>,
    pub(crate) gap: Option<String>,
    // Only an owning inspection reply can resolve an unknown association.
    pub(crate) reply_received: bool,
    pub(crate) selection: crate::process_view::Selection,
    pub(crate) viewports: std::cell::RefCell<HashMap<PaneId, Viewport>>,
}
/// Geometry and scroll belong to an actual pane, including restored duplicate
/// Process bodies. Differently sized panes must not fight over one width cache.
#[derive(Clone, Default)]
pub(crate) struct Viewport {
    pub(crate) scroll: gpui::ScrollHandle,
    pub(crate) width: std::rc::Rc<std::cell::Cell<f32>>,
    pub(crate) focus: Option<gpui::FocusHandle>,
}

impl Root {
    pub(crate) fn process_tab_key(
        &self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !matches!(self.active_body(), Some(Body::Process { .. }))
            || gpui_component::WindowExt::has_focused_input(window, cx)
            || self
                .overlay_focus
                .borrow()
                .values()
                .any(|focus| focus.handle.contains_focused(window, cx))
        {
            return false;
        }
        if self.focus.is_focused(window)
            && crate::view::is_tab_navigation(event)
            && let Some(Body::Process { run }) = self.active_body()
            && let Some(entry) = self.processes.runs.get(run)
            && let Some(pane) = self
                .active_tab
                .and_then(|id| self.tabs.iter().find(|t| t.id == id))
                .map(|t| t.panes.focused())
            && let Some(focus) = entry
                .viewports
                .borrow()
                .get(&pane)
                .and_then(|v| v.focus.clone())
        {
            window.focus(&focus, cx);
        }
        crate::view::process_tab_navigation(event, window, cx)
    }

    /// Only an already-held native job can be revealed. Archive/recovery or
    /// provider launch is deliberately outside this inspection action.
    pub(crate) fn process_job_target(
        &self,
        run: RunId,
        step: &str,
        id: guild_session::terminal_job::JobId,
    ) -> Option<(TabId, PaneId, Seat)> {
        let snapshot = self.processes.runs.get(&run)?.value.as_ref()?;
        let saved = snapshot["reviews"]
            .as_array()?
            .iter()
            .filter(|r| r["step"] == step)
            .find_map(|r| {
                serde_json::from_value::<guild_session::terminal_job::JobReceipt>(r["job"].clone())
                    .ok()
                    .filter(|j| j.id == id)
            })?;
        let held = self.terminal_jobs.receipt(&id)?;
        if held.owner != saved.owner
            || held.project != saved.project
            || self.terminal_jobs.terminal(&id).is_none()
        {
            return None;
        }
        for tab in &self.tabs {
            if tab.seat.workspace != self.active_workspace
                && !self.restored_workspaces.contains(&tab.seat.workspace)
            {
                continue;
            }
            for (pane, body) in tab.panes.bodies() {
                let matches = match held.target.as_ref()? {
                    guild_session::overlay::Target::Session(session) => {
                        body.session() == Some(*session)
                    }
                    guild_session::overlay::Target::Pane(target) => pane == *target,
                };
                if matches {
                    return Some((tab.id, pane, tab.seat));
                }
            }
        }
        None
    }

    pub(crate) fn open_process_job(
        &mut self,
        run: RunId,
        step: &str,
        id: guild_session::terminal_job::JobId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some((tab, pane, seat)) = self.process_job_target(run, step, id) else {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::ProcessJobUnavailable)
                    .to_owned(),
            );
            return;
        };
        self.select_seat(seat, cx);
        self.focus_pane(tab, pane, cx);
        self.active_tab = Some(tab);
        if let Err(reason) = self.open_terminal_presentation(&id, cx) {
            self.warn(reason);
        } else {
            self.focus_terminal_job(&id, window, cx);
        }
        cx.notify();
    }

    pub(crate) fn select_process_step(&mut self, run: RunId, step: &str, cx: &mut Context<Self>) {
        if let Some(entry) = self.processes.runs.get_mut(&run)
            && let Some(snapshot) = &entry.value
            && let Ok((_, phases)) = crate::process_view::phases(snapshot)
            && entry.selection.select(step, &phases)
        {
            cx.notify();
        }
    }

    /// Inspection links never lazily restore a workspace or launch a harness.
    /// Recheck the current native binding at the click, including unfocused panes.
    pub(crate) fn process_session_available(&self, run: RunId, session: SessionId) -> bool {
        let Some(record) = self.desk.session(session).filter(|r| r.open) else {
            return false;
        };
        if record.workspace != self.active_workspace
            && !self.restored_workspaces.contains(&record.workspace)
        {
            return false;
        }
        if !self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .any(|b| b.session() == Some(session))
        {
            return false;
        }
        let Ok(binding) = self.workflow_binding(session) else {
            return false;
        };
        if self.tabs.iter().flat_map(crate::Tab::bodies).any(|body| matches!(body,
            Body::Terminal {session:id,facts,..} if *id==session && facts.resumed().is_some_and(|(kind,conversation)|
                kind!=binding.harness || Some(conversation)!=binding.conversation))) { return false; }
        let Some(snapshot) = self.processes.runs.get(&run).and_then(|r| r.value.as_ref()) else {
            return false;
        };
        let Ok(origin) = serde_json::from_value::<OriginBinding>(snapshot["origin"].clone()) else {
            return false;
        };
        if origin.session == session {
            return self.workflow_binding(session).is_ok_and(|binding| {
                binding.project == origin.project
                    && binding.workspace == origin.workspace
                    && binding.harness == origin.harness
                    && binding.conversation == origin.conversation
                    && binding.cwd == origin.cwd
            });
        }
        self.process_for_session(session).contains(&run)
    }

    pub(crate) fn open_process_session(
        &mut self,
        run: RunId,
        session: SessionId,
        cx: &mut Context<Self>,
    ) {
        if !self.process_session_available(run, session)
            || !self.go_to_session_in_workspace(session, cx)
        {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::ProcessSessionUnavailable)
                    .to_owned(),
            );
        }
    }

    pub(crate) fn process_file_relative(
        &self,
        run: RunId,
        step: &str,
        path: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        let snapshot = self.processes.runs.get(&run)?.value.as_ref()?;
        if !crate::process_view::artifacts(snapshot, step)
            .iter()
            .any(|a| a.path == path)
        {
            return None;
        }
        let origin: OriginBinding = serde_json::from_value(snapshot["origin"].clone()).ok()?;
        if origin.workspace != self.active_workspace {
            return None;
        }
        let root = self.root_of(origin.project, crate::Origin::Project)?;
        crate::process_view::project_file(path, &root)
    }

    pub(crate) fn open_process_file(
        &mut self,
        run: RunId,
        step: &str,
        path: &std::path::Path,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(relative) = self.process_file_relative(run, step, path) else {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::ProcessFileUnavailable)
                    .to_owned(),
            );
            return;
        };
        let Some(snapshot) = self.processes.runs.get(&run).and_then(|r| r.value.as_ref()) else {
            return;
        };
        let Ok(origin) = serde_json::from_value::<OriginBinding>(snapshot["origin"].clone()) else {
            return;
        };
        let Some(root) = self.root_of(origin.project, crate::Origin::Project) else {
            return;
        };
        // Do not turn a recorded path into a link outside its actual project.
        if path.canonicalize().ok().is_none_or(|p| {
            root.canonicalize()
                .ok()
                .is_none_or(|root| !p.starts_with(root))
        }) {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::ProcessFileUnavailable)
                    .to_owned(),
            );
            return;
        }
        self.select_seat(
            Seat {
                project: origin.project,
                workspace: origin.workspace,
            },
            cx,
        );
        self.open_file(&relative, window, cx);
    }

    /// Called only after an admitted close has removed actual session bodies.
    /// View ownership never cancels the graph, reviews, children or worktrees.
    pub(crate) fn close_origin_process_views(
        &mut self,
        sessions: &[SessionId],
        cx: &mut Context<Self>,
    ) {
        let origins: HashSet<_> = sessions
            .iter()
            .copied()
            .filter(|session| {
                !self
                    .tabs
                    .iter()
                    .any(|tab| tab.sessions().any(|s| s == *session))
            })
            .collect();
        if origins.is_empty() {
            return;
        }
        self.processes.epoch = self.processes.epoch.saturating_add(1);
        for origin in &origins {
            self.processes
                .closed_at
                .insert(*origin, self.processes.epoch);
        }
        for tab in &self.tabs {
            for (pane, body) in tab.panes.bodies() {
                if let Body::Process { run } = body {
                    if let Some(candidate) = self
                        .processes
                        .closing
                        .iter_mut()
                        .find(|c| c.tab == tab.id && c.pane == pane)
                    {
                        candidate.origins.extend(&origins);
                    } else {
                        self.processes.closing.push(ClosingView {
                            tab: tab.id,
                            pane,
                            run: *run,
                            origins: origins.clone(),
                        });
                    }
                }
            }
        }
        self.settle_process_view_closes(cx);
    }

    fn settle_process_view_closes(&mut self, cx: &mut Context<Self>) {
        let mut close = Vec::new();
        for candidate in std::mem::take(&mut self.processes.closing) {
            let still_same = self
                .tabs
                .iter()
                .find(|t| t.id == candidate.tab)
                .and_then(|t| t.panes.body_of(candidate.pane))
                .is_some_and(|body| matches!(body, Body::Process { run } if *run == candidate.run));
            if !still_same {
                continue;
            }
            let origin = self
                .processes
                .runs
                .get(&candidate.run)
                .and_then(|s| s.value.as_ref())
                .and_then(|s| s["origin"]["session"].as_str())
                .and_then(|s| s.parse::<SessionId>().ok());
            match origin {
                Some(origin) => {
                    if candidate.origins.contains(&origin)
                        && !self.tabs.iter().any(|t| t.sessions().any(|s| s == origin))
                    {
                        close.push((candidate.tab, candidate.pane));
                    }
                }
                None => self.processes.closing.push(candidate),
            }
        }
        for (tab, pane) in close {
            self.close_pane(tab, pane, cx);
        }
    }

    pub(crate) fn process_view_open(&self, run: RunId) -> bool {
        self.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .any(|body| matches!(body, Body::Process { run: open } if *open == run))
    }
    pub(crate) fn process_tab_tip(&self, run: RunId) -> String {
        let parent = self
            .processes
            .runs
            .get(&run)
            .and_then(|s| s.value.as_ref())
            .and_then(|s| s["origin"]["session"].as_str())
            .and_then(|s| s.parse::<SessionId>().ok())
            .map(|session| {
                self.tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .find(|b| b.session() == Some(session))
                    .and_then(|b| self.session_body_name(b))
                    .unwrap_or_else(|| session.to_string())
            });
        parent.map_or_else(
            || self.ui().text(crate::i18n::Text::Process).to_owned(),
            |p| format!("{} · {}", self.ui().text(crate::i18n::Text::Process), p),
        )
    }
    /// UI links are discovered from actual correlated delivery evidence. The
    /// writer independently verifies the exact assignment again on recovery.
    pub(crate) fn process_for_session(&self, session: SessionId) -> Vec<RunId> {
        let mut runs = self.process_for_origin(session);
        if self.desk.session(session).is_some_and(|r| r.task)
            && let Ok(peer) = self.workflow_binding(session)
        {
            for (run, entry) in &self.processes.runs {
                let Some(value) = entry.value.as_ref() else {
                    continue;
                };
                let launched = value["assignments"].as_array().is_some_and(|rows| {
                    rows.iter().any(|r| {
                        r["observation"]["session"] == serde_json::json!(session)
                            && r["observation"]["bound"] == true
                    })
                });
                let delivered = value["events"].as_array().is_some_and(|events| {
                    events.iter().any(|e| {
                        let o = &e["observation"];
                        o["operation"] == "assignment_delivered"
                            && serde_json::from_value::<OriginBinding>(o["binding"].clone())
                                .is_ok_and(|binding| {
                                    binding.session == peer.session
                                        && binding.project == peer.project
                                        && binding.workspace == peer.workspace
                                        && binding.harness == peer.harness
                                        && binding.conversation == peer.conversation
                                        && binding.cwd == peer.cwd
                                })
                    })
                });
                if launched && delivered {
                    runs.push(*run);
                }
            }
        }
        runs.sort_by_key(ToString::to_string);
        runs.dedup();
        runs
    }
    pub(crate) fn process_navigation_visible(&self, session: SessionId, run: RunId) -> bool {
        !self.process_for_origin(session).contains(&run) || !self.process_view_open(run)
    }
    pub(crate) fn navigate_process(
        &mut self,
        session: SessionId,
        run: RunId,
        cx: &mut Context<Self>,
    ) {
        if !self.process_for_session(session).contains(&run) {
            return;
        }
        if self.process_for_origin(session).contains(&run) {
            self.open_process(run, true, cx);
            return;
        }
        let Some(revision) = self.processes.runs.get(&run).and_then(|s| s.revision) else {
            return;
        };
        // Explicit child navigation may show retained evidence even when recovery
        // reports a gap. It is not a startup or continuation claim.
        self.open_process(run, true, cx);
        let request = guild_session::workflow::WorkflowCommand::Recover {
            run,
            spec: guild_session::workflow::ControlRequest {
                version: 1,
                request_id: format!("navigate-{run}-{revision}"),
                expected_revision: revision,
            },
        };
        let (answer, _) = std::sync::mpsc::channel();
        self.navigate_workflow_recovery(&request, session, answer, cx);
    }
    pub(crate) fn process_for_origin(&self, session: SessionId) -> Vec<RunId> {
        let mut runs: Vec<_> = self
            .processes
            .runs
            .iter()
            .filter_map(|(run, snapshot)| {
                let origin: OriginBinding =
                    serde_json::from_value(snapshot.value.as_ref()?["origin"].clone()).ok()?;
                (origin.session == session).then_some(*run)
            })
            .collect();
        runs.sort_by_key(ToString::to_string);
        runs
    }
    pub(crate) fn observe_process_reply(
        &mut self,
        reply: &WorkflowReply,
        presentation: Presentation,
        cx: &mut Context<Self>,
    ) {
        let (run, value, revision, gap) = match reply {
            WorkflowReply::Receipt {
                run,
                snapshot,
                revision,
                ..
            }
            | WorkflowReply::Snapshot {
                run,
                snapshot,
                revision,
            } => (*run, Some(snapshot.clone()), Some(*revision), None),
            WorkflowReply::Gap {
                run: Some(run),
                message,
                ..
            } => (*run, None, None, Some(message.clone())),
            _ => return,
        };
        self.processes.pending.remove(&run);
        let cached = self.processes.runs.entry(run).or_default();
        let previous_origin = cached.origin_hint;
        if let Some(value) = value {
            if let Ok((_, phases)) = crate::process_view::phases(&value) {
                cached.selection.reconcile(&phases);
            }
            cached.origin_hint = value["origin"]["session"]
                .as_str()
                .and_then(|s| s.parse().ok());
            cached.value = Some(value);
            cached.revision = revision;
        }
        cached.gap = gap;
        cached.reply_received |=
            !matches!(reply, WorkflowReply::Gap {code, ..} if code == "admission");
        let origin = cached
            .value
            .as_ref()
            .and_then(|v| v["origin"]["session"].as_str())
            .and_then(|s| s.parse::<SessionId>().ok());
        let show = cached.value.is_some()
            && match presentation {
                Presentation::None | Presentation::Recover(_) => false,
                Presentation::Background => origin.is_some_and(|origin| {
                    self.tabs.iter().any(|t| t.sessions().any(|s| s == origin))
                }),
                Presentation::Select(epoch) => origin.is_none_or(|origin| {
                    self.processes
                        .closed_at
                        .get(&origin)
                        .is_none_or(|closed| *closed <= epoch)
                }),
            };
        let association_changed = origin.is_some() && origin != previous_origin;
        if association_changed {
            // Restored views may learn ownership without another layout gesture.
            // Persist placement metadata through the existing view-state owner.
            self.store_tabs();
        }
        let linked: Vec<_> = self
            .desk
            .sessions()
            .filter(|r| r.task && r.workflow_run != Some(run))
            .map(|r| r.id)
            .filter(|id| self.process_for_session(*id).contains(&run))
            .collect();
        if !linked.is_empty() {
            for session in linked {
                self.desk.record_workflow_run(session, run);
            }
            self.save();
        }
        self.settle_process_view_closes(cx);
        if show {
            self.open_process(run, matches!(presentation, Presentation::Select(_)), cx);
        }
    }
    /// An explicit inspection action never resumes an origin or starts a provider.
    pub(crate) fn open_process(
        &mut self,
        run: RunId,
        select: bool,
        cx: &mut Context<Self>,
    ) -> Option<TabId> {
        let origin: OriginBinding = serde_json::from_value(
            self.processes.runs.get(&run)?.value.as_ref()?["origin"].clone(),
        )
        .ok()?;
        let existing = self.tabs.iter().find_map(|t| {
            t.panes.bodies().into_iter().find_map(|(p, b)| {
                matches!(b,Body::Process {run:r} if *r==run).then_some((t.id, p, t.seat))
            })
        });
        let seat = existing
            .map(|(_, _, seat)| seat)
            .or_else(|| {
                self.tabs
                    .iter()
                    .find(|t| t.sessions().any(|s| s == origin.session))
                    .map(|t| t.seat)
            })
            .unwrap_or(Seat {
                project: origin.project,
                workspace: origin.workspace,
            });
        if self.desk.project(seat.project).is_none()
            || self.desk.workspace(seat.workspace).is_none()
        {
            return None;
        }
        // Revealing saved evidence must not implicitly restore native runtimes.
        if select
            && seat.workspace != self.active_workspace
            && !self.restored_workspaces.contains(&seat.workspace)
        {
            self.warn(self.ui().text(crate::i18n::Text::ProcessRecoveryGap).to_owned()+": original workspace is not restored; open it explicitly to reconcile native holders");
            return None;
        }
        let (tab, pane) = match existing.map(|(t, p, _)| (t, p)) {
            Some(found) => found,
            None => {
                let neighbor = self
                    .tabs
                    .iter()
                    .position(|t| t.seat == seat && t.sessions().any(|s| s == origin.session));
                let area = neighbor.map(|i| self.tabs[i].area);
                let tab = self.adopt(seat, seat.project, Body::Process { run }, cx);
                if let Some(area) = area {
                    self.tabs.last_mut().unwrap().area = area;
                }
                let made = self.tabs.pop().unwrap();
                let pane = made.panes.focused();
                self.tabs
                    .insert(neighbor.map_or(self.tabs.len(), |i| i + 1), made);
                (tab, pane)
            }
        };
        if select {
            self.select_seat(seat, cx);
            self.focus_pane(tab, pane, cx);
            self.active_tab = Some(tab);
            self.focusing_front = true;
        }
        self.save();
        cx.notify();
        Some(tab)
    }
    pub(crate) fn refresh_process_views(&mut self, cx: &mut Context<Self>) {
        if self
            .processes
            .last_poll
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.processes.last_poll = Some(Instant::now());
        // Restored references load off-thread too; errors belong to that body.
        let mut runs: HashSet<_> = self
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter_map(|b| {
                if let Body::Process { run } = b {
                    Some(*run)
                } else {
                    None
                }
            })
            .collect();
        runs.extend(
            self.tabs
                .iter()
                .flat_map(crate::Tab::sessions)
                .filter_map(|session| self.desk.session(session).and_then(|r| r.workflow_run)),
        );
        for run in runs {
            if !self.processes.pending.insert(run) {
                continue;
            }
            self.processes.runs.entry(run).or_default();
            if let Err(gap) = self.inspect_process(run) {
                self.processes.pending.remove(&run);
                self.processes.runs.entry(run).or_default().gap = Some(gap);
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;
    #[gpui::test]
    fn open_process_matches_exact_run_in_inactive_mixed_panes_and_other_areas(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let project = desk.open_in(workspace, dir.path());
        let run = RunId::new();
        let other = RunId::new();
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().join("state"), cx));
        root.update(cx, |root, cx| {
            let seat = Seat { workspace, project };
            let parent = root.adopt(
                seat,
                project,
                Body::Settings {
                    section: crate::view::settings::Section::Sessions,
                },
                cx,
            );
            root.active_tab = Some(parent);
            root.processes.runs.entry(run).or_default();
            assert!(!root.process_view_open(run), "cache is not an open view");
            root.adopt(seat, project, Body::Process { run: other }, cx);
            assert!(
                !root.process_view_open(run),
                "another RunId is not this view"
            );
            let first_area = root.tabs[0].area;
            let second_area = root
                .stage
                .split(seat, first_area, guild_session::layout::Axis::Horizontal)
                .unwrap();
            let mut panes = guild_session::Panes::one(Body::Settings {
                section: crate::view::settings::Section::Sessions,
            });
            let focused = panes.focused();
            let pane = panes
                .split(focused, guild_session::layout::Axis::Vertical)
                .unwrap();
            panes.fill(pane, Body::Process { run }).ok().unwrap();
            panes.focus(focused);
            let mixed = root.adopt_panes(seat, project, panes, cx);
            root.tabs.iter_mut().find(|t| t.id == mixed).unwrap().area = second_area;
            assert_eq!(root.active_tab, Some(parent));
            assert!(
                root.process_view_open(run),
                "inactive/unfocused Process in another area is open"
            );
            let tab = root.tabs.iter_mut().find(|t| t.id == mixed).unwrap();
            tab.panes.take(pane);
            assert!(
                !root.process_view_open(run),
                "removed pane cannot be kept open by stale cache"
            );
            assert!(root.process_view_open(other));
        });
    }
}

#[cfg(test)]
mod close_tests;

#[cfg(test)]
mod move_tests;

mod movement;
