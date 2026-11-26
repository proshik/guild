//! Opening, adopting and closing a project.

use gpui::{Context, PathPromptOptions, Window};

use crate::Root;
use crate::i18n::Text;
use guild_session::{ProjectId, Released, Seat};
use std::path::Path;
use std::path::PathBuf;
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

impl Root {
    /// One adoption sequence for user-opened and freshly cloned projects.
    /// Refuse a removed workspace before Desk's fallback can choose another.
    pub(crate) fn open_project_in_workspace(
        &mut self,
        workspace: guild_session::WorkspaceId,
        path: &Path,
        cx: &mut Context<Root>,
    ) -> bool {
        self.note_place(cx);
        self.history.suspended += 1;
        let result = self.open_project_in_workspace_without_history(workspace, path, cx);
        self.history.suspended -= 1;
        result
    }

    fn open_project_in_workspace_without_history(
        &mut self,
        workspace: guild_session::WorkspaceId,
        path: &Path,
        cx: &mut Context<Root>,
    ) -> bool {
        if self.desk.workspace(workspace).is_none() || !path.is_dir() {
            return false;
        }
        self.select_workspace(workspace, cx);
        let project = self.desk.open_in(workspace, path);
        self.know_repository(project);
        self.adopt_project(project);
        self.select_project(project, cx);
        self.rebuild_menu(cx);
        true
    }

