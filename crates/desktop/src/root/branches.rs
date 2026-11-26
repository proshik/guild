//! The branch popup: what the window keeps for it.
//!
//! The two lists — the local branches and the stash — are read by the
//! project's git thread like everything else git says, and land here
//! through `poll_git`. They are kept per project rather than inside the
//! popup, because the Log's branch column reads the same list and two
//! copies of one list are two lists that will disagree.

use std::collections::HashMap;

use gpui::Context;
use guild_session::ProjectId;

use crate::Root;
use crate::branch_list::Row;
use crate::git::GitError;
use crate::git::branches::Branch;
use crate::git::stashes::Stash;
use crate::picker::{Jump, Key, Lit, Take};
use crate::root::picker::{Focus, Holds, Scroll};

/// Which tab of the popup is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Face {
    /// The local branches.
    #[default]
    Branches,
    /// What is in the stash.
    Stashes,
    Sources,
}

impl Root {
    /// The project's local branches, as git last said them.
    pub(crate) fn branches_of(&self, project: ProjectId) -> &[Branch] {
        self.branch_lists
            .get(&project)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    /// The active project's, which is what the popup and the Log draw.
    pub(crate) fn branches_here(&self) -> &[Branch] {
        match self.active_project {
            Some(project) => self.branches_of(project),
            None => &[],
        }
    }

    /// What is in the active project's stash.
    pub(crate) fn stashes_here(&self) -> &[Stash] {
        match self.active_project {
            Some(project) => self
                .stash_lists
                .get(&project)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            None => &[],
        }
    }

    /// The branches landed. A refusal leaves the last list standing —
    /// the same rule the status map has: what git said last is better
    /// than nothing at all while git is busy failing.
    pub(crate) fn branches_landed(
        &mut self,
        project: ProjectId,
        result: Result<Vec<Branch>, GitError>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(branches) = result {
            self.branch_lists
                .insert(project, in_reading_order(branches));
            // The list the highlight was counted against has just been
            // replaced: a branch deleted under it leaves it past the end.
            if self.active_project == Some(project) && self.branch_popup_open() {
                self.settle_picker();
            }
            cx.notify();
        }
    }

    /// The stash landed.
    pub(crate) fn stashes_landed(
        &mut self,
        project: ProjectId,
        result: Result<Vec<Stash>, GitError>,
        cx: &mut Context<Self>,
    ) {
        if let Ok(stashes) = result {
            self.stash_lists.insert(project, stashes);
            if self.active_project == Some(project) && self.branch_popup_open() {
                self.settle_picker();
            }
            cx.notify();
        }
    }

    /// Ask git for both lists, which is what opening the popup does and
    /// what a change under `.git` does while it is open.
    pub(crate) fn refresh_branches(&mut self, project: ProjectId) {
        if let Some(worker) = self.status_workers.get(&project) {
            worker.update_sources();
            worker.branches();
            worker.stashes();
        }
    }

    /// Whether the branch popup is the popup that is up.
    pub(crate) fn branch_popup_open(&self) -> bool {
        self.branch_face().is_some()
    }

    /// Which tab of the branch popup is showing, while it is up.
    pub(crate) fn branch_face(&self) -> Option<Face> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::Branches(face)) => Some(*face),
            _ => None,
        }
    }

    /// The button in the title bar: open the popup, or close it if it is
    /// the popup that is open.
    ///
    /// The popup is a field: it takes the focus, or its keys are the
    /// window's and a letter typed at it reaches the terminal behind —
    /// M5.8's lesson, paid for once already. `open_picker` takes it.
    pub(crate) fn toggle_branch_popup(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if self.branch_popup_open() {
            self.close_picker(Focus::Back, window, cx);
            return;
        }
        if let Some(project) = self.active_project {
            self.refresh_branches(project);
        }
        self.open_picker(
            Holds::Branches(Face::default()),
            self.active_project,
            Scroll::children(),
            window,
            cx,
        );
    }

    /// Switch tabs. The query stays: a person who typed a name and
    /// looked at the stashes did not mean to lose it, and the same
    /// query narrows both lists. The highlight goes to the new list's
    /// first row — what was lit was lit in the other one.
    pub(crate) fn show_branch_face(&mut self, face: Face, cx: &mut Context<Self>) {
        let Some(open) = self.picker.as_mut() else {
            return;
        };
        let Holds::Branches(shown) = &mut open.holds else {
            return;
        };
        *shown = face;
        let rows = self.picker_lit();
        if let Some(open) = self.picker.as_mut() {
            open.picker.jump(Jump::First, &rows);
        }
        self.settle_picker();
        cx.notify();
    }

    /// The Branches face's rows for what has been typed —
    /// `branch_list::rows`, which the view draws and a take reads.
    pub(crate) fn branch_rows_shown(&self) -> Vec<Row> {
        let names: Vec<String> = self
            .branches_here()
            .iter()
            .map(|branch| branch.name.clone())
            .collect();
        crate::branch_list::rows(&names, self.picker_query())
    }

    /// The Stashes face's rows: indices into the stash, found by their
    /// subjects' scattered letters.
    pub(crate) fn stash_rows_shown(&self) -> Vec<usize> {
        let subjects: Vec<String> = self
            .stashes_here()
            .iter()
            .map(|stash| stash.subject.clone())
            .collect();
        crate::fuzzy::rank(
            self.picker_query().trim(),
            &subjects,
            String::as_str,
            crate::fuzzy::Shape::Name,
        )
        .into_iter()
        .map(|(index, _)| index)
        .collect()
    }

    /// The branch popup's rows, for lighting: the *Local* heading is never
    /// lit.
    pub(crate) fn branches_lit(&self) -> Vec<Lit> {
        match self.branch_face() {
            Some(Face::Sources) => vec![Lit::Row; self.update_source_rows().len()],
            Some(Face::Branches) => self.branch_rows_shown().iter().map(Row::lit).collect(),
            Some(Face::Stashes) => vec![Lit::Row; self.stash_rows_shown().len()],
            None => Vec::new(),
        }
    }

    /// The branch popup's own keys: Tab between its two faces.
    pub(crate) fn branches_key(
        &mut self,
        key: &Key,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.branch_face() == Some(Face::Sources) {
            return false;
        }
        if !matches!(key, Key::NextTab | Key::PreviousTab) {
            return false;
        }
        let next = match self.branch_face() {
            Some(Face::Branches) => Face::Stashes,
            _ => Face::Branches,
        };
        self.show_branch_face(next, cx);
        true
    }

    /// Narrowing is the whole of what an edit does here.
    pub(crate) fn branches_edited(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) {}

    /// What the row taken does — on the Branches tab, the branch it is on
    /// or the name typed; on the Stashes tab, applying the entry.
    ///
    /// The deeds close the popup themselves, and only when they went
    /// ahead: a refusal is a notice read against the list it is about.
    pub(crate) fn branches_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.picker_query().trim().to_owned();
        match self.branch_face() {
            Some(Face::Sources) => {
                if let Some(source) = self.update_source_rows().get(row).cloned() {
                    self.update_from_asking(source, window, cx);
                }
            }
            Some(Face::Stashes) => {
                let Some(index) = self.stash_rows_shown().get(row).copied() else {
                    return;
                };
                // The reference git printed, taken from the entry the row
                // is on — never the row's own number. The list shifts
                // under a drop, and a number would then name somebody
                // else's work.
                let Some(reference) = self
                    .stashes_here()
                    .get(index)
                    .map(|stash| stash.reference.clone())
                else {
                    return;
                };
                self.picker_deed(window, cx, |root, cx| root.apply_stash(reference, cx));
            }
            Some(Face::Branches) => match self.branch_rows_shown().get(row).copied() {
                // The first row makes the branch here; the second makes a
                // worktree of its own beside the repository, on a branch
                // of that name. M12.
                Some(Row::Create) => {
                    self.picker_deed(window, cx, |root, cx| root.create_branch(query, cx))
                }
                Some(Row::Worktree) => {
                    self.picker_deed(window, cx, |root, cx| root.create_worktree(query, cx))
                }
                Some(Row::Branch(index)) => {
                    let Some(name) = self
                        .branches_here()
                        .get(index)
                        .map(|branch| branch.name.clone())
                    else {
                        return;
                    };
                    self.picker_deed(window, cx, |root, cx| root.switch_branch(name, cx));
                }
                Some(Row::Heading) | None => {}
            },
            None => {}
        }
    }

    /// Delete a branch, if git agrees nothing is lost with it.
    ///
    /// **The popup stays open.** git's refusal — a branch whose commits
    /// are on no other — arrives as a notice, and it is read against the
    /// list it is about. Closing the popup would put the answer on a
    /// window that no longer shows the question.
    ///
    /// The current branch is never offered: git refuses to delete the
    /// one you are on, and a bin that only ever produces a refusal is a
    /// bin that should not be drawn.
    pub(crate) fn delete_branch(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if self
            .branches_here()
            .iter()
            .any(|b| b.name == name && b.current)
        {
            return;
        }
        self.run_git_write(project, crate::git::Write::DeleteBranch(name));
        self.refresh_branches(project);
        cx.notify();
    }

    /// Put a stash's changes onto the working tree.
    ///
    /// It writes the working tree, so it asks what a switch asks: the
    /// buffers go to disk first, and a tab that cannot be written stops
    /// it — git must see what the window shows.
    ///
    /// The popup closes, because the answer to "apply this" is the
    /// working tree, which is behind it.
    pub(crate) fn apply_stash(&mut self, reference: String, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.autosave_files(crate::autosave::Trigger::Commit, cx);
        if let Some(open) = self.unwritable_tab(project, cx) {
            let text = self.ui().commit_refused_by(&open);
            self.warn(text);
            cx.notify();
            return;
        }
        self.run_git_write(project, crate::git::Write::ApplyStash(reference));
        self.close_branch_popup(cx);
    }

    /// Drop a stash entry — after asking, in a sheet that names it.
    ///
    /// **The window asks because git will not.** `stash drop` answers
    /// without a word, and what it drops is kept only by its hash until git
    /// next collects garbage; the sheet says so, and the Console keeps the
    /// hash git prints. Decision 17 of M5.11, decision 21's third kind.
    ///
    /// **The entry, not the position.** The write carries the entry's
    /// commit, and the git thread finds where that commit is in the list
    /// at the moment of the drop — a stash pushed from a terminal while the
    /// sheet was up moves every entry down one. An entry gone by then is
    /// not dropped, and the window says so.
    ///
    /// Nothing of the working tree changes, so there is nothing to
    /// autosave and nothing to refuse; the popup stays open and the list
    /// under it shortens.
    pub(crate) fn drop_stash_asking(
        &mut self,
        stash: Stash,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        // One question at a time: the popup stays live under a sheet, and
        // a double-click on the bin would otherwise queue a second sheet
        // for an entry the first one has already dropped.
        if self.asking {
            return;
        }
        let ui = self.ui();
        let title = ui.drop_stash_title(&stash.subject);
        let answers = [
            gpui::PromptButton::new(ui.text(crate::i18n::Text::DropStash)),
            // Typed rather than a plain string: the platform binds Escape
            // to the button it knows as Cancel, and from a `&str` it knows
            // the English word only.
            gpui::PromptButton::cancel(ui.text(crate::i18n::Text::Cancel)),
        ];
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            &title,
            Some(ui.confirm_drop_stash()),
            &answers,
            cx,
        );
        self.asking = true;
        let write = crate::git::Write::DropStash {
            reference: stash.reference,
            sha: stash.sha,
        };
        cx.spawn(async move |root, cx| {
            let answer = asked.await;
            let _ = root.update(cx, |root, cx| {
                root.asking = false;
                if answer == Ok(0) {
                    root.run_git_write(project, write);
                    root.refresh_branches(project);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Make a branch from what was typed, and move onto it.
    ///
    /// The same order as a switch, because it *is* a switch with one
    /// more thing before it: the disk is written first, and git refuses
    /// a name it does not like in its own words.
    pub(crate) fn create_branch(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if name.trim().is_empty() {
            return;
        }
        self.autosave_files(crate::autosave::Trigger::Commit, cx);
        if let Some(open) = self.unwritable_tab(project, cx) {
            let text = self.ui().commit_refused_by(&open);
            self.warn(text);
            cx.notify();
            return;
        }
        self.run_git_write(project, crate::git::Write::Create(name.trim().to_owned()));
        self.close_branch_popup(cx);
    }

    /// Switch to a branch.
    ///
    /// **The disk is written first.** A checkout takes the working tree,
    /// so what git sees has to be what the window shows: every dirty tab
    /// is saved, and a tab that cannot be saved stops the switch by name
    /// — the rule a commit has had since M8.3, and for the same reason.
    ///
    /// Everything after that is git's. It refuses a switch that would
    /// overwrite work, in words that name the files, and that refusal
    /// arrives as a notice like any other. Nothing here checks anything
    /// git checks better — decision 21's first kind of guard.
    pub(crate) fn switch_branch(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if self
            .branches_here()
            .iter()
            .any(|b| b.name == name && b.current)
        {
            // Already there: git would say so, and a notice about it is
            // noise for a click that means "show me this branch".
            self.close_branch_popup(cx);
            return;
        }
        self.autosave_files(crate::autosave::Trigger::Commit, cx);
        if let Some(name) = self.unwritable_tab(project, cx) {
            let text = self.ui().commit_refused_by(&name);
            self.warn(text);
            cx.notify();
            return;
        }
        self.run_git_write(project, crate::git::Write::Switch(name));
        self.close_branch_popup(cx);
    }

    /// Close it after a deed that went ahead — from a door with no window.
    ///
    /// **The focus is given back by the caller**: a take goes through
    /// `picker_deed`, which focuses what the popup was opened over once
    /// the deed has closed it. Escape and a click away are the picker's
    /// own `close_picker`.
    pub(crate) fn close_branch_popup(&mut self, cx: &mut Context<Self>) {
        if self.branch_popup_open() {
            self.picker = None;
            cx.notify();
        }
    }
}

/// The two maps the root keeps, one entry per project.
pub(crate) type Lists<T> = HashMap<ProjectId, Vec<T>>;

/// The list as the window reads it: the default branch first.
///
/// Applied where the list **lands** rather than where it is drawn. Two
/// surfaces show these branches — the popup and the Log's column — and
/// they take the same slice on purpose; ordering it twice is two chances
/// to disagree, and an index from `branch_list::rows` would then mean a
/// different branch in each.
fn in_reading_order(branches: Vec<Branch>) -> Vec<Branch> {
    let names: Vec<String> = branches.iter().map(|branch| branch.name.clone()).collect();
    let mut moved: Vec<Option<Branch>> = branches.into_iter().map(Some).collect();

    crate::branch_list::ordered(&names)
        .into_iter()
        .filter_map(|index| moved[index].take())
        .collect()
}

impl Root {
    pub(crate) fn update_snapshot(&self) -> Option<&crate::git::branch_update::Snapshot> {
        self.update_snapshot_of(self.active_project?)
    }
    fn update_snapshot_of(
        &self,
        project: ProjectId,
    ) -> Option<&crate::git::branch_update::Snapshot> {
        self.update_sources.get(&project)?.as_ref().ok()
    }
    pub(crate) fn update_source(&self) -> Option<&str> {
        self.update_source_of(self.active_project?)
    }
    fn update_source_of(&self, project: ProjectId) -> Option<&str> {
        let snapshot = self.update_snapshot_of(project)?;
        self.settings
            .branch_update_sources
            .get(&snapshot.root)
            .map(String::as_str)
            .or(snapshot.default.as_deref())
    }
    /// The source popup's rows for what has been typed —
    /// `branch_list::source_rows`, which orders and filters them.
    pub(crate) fn update_source_rows(&self) -> Vec<crate::git::branch_update::Source> {
        let Some(snapshot) = self.update_snapshot() else {
            return Vec::new();
        };
        let references: Vec<String> = snapshot
            .sources
            .iter()
            .map(|source| source.reference.clone())
            .collect();
        crate::branch_list::source_rows(
            &references,
            snapshot.default.as_deref(),
            self.picker_query(),
        )
        .into_iter()
        .map(|i| snapshot.sources[i].clone())
        .collect()
    }

    /// The row lit when the source popup opens: the source an update
    /// would use now, which is the one marked.
    pub(crate) fn update_sources_opening(&self) -> usize {
        let chosen = self.update_source();
        self.update_source_rows()
            .iter()
            .position(|source| Some(source.reference.as_str()) == chosen)
            .unwrap_or(0)
    }

    /// The title bar's ↓: open the source popup under it. **It never
    /// updates by itself** — a source is chosen and confirmed first — and
    /// while an update is running it does nothing; the spinner says why.
    pub(crate) fn open_update_sources(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        if self.updating_branches.contains_key(&project) {
            return;
        }
        // Escape and a click away give the focus back to ↓, which is what
        // the popup was opened from — by a click as much as by a key.
        window.focus(&self.update_branch_focus, cx);
        self.refresh_branches(project);
        self.open_picker(
            Holds::Branches(Face::Sources),
            self.active_project,
            Scroll::Children(gpui::ScrollHandle::new()),
            window,
            cx,
        );
    }
    /// A source taken in the popup: close it and ask, in a sheet that
    /// names the branch, the source and what the update will do.
    ///
    /// **Update** remembers the source and runs the update the ↓ always
    /// ran — the held run, the unsaved buffers and git's refusals are
    /// [`Root::update_branch_of`]'s; **Cancel** or Escape change nothing,
    /// and the source is not remembered. A source the update cannot use
    /// is refused before any sheet, in the notice the update itself would
    /// have raised, and the popup stays up to choose another.
    fn update_from_asking(
        &mut self,
        source: crate::git::branch_update::Source,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        // One question at a time, as the stash's sheet asks.
        if self.asking || self.updating_branches.contains_key(&project) {
            return;
        }
        let Some(snapshot) = self.update_snapshot() else {
            return;
        };
        let request = match snapshot.request(&source.reference) {
            Ok(request) => request,
            Err(err) => {
                let ui = self.ui();
                self.warn(ui.notice_git_refused(&ui.branch_update_diagnostic(&err.to_string())));
                cx.notify();
                return;
            }
        };
        let root = snapshot.root.clone();
        let ui = self.ui();
        let branch = crate::branch_list::source_name(&request.head.0);
        let title =
            ui.update_branch_title(branch, crate::branch_list::source_name(&source.reference));
        let detail = ui.update_branch_detail(
            branch,
            crate::branch_list::source_name(&request.source.reference),
            request.source.remote.is_some(),
        );
        self.close_picker(Focus::Back, window, cx);
        let answers = [
            gpui::PromptButton::new(ui.text(crate::i18n::Text::UpdateBranchConfirm)),
            gpui::PromptButton::cancel(ui.text(crate::i18n::Text::Cancel)),
        ];
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            &title,
            Some(&detail),
            &answers,
            cx,
        );
        self.asking = true;
        let reference = source.reference;
        cx.spawn(async move |root_view, cx| {
            let answer = asked.await;
            let _ = root_view.update(cx, |this, cx| {
                this.asking = false;
                if answer == Ok(0) {
                    this.settings.branch_update_sources.insert(root, reference);
                    this.store_settings();
                    this.update_project_branch(project, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Update the branch in front from its remembered source — what the
    /// sheet's Update runs, here for the tests that drive it directly.
    #[cfg(test)]
    pub(crate) fn update_current_branch(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.update_project_branch(project, cx);
    }

    /// Update one project's branch from its remembered source — whichever
    /// project is in front by the time a sheet is answered.
    fn update_project_branch(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if self.updating_branches.contains_key(&project) {
            return;
        }
        // Never beside an auto fast-forward of the same repository: the
        // click is held, shown running, and carried out from the state
        // the run leaves — built then, so the run moving `main` is not a
        // "HEAD changed" refusal. Decision 21.
        if self
            .repository_of(project)
            .is_some_and(|common| self.auto_ff.running(&common))
        {
            self.held_updates.insert(project, Held::Waiting);
            self.updating_branches.insert(
                project,
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            );
            cx.notify();
            return;
        }
        self.update_branch_of(project, cx);
    }

    /// The click's deed for one project, now.
    fn update_branch_of(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        if !self.dirty_file_tabs(Some(project), cx).is_empty() {
            self.warn(
                self.ui()
                    .text(crate::i18n::Text::UpdateBranchUnsaved)
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        let request = self
            .update_snapshot_of(project)
            .zip(self.update_source_of(project))
            .map(|(s, source)| s.request(source));
        match request {
            Some(Ok(request)) => {
                let clean = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
                let (in_flight, flag) = crate::root::git::InFlight::new();
                if self
                    .status_workers
                    .get(&project)
                    .is_some_and(|worker| worker.update_branch(request, clean.clone(), in_flight))
                {
                    self.updating_branches.insert(project, clean);
                    if let Some(common) = self.repository_of(project) {
                        self.auto_ff.manual_started(common, flag);
                    }
                }
            }
            Some(Err(err)) => self.warn(
                self.ui()
                    .notice_git_refused(&self.ui().branch_update_diagnostic(&err.to_string())),
            ),
            None => self.warn(
                self.ui()
                    .text(crate::i18n::Text::UpdateBranchChoose)
                    .to_owned(),
            ),
        }
        cx.notify();
    }
}

/// A manual Update branch click held while its repository's auto
/// fast-forward was in flight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// The run is still in flight.
    Waiting,
    /// The run landed and the project's identity was asked again; the
    /// click runs when that answer lands.
    Asked,
}

impl Root {
    /// The run of `common` landed: every click held for it asks its root
    /// for a fresh identity, which [`Root::held_update_sources_landed`]
    /// then acts on.
    pub(crate) fn release_held_updates(&mut self, common: &std::path::Path) {
        let ready: Vec<ProjectId> = self
            .held_updates
            .iter()
            .filter(|(project, held)| {
                **held == Held::Waiting && self.repository_of(**project).as_deref() == Some(common)
            })
            .map(|(project, _)| *project)
            .collect();
        for project in ready {
            match self.status_workers.get(&project) {
                Some(worker) => {
                    worker.update_sources();
                    self.held_updates.insert(project, Held::Asked);
                }
                None => {
                    self.held_updates.remove(&project);
                    self.updating_branches.remove(&project);
                }
            }
        }
    }

    /// A project's identity landed: a click held for it runs now, from it.
    pub(crate) fn held_update_sources_landed(
        &mut self,
        project: ProjectId,
        cx: &mut Context<Self>,
    ) {
        if self.held_updates.get(&project) != Some(&Held::Asked) {
            return;
        }
        self.held_updates.remove(&project);
        self.updating_branches.remove(&project);
        self.update_branch_of(project, cx);
    }
}
