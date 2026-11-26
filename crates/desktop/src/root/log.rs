//! The Log panel: what the window does with it.
//!
//! The model is `git::log`, and it does not know about windows. This is
//! the other half — which project's commits, how many pages of them,
//! which one is opened and what its files are, where the list is
//! scrolled to — and the answers to a click. A page is one ask of the
//! project's git thread; the answers land through `poll_git` like
//! everything else git says.

use std::collections::HashMap;
use std::path::Path;

use gpui::{
    AppContext as _, Context, FocusHandle, KeyDownEvent, ScrollStrategy, UniformListScrollHandle,
    Window,
};
use guild_session::{Base, ProjectId};

use crate::Root;
use crate::git::GitError;
use crate::git::log::{Commit, FileChange};
mod reveal;
pub(crate) use reveal::{Navigation, PageReply, RevealReply};

/// How tall the Console's editor may grow, in rows: more than two
/// hundred commands' worth of answers, so it is never the editor that
/// scrolls but the box around it.
const CONSOLE_ROWS: usize = 100_000;

/// How many commits one ask brings. Task 0 measured 200 at 12.7 ms warm.
pub(crate) const PAGE: usize = 200;

/// How many recently listed branches the Branch menu offers.
///
/// Five: enough for the two or three a person is moving between, short
/// enough that the menu's head stays a head rather than becoming the list
/// it opens.
const RECENT_BRANCHES: usize = 5;

/// Which of the Log's two fields takes what is typed.
///
/// **A click chooses**, and the toolbar's is the one that starts chosen:
/// it is the field a person came to the Log to use, and the branch
/// column's narrows a list of a dozen names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Typing {
    #[default]
    Filter,
    Branch,
}

/// One project's log as the window holds it.
pub(crate) struct LogState {
    pub epoch: u64,
    pub navigation: Option<Navigation>,
    pub navigation_error: Option<String>,
    /// The pages so far, newest first.
    pub commits: Vec<Commit>,
    pub graph: std::rc::Rc<crate::log_graph::Graph>,
    pub file_tree: crate::files_tree::Tree,
    /// What is being listed: every branch, the head of the current
    /// one, or one branch. Chosen in the toolbar or in the branch
    /// column; **read** only — nothing here moves `HEAD`, decision 3 of
    /// M8.3.2.
    pub scope: crate::git::Scope,
    /// What the toolbar narrows the list by.
    ///
    /// Part of the state rather than of the view: the next page has to
    /// be asked for with the same filter, or scrolling would mix a
    /// filtered page with an unfiltered one.
    pub filter: crate::git::Filter,
    /// What is typed into the branch column's own field.
    pub branch_query: String,
    /// Whether the *Local* group is open.
    pub local_open: bool,
    /// Which field takes what is typed.
    pub typing: Typing,
    /// The branches whose log was read lately, newest first.
    ///
    /// **Recently *listed*, not recently checked out** — this is the
    /// filter's own history, and it is what a person flipping between two
    /// branches' logs wants at the top. It lives for the run: a list of
    /// five names is not worth a file, and the first choice of a session
    /// refills it.
    pub recent: Vec<String>,
    /// Why there are none, when git said so.
    pub error: Option<GitError>,
    /// The last page was short: there is nothing past it.
    pub exhausted: bool,
    /// A page has been asked for and has not landed.
    pub in_flight: bool,
    /// Immutable inspection key: selected hashes in displayed log order.
    pub opened: Option<String>,
    pub selection: crate::push_dialog::Selection,
    pub cursor: Option<String>,
    /// Files cached by immutable inspection key, once answered.
    pub files: HashMap<String, Result<Vec<FileChange>, GitError>>,
    /// Which branches contain each opened commit — the footer's *In N
    /// branches*. Asked when a commit is opened, beside its files.
    pub contains: HashMap<String, Vec<String>>,
    pub scroll: UniformListScrollHandle,
    pub focus: FocusHandle,
    pub file_focus: FocusHandle,
    pub file_scroll: gpui::ScrollHandle,
    pub file_selection: crate::file_selection::FileSelection,
    pub speed: Option<crate::speed_search::Search<std::path::PathBuf>>,
}