    /// Create a project: a new directory, then open it.
    ///
    /// Adding a project asks for a directory that exists; this one makes
    /// it. Starting something new otherwise means leaving Guild, creating
    /// a folder somewhere, and coming back — which is the kind of errand
    /// an environment is supposed to remove.
    pub(crate) fn new_project(&mut self, cx: &mut Context<Root>) {
        // Beside the last project rather than in the home directory: a
        // person keeps their work in one place, and that place is where
        // the last one came from.
        let start = self
            .desk
            .recent()
            .next()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| std::env::var("HOME").ok().map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));

        let chosen = cx.prompt_for_new_path(&start, None);

        cx.spawn(async move |root, cx| {
            let Ok(Ok(Some(path))) = chosen.await else {
                return;
            };

            // The panel asks where and under what name; the directory is
            // ours to make. An existing one is not an error: the person
            // may well have pointed at a folder they made a moment ago.
            if let Err(err) = std::fs::create_dir_all(&path) {
                tracing::error!(path = %path.display(), %err, "the project directory was not created");
                let _ = root.update(cx, |root, cx| {
                    let text = root
                        .ui()
                        .notice_not_created(&path.display().to_string(), &err.to_string());
                    root.fail(text);
                    cx.notify();
                });
                return;
            }

            let _ = root.update(cx, |root, cx| {
                root.open_project_in_workspace(root.active_workspace, &path, cx);
                // A project is opened, not a session: nothing is started
                // until the person asks. The empty body tells what an
                // opening does, and a session is opened by hand.
            });
        })
        .detach();
    }

    /// Ask for a directory and create a project.
    pub(crate) fn add_project(&mut self, cx: &mut Context<Root>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });

        cx.spawn(async move |root, cx| {
            // The dialogue is the system's: it does not return until the
            // person answers, so it is awaited in a task rather than in
            // the handler.
            let Ok(Ok(Some(chosen))) = paths.await else {
                return;
            };
            let Some(path) = chosen.into_iter().next() else {
                return;
            };

            let _ = root.update(cx, |root, cx| {
                root.open_project_in_workspace(root.active_workspace, &path, cx);
                // A project is opened, not a session: nothing is started
                // until the person asks, and the empty body says so.
            });
        })
        .detach();
    }

    /// Take on what a project already knows about itself.
    ///
    /// Its sessions live beside it, in `.guild/`, so a project reopened
    /// after being closed — or opened for the first time on this machine —
    /// arrives with the conversations it had. They are re-homed onto the
    /// identifier this run gave the project: the one in the file belongs to
    /// whichever run wrote it, and identifiers are not stable across a
    /// close and a reopen.
    ///
    /// Nothing happens for a project whose sessions are already loaded,
    /// which is every project at startup — `persist::load` did it there.
    pub(crate) fn adopt_project(&mut self, project: ProjectId) {
        if self.desk.sessions_of(project).next().is_some() {
            return;
        }
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };

        let state = guild_session::project_state::load(&path);
        // What it was last asked to run comes back with it, so the Run
        // button means something the moment the project is open.
        self.commands.insert(project, state.commands);

        if state.sessions.is_empty() {
            return;
        }

        tracing::info!(
            project = %path.display(),
            sessions = state.sessions.len(),
            "a reopened project brought its sessions with it"
        );
        self.desk
            .absorb_sessions(state.sessions.into_iter().map(|mut session| {
                session.project = project;
                session
            }));
    }

    pub(crate) fn select_project(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.select_project_without_history(project, cx);
        self.history.suspended -= 1;
    }

    fn select_project_without_history(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        if !self.desk.holders(project).contains(&self.active_workspace) {
            return;
        }
        // A menu built from the project that was active a moment ago must
        // not survive into this one — M12.7 final fix.
        self.close_menu_for_switch();
        self.active_project = Some(project);
        // The row in front is never folded away: whatever brought it
        // forward — `ctrl-tab`, the counter's list, `guild` — opens its
        // project in the column. M12.5 decision 18.
        self.unfold_group_of(project);
        // The branch belongs to the project, so it is stale the moment the
        // project changes; waiting out the timer would show the previous
        // project's branch under the new project's name.
        self.branch_read = None;
        // The active tab becomes one of this project's, not another's.
        self.come_back();
        self.save();
        cx.notify();
    }

    /// Put the focus on a project by its place in the row.
    ///
    /// The row is what a person counts along, so the position is taken
    /// from the same list the row is drawn from. Nothing happens when
    /// there is no project there — pressing cmd-5 with three projects open
    /// is a miss, not a request to do something else.
    pub(crate) fn select_project_at(&mut self, index: usize, cx: &mut Context<Root>) {
        let Some(project) = self
            .column_groups_for(self.active_workspace)
            .iter()
            .flat_map(crate::column::opened_in)
            .nth(index)
        else {
            return;
        };
        if Some(project) == self.active_project {
            return;
        }
        self.select_project(project, cx);
    }

    /// Move to the next project.
    pub(crate) fn step_project(&mut self, forward: bool, cx: &mut Context<Root>) {
        let ids: Vec<ProjectId> = self
            .column_groups_for(self.active_workspace)
            .iter()
            .flat_map(crate::column::opened_in)
            .collect();
        if ids.len() < 2 {
            return;
        }

        let current = self
            .active_project
            .and_then(|id| ids.iter().position(|x| *x == id));
        // **It wraps, and that is what a cycle is.** `ctrl-tab` is held
        // and pressed repeatedly with one hand; a cycle that stops at
        // the end is a cycle a person then has to look at to use, which
        // is the thing the stroke exists to avoid. `pick::step` clamps
        // deliberately, for lists where running off the end means
        // something — this is not one of them.
        let next = match (current, forward) {
            (None, _) => 0,
            (Some(index), true) => (index + 1) % ids.len(),
            (Some(index), false) => (index + ids.len() - 1) % ids.len(),
        };
        self.select_project(ids[next], cx);
    }

    /// Close the active project.
    ///
    /// Asks first, because this is the one action in the menu that throws
    /// something away: the record of the project's sessions goes with it.
    /// What it does **not** touch is the directory or the conversations —
    /// `claude` keeps those — and the prompt says so, because "close" in
    /// an environment usually means nothing is lost.
    pub(crate) fn close_project(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.ask_to_close_project(project, window, cx);
    }

    /// Ask, then close the given project.
    ///
    /// One path for the menu item and for the tab's cross: two would drift,
    /// and the one that drifts would be the one that stops asking.
    pub(crate) fn ask_to_close_project(
        &mut self,
        project: ProjectId,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(name) = self.desk.project(project).map(|p| p.name.clone()) else {
            return;
        };

        // One question at a time, and a project closed under a sheet about
        // one of its tabs would answer that sheet about a tab that is gone.
        if self.asking {
            return;
        }

        // Ask only when there is something to lose. It asked every time,
        // which teaches people to dismiss it without reading — and then it
        // is not there when it matters.
        let ask_live = self.settings.confirm_closing_live_project;
        self.autosave_files(crate::autosave::Trigger::Close, cx);
        let dirty = self.dirty_file_tabs(Some(project), cx);
        if !(ask_live && self.has_live_session(project, cx)) && dirty.is_empty() {
            self.close_project_now(project, cx);
            return;
        }

        // The edited files first, one question each, and the live session
        // after them: a Cancel anywhere leaves the project open. **`asking`
        // is held across both sheets** — the walk would let it go between
        // them, and the menu stays live under a sheet: a `cmd-w` in that gap
        // closed a clean tab of this project, or queued a second question.
        let walk = self.walk_dirty(dirty, true, window, cx);
        let ui = self.ui();
        let title = ui.close_project_title(&name);
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let mut proceed = walk.await;
                // The agents are read after the files are answered, as the
                // tab close reads them: a turn that ended while a person sat
                // on *Save* is not asked about.
                let live = proceed
                    && ask_live
                    && cx
                        .update(|_, cx| {
                            root.read_with(cx, |root, cx| root.has_live_session(project, cx))
                        })
                        .ok()
                        .and_then(Result::ok)
                        .unwrap_or(false);
                if live {
                    let answers = [
                        gpui::PromptButton::new(ui.text(Text::ConfirmClose)),
                        gpui::PromptButton::cancel(ui.text(Text::Cancel)),
                    ];
                    proceed = match cx.update(|window, cx| {
                        window.prompt(gpui::PromptLevel::Warning, &title, None, &answers, cx)
                    }) {
                        // Nothing is closed on silence.
                        Ok(answer) => answer.await == Ok(0),
                        Err(_) => false,
                    };
                }
                let _ = cx.update(|_, cx| {
                    root.update(cx, |root, cx| {
                        root.asking = false;
                        if proceed {
                            root.close_project_now(project, cx);
                        }
                    })
                });
            })
            .detach();
    }

    /// Close a project, with the asking already done.
    ///
    /// Its `.guild/` is left where it is, and that is what makes reopening
    /// bring the sessions back. A person who would rather it did not can
    /// say so — see `forget_sessions_on_close`.
    pub(crate) fn close_project_now(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        self.close_project_in(self.active_workspace, project, cx);
    }

    /// Physical removal releases every holder, including hidden workspaces.
    /// Closing only the front seat leaves another one saving into the tree
    /// while git removes it. Ordinary Close Project remains seat-local.
    pub(crate) fn close_worktree_now(&mut self, project: ProjectId, cx: &mut Context<Root>) {
        for workspace in self.desk.holders(project) {
            // Git owns deleting .guild here. Removing its .gitignore before
            // queued snapshots land can turn our own state into untracked
            // files, and a refusal must leave the last good state intact.
            self.close_project_seat(workspace, project, false, cx);
        }
    }

    /// Close one workspace's seat even if the person switched workspaces
    /// while its confirmation sheet was open.
    pub(crate) fn close_project_in(
        &mut self,
        workspace: guild_session::WorkspaceId,
        project: ProjectId,
        cx: &mut Context<Root>,
    ) {
        self.close_project_seat(
            workspace,
            project,
            self.settings.forget_sessions_on_close,
            cx,
        );
    }

    /// Close one workspace's seat leaving its `.guild/` as it is — a
    /// released group folder's closing (GLD-055), which loses nothing.
    pub(crate) fn close_project_keeping_state(
        &mut self,
        workspace: guild_session::WorkspaceId,
        project: ProjectId,
        cx: &mut Context<Root>,
    ) {
        self.close_project_seat(workspace, project, false, cx);
    }

    fn close_project_seat(
        &mut self,
        workspace: guild_session::WorkspaceId,
        project: ProjectId,
        forget: bool,
        cx: &mut Context<Root>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.close_project_seat_without_history(workspace, project, forget, cx);
        self.history.suspended -= 1;
    }

    fn close_project_seat_without_history(
        &mut self,
        workspace: guild_session::WorkspaceId,
        project: ProjectId,
        forget: bool,
        cx: &mut Context<Root>,
    ) {
        // The project this seat is about is going away — a menu snapshot
        // of it (or of whichever project ends up active once it does)
        // must not outlive it. M12.7 final fix.
        self.close_menu_for_switch();
        let seat = Seat { workspace, project };
        let holders = self.desk.holders(project);
        let released = holders.len() <= 1;
        let path = self
            .desk
            .project(project)
            .map(|project| project.path.clone());
        // The tabs as they stood, so that Recent Projects brings the same
        // ones back. Before the record goes: the list is written per
        // open project, and this one is about to stop being one.
        self.store_tabs();
        self.store_expanded();

        // Forgetting state means this seat, not another workspace's seat.
        // Only the final holder owns the whole directory.
        if forget && let Some(path) = path.as_ref() {
            if released {
                // .guild also holds configured workflows and durable run history.
                // Drain/exclude the ordered writer and remove only view-owned files.
                if let Err(err) = guild_session::project_state::forget_views(path) {
                    tracing::warn!(project = %path.display(), %err, "the project's view state was not forgotten");
                }
            } else {
                let mut state = guild_session::project_state::load(path);
                state.forget_seat(workspace);
                state
                    .sessions
                    .retain(|session| session.workspace != workspace);
                if let Err(err) = guild_session::project_state::save(path, &state) {
                    tracing::warn!(project = %path.display(), %err, "the workspace seat was not forgotten");
                }
            }
        }

        // What belongs to the seat leaves even when the root remains held
        // elsewhere. File entities survive through the other seat's tabs.
        let closing_sessions: Vec<_> = self
            .tabs
            .iter()
            .filter(|tab| tab.seat == seat)
            .flat_map(crate::Tab::sessions)
            .collect();
        self.tabs.retain(|tab| tab.seat != seat);
        self.trees.remove(&seat);
        self.closed.forget_seat(seat);
        self.stage.forget(seat);

        if !released {
            // Never put the deed inside `debug_assert`: release builds erase
            // the whole assertion, arguments included. That made Close and
            // Remove Project look successful in debug window checks while a
            // bundled Guild did not take the root out of its workspace.
            let outcome = self.desk.leave(workspace, project);
            debug_assert_eq!(outcome, Released::Kept);
            self.close_origin_process_views(&closing_sessions, cx);
            if workspace == self.active_workspace {
                self.active_project = self
                    .desk
                    .workspace(self.active_workspace)
                    .and_then(|workspace| workspace.front);
                self.come_back();
            }
            self.save();
            self.rebuild_menu(cx);
            cx.notify();
            return;
        }

        // The run goes with the project, like the watcher: dropping it
        // kills the process, so nobody has to find `just check` in
        // Activity Monitor afterwards.
        self.forget_panel_terminals(project, cx);
        self.forget_run(project, cx);
        // And its search, whose worker's `Drop` stops the walk — a tree
        // nobody is waiting for any more.
        self.forget_file_operations(project);
        self.forget_search(project);
        self.forget_finds(project);
        // And its indexes, and a popup about it — whose walk is of a tree
        // the window no longer has open. M8.9.
        self.forget_indexes(project);
        if self
            .picker
            .as_ref()
            .is_some_and(|open| open.project == Some(project))
        {
            self.picker = None;
        }
        // And what the Problems panel had folded of it: the diagnostics
        // themselves go with the servers, below.
        self.forget_problems(project);
        // And what was waiting to be pushed: up to two hundred commits
        // of a repository the window no longer has open. M8.8.4.
        self.push.forget(project);
        // And its language servers: 3.5 GB apiece is not something to
        // leave behind a closed project.
        self.stop_servers_of(project);
        // The watcher goes with the project, like the pty.
        self.watchers.remove(&project);
        // The tabs went before the record: a live process with no project
        // left is the orphan we have a guard against.
        let outcome = self.desk.leave(workspace, project);
        debug_assert_eq!(outcome, Released::Released);
        self.auto_ff_released(project);
        self.close_origin_process_views(&closing_sessions, cx);
        if workspace == self.active_workspace {
            self.active_project = self
                .desk
                .workspace(self.active_workspace)
                .and_then(|workspace| workspace.front);
            self.come_back();
        }
        self.save();
        self.rebuild_menu(cx);
        cx.notify();
    }
}

