//! Switching the workspace-local arrangement without restarting roots.

use gpui::prelude::*;
use gpui::{Context, ScrollDelta, ScrollWheelEvent};
use guild_session::{ProjectId, Seat, WorkspaceId};

use crate::Root;
use crate::actions::{NextWorkspace, PreviousWorkspace};

impl Root {
    pub(crate) fn workspace_refusal(&self, refusal: guild_session::Refused) -> String {
        match refusal {
            guild_session::Refused::Capacity => self
                .ui()
                .text(crate::i18n::Text::WorkspaceCapacity)
                .to_owned(),
            other => other.to_string(),
        }
    }

    pub(crate) fn delete_workspace_asking(
        &mut self,
        id: WorkspaceId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.asking {
            return;
        }
        if self.desk.workspaces().len() == 1 {
            self.inform(
                self.ui()
                    .text(crate::i18n::Text::WorkspaceLastCannotDelete)
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        let Some(workspace) = self.desk.workspace(id) else {
            return;
        };
        let name = workspace.name.clone();
        let released: Vec<String> = workspace
            .projects
            .iter()
            .filter_map(|project| {
                (self.desk.holders(*project).len() == 1)
                    .then(|| {
                        self.desk
                            .project(*project)
                            .map(|project| project.name.clone())
                    })
                    .flatten()
            })
            .collect();
        let detail = self.ui().delete_workspace_detail(&released);
        let title = self.ui().delete_workspace_title(&name);
        let buttons = [
            gpui::PromptButton::new(self.ui().text(crate::i18n::Text::WorkspaceDelete)),
            gpui::PromptButton::cancel(self.ui().text(crate::i18n::Text::Cancel)),
        ];
        self.autosave_files(crate::autosave::Trigger::Close, cx);
        let dirty = self.dirty_workspace_tabs(id, cx);
        let walk = self.walk_dirty(dirty, true, window, cx);
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let mut proceed = walk.await;
                if proceed {
                    proceed = match cx.update(|window, cx| {
                        window.prompt(
                            gpui::PromptLevel::Warning,
                            &title,
                            Some(&detail),
                            &buttons,
                            cx,
                        )
                    }) {
                        Ok(answer) => answer.await == Ok(0),
                        Err(_) => false,
                    };
                }
                let _ = cx.update(|_, cx| {
                    root.update(cx, |root, cx| {
                        root.asking = false;
                        if proceed {
                            root.delete_workspace_now(id, cx);
                        }
                    })
                });
            })
            .detach();
    }

    fn delete_workspace_now(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.delete_workspace_now_without_history(id, cx);
        self.history.suspended -= 1;
    }

    fn delete_workspace_now_without_history(&mut self, id: WorkspaceId, cx: &mut Context<Self>) {
        // Covered again below by whichever of `select_workspace` and
        // `close_project_now` this deletion goes through, but a workspace
        // with no projects left in it goes through neither — M12.7 final
        // fix.
        self.close_menu_for_switch();
        let projects: Vec<_> = self
            .desk
            .projects_of(id)
            .map(|project| (project.id, project.path.clone()))
            .collect();
        if self.active_workspace != id {
            self.select_workspace(id, cx);
        }
        for (project, _) in &projects {
            if self.desk.holders(*project).contains(&id) {
                self.close_project_now(*project, cx);
            }
        }
        if let Ok(released) = self.desk.delete_workspace(id) {
            for project in released {
                self.auto_ff_released(project);
            }
        }
        // Closing stores the seat one last time. Forget it only afterwards,
        // or a deleted workspace can spring back when this root is adopted.
        for (_, path) in &projects {
            let mut state = guild_session::project_state::load(path);
            state.forget_seat(id);
            state.sessions.retain(|session| session.workspace != id);
            if let Err(err) = guild_session::project_state::save(path, &state) {
                tracing::warn!(project = %path.display(), %err, "the deleted workspace seat was not forgotten");
            }
        }
        self.restored_workspaces.remove(&id);
        self.active_workspace = self.desk.active_workspace();
        self.active_project = self.desk.active_project();
        self.come_back();
        self.save();
        self.rebuild_menu(cx);
        cx.notify();
    }

    pub(crate) fn active_seat(&self) -> Option<Seat> {
        self.active_project.map(|project| Seat {
            workspace: self.active_workspace,
            project,
        })
    }

