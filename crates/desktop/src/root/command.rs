//! The window's end of the `guild` command — M13.
//!
//! **Every command is a thing the window already does**, and this module
//! is where a [`Command`] becomes that thing — decision 4 of M13's plan.
//! Mutations call the same guarded actions as the UI. Read-only lookups
//! also resolve paths, inspect git state and check process ancestry for caller
//! provenance. Those reads do not authorize a second mutation path.
//!
//! It runs on the watch loop's clock, where the window's own state is in
//! hand, and answers down the channel the asking connection is holding
//! open.

use gpui::Context;
use guild_session::command::{Answer, Command, Project, WorkspaceRow, Worktree};
use guild_session::{ProjectId, WorkspaceId};

use crate::hooks::server::Asked;
use crate::{Root, column};

impl Root {
    /// Take one command: answer it now, or keep its way back until the
    /// thing it asked for has happened.
    ///
    /// **Most answer at once**, because what they ask for is already in
    /// the window's hands. A worktree does not exist until git has made it,
    /// so that command keeps the connection's sender beside the write it
    /// is waiting on, and `answer_pending_write` finishes it when git's
    /// line arrives.
    pub(crate) fn take_command(&mut self, asked: Asked, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.take_command_without_history(asked, cx);
        self.history.suspended -= 1;
    }

    fn take_command_without_history(&mut self, asked: Asked, cx: &mut Context<Root>) {
        let Asked { command, answer } = asked;
        let now = match &command {
            Command::Workflow { request, caller } => {
                self.command_workflow(request, caller.as_deref(), answer, cx);
                return;
            }
            Command::Terminal { request, caller } => {
                self.command_terminal(request, caller.as_deref(), cx)
            }
            Command::Overlay {
                operation,
                id,
                title,
                text,
                caller,
            } => match resolved_caller(caller.as_deref(), &self.started_as) {
                Ok(caller) => {
                    self.command_overlay(operation, id, title, text, caller.as_deref(), cx)
                }
                Err(because) => Answer::Refused { because },
            },
            Command::Status => Answer::Status {
                projects: self.command_projects(),
            },
            Command::WorktreeList => Answer::Worktrees {
                worktrees: self.command_worktrees(),
            },
            Command::WorktreeOpen {
                path,
                workspace,
                caller,
            } => self.command_open(path, workspace.as_deref(), caller.as_deref(), cx),
            Command::WorktreeCreate {
                from,
                name,
                base,
                prompt,
                caller,
                front,
                workspace,
                harness,
            } => {
                let Some(project) = self.root_containing(from) else {
                    let _ = answer.send(Answer::Refused {
                        because: format!(
                            "no root of this window contains {} — open it first with `guild worktree open`",
                            from.display()
                        ),
                    });
                    return;
                };
                let task = prompt.clone().map(|prompt| Task {
                    prompt,
                    caller: caller.clone(),
                    front: *front,
                    workspace: workspace.clone(),
                    harness: harness.unwrap_or(guild_session::HarnessKind::Claude),
                });
                if let Some(task) = &task
                    && let Err(because) = self.refuses_task(project, task)
                {
                    let _ = answer.send(Answer::Refused { because });
                    return;
                }
                self.command_create(project, name.clone(), base.clone(), task, answer, cx);
                return;
            }
            Command::WorktreeRemove {
                path,
                delete_ignored,
                caller,
            } => {
                self.command_remove(path.clone(), *delete_ignored, caller.clone(), answer, cx);
                return;
            }
            Command::SessionNew {
                worktree,
                prompt,
                caller,
                front,
                workspace,
                harness,
            } => self.command_session_new(
                worktree,
                prompt.clone(),
                caller.as_deref(),
                *front,
                workspace.as_deref(),
                harness.unwrap_or(guild_session::HarnessKind::Claude),
                None,
                cx,
            ),
            Command::SessionNewInGroup {
                group,
                prompt,
                caller,
                front,
                workspace,
                harness,
            } => self.command_session_new_in_group(
                group,
                prompt.clone(),
                caller.as_deref(),
                *front,
                workspace.as_deref(),
                harness.unwrap_or(guild_session::HarnessKind::Claude),
                cx,
            ),
            Command::GroupCheck {
                group,
                workspace,
                caller,
            } => {
                self.command_group_check(group.as_deref(), workspace.as_deref(), caller.as_deref())
            }
            Command::WorkspaceList => Answer::Workspaces {
                workspaces: self.command_workspaces(),
            },
            Command::WorkspaceCreate { name, icon, select } => {
                match self.desk.add_workspace(name, icon.as_deref()) {
                    Ok(id) => {
                        if *select {
                            self.select_workspace(id, cx);
                        }
                        self.save();
                        cx.notify();
                        Answer::WorkspaceCreated {
                            id: id.to_string(),
                            name: name.trim().to_owned(),
                        }
                    }
                    Err(err) => Answer::Refused {
                        because: self.workspace_refusal(err),
                    },
                }
            }
            Command::WorkspaceSelect { workspace } => match self.parse_workspace(workspace) {
                Ok(id) => {
                    self.select_workspace(id, cx);
                    Answer::WorkspaceSelected { id: id.to_string() }
                }
                Err(because) => Answer::Refused { because },
            },
            Command::AgentList => Answer::Agents {
                agents: self.command_agents(cx),
            },
            Command::AgentSend {
                session,
                text,
                caller,
            } => {
                self.command_send(session, text, caller.as_deref(), answer, cx);
                return;
            }
            Command::SessionResumeTask {
                session,
                request_id,
                prompt,
                caller,
                front,
            } => {
                self.command_resume_task(
                    session,
                    request_id,
                    prompt,
                    caller.as_deref(),
                    *front,
                    answer,
                    cx,
                );
                return;
            }
            Command::SessionContinuationStatus {
                session,
                request_id,
                caller,
            } => {
                if let Some(key) = request_id {
                    self.command_continuation_status(session, key, caller.as_deref())
                } else {
                    self.command_inspect_continuation(session, caller.as_deref(), answer, cx);
                    return;
                }
            }
            Command::AgentStop { session } => self.command_stop(session, cx),
            Command::SessionResume {
                session,
                caller,
                front,
            } => {
                self.command_resume(session, caller.as_deref(), *front, answer, cx);
                return;
            }
        };
        let _ = answer.send(now);
    }

    /// Make a worktree for the command line, from the repository's main
    /// branch unless `base` names another — M13.1, decision 2.
    ///
    /// **The main branch is asked of git off the window's thread**, as
    /// `command_remove` asks what git ignores: two reads, and then the
    /// write goes through `write_worktree`, the branch popup's own path.
    /// A repository with no main branch to name starts from `HEAD`, as it
    /// always did.
    fn command_create(
        &mut self,
        project: guild_session::ProjectId,
        name: String,
        base: Option<String>,
        task: Option<Task>,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Root>,
    ) {
        // Refuse before even reading the default base from Git. The native
        // writer repeats this shared guard for the popup and explicit bases.
        if let Err(because) = crate::column::worktree_name(&name) {
            let _ = answer.send(Answer::Refused { because });
            return;
        }
        if let Err(because) = self.require_repository_identity(project) {
            let _ = answer.send(Answer::Refused { because });
            return;
        }
        let base = base.or_else(|| {
            self.repository_identity(project)
                .and_then(|r| self.settings.repository_settings.get(&r.key))
                .and_then(|p| p.worktree_base.clone())
        });
        if base.is_some() {
            self.send_create(project, &name, base, task, answer);
            return;
        }
        let Some(root) = self
            .desk
            .project(project)
            .map(|project| project.path.clone())
        else {
            let _ = answer.send(Answer::Refused {
                because: "that root is no longer open".to_owned(),
            });
            return;
        };
        cx.spawn(async move |this, cx| {
            let base = cx
                .background_executor()
                .spawn(async move { crate::git::main_branch(&root) })
                .await;
            let _ = this.update(cx, |this, _| {
                this.send_create(project, &name, base, task, answer);
            });
        })
        .detach();
    }

    /// The write itself, held beside the answer until git's line arrives,
    /// and the task beside the folder it will run in.
    fn send_create(
        &mut self,
        project: guild_session::ProjectId,
        name: &str,
        base: Option<String>,
        task: Option<Task>,
        answer: std::sync::mpsc::Sender<Answer>,
    ) {
        match self.write_worktree(project, name, base) {
            Ok(write) => {
                if let (crate::git::Write::WorktreeAdd { path, .. }, Some(task)) = (&write, task) {
                    self.pending_tasks.push((path.clone(), task));
                }
                self.pending_writes.push((write, answer));
            }
            Err(because) => {
                let _ = answer.send(Answer::Refused { because });
            }
        }
    }

    /// Why a task for a worktree of `project`'s repository would not start,
    /// asked **before the worktree is made** — M13.1, decision 3. The same
    /// three questions `command_session_new` asks after, so a worktree is
    /// never made for a session that will be refused: a task may not start
    /// a task, OpenCode must be v2, and the workspace must be decidable.
    fn refuses_task(&self, project: guild_session::ProjectId, task: &Task) -> Result<(), String> {
        if guild_session::command::refuses_session(true, self.is_task(task.caller.as_deref())) {
            return Err(TASK_FROM_A_TASK.to_owned());
        }
        if task.harness == guild_session::HarnessKind::OpenCode && Self::opencode_floor_missed() {
            return Err(OPENCODE_FLOOR.to_owned());
        }
        let Some(root) = self.desk.project(project) else {
            return Err("that root is no longer open".to_owned());
        };
        let holders = match &root.repository {
            Some(common) => self.repository_holders(common),
            None => self.desk.holders(project),
        };
        self.command_workspace_among(
            &holders,
            &root.path,
            task.workspace.as_deref(),
            task.caller.as_deref(),
        )
        .map(|_| ())
    }

    /// Validate the same prospective task before a workflow creates its checkout.
    pub(super) fn preflight_workflow_assignment(
        &self,
        origin: &guild_session::workflow::OriginBinding,
        harness: guild_session::HarnessKind,
        selection: &guild_session::harness::LaunchSelection,
    ) -> Result<(), String> {
        validate_launch_selection(harness, Some(selection))?;
        self.refuses_task(
            origin.project,
            &Task {
                prompt: String::new(),
                caller: Some(origin.session.to_string()),
                front: false,
                workspace: Some(origin.workspace.to_string()),
                harness,
            },
        )
    }

    /// Every session the window holds, with what M5.10's board says its
    /// agent is doing — and, since M5.12, whether a terminal's process has
    /// ended under an open tab, which `guild agent wait` reads as *exited*.
    fn command_agents(&self, cx: &gpui::App) -> Vec<guild_session::command::Agent> {
        self.desk
            .sessions()
            .map(|record| guild_session::command::Agent {
                session: record.id.to_string(),
                worktree: self
                    .desk
                    .project(record.project)
                    .map(|project| project.path.clone())
                    .unwrap_or_else(|| record.cwd.clone()),
                harness: record.runs.harness(),
                conversation: match record.runs.harness() {
                    Some(guild_session::HarnessKind::Claude) => Some(record.id.to_string()),
                    _ => record.harness_session.clone(),
                },
                status: self.board.observed_status(record.id).to_owned(),
                kind: guild_session::command::kind_word(self.board.status(record.id))
                    .map(str::to_owned),
                turned: record.had_turn,
                task: record.task,
                open: record.open,
                workspace: record.workspace.to_string(),
                exited: self
                    .tabs
                    .iter()
                    .flat_map(crate::Tab::bodies)
                    .any(|body| match body {
                        crate::Body::Terminal { session, view, .. } => {
                            *session == record.id && view.read(cx).exited()
                        }
                        _ => false,
                    }),
                tab: self
                    .tabs
                    .iter()
                    .any(|open| open.sessions().any(|held| held == record.id)),
            })
            .collect()
    }

    /// Type a message into a task's prompt and send it — M13 task 9.
    ///
    /// **Who and when are decided first**, by `command::refuses_send`: only
    /// to a task, not from one, and only while it reads a prompt. Then the
    /// tab's own paste path types it, and the answer waits for the
    /// session's `UserPromptSubmit` — see `prompt_began` — because a
    /// caller that waits straight after must not be told the turn before
    /// was this one.
    fn command_send(
        &mut self,
        session: &str,
        text: &str,
        caller: Option<&str>,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Root>,
    ) {
        let refuse = |because: String| {
            let _ = answer.send(Answer::Refused { because });
        };
        let Ok(id) = session.parse::<guild_session::SessionId>() else {
            refuse(format!("{session} is not a session id"));
            return;
        };
        let Some(record) = self.desk.session(id) else {
            refuse(format!("this window has no session {session}"));
            return;
        };
        // Ownership comes before presence and telemetry; a task cannot steer
        // another, and a person's own tab is never a CLI typing target.
        if let Some(because) =
            guild_session::command::refuses_send(record.task, self.is_task(caller), "done", true)
        {
            refuse(format!("{session}: {because}"));
            return;
        }
        self.submit_agent_prompt(id, text, super::agent_submit::Origin::Command, answer, cx);
    }