impl Root {
    /// The active project's log, asked for the first time the Log face
    /// is drawn — a page is a process, and there is no reason to run it
    /// for a project nobody is looking at.
    pub(crate) fn ensure_log(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if self.logs.contains_key(&project) {
            return;
        }
        self.logs.insert(
            project,
            LogState {
                epoch: 0,
                navigation: None,
                navigation_error: None,
                commits: Vec::new(),
                graph: Default::default(),
                file_tree: Default::default(),
                scope: crate::git::Scope::default(),
                filter: crate::git::Filter::default(),
                branch_query: String::new(),
                local_open: true,
                typing: Typing::default(),
                recent: Vec::new(),
                error: None,
                exhausted: false,
                in_flight: false,
                opened: None,
                selection: Default::default(),
                cursor: None,
                files: HashMap::new(),
                contains: HashMap::new(),
                scroll: UniformListScrollHandle::new(),
                focus: cx.focus_handle(),
                file_focus: cx.focus_handle(),
                file_scroll: gpui::ScrollHandle::new(),
                file_selection: Default::default(),
                speed: None,
            },
        );
        self.ask_log_page(project);
        self.ask_branches(project);
    }

    /// The branches, asked with the first page and again when `HEAD`
    /// moves.
    fn ask_branches(&self, project: ProjectId) {
        if let Some(worker) = self.status_workers.get(&project) {
            worker.branches();
        }
    }

    /// The active project's log, if it has been asked for.
    pub(crate) fn log(&self) -> Option<&LogState> {
        self.active_project.and_then(|p| self.logs.get(&p))
    }

    /// The next page, unless one is already on its way or there is none:
    /// the list's last row asks for it as it is drawn.
    pub(crate) fn ask_log_page(&mut self, project: ProjectId) {
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        if state.exhausted || state.in_flight {
            return;
        }
        if let Some(worker) = self.status_workers.get(&project) {
            worker.log(
                state.epoch,
                state.scope.clone(),
                state.filter.clone(),
                state.commits.len(),
                PAGE,
            );
            state.in_flight = true;
        }
    }

    /// `HEAD` moved — a commit, from the panel or a terminal: the first
    /// page again, for a project whose log is open. It replaces what was
    /// loaded; the pages past it come back as one scrolls.
    pub(crate) fn refresh_log(&mut self, project: ProjectId) {
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        if let Some(worker) = self.status_workers.get(&project) {
            if let Some(intent) = &state.navigation {
                worker.reveal_commit(intent.clone());
                return;
            }
            state.epoch += 1;
            worker.log(
                state.epoch,
                state.scope.clone(),
                state.filter.clone(),
                0,
                PAGE,
            );
            worker.branches();
            state.in_flight = true;
        }
    }

    /// A page landed: the first replaces the list, a later one extends
    /// it, and a short one says the log ends there.
    pub(crate) fn log_landed(
        &mut self,
        project: ProjectId,
        skip: usize,
        result: Result<Vec<Commit>, GitError>,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        state.in_flight = false;
        match result {
            Ok(page) => {
                let short = page.len() < PAGE;
                if skip == 0 {
                    state.commits = page;
                } else {
                    state.commits.truncate(skip);
                    state.commits.extend(page);
                }
                state.graph = std::rc::Rc::new(crate::log_graph::Graph::of(&state.commits));
                state.exhausted = short;
                state.error = None;
                let shas: Vec<_> = state.commits.iter().map(|c| c.sha.as_str()).collect();
                state.selection.retain(&shas);
                state.sync_selection();
            }
            Err(err) => state.error = Some(err),
        }
        if let Some(state) = self.logs.get(&project)
            && let Some(key) = &state.opened
            && let Some(worker) = self.status_workers.get(&project)
        {
            if !state.files.contains_key(key) {
                worker.files(key.clone());
            }
            if !key.contains(',') && !state.contains.contains_key(key) {
                worker.contains(key.clone());
            }
        }
        cx.notify();
    }