impl Root {
    /// The handlers for the project row, the recent list and what closes a project.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_projects(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(
            cx.listener(|root, action: &OpenRecent, _window, cx| {
                root.open_recent(action.index, cx)
            }),
        )
        .on_action(cx.listener(|root, action: &SelectProject, _window, cx| {
            root.select_project_at(action.index, cx)
        }))
        .on_action(cx.listener(|root, _: &ClearRecent, _window, cx| {
            root.desk.clear_recent();
            root.save();
            root.rebuild_menu(cx);
        }))
        .on_action(cx.listener(|root, _: &OpenSelectedApp, _window, cx| root.open_selected_app(cx)))
        .on_action(
            cx.listener(|root, _: &SelectOpenApp, window, cx| root.open_open_in(true, window, cx)),
        )
        .on_action(cx.listener(|root, _: &AddProject, _window, cx| root.add_project(cx)))
        .on_action(cx.listener(|root, _: &NewProject, _window, cx| root.new_project(cx)))
        .on_action(cx.listener(|root, _: &GetFromVersionControl, window, cx| {
            root.open_clone_project(window, cx)
        }))
        .on_action(cx.listener(|root, _: &CloseProject, window, cx| root.close_project(window, cx)))
        .on_action(cx.listener(|root, _: &CopyWorktreePath, window, cx| {
            root.close_context_menu(window, cx);
            root.copy_worktree_path(cx);
        }))
        .on_action(cx.listener(|root, _: &RemoveWorktree, window, cx| {
            root.close_context_menu(window, cx);
            root.remove_active_worktree(window, cx);
        }))
        .on_action(cx.listener(|root, _: &ViewMergeRequest, window, cx| {
            root.close_context_menu(window, cx);
            root.view_active_merge_request(cx);
        }))
        .on_action(cx.listener(|root, _: &NextProject, _window, cx| root.step_project(true, cx)))
        .on_action(
            cx.listener(|root, _: &PreviousProject, _window, cx| root.step_project(false, cx)),
        )
        .on_action(cx.listener(|root, _: &RecentProjects, window, cx| {
            root.toggle_recent_projects(window, cx)
        }))
    }
}

