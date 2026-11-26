//! Files in tabs: opening one from the tree, keeping the tree on the one
//! in front, saving, reloading, and the question a dirty tab asks before
//! it goes.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{Context, Entity, Focusable, PromptButton, Task, Window};
use guild_session::{PaneId, ProjectId};

use crate::closed::ClosedTab;
use crate::file_tab::{FileTab, Look};
use crate::i18n::Text;
use crate::{Body, Root, Tab, TabId};
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;

/// What the unsaved-changes sheet answered, by button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Answer {
    Save,
    DontSave,
    Cancel,
}

impl Answer {
    /// The sheet's buttons by position. A sheet that went away without
    /// an answer — the window closed under it — is a Cancel: nothing is
    /// closed on silence.
    pub(crate) fn of<E>(answer: Result<usize, E>) -> Self {
        match answer {
            Ok(0) => Self::Save,
            Ok(1) => Self::DontSave,
            _ => Self::Cancel,
        }
    }

    /// Whether the tab may go — saving first if that is the answer. A
    /// save that failed is a Cancel: the tab stays, dirty, and whoever
    /// ran the save has shown the failure.
    pub(crate) fn may_proceed(self, save: impl FnOnce() -> Result<(), String>) -> bool {
        match self {
            Self::Save => save().is_ok(),
            Self::DontSave => true,
            Self::Cancel => false,
        }
    }
}

/// What a close of the window does, from whether a question is already
/// on the screen and what is at stake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Closing {
    /// Nothing is lost: close now.
    Now,
    /// Something is at stake and nothing is being asked: ask.
    Ask,
    /// Something is at stake and a question is already up: that question
    /// is the answer, and a second sheet queued behind it would be asked
    /// about a window the first one may already have closed.
    AlreadyAsked,
}

impl Closing {
    pub(crate) fn of(asking: bool, dirty: bool, running: bool) -> Self {
        match (dirty || running, asking) {
            (false, _) => Self::Now,
            (true, false) => Self::Ask,
            (true, true) => Self::AlreadyAsked,
        }
    }
}

/// What a close asks about and then closes — M12.8: a whole tab, or one
/// pane of a divided one (decision 14). One walk takes both, so a pane is
/// asked about exactly as a tab is, and the walk stays one walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CloseTarget {
    /// A tab, with every pane it has.
    Tab(TabId),
    /// One pane of a tab, and what it shows.
    Pane(TabId, PaneId),
}

impl CloseTarget {
    /// The tab it is, or is in.
    pub(crate) fn tab(self) -> TabId {
        match self {
            Self::Tab(tab) | Self::Pane(tab, _) => tab,
        }
    }
}

impl Root {
    /// The file in front in a tab — its focused pane's — if it is one:
    /// what `cmd-s` and *Reload from Disk* act on.
    fn file_view(&self, tab: TabId) -> Option<Entity<FileTab>> {
        self.tabs
            .iter()
            .find(|open| open.id == tab)
            .and_then(Tab::body)
            .and_then(|body| match body {
                Body::File { tab, .. } => Some(tab.clone()),
                _ => None,
            })
    }