    /// Bring a closed session back for the command line — M13.3.
    ///
    /// **What the window knows is asked first**, then `ps` off the window's
    /// thread, then everything again with its answer in hand — a person
    /// may have reopened it meanwhile — and only then `reopen_into`, the
    /// Sessions panel's own door.
    fn command_resume(
        &mut self,
        session: &str,
        caller: Option<&str>,
        front: bool,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Root>,
    ) {
        let (id, conversation) = match self.resumable(session, caller) {
            Ok(found) => found,
            Err(because) => {
                let _ = answer.send(Answer::Refused { because });
                return;
            }
        };
        let (session, caller) = (session.to_owned(), caller.map(str::to_owned));
        cx.spawn(async move |root, cx| {
            let holders = cx
                .background_executor()
                .spawn(async move { crate::terminal::ownership::await_holders(&conversation) })
                .await;
            let _ = root.update(cx, |root, cx| {
                let reply = match holders {
                    Ok(holders) => {
                        root.resume_now(id, &session, caller.as_deref(), front, &holders, cx)
                    }
                    Err(because) => Answer::Refused { because },
                };
                let _ = answer.send(reply);
            });
        })
        .detach();
    }

    /// The window's own refusals, in the spec's order: the record, the
    /// rule, a tab holding it, the harness on the `PATH`. Answers the id
    /// and the conversation `ps` is to be searched for.
    pub(super) fn resumable(
        &self,
        session: &str,
        caller: Option<&str>,
    ) -> Result<(guild_session::SessionId, String), String> {
        let Ok(id) = session.parse::<guild_session::SessionId>() else {
            return Err(format!("{session} is not a session id"));
        };
        let Some(record) = self.desk.session(id) else {
            return Err(format!("this window has no session {session}"));
        };
        // Bringing a task back is starting it: decision 35's rule, as
        // `session new` asks it. A person's session is a tab like any other.
        if guild_session::command::refuses_session(record.task, self.is_task(caller)) {
            return Err(TASK_FROM_A_TASK.to_owned());
        }
        if let Some(tab) = self
            .tabs
            .iter()
            .find(|tab| tab.sessions().any(|shown| shown == id))
        {
            let worktree = self
                .desk
                .project(tab.project)
                .map(|project| project.path.clone())
                .unwrap_or_else(|| record.cwd.clone());
            return Err(format!(
                "session {session} is already open, in the tab \"{}\" in {}",
                self.session_tab_name(tab).unwrap_or_default(),
                worktree.display()
            ));
        }
        if let Some(kind) = Self::harness_missing(record.runs) {
            return Err(format!("{} is not on the PATH", kind.program()));
        }
        if record.runs.harness() == Some(guild_session::HarnessKind::OpenCode)
            && Self::opencode_floor_missed()
        {
            return Err(OPENCODE_FLOOR.to_owned());
        }
        let conversation = match record.runs.harness() {
            Some(guild_session::HarnessKind::Claude) | None => id.to_string(),
            Some(_) => record
                .harness_session
                .clone()
                .unwrap_or_else(|| id.to_string()),
        };
        Ok((id, conversation))
    }

    /// After `ps`: ask again, refuse a holder outside the window, then
    /// reopen.
    #[allow(clippy::too_many_arguments, reason = "one call site, one fact each")]
    fn resume_now(
        &mut self,
        id: guild_session::SessionId,
        session: &str,
        caller: Option<&str>,
        front: bool,
        holders: &[(u32, u32, String)],
        cx: &mut Context<Root>,
    ) -> Answer {
        if let Err(because) = self.resumable(session, caller) {
            return Answer::Refused { because };
        }
        if let Some((pid, _, args)) = holders.first() {
            return Answer::Refused {
                because: format!(
                    "session {session} is held by another process — pid {pid}: {args}; end it first"
                ),
            };
        }
        // Its last tab closed a moment ago and its process is still going:
        // a second one on the same transcript is what this refuses.
        if crate::terminal::retiring::alive(&id.to_string()) {
            return Answer::Refused {
                because: format!(
                    "session {session} is still closing its last process; try again in a moment"
                ),
            };
        }
        match self.reopen_into(id, None, front, cx) {
            Some(tab) => Answer::Resumed {
                session: session.to_owned(),
                worktree: self
                    .tabs
                    .iter()
                    .find(|open| open.id == tab)
                    .and_then(|open| self.desk.project(open.project))
                    .map(|project| project.path.clone())
                    .unwrap_or_default(),
            },
            None => Answer::Refused {
                because: format!(
                    "session {session} has nothing to resume — it never had a turn, and the window forgot it"
                ),
            },
        }
    }

    /// A session's `UserPromptSubmit` has arrived: answer the send that was
    /// waiting on it, if one was.
    pub(crate) fn prompt_began(&mut self, session: guild_session::SessionId) {
        self.workflow_prompt_submitted(session);
        let (began, still): (Vec<_>, Vec<_>) = std::mem::take(&mut self.pending_settle)
            .into_iter()
            .partition(|(settles, _, _)| {
                matches!(settles, Settles::Began { session: waiting, .. } if *waiting == session)
            });
        self.pending_settle = still;
        for (_, answer, reply) in began {
            let _ = answer.send(reply);
        }
    }

    /// End a session the command line started — its tab closes and its
    /// process goes with it.
    ///
    /// **A person's session is refused.** The command line ends only what
    /// it started: an agent able to close any tab could close the one the
    /// person is typing in, and `task` is the record of which is which — a
    /// task continued in a person's tab is still the task, decision 52.
    fn command_stop(&mut self, session: &str, cx: &mut Context<Root>) -> Answer {
        self.command_stop_observed(session, cx).answer
    }
    /// The same guarded stop, with its actual PTY-owner completion witness.
    pub(super) fn command_stop_observed(
        &mut self,
        session: &str,
        cx: &mut Context<Root>,
    ) -> TaskStop {
        let result = (|| {
            let id = session
                .parse::<guild_session::SessionId>()
                .map_err(|_| format!("{session} is not a session id"))?;
            let record = self
                .desk
                .session(id)
                .ok_or_else(|| format!("this window has no session {session}"))?;
            if !record.task {
                return Err("that session was opened by a person — the command line ends only what it started".to_owned());
            }
            let (tab, pane) = self
                .tabs
                .iter()
                .find_map(|tab| {
                    tab.panes
                        .find(|body| body.session() == Some(id))
                        .map(|pane| (tab.id, pane))
                })
                .ok_or_else(|| format!("session {session} is not open"))?;
            let view = self
                .tabs
                .iter()
                .flat_map(crate::Tab::bodies)
                .find_map(|body| match body {
                    crate::Body::Terminal { session, view, .. } if *session == id => {
                        Some(view.clone())
                    }
                    _ => None,
                });
            let (completion, already_stopped) = view
                .map(|view| {
                    view.update(cx, |view, _| {
                        let ended = view.exited();
                        (view.let_go(), ended)
                    })
                })
                .unwrap_or((None, false));
            // Close only this task's pane. Existing split/user bodies survive.
            self.close_pane(tab, pane, cx);
            Ok((completion, already_stopped))
        })();
        match result {
            Ok((completion, already_stopped)) => TaskStop {
                answer: Answer::Done,
                completion,
                already_stopped,
            },
            Err(because) => TaskStop {
                answer: Answer::Refused { because },
                completion: None,
                already_stopped: false,
            },
        }
    }

    /// Start a `claude` session in a worktree — the point of M13.
    ///
    /// **Who may start a task is decided first, before anything is made.**
    /// A session that was itself started with a task may not start another
    /// with a task — decision 5 of M13's plan:
    /// an agent that may start agents will start them for its own
    /// convenience, and parallelism, isolation and *it would be faster* are
    /// not the person asking. The window cannot see intent, so it checks
    /// provenance. A nested `claude -p` inherits `GUILD_SESSION` from the
    /// session that ran it, which is exactly right: a task's children are
    /// that task's.
    ///
    /// The `Runs` a `session new` starts a tab with, for a chosen harness.
    /// All harnesses run in terminals.
    fn runs_for(harness: guild_session::HarnessKind) -> guild_session::Runs {
        use guild_session::{HarnessKind, Runs, SessionMode};
        match harness {
            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
        }
    }

