//! Caller-checked terminal commands share the retained native job registry.
use crate::{Root, terminal::view::Look};
use gpui::Context;
use guild_session::{
    command::Answer,
    overlay::{Content, Target},
    terminal_job::{
        HistoryContext, JobError, JobId, Presentation, Scope, TerminalCommand, TerminalReply,
    },
};

impl Root {
    pub(crate) fn open_revmux_archive(
        &mut self,
        id: &JobId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Result<(), JobError> {
        let target = self
            .active_tab
            .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
            .map(|tab| Target::Pane(tab.panes.focused()))
            .ok_or("Select an existing pane first")?;
        let viewer = self.terminal_jobs.open_archive(
            id,
            target.clone(),
            Look::of(&self.settings, self.theme),
            cx,
        )?;
        if let Some(view) = self.terminal_jobs.terminal(&viewer.id) {
            Self::hear_terminal_input(&view, cx);
        }
        if let Some(error) = viewer.error.as_ref() {
            return Err(error.clone());
        }
        self.terminal_geometry.entry(viewer.id).or_insert(Some(
            guild_session::terminal_job::Geometry {
                width_percent: 90,
                height_percent: 90,
            },
        ));
        self.reattach_terminal_job(&viewer.id, target, window, cx)
    }

    pub(crate) fn command_terminal(
        &mut self,
        request: &TerminalCommand,
        caller: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Answer {
        let result = (|| -> Result<TerminalReply, JobError> {
            let (owner, current) = super::command::terminal_callers(caller, &self.started_as)?;
            let active_caller = current.to_string();
            let pane = self.tabs.iter().find_map(|tab| {
                tab.panes.ids().into_iter().find(|pane| {
                    tab.panes.body_of(*pane).and_then(crate::Body::session) == Some(current)
                })
            });
            let resolved = super::command::resolved_terminal_request(request, &self.started_as);
            let target = match &resolved {
                TerminalCommand::Start { spec } => Some(&spec.target),
                _ => None,
            };
            let (_, project) =
                crate::terminal_job::authorize(&self.desk, Some(&active_caller), target, pane)?;
            self.terminal_jobs.poll(cx);
            match request {
                TerminalCommand::Capabilities => {
                    return Ok(TerminalReply::Capabilities {
                        version: 1,
                        operations: [
                            "start",
                            "status",
                            "wait",
                            "show",
                            "hide",
                            "close",
                            "cancel",
                            "archive_viewer",
                            "revmux_archive",
                        ]
                        .into_iter()
                        .map(str::to_owned)
                        .collect(),
                    });
                }
                TerminalCommand::Start { spec } => {
                    self.workflow_review_terminal_guard(spec, current, cx)?;
                    let existing = self
                        .terminal_jobs
                        .receipt_snapshot()
                        .iter()
                        .any(|r| r.owner == owner && r.request_id == spec.request_id);
                    let context = self.terminal_launch_context(current, project, &spec.cwd);
                    let receipt = self.terminal_jobs.start_in(
                        owner,
                        project,
                        spec,
                        Some(context),
                        Look::of(&self.settings, self.theme),
                        cx,
                    )?;
                    if let Some(view) = self.terminal_jobs.terminal(&receipt.id) {
                        Self::hear_terminal_input(&view, cx);
                    }
                    self.workflow_review_job_started(spec, &receipt);
                    if !existing {
                        self.review_activity.launched(receipt.id);
                    }
                    if !existing && self.terminal_jobs.terminal(&receipt.id).is_some() {
                        // Authority and the reservation fingerprint belong to
                        // the launching process. Only presentation follows its
                        // current conversation, which can change after resume.
                        if let Err(error) = self.terminal_jobs.present(
                            &receipt.id,
                            target.cloned(),
                            Presentation::Closed,
                        ) {
                            self.warn(error);
                        }
                        let occupied = target.is_some_and(|target| {
                            self.overlays.at(target).iter().any(|entry| {
                                entry.visible && matches!(entry.content, Content::Terminal { .. })
                            })
                        });
                        if occupied && !spec.foreground {
                            // Replacing the visible surface also restores its
                            // previous focus. A background job must not do either.
                            if let Err(error) = self.terminal_jobs.present(
                                &receipt.id,
                                target.cloned(),
                                Presentation::Hidden,
                            ) {
                                self.warn(error);
                            }
                        } else if let Err(error) = self.open_terminal_presentation(&receipt.id, cx)
                        {
                            self.warn(error);
                        }
                        self.terminal_geometry
                            .insert(receipt.id, spec.geometry.clone());
                        if spec.foreground {
                            self.terminal_focus = Some(receipt.id);
                        }
                    }
                    return Ok(TerminalReply::Job {
                        receipt: Box::new(
                            self.terminal_jobs.receipt(&receipt.id).expect("started"),
                        ),
                    });
                }
                _ => {}
            }
            let id = match request {
                TerminalCommand::Status { id }
                | TerminalCommand::Show { id }
                | TerminalCommand::Hide { id }
                | TerminalCommand::Close { id }
                | TerminalCommand::Cancel { id } => id,
                _ => unreachable!(),
            };
            let receipt = self
                .terminal_jobs
                .receipt(id)
                .filter(|r| r.owner == owner && r.project == project)
                .ok_or("No terminal job owned by this caller has that ID")?;
            match request {
                TerminalCommand::Show { .. } => {
                    // Native reattachment never authorizes an agent to manipulate
                    // a pane which belongs to another caller.
                    crate::terminal_job::authorize(
                        &self.desk,
                        Some(&active_caller),
                        receipt.target.as_ref(),
                        pane,
                    )?;
                    self.open_terminal_presentation(id, cx)?;
                }
                TerminalCommand::Hide { .. } | TerminalCommand::Close { .. } => {
                    let overlay_id = format!("terminal-{id}");
                    let closed = matches!(request, TerminalCommand::Close { .. });
                    if self.overlays.get(&overlay_id).is_some() {
                        if closed {
                            self.overlays.close(&overlay_id, &owner.to_string())?;
                        } else {
                            self.overlays.hide(&overlay_id, &owner.to_string())?;
                        }
                    }
                    self.terminal_jobs.present(
                        id,
                        receipt.target,
                        if closed {
                            Presentation::Closed
                        } else {
                            Presentation::Hidden
                        },
                    )?;
                }
                TerminalCommand::Cancel { .. } => {
                    self.terminal_jobs.cancel(id, "Caller cancellation", cx)?
                }
                _ => {}
            }
            Ok(TerminalReply::Job {
                receipt: Box::new(self.terminal_jobs.receipt(id).expect("owned")),
            })
        })();
        cx.notify();
        match result {
            Ok(reply) => Answer::Terminal { reply },
            Err(because) => Answer::Refused { because },
        }
    }

    pub(crate) fn terminal_launch_context(
        &self,
        conversation: guild_session::SessionId,
        project: guild_session::ProjectId,
        cwd: &std::path::Path,
    ) -> guild_session::terminal_job::LaunchContext {
        let branch = self
            .update_sources
            .get(&project)
            .and_then(|answer| answer.as_ref().ok())
            .and_then(|snapshot| snapshot.head.as_ref())
            .map(|(reference, _)| reference.strip_prefix("refs/heads/").unwrap_or(reference));
        crate::terminal_job::launch_context(&self.desk, conversation, project, cwd, branch)
    }

    pub(crate) fn terminal_target_exists(&self, target: &Target) -> bool {
        self.tabs.iter().any(|tab| match target {
            Target::Session(id) => tab.sessions().any(|session| session == *id),
            Target::Pane(id) => tab.panes.ids().contains(id),
        })
    }

    pub(crate) fn open_terminal_presentation(
        &mut self,
        id: &JobId,
        cx: &mut Context<Self>,
    ) -> Result<(), JobError> {
        let receipt = self
            .terminal_jobs
            .receipt(id)
            .ok_or("No terminal job has that ID")?;
        if self.terminal_jobs.terminal(id).is_none() {
            return Err(
                "Terminal output was released or belongs to an earlier Guild lifetime".into(),
            );
        }
        let target = receipt
            .target
            .ok_or("Terminal target was closed; reattach using Terminal Jobs")?;
        if !self.terminal_target_exists(&target) {
            return Err("Terminal target no longer exists; reattach using Terminal Jobs".into());
        }
        let overlay_id = format!("terminal-{id}");
        let owner = receipt.owner.to_string();
        self.overlays.open(
            &overlay_id,
            &owner,
            target.clone(),
            &receipt.title,
            Content::Terminal { job: *id },
        )?;
        self.overlays.show(&overlay_id, &owner)?;
        self.ensure_overlay_focus(&overlay_id, cx);
        self.terminal_jobs
            .present(id, Some(target), Presentation::Shown)?;
        Ok(())
    }
}

/// Native list focus is separate from terminal input and automated ownership.
pub(crate) struct List {
    pub(crate) focus: gpui::FocusHandle,
    pub(crate) tab_start: gpui::FocusHandle,
    pub(crate) tab_end: gpui::FocusHandle,
    pub(crate) previous: Option<gpui::FocusHandle>,
    pub(crate) context: HistoryContext,
    pub(crate) scope: Scope,
    pub(crate) scroll: gpui::ScrollHandle,
    pub(crate) labels: guild_session::terminal_job::LaunchContext,
    pub(crate) hovered: Option<JobId>,
    pub(crate) fading: Option<JobId>,
    pub(crate) review_details: std::collections::HashSet<JobId>,
    pub(crate) actions:
        std::cell::RefCell<std::collections::HashMap<(JobId, &'static str), Action>>,
}

#[derive(Clone)]
pub(crate) struct Action {
    pub(crate) focus: gpui::FocusHandle,
    pub(crate) bounds: std::rc::Rc<std::cell::Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
}
impl Root {
    pub(crate) fn focus_terminal_job(
        &mut self,
        id: &JobId,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        use gpui::Focusable;
        let Some(terminal) = self.terminal_jobs.terminal(id) else {
            return;
        };
        let Some(receipt) = self.terminal_jobs.receipt(id) else {
            return;
        };
        let visible = self.fronts_on_screen().iter().any(|tab| {
            self.tabs
                .iter()
                .find(|t| t.id == *tab)
                .is_some_and(|tab| match receipt.target {
                    Some(Target::Session(session)) => tab.sessions().any(|id| id == session),
                    Some(Target::Pane(pane)) => tab.panes.ids().contains(&pane),
                    None => false,
                })
        });
        if !visible {
            return;
        }
        let overlay_id = format!("terminal-{id}");
        if let Some(focus) = self.overlay_focus.borrow_mut().get_mut(&overlay_id) {
            if !focus.handle.contains_focused(window, cx) {
                focus.previous = window.focused(cx);
            }
            terminal.focus_handle(cx).focus(window, cx);
            self.focusing_front = false;
        }
    }

    pub(crate) fn settle_terminal_jobs(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        for receipt in self.terminal_jobs.receipt_snapshot().iter() {
            let id = format!("terminal-{}", receipt.id);
            let finished_view = receipt.is_archive_view()
                && receipt.presentation == Presentation::Closed
                && receipt.state == guild_session::terminal_job::JobState::Exited
                && self.overlays.get(&id).is_some();
            if finished_view
                || receipt
                    .target
                    .as_ref()
                    .is_some_and(|target| !self.terminal_target_exists(target))
            {
                let _ = self.overlays.close(&id, &receipt.owner.to_string());
                let _ = self
                    .terminal_jobs
                    .present(&receipt.id, None, Presentation::Closed);
            } else if let Some(entry) = self.overlays.get(&id) {
                let _ = self.terminal_jobs.present(
                    &receipt.id,
                    Some(entry.target.clone()),
                    if entry.visible {
                        Presentation::Shown
                    } else {
                        Presentation::Hidden
                    },
                );
            }
        }
        if let Some(id) = self.terminal_focus.take() {
            self.focus_terminal_job(&id, window, cx);
        }
    }

    pub(crate) fn reattach_terminal_job(
        &mut self,
        id: &JobId,
        target: Target,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> Result<(), JobError> {
        if !self.terminal_target_exists(&target) {
            return Err("Select an existing pane first".into());
        }
        let receipt = self
            .terminal_jobs
            .receipt(id)
            .ok_or("No terminal job has that ID")?;
        if self.terminal_jobs.terminal(id).is_none() {
            return Err("Terminal output is no longer retained".into());
        }
        let overlay_id = format!("terminal-{id}");
        let _ = self.overlays.close(&overlay_id, &receipt.owner.to_string());
        self.terminal_jobs
            .present(id, Some(target), Presentation::Closed)?;
        self.open_terminal_presentation(id, cx)?;
        self.focus_terminal_job(id, window, cx);
        Ok(())
    }

    pub(crate) fn toggle_terminal_list(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.terminal_list.is_some() {
            self.close_terminal_list(super::picker::Focus::Back, window, cx);
        } else {
            self.put_fields_away();
            let focus = cx.focus_handle();
            let previous = window.focused(cx);
            focus.focus(window, cx);
            let conversation = self
                .active_tab
                .and_then(|id| self.tabs.iter().find(|t| t.id == id))
                // Body already carries the current conversation after rebind.
                .and_then(crate::Tab::session);
            let context = HistoryContext {
                conversation,
                project: self.active_project,
                ..Default::default()
            };
            let labels = crate::terminal_job::launch_context(
                &self.desk,
                conversation.unwrap_or_default(),
                self.active_project.unwrap_or_default(),
                std::path::Path::new("/"),
                None,
            );
            self.terminal_list = Some(List {
                focus,
                tab_start: cx.focus_handle().tab_stop(true),
                tab_end: cx.focus_handle().tab_stop(true),
                previous,
                scope: Scope::initial(&context),
                context,
                scroll: gpui::ScrollHandle::new(),
                labels,
                hovered: None,
                fading: None,
                actions: Default::default(),
                review_details: Default::default(),
            });
            self.focusing_front = false;
        }
        cx.notify();
    }

    /// Keep traversal between the popup's boundary stops, in either direction.
    pub(crate) fn terminal_list_tab(
        &self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let stroke = &event.keystroke;
        if stroke.key != "tab"
            || stroke.modifiers.control
            || stroke.modifiers.alt
            || stroke.modifiers.platform
        {
            return;
        }
        let Some(list) = self.terminal_list.as_ref() else {
            return;
        };
        let backwards = stroke.modifiers.shift;
        if backwards {
            if list.focus.is_focused(window) {
                list.tab_end.focus(window, cx);
            }
            window.focus_prev(cx);
        } else {
            window.focus_next(cx);
        }
        if list.tab_start.is_focused(window) {
            if backwards {
                list.tab_end.focus(window, cx);
                window.focus_prev(cx);
            } else {
                window.focus_next(cx);
            }
        } else if list.tab_end.is_focused(window) {
            if backwards {
                window.focus_prev(cx);
            } else {
                list.tab_start.focus(window, cx);
                window.focus_next(cx);
            }
        }
        let actions = list.actions.borrow();
        for action in actions.values() {
            if action.focus.contains_focused(window, cx) {
                if let Some(bounds) = action.bounds.get() {
                    let viewport = list.scroll.bounds();
                    let mut offset = list.scroll.offset();
                    // Rows may exceed the viewport; reveal only the focused action.
                    if bounds.top() < viewport.top() {
                        offset.y += viewport.top() - bounds.top();
                    } else if bounds.bottom() > viewport.bottom() {
                        offset.y += viewport.bottom() - bounds.bottom();
                    }
                    list.scroll.set_offset(offset);
                    cx.notify();
                }
                return;
            }
        }
        // The scope control is outside the row scroller; restore its starting position.
        list.scroll.set_offset(gpui::Point::default());
        cx.notify();
    }

    /// Preserve opening IDs while enriching membership from exact extant roots.
    pub(crate) fn terminal_history_context(&self) -> HistoryContext {
        let mut context = self
            .terminal_list
            .as_ref()
            .map(|list| list.context.clone())
            .unwrap_or_default();
        context.repositories = self
            .desk
            .projects()
            .filter_map(|p| p.repository.clone().map(|repo| (p.id, repo)))
            .collect();
        // poll_git observes exactly Desk's current projects. The column retains
        // other cached lists, but their absence is no longer current evidence.
        context.worktrees = self
            .worktrees
            .iter()
            .filter(|(repository, _)| {
                context
                    .repositories
                    .values()
                    .any(|observed| observed == *repository)
            })
            .map(|(repository, list)| {
                (
                    repository.clone(),
                    list.iter()
                        .filter(|tree| tree.shown())
                        .map(|tree| tree.path.clone())
                        .collect(),
                )
            })
            .collect();
        context
    }

    pub(crate) fn select_terminal_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        if let Some(list) = self.terminal_list.as_mut()
            && scope.available(&list.context)
        {
            list.scope = scope;
            list.hovered = None;
            list.fading = None;
            list.scroll = gpui::ScrollHandle::new();
            cx.notify();
        }
    }

    pub(crate) fn close_terminal_list(
        &mut self,
        reason: super::picker::Focus,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            self.context_menu.as_ref().map(|menu| &menu.target),
            Some(crate::view::menu::Target::TerminalJobScopes { .. })
        ) {
            self.context_menu = None;
        }
        if let Some(list) = self.terminal_list.take()
            && reason == super::picker::Focus::Back
        {
            match list.previous {
                Some(previous) => previous.focus(window, cx),
                None => self.refocus(window, cx),
            }
        }
        cx.notify();
    }
}

#[cfg(test)]
impl Root {
    /// Construct the shell used by popup interaction regressions outside render sources.
    pub(crate) fn history_session_fixture(
        &mut self,
        dir: &std::path::Path,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> (
        guild_session::SessionId,
        gpui::Entity<crate::terminal::view::TerminalTab>,
    ) {
        use gpui::Focusable;
        let project = self.active_project.unwrap();
        let seat = self.active_seat().unwrap();
        let session = self.desk.add_session(guild_session::SessionRecord::new(
            "History session".into(),
            seat,
            dir.into(),
            guild_session::Runs::Shell,
        ));
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.args(["-c", "stty -echo -icanon; cat > input", "fixture"]);
        let view = crate::terminal::view::open_command(
            command,
            dir,
            Look::of(&self.settings, self.theme),
            None,
            cx,
        )
        .unwrap();
        let facts = view.read(cx).facts().clone();
        let tab = self.adopt(
            seat,
            project,
            crate::Body::Terminal {
                session,
                view: view.clone(),
                facts,
            },
            cx,
        );
        self.show_tab(tab, cx);
        view.read(cx).focus_handle(cx).focus(window, cx);
        (session, view)
    }
}