    /// Every file a tab holds, in its panes' draw order — what a close
    /// asks about, because closing a tab closes every pane.
    fn file_views(&self, tab: TabId) -> Vec<Entity<FileTab>> {
        self.tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| {
                open.bodies()
                    .filter_map(|body| match body {
                        Body::File { tab, .. } => Some(tab.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every file a close of `target` would take away: the tab's, or the
    /// one pane's.
    fn target_views(&self, target: CloseTarget) -> Vec<Entity<FileTab>> {
        match target {
            CloseTarget::Tab(tab) => self.file_views(tab),
            CloseTarget::Pane(tab, pane) => self
                .tabs
                .iter()
                .find(|open| open.id == tab)
                .and_then(|open| open.panes.body_of(pane))
                .and_then(|body| match body {
                    Body::File { tab, .. } => Some(tab.clone()),
                    _ => None,
                })
                .into_iter()
                .collect(),
        }
    }

    /// Whether `target` is what has the keys: the tab in front, or its
    /// focused pane — what a close must put the keys back after.
    pub(crate) fn holds_the_keys(&self, target: CloseTarget) -> bool {
        match target {
            CloseTarget::Tab(tab) => self.active_tab == Some(tab),
            CloseTarget::Pane(tab, pane) => {
                self.active_tab == Some(tab)
                    && self
                        .tabs
                        .iter()
                        .any(|open| open.id == tab && open.panes.focused() == pane)
            }
        }
    }

    /// Whether any pane of the tab holds unsaved text.
    fn holds_dirty(&self, tab: &Tab, cx: &gpui::App) -> bool {
        tab.bodies().any(|body| match body {
            Body::File { tab, .. } => tab.read(cx).is_dirty(),
            _ => false,
        })
    }

    /// HEAD/ref refreshes invalidate only project source baselines, never an
    /// external library with the same relative path. Ordinary disk-only status
    /// polling leaves the cached immutable HEAD snapshot alone.
    pub(crate) fn refresh_source_git_baselines(&self, project: ProjectId, cx: &mut Context<Self>) {
        for body in self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
        {
            if let Body::File {
                origin: crate::Origin::Project,
                tab,
                ..
            } = body
            {
                tab.update(cx, |file, cx| file.refresh_git_baseline(cx));
            }
        }
    }

    /// Give every file tab the chance to write itself, for this reason.
    /// What the table refuses stays dirty, and the callers that ask — a
    /// close, a quit, a commit — ask only about those.
    pub(crate) fn autosave_files(
        &mut self,
        trigger: crate::autosave::Trigger,
        cx: &mut Context<Self>,
    ) {
        let views: Vec<Entity<FileTab>> = self
            .tabs
            .iter()
            .flat_map(Tab::bodies)
            .filter_map(|body| match body {
                Body::File { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            view.update(cx, |file, cx| {
                file.autosave(trigger, cx);
            });
        }
    }

    /// The tabs with unsaved text — of one project, or of all of them —
    /// in the strip's order.
    pub(crate) fn dirty_file_tabs(
        &self,
        project: Option<ProjectId>,
        cx: &Context<Self>,
    ) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|tab| project.is_none_or(|project| tab.project == project))
            .filter(|tab| self.holds_dirty(tab, cx))
            .map(|tab| tab.id)
            .collect()
    }

    /// Unsaved files whose tabs belong to one workspace. A file view can
    /// be shared by seats, but the question belongs to the arrangement that
    /// is about to disappear.
    pub(crate) fn dirty_workspace_tabs(
        &self,
        workspace: guild_session::WorkspaceId,
        cx: &Context<Self>,
    ) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|tab| tab.seat.workspace == workspace)
            .filter(|tab| self.holds_dirty(tab, cx))
            .map(|tab| tab.id)
            .collect()
    }

    /// Unsaved files belonging to exactly one workspace-local project seat.
    pub(crate) fn dirty_seat_tabs(
        &self,
        seat: guild_session::Seat,
        cx: &Context<Self>,
    ) -> Vec<TabId> {
        self.tabs
            .iter()
            .filter(|tab| tab.seat == seat)
            .filter(|tab| self.holds_dirty(tab, cx))
            .map(|tab| tab.id)
            .collect()
    }

    /// Write the file in front in one tab to disk. A failure is a notice
    /// in the OS's words, and the tab stays dirty.
    pub(crate) fn save_file(&mut self, tab: TabId, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(view) = self.file_view(tab) else {
            return Ok(());
        };
        self.save_view(view, cx)
    }

    /// Write one file to disk, whichever pane shows it — [`Root::save_file`]
    /// once the file is found.
    pub(crate) fn save_view(
        &mut self,
        view: Entity<FileTab>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let saved = view.update(cx, |file, cx| file.save(cx));
        if let Err(reason) = &saved {
            let name = view.read(cx).name();
            let text = self.ui().save_failed(&name, reason);
            tracing::warn!(file = %name, %reason, "the file was not saved");
            self.fail(text);
            cx.notify();
        }
        saved
    }

    /// `cmd-s`: the tab in front, when it is a file.
    pub(crate) fn save_active_file(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.active_tab {
            let _ = self.save_file(tab, cx);
        }
    }

    /// `cmd-alt-s`: every dirty file, each failure its own notice.
    pub(crate) fn save_all_files(&mut self, cx: &mut Context<Self>) {
        for tab in self.dirty_file_tabs(None, cx) {
            for view in self.file_views(tab) {
                if view.read(cx).is_dirty() {
                    let _ = self.save_view(view, cx);
                }
            }
        }
    }

    /// *Reload from Disk*: read the file in front again. A dirty tab is
    /// asked the same question as a closing one, because the same text
    /// is at stake.
    pub(crate) fn reload_active_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.active_tab else {
            return;
        };
        let Some(view) = self.file_view(tab) else {
            return;
        };
        if !view.read(cx).is_dirty() {
            view.update(cx, |file, cx| file.reload(window, cx));
            return;
        }
        // The one file the focused pane shows: the tab's other panes are
        // not reloaded, so they are not asked about either — M12.8.
        let walk = self.walk_dirty_views(vec![view.clone()], false, window, cx);
        window
            .spawn(cx, async move |cx| {
                if walk.await {
                    let _ =
                        cx.update(|window, cx| view.update(cx, |file, cx| file.reload(window, cx)));
                }
            })
            .detach();
    }

    /// Ask about each dirty tab in turn, through the platform's own
    /// sheet — *Save*, *Don't Save*, *Cancel* — and answer whether what
    /// the caller meant to do may go ahead. *Save* saves here and now; a
    /// *Cancel*, or a save that failed, stops the walk where it is and
    /// answers no. The tabs themselves are left to the caller: what
    /// happens to them after a yes differs between closing a tab, a
    /// project and the window.
    pub(crate) fn ask_dirty_tabs(
        &mut self,
        tabs: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        self.walk_dirty(tabs, false, window, cx)
    }

    /// The walk behind [`Root::ask_dirty_tabs`]. **`hold` keeps `asking`
    /// set when the walk ends**, and sets it even with nothing to ask: a
    /// close that has a second question after the files — the agents at
    /// work — must not leave a gap between the two sheets, because the menu
    /// stays live under a sheet and a second `cmd-w` in that gap would queue
    /// a question about a tab that has gone. Whoever holds lets go — the tab
    /// close and the project close both do.
    pub(crate) fn walk_dirty(
        &mut self,
        tabs: Vec<TabId>,
        hold: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        // Every dirty file the tabs hold, whichever pane shows it: a tab
        // closes with all its panes — M12.8.
        let views = tabs
            .into_iter()
            .flat_map(|tab| self.file_views(tab))
            .collect();
        self.walk_dirty_views(views, hold, window, cx)
    }

    /// [`Root::walk_dirty`] over the files themselves rather than the tabs
    /// that hold them — a reload asks about the one file it reloads, not
    /// about every file of a divided tab (M12.8).
    fn walk_dirty_views(
        &mut self,
        views: Vec<Entity<FileTab>>,
        hold: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Task<bool> {
        let ui = self.ui();
        let named: Vec<(Entity<FileTab>, String)> = views
            .into_iter()
            .filter(|view| view.read(cx).is_dirty())
            .map(|view| {
                let name = view.read(cx).name();
                (view, name)
            })
            .collect();
        // One question at a time. The menu stays live under a sheet, so
        // a second cmd-w would otherwise queue a second sheet for the
        // same tab — and answer it about a tab that had already gone.
        if self.asking {
            return Task::ready(false);
        }
        self.asking = hold || !named.is_empty();
        let root = cx.weak_entity();
        window.spawn(cx, async move |cx| {
            let mut proceed = true;
            for (view, name) in named {
                let title = ui.unsaved_title(&name);
                // Typed rather than plain strings: the platform binds
                // Escape to a button it knows as Cancel, and it knows the
                // English word only.
                let answers = [
                    PromptButton::new(ui.text(Text::Save)),
                    PromptButton::new(ui.text(Text::DontSave)),
                    PromptButton::cancel(ui.text(Text::Cancel)),
                ];
                let Ok(asked) = cx.update(|window, cx| {
                    crate::system::close_prompt::prompt(
                        window,
                        &title,
                        Some(ui.text(Text::UnsavedDetail)),
                        &answers,
                        cx,
                    )
                }) else {
                    proceed = false;
                    break;
                };
                let answer = Answer::of(asked.await);
                let root = &root;
                proceed = answer.may_proceed(|| {
                    let view = view.clone();
                    match cx.update(|_, cx| root.update(cx, |root, cx| root.save_view(view, cx))) {
                        Ok(Ok(saved)) => saved,
                        Ok(Err(gone)) => Err(gone.to_string()),
                        Err(gone) => Err(gone.to_string()),
                    }
                });
                if !proceed {
                    break;
                }
            }
            if !hold {
                let _ = cx.update(|_, cx| root.update(cx, |root, _| root.asking = false));
            }
            proceed
        })
    }

    /// Close a tab, asking first if it holds unsaved text or an agent at
    /// work. One path for `cmd-w`, the strip's cross and the menu: two
    /// would drift, and the one that drifts would be the one that stops
    /// asking.
    pub(crate) fn close_tab_asking(
        &mut self,
        tab: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_asking(vec![CloseTarget::Tab(tab)], window, cx);
    }

    /// Close one pane of `tab`, asking first as a tab's close asks — its
    /// unsaved file, its agent at work — M12.8, decision 14. A tab of one
    /// pane is closed as a tab: its pane is all of it.
    pub(crate) fn close_pane_asking(
        &mut self,
        tab: TabId,
        pane: PaneId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let divided = self
            .tabs
            .iter()
            .any(|open| open.id == tab && open.panes.ids().len() > 1);
        let target = if divided {
            CloseTarget::Pane(tab, pane)
        } else {
            CloseTarget::Tab(tab)
        };
        self.close_asking(vec![target], window, cx);
    }

    /// Close several tabs, asking about each dirty one in turn and then
    /// once about every agent at work among them.
    ///
    /// The menu's *close the others* and *close all*, and the same walk
    /// the window's own close uses: one question at a time, and a Cancel
    /// anywhere leaves **every** tab open — half-closing a set because
    /// the third question was cancelled is worse than either answer.
    pub(crate) fn close_tabs_asking(
        &mut self,
        tabs: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_asking(tabs.into_iter().map(CloseTarget::Tab).collect(), window, cx);
    }

    /// The one walk behind both doors — M5.12, decisions 1 and 3.
    ///
    /// **Refused at once while a question is up**; then the dirty files,
    /// one sheet each; then one sheet naming every tab at stake — an agent
    /// working or waiting while `confirm_closing_live_project` is on, any
    /// open agent's tab while `confirm_closing_agent_tab` is — in the
    /// project close's shape; then the tabs close. `asking` is held across
    /// both questions and let go at the end, whatever the answers.
    ///
    /// **The agents are read after the files are answered**, not when the
    /// gesture began: a person can sit on an unsaved-file sheet while a turn
    /// ends, and a sheet about a turn that is over is a question about
    /// nothing.
    ///
    /// **A pane is asked about as a tab is** — M12.8: its own file, its own
    /// agent, and nothing of the panes beside it, which stay.
    fn close_asking(
        &mut self,
        tabs: Vec<CloseTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.asking {
            return;
        }
        let dirty: Vec<Entity<FileTab>> = tabs
            .iter()
            .flat_map(|target| self.target_views(*target))
            .collect();
        let any_dirty = dirty.iter().any(|view| view.read(cx).is_dirty());
        // Whether any is at stake, without naming them: the names are built
        // for the sheet, and this runs on every `cmd-w`. Both settings are
        // read inside `target_stake`.
        let staked = tabs
            .iter()
            .any(|target| self.target_stake(*target, cx).is_some());
        // **The keyboard follows the tab in front when that one goes** —
        // 2026-09-24. A closed tab's editor kept the focus, drawn nowhere;
        // the path from it reached no handler of ours, so `cmd-w` went
        // unanswered, and the window's focus-lost rule did not fire
        // because the path was not empty. Measured on a fixture: every
        // second or third `cmd-w` came back `action=None`.
        // A pane's close moves the keys only when they were in it.
        let front = tabs.iter().any(|target| self.holds_the_keys(*target));
        if !any_dirty && !staked {
            for target in tabs {
                self.close_target(target, cx);
            }
            if front {
                self.refocus(window, cx);
            }
            return;
        }
        let walk = self.walk_dirty_views(dirty, true, window, cx);
        let ui = self.ui();
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                let mut proceed = walk.await;
                if proceed {
                    let sheet = cx
                        .update(|_, cx| {
                            root.read_with(cx, |root, cx| {
                                crate::closing::sheet(ui, &root.staked_targets(&tabs, cx))
                            })
                        })
                        .ok()
                        .and_then(Result::ok)
                        .flatten();
                    if let Some(title) = sheet {
                        let answers = [
                            PromptButton::new(ui.text(Text::ConfirmClose)),
                            PromptButton::cancel(ui.text(Text::Cancel)),
                        ];
                        proceed = match cx.update(|window, cx| {
                            crate::system::close_prompt::prompt(window, &title, None, &answers, cx)
                        }) {
                            // A sheet that could not be put up, or went
                            // away unanswered, is a no: nothing is closed
                            // on silence.
                            Ok(answer) => answer.await == Ok(0),
                            Err(_) => false,
                        };
                    }
                }
                let _ = cx.update(|window, cx| {
                    root.update(cx, |root, cx| {
                        root.asking = false;
                        if proceed {
                            for target in tabs {
                                root.close_target(target, cx);
                            }
                            if front {
                                root.refocus(window, cx);
                            }
                        }
                        // Resolve a selector's close intent on completion, before
                        // another gesture can close the same file independently.
                        if root.recent_files_poll() {
                            root.settle_picker();
                            cx.notify();
                        }
                    })
                });
            })
            .detach();
    }

    /// Whether the window may close right now. When something is at
    /// stake it asks — each dirty file, then the running sessions — and
    /// quits on the answers; the window is not closed in advance of them.
    /// Public because the binary installs it as the window's close
    /// handler, like `running_sessions` before it.
    pub fn may_close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.may_quit_then(window, cx, crate::quit)
    }