    /// The terminal session receives the initial prompt through CLI arguments.
    /// It opens behind what the person is doing unless `front` says otherwise.
    #[allow(
        clippy::too_many_arguments,
        reason = "one fact per argument, built at one call site"
    )]
    pub(super) fn command_session_new(
        &mut self,
        worktree: &std::path::Path,
        prompt: Option<String>,
        caller: Option<&str>,
        front: bool,
        workspace: Option<&str>,
        harness: guild_session::HarnessKind,
        selection: Option<guild_session::harness::LaunchSelection>,
        cx: &mut Context<Root>,
    ) -> Answer {
        if let Err(because) =
            self.command_session_refusal(prompt.is_some(), caller, harness, selection.as_ref())
        {
            return Answer::Refused { because };
        }
        if !worktree.is_dir() {
            return Answer::Refused {
                because: format!("there is no folder at {}", worktree.display()),
            };
        }

        // A root already, or made one — without taking the front, which is
        // `front`'s to decide.
        let target = match self.command_workspace_for(worktree, workspace, caller) {
            Ok(target) => target,
            Err(because) => return Answer::Refused { because },
        };
        let project = self.desk.open_in(target, worktree);
        self.know_repository(project);
        self.command_session_start(
            target, project, worktree, prompt, front, harness, selection, cx,
        )
    }

    /// What refuses a session before anything is opened: provenance — a
    /// task may not start a task — the launch selection, and the OpenCode
    /// floor. The same for `session new` and its `--group` door.
    fn command_session_refusal(
        &self,
        task: bool,
        caller: Option<&str>,
        harness: guild_session::HarnessKind,
        selection: Option<&guild_session::harness::LaunchSelection>,
    ) -> Result<(), String> {
        if guild_session::command::refuses_session(task, self.is_task(caller)) {
            return Err(TASK_FROM_A_TASK.to_owned());
        }
        validate_launch_selection(harness, selection)?;
        // The `guild session new --harness opencode` door is the agent's
        // and the shell's opencode — an opencode below V2 would launch and
        // never report status. The menu refuses at `open_harness_tab`; this
        // second route to the same gesture meets the same guard (decision
        // 30's shape: where a gesture has two routes, the refusal goes
        // where they meet). Agent-facing, so the words are English.
        if harness == guild_session::HarnessKind::OpenCode && Self::opencode_floor_missed() {
            return Err(OPENCODE_FLOOR.to_owned());
        }
        Ok(())
    }

    /// A session in a root the command has opened, behind the person's work
    /// unless `front`, answered as `session new` answers.
    #[allow(clippy::too_many_arguments)]
    fn command_session_start(
        &mut self,
        target: WorkspaceId,
        project: ProjectId,
        worktree: &std::path::Path,
        prompt: Option<String>,
        front: bool,
        harness: guild_session::HarnessKind,
        selection: Option<guild_session::harness::LaunchSelection>,
        cx: &mut Context<Root>,
    ) -> Answer {
        if front && target != self.active_workspace {
            self.select_workspace(target, cx);
        }
        if front && target == self.active_workspace && self.active_project != Some(project) {
            self.select_project(project, cx);
        }
        // GLD-058: the session goes into its own seat, named, without its
        // workspace being made active — the person's place, fields and the
        // workspace's remembered project stay as they are — and a
        // workspace behind is written down by `store_behind`: from memory
        // when restored, else by adding the tab to its project's file
        // alone. Making it active to record the tab saved every project
        // of a workspace not yet restored from an empty memory.
        let seat = guild_session::Seat {
            workspace: target,
            project,
        };
        let behind = target != self.active_workspace;
        let task = prompt.is_some();
        let opened =
            self.open_tab_in_seat(seat, Self::runs_for(harness), prompt, front, selection, cx);
        let Some(session) = opened else {
            return Answer::Refused {
                because: format!(
                    "the window could not open a session in {}",
                    worktree.display()
                ),
            };
        };
        if behind {
            self.store_behind(seat, session);
        }
        if task {
            self.desk.mark_task(session);
            self.save();
        }
        Answer::SessionStarted {
            session: session.to_string(),
            worktree: worktree.to_path_buf(),
        }
    }

    /// `guild session new --group NAME` — GLD-055: a session in the group's
    /// folder, opened as the group's root through the path the window's `+`
    /// on the folder's row takes. It never asks the bind question and never
    /// binds: a group with no folder is refused.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn command_session_new_in_group(
        &mut self,
        group: &str,
        prompt: Option<String>,
        caller: Option<&str>,
        front: bool,
        workspace: Option<&str>,
        harness: guild_session::HarnessKind,
        cx: &mut Context<Root>,
    ) -> Answer {
        if let Err(because) = self.command_session_refusal(prompt.is_some(), caller, harness, None)
        {
            return Answer::Refused { because };
        }
        let (target, found) = match self.command_groups_named(group, workspace, caller) {
            Ok(found) => found,
            Err(because) => return Answer::Refused { because },
        };
        let group = match found.as_slice() {
            [one] => one.clone(),
            _ => {
                return Answer::Refused {
                    because: format!(
                        "{} groups are named {} in this workspace; rename one so the name means one",
                        found.len(),
                        group.trim()
                    ),
                };
            }
        };
        let Some(folder) = group.folder.clone() else {
            return Answer::Refused {
                because: format!(
                    "the group {} has no folder — bind one from the group's … menu in the column",
                    group.name
                ),
            };
        };
        let project = match self.open_group_root(target, &folder) {
            Ok(project) => project,
            Err(because) => return Answer::Refused { because },
        };
        self.command_session_start(target, project, &folder, prompt, front, harness, None, cx)
    }

    /// `guild group check` — how the groups stand against their folders,
    /// the read the column's match line does.
    pub(super) fn command_group_check(
        &mut self,
        group: Option<&str>,
        workspace: Option<&str>,
        caller: Option<&str>,
    ) -> Answer {
        let (target, groups) = match group {
            Some(name) => match self.command_groups_named(name, workspace, caller) {
                Ok(found) => found,
                Err(because) => return Answer::Refused { because },
            },
            None => {
                let target = match self.command_workspace_among(
                    &[],
                    std::path::Path::new(""),
                    workspace,
                    caller,
                ) {
                    Ok(target) => target,
                    Err(because) => return Answer::Refused { because },
                };
                let groups = self
                    .desk
                    .workspace(target)
                    .map(|holder| holder.project_groups.clone())
                    .unwrap_or_default();
                (target, groups)
            }
        };
        self.refresh_group_reads(true);
        let groups = groups
            .iter()
            .map(|group| {
                let found = self.group_folder_match(target, group.id);
                guild_session::command::GroupCheck {
                    name: group.name.clone(),
                    workspace: target.to_string(),
                    folder: group.folder.clone(),
                    matches: found.as_ref().map(|found| found.matches()),
                    members: match found {
                        Some(found) => found
                            .members
                            .into_iter()
                            .map(|member| guild_session::command::GroupMember {
                                checkout: member.checkout,
                                inside: Some(member.inside),
                            })
                            .collect(),
                        None => group
                            .repositories
                            .iter()
                            .map(|key| guild_session::command::GroupMember {
                                checkout: guild_session::project_group::main_checkout(key),
                                inside: None,
                            })
                            .collect(),
                    },
                }
            })
            .collect();
        Answer::Groups { groups }
    }

    /// The groups named `name` — trimmed, exactly as shown — and their
    /// workspace, by `workspace_for`'s rule with "the workspaces holding a
    /// group of that name" as the holders: `--workspace`; the one holding
    /// it; among several, the caller's; otherwise refused naming them. A
    /// name no workspace holds is refused by name.
    fn command_groups_named(
        &self,
        name: &str,
        workspace: Option<&str>,
        caller: Option<&str>,
    ) -> Result<(WorkspaceId, Vec<guild_session::project_group::ProjectGroup>), String> {
        let name = name.trim();
        let named = |holder: &guild_session::workspace::Workspace| {
            holder
                .project_groups
                .iter()
                .filter(|group| group.name == name)
                .cloned()
                .collect::<Vec<_>>()
        };
        let holders: Vec<WorkspaceId> = self
            .desk
            .workspaces()
            .iter()
            .filter(|holder| !named(holder).is_empty())
            .map(|holder| holder.id)
            .collect();
        if holders.is_empty() && workspace.is_none() {
            return Err(format!("no workspace has a group named {name}"));
        }
        let target =
            self.command_workspace_among(&holders, std::path::Path::new(name), workspace, caller)?;
        let found = self.desk.workspace(target).map(named).unwrap_or_default();
        if found.is_empty() {
            return Err(format!("this workspace has no group named {name}"));
        }
        Ok((target, found))
    }

    /// Whether the session asking was itself started with a task — a caller
    /// the window does not know is not one.
    ///
    /// **Through what its tab holds now** — M13.3, decision 52: a task
    /// resumed by `/resume` in a person's tab runs in a process started as
    /// the person's session, and is still a task.
    fn is_task(&self, caller: Option<&str>) -> bool {
        caller
            .and_then(|id| id.parse::<guild_session::SessionId>().ok())
            .map(|id| crate::binding::resolve(id, &self.started_as))
            .and_then(|id| self.desk.session(id))
            .is_some_and(|record| record.task)
    }

    /// Remove a worktree for the command line — M13 task 8.
    ///
    /// **Asked twice, and that is on purpose.** What the window already
    /// knows is refused at once; then git is asked, on a thread, what it
    /// ignores in the folder — and by the time it answers an agent may have
    /// started there, so everything is asked again with the list in hand,
    /// in the same update that closes the root and sends the write.
    ///
    /// The rest is the column's own path: the root is closed before the
    /// folder goes, and the write runs on a sibling's worker, because this
    /// root's worker stands in the folder being deleted.
    fn command_remove(
        &mut self,
        path: std::path::PathBuf,
        delete_ignored: bool,
        caller: Option<String>,
        answer: std::sync::mpsc::Sender<Answer>,
        cx: &mut Context<Root>,
    ) {
        if let Err(because) = self.removable(&path, caller.as_deref(), None, delete_ignored, cx) {
            let _ = answer.send(Answer::Refused { because });
            return;
        }
        let folder = path.clone();
        cx.spawn(async move |root, cx| {
            let ignored = cx
                .background_executor()
                .spawn(async move { crate::git::worktrees::ignored(&folder) })
                .await;
            let _ = root.update(cx, |root, cx| {
                let ignored = match ignored {
                    Ok(ignored) => ignored,
                    Err(err) => {
                        let _ = answer.send(Answer::Refused {
                            because: format!(
                                "git could not say what it ignores in {}: {err}",
                                path.display()
                            ),
                        });
                        return;
                    }
                };
                match root.removable(&path, caller.as_deref(), Some(&ignored), delete_ignored, cx) {
                    Err(because) => {
                        let _ = answer.send(Answer::Refused { because });
                    }
                    Ok((project, runner)) => {
                        // The tabs, the watcher, the servers and the
                        // terminals go before the folder does.
                        if let Some(project) = project {
                            root.close_worktree_now(project, cx);
                        }
                        let write = crate::git::Write::WorktreeRemove(path);
                        root.run_git_write(runner, write.clone());
                        root.pending_writes.push((write, answer));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    /// Whether a command may remove this worktree: the root it is open as,
    /// if it is, and the root that will run the write — or why not.
    ///
    /// `ignored` is `None` before git has been asked, and then nothing is
    /// refused on its account.
    fn removable(
        &self,
        path: &std::path::Path,
        caller: Option<&str>,
        ignored: Option<&[String]>,
        delete_ignored: bool,
        cx: &Context<Root>,
    ) -> Result<(Option<guild_session::ProjectId>, guild_session::ProjectId), String> {
        let Some((common, _)) = self.worktrees.iter().find(|(_, list)| {
            list.iter()
                .any(|tree| tree.path.as_path() == path && tree.shown())
        }) else {
            return Err(format!(
                "git lists no worktree at {} — `guild worktree list` shows what there is",
                path.display()
            ));
        };
        let primary = column::main_root(common).as_deref() == Some(path);
        let project = self
            .desk
            .projects()
            .find(|project| project.path == path)
            .map(|project| project.id);
        let agent_busy = project.is_some_and(|project| self.worktree_busy(project));
        let persons_session = project.is_some_and(|project| {
            self.tabs
                .iter()
                .filter(|tab| tab.project == project)
                .flat_map(crate::Tab::sessions)
                .any(|id| self.desk.session(id).is_none_or(|record| !record.task))
        });
        let unsaved =
            project.is_some_and(|project| !self.dirty_file_tabs(Some(project), cx).is_empty());
        let lock = self.lock_on_worktree(path);
        let changed = project.is_some_and(|project| self.column_changes(project) > 0);
        if let Some(because) = column::refuse_removal(&column::Removing {
            path,
            caller_is_task: self.is_task(caller),
            primary,
            agent_busy,
            persons_session,
            unsaved,
            lock: lock.as_deref(),
            changed,
            ignored: ignored.unwrap_or(&[]),
            delete_ignored,
        }) {
            return Err(because);
        }
        let Some(runner) = self.removal_runner(path) else {
            return Err(format!(
                "no other root of this repository is open to remove {} from — open the \
                 repository first with `guild worktree open`",
                path.display()
            ));
        };
        Ok((project, runner))
    }

    /// Finish a command that was waiting on this write, if one was.
    ///
    /// A refusal carries git's own words — `fatal: a branch named 'x'
    /// already exists` is what a person or an agent can act on, and it is
    /// what the Console shows too.
    pub(crate) fn answer_pending_write(&mut self, line: &crate::console::Line) {
        let Some(at) = self
            .pending_writes
            .iter()
            .position(|(write, _)| write.args() == line.argv)
        else {
            return;
        };
        let (write, answer) = self.pending_writes.remove(at);
        if line.failed {
            if let crate::git::Write::WorktreeAdd { path, .. } = &write {
                self.pending_tasks.retain(|(folder, _)| folder != path);
            }
            let because = match write {
                crate::git::Write::WorktreeRemove(_) => column::refused_removal(&line.said),
                _ => line.said.trim().to_owned(),
            };
            let _ = answer.send(Answer::Refused { because });
            return;
        }
        match write {
            // Held until the list the other commands read has it — see
            // `pending_settle`.
            crate::git::Write::WorktreeAdd { path, branch, base } => {
                self.ask_worktrees();
                self.pending_settle.push((
                    Settles::Listed(path.clone()),
                    answer,
                    Answer::Created {
                        path,
                        branch,
                        base,
                        session: None,
                    },
                ));
            }
            // Held until the list no longer has it, for the same reason.
            crate::git::Write::WorktreeRemove(path) => {
                self.ask_worktrees();
                self.pending_settle.push((
                    Settles::Unlisted(path.clone()),
                    answer,
                    Answer::Removed { path },
                ));
            }
            _ => {
                let _ = answer.send(Answer::Done);
            }
        }
    }

    /// Send every held answer whose result the window can now show — and,
    /// for a worktree made with its task, start the task first and answer
    /// both, M13.1.
    pub(crate) fn answer_settled(&mut self, cx: &mut Context<Root>) {
        let mut still = Vec::new();
        for (settles, answer, reply) in std::mem::take(&mut self.pending_settle) {
            let shown = match &settles {
                Settles::Began { session, by } => {
                    if std::time::Instant::now() >= *by {
                        let _ = answer.send(Answer::Refused {
                            because: format!(
                                "the message was typed into {session}, but no turn began within \
                                 {} seconds — look at its tab",
                                BEGIN_PATIENCE.as_secs()
                            ),
                        });
                        continue;
                    }
                    false
                }
                Settles::Listed(path) => self
                    .worktrees
                    .values()
                    .flatten()
                    .any(|tree| &tree.path == path),
                Settles::Unlisted(path) => !self
                    .worktrees
                    .values()
                    .flatten()
                    .any(|tree| &tree.path == path),
            };
            if !shown {
                still.push((settles, answer, reply));
                continue;
            }
            let reply = match reply {
                Answer::Created {
                    path,
                    branch,
                    base,
                    session: None,
                } => match self
                    .pending_tasks
                    .iter()
                    .position(|(folder, _)| *folder == path)
                {
                    Some(at) => {
                        let (_, task) = self.pending_tasks.remove(at);
                        let started = self.command_session_new(
                            &path,
                            Some(task.prompt),
                            task.caller.as_deref(),
                            task.front,
                            task.workspace.as_deref(),
                            task.harness,
                            None,
                            cx,
                        );
                        guild_session::command::with_session(path, branch, base, started)
                    }
                    None => Answer::Created {
                        path,
                        branch,
                        base,
                        session: None,
                    },
                },
                other => other,
            };
            let _ = answer.send(reply);
        }
        // A task started above may have held an answer of its own.
        still.append(&mut self.pending_settle);
        self.pending_settle = still;
    }

    /// The root a folder is inside — the deepest one, because a worktree
    /// kept inside its repository's folder is inside two roots at once and
    /// the caller means the one it is standing in.
    fn root_containing(&self, folder: &std::path::Path) -> Option<guild_session::ProjectId> {
        self.desk
            .projects()
            .filter(|project| folder.starts_with(&project.path))
            .max_by_key(|project| project.path.components().count())
            .map(|project| project.id)
    }

    /// The roots the window has open, as the command line sees them.
    ///
    /// The repository is the common git directory, which is what makes
    /// two rows one project in the column — so a caller can tell a
    /// worktree from a separate checkout without asking git itself.
    fn command_projects(&self) -> Vec<Project> {
        self.desk
            .projects()
            .map(|project| Project {
                path: project.path.clone(),
                name: project.name.clone(),
                repository: project.repository.clone(),
                active: self.active_project == Some(project.id),
                sessions: self.desk.sessions_of(project.id).count(),
                workspaces: self
                    .desk
                    .holders(project.id)
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect(),
            })
            .collect()
    }

    /// Every worktree git has listed for a repository the window has a
    /// root in — the column's own source, not a second `git worktree
    /// list`: what the command line sees is what the column draws.
    ///
    /// The primary of each repository comes first and the repositories
    /// come in a fixed order, because a list an agent reads twice must not
    /// reorder itself between the readings — a `HashMap`'s order is not
    /// an order.
    fn command_worktrees(&self) -> Vec<Worktree> {
        let mut repositories: Vec<_> = self.worktrees.iter().collect();
        repositories.sort_by_key(|(common, _)| *common);

        let mut worktrees = Vec::new();
        for (common, list) in repositories {
            let main = column::main_root(common);
            let mut rows: Vec<Worktree> = list
                .iter()
                .filter(|tree| tree.shown())
                .map(|tree| Worktree {
                    path: tree.path.clone(),
                    repository: common.clone(),
                    branch: tree.branch.clone(),
                    primary: main.as_deref() == Some(tree.path.as_path()),
                    opened: self
                        .desk
                        .projects()
                        .any(|project| project.path == tree.path),
                    locked: tree.locked.clone(),
                })
                .collect();
            rows.sort_by_key(|row| !row.primary);
            worktrees.extend(rows);
        }
        worktrees
    }

    /// Make a checkout a root and put it in front — the column's unopened
    /// row, clicked.
    ///
    /// **An existing root is not an error.** Opening what is open brings it
    /// to the front and says so, which is what a person clicking a row that
    /// is already a root sees too; an agent that opens before it works
    /// should not have to ask first whether it needs to.
    fn command_open(
        &mut self,
        path: &std::path::Path,
        workspace: Option<&str>,
        caller: Option<&str>,
        cx: &mut Context<Root>,
    ) -> Answer {
        if !path.is_dir() {
            return Answer::Refused {
                because: format!("there is no folder at {}", path.display()),
            };
        }
        let target = match self.command_workspace_for(path, workspace, caller) {
            Ok(target) => target,
            Err(because) => return Answer::Refused { because },
        };
        let project = self.desk.open_in(target, path);
        self.know_repository(project);
        if target == self.active_workspace && self.active_project != Some(project) {
            self.select_project(project, cx);
        }
        // Its watcher and git thread start on the next tick of the watch
        // loop, as they do for a row clicked in the column.
        self.save();
        cx.notify();
        Answer::Opened {
            path: path.to_path_buf(),
        }
    }

    fn command_workspaces(&self) -> Vec<WorkspaceRow> {
        let selected = self.active_workspace;
        self.desk
            .workspaces()
            .iter()
            .map(|workspace| WorkspaceRow {
                id: workspace.id.to_string(),
                name: workspace.name.clone(),
                icon: workspace.icon.clone(),
                selected: workspace.id == selected,
                projects: self
                    .desk
                    .projects_of(workspace.id)
                    .map(|project| project.path.clone())
                    .collect(),
            })
            .collect()
    }

    fn parse_workspace(&self, value: &str) -> Result<WorkspaceId, String> {
        let id = value
            .parse::<WorkspaceId>()
            .map_err(|_| format!("{value} is not a workspace id"))?;
        self.desk
            .workspace(id)
            .map(|_| id)
            .ok_or_else(|| format!("this window has no workspace {value}"))
    }

    fn command_workspace_for(
        &self,
        path: &std::path::Path,
        asked: Option<&str>,
        caller: Option<&str>,
    ) -> Result<WorkspaceId, String> {
        let holders =
            if let Some(project) = self.desk.projects().find(|project| project.path == path) {
                self.desk.holders(project.id)
            } else if let Some((common, _)) = self
                .worktrees
                .iter()
                .find(|(_, rows)| rows.iter().any(|row| row.path == path))
            {
                self.repository_holders(common)
            } else {
                Vec::new()
            };
        self.command_workspace_among(&holders, path, asked, caller)
    }

    /// Every workspace holding a root of the repository whose common git
    /// directory is `common`, each once.
    fn repository_holders(&self, common: &std::path::Path) -> Vec<WorkspaceId> {
        let mut holders = Vec::new();
        for project in self
            .desk
            .projects()
            .filter(|project| project.repository.as_deref() == Some(common))
        {
            for holder in self.desk.holders(project.id) {
                if !holders.contains(&holder) {
                    holders.push(holder);
                }
            }
        }
        holders
    }

    /// [`crate::workspaces::workspace_for`] with the window's facts.
    fn command_workspace_among(
        &self,
        holders: &[WorkspaceId],
        path: &std::path::Path,
        asked: Option<&str>,
        caller: Option<&str>,
    ) -> Result<WorkspaceId, String> {
        let asked = asked.map(|value| self.parse_workspace(value)).transpose()?;
        let caller = caller
            .and_then(|value| value.parse().ok())
            .and_then(|id| self.desk.session(id))
            .map(|record| record.workspace);
        let names = self
            .desk
            .workspaces()
            .iter()
            .map(|workspace| (workspace.id, workspace.name.clone()))
            .collect::<Vec<_>>();
        match crate::workspaces::workspace_for(
            asked,
            caller,
            holders,
            self.active_workspace,
            &names,
            path,
        ) {
            crate::workspaces::Meant::Workspace(id) => Ok(id),
            crate::workspaces::Meant::Refused(because) => Err(because),
        }
    }
}

fn validate_launch_selection(
    harness: guild_session::HarnessKind,
    selection: Option<&guild_session::harness::LaunchSelection>,
) -> Result<(), String> {
    let Some(selection) = selection else {
        return Ok(());
    };
    selection.validate()?;
    if harness == guild_session::HarnessKind::OpenCode
        && (selection.model.is_some() || selection.effort.is_some())
    {
        let inherited = std::env::var_os("OPENCODE_CONFIG_CONTENT");
        let text = inherited
            .as_deref()
            .map(|v| {
                v.to_str().ok_or_else(|| {
                    "harness.opencode: inherited inline selection is not UTF-8".to_owned()
                })
            })
            .transpose()?;
        selection.opencode_inline(text)?;
    }
    Ok(())
}

/// A native task stop acknowledgement is separate from actual group completion.
pub(super) struct TaskStop {
    pub answer: Answer,
    pub completion: Option<std::sync::mpsc::Receiver<()>>,
    pub already_stopped: bool,
}

/// A session `guild worktree create` was asked to start in the worktree it
/// makes — M13.1. `command_session_new`'s arguments, kept until the
/// worktree is listed.
pub(crate) struct Task {
    /// What the session is asked first.
    prompt: String,
    /// The session asking, from its `GUILD_SESSION`.
    caller: Option<String>,
    /// Bring the session to the front.
    front: bool,
    /// The workspace asked for with `--workspace`.
    workspace: Option<String>,
    /// Which harness runs it.
    harness: guild_session::HarnessKind,
}

/// Decision 5 of M13, in the words the command line prints.
const TASK_FROM_A_TASK: &str = "a session started with a task may not start another with a task — \
                                do the work here, or ask the person to start it";

/// OpenCode below v2 launches and never reports status. Agent-facing, so
/// English.
const OPENCODE_FLOOR: &str = "OpenCode v2 is required — please update OpenCode";

/// What has to be true of the window before a held answer may go.
pub(crate) enum Settles {
    /// git's list of worktrees has this folder in it.
    Listed(std::path::PathBuf),
    /// git's list of worktrees no longer has this folder in it.
    Unlisted(std::path::PathBuf),
    /// A message typed into this session has begun a turn — its
    /// `UserPromptSubmit` has arrived — before `by`.
    Began {
        /// The session typed into.
        session: guild_session::SessionId,
        /// When to stop waiting and say so.
        by: std::time::Instant,
    },
}

impl Settles {
    pub(super) fn session(&self) -> Option<guild_session::SessionId> {
        match self {
            Self::Began { session, .. } => Some(*session),
            Self::Listed(_) | Self::Unlisted(_) => None,
        }
    }
}

/// How long a sent message may take to begin a turn. The hook is posted the
/// moment `claude` takes the prompt, so this is the slack for a loaded
/// machine, and well inside the ten seconds a connection is held.
pub(super) const BEGIN_PATIENCE: std::time::Duration = std::time::Duration::from_secs(5);

/// Caller identity follows the live record after an in-terminal resume.
pub(crate) fn resolved_caller(
    caller: Option<&str>,
    started_as: &std::collections::HashMap<guild_session::SessionId, guild_session::SessionId>,
) -> Result<Option<String>, String> {
    caller
        .map(|raw| {
            raw.parse::<guild_session::SessionId>()
                .map(|id| crate::binding::resolve(id, started_as).to_string())
                .map_err(|_| "Invalid calling session".into())
        })
        .transpose()
}

pub(crate) fn resolved_terminal_request(
    request: &guild_session::terminal_job::TerminalCommand,
    bindings: &std::collections::HashMap<guild_session::SessionId, guild_session::SessionId>,
) -> guild_session::terminal_job::TerminalCommand {
    let mut request = request.clone();
    if let guild_session::terminal_job::TerminalCommand::Start { spec } = &mut request
        && let guild_session::overlay::Target::Session(id) = &mut spec.target
    {
        *id = crate::binding::resolve(*id, bindings);
    }
    request
}

/// Keep process identity distinct from the conversation currently in its pane.
pub(crate) fn terminal_callers(
    caller: Option<&str>,
    bindings: &std::collections::HashMap<guild_session::SessionId, guild_session::SessionId>,
) -> Result<(guild_session::SessionId, guild_session::SessionId), String> {
    let launched: guild_session::SessionId = caller
        .ok_or("Run terminal commands inside a Guild session")?
        .parse()
        .map_err(|_| "Invalid calling session")?;
    let current = crate::binding::resolve(launched, bindings);
    Ok((launched, current))
}

#[cfg(test)]
mod caller_tests {
    use super::resolved_caller;
    use guild_session::{Desk, Runs, SessionRecord};
    #[test]
    fn terminal_retry_after_resume_keeps_one_job_and_original_control_identity() {
        use guild_session::{
            ProjectId, SessionId,
            terminal_job::{Book, LaunchSpec},
        };
        let launched = SessionId::new();
        let resumed = SessionId::new();
        let project = ProjectId::new();
        let spec: LaunchSpec = serde_json::from_value(serde_json::json!({
            "version":1,"request_id":"stable", "program":"/bin/echo","cwd":"/tmp",
            "target":{"kind":"session","id":launched}
        }))
        .unwrap();
        let mut bindings = std::collections::HashMap::new();
        let mut book = Book::default();
        let (owner, active) =
            super::terminal_callers(Some(&launched.to_string()), &bindings).unwrap();
        assert_eq!(active, launched);
        let first = *book.reserve(owner, project, &spec).unwrap().job_id();
        bindings.insert(launched, resumed);
        let (owner, active) =
            super::terminal_callers(Some(&launched.to_string()), &bindings).unwrap();
        assert_eq!(active, resumed);
        assert_eq!(
            book.get(&first).unwrap().owner,
            owner,
            "control access changed after resume"
        );
        assert_eq!(
            *book.reserve(owner, project, &spec).unwrap().job_id(),
            first
        );
    }
    #[test]
    fn terminal_target_follows_the_same_resume_binding_as_its_caller() {
        use guild_session::{SessionId, overlay::Target, terminal_job::TerminalCommand};
        let launched = SessionId::new();
        let live = SessionId::new();
        let request: TerminalCommand = serde_json::from_value(serde_json::json!({
            "operation":"start", "spec":{"version":1,"request_id":"resume", "program":"/bin/echo","cwd":"/tmp","target":{"kind":"session","id":launched}}
        })).unwrap();
        let bindings = std::collections::HashMap::from([(launched, live)]);
        let TerminalCommand::Start { spec } = super::resolved_terminal_request(&request, &bindings)
        else {
            panic!("start")
        };
        assert_eq!(spec.target, Target::Session(live));
    }
    #[test]
    fn resumed_observation_commands_authorize_the_live_record_and_preserve_task_role() {
        let mut desk = Desk::default();
        let project = desk.add_project(std::path::Path::new("/project"));
        let record =
            || SessionRecord::new("session".into(), project, "/project".into(), Runs::Shell);
        let launched = desk.add_session(record());
        let live = desk.add_session(record());
        desk.close_session(launched);
        let bindings = std::collections::HashMap::from([(launched, live)]);
        let owner = resolved_caller(Some(&launched.to_string()), &bindings)
            .unwrap()
            .unwrap();
        let mut board = guild_session::overlay::Board::default();
        board
            .open(
                "activity",
                &owner,
                guild_session::overlay::Target::Session(live),
                "Activity",
                guild_session::overlay::Content::Text {
                    text: "Actual output".into(),
                },
            )
            .unwrap();
        assert!(
            board
                .list(&owner)
                .iter()
                .all(|entry| entry.owner == live.to_string())
        );
        assert!(resolved_caller(Some("invalid"), &bindings).is_err());
        assert_eq!(resolved_caller(None, &bindings).unwrap(), None);
    }
}

#[cfg(test)]
mod worktree_name_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    const OPTIONS: &[&str] = &["--help", "-h", "-topic", "  --help  ", "\t-h\n", " -topic "];

    fn root(cx: &mut TestAppContext, path: &std::path::Path) -> gpui::Entity<Root> {
        cx.update(|cx| {
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), path);
            cx.new(|cx| {
                let mut root = Root::from_state(Default::default(), desk, cx);
                root.active_project = Some(project);
                root
            })
        })
    }

    #[gpui::test]
    fn worktree_names_are_refused_by_the_shared_native_path(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::worktree_names_are_refused_by_the_shared_native_path"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = root(cx, dir.path());
        cx.update(|cx| {
            root.update(cx, |root, _| {
                let project = root.active_project.unwrap();
                let results: Vec<_> = OPTIONS
                    .iter()
                    .map(|name| (*name, root.write_worktree(project, name, None).unwrap_err()))
                    .collect();
                for (name, because) in &results {
                    eprintln!("native {name:?}: {because}");
                }
                assert!(
                    results
                        .iter()
                        .all(|(_, because)| because == "a worktree name cannot begin with '-'"),
                    "{results:?}"
                );
                assert_eq!(
                    root.write_worktree(project, " \t", None).unwrap_err(),
                    "a worktree needs a name"
                );
                assert_eq!(
                    root.write_worktree(project, "topic", Some("--help".into()))
                        .unwrap_err(),
                    "--help is not a branch or a commit to start from"
                );
                assert!(root.pending_writes.is_empty());
                assert!(root.pending_tasks.is_empty());
            })
        });
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[gpui::test]
    fn worktree_cli_refuses_option_names_before_reading_the_default_base(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::worktree_cli_refuses_option_names_before_reading_the_default_base"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = root(cx, dir.path());
        cx.update(|cx| root.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let mut failures = Vec::new();
            for base in [None, Some("HEAD".to_owned())] {
                for name in OPTIONS {
                    let (answer, received) = std::sync::mpsc::channel();
                    root.command_create(
                        project,
                        (*name).into(),
                        base.clone(),
                        Some(Task {
                            prompt: "must never start".into(),
                            caller: None,
                            front: false,
                            workspace: None,
                            harness: guild_session::HarnessKind::Codex,
                        }),
                        answer,
                        cx,
                    );
                    // An immediate answer is required: deferring to main_branch
                    // would already schedule Git reads, even without a worker.
                    let result = received.try_recv();
                    eprintln!("CLI {name:?}, base={base:?}: {result:?}");
                    if !matches!(result, Ok(Answer::Refused { ref because }) if because == "a worktree name cannot begin with '-'") {
                        failures.push(format!("{name:?}, base={base:?}: {result:?}"));
                    }
                }
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
            assert!(root.pending_writes.is_empty());
            assert!(root.pending_tasks.is_empty());
        }));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[gpui::test]
    fn unresolved_worktree_identity_refuses_before_selecting_an_implicit_base(
        cx: &mut TestAppContext,
    ) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::unresolved_worktree_identity_refuses_before_selecting_an_implicit_base"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let entity = root(cx, dir.path());
        cx.update(|cx| entity.update(cx, |root, cx| {
            let project = root.active_project.unwrap();
            let (answer, received) = std::sync::mpsc::channel();
            root.command_create(project, "topic".into(), None, None, answer, cx);
            assert!(matches!(received.try_recv(), Ok(Answer::Refused { .. })),
                "an unresolved identity must not schedule an implicit base that can override repository preferences resolved later");
            root.scoped_settings.identities.insert(project, Err("metadata unavailable".into()));
            let error = root.write_worktree(project, "topic", None).unwrap_err();
            assert!(error.contains("metadata unavailable"), "{error}");
            assert!(root.pending_writes.is_empty());
            assert!(root.pending_tasks.is_empty());
        }));
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    fn git(path: &std::path::Path, args: &[&str]) -> std::process::Output {
        std::process::Command::new("git")
            .current_dir(path)
            .args(args)
            .output()
            .unwrap()
    }

    #[gpui::test]
    fn worktree_native_names_still_reach_git_validation(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::worktree_native_names_still_reach_git_validation"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir(&repo).unwrap();
        assert!(git(&repo, &["init", "-b", "main"]).status.success());
        assert!(
            git(
                &repo,
                &[
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "fixture"
                ]
            )
            .status
            .success()
        );
        let custom = dir.path().join("worktrees");
        std::fs::create_dir(&custom).unwrap();
        let root = root(cx, &repo);
        cx.update(|cx| {
            root.update(cx, |root, _| {
                let project = root.active_project.unwrap();
                root.desk.set_repository(project, Some(repo.join(".git")));
                root.scoped_settings.identities.insert(
                    project,
                    crate::repository_settings::resolve_repository(&repo),
                );
                let key = root.repository_identity(project).unwrap().key.clone();
                root.settings.repository_settings.insert(
                    key,
                    guild_session::repository_settings::RepositorySettings {
                        worktree_base: Some("main".into()),
                        worktree_directory: Some(custom.clone()),
                        ..Default::default()
                    },
                );
                root.status_workers
                    .insert(project, crate::root::git::StatusWorker::start(repo.clone()));
            })
        });
        // A live worker would actually dispatch a write if the native guard
        // were bypassed; refusal must still happen before its queue.
        cx.update(|cx| {
            root.update(cx, |root, _| {
                for name in OPTIONS {
                    assert_eq!(
                        root.write_worktree(root.active_project.unwrap(), name, None)
                            .unwrap_err(),
                        "a worktree name cannot begin with '-'"
                    );
                }
            })
        });
        for name in [
            "topic",
            "feature/topic",
            "topic-name",
            "  trimmed-topic  ",
            "bad..name",
        ] {
            let write = cx.update(|cx| {
                root.update(cx, |root, _| {
                    root.write_worktree(
                        root.active_project.unwrap(),
                        name,
                        (name == "feature/topic").then(|| "HEAD".into()),
                    )
                    .expect("the native guard leaves other names to Git")
                })
            });
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            let line = loop {
                let ran = cx.update(|cx| {
                    root.update(cx, |root, _| {
                        root.status_workers[&root.active_project.unwrap()]
                            .take()
                            .ran
                    })
                });
                if let Some(line) = ran.into_iter().next() {
                    break line;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "Git did not answer {name}"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            };
            let crate::git::Write::WorktreeAdd { path, base, .. } = write else {
                panic!("worktree write")
            };
            assert_eq!(
                base.as_deref(),
                Some(if name == "feature/topic" {
                    "HEAD"
                } else {
                    "main"
                })
            );
            assert_eq!(
                path.parent(),
                Some(custom.canonicalize().unwrap().as_path())
            );
            eprintln!(
                "Git {name:?}: failed={}, folder={}",
                line.failed,
                path.exists()
            );
            if name == "bad..name" {
                assert!(line.failed);
                assert!(!path.exists());
                assert!(
                    line.said.contains("not a valid branch name"),
                    "{}",
                    line.said
                );
            } else {
                assert!(!line.failed, "{}", line.said);
                assert!(path.join(".git").exists());
                assert_eq!(
                    String::from_utf8(git(&path, &["branch", "--show-current"]).stdout)
                        .unwrap()
                        .trim(),
                    name.trim()
                );
            }
        }
        cx.update(|cx| {
            root.update(cx, |root, _| {
                root.status_workers.clear();
            })
        });
    }
}

#[cfg(test)]
mod workflow_selection_tests {
    use super::*;
    use gpui::AppContext;
    #[gpui::test]
    fn invalid_assignment_selection_is_refused_before_any_native_session_is_created(
        cx: &mut gpui::TestAppContext,
    ) {
        let directory = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        desk.open_in(desk.active_workspace(), directory.path());
        let root = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        root.update(cx, |root, cx| {
            let before = root.desk.sessions().count();
            let result = root.command_session_new(
                directory.path(),
                Some("must not launch".into()),
                None,
                false,
                None,
                guild_session::HarnessKind::Codex,
                Some(guild_session::harness::LaunchSelection {
                    model: Some("--invalid-option".into()),
                    effort: None,
                }),
                cx,
            );
            assert!(matches!(result,Answer::Refused {because} if because.contains("model")));
            assert_eq!(root.desk.sessions().count(), before);
            assert!(root.tabs.is_empty());
        });
    }
}

#[cfg(test)]
mod listing_tests {
    use gpui::{AppContext, TestAppContext};
    use guild_session::status::{Kind, Reported};
    use guild_session::{Desk, Runs, Seat, SessionMode, SessionRecord};

    /// `guild agent list` says whether a tab of this window holds an open
    /// record: a project's state is shared by every window that opens its
    /// folder, so a record can be open in another window's workspace.
    #[gpui::test]
    fn the_listing_says_which_open_records_a_tab_here_holds(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::the_listing_says_which_open_records_a_tab_here_holds"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let record = |desk: &mut Desk, seat: Seat| {
                desk.add_session(SessionRecord::new(
                    "Agent".into(),
                    seat,
                    dir.path().into(),
                    Runs::Claude(SessionMode::Terminal),
                ))
            };
            let here = record(&mut desk, seat);
            // Another window's workspace, which this window does not have.
            let elsewhere = record(
                &mut desk,
                Seat {
                    workspace: guild_session::WorkspaceId::new(),
                    project,
                },
            );
            let root = cx.new(|cx| {
                let mut root = crate::Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root
            });
            root.update(cx, |root, cx| {
                root.adopt(
                    seat,
                    project,
                    crate::Body::Broken {
                        session: here,
                        reason: String::new(),
                    },
                    cx,
                );
                let agents = root.command_agents(cx);
                let row = |id: guild_session::SessionId| {
                    agents
                        .iter()
                        .find(|a| a.session == id.to_string())
                        .unwrap()
                        .clone()
                };
                assert!(row(here).open && row(here).tab);
                assert!(row(elsewhere).open, "its record still says open");
                assert!(!row(elsewhere).tab, "but no tab here holds it");
            });
        });
    }

    /// `guild agent list --json` says why a session waits when its program
    /// said why (OSC 7501 `kind`), beside the unchanged word; and a session
    /// that never reported is `unknown`, not idle.
    #[gpui::test]
    fn the_listing_carries_the_reported_waiting_reason(cx: &mut TestAppContext) {
        if !crate::root::isolation::isolated_test(concat!(
            module_path!(),
            "::the_listing_carries_the_reported_waiting_reason"
        )) {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        cx.update(|cx| {
            crate::editor::init(cx);
            let mut desk = Desk::default();
            desk.normalise();
            let workspace = desk.active_workspace();
            let project = desk.open_in(workspace, dir.path());
            let seat = Seat { workspace, project };
            let ids: Vec<_> = (0..3)
                .map(|_| {
                    desk.add_session(SessionRecord::new(
                        "Agent".into(),
                        seat,
                        dir.path().into(),
                        Runs::Claude(SessionMode::Terminal),
                    ))
                })
                .collect();
            let root = cx.new(|cx| {
                let mut root = crate::Root::from_state(Default::default(), desk, cx);
                root.isolated_render = true;
                root
            });
            root.update(cx, |root, cx| {
                root.board
                    .report(ids[0], Some(Reported::Blocked(Some(Kind::Auth))));
                root.board.report(ids[1], Some(Reported::Blocked(None)));
                let agents = root.command_agents(cx);
                let row = |id: guild_session::SessionId| {
                    agents
                        .iter()
                        .find(|a| a.session == id.to_string())
                        .unwrap()
                        .clone()
                };
                assert_eq!(row(ids[0]).status, "waiting");
                assert_eq!(row(ids[0]).kind.as_deref(), Some("auth"));
                assert_eq!(row(ids[1]).status, "waiting");
                assert_eq!(row(ids[1]).kind, None, "no reason invented");
                assert_eq!(row(ids[2]).status, "unknown");
                assert_eq!(row(ids[2]).kind, None);
            });
        });
    }
}

/// GLD-055: `guild session new --group NAME` and `guild group check`, against
/// the window's command handling — every refusal, none of which starts a
/// session.
#[cfg(test)]
mod group_door_tests {
    use super::*;
    use gpui::AppContext;

    fn refused(answer: Answer) -> String {
        match answer {
            Answer::Refused { because } => because,
            other => panic!("refused, not {other:?}"),
        }
    }

    #[gpui::test]
    fn the_group_door_resolves_the_name_and_refuses_what_it_cannot_do(
        cx: &mut gpui::TestAppContext,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let path = |name: &str| {
            let path = dir.path().join(name);
            std::fs::create_dir_all(&path).unwrap();
            path
        };
        let (a, b, c, d) = (path("a"), path("b"), path("c"), path("d"));
        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let first = desk.active_workspace();
        let second = desk.add_workspace("Second", None).unwrap();
        desk.open_in(first, &a);
        desk.open_in(second, &b);
        desk.open_in(second, &c);
        desk.open_in(second, &d);
        // `acme` in both workspaces; `twin` twice in the second.
        desk.create_project_group(first, "acme", &a).unwrap();
        let acme_second = desk.create_project_group(second, "acme", &b).unwrap();
        desk.create_project_group(second, "twin", &c).unwrap();
        let root = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        root.update(cx, |root, cx| {
            let ask =
                |root: &mut Root, name: &str, workspace: Option<&str>, cx: &mut Context<Root>| {
                    root.command_session_new_in_group(
                        name,
                        None,
                        None,
                        false,
                        workspace,
                        guild_session::HarnessKind::Claude,
                        cx,
                    )
                };
            let before = root.desk.sessions().count();
            // A name no workspace holds is refused by name.
            assert!(refused(ask(root, "nowhere", None, cx)).contains("nowhere"));
            // A name in several workspaces and no caller: pass --workspace.
            assert!(refused(ask(root, " acme ", None, cx)).contains("--workspace"));
            // With --workspace it resolves, and a group with no folder is
            // refused, saying where a folder is bound — never bound here.
            let second = second.to_string();
            assert!(refused(ask(root, "acme", Some(&second), cx)).contains("no folder"));
            assert!(
                root.desk
                    .workspace(second.parse().unwrap())
                    .unwrap()
                    .project_group(acme_second)
                    .unwrap()
                    .folder
                    .is_none()
            );
            // Two groups of one name in the resolved workspace: ambiguous.
            root.desk
                .create_project_group(second.parse().unwrap(), "twin", &d);
            assert!(refused(ask(root, "twin", Some(&second), cx)).contains("rename"));
            // A folder that is not a directory any more.
            let gone = dir.path().join("gone");
            std::fs::create_dir_all(&gone).unwrap();
            root.desk
                .bind_project_group_folder(second.parse().unwrap(), acme_second, &gone)
                .unwrap();
            std::fs::remove_dir(&gone).unwrap();
            assert!(refused(ask(root, "acme", Some(&second), cx)).contains("no folder at"));
            assert_eq!(root.desk.sessions().count(), before);

            // A task may not start a task, through this door too.
            let project = root.desk.projects().next().unwrap().id;
            let task = root.desk.add_session(guild_session::SessionRecord::new(
                "task".into(),
                (first, project),
                a.clone(),
                guild_session::Runs::Shell,
            ));
            root.desk.mark_task(task);
            let caller = task.to_string();
            let answer = root.command_session_new_in_group(
                "acme",
                Some("a task".into()),
                Some(&caller),
                false,
                Some(&second),
                guild_session::HarnessKind::Claude,
                cx,
            );
            assert_eq!(refused(answer), TASK_FROM_A_TASK);

            // A name in several workspaces resolves through the caller's.
            let project = root
                .desk
                .projects_of(second.parse().unwrap())
                .next()
                .unwrap()
                .id;
            let caller = root.desk.add_session(guild_session::SessionRecord::new(
                "caller".into(),
                (second.parse().unwrap(), project),
                b.clone(),
                guild_session::Runs::Shell,
            ));
            let (found, groups) = root
                .command_groups_named("acme", None, Some(&caller.to_string()))
                .unwrap();
            assert_eq!(found.to_string(), second);
            assert_eq!(groups.len(), 1);

            // `group check` reads every group of the workspace.
            let Answer::Groups { groups } = root.command_group_check(None, Some(&second), None)
            else {
                panic!("groups");
            };
            let names: Vec<_> = groups.iter().map(|group| group.name.as_str()).collect();
            assert_eq!(names, ["acme", "twin", "twin"]);
            assert_eq!(groups[0].matches, Some(false));
            assert_eq!(groups[1].folder, None);
            assert_eq!(groups[1].members[0].inside, None);
        });
    }
}

/// GLD-058: a session a command opens behind the person's work, in a
/// workspace this run has not shown yet, must not write that workspace's
/// projects back from a memory that holds none of their tabs, nor touch the
/// person's place. Each of the three doors into `command_session_start` —
/// `session new`, `session new --group` and `worktree create --prompt` — is
/// driven against its own fresh restart, and the projects' state files are
/// read back from disk once the writer has drained.
#[cfg(test)]
mod background_launch_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};
    use guild_session::layout::Axis;
    use guild_session::{
        AreaState, OpenTab, ProjectState, SavedAreas, SeatState, SessionId, Shows,
    };
    use std::path::{Path, PathBuf};

    /// Which door the session goes through.
    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Door {
        SessionNew,
        Group,
        WorktreePrompt,
    }

    /// Names the fixture's own directory in the isolated child.
    const FIXTURE: &str = "GUILD_BACKGROUND_LAUNCH_FIXTURE";

    /// Run `name` in an isolated child whose terminal programs are the
    /// fixture's: zsh with an empty startup directory, a `PATH` of the
    /// fixture's and the system's, Claude's configuration in the fixture,
    /// a `claude` that exits, and a launch prefix standing in for the
    /// agent — it marks its pid and stays until its restart's fixture is
    /// dropped, a minute at most, starting nothing. The fixture's
    /// directory in the child, `None` in the parent.
    fn isolated(name: &str) -> Option<PathBuf> {
        if let Some(dir) = std::env::var_os(FIXTURE) {
            assert!(crate::root::isolation::isolated_test_with(name, &[]));
            return Some(dir.into());
        }
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let startup = dir.path().join("zdotdir");
        let bin = dir.path().join("bin");
        let launched = dir.path().join("launched");
        let claude = dir.path().join("claude-home");
        for made in [&startup, &bin, &launched, &claude] {
            std::fs::create_dir_all(made).unwrap();
        }
        for file in [".zshenv", ".zprofile", ".zshrc", ".zlogin"] {
            std::fs::write(startup.join(file), "").unwrap();
        }
        let executable = |path: PathBuf, text: String| {
            std::fs::write(&path, text).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        executable(bin.join("claude"), "#!/bin/sh\nexit 0\n".into());
        let here = dir.path().display();
        executable(
            dir.path().join("prefix"),
            format!(
                "#!/bin/sh\nrestart=$(cat '{here}/restart')\n: > '{here}/launched/'\"$$\"\n\
                 i=0; while [ ! -e \"{here}/stop-$restart\" ] && [ $i -lt 600 ]; do \
                 sleep 0.1; i=$((i+1)); done\nexit 0\n"
            ),
        );
        let path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display());
        crate::root::isolation::isolated_test_with(
            name,
            &[
                ("SHELL", "/bin/zsh".as_ref()),
                ("ZDOTDIR", startup.as_os_str()),
                ("PATH", path.as_ref()),
                ("CLAUDE_CONFIG_DIR", claude.as_os_str()),
                (FIXTURE, dir.path().as_os_str()),
            ],
        );
        None
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        _state: tempfile::TempDir,
        /// The isolated child's fixture directory: the prefix, its marks.
        home: PathBuf,
        /// Which restart this is, for its stand-ins to stop on.
        restart: usize,
        /// The stand-ins earlier restarts marked, stopped by now.
        earlier: Vec<u32>,
        root: gpui::Entity<Root>,
        first: WorkspaceId,
        second: WorkspaceId,
        a: ProjectId,
        /// B's two projects and its group's folder, each with saved tabs.
        b1: PathBuf,
        b2: PathBuf,
        b2_id: ProjectId,
        group: PathBuf,
        /// A worktree folder git has just made, B's from the command.
        worktree: PathBuf,
        /// What b1, b2 and the group's folder were saved with in B.
        saved: [SeatState; 3],
    }

    impl Drop for Fixture {
        /// The stand-ins of this restart go, and every queued write lands
        /// before the folders it writes into are removed — on an unwind
        /// too, or the writer makes them again after the test.
        fn drop(&mut self) {
            let _ = std::fs::write(self.home.join(format!("stop-{}", self.restart)), "");
            guild_session::flush();
        }
    }

    fn file(name: &str) -> OpenTab {
        OpenTab::showing(Shows::File(name.into()))
    }

    /// b1's seat in B: divided, two tabs on the left with the second in
    /// front, one on the right, the keys on the left.
    fn divided() -> SeatState {
        let mut saved = SavedAreas::one(vec![file("one.txt"), file("two.txt")], Some(1));
        let left = saved.areas.focused();
        let right = saved.areas.split(left, Axis::Horizontal).unwrap();
        *saved.strip_mut(right) = AreaState {
            tabs: vec![file("three.txt")],
            active: Some(0),
        };
        saved.areas.focus(left);
        SeatState::from_areas(saved, Vec::new())
    }

    fn single(name: &str) -> SeatState {
        SeatState::from_areas(SavedAreas::one(vec![file(name)], Some(0)), Vec::new())
    }

    /// `saved` with `tabs` added at the end of its focused area's strip,
    /// its order and every area's front as they were.
    fn appended(saved: &SeatState, tabs: &[Shows]) -> SeatState {
        let mut areas = saved.areas_or_one();
        let focused = areas.areas.focused();
        let strip = areas.strip_mut(focused);
        strip
            .tabs
            .extend(tabs.iter().cloned().map(OpenTab::showing));
        SeatState::from_areas(areas, saved.expanded.clone())
    }

    fn folder(dir: &Path, name: &str, files: &[&str]) -> PathBuf {
        let path = dir.join(name);
        std::fs::create_dir_all(&path).unwrap();
        for file in files {
            std::fs::write(path.join(file), "text\n").unwrap();
        }
        path
    }

    fn save_seat(path: &Path, workspace: WorkspaceId, first: WorkspaceId, seat: SeatState) {
        let mut state = guild_session::project_state::load(path);
        state.set_seat(workspace, first, seat);
        guild_session::project_state::save(path, &state).unwrap();
    }

    /// The seat as the project's state file holds it — the file itself,
    /// read once every queued write has landed, not the writer's pending
    /// snapshot that `project_state::load` would answer.
    fn seat_on_disk(path: &Path, workspace: WorkspaceId, first: WorkspaceId) -> SeatState {
        guild_session::flush();
        let state = match std::fs::read(path.join(".guild").join("state.json")) {
            Ok(bytes) => serde_json::from_slice::<ProjectState>(&bytes).unwrap(),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => ProjectState::default(),
            Err(err) => panic!("{}: {err}", path.display()),
        };
        state.seat(workspace, first)
    }

    /// Guild restarted in workspace A: A's tab is back and in front, B —
    /// two projects and a group with a bound folder, every one with saved
    /// tabs, b2 the project it was left on — has not been visited.
    fn restarted_in_a(home: &Path, cx: &mut TestAppContext) -> Fixture {
        restarted_with(home, cx, |_, _, _| {})
    }

    /// [`restarted_in_a`], with `before` putting more in the desk and on
    /// disk before the restart reads them: B's workspace, b2's id and path.
    fn restarted_with(
        home: &Path,
        cx: &mut TestAppContext,
        before: impl FnOnce(&mut guild_session::Desk, WorkspaceId, (ProjectId, &Path)),
    ) -> Fixture {
        static RESTARTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let restart = RESTARTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::fs::write(home.join("restart"), restart.to_string()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let a_path = folder(dir.path(), "a", &["a.txt"]);
        let b1 = folder(dir.path(), "b1", &["one.txt", "two.txt", "three.txt"]);
        let b2 = folder(dir.path(), "b2", &["four.txt"]);
        let group = folder(dir.path(), "acme", &["notes.txt"]);
        let worktree = folder(dir.path(), "b1-topic", &[]);

        let mut desk = guild_session::Desk::default();
        desk.normalise();
        let first = desk.active_workspace();
        let second = desk.add_workspace("Second", None).unwrap();
        let a = desk.open_in(first, &a_path);
        desk.open_in(second, &b1);
        let b2_id = desk.open_in(second, &b2);
        let acme = desk.create_project_group(second, "acme", &b1).unwrap();
        desk.bind_project_group_folder(second, acme, &group)
            .unwrap();
        desk.select_workspace(second);
        desk.look_at(Some(b2_id), None);
        desk.select_workspace(first);
        desk.look_at(Some(a), None);

        save_seat(&a_path, first, first, single("a.txt"));
        let mut saved = [divided(), single("four.txt"), single("notes.txt")];
        for (path, seat) in [&b1, &b2, &group].into_iter().zip(&saved) {
            save_seat(path, second, first, seat.clone());
        }
        before(&mut desk, second, (b2_id, &b2));
        saved[1] = guild_session::project_state::load(&b2).seat(second, first);
        // What a restart reads is on disk, not in the writer's queue.
        guild_session::flush();

        let prefix = home.join("prefix").display().to_string();
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            cx.new(|cx| {
                let mut root = Root::diagnostic(desk, state.path().into(), cx);
                root.settings.launch_prefixes.claude = prefix;
                root.isolated_render = true;
                root.active_workspace = first;
                root.active_project = Some(a);
                root.restore_tabs(cx);
                root
            })
        });
        Fixture {
            _dir: dir,
            _state: state,
            home: home.to_path_buf(),
            restart,
            earlier: std::fs::read_dir(home.join("launched"))
                .unwrap()
                .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
                .collect(),
            root,
            first,
            second,
            a,
            b1,
            b2,
            b2_id,
            group,
            worktree,
            saved,
        }
    }

    /// The pids the stand-in has marked, in no order.
    fn launches(fixture: &Fixture) -> Vec<u32> {
        std::fs::read_dir(fixture.home.join("launched"))
            .unwrap()
            .filter_map(|entry| entry.ok()?.file_name().to_str()?.parse().ok())
            .collect()
    }

    /// The launch's stand-in, once it has marked that it runs.
    fn launched_since(fixture: &Fixture, before: &[u32]) -> u32 {
        let by = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Some(pid) = launches(fixture)
                .into_iter()
                .find(|pid| !before.contains(pid))
            {
                return pid;
            }
            assert!(
                std::time::Instant::now() < by,
                "the launch prefix never ran"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    fn alive(pid: u32) -> bool {
        std::process::Command::new("/bin/kill")
            .args(["-0", &pid.to_string()])
            .status()
            .is_ok_and(|status| status.success())
    }

    fn launch(fixture: &Fixture, door: Door, front: bool, cx: &mut TestAppContext) -> SessionId {
        let before = launches(fixture);
        let second = fixture.second.to_string();
        let answer = fixture.root.update(cx, |root, cx| match door {
            Door::SessionNew => root.command_session_new(
                &fixture.b1,
                None,
                None,
                front,
                Some(&second),
                guild_session::HarnessKind::Claude,
                None,
                cx,
            ),
            Door::Group => root.command_session_new_in_group(
                "acme",
                None,
                None,
                front,
                Some(&second),
                guild_session::HarnessKind::Claude,
                cx,
            ),
            Door::WorktreePrompt => {
                // git has made the worktree; its list now says so, and the
                // held answer starts the task, as `answer_settled` does
                // when the list agrees.
                root.worktrees.insert(
                    fixture.b1.clone(),
                    vec![crate::git::worktrees::Worktree {
                        path: fixture.worktree.clone(),
                        branch: Some("topic".into()),
                        ..Default::default()
                    }],
                );
                root.pending_tasks.push((
                    fixture.worktree.clone(),
                    Task {
                        prompt: "the task".into(),
                        caller: None,
                        front,
                        workspace: Some(second.clone()),
                        harness: guild_session::HarnessKind::Claude,
                    },
                ));
                let (answer, received) = std::sync::mpsc::channel();
                root.pending_settle.push((
                    Settles::Listed(fixture.worktree.clone()),
                    answer,
                    Answer::Created {
                        path: fixture.worktree.clone(),
                        branch: "topic".into(),
                        base: None,
                        session: None,
                    },
                ));
                root.answer_settled(cx);
                received.try_recv().unwrap()
            }
        });
        let session = started(answer, door);
        // The launch reached the fixture's prefix through the fixture's
        // shell: the session's program really started, and nothing else.
        launched_since(fixture, &before);
        session
    }

    fn started(answer: Answer, door: Door) -> SessionId {
        let session = match answer {
            Answer::SessionStarted { session, .. } => session,
            Answer::Created {
                session: Some(session),
                ..
            } => session,
            other => panic!("{door:?}: a session, not {other:?}"),
        };
        session.parse().unwrap()
    }

    /// The project of B the door's session goes into.
    fn launched_into(fixture: &Fixture, door: Door) -> &Path {
        match door {
            Door::SessionNew => &fixture.b1,
            Door::Group => &fixture.group,
            Door::WorktreePrompt => &fixture.worktree,
        }
    }

    /// What B's projects hold on disk: every saved tab, area, order and
    /// front kept, and the new session added at the end of its own
    /// project's focused area.
    fn assert_b_kept_on_disk(fixture: &Fixture, door: Door, session: SessionId) {
        let (first, second) = (fixture.first, fixture.second);
        let into = launched_into(fixture, door);
        let new = [Shows::Session(session)];
        for (path, saved) in [&fixture.b1, &fixture.b2, &fixture.group]
            .into_iter()
            .zip(fixture.saved.iter())
        {
            let expected = if path.as_path() == into {
                appended(saved, &new)
            } else {
                saved.clone()
            };
            assert_eq!(
                seat_on_disk(path, second, first),
                expected,
                "{door:?}: {}",
                path.display()
            );
        }
        if door == Door::WorktreePrompt {
            assert_eq!(
                seat_on_disk(&fixture.worktree, second, first),
                appended(&SeatState::default(), &new),
                "{door:?}: the worktree's strip is its session"
            );
        }
    }

    /// The person's place in A, the focus included.
    fn place(
        root: &Root,
    ) -> (
        WorkspaceId,
        Option<ProjectId>,
        Option<crate::TabId>,
        Option<guild_session::AreaId>,
        Option<guild_session::PaneId>,
    ) {
        let pane = root
            .active_tab
            .and_then(|id| root.tabs.iter().find(|tab| tab.id == id))
            .map(|tab| tab.panes.focused());
        (
            root.active_workspace,
            root.active_project,
            root.active_tab,
            root.focused_area(),
            pane,
        )
    }

    /// The terminal showing `session`, by its entity.
    fn terminal_of(root: &Root, session: SessionId) -> Option<gpui::EntityId> {
        root.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                crate::Body::Terminal {
                    session: shown,
                    view,
                    ..
                } if *shown == session => Some(view.entity_id()),
                _ => None,
            })
    }

    /// Visiting B shows each of its tabs once — none restored a second
    /// time, the session in one tab, still the terminal and the process it
    /// was started in — on the project B was left on.
    fn assert_visit_shows_each_once(
        fixture: &Fixture,
        door: Door,
        session: SessionId,
        cx: &mut TestAppContext,
    ) {
        let launched = launches(fixture);
        let terminal = fixture
            .root
            .read_with(cx, |root, _| terminal_of(root, session))
            .expect("the session's terminal");
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            assert_eq!(
                root.active_project,
                Some(fixture.b2_id),
                "{door:?}: B opens on the project it was left on"
            );
            assert_eq!(
                root.visible_tabs().count(),
                1,
                "{door:?}: and that project's strip is there"
            );
            let holding = root
                .tabs
                .iter()
                .filter(|tab| tab.sessions().any(|held| held == session))
                .count();
            assert_eq!(holding, 1, "{door:?}: one tab holds the session");
            assert_eq!(
                terminal_of(root, session),
                Some(terminal),
                "{door:?}: the terminal it was started in"
            );
            let into = launched_into(fixture, door).to_path_buf();
            for (path, files) in [
                (&fixture.b1, 3),
                (&fixture.b2, 1),
                (&fixture.group, 1),
                (&fixture.worktree, 0),
            ] {
                let project = root
                    .desk
                    .projects_of(fixture.second)
                    .find(|project| &project.path == path)
                    .map(|project| project.id);
                let Some(project) = project else {
                    assert_eq!(files, 0, "{door:?}: {} is B's", path.display());
                    continue;
                };
                let seat = guild_session::Seat {
                    workspace: fixture.second,
                    project,
                };
                let tabs = root.tabs.iter().filter(|tab| tab.seat == seat).count();
                let expected = files + usize::from(*path == into);
                assert_eq!(
                    tabs,
                    expected,
                    "{door:?}: {} shows each tab once",
                    path.display()
                );
            }
        });
        let now = launches(fixture);
        assert_eq!(
            now.len(),
            launched.len(),
            "{door:?}: nothing launched again"
        );
        assert!(
            launched
                .iter()
                .filter(|pid| !fixture.earlier.contains(pid))
                .all(|pid| alive(*pid)),
            "{door:?}: the stand-in still runs"
        );
        // And what the visit wrote keeps them: the saved tabs, and the
        // session once, at the end of its strip.
        assert_b_kept_on_disk(fixture, door, session);
    }

    fn background(home: &Path, cx: &mut TestAppContext, door: Door) {
        let fixture = restarted_in_a(home, cx);
        let before = fixture.root.read_with(cx, |root, _| {
            assert!(root.active_tab.is_some(), "A's tab is back and in front");
            assert_eq!(root.active_project, Some(fixture.a));
            place(root)
        });
        let session = launch(&fixture, door, false, cx);
        fixture.root.read_with(cx, |root, _| {
            assert_eq!(place(root), before, "{door:?}: A stays in front");
            assert_eq!(root.desk.active_workspace(), fixture.first);
            assert_eq!(root.desk.active_project(), Some(fixture.a));
            assert_eq!(
                root.desk.workspace(fixture.second).unwrap().front,
                Some(fixture.b2_id),
                "{door:?}: B still remembers the project it was left on"
            );
        });
        assert_b_kept_on_disk(&fixture, door, session);
        assert_visit_shows_each_once(&fixture, door, session, cx);
    }

    #[gpui::test]
    fn a_background_launch_into_an_unvisited_workspace_keeps_its_saved_tabs(
        cx: &mut TestAppContext,
    ) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_into_an_unvisited_workspace_keeps_its_saved_tabs"
        )) else {
            return;
        };
        for door in [Door::SessionNew, Door::Group, Door::WorktreePrompt] {
            background(&home, cx, door);
        }
    }

    /// What the person had open in A stays open, and nothing is set to
    /// take the keys at the next frame: a background launch is not a
    /// workspace switch. Whether a real field keeps the window's focus
    /// needs a window, which this test does not have.
    #[gpui::test]
    fn a_background_launch_leaves_the_foreground_s_fields_open(cx: &mut TestAppContext) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_leaves_the_foreground_s_fields_open"
        )) else {
            return;
        };
        let fixture = restarted_in_a(&home, cx);
        fixture.root.update(cx, |root, _| {
            root.tab_launcher.visible = true;
            root.showing_servers = true;
            root.focusing_front = false;
        });
        launch(&fixture, Door::SessionNew, false, cx);
        fixture.root.read_with(cx, |root, _| {
            assert!(root.tab_launcher.visible, "the + menu is still open");
            assert!(root.showing_servers, "the servers list is still open");
            assert!(
                !root.focusing_front,
                "the next frame does not give the keys to the body in front"
            );
            assert!(
                root.closed_focus.is_empty(),
                "no closed body asks the next frame to move the keys"
            );
        });
    }

    /// A project of B whose folder is away while the session starts — a
    /// volume not mounted — keeps its saved tabs when the folder is back
    /// before B is visited, and nothing makes the folder meanwhile.
    #[gpui::test]
    fn a_background_launch_keeps_a_project_whose_folder_is_away(cx: &mut TestAppContext) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_keeps_a_project_whose_folder_is_away"
        )) else {
            return;
        };
        let fixture = restarted_in_a(&home, cx);
        // The restart's own writes have landed before the volume goes.
        guild_session::flush();
        let away = fixture.b2.with_extension("away");
        std::fs::rename(&fixture.b2, &away).unwrap();
        let session = launch(&fixture, Door::SessionNew, false, cx);
        guild_session::flush();
        let made = walkdir(&fixture.b2);
        assert!(made.is_empty(), "the away folder was made again: {made:?}");
        std::fs::rename(&away, &fixture.b2).unwrap();
        assert_b_kept_on_disk(&fixture, Door::SessionNew, session);
        // B visited with the folder back: what the visit saves is what b2
        // was saved with, not the empty seat a read of the absent folder
        // would have given.
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            root.select_workspace(fixture.first, cx);
        });
        assert_eq!(
            seat_on_disk(&fixture.b2, fixture.second, fixture.first),
            fixture.saved[1],
            "b2 keeps its saved tabs once its folder is back"
        );
        assert_visit_shows_each_once(&fixture, Door::SessionNew, session, cx);
    }

    /// A file a saved tab of B shows, away while the session starts —
    /// another branch checked out — comes back as a tab when it is back
    /// before B is visited.
    #[gpui::test]
    fn a_background_launch_keeps_a_tab_whose_file_is_away(cx: &mut TestAppContext) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_keeps_a_tab_whose_file_is_away"
        )) else {
            return;
        };
        let fixture = restarted_in_a(&home, cx);
        let three = fixture.b1.join("three.txt");
        let away = three.with_extension("away");
        std::fs::rename(&three, &away).unwrap();
        let session = launch(&fixture, Door::Group, false, cx);
        std::fs::rename(&away, &three).unwrap();
        assert_b_kept_on_disk(&fixture, Door::Group, session);
        assert_visit_shows_each_once(&fixture, Door::Group, session, cx);
    }

    /// A seat saved before tab lists — sessions marked open and nothing
    /// else — keeps those sessions when a background session is added to
    /// it: they are written into its strip first, in the order the
    /// restore would have shown them, and the visit shows them all.
    #[gpui::test]
    fn a_background_launch_keeps_a_seat_s_open_sessions_from_before_tab_lists(
        cx: &mut TestAppContext,
    ) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_keeps_a_seat_s_open_sessions_from_before_tab_lists"
        )) else {
            return;
        };
        let mut shells = Vec::new();
        let fixture = restarted_with(&home, cx, |desk, second, (b2, path)| {
            // b2 has no tab list in B, and two shells marked open there.
            let mut state = guild_session::project_state::load(path);
            state.set_seat(second, desk.workspaces()[0].id, SeatState::default());
            guild_session::project_state::save(path, &state).unwrap();
            for name in ["one", "two"] {
                shells.push(desk.add_session(guild_session::SessionRecord::new(
                    name.into(),
                    (second, b2),
                    path.to_path_buf(),
                    guild_session::Runs::Shell,
                )));
            }
        });
        assert_eq!(fixture.saved[1], SeatState::default());
        let before = launches(&fixture);
        let second = fixture.second.to_string();
        let answer = fixture.root.update(cx, |root, cx| {
            root.command_session_new(
                &fixture.b2,
                None,
                None,
                false,
                Some(&second),
                guild_session::HarnessKind::Claude,
                None,
                cx,
            )
        });
        let session = started(answer, Door::SessionNew);
        launched_since(&fixture, &before);
        let order: Vec<Shows> = shells
            .iter()
            .chain([&session])
            .map(|id| Shows::Session(*id))
            .collect();
        assert_eq!(
            seat_on_disk(&fixture.b2, fixture.second, fixture.first),
            appended(&SeatState::default(), &order),
            "the open sessions first, then the new one"
        );
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            assert_eq!(root.active_project, Some(fixture.b2_id));
            let shown: Vec<SessionId> = root
                .visible_tabs()
                .flat_map(|tab| tab.sessions().collect::<Vec<_>>())
                .collect();
            assert_eq!(
                shown,
                shells.iter().copied().chain([session]).collect::<Vec<_>>(),
                "each once, in order"
            );
        });
        // Nothing was in front of a seat from before tab lists: the
        // visit brings its last forward, as its restore always has.
        let mut visited = appended(&SeatState::default(), &order);
        visited.active = Some(2);
        assert_eq!(
            seat_on_disk(&fixture.b2, fixture.second, fixture.first),
            visited,
            "and the visit keeps them"
        );
    }

    /// A background task stopped before its workspace is visited is not
    /// started again by the visit, though its conversation could be
    /// continued: its tab leaves its seat's file, the rest of the seat as
    /// it was.
    #[gpui::test]
    fn a_task_stopped_before_its_workspace_is_visited_stays_stopped(cx: &mut TestAppContext) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_task_stopped_before_its_workspace_is_visited_stays_stopped"
        )) else {
            return;
        };
        let fixture = restarted_in_a(&home, cx);
        let session = launch(&fixture, Door::SessionNew, false, cx);
        // It had a turn: a restore would continue it.
        let store = home.join("claude-home").join("projects").join("fixture");
        std::fs::create_dir_all(&store).unwrap();
        std::fs::write(
            store.join(format!("{session}.jsonl")),
            serde_json::json!({
                "type": "user",
                "cwd": fixture.b1,
                "sessionId": session.to_string(),
                "message": {"role": "user", "content": "the task"},
            })
            .to_string()
                + "\n",
        )
        .unwrap();
        let stopped = fixture.root.update(cx, |root, cx| {
            root.desk.mark_task(session);
            root.command_stop(&session.to_string(), cx)
        });
        assert!(matches!(stopped, Answer::Done), "{stopped:?}");
        assert_eq!(
            seat_on_disk(&fixture.b1, fixture.second, fixture.first),
            fixture.saved[0],
            "b1's file is as it was saved"
        );
        let launched = launches(&fixture);
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            assert!(
                root.tabs
                    .iter()
                    .all(|tab| tab.sessions().all(|held| held != session)),
                "no tab holds the stopped task"
            );
            assert!(
                root.desk.session(session).is_none_or(|record| !record.open),
                "its record stays closed"
            );
        });
        assert_eq!(launches(&fixture).len(), launched.len(), "nothing launched");
        assert_eq!(
            seat_on_disk(&fixture.b1, fixture.second, fixture.first),
            fixture.saved[0],
            "and the visit keeps b1 as it was saved"
        );
    }

    /// A saved tab whose record is closed — a task stopped in a workspace
    /// not shown, by a build before its tab left the file — is not
    /// continued by a restore, though its conversation could be: closed
    /// is what the record says, and a restart reopens only what was open.
    #[gpui::test]
    fn a_restore_does_not_continue_a_closed_record_its_seat_still_names(cx: &mut TestAppContext) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_restore_does_not_continue_a_closed_record_its_seat_still_names"
        )) else {
            return;
        };
        let mut closed = None;
        let fixture = restarted_with(&home, cx, |desk, second, (b2, path)| {
            let mut record = guild_session::SessionRecord::new(
                "stopped".into(),
                (second, b2),
                path.to_path_buf(),
                guild_session::Runs::Claude(guild_session::SessionMode::Terminal),
            );
            record.open = false;
            record.task = true;
            // It had a turn: kept across the restart, and continuable.
            record.had_turn = true;
            let id = desk.add_session(record);
            let store = home.join("claude-home").join("projects").join("fixture");
            std::fs::create_dir_all(&store).unwrap();
            std::fs::write(
                store.join(format!("{id}.jsonl")),
                serde_json::json!({
                    "type": "user",
                    "cwd": path,
                    "sessionId": id.to_string(),
                    "message": {"role": "user", "content": "the task"},
                })
                .to_string()
                    + "\n",
            )
            .unwrap();
            let first = desk.workspaces()[0].id;
            let mut state = guild_session::project_state::load(path);
            let seat = appended(&state.seat(second, first), &[Shows::Session(id)]);
            state.set_seat(second, first, seat);
            guild_session::project_state::save(path, &state).unwrap();
            closed = Some(id);
        });
        let closed = closed.unwrap();
        let launched = launches(&fixture);
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            assert_eq!(root.active_project, Some(fixture.b2_id));
            assert_eq!(root.visible_tabs().count(), 1, "four.txt alone");
            assert!(
                root.tabs
                    .iter()
                    .all(|tab| tab.sessions().all(|held| held != closed))
            );
            assert!(root.desk.session(closed).is_none_or(|record| !record.open));
        });
        assert_eq!(launches(&fixture).len(), launched.len(), "nothing launched");
        assert_eq!(
            seat_on_disk(&fixture.b2, fixture.second, fixture.first),
            single("four.txt"),
            "the visit writes the seat without it"
        );
    }

    /// The session ids a project's own file holds, read once every queued
    /// write has landed.
    fn records_on_disk(path: &Path) -> Vec<SessionId> {
        guild_session::flush();
        let bytes = std::fs::read(path.join(".guild").join("state.json")).unwrap();
        let state: ProjectState = serde_json::from_slice(&bytes).unwrap();
        state.sessions.iter().map(|record| record.id).collect()
    }

    /// Everything a seat's strips name, area by area, in order.
    fn shown(seat: &SeatState) -> Vec<Shows> {
        let areas = seat.areas_or_one();
        areas
            .strips
            .values()
            .flat_map(|strip| strip.tabs.iter().map(|tab| tab.shows.clone()))
            .collect()
    }

    /// Two shells of B in b2, both in its strip: on disk only, or in the
    /// desk too — what the caller does with them.
    fn shells_of_b2(
        desk: &guild_session::Desk,
        second: WorkspaceId,
        b2: ProjectId,
        path: &Path,
    ) -> (Vec<guild_session::SessionRecord>, SeatState) {
        let records: Vec<_> = ["one", "two"]
            .into_iter()
            .map(|name| {
                guild_session::SessionRecord::new(
                    name.into(),
                    (second, b2),
                    path.to_path_buf(),
                    guild_session::Runs::Shell,
                )
            })
            .collect();
        let order: Vec<Shows> = records
            .iter()
            .map(|record| Shows::Session(record.id))
            .collect();
        let first = desk.workspaces()[0].id;
        let seat = appended(&SeatState::default(), &order);
        let mut state = guild_session::project_state::load(path);
        state.set_seat(second, first, seat.clone());
        guild_session::project_state::save(path, &state).unwrap();
        (records, seat)
    }

    /// GLD-060, the first route: b2 was closed in B with
    /// `forget_sessions_on_close` off, so B's records of it stayed beside
    /// it; Guild restarted in A, and `guild session new b2` without
    /// `--front` opens it in B again. The launch's save must not drop
    /// those records, and the visit brings every tab of the strip back.
    #[gpui::test]
    fn a_background_launch_into_a_project_closed_in_its_workspace_keeps_its_records(
        cx: &mut TestAppContext,
    ) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_background_launch_into_a_project_closed_in_its_workspace_keeps_its_records"
        )) else {
            return;
        };
        let mut shells = Vec::new();
        let fixture = restarted_with(&home, cx, |desk, second, (b2, path)| {
            let (records, _) = shells_of_b2(desk, second, b2, path);
            let mut state = guild_session::project_state::load(path);
            shells = records.iter().map(|record| record.id).collect();
            state.sessions.extend(records);
            guild_session::project_state::save(path, &state).unwrap();
            // Closed in B, its only holder: the root goes from the desk,
            // its file keeps B's records and strip.
            assert_eq!(desk.leave(second, b2), guild_session::Released::Released);
        });
        assert_eq!(records_on_disk(&fixture.b2), shells);
        let before = launches(&fixture);
        let second = fixture.second.to_string();
        let answer = fixture.root.update(cx, |root, cx| {
            root.command_session_new(
                &fixture.b2,
                None,
                None,
                false,
                Some(&second),
                guild_session::HarnessKind::Claude,
                None,
                cx,
            )
        });
        let session = started(answer, Door::SessionNew);
        launched_since(&fixture, &before);
        let expected: Vec<SessionId> = shells.iter().copied().chain([session]).collect();
        assert_eq!(
            records_on_disk(&fixture.b2),
            expected,
            "B's retained records are kept, and the new one added"
        );
        let strip = shown(&seat_on_disk(&fixture.b2, fixture.second, fixture.first));
        let order: Vec<Shows> = expected.iter().map(|id| Shows::Session(*id)).collect();
        assert_eq!(strip, order);

        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            let b2 = root
                .desk
                .projects_of(fixture.second)
                .find(|project| project.path == fixture.b2)
                .map(|project| project.id)
                .expect("b2 is B's again");
            let seat = guild_session::Seat {
                workspace: fixture.second,
                project: b2,
            };
            let restored: Vec<SessionId> = root
                .tabs
                .iter()
                .filter(|tab| tab.seat == seat)
                .flat_map(|tab| tab.sessions().collect::<Vec<_>>())
                .collect();
            assert_eq!(restored, expected, "every tab the strip names is back");
        });
        assert_eq!(
            shown(&seat_on_disk(&fixture.b2, fixture.second, fixture.first)),
            order,
            "and the visit's save keeps the strip whole"
        );
        assert_eq!(records_on_disk(&fixture.b2), expected);
    }

    /// GLD-060, the second route: Guild starts while b2, a project of the
    /// unvisited B, is away — a volume not mounted — so the desk is read
    /// without B's records of it; the folder is back before the next save.
    /// That save must not drop them, and the visit brings b2's tabs back.
    #[gpui::test]
    fn a_save_after_a_folder_comes_back_keeps_the_records_it_was_read_without(
        cx: &mut TestAppContext,
    ) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_save_after_a_folder_comes_back_keeps_the_records_it_was_read_without"
        )) else {
            return;
        };
        let mut shells = Vec::new();
        let fixture = restarted_with(&home, cx, |desk, second, (b2, path)| {
            let (records, _) = shells_of_b2(desk, second, b2, path);
            shells = records.iter().map(|record| record.id).collect();
            for record in records {
                desk.add_session(record);
            }
            // The run before wrote them beside b2; this one reads the
            // desk while the folder is away and starts with what it read.
            let global = path.parent().unwrap().join("before.json");
            guild_session::persist::save(desk, &global).unwrap();
            assert_eq!(records_on_disk(path), shells);
            let away = path.with_extension("away");
            std::fs::rename(path, &away).unwrap();
            *desk = guild_session::persist::load(&global);
            std::fs::rename(&away, path).unwrap();
            assert!(shells.iter().all(|id| desk.session(*id).is_none()));
        });
        // Whatever the restart saved, once the folder was back, kept them.
        assert_eq!(records_on_disk(&fixture.b2), shells);
        // And so does the next save: a background session into b1 of B.
        let session = launch(&fixture, Door::SessionNew, false, cx);
        assert_eq!(
            records_on_disk(&fixture.b2),
            shells,
            "b2's records are kept by a save that never read them"
        );
        assert_b_kept_on_disk(&fixture, Door::SessionNew, session);

        let order: Vec<Shows> = shells.iter().map(|id| Shows::Session(*id)).collect();
        fixture.root.update(cx, |root, cx| {
            root.select_workspace(fixture.second, cx);
            assert_eq!(root.active_project, Some(fixture.b2_id));
            let restored: Vec<SessionId> = root
                .visible_tabs()
                .flat_map(|tab| tab.sessions().collect::<Vec<_>>())
                .collect();
            assert_eq!(restored, shells, "every tab the strip names is back");
        });
        assert_eq!(
            shown(&seat_on_disk(&fixture.b2, fixture.second, fixture.first)),
            order,
            "and the visit's save keeps the strip whole"
        );
        assert_eq!(records_on_disk(&fixture.b2), shells);
    }

    /// Every path under `dir`, none when it does not exist.
    fn walkdir(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut next = vec![dir.to_path_buf()];
        while let Some(dir) = next.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                next.push(entry.path());
                found.push(entry.path());
            }
        }
        found
    }

    /// `--front`, and a B already restored: as before GLD-058 — the
    /// first takes the person to B with its tabs, the second leaves them
    /// in A; neither loses a saved tab.
    #[gpui::test]
    fn a_front_launch_and_a_launch_into_a_restored_workspace_are_unchanged(
        cx: &mut TestAppContext,
    ) {
        let Some(home) = isolated(concat!(
            module_path!(),
            "::a_front_launch_and_a_launch_into_a_restored_workspace_are_unchanged"
        )) else {
            return;
        };
        let fixture = restarted_in_a(&home, cx);
        let session = launch(&fixture, Door::SessionNew, true, cx);
        fixture.root.read_with(cx, |root, _| {
            assert_eq!(root.active_workspace, fixture.second);
            let front = root.front_session();
            assert_eq!(front, Some(session), "--front brings the session forward");
        });
        // In front, the session's own tab is its area's front now.
        let mut expected = appended(&fixture.saved[0], &[Shows::Session(session)]);
        expected.active = Some(2);
        if let Some(saved) = expected.areas.as_mut() {
            let focused = saved.areas.focused();
            saved.strip_mut(focused).active = Some(2);
        }
        assert_eq!(
            seat_on_disk(&fixture.b1, fixture.second, fixture.first),
            expected
        );
        drop(fixture);

        let fixture = restarted_in_a(&home, cx);
        let before = fixture.root.update(cx, |root, cx| {
            let before = place(root);
            root.select_workspace(fixture.second, cx);
            root.select_workspace(fixture.first, cx);
            root.active_project = before.1;
            root.active_tab = before.2;
            place(root)
        });
        let session = launch(&fixture, Door::Group, false, cx);
        fixture
            .root
            .read_with(cx, |root, _| assert_eq!(place(root), before));
        assert_b_kept_on_disk(&fixture, Door::Group, session);
        assert_visit_shows_each_once(&fixture, Door::Group, session, cx);
    }
}