    /// Select a workspace while leaving root-owned workers alive.
    pub(crate) fn select_workspace(
        &mut self,
        workspace: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.note_place(cx);
        self.history.suspended += 1;
        let result = self.select_workspace_without_history(workspace, cx);
        self.history.suspended -= 1;
        result
    }

    fn select_workspace_without_history(
        &mut self,
        workspace: WorkspaceId,
        cx: &mut Context<Self>,
    ) -> bool {
        if workspace == self.active_workspace || !self.desk.select_workspace(workspace) {
            return false;
        }
        // The seat being left writes the tab in front and its area as they
        // are now, not as the last save heard them — M12.8: `store_tabs`
        // reads both from the stage.
        self.note_front();
        self.store_tabs();
        self.store_expanded();
        self.sidebar_speed = None;
        self.sidebar_speed_revealed = None;
        self.active_workspace = workspace;
        self.active_project = self.desk.active_project();
        self.branch_read = None;
        self.put_fields_away();
        self.restore_tabs(cx);
        self.come_back();
        self.save();
        self.rebuild_menu(cx);
        cx.notify();
        true
    }

    /// Bring a seat forward, switching workspace first when necessary.
    pub(crate) fn select_seat(&mut self, seat: Seat, cx: &mut Context<Self>) -> bool {
        self.note_place(cx);
        self.history.suspended += 1;
        let result = self.select_seat_without_history(seat, cx);
        self.history.suspended -= 1;
        result
    }

    fn select_seat_without_history(&mut self, seat: Seat, cx: &mut Context<Self>) -> bool {
        if !self.desk.holders(seat.project).contains(&seat.workspace) {
            return false;
        }
        if seat.workspace != self.active_workspace {
            self.select_workspace(seat.workspace, cx);
        }
        // `select_workspace` already put a menu away when the workspace
        // changed above; this also covers the seat's own project changing
        // within the same workspace, which it would not have. M12.7 final
        // fix.
        self.close_menu_for_switch();
        self.active_project = Some(seat.project);
        self.desk.look_at(Some(seat.project), self.active_session());
        self.come_back();
        self.unfold_group_of(seat.project);
        self.save();
        cx.notify();
        true
    }

    /// Resolve the workspace of a session and bring its exact tab forward.
    pub(crate) fn go_to_session_in_workspace(
        &mut self,
        session: guild_session::SessionId,
        cx: &mut Context<Self>,
    ) -> bool {
        self.note_place(cx);
        self.history.suspended += 1;
        let result = self.go_to_session_in_workspace_without_history(session, cx);
        self.history.suspended -= 1;
        result
    }

    fn go_to_session_in_workspace_without_history(
        &mut self,
        session: guild_session::SessionId,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(record) = self.desk.session(session) else {
            return false;
        };
        let seat = Seat {
            workspace: record.workspace,
            project: record.project,
        };
        // A removed/closed target cannot select an unrelated tab. A workspace
        // not restored yet still needs its existing lazy restoration path.
        if !record.open
            || (self.restored_workspaces.contains(&seat.workspace)
                && self
                    .find_showing(seat, |body| body.session() == Some(session))
                    .is_none())
        {
            return false;
        }
        if !self.select_seat(seat, cx) && self.active_seat() != Some(seat) {
            return false;
        }
        if let Some((tab, pane)) = self.find_showing(seat, |body| body.session() == Some(session)) {
            self.focus_pane(tab, pane, cx);
            // A row pressed: the keys go with the outline, even to the
            // other pane of the tab already in front (12.2).
            self.focusing_front = true;
            self.active_tab = Some(tab);
            self.save();
            cx.notify();
            return true;
        }
        false
    }

    pub(crate) fn seat_for_project(&self, project: ProjectId) -> Seat {
        Seat {
            workspace: self.active_workspace,
            project,
        }
    }

    /// Move through the switcher's order. Keyboard traversal wraps; a
    /// trackpad carousel deliberately does not and passes an exact target.
    pub(crate) fn step_workspace(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let workspaces = self.desk.workspaces();
        if workspaces.len() < 2 {
            self.inform(
                self.ui()
                    .text(crate::i18n::Text::WorkspaceOnlyOne)
                    .to_owned(),
            );
            cx.notify();
            return false;
        }
        let current = workspaces
            .iter()
            .position(|workspace| workspace.id == self.active_workspace)
            .unwrap_or(0);
        let next = crate::workspaces::stepped(workspaces.len(), current, forward);
        self.select_workspace(workspaces[next].id, cx)
    }