    /// [`Root::may_close_window`], with what follows a yes given by the
    /// caller: `crate::quit` for `cmd-q` and the close button, the swap's
    /// preparation for Update — M5.15, which must not leave a helper
    /// waiting for a quit the person cancelled. `then` runs only after
    /// questions that were asked; with nothing at stake the answer is
    /// `true` and the caller goes on itself, as the close handler always
    /// has.
    pub(crate) fn may_quit_then(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut gpui::App) + 'static,
    ) -> bool {
        self.autosave_files(crate::autosave::Trigger::Close, cx);
        let dirty = self.dirty_file_tabs(None, cx);
        let running = self.running_sessions();
        match Closing::of(self.asking, !dirty.is_empty(), running > 0) {
            Closing::Now => return true,
            Closing::AlreadyAsked => return false,
            Closing::Ask => {}
        }
        let ui = self.ui();
        // The sessions question is a question too, and holds `asking`
        // like the unsaved-file ones: the menu stays live under its sheet,
        // and a second `cmd-q` or a click on the close button would queue
        // a second one. With no dirty tab it is the only question, so it
        // takes the flag here, before its sheet is up. With dirty tabs the
        // walk holds the flag and lets it go when it ends, and the
        // sessions question takes it back after — refusing itself if
        // another question got in between.
        let first = dirty.is_empty();
        let walk = if first {
            self.asking = true;
            Task::ready(true)
        } else {
            self.ask_dirty_tabs(dirty, window, cx)
        };
        let root = cx.weak_entity();
        window
            .spawn(cx, async move |cx| {
                if !walk.await {
                    return;
                }
                if running > 0 {
                    if !first {
                        let taken = cx.update(|_, cx| {
                            root.update(cx, |root, _| !std::mem::replace(&mut root.asking, true))
                        });
                        if !matches!(taken, Ok(Ok(true))) {
                            return;
                        }
                    }
                    // Quitting mid-turn loses work. The conversations are
                    // kept, so this is a warning rather than a refusal.
                    let answers = [
                        PromptButton::new(ui.text(Text::QuitAnyway)),
                        PromptButton::cancel(ui.text(Text::Cancel)),
                    ];
                    let asked = cx.update(|window, cx| {
                        crate::system::close_prompt::prompt(
                            window,
                            ui.text(Text::QuitTitle),
                            None,
                            &answers,
                            cx,
                        )
                    });
                    // A sheet that could not be put up, or went away
                    // unanswered, is a no.
                    let asked = match asked {
                        Ok(answer) => answer.await == Ok(0),
                        Err(_) => false,
                    };
                    let _ = cx.update(|_, cx| root.update(cx, |root, _| root.asking = false));
                    if !asked {
                        return;
                    }
                }
                let _ = cx.update(|_, cx| then(cx));
            })
            .detach();
        false
    }

    /// Open a file of the active project in a tab — or go to the tab
    /// that already shows it. One tab per file: a second click on the
    /// same row is a way back to it, not a second copy.
    pub(crate) fn open_file(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file_from(crate::Origin::Project, relative, window, cx);
    }

    /// The same, for a file under one of the project's external roots —
    /// where `f12` lands when a definition is in a dependency. M8.8.3.
    ///
    /// **The path is still relative to something**, which is the whole
    /// point of `Origin`: an absolute path smuggled into `Body::File`
    /// for this one case would make the two kinds of tab different in
    /// every place that reads one.
    pub(crate) fn open_file_from(
        &mut self,
        origin: crate::Origin,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.open_file_from_without_history(origin, relative, window, cx);
        self.history.suspended -= 1;
        // An actual open/reopen counts immediately, even before the next watch tick.
        self.note_front_file();
    }

    /// Where a file of a project group's folder opens when a member holds
    /// it: that member's root and the path in it. `None` for a root that is
    /// no group's folder, and for a file outside every member, which stays
    /// the folder's.
    pub(crate) fn member_file(
        &self,
        project: ProjectId,
        relative: &Path,
    ) -> Option<(ProjectId, PathBuf)> {
        if !self.desk.is_project_group_root(project) {
            return None;
        }
        let path = self.desk.project(project)?.path.join(relative);
        crate::column::owning_root(self.group_roots_of(project), &path)
            .filter(|(owner, _)| *owner != project)
    }

    /// The roots a file of a project group's folder may belong to: the
    /// folder's own, and its group's members' — never another project that
    /// happens to sit in the folder (decision 11).
    fn group_roots_of(&self, folder: ProjectId) -> Vec<(ProjectId, &Path)> {
        let workspace = self.active_workspace;
        let Some(holder) = self.desk.workspace(workspace) else {
            return Vec::new();
        };
        let Some(path) = self.desk.project(folder).map(|found| found.path.as_path()) else {
            return Vec::new();
        };
        let Some(group) = holder.project_group_of_folder(path) else {
            return Vec::new();
        };
        self.desk
            .projects_of(workspace)
            .filter(|found| {
                found.id == folder
                    || group
                        .repositories
                        .iter()
                        .any(|key| key == guild_session::project_group::repository_key(found))
            })
            .map(|found| (found.id, found.path.as_path()))
            .collect()
    }

    fn open_file_from_without_history(
        &mut self,
        origin: crate::Origin,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        // **A member's file opens in the member** — GLD-055 decision 11. A
        // project group's folder holds its members as subdirectories, and a
        // file clicked there belongs to the member's root, where its
        // language server, git markers and go-to-definition are: the member
        // comes to the front and the file opens in it, as from its own
        // Explorer. A file outside every member stays the folder's.
        if origin == crate::Origin::Project
            && let Some((owner, inner)) = self.member_file(project, relative)
        {
            self.select_project(owner, cx);
            if self.active_project == Some(owner) {
                self.open_file_from_without_history(origin, &inner, window, cx);
            }
            return;
        }
        if origin == crate::Origin::Project
            && self.file_operations.busy.as_ref().is_some_and(|pending| {
                pending.project == project
                    && pending
                        .sources
                        .iter()
                        .any(|source| relative.starts_with(source))
            })
        {
            return;
        }
        let Some(root) = self.root_of(project, origin) else {
            return;
        };
        // Import watches must exist for editor controls even when Output has
        // never been opened. External library files do not own project tasks.
        if origin == crate::Origin::Project && crate::run::source::Runner::of(relative).is_some() {
            self.ensure_declared(project, cx);
        }

        // **The origin is part of a tab's identity.** Two dependencies
        // both have a `src/lib.rs`, and so does the project; a lookup by
        // path alone would answer with somebody else's file.
        let seat = self.seat_for_project(project);
        // In every area of the seat (decision 15): a file open in the
        // other half of the screen is brought forward, not opened twice.
        let existing = self.find_showing(seat, |body| {
            matches!(
                body,
                Body::File { path, origin: theirs, .. }
                    if path == relative && *theirs == origin
            )
        });
        if let Some((tab, pane)) = existing {
            self.active_tab = Some(tab);
            if let Some(focus) = self
                .focus_pane(tab, pane, cx)
                .and_then(|body| body.focus_handle(cx))
            {
                window.focus(&focus, cx);
            }
            // The one in front is part of the state: a restart shows it.
            self.save();
            cx.notify();
            return;
        }

        // A file buffer is root-owned: another workspace gets its own tab
        // identity and strip position, but both tabs point at one entity.
        let shared = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
            .find_map(|body| match body {
                Body::File {
                    path,
                    tab,
                    origin: theirs,
                    ..
                } if path == relative && *theirs == origin => Some(tab.clone()),
                _ => None,
            });
        if let Some(view) = shared {
            let presentation = Self::markdown_presentation(
                &view,
                self.settings.markdown_default_view,
                &self.diagrams,
                cx,
            );
            let focus = presentation.as_ref().map_or_else(
                || view.read(cx).focus_handle(cx),
                |p| p.view.read(cx).focus_handle(cx),
            );
            let id = self.adopt(
                seat,
                project,
                Body::File {
                    path: relative.to_path_buf(),
                    tab: view,
                    presentation,
                    origin,
                },
                cx,
            );
            self.active_tab = Some(id);
            window.focus(&focus, cx);
            self.save();
            cx.notify();
            return;
        }

        // The language server of this file's language, started here and
        // nowhere else: a project whose files are never opened never
        // starts one, which is the whole of M8.5's decision 4. A file
        // under a dependency's root starts nothing — a server is the
        // project's, and this file is not in it.
        if origin == crate::Origin::Project
            && let Some(language) = crate::highlight::Language::of(relative)
        {
            self.ensure_server(project, language, cx);
        }

        let look = self.file_look();
        let ui = self.ui();
        let relative = relative.to_path_buf();
        let path_for_diffs = relative.clone();
        let without_git = self.desk.is_project_group_root(project);
        let view = cx.new(|cx| {
            FileTab::open(&root, &relative, look, ui, window, cx)
                .read_only(origin != crate::Origin::Project)
                .without_git(without_git)
        });
        // **Only the project's files are told to a server or to a
        // diff.** A URI is built from the project root and a relative
        // path, and a diff asks git about a path inside the repository;
        // for a file under a dependency's root both would name something
        // else entirely — and a plausible wrong answer is worse than
        // none. M8.8.3.
        if origin == crate::Origin::Project
            && let Some(editor) = view.read(cx).editor().cloned()
        {
            self.open_document(project, &relative, &editor, cx);
        }
        let presentation = Self::markdown_presentation(
            &view,
            self.settings.markdown_default_view,
            &self.diagrams,
            cx,
        );
        self.hear_file_runs(
            project,
            &view,
            (origin == crate::Origin::Project).then_some(relative.as_path()),
            cx,
        );
        let focus = presentation.as_ref().map_or_else(
            || view.read(cx).focus_handle(cx),
            |p| p.view.read(cx).focus_handle(cx),
        );
        let id = self.adopt(
            seat,
            project,
            Body::File {
                path: relative,
                tab: view,
                presentation,
                origin,
            },
            cx,
        );
        self.active_tab = Some(id);
        window.focus(&focus, cx);
        if origin == crate::Origin::Project {
            self.follow_in_diffs(project, &path_for_diffs, cx);
        }
        self.save();
        cx.notify();
    }

    /// A file tab brought back after a restart: pushed where it was,
    /// not brought to the front. Its editor is made when it is first
    /// drawn — at this point there is no window to make it in.
    pub(crate) fn restore_file_tab(
        &mut self,
        project: ProjectId,
        root: &Path,
        relative: PathBuf,
        cx: &mut Context<Self>,
    ) -> TabId {
        // Restored project sources need import watches before Output opens,
        // just like newly opened sources. Editor setup remains deferred.
        if crate::run::source::Runner::of(&relative).is_some() {
            self.ensure_declared(project, cx);
        }
        let shared = self
            .tabs
            .iter()
            .flat_map(|open| open.bodies().map(move |body| (open.project, body)))
            .find_map(|(theirs, body)| match body {
                Body::File {
                    path,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } if view.read(cx).path() == root.join(&relative)
                    || (theirs == project && path == &relative) =>
                {
                    Some(view.clone())
                }
                _ => None,
            });
        let view = shared.unwrap_or_else(|| {
            let look = self.file_look();
            let ui = self.ui();
            let without_git = self.desk.is_project_group_root(project);
            cx.new(|cx| FileTab::restore(root, &relative, look, ui, cx).without_git(without_git))
        });
        let presentation = Self::markdown_presentation(
            &view,
            self.settings.markdown_default_view,
            &self.diagrams,
            cx,
        );
        self.hear_file_runs(project, &view, Some(&relative), cx);
        // **No server is started here.** A restored tab wants one — M8.5
        // met a window reopened on its file saying *no server* for ever —
        // but it wants it when it is first drawn, and `ensure_documents`
        // on the render starts it then, for every file tab whose editor
        // exists; a tab's editor is made at its first render. Started
        // here, every tab restored in every worktree started its server
        // at launch, and a worktree nobody switched to ran a 3.5 GB
        // rust-analyzer. M5.11's decision 11.
        self.adopt(
            self.seat_for_project(project),
            project,
            Body::File {
                path: relative,
                tab: view,
                presentation,
                // **A restored tab is the project's.** An external one
                // is never written down: see `store_tabs`.
                origin: crate::Origin::Project,
            },
            cx,
        )
    }

    /// How a file tab is set right now: the terminal's face and size,
    /// the palette. Code is code, and two monospace faces in one window
    /// would be a fault.
    pub(crate) fn file_look(&self) -> Look {
        Look {
            family: self.code_font.clone(),
            font_size: self.settings.terminal_font_size,
            theme: self.theme,
            autosave: self.settings.autosave,
        }
    }

    /// Resolve the code face from what is installed — once, and again
    /// only when the setting changes. Called from the window's render,
    /// which is where the text system is; the first render puts the
    /// real face on every tab opened before it.
    pub(crate) fn refresh_code_font(&mut self, window: &Window, cx: &mut Context<Self>) {
        let setting = self.settings.code_font.clone();
        if self.code_font_setting == setting
            && !self.code_font.is_empty()
            && self.code_font_resolved
        {
            return;
        }
        // **The editor's face, not the terminal's** — its own setting
        // since M12.11: a file has no Nerd glyphs, so a person's choice
        // wins and Lilex is everything else.
        let chosen: gpui::SharedString = crate::terminal::font::choose_code(
            &window.text_system().all_font_names(),
            setting.as_deref(),
        )
        .into();
        self.code_font_setting = setting;
        self.code_font_resolved = true;
        // A Markdown preview's code too, which the component draws in its
        // own monospace face — at every resolution, not only on a change:
        // the window starts with the default face already here, and the
        // component with Menlo.
        crate::editor::apply_code_font(&chosen, cx);
        if chosen != self.code_font {
            self.code_font = chosen;
            self.restyle_file_tabs(cx);
        }
    }

    /// Tell every file tab the palette or the font changed.
    pub(crate) fn restyle_file_tabs(&mut self, cx: &mut Context<Self>) {
        let look = self.file_look();
        let ui = self.ui();
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::File { tab: view, .. } = body {
                let look = look.clone();
                view.update(cx, |file, cx| file.restyle(look, ui, cx));
            }
        }
    }

    /// Bring a closed file tab back, if its file is still there.
    pub(crate) fn reopen_file(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file(&path, window, cx);
    }

    /// Record a closing file tab so that `cmd-shift-t` can bring it back
    /// — into `area`, the one it was closed from (M12.8).
    pub(crate) fn remember_closed_file(
        &mut self,
        project: guild_session::ProjectId,
        path: PathBuf,
        area: Option<guild_session::AreaId>,
    ) {
        self.closed.push_from(
            self.active_workspace,
            area,
            ClosedTab::File { project, path },
        );
    }

    /// The tree's selection follows the tab in front: when a file tab
    /// becomes active, the way to it is opened and its row brought into
    /// view. Asked on every render and cheap when nothing changed.
    pub(crate) fn follow_active_file_in_tree(&mut self) {
        let active = self.active_tab;
        if active == self.tree_followed {
            return;
        }
        self.tree_followed = active;
        // A dependency's file is followed into External Libraries too —
        // since 2026-09-28. M8.8.3 kept
        // the selection where it was, when the section had no rows to
        // light; it has had them since M8.14.
        let path = match self.active_body() {
            Some(Body::File {
                path,
                origin: crate::Origin::Project,
                ..
            }) => path.clone(),
            Some(Body::File {
                path,
                origin: crate::Origin::External(at),
                ..
            }) => {
                let (at, path) = (*at, path.clone());
                self.reveal_in_library(at, &path);
                return;
            }
            _ => return,
        };
        let Some(project) = self.active_project else {
            return;
        };
        let seat = self.seat_for_project(project);
        let Some(state) = self.trees.get_mut(&seat) else {
            return;
        };
        if let Some(row) = state.tree.reveal(&path) {
            let key = (crate::Origin::Project, path.clone());
            if !state.files.contains(&key) {
                state
                    .files
                    .click(&key, std::slice::from_ref(&key), false, false);
                state.selected = Some(path);
                state.library_selected = None;
            }
            state
                .scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
        }
    }
}

