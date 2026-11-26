//! The window's half of a run: starting one, stopping it, and asking
//! what became of it.
//!
//! The model is [`crate::run`] and the console is
//! [`crate::terminal::view::TerminalTab`] — the same terminal a tab has,
//! in a panel. Decision 1 of M8.4 says why the output is a terminal and
//! not lines of our own.
//!
//! **A run belongs to the project, not to the window.** Switching
//! projects leaves it running, the way closing a tab leaves a session
//! running — section 2.3 of the spec. It ends when the project is closed
//! or the window quits, because the emulator owns the pty and its `Drop`
//! kills and reaps.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui::{Context, Entity, Window};
use guild_session::{ProjectId, Tool};

use crate::picker::Take;
use crate::root::picker::{Focus, Holds, Scroll};
use crate::run::discover;
use crate::terminal::view::{Look, TerminalTab};
use crate::watch::Batch;
use crate::{Root, run};
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

#[cfg(test)]
mod tests;

/// What a project declares it can run.
pub(crate) enum Declared {
    /// A thread is finding out; asking again does nothing.
    Asking {
        generation: u64,
        dependencies: std::collections::HashSet<PathBuf>,
    },
    /// What it said, which may be nothing at all.
    Known {
        generation: u64,
        lines: Vec<String>,
        tasks: Vec<run::source::Task>,
        dependencies: std::collections::HashSet<PathBuf>,
    },
}

/// One run of one command, in one project.
pub(crate) struct Run {
    /// The line that was run.
    pub line: String,
    /// The terminal it is drawn in — nothing when it never started.
    pub view: Option<Entity<TerminalTab>>,
    restart: Option<gpui::Task<()>>,
    /// When it started.
    started: Instant,
    /// When it ended, so the clock stops where the run did.
    ended: Option<Instant>,
    /// What has become of it.
    pub state: run::State,
    /// The whole second last drawn, so the window is woken when the
    /// clock changes rather than ten times a second for a number that
    /// changes once.
    shown: u64,
}

impl Run {
    /// How long it has been going, or how long it took.
    pub fn taken(&self) -> Duration {
        self.ended.unwrap_or_else(Instant::now) - self.started
    }
}

impl Root {
    pub(crate) fn select_run(&mut self, project: ProjectId, id: u64, cx: &mut Context<Self>) {
        if let Some(tabs) = self.runs.get_mut(&project) {
            tabs.select(id);
        }
        cx.notify();
    }

    fn close_run_now(&mut self, project: ProjectId, id: u64, cx: &mut Context<Self>) {
        if let Some(entry) = self.runs.get_mut(&project).and_then(|tabs| tabs.remove(id))
            && let Some(view) = entry.value.view
        {
            view.update(cx, |view, cx| view.hang_up_for_quit(cx));
        }
        cx.notify();
    }

    pub(crate) fn close_run_asking(
        &mut self,
        project: ProjectId,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.runs.get(&project).and_then(|tabs| tabs.get(id)) else {
            return;
        };
        if entry.value.state.is_over() {
            self.close_run_now(project, id, cx);
            return;
        }
        let title = format!(
            "{}: {}",
            self.ui().text(crate::i18n::Text::RunClosePrompt),
            entry.value.line
        );
        let answers = [
            gpui::PromptButton::new(self.ui().text(crate::i18n::Text::RunStopAndClose)),
            gpui::PromptButton::cancel(self.ui().text(crate::i18n::Text::Cancel)),
        ];
        let answer = window.prompt(gpui::PromptLevel::Warning, &title, None, &answers, cx);
        cx.spawn(async move |root, cx| {
            if answer.await == Ok(0) {
                let _ = root.update(cx, |root, cx| root.close_run_now(project, id, cx));
            }
        })
        .detach();
    }

    /// The run of the project in front, if it has one.
    pub(crate) fn active_run(&self) -> Option<&Run> {
        self.active_project
            .and_then(|id| self.runs.get(&id))
            .and_then(|tabs| tabs.active())
            .map(|entry| &entry.value)
    }

    /// A typed command captures its project directory at launch.
    pub(crate) fn start_run(&mut self, line: String, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(cwd) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        self.start_invocation(
            project,
            run::tabs::Invocation {
                cwd,
                line,
                task: None,
            },
            cx,
        );
    }

