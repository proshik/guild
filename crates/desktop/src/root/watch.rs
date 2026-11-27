//! What the window does with what the disk says: the tree re-reads the
//! directories touched, and an open file tab is told about its file.

use std::collections::HashMap;
use std::path::PathBuf;

use gpui::{Context, Window};
use guild_session::ProjectId;

use crate::watch::{Batch, LATENCY, Watcher};
use crate::{Body, Root};

impl Root {
    /// Start asking the watchers, on the window's clock. Once, from the
    /// first render: applying a batch to a file tab needs the window.
    pub(crate) fn start_watching(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.watching {
            return;
        }
        self.watching = true;
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                loop {
                    let alive = cx.update(|window, cx| {
                        root.update(cx, |root, cx| root.apply_batches(window, cx))
                            .is_ok()
                    });
                    if !matches!(alive, Ok(true)) {
                        return;
                    }
                    cx.background_executor().timer(LATENCY).await;
                }
            })
            .detach();
    }

    pub(super) fn poll_integrations(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.poll_workflows(cx);
        if self.network_check.poll() {
            cx.notify();
        }
        if !self.integrations.external() {
            return;
        }
        #[cfg(test)]
        self.integrations.assert_external_boundary();
        // Git first: its workers answer on this same clock, and so do
        // the diff tabs' threads.
        self.poll_git(cx);
        if self
            .review_activity
            .poll(|| self.terminal_jobs.receipt_snapshot())
        {
            cx.notify();
        }
        if self.terminal_jobs.poll(cx) {
            cx.notify();
        }
        let terminal_error = self.terminal_jobs.error().map(str::to_owned);
        if terminal_error != self.terminal_job_error {
            if let Some(error) = &terminal_error {
                self.warn(error.clone());
            }
            self.terminal_job_error = terminal_error;
            cx.notify();
        }
        self.poll_diffs(cx);
        // The runs are on this clock too: an exit code arrives from the
        // child rather than from the pty closing, and the elapsed time
        // has to count while a silent build says nothing at all.
        self.poll_runs(cx);
        // And the file a `cmd`-click in a terminal left for the window —
        // the same clock, since the click is the tab's and the tab cannot
        // open a file. M5.12.
        self.poll_links(window, cx);
        // And whatever `claude` has asked over the IDE socket: the same
        // clock again, for the same reason — the connections are threads
        // and this is where a thread's word reaches the window.
        self.poll_ide(window, cx);
        // And what the agents are doing: the hooks' socket is threads
        // too, and a status that arrives off this clock is one the tabs
        // never draw. M5.10.
        self.poll_status(cx);
        // And what the sessions' own views know that must outlive them —
        // a terminal's cost, first turn and topic — off the
        // frame, where reading them tied the window to their output.
        self.sync_from_views(cx);
        // And what the search threads have walked into since the last
        // look — the same clock again, and the reason a twelve-second
        // search fills a panel instead of freezing a window.
        self.poll_search(cx);
        // And the file-name and symbol indexes, and a popup's own walk —
        // threads too, M8.9.
        self.poll_indexes(cx);
        self.poll_picker(cx);
        // And what the tree's Kotlin and Java files declare: read on a
        // task, drawn when it lands. M8.13.
        self.read_declarations(cx);
        // And what the build systems said a project depends on. M8.14.
        self.poll_dependencies(cx);
        // And the language servers: their answers arrive on another
        // thread and land here, like git's.
        self.poll_servers(cx);
        // And a project being set up for Xcode, whose server starts when
        // the file is written.
        self.poll_xcode_setups(cx);
        // And what the buffers have changed since the last pause.
        self.flush_documents(cx);
        // And where the caret has got to. There is no caret-moved event
        // to subscribe to, so the place is sampled — on this clock,
        // because a tenth of a second is shorter than a hand and the
        // four-line merge swallows everything faster than that.
        self.note_place(cx);
        // …and which file is in front, for `cmd-e`'s list. The same
        // clock and the same reason: there is no one place a file
        // becomes the front one.
        self.note_front_file();
        // And a quit from outside the window — the Dock, a logout. What
        // AppKit asks is written here and a cancelled quit is picked up
        // here, before the early return below, because this clock runs
        // whether the window paints or not. M5.11's decision 20.
        self.settle_quit(window, cx);
        // And whether `main` has moved past the Guild being worked in
        // (M5.13), whether a newer build is staged, and whether the copy
        // an Update began has answered (M5.15).
        self.poll_update(cx);
        // And the plan's limits, at start and every five minutes while
        // the window is in front (M12.10).
        self.poll_usage(cx);
        // And each worktree's merge request: asked on threads at start,
        // on a write, on coming back and every two minutes while the
        // window is in front, and landed here (GLD-063).
        self.poll_forge(cx);
        // And the start screen's history, read on a thread while the
        // screen is up for a folder (M14.5).
        self.poll_start(cx);
        // And which agents the login `PATH` has, asked on a thread at the
        // start and every ten seconds while the window is in front — with
        // tabs open as well as on the start screen (GLD-081's review).
        self.poll_available(cx);
        // And the Sessions mode's Settled history, read on a thread while
        // the column lists sessions.
        self.poll_settled(cx);
    }

    /// A watcher for every open project and none for a closed one, then
    /// whatever they have to say.
    pub(super) fn apply_batches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A finger can leave the trackpad without AppKit sending `Ended`.
        // The recorded gesture did exactly that, so the detector owns a
        // quiet-time landing which must be driven even when no files move.
        let motion = self
            .workspace_swipe
            .tick(self.workspace_swipe_epoch.elapsed());
        self.apply_workspace_motion(motion, cx);
        // A notice that only informs leaves by itself, and this clock is
        // the one that is always running. 2026-09-22.
        self.sync_notice_hover(window, std::time::Instant::now());
        if self.notices.expire(std::time::Instant::now()) {
            cx.notify();
        }
        // The state writer's thread cannot show a notice; this loop asks
        // it, so a failing disk is still seen rather than only logged.
        if let Some((path, reason)) = guild_session::take_failure() {
            let text = format!(
                "{} ({}: {reason})",
                self.ui().text(crate::i18n::Text::NoticeStateNotSaved),
                path.display()
            );
            self.fail(text);
            cx.notify();
        }
        // The login shell did not say what `PATH` is in time: sessions and
        // servers run with the one the window was started with, and a
        // `claude` that is not on it says so in its tab — this says why,
        // once. Decision 22 of M5.11.
        if let Some(reason) = guild_session::login_path::take_failure() {
            tracing::warn!(%reason, "the login shell's PATH");
            let text = self
                .ui()
                .text(crate::i18n::Text::NoticeLoginPathFailed)
                .to_owned();
            self.warn(text);
            cx.notify();
        }
        // A state file that could not be read was moved aside by the load
        // that met it; the window names where, once per file per run, so
        // that a person can find what a newer build or a crash left.
        // Decision 21 of M5.11.
        for report in guild_session::take_unreadable() {
            let kept = report.kept.file_name().map_or_else(
                || report.kept.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
            let text = self.ui().notice_state_unreadable(
                &report.path.display().to_string(),
                &kept,
                report.from_backup,
            );
            self.warn(text);
            cx.notify();
        }
        // A state file that could not be read at all — too many open
        // files, a failing disk — was left where it is and is not saved
        // over until Guild restarts; the window says so once, with the
        // system's words. 2026-11-07.
        for failure in guild_session::take_read_failures() {
            let text = self
                .ui()
                .notice_state_not_read(&failure.path.display().to_string(), &failure.reason);
            self.fail(text);
            cx.notify();
        }
        self.poll_integrations(window, cx);
        let projects: Vec<(ProjectId, PathBuf)> = self
            .desk
            .projects()
            .map(|project| (project.id, project.path.clone()))
            .collect();
        self.watchers
            .retain(|id, _| projects.iter().any(|(open, _)| open == id));
        for (id, path) in &projects {
            if self.watchers.contains_key(id) {
                continue;
            }
            // Starting one is instant here: the platform's watcher, which
            // takes seconds to arm on macOS, is made on its own thread,
            // and a failure there is logged there, once.
            // A project group's folder is watched for its tree alone.
            let watcher = if self.desk.is_project_group_root(*id) {
                Watcher::start_without_git(path)
            } else {
                Watcher::start(path)
            };
            self.watchers.insert(*id, watcher);
        }

        let mut batches: Vec<(ProjectId, Batch)> = Vec::new();
        for (id, watcher) in &self.watchers {
            while let Some(batch) = watcher.try_recv() {
                batches.push((*id, batch));
            }
        }
        if batches.is_empty() {
            return;
        }
        for (project, batch) in batches {
            self.apply_batch(project, &batch, window, cx);
            if !self.integrations.external() {
                continue;
            }
            // A search whose files moved is an answer gone stale. M5.11.
            self.search_heard(project, &batch);
            // A rewritten file may declare something else now. M8.13.
            self.declarations.forget(project, batch.files());
            // A manifest or a lockfile that changed may mean other
            // dependencies. M8.14.
            self.dependencies_heard(project, &batch);
            // And an index of the names, and the popup that is up. M8.9.
            self.index_batch(project, &batch);
            self.picker_heard(project, &batch);
            // Anything that moved on the disk can move the marks: a
            // write turns clean into modified, and a change under .git
            // is a commit or a stage. One ask per batch, coalesced by
            // the worker. Only a batch that touched `.git` also asks
            // the log again — M8.3.2's decision 4.
            self.ask_git(project, batch.git(), cx);
            // A `justfile` or a `package.json` the agent in the next tab
            // rewrote declares different commands now.
            self.rediscover(project, &batch, cx);
        }
        cx.notify();
    }

    /// One batch: the tree re-reads what it had read of the directories
    /// touched, and each open tab whose file is named decides for
    /// itself — reload, mark, or nothing for a save of its own.
    fn apply_batch(
        &mut self,
        project: ProjectId,
        batch: &Batch,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for state in self
            .trees
            .iter_mut()
            .filter_map(|(seat, state)| (seat.project == project).then_some(state))
        {
            for dir in batch.directories() {
                state.tree.refresh(dir);
            }
            // A directory that gained or lost its only child made or
            // broke a chain, and the selected row's name with it.
            // A transfer completion maps the old selection after the disk move.
            // Its watcher batch may arrive first; do not prune that identity yet.
            if !self
                .file_operations
                .busy
                .as_ref()
                .is_some_and(|pending| pending.project == project)
            {
                state.settle_selection();
            }
        }
        let views: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                // The watcher walks the project's own tree, so its
                // paths are the project's; a dependency's tab with the
                // same relative name is a different file and is not
                // what changed.
                Body::File {
                    path,
                    origin: crate::Origin::Project,
                    tab,
                    ..
                } if batch.files().contains(path) => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            view.update(cx, |file, cx| file.disk_changed(window, cx));
        }
        self.apply_batch_to_diffs(project, batch, cx);
    }
}

/// The map the root keeps: a watcher per open project.
pub(crate) type Watchers = HashMap<ProjectId, Watcher>;