    pub(crate) fn workspace_activity(
        &self,
        workspace: WorkspaceId,
    ) -> guild_session::status::Activity {
        let sessions: Vec<_> = self
            .desk
            .projects_of(workspace)
            .flat_map(|project| {
                self.desk
                    .sessions_of(Seat {
                        workspace,
                        project: project.id,
                    })
                    .map(|record| record.id)
            })
            .collect();
        self.board.summary(sessions)
    }

    /// Feed one precise wheel event from the sidebar into the measured
    /// detector. Answers whether the sidebar claimed it.
    pub(crate) fn workspace_swipe_heard(
        &mut self,
        event: &ScrollWheelEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.worktree_removal.is_some() {
            return true;
        }
        let elapsed = self.workspace_swipe_epoch.elapsed();
        let (precise, dx, dy) = match event.delta {
            ScrollDelta::Pixels(delta) => (true, f32::from(delta.x), f32::from(delta.y)),
            ScrollDelta::Lines(delta) => (false, delta.x, delta.y),
        };
        let workspaces = self.desk.workspaces();
        let position = workspaces
            .iter()
            .position(|workspace| workspace.id == self.active_workspace)
            .unwrap_or(0);
        let ends = (position > 0, position + 1 < workspaces.len());
        let (take, motion) = self.workspace_swipe.heard(
            elapsed,
            event.touch_phase,
            precise,
            dx,
            dy,
            crate::system::natural_scrolling(),
            self.settings.column_width,
            ends,
        );
        self.apply_workspace_motion(motion, cx);
        take == crate::swipe::Take::Swallow
    }

    pub(crate) fn apply_workspace_motion(
        &mut self,
        motion: crate::swipe::Motion,
        cx: &mut Context<Self>,
    ) {
        match motion {
            crate::swipe::Motion::None => {}
            crate::swipe::Motion::Follow(offset) => {
                self.workspace_swipe_landing = self.workspace_swipe_landing.wrapping_add(1);
                self.workspace_swipe_offset = offset;
                cx.notify();
            }
            crate::swipe::Motion::Land { to, from } => {
                self.land_workspace_swipe(to, from, cx);
            }
        }
    }

    fn land_workspace_swipe(
        &mut self,
        to: Option<crate::swipe::Step>,
        from: f32,
        cx: &mut Context<Self>,
    ) {
        let workspaces = self.desk.workspaces();
        let position = workspaces
            .iter()
            .position(|workspace| workspace.id == self.active_workspace)
            .unwrap_or(0);
        let target = match to {
            Some(crate::swipe::Step::Previous) if position > 0 => Some(workspaces[position - 1].id),
            Some(crate::swipe::Step::Next) if position + 1 < workspaces.len() => {
                Some(workspaces[position + 1].id)
            }
            _ => None,
        };
        let end = match to {
            Some(crate::swipe::Step::Previous) if target.is_some() => self.settings.column_width,
            Some(crate::swipe::Step::Next) if target.is_some() => -self.settings.column_width,
            _ => 0.0,
        };
        self.workspace_swipe_landing = self.workspace_swipe_landing.wrapping_add(1);
        let landing = self.workspace_swipe_landing;
        cx.spawn(async move |root, cx| {
            const FRAMES: u32 = 12;
            for frame in 1..=FRAMES {
                cx.background_executor()
                    .timer(crate::swipe::LANDING / FRAMES)
                    .await;
                let alive = root.update(cx, |root, cx| {
                    if root.workspace_swipe_landing != landing {
                        return false;
                    }
                    let t = frame as f32 / FRAMES as f32;
                    let eased = 1.0 - (1.0 - t).powi(3);
                    root.workspace_swipe_offset = from + (end - from) * eased;
                    if frame == FRAMES {
                        if let Some(target) = target {
                            root.select_workspace(target, cx);
                        }
                        root.workspace_swipe_offset = 0.0;
                    }
                    cx.notify();
                    true
                });
                if !matches!(alive, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    /// The workspace actions, separate from project traversal so their
    /// shortcuts remain independent.
    pub(crate) fn bind_workspaces(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &NextWorkspace, _window, cx| {
            root.step_workspace(true, cx);
        }))
        .on_action(cx.listener(|root, _: &PreviousWorkspace, _window, cx| {
            root.step_workspace(false, cx);
        }))
    }
}