    pub(crate) fn start_invocation(
        &mut self,
        project: ProjectId,
        invocation: run::tabs::Invocation,
        cx: &mut Context<Self>,
    ) {
        let Some(mut command) = run::command(&invocation.line) else {
            return;
        };
        if let Err(error) =
            crate::network::apply_pty(&mut command, guild_session::network::Scope::Tools)
        {
            self.fail(error.to_string());
            return;
        }
        // Rerun may replace a grid only after its supervisor completed. Wait
        // asynchronously and keep one coalesced restart owned by that entry.
        if let Some((id, view)) = self
            .runs
            .get(&project)
            .and_then(|tabs| tabs.find(&invocation))
            .and_then(|entry| entry.value.view.clone().map(|v| (entry.id, v)))
            && view.read(cx).job_completion().is_none()
        {
            view.update(cx, |terminal, cx| terminal.hang_up_for_quit(cx));
            let pending = view.clone();
            let task = cx.spawn(async move |root, cx| {
                loop {
                    let finished = cx.update(|cx| pending.read(cx).job_completion().is_some());
                    if finished {
                        break;
                    }
                    cx.background_executor()
                        .timer(Duration::from_millis(10))
                        .await;
                }
                let _ = root.update(cx, |root, cx| {
                    if root
                        .runs
                        .get(&project)
                        .and_then(|tabs| tabs.get(id))
                        .and_then(|e| e.value.view.as_ref())
                        .is_some_and(|v| v == &pending)
                    {
                        root.start_invocation(project, invocation, cx);
                    }
                });
            });
            if let Some(entry) = self
                .runs
                .get_mut(&project)
                .and_then(|tabs| tabs.entries_mut().iter_mut().find(|entry| entry.id == id))
            {
                entry.value.restart = Some(task);
            }
            return;
        }
        let line = invocation.line.clone();
        let look = Look::of(&self.settings, self.theme);
        let (view, state) =
            match crate::terminal::view::open_job(command, &invocation.cwd, look, cx) {
                Ok(view) => {
                    Self::hear_terminal_input(&view, cx);
                    (Some(view), run::State::Running)
                }
                Err(err) => {
                    // In the window as well as in the panel: a person who
                    // pressed a button and got nothing has no reason to look
                    // at a panel that still says what it said before.
                    let reason = err.to_string();
                    self.fail(self.ui().notice_run_failed(&line, &reason));
                    (None, run::State::Failed { reason })
                }
            };

        // Remembered whatever happened to it: a command that would not
        // start is exactly the one a person wants back to fix a typo in.
        let remembered = if invocation.task.is_some() {
            format!(
                "cd {} && {line}",
                run::source::shell_line(&[invocation.cwd.to_string_lossy().into_owned()])
            )
        } else {
            line.clone()
        };
        self.remember_command(project, &remembered);

        self.runs.entry(project).or_default().insert(
            invocation,
            Run {
                line,
                view,
                restart: None,
                started: Instant::now(),
                ended: None,
                state,
                shown: 0,
            },
        );
        cx.notify();
    }

    /// Run the last command of the project in front, if it has one.
    pub(crate) fn rerun(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(invocation) = self
            .runs
            .get(&project)
            .and_then(|tabs| tabs.active())
            .map(|entry| entry.invocation.clone())
        {
            self.start_invocation(project, invocation, cx);
        } else if let Some(line) = self.last_command() {
            self.start_run(line, cx);
        }
    }

    /// What Run repeats: the run on screen, or the last one this project
    /// was asked to make.
    pub(crate) fn last_command(&self) -> Option<String> {
        let project = self.active_project?;
        if let Some(run) = self.active_run() {
            return Some(run.line.clone());
        }
        self.commands.get(&project)?.last.clone()
    }

    /// Keep a command with its project, in the window and on the disk.
    ///
    /// The map is the cache the panel reads every frame — the shape
    /// `panel_state` has, and for the same reason — and `.guild/` is
    /// where it survives a restart.
    fn remember_command(&mut self, project: ProjectId, line: &str) {
        let commands = self.commands.entry(project).or_default();
        commands.remember(line);
        let commands = commands.clone();

        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let mut state = guild_session::project_state::load(&path);
        state.commands = commands;
        if let Err(err) = guild_session::project_state::save(&path, &state) {
            tracing::warn!(project = %path.display(), %err, "the command was not remembered");
        }
    }