    /// A commit's files landed.
    pub(crate) fn log_files_landed(
        &mut self,
        project: ProjectId,
        sha: String,
        result: Result<Vec<FileChange>, GitError>,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.logs.get_mut(&project) {
            if state.opened.as_ref() == Some(&sha) {
                match &result {
                    Ok(files) => state.file_tree.refresh(files),
                    // Hide stale rows/selection during an error, but retain the last
                    // fold intent until a successful reply can validate its keys.
                    Err(_) => state.file_tree.rows.clear(),
                }
            }
            state.files.insert(sha, result);
            state.file_selection.retain(&state.file_paths());
            cx.notify();
        }
    }

    /// Select the clicked commit, toggling or extending with modifiers.
    pub(crate) fn log_click(
        &mut self,
        sha: &str,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        if !self.select_log_commit(project, sha, modifiers.platform, modifiers.shift) {
            return;
        }
        let focus = self.logs[&project].focus.clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    fn open_log_commit(&mut self, project: ProjectId, sha: &str) -> bool {
        self.select_log_commit(project, sha, false, false)
    }

    fn select_log_commit(
        &mut self,
        project: ProjectId,
        sha: &str,
        toggle: bool,
        extend: bool,
    ) -> bool {
        let Some(state) = self.logs.get_mut(&project) else {
            return false;
        };
        if state.navigation.take().is_some() {
            state.epoch += 1;
            state.in_flight = false;
        }
        let shas: Vec<_> = state.commits.iter().map(|c| c.sha.as_str()).collect();
        if !shas.contains(&sha) {
            return false;
        }
        state.selection.click(sha, &shas, toggle, extend);
        state.cursor = Some(sha.to_owned());
        state.sync_selection();
        if let Some(key) = &state.opened
            && let Some(worker) = self.status_workers.get(&project)
        {
            if !state.files.contains_key(key) {
                worker.files(key.clone());
            }
            if !key.contains(',') && !state.contains.contains_key(key) {
                worker.contains(key.clone());
            }
        }
        true
    }

    /// A key pressed with the Log focused. Answers whether the panel
    /// took it.
    ///
    /// Arrows move the cursor and select one commit; Shift extends from
    /// the selection anchor. Enter selects the cursor alone. Escape clears
    /// the active filter or returns focus to the tab in front.
    pub(crate) fn log_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.active_project else {
            return false;
        };
        let Some(state) = self.logs.get(&project) else {
            return false;
        };
        let count = state.commits.len();
        let current = state
            .cursor
            .as_deref()
            .and_then(|sha| state.commits.iter().position(|c| c.sha == sha));

        match event.keystroke.key.as_str() {
            key @ ("down" | "up") => {
                let Some(index) = crate::pick::step(count, current, key == "down") else {
                    return true;
                };
                let sha = state.commits[index].sha.clone();
                self.select_log_commit(project, &sha, false, event.keystroke.modifiers.shift);
                if let Some(state) = self.logs.get_mut(&project) {
                    state.scroll.scroll_to_item(index, ScrollStrategy::Top);
                }
            }
            "enter" => {
                let sha = match current {
                    Some(index) => state.commits[index].sha.clone(),
                    None if count > 0 => state.commits[0].sha.clone(),
                    None => return true,
                };
                self.open_log_commit(project, &sha);
            }
            "escape" => {
                // A filter first, the focus second. Escape means *undo
                // the last narrowing*, and a person who has typed into
                // the field and pressed escape means the field.
                if !state.branch_query.is_empty() && state.typing == Typing::Branch {
                    self.clear_branch_query(cx);
                } else if !state.filter.text.is_empty() {
                    self.set_log_text(String::new(), cx);
                } else {
                    let focus = self
                        .active_body()
                        .and_then(|body| body.focus_handle(cx))
                        .unwrap_or_else(|| self.focus.clone());
                    window.focus(&focus, cx);
                }
            }
            // Anything else is the field: the arrows walk the list and
            // the letters go to the filter, which is why the field needs
            // no click to be typed into.
            _ => return false,
        }
        cx.notify();
        true
    }