#[cfg(test)]
mod sorting_tests {
    use crate::Root;
    use gpui::{AppContext, TestAppContext};
    use guild_session::{Desk, Runs, SessionMode, SessionRecord, project_sort::Mode};

    #[test]
    fn isolated_sorted_project_navigation() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::projects::sorting_tests::sorted_project_navigation_and_stale_drop",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_SORT_NAV_FIXTURE", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "isolated_sorted_project_navigation owns global persistence paths"]
    fn sorted_project_navigation_and_stale_drop(cx: &mut TestAppContext) {
        assert!(std::env::var_os("GUILD_SORT_NAV_FIXTURE").is_some());
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a");
        let b_path = dir.path().join("b");
        let linked_path = dir.path().join("a-linked");
        for path in [&a_path, &b_path, &linked_path] {
            std::fs::create_dir_all(path).unwrap();
        }
        let mut desk = Desk::default();
        desk.normalise();
        let workspace = desk.active_workspace();
        let a = desk.open_in(workspace, &a_path);
        let b = desk.open_in(workspace, &b_path);
        let linked = desk.open_in(workspace, &linked_path);
        desk.set_repository(a, Some(a_path.join(".git")));
        desk.set_repository(linked, Some(a_path.join(".git")));
        desk.set_repository(b, Some(b_path.join(".git")));
        let session = desk.add_session(SessionRecord::new(
            "fixture".into(),
            (workspace, b),
            b_path.clone(),
            Runs::Claude(SessionMode::Terminal),
        ));
        let reported = |began, ended| guild_session::status::ReportOutcome {
            changed: true,
            began,
            ended,
        };
        desk.observe_reported_activity(session, reported(true, false), 20);
        desk.set_project_sort(workspace, Mode::RecentActivity);
        cx.update(|cx| {
            crate::editor::init(cx);
            let entity = cx.new(|cx| Root::diagnostic(desk, dir.path().join("diagnostic"), cx));
            entity.update(cx, |root, cx| {
                root.select_project_at(0, cx);
                assert_eq!(
                    root.active_project,
                    Some(b),
                    "numeric selection follows displayed groups"
                );
                let linked_session = root.desk.add_session(SessionRecord::new(
                    "linked fixture".into(),
                    (workspace, linked),
                    linked_path.clone(),
                    Runs::Claude(SessionMode::Terminal),
                ));
                // What the program reports without beginning a turn: a tool
                // under way, a question, the turn's end (OSC 7501).
                for (outcome, at) in [
                    (reported(false, false), 30),
                    (reported(false, false), 40),
                    (reported(false, true), 45),
                ] {
                    root.desk
                        .observe_reported_activity(linked_session, outcome, at);
                    assert_eq!(
                        root.column_groups_for(workspace)[0].key,
                        b_path.join(".git"),
                        "{outcome:?} in a linked checkout does not lift its repository"
                    );
                }
                root.step_project(true, cx);
                assert_eq!(root.active_project, Some(a));
                root.step_project(true, cx);
                assert_eq!(
                    root.active_project,
                    Some(linked),
                    "within-group row order is retained"
                );
                root.step_project(true, cx);
                assert_eq!(
                    root.active_project,
                    Some(b),
                    "cycling wraps in displayed order"
                );
                let before = root.desk.workspace(workspace).unwrap().projects.clone();
                root.drop_repository(
                    workspace,
                    &a_path.join(".git"),
                    &[a],
                    crate::column::DropOn::Repository {
                        group: None,
                        itself: false,
                    },
                    Some(b),
                    cx,
                );
                assert_eq!(
                    root.desk.workspace(workspace).unwrap().projects,
                    before,
                    "a stale payload cannot move half a group"
                );
                assert_eq!(
                    root.desk.workspace(workspace).unwrap().project_sort,
                    Mode::RecentActivity
                );
                root.desk
                    .observe_reported_activity(linked_session, reported(true, false), 50);
                assert_eq!(
                    root.column_groups_for(workspace)[0].key,
                    a_path.join(".git"),
                    "a turn started in a linked checkout lifts its repository"
                );
                root.select_project_at(0, cx);
                assert_eq!(
                    root.active_project,
                    Some(a),
                    "numeric selection follows the new order"
                );
            });
        });
    }
}