impl Root {
    /// The handlers for a file in a tab: saving it, reloading it, finding where a name is defined.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_files(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &SaveFile, _window, cx| root.save_active_file(cx)))
            .on_action(cx.listener(|root, _: &SaveAllFiles, _window, cx| root.save_all_files(cx)))
            .on_action(
                cx.listener(|root, _: &ReloadFile, window, cx| root.reload_active_file(window, cx)),
            )
            .on_action(
                cx.listener(|root, _: &RevealFile, window, cx| root.reveal_active_file(window, cx)),
            )
            .on_action(
                cx.listener(|root, _: &GoToDefinition, window, cx| {
                    root.go_to_definition(window, cx)
                }),
            )
            // M8.15: the usages of the name under the caret, and the Find
            // tool's next and previous — from the editor as from the tool.
            .on_action(cx.listener(|root, _: &FindUsages, window, cx| root.find_usages(window, cx)))
            .on_action(
                cx.listener(|root, _: &NextUsage, window, cx| root.step_usage(true, window, cx)),
            )
            .on_action(
                cx.listener(|root, _: &PreviousUsage, window, cx| {
                    root.step_usage(false, window, cx)
                }),
            )
            // **The tree's four, and they aim at what it has selected.**
            // A stroke reaches the thing in front — decision 20 — and for
            // the Explorer that is the selected row; the menu's items are
            // deeds on the row under the pointer and are a different
            // route to the same three methods.
            .on_action(cx.listener(|root, _: &NewFile, window, cx| {
                root.name_something(crate::root::tree::For::File, window, cx);
            }))
            .on_action(cx.listener(|root, _: &NewDirectory, window, cx| {
                root.name_something(crate::root::tree::For::Directory, window, cx);
            }))
            .on_action(cx.listener(|root, _: &RenameFile, window, cx| {
                root.name_something(crate::root::tree::For::Rename, window, cx);
            }))
            .on_action(cx.listener(|root, _: &JumpToSource, window, cx| {
                root.jump_to_source(window, cx);
            }))
            .on_action(cx.listener(|root, _: &DeleteFile, window, cx| {
                if root.changes().is_some_and(|s| s.focus.is_focused(window)) {
                    root.changes_trash_selected(window, cx);
                } else {
                    root.delete_selected_row(window, cx);
                }
            }))
    }
}