    /// Stop the run in front, and keep what it printed.
    ///
    /// **The terminal is not dropped.** The first version of this dropped
    /// it — the emulator owns the pty and its `Drop` kills and reaps, so
    /// the process did die — and the panel went blank in the same breath:
    /// a person presses Stop while *reading* the output, and taking it
    /// away is a worse answer than not stopping at all. The window
    /// showed it within a minute of the button existing. The process is
    /// killed through the terminal instead, which keeps the grid.
    ///
    /// It says *stopped*, which is ours to know because we are what
    /// stopped it: task 0 measured the alternative, and a killed process
    /// reports exit code 1 — exactly what a command that genuinely failed
    /// with 1 reports.
    pub(crate) fn stop_run(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(entry) = self
            .runs
            .get_mut(&project)
            .and_then(|tabs| tabs.active_mut())
            && entry.value.restart.take().is_some()
        {
            cx.notify();
        }
        let Some(view) = self
            .runs
            .get(&project)
            .and_then(|tabs| tabs.active())
            .map(|entry| &entry.value)
            .and_then(|run| (!run.state.is_over()).then(|| run.view.clone()).flatten())
        else {
            return;
        };
        view.update(cx, |terminal, cx| {
            terminal.stop(cx);
            terminal.hang_up_for_quit(cx);
        });

        let Some(run) = self
            .runs
            .get_mut(&project)
            .and_then(|tabs| tabs.active_mut())
            .map(|entry| &mut entry.value)
        else {
            return;
        };
        run.state = match view.read(cx).job_completion() {
            Some(Err(reason)) => run::State::Failed { reason },
            _ => run::State::Stopped,
        };
        run.ended = Some(Instant::now());
        cx.notify();
    }

    /// Empty the panel's screen and its history, as the terminal's own
    /// `cmd-shift-k` does for a tab.
    pub(crate) fn clear_run(&mut self, cx: &mut Context<Self>) {
        let Some(view) = self.active_run().and_then(|run| run.view.clone()) else {
            return;
        };
        view.update(cx, |terminal, cx| terminal.clear(cx));
        cx.notify();
    }

    /// Ask every live run how it is getting on, on the watch loop's
    /// clock.
    ///
    /// The exit code comes from the child rather than from the pty
    /// closing: the two are different instants, and only one of them
    /// carries a number.
    pub(crate) fn poll_runs(&mut self, cx: &mut Context<Self>) {
        let mut moved = false;
        let mut clock_changed = false;
        let shown_project = self
            .active_project
            .filter(|_| self.tool_is_up(Tool::Output));
        for (project, run) in self.runs.iter_mut().flat_map(|(project, tabs)| {
            tabs.entries_mut()
                .iter_mut()
                .map(move |entry| (*project, &mut entry.value))
        }) {
            if run.state.is_over() {
                continue;
            }
            if let Some(view) = &run.view
                && let Some(result) = view.read(cx).job_completion()
            {
                run.state = match result {
                    Ok(status) => run::State::Finished {
                        code: status.exit_code(),
                    },
                    Err(reason) => run::State::Failed { reason },
                };
                run.ended = Some(Instant::now());
                moved = true;
                continue;
            }
            // The clock, and only when the second it shows has changed:
            // a silent `cargo build` writes nothing for a minute, and
            // the elapsed time still has to count.
            let seconds = run.taken().as_secs();
            if seconds != run.shown {
                run.shown = seconds;
                clock_changed |= shown_project == Some(project);
            }
        }
        if moved {
            cx.notify();
        } else if clock_changed {
            // Elapsed text belongs to Output. Root notification would invalidate
            // every cached region, including unrelated projects and terminals.
            let part = crate::view::region::Part::Dock(self.docks().dock_of(Tool::Output));
            if let Some(region) = self.regions.get(&part) {
                region.update(cx, |_, cx| cx.notify());
            }
        }
    }