    /// List every branch, the head of the current one, or one branch.
    ///
    /// A read and nothing more: `git log --all`, `git log`, `git log
    /// <branch>`. The working tree does not move, `HEAD` does not move,
    /// and the star in the column stays on the branch that is checked
    /// out — decision 3 of M8.3.2, which is spec decision 18 standing.
    pub(crate) fn show_branch_log(&mut self, scope: crate::git::Scope, cx: &mut Context<Self>) {
        let same = self.log().map(|state| state.scope == scope).unwrap_or(true);
        if same {
            return;
        }
        self.restart_log(cx, |state| {
            if let Some(name) = scope.branch() {
                state.recent.retain(|known| known != name);
                state.recent.insert(0, name.to_owned());
                state.recent.truncate(RECENT_BRANCHES);
            }
            state.scope = scope;
        });
    }

    /// Change what is asked for, and start the list again.
    ///
    /// **The pages go.** A filter is not a sieve over what is loaded — it
    /// is a different ask, and the second page of it has to come from git
    /// under the same filter or scrolling would mix two lists.
    fn restart_log(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut LogState)) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        change(state);
        state.navigation = None;
        state.navigation_error = None;
        state.epoch += 1;
        // **What is on screen stays there until the new page lands.**
        // Clearing here emptied the panel between one letter and the
        // next: the rows went, the note took their place, the opened
        // commit closed, and the whole tool blinked once per keystroke.
        // Nothing here is wrong to show meanwhile — it is the same log,
        // one narrowing behind — and `log_landed` replaces it whole.
        state.exhausted = false;
        state.error = None;
        state.scroll.scroll_to_item(0, ScrollStrategy::Top);
        self.ask_first_log_page(project);
        cx.notify();
    }

    /// Ask for the page a changed filter needs: the first one.
    ///
    /// Its own function because [`Self::ask_log_page`] asks for *the
    /// next* page — it counts what is loaded, and what is loaded is now
    /// deliberately still the old list.
    fn ask_first_log_page(&mut self, project: ProjectId) {
        let Some(state) = self.logs.get_mut(&project) else {
            return;
        };
        if let Some(worker) = self.status_workers.get(&project) {
            worker.log(
                state.epoch,
                state.scope.clone(),
                state.filter.clone(),
                0,
                PAGE,
            );
            state.in_flight = true;
        }
    }

    /// The branches containing a commit landed.
    ///
    /// A failure is dropped rather than shown: the footer is a nicety,
    /// and a repository mid-rebase answering badly should not put a
    /// notice in front of somebody reading a commit message.
    pub(crate) fn log_contains_landed(
        &mut self,
        project: ProjectId,
        sha: String,
        branches: Result<Vec<String>, GitError>,
    ) {
        if let Ok(branches) = branches
            && let Some(state) = self.logs.get_mut(&project)
        {
            state.contains.insert(sha, branches);
        }
    }

    /// Which field takes what is typed.
    pub(crate) fn log_typing(&self) -> Typing {
        self.log().map(|s| s.typing).unwrap_or_default()
    }

    /// Choose one, by clicking it.
    pub(crate) fn type_into_log(&mut self, into: Typing, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(state) = self.logs.get_mut(&project) {
            state.typing = into;
            cx.notify();
        }
    }

    /// What is typed into the branch column's field.
    pub(crate) fn branch_query(&self) -> String {
        self.log()
            .map(|s| s.branch_query.clone())
            .unwrap_or_default()
    }

    /// Whether the column's *Local* group is open.
    pub(crate) fn local_branches_open(&self) -> bool {
        self.log().map(|s| s.local_open).unwrap_or(true)
    }

    /// Open or close it.
    pub(crate) fn toggle_local_branches(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(state) = self.logs.get_mut(&project) {
            state.local_open = !state.local_open;
            cx.notify();
        }
    }

    /// Type into the column's field. **It narrows the column only** — it
    /// is not the toolbar's filter and does not ask git anything: the
    /// branches are already here, and re-walking the log to find a name
    /// in a list of twelve would be absurd.
    pub(crate) fn clear_branch_query(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(state) = self.logs.get_mut(&project) {
            state.branch_query.clear();
            cx.notify();
        }
    }

    /// The active project's own name, for the file tree's root row.
    pub(crate) fn active_project_name(&self) -> String {
        self.active_project
            .and_then(|id| self.desk.project(id))
            .map(|project| project.name.clone())
            .unwrap_or_default()
    }

    /// What the toolbar is narrowing by right now.
    pub(crate) fn log_filter(&self) -> crate::git::Filter {
        self.log()
            .map(|state| state.filter.clone())
            .unwrap_or_default()
    }

    /// What the log is listing right now.
    pub(crate) fn log_scope(&self) -> crate::git::Scope {
        self.log().map(|s| s.scope.clone()).unwrap_or_default()
    }

    /// The branches whose log was read lately.
    pub(crate) fn recent_branches(&self) -> Vec<String> {
        self.log().map(|s| s.recent.clone()).unwrap_or_default()
    }

    /// The window's own size, for a layer that must cover it.
    pub(crate) fn viewport(&self) -> gpui::Size<gpui::Pixels> {
        self.viewport
    }

    /// Narrow the list by an author, or by everyone.
    pub(crate) fn filter_log_author(&mut self, author: Option<String>, cx: &mut Context<Self>) {
        self.restart_log(cx, |state| {
            state.filter.author = author;
        });
    }

    /// Narrow it by a date git understands, or by none.
    pub(crate) fn filter_log_since(&mut self, since: Option<String>, cx: &mut Context<Self>) {
        self.restart_log(cx, |state| {
            state.filter.since = since;
        });
    }

    /// What was typed into the field.
    pub(crate) fn set_log_text(&mut self, text: String, cx: &mut Context<Self>) {
        self.restart_log(cx, |state| state.filter.text = text);
    }
    /// The authors of what is loaded.
    ///
    /// **Honest about being what is loaded**, not every author the
    /// repository has ever had: that would be a second walk of the whole
    /// history for a list of a dozen names, and the page in front of a
    /// person is the one they are filtering.
    pub(crate) fn log_authors(&self) -> Vec<String> {
        let Some(state) = self.log() else {
            return Vec::new();
        };
        let mut authors: Vec<String> = state
            .commits
            .iter()
            .map(|commit| commit.author.clone())
            .collect();
        authors.sort();
        authors.dedup();
        authors
    }

    /// Modifier clicks select without opening; a plain click keeps the preview behavior.
    pub(crate) fn log_file_click(
        &mut self,
        sha: &str,
        path: &Path,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.active_project.and_then(|p| self.logs.get_mut(&p)) else {
            return;
        };
        let rows = state.file_paths();
        state
            .file_selection
            .click(path, &rows, modifiers.platform, modifiers.shift);
        let focus = state.file_focus.clone();
        if !modifiers.platform && !modifiers.shift {
            self.open_diff(
                Some(path.to_path_buf()),
                Base::Commit(sha.to_owned()),
                window,
                cx,
            );
        }
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn log_files_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.log().is_some_and(|s| s.file_focus.is_focused(window)) {
            return false;
        }
        let Some(state) = self.active_project.and_then(|p| self.logs.get_mut(&p)) else {
            return false;
        };
        let paths = state.file_paths();
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        if state
            .file_selection
            .key(key, &paths, mods.platform, mods.shift)
        {
            if let Some(path) = &state.file_selection.focus
                && let Some(index) = state.file_tree.visible().iter().position(
                    |row| matches!(row, crate::files_tree::Row::File { path: p, .. } if p == path),
                )
            {
                state.file_scroll.scroll_to_item(index + 1);
            }
            cx.notify();
            return true;
        }
        if key == "c" && mods.platform {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                state.file_selection.copied_paths(),
            ));
        } else if key == "enter" {
            let sha = state.opened.clone().unwrap_or_default();
            let paths = state.file_selection.selected.clone();
            for path in paths {
                self.open_diff(Some(path), Base::Commit(sha.clone()), window, cx);
            }
        } else if key == "escape" {
            window.focus(&state.focus, cx);
        } else {
            return false;
        }
        true
    }
}