impl Root {
    fn hear_file_runs(
        &mut self,
        project: ProjectId,
        view: &Entity<FileTab>,
        document: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        if self
            .tabs
            .iter()
            .flat_map(Tab::bodies)
            .any(|body| matches!(body, Body::File {tab,..} if tab == view))
        {
            return;
        }
        if let Some(path) = document {
            self.watch_file_document(project, path, view, cx);
        }
        // A background SVG read or first editor realization wakes deferred
        // navigation; no frame budget or polling loop expires the destination.
        cx.observe(view, |root, _, cx| {
            if root.file_landing_pending() {
                cx.notify();
            }
        })
        .detach();
        cx.subscribe(
            view,
            |root, file, _: &crate::file_tab::BufferChanged, cx| {
                root.follow_file_in_diffs(&file, cx);
                if root.file_landing_pending() {
                    cx.notify();
                }
            },
        )
        .detach();
        cx.subscribe(
            view,
            move |root, _, event: &crate::file_tab::LaunchRun, cx| {
                if root.desk.project(project).is_some() {
                    root.reveal_tool(guild_session::Tool::Output, cx);
                    root.start_invocation(project, event.0.clone(), cx);
                }
            },
        )
        .detach();
        // GLD-026: the marker popup's Stage or Unstage, sent to the git
        // thread of the project the file belongs to.
        cx.subscribe(
            view,
            move |root, _, event: &crate::file_tab::HunkRequest, cx| {
                root.send_hunk(project, event.0.clone(), cx);
                cx.notify();
            },
        )
        .detach();
        cx.subscribe(
            view,
            move |root, file, event: &crate::file_tab::BlameRequest, _| {
                if let Some(worker) = root.status_workers.get(&project) {
                    worker.inline_blame(file.entity_id(), event.generation, event.path.clone());
                }
            },
        )
        .detach();
        cx.subscribe(
            view,
            move |root, _, event: &crate::file_tab::RevealCommit, cx| {
                if root.active_project == Some(project) {
                    root.reveal_commit(
                        project,
                        &event.0.sha,
                        Some(event.0.filename.clone()),
                        Some(event.0.original_line),
                        cx,
                    );
                }
            },
        )
        .detach();
    }
    pub(super) fn markdown_presentation(
        file: &Entity<FileTab>,
        initial_mode: guild_session::markdown::Mode,
        renderer: &Entity<crate::diagrams::runtime::Manager>,
        cx: &mut Context<Self>,
    ) -> Option<crate::markdown::Presentation> {
        let presentation = crate::markdown::Presentation::for_file(file, initial_mode, cx)?;
        presentation.attach_renderer(renderer, cx);
        cx.subscribe(
            &presentation.view,
            |root, _, _: &crate::markdown::ModeChanged, cx| {
                root.store_tabs();
                cx.notify();
            },
        )
        .detach();
        Some(presentation)
    }
}