    /// Every terminal in the window: the tabs' and the Output panel's.
    ///
    /// One list, because a setting has to reach all of them — a run left
    /// on the old palette after a theme change is the most visible way a
    /// theme can be half applied, and the walk that missed it would be
    /// the walk over tabs alone.
    pub(crate) fn terminals(&self) -> Vec<Entity<TerminalTab>> {
        self.tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::Terminal { view, .. } => Some(view.clone()),
                _ => None,
            })
            .chain(
                self.runs
                    .values()
                    .flat_map(|tabs| tabs.entries())
                    .filter_map(|entry| entry.value.view.clone()),
            )
            .chain(
                self.panel_terminals
                    .values()
                    .flat_map(|tabs| tabs.entries().iter().map(|entry| entry.value.clone())),
            )
            .chain(self.terminal_jobs.views())
            .collect()
    }

    /// Ask what this project declares, once, off the window's thread.
    ///
    /// Called when the Output panel is drawn. Discovery reads source files on
    /// a bounded worker and never runs a project executable.
    pub(crate) fn ensure_declared(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if !self.integrations.external() {
            return;
        }
        if self.declared.contains_key(&project) {
            return;
        }
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        self.discover(project, path, cx);
    }

    /// What the project declares, from the last answer. Empty while it is
    /// being asked for, which is a few milliseconds and reads as a list
    /// that is simply short.
    pub(crate) fn declared_commands(&self, project: ProjectId) -> &[String] {
        match self.declared.get(&project) {
            Some(Declared::Known { lines, .. }) => lines,
            _ => &[],
        }
    }

    pub fn declared_tasks(&self, project: ProjectId) -> &[run::source::Task] {
        match self.declared.get(&project) {
            Some(Declared::Known { tasks, .. }) => tasks,
            _ => &[],
        }
    }

    /// The `justfile` or the `package.json` changed under us — an agent
    /// in the next tab writes them like anything else — so the list is
    /// asked for again.
    pub(crate) fn rediscover(&mut self, project: ProjectId, batch: &Batch, cx: &mut Context<Self>) {
        let dependencies = match self.declared.get(&project) {
            Some(Declared::Asking { dependencies, .. } | Declared::Known { dependencies, .. }) => {
                Some(dependencies)
            }
            None => None,
        };
        let asking = matches!(self.declared.get(&project), Some(Declared::Asking { .. }));
        let declares = batch.files().iter().any(|file| {
            discover::relevant(file)
            || dependencies.is_some_and(|paths| paths.contains(file))
            // A newly found import may change before the in-flight catalogue
            // publishes its dependency set. Supersede that ask as well.
            || asking
        }) || (batch.files().is_empty()
            && batch.directories().iter().any(|directory| {
                dependencies
                    .is_some_and(|paths| paths.iter().any(|path| path.starts_with(directory)))
            }));
        if !declares {
            return;
        }
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        self.discover(project, path, cx);
        let views: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File {
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            view.update(cx, |file, cx| file.discover_tasks(cx));
        }
    }

    /// One ask, on a thread, landing back on the window.
    fn discover(&mut self, project: ProjectId, path: PathBuf, cx: &mut Context<Self>) {
        let generation = match self.declared.get(&project) {
            Some(
                Declared::Asking { generation: g, .. } | Declared::Known { generation: g, .. },
            ) => g + 1,
            None => 1,
        };
        let dependencies = match self.declared.remove(&project) {
            Some(Declared::Asking { dependencies, .. } | Declared::Known { dependencies, .. }) => {
                dependencies
            }
            None => Default::default(),
        };
        self.declared.insert(
            project,
            Declared::Asking {
                generation,
                dependencies,
            },
        );
        cx.spawn(async move |root, cx| {
            let catalogue = cx
                .background_executor()
                .spawn(async move { discover::catalogue(&path) })
                .await;
            root.update(cx, |root, cx| {
                if !matches!(root.declared.get(&project), Some(Declared::Asking { generation: g, .. }) if *g == generation) || root.desk.project(project).is_none() { return; }
                let discover::Catalogue { tasks, dependencies, .. } = catalogue;
                let lines = tasks.iter().map(run::source::Task::chooser_label).collect();
                root.declared.insert(project, Declared::Known { generation, lines, tasks, dependencies });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Kill every process the window owns, before it goes.
    ///
    /// **A run outlived `cmd-q`.** GPUI's quit is `NSApplication
    /// terminate:`, and nothing guarantees an entity is dropped on the
    /// way out — so the `Drop` that owns the pty never ran, and `just
    /// slow` was still counting with no window to show it. The window
    /// showed it the first time the panel could start something long
    /// enough to notice.
    ///
    /// Every terminal, not only the runs: a tab's `claude` has always had
    /// the same hole, and it is worse there — an orphan mid-turn goes on
    /// spending money with nothing to read it. `--resume` is the way back
    /// to that conversation, and it starts a new process anyway.
    pub(crate) fn stop_every_process(&mut self, cx: &mut Context<Self>) {
        self.scoped_settings.stopped = true;
        for tabs in self.runs.values_mut() {
            for entry in tabs.entries_mut() {
                entry.value.restart.take();
            }
        }
        // Drop cancels and joins the clone before application shutdown.
        self.clone_project = None;
        self.terminal_jobs.shutdown(cx);
        // A hangup, as a terminal closing gives, not Stop's SIGKILL: a
        // shell passes it to its jobs, and a job put in the background —
        // `npm run dev &` — no longer outlives Guild. M5.14 review.
        for view in self.terminals() {
            view.update(cx, |terminal, cx| terminal.hang_up_for_quit(cx));
        }
        // The language servers go the same way, and they are the
        // heaviest thing the window starts.
        self.stop_every_server();
        // And every search thread, which is a walk of a tree nobody is
        // waiting for any more.
        self.stop_every_search();
        // And every index thread, and a popup's walk — a thread too. M8.9.
        self.stop_every_index();
        self.picker = None;
        // And the IDE socket, whose `Drop` takes the lock file with it:
        // a lock naming a port nothing listens on is a `claude` that
        // fails to connect and says nothing about why.
        self.ide = None;
    }

    /// Forget a project's run when the project is closed: the emulator's
    /// `Drop` kills the process, so nobody has to find `just check` in
    /// Activity Monitor.
    pub(crate) fn forget_run(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if let Some(tabs) = self.runs.remove(&project) {
            for entry in tabs.entries() {
                if let Some(view) = &entry.value.view {
                    view.update(cx, |view, cx| view.hang_up_for_quit(cx));
                }
            }
        }
    }
}

impl Root {
    /// What the Run action and button do: run the last command, or open
    /// the list when there is not one yet.
    ///
    /// **Never a refusal.** A gesture that returns silently is the shape
    /// of bug `cmd-k` shipped with — the button greyed itself and
    /// explained in a tooltip, and the stroke did neither.
    pub(crate) fn run_or_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // You pressed run: you want to see it.
        self.reveal_tool(Tool::Output, cx);
        match self.last_command() {
            Some(_) => self.rerun(cx),
            None => self.open_run_picker(window, cx),
        }
    }

    /// Open the list, or close it if it is what is open.
    pub(crate) fn toggle_run_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.picker.as_ref().map(|open| &open.holds),
            Some(Holds::Run)
        ) {
            self.close_picker(Focus::Back, window, cx);
            return;
        }
        self.open_run_picker(window, cx);
    }

    /// The list, hanging from the command. Its keys are its own card's —
    /// without the focus a letter typed at it would reach the terminal
    /// behind — and a row is lit from the moment it opens, because `enter`
    /// is the gesture this list exists for.
    fn open_run_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(project) = self.active_project {
            self.ensure_declared(project, cx);
        }
        self.open_picker(
            Holds::Run,
            self.active_project,
            Scroll::children(),
            window,
            cx,
        );
    }

    /// The rows the list is showing: what was typed, what was run here,
    /// what the project declares.
    pub(crate) fn run_rows(&self) -> Vec<run::Row> {
        if !matches!(
            self.picker.as_ref().map(|open| &open.holds),
            Some(Holds::Run)
        ) {
            return Vec::new();
        }
        let Some(project) = self.active_project else {
            return Vec::new();
        };
        let recent = self
            .commands
            .get(&project)
            .map(|commands| commands.recent.clone())
            .unwrap_or_default();
        run::rows(
            self.picker_query(),
            &recent,
            self.declared_commands(project),
        )
    }

    /// `enter`, and a click on a row: run it. The focus goes back to what
    /// the list was opened over — a run opens nothing that would take it.
    pub(crate) fn run_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(line) = self.run_rows().get(row).map(|row| row.line.clone()) else {
            return;
        };
        self.close_picker(Focus::Back, window, cx);
        if let Some(project) = self.active_project
            && let Some(task) = self
                .declared_tasks(project)
                .iter()
                .find(|task| task.chooser_label() == line)
                .cloned()
        {
            self.open_file(&task.identity.source, window, cx);
            if let Some(crate::Body::File { tab, .. }) = self.active_body() {
                let view = tab.clone();
                view.update(cx, |file, cx| {
                    file.activate_task(vec![task], false, window, cx)
                });
            }
            return;
        }
        self.start_run(line, cx);
    }
}

impl Root {
    /// The handlers for the Output panel's run, and clearing a terminal.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_run(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(
            cx.listener(|root, _: &crate::actions::Run, window, cx| root.run_or_choose(window, cx)),
        )
        .on_action(cx.listener(|root, _: &StopRun, _window, cx| root.stop_run(cx)))
        .on_action(cx.listener(|root, _: &ClearRun, _window, cx| root.clear_run(cx)))
        .on_action(cx.listener(|root, _: &ClearTerminal, _window, cx| root.clear_terminal(cx)))
    }
}