impl Root {
    /// Which of the Git tool's tabs is showing.
    pub(crate) fn git_face(&self) -> guild_session::GitFace {
        self.panels().git_face
    }

    /// Show one of them, and remember it with the project.
    ///
    /// A tab is a place a person left the tool in, like a dock or a
    /// column's width, so it is written where those are rather than kept
    /// for this run only.
    pub(crate) fn show_git_face(
        &mut self,
        face: guild_session::GitFace,
        cx: &mut gpui::Context<Self>,
    ) {
        self.with_panels(|panels| panels.git_face = face);
        // Opened at its end, where the last thing that happened is.
        if face == guild_session::GitFace::Console {
            self.show_console_end();
        }
        cx.notify();
    }

    /// Put the Console's last line in view.
    ///
    /// Applied at the next prepaint, so it can be asked for from anywhere
    /// — a git answer landing on the watch loop has no window, and does
    /// not need one.
    pub(crate) fn show_console_end(&self) {
        self.console_scroll.scroll_to_bottom();
    }

    /// Make the Console's editor, and give it the text the project's
    /// console holds now — 2026-09-23.
    ///
    /// **It grows to its text and the box around it scrolls**, rather
    /// than scrolling itself: the component moves its view only by
    /// following its caret, and the one public way to move the caret
    /// takes the keyboard too — a git answer would take it from the
    /// terminal the person is typing in. The box is ours, and ours is
    /// told to go to the end.
    pub(crate) fn ensure_console_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.git_face() != guild_session::GitFace::Console {
            return;
        }
        let offset = crate::system::utc_offset();
        let text = self
            .active_project
            .and_then(|id| self.consoles.get(&id))
            .map(|console| console.text(offset))
            .unwrap_or_default();
        let (editor, shown) = self.console_view.get_or_insert_with(|| {
            let editor = cx.new(|cx| {
                gpui_component::input::TextareaState::new(window, cx)
                    .auto_grow(1, CONSOLE_ROWS)
                    .soft_wrap(true)
            });
            (editor, String::new())
        });
        if *shown == text {
            return;
        }
        editor.update(cx, |state, cx| state.set_value(text.clone(), window, cx));
        *shown = text;
        self.console_scroll.scroll_to_bottom();
    }
}