impl Root {
    /// A navigation command that places a caret must reveal its source first.
    pub(crate) fn reveal_markdown_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(Body::File { tab, .. }) = self.active_body() {
            let file = tab.clone();
            file.update(cx, |file, cx| file.reveal_image_source(window, cx));
        }
        let pane = match self.active_body() {
            Some(Body::File {
                presentation: Some(p),
                ..
            }) if p.mode() == guild_session::markdown::Mode::Preview => Some(p.view.clone()),
            _ => None,
        };
        if let Some(pane) = pane {
            pane.update(cx, |pane, cx| {
                pane.set_mode(guild_session::markdown::Mode::Split, window, cx)
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The sheet's buttons by position — and a sheet that went away
    /// without an answer is a Cancel: nothing is closed on silence.
    #[test]
    fn the_answer_is_read_by_position() {
        assert_eq!(Answer::of(Ok::<usize, ()>(0)), Answer::Save);
        assert_eq!(Answer::of(Ok::<usize, ()>(1)), Answer::DontSave);
        assert_eq!(Answer::of(Ok::<usize, ()>(2)), Answer::Cancel);
        assert_eq!(Answer::of(Err::<usize, ()>(())), Answer::Cancel);
    }

    /// A dirty tab goes only on an answer that leaves nothing to lose: a
    /// save that succeeded, or a person saying the changes may go. A save
    /// that failed keeps the tab, dirty, and the walk stops there.
    #[test]
    fn a_dirty_tab_goes_only_when_nothing_is_lost() {
        assert!(Answer::Save.may_proceed(|| Ok(())));
        assert!(!Answer::Save.may_proceed(|| Err("disk full".to_owned())));
        assert!(Answer::DontSave.may_proceed(|| unreachable!("nothing is saved")));
        assert!(!Answer::Cancel.may_proceed(|| unreachable!("nothing is saved")));
    }

    /// **One question at a time, the running sessions' included.** The
    /// menu stays live under a sheet, and until M5.11's merge the
    /// sessions question set no flag: a second `cmd-q` under it queued a
    /// second *Quit anyway?*, and the close button a third. A close with
    /// nothing at stake still goes at once, as it did — a quit under an
    /// unrelated sheet was never held back, and this changes only what
    /// would have been asked twice.
    #[test]
    fn a_close_asks_nothing_while_a_question_is_up() {
        assert_eq!(Closing::of(false, false, false), Closing::Now);
        assert_eq!(Closing::of(true, false, false), Closing::Now);

        assert_eq!(Closing::of(false, true, false), Closing::Ask);
        assert_eq!(Closing::of(false, false, true), Closing::Ask);
        assert_eq!(Closing::of(false, true, true), Closing::Ask);

        assert_eq!(Closing::of(true, true, false), Closing::AlreadyAsked);
        assert_eq!(Closing::of(true, false, true), Closing::AlreadyAsked);
        assert_eq!(Closing::of(true, true, true), Closing::AlreadyAsked);
    }
}