impl LogState {
    /// Cache identity and visible rows move together; late replies are keyed by
    /// the immutable selection and cannot paint another selection's tree.
    fn sync_selection(&mut self) {
        let shas: Vec<_> = self.commits.iter().map(|c| c.sha.as_str()).collect();
        let key = if self.selection.shas.is_empty() {
            None
        } else {
            crate::push_dialog::comparison(&self.selection.shas, &shas)
        };
        if self
            .cursor
            .as_ref()
            .is_some_and(|sha| !shas.contains(&sha.as_str()))
        {
            self.cursor = self.selection.shas.first().cloned();
        }
        if key == self.opened {
            return;
        }
        self.opened = key;
        self.speed = None;
        self.file_selection = Default::default();
        self.file_tree = self
            .opened
            .as_ref()
            .and_then(|k| self.files.get(k))
            .and_then(|f| f.as_ref().ok())
            .map(|f| crate::files_tree::Tree::new(f))
            .unwrap_or_default();
        self.file_scroll
            .set_offset(gpui::point(gpui::px(0.0), gpui::px(0.0)));
    }

    pub(crate) fn file_paths(&self) -> Vec<std::path::PathBuf> {
        self.file_tree.paths()
    }
}

impl Root {
    pub(crate) fn toggle_log_directory(
        &mut self,
        key: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.active_project.and_then(|p| self.logs.get_mut(&p)) {
            state.file_tree.toggle(key);
            state.file_selection.retain(&state.file_paths());
            state.speed = None;
            window.focus(&state.file_focus, cx);
            cx.notify();
        }
    }
    pub(crate) fn focus_and_fold_log_files(
        &mut self,
        expand: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.log() {
            // The focused speed-search input is about to leave the render tree.
            window.focus(&state.file_focus, cx);
        }
        self.fold_log_files(expand, cx);
    }

    fn fold_log_files(&mut self, expand: bool, cx: &mut Context<Self>) {
        if let Some(state) = self.active_project.and_then(|p| self.logs.get_mut(&p)) {
            if expand {
                state.file_tree.expand_all();
            } else {
                state.file_tree.collapse_all();
            }
            state.file_selection.retain(&state.file_paths());
            state.speed = None;
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests;
