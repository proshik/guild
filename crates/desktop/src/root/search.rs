//! Search as a tool window: the thread that walks, and what the panel
//! knows. M8.7.
//!
//! The walk is `search::Walk` since M8.9 — a thread, its channel and its
//! generation, which the tool, the Find in Files popup and Search
//! Everywhere's *Text* tab each hold one of (decision 12). The window reads
//! it on the watch loop's clock. M8.7's task 0 measured a search of a tree
//! with no ignore file at **11.4 to 12.7 seconds** — a search that answered
//! only when it had finished would be a window frozen for that long, and
//! the row a person wanted was usually in the first ten.
//!
//! Here is what the panel keeps around its walk: the query, the
//! highlight, the folds, what the disk has said.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use gpui::{Context, FocusHandle, KeyDownEvent, UniformListScrollHandle, Window};
use guild_session::ProjectId;

use crate::Root;
use crate::search::{self, Query, Toggle};
use crate::search_tree::{Kept, Lit};

/// Which of the toolbar's two fields the letters go to.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Typing {
    /// What is being looked for.
    #[default]
    Text,
    /// Which files to look in.
    Mask,
}

/// One project's search: what was asked, what has come back, and the
/// thread that is still looking.
pub(crate) struct SearchState {
    /// What is being looked for, as the toolbar has it.
    pub query: Query,
    /// Which field the keyboard is in.
    pub typing: Typing,
    /// Whether the query is lit as selected whole, so that the next edit
    /// replaces it rather than adding to it — `cmd-shift-f` over a query
    /// already there. The shared input receives this as a selection request;
    /// it owns subsequent caret movement and partial selection.
    ///
    /// **Put down by anything that is not typing into the query**: a
    /// click in a field, an arrow, Tab, Escape, a toggle, the scope, a
    /// click on a row, and the panel losing the focus. A light that
    /// outlived the gesture would make the first letter after coming back
    /// by some other route throw the query away.
    pub replacing: bool,
    /// The walk: the hits that have landed so far, in the order `ignore`
    /// walks — so a person sees the same list twice for the same query —
    /// how many files, whether a limit stopped it (task 0: a common word is
    /// 16,765 hits here and 503,629 in a tree with no ignore file), whether
    /// it is still walking, and what the regex crate or `ignore` said about
    /// a query nobody can compile, **in their words, not ours**.
    ///
    /// **The list is not blanked when a search starts** — the walk keeps
    /// the old hits until the new one has something to put in their place.
    pub walk: search::Walk,
    /// Immutable data shared by the virtual list and keyboard navigation.
    /// Only result replacement/appending, folding and path moves invalidate it.
    presentation: RefCell<Option<Rc<SearchPresentation>>>,
    /// Which row the keyboard is on — an index into the rows as they
    /// are drawn, which is why it is found again (`held`) or settled at
    /// every point the list changes under it.
    pub selected: Option<usize>,
    pub files: crate::file_selection::FileSelection,
    /// Enter opens an explicit file selection; ordinary row navigation folds groups.
    pub explicit_file_selection: bool,
    pub results_focus: FocusHandle,
    /// What the highlight is held on, by what the row is: set when the
    /// person moves it or folds, and when a kept hit is found again.
    /// `None` while the walk's first hit leads — decision 2 — which is
    /// lit again on every batch, because a batch can move it.
    held: Option<Lit>,
    /// The directories and files folded shut in the results, by path.
    pub folded: std::collections::HashSet<std::path::PathBuf>,
    pub scroll: UniformListScrollHandle,
    pub focus: FocusHandle,
    /// When the disk first and last said something this answer has not
    /// seen — `None` while the answer is current. M5.11, decision 4.
    stale: Option<(Instant, Instant)>,
    /// The files the disk has named since the answer on screen was asked
    /// for, each with the generation that was asked when it was heard. A
    /// click on a hit in one of them looks for its match again first.
    ///
    /// **Forgotten when an answer replaces the rows, not when a walk
    /// starts.** The old rows stay on screen for the whole walk, and a
    /// click on one of them in those seconds is still a click on a place
    /// that may have moved; a file heard after the walk started is kept
    /// through the replacement, because that walk may have read it before
    /// the write.
    touched: HashMap<PathBuf, u64>,
    /// Whether the answer went stale while the panel was not on screen,
    /// so that drawing it again asks again.
    away: bool,
    /// The hit the highlight was on when a refresh was asked for, until
    /// the new answer brings it back or the person chooses another.
    keep: Option<Kept>,
    /// Puts the query's light down when the panel loses the focus.
    _blurred: gpui::Subscription,
}

pub(crate) struct SearchPresentation {
    pub hits: Vec<search::Hit>,
    pub rows: Rc<Vec<crate::search_tree::Row>>,
    pub folded: std::collections::HashSet<PathBuf>,
}

/// Why a walk was started again — what the debug log says, and what
/// window check 3 counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Why {
    /// The person changed the query: the words, a toggle, the scope.
    Typed,
    /// The panel came back with a query remembered from the last run.
    Restored,
    /// The disk changed under an answer on screen, and went quiet.
    Refreshed,
    /// The disk changed under an answer while it was out of sight.
    DrawnAgain,
}

impl Why {
    fn said(self) -> &'static str {
        match self {
            Self::Typed => "typed",
            Self::Restored => "restored",
            Self::Refreshed => "refreshed",
            Self::DrawnAgain => "drawn again",
        }
    }
}

impl Root {
    /// The active project's search state, made the first time the panel
    /// is drawn. Nothing is searched here — an empty query is not a
    /// question.
    pub(crate) fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        if let Some(state) = self.searches.get(&project) {
            // **The one re-ask a render makes**, and only for an answer
            // that went stale while nobody could see it. A state that is
            // merely stale is the watch loop's: this runs on every frame
            // the tool is up, and a re-ask here would restart the walk on
            // the frame after every batch. A walk still running is let
            // finish, and `poll_search` refreshes after it.
            if state.away && !state.walk.running() {
                self.restart_search_because(project, Why::DrawnAgain, cx);
            }
            return;
        }
        // What this project was last searched for. A search is a walk
        // that costs milliseconds and can be run again; the words and
        // the toggles are work a person did, and coming back to an empty
        // field is that work thrown away.
        let remembered = self
            .desk
            .project(project)
            .map(|p| guild_session::project_state::load(&p.path).search)
            .unwrap_or_default();
        let query = Query {
            text: remembered.query,
            case_sensitive: remembered.match_case,
            whole_word: remembered.whole_word,
            regular_expression: remembered.regex,
            // A mask the Find in Files popup turned off is not applied
            // here either — M8.9 — and stays in the file.
            mask: if remembered.mask_off {
                String::new()
            } else {
                remembered.mask
            },
            // **The scope is not remembered, and the words are.** A
            // scope is a gesture aimed at a row — *Find in Files…* on
            // `crates/desktop/src` — and a window that came back
            // searching inside a directory nobody had pointed at would
            // be answering a question the person did not ask. The query
            // itself is work they did; where they last pointed is not.
            scope: None,
        };
        let restore = !query.is_empty();
        let focus = cx.focus_handle();
        let blurred = cx.on_blur(&focus, window, move |root, _window, cx| {
            if let Some(state) = root.searches.get_mut(&project)
                && state.replacing
            {
                state.replacing = false;
                cx.notify();
            }
        });
        self.searches.insert(
            project,
            SearchState {
                query,
                typing: Typing::default(),
                replacing: false,
                walk: search::Walk::default(),
                presentation: RefCell::default(),
                selected: None,
                files: Default::default(),
                explicit_file_selection: false,
                results_focus: cx.focus_handle(),
                held: None,
                folded: std::collections::HashSet::new(),
                scroll: UniformListScrollHandle::new(),
                focus,
                stale: None,
                touched: HashMap::new(),
                away: false,
                keep: None,
                _blurred: blurred,
            },
        );
        // **Asked again, not restored.** The hits belong to a tree as it
        // was; a list read back from a file would be an answer about
        // files that have changed since — and the walk is ten
        // milliseconds here, which is cheaper than the lie.
        if restore {
            self.restart_search_because(project, Why::Restored, cx);
        }
    }

    /// What the active project's panel is drawing, if it has one.
    pub(crate) fn search_state(&self) -> Option<&SearchState> {
        self.active_project
            .and_then(|project| self.searches.get(&project))
    }

    /// `cmd-shift-f`: show the panel and put the keyboard in its field.
    ///
    /// **The stroke is a question, not a place.** Opening the panel and
    /// leaving the focus wherever it was would make the gesture two
    /// gestures, and the second one a click.
    ///
    /// And it **reveals** rather than shows: `show_tool` is a toggle, and
    /// a toggle is right for *the dock that has the Log, on it* — but
    /// pressing *find in files* twice must not mean *stop finding*.
    /// Found in the window, where the second `cmd-shift-f` put the panel
    /// away and the letters went to the terminal behind it.
    ///
    /// **The keyboard goes to the query, and the query is the selection**
    /// — M5.11, decision 5. A person who selects a name in a file and
    /// presses the stroke means *search for this*, which is what editors
    /// commonly do; and one who presses it over an old query
    /// means *a new one*, so the old query is lit and the first letter
    /// replaces it. The field is always the query, even when the mask had
    /// the keyboard last: the stroke is *find*, not *where I was*.
    pub(crate) fn find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_tool(guild_session::Tool::Search, cx);
        self.ensure_search(window, cx);
        let selection = self.selection_for_search(window, cx);
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        state.typing = Typing::Text;
        let asked = match selection {
            Some(text) if text != state.query.text => {
                state.query.text = text;
                true
            }
            _ => false,
        };
        state.replacing = !state.query.text.is_empty();
        let focus = state.focus.clone();
        focus_query(&focus, window, cx);
        if asked {
            self.restart_search(project, cx);
        }
        cx.notify();
    }

    /// Search this project for this query in the Search tool, with the
    /// keyboard in its field — where Find in Files' *Open in Find Window*
    /// hands its question. M8.9, decision 12.
    ///
    /// **Revealed, not toggled**, as `find_in_files` is; and the query is the
    /// person's, so it is written as typed.
    pub(crate) fn search_in_window(
        &mut self,
        project: ProjectId,
        query: Query,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_project != Some(project) {
            return;
        }
        self.reveal_tool(guild_session::Tool::Search, cx);
        self.ensure_search(window, cx);
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        state.query = query;
        state.typing = Typing::Text;
        state.replacing = false;
        let focus = state.focus.clone();
        focus_query(&focus, window, cx);
        self.restart_search(project, cx);
    }

    /// The selection in the file tab in front, when it is one line with
    /// something on it — of any origin, because a name in a dependency's
    /// source is as much a thing to search for as one in the project.
    ///
    /// **Read through the component's input handler**, the one door to a
    /// selection 0.5.1 makes public: `selected_text_range` and
    /// `text_for_range` of `EntityInputHandler`, in UTF-16 as the platform
    /// speaks. The editor is its own entity, so updating it from `Root`'s
    /// action handler is not an entity updating itself.
    ///
    /// A selection over several lines is not taken, for the reason a
    /// paste is flattened: the field is one line. One trailing newline is
    /// forgiven, because a triple-click selects the line with its end.
    ///
    /// **Only while the editor holds the focus.** A
    /// selection left in the tab in front is not what a person pressing
    /// the stroke from the panel means — the second `cmd-shift-f` over a
    /// query they typed would put the old selection back and walk again.
    pub(crate) fn selection_for_search(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        use gpui::EntityInputHandler;

        let crate::Body::File { tab, .. } = self.active_body()? else {
            return None;
        };
        let editor = tab.read(cx).editor().cloned()?;
        if !gpui::Focusable::focus_handle(editor.read(cx), cx).contains_focused(window, cx) {
            return None;
        }
        let text = editor.update(cx, |state, cx| {
            let range = state.selected_text_range(false, window, cx)?.range;
            if range.is_empty() {
                return None;
            }
            let mut adjusted = None;
            state.text_for_range(range, &mut adjusted, window, cx)
        })?;
        let text = text
            .strip_suffix('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .unwrap_or(&text);
        (!text.is_empty() && !text.contains(['\n', '\r'])).then(|| text.to_owned())
    }

    /// Change the query, and start the walk again.
    ///
    /// Every keystroke is a new search. What is on screen **stays** until
    /// the new hits land — M8.3.4's rule, learned from a Log that blinked
    /// once per letter: the old list is the same answer one narrowing
    /// behind, and blanking it says the panel broke.
    pub(crate) fn clear_search_field(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        match state.typing {
            Typing::Text => state.query.text.clear(),
            Typing::Mask => state.query.mask.clear(),
        }
        state.replacing = false;
        self.restart_search(project, cx);
    }

    /// Move the keyboard between the two fields.
    pub(crate) fn type_search_into(&mut self, typing: Typing, cx: &mut Context<Self>) {
        if let Some(project) = self.active_project
            && let Some(state) = self.searches.get_mut(&project)
        {
            state.typing = typing;
            // A click in either field, or Tab, is a choice of where the
            // letters go, and the light on the query does not survive it.
            state.replacing = false;
            cx.notify();
        }
    }

    /// Narrow the search to one directory, or widen it back to the
    /// project. M8.8.3, from *Find in Files…* on a tree row and from the
    /// cross the toolbar draws beside the scope.
    ///
    /// The project's own row is the empty path, which is the project:
    /// scoping to it would draw a chip saying nothing.
    pub(crate) fn scope_search(
        &mut self,
        scope: Option<std::path::PathBuf>,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        let scope = scope.filter(|path| !path.as_os_str().is_empty());
        if state.query.scope == scope {
            return;
        }
        state.query.scope = scope;
        state.replacing = false;
        self.restart_search(project, cx);
    }

    /// Turn one of the three toggles, and search again.
    pub(crate) fn toggle_search(&mut self, which: Toggle, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        which.turn(&mut state.query);
        state.replacing = false;
        self.restart_search(project, cx);
    }

    /// Keep the query with its project, in the window and on the disk.
    ///
    /// The shape `remember_command` has, and for the same reason: the map
    /// is what the panel reads every frame, and `.guild/` is where it
    /// survives a restart.
    fn remember_search(&mut self, project: ProjectId) {
        let Some(query) = self.searches.get(&project).map(|state| state.query.clone()) else {
            return;
        };
        self.write_search(project, |search| {
            search.query = query.text;
            search.match_case = query.case_sensitive;
            search.whole_word = query.whole_word;
            search.regex = query.regular_expression;
            // The tool has no checkbox and walks with the mask as
            // applied: an empty field over a mask the Find in Files popup
            // turned off keeps that mask, still off. M8.9.
            let (mask, off) =
                crate::find_in_files::tool_mask(&search.mask, search.mask_off, &query.mask);
            search.mask = mask;
            search.mask_off = off;
        });
    }

    /// Change what a project remembers of its search, and write it when it
    /// changed — the one door the Search tool and the Find in Files popup
    /// both write by, M8.9's decision 12.
    ///
    /// **The whole file is read again before it is written**, because
    /// `save` serialises all of `ProjectState` and a stale copy would drop
    /// the tabs, the sessions and the panels; and **what `change` does not
    /// set is kept from the file** — the popup's history, its masks and
    /// whether the mask is off, which a literal of the tool's five fields
    /// would wipe on every save.
    pub(crate) fn write_search(
        &mut self,
        project: ProjectId,
        change: impl FnOnce(&mut guild_session::Search),
    ) {
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let mut file = guild_session::project_state::load(&path);
        let mut remembered = file.search.clone();
        change(&mut remembered);
        if file.search == remembered {
            return;
        }
        file.search = remembered;
        if let Err(err) = guild_session::project_state::save(&path, &file) {
            tracing::warn!(project = %path.display(), %err, "the query was not remembered");
        }
    }

    /// Stop whatever is walking and start again from what the toolbar
    /// now says — because the person changed it.
    pub(crate) fn restart_search(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        self.restart_search_because(project, Why::Typed, cx);
    }

    /// Stop whatever is walking and start again, saying why.
    fn restart_search_because(&mut self, project: ProjectId, why: Why, cx: &mut Context<Self>) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        tracing::debug!(project = %root.display(), why = why.said(), "search restarted");
        state.stale = None;
        state.away = false;
        // A refresh keeps the hit the highlight is on; a new question
        // has no old place to go back to.
        if why != Why::Refreshed {
            state.keep = None;
        }
        // **Only the person's change is written.** A refresh, a panel
        // drawn again and a restored query ask the words that are already
        // on the disk, and reading the state file to find that out would
        // be a disk read on the watch loop's clock and inside a render.
        if why == Why::Typed {
            self.remember_search(project);
        }
        let buffers = self.project_search_buffers(project, cx);
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };

        let query = state.query.clone();
        let empty = query.is_empty();
        // The old walk is dropped here, which stops its thread.
        state.walk.restart_documents(&root, query, buffers);
        if empty {
            state.presentation.get_mut().take();
            // Nothing typed is not a search, and it is also not a result
            // of one: the list goes, because there is no older answer
            // that this is a narrowing of.
            state.selected = None;
            state.files = Default::default();
            state.explicit_file_selection = false;
            state.held = None;
            state.touched.clear();
        }
        cx.notify();
    }

    /// Whatever the walking threads have said, on the watch loop's
    /// clock — beside git's answers, the runs and the language servers.
    ///
    /// **And the only place an answer on screen is refreshed** — M5.11,
    /// decision 4. The render loop draws the panel on every frame and
    /// `apply_batches` notifies after every batch, so a re-ask from a
    /// render would restart the walk once per batch; here it is once per
    /// quiet, and never over a walk that is still running.
    pub(crate) fn poll_search(&mut self, cx: &mut Context<Self>) {
        let mut moved = false;
        for state in self.searches.values_mut() {
            moved |= state.take();
        }

        let now = Instant::now();
        let stale: Vec<ProjectId> = self
            .searches
            .iter()
            .filter(|(_, state)| state.stale.is_some())
            .map(|(project, _)| *project)
            .collect();
        let mut due = Vec::new();
        for project in stale {
            let visible = self.search_visible(project);
            let Some(state) = self.searches.get_mut(&project) else {
                continue;
            };
            let Some((first, last)) = state.stale else {
                continue;
            };
            if !visible {
                // Nobody is reading it: no walk now, one when it is drawn.
                state.away = true;
                continue;
            }
            if search::refresh_due(first, last, now, state.walk.running()) {
                if state.keep.is_none() {
                    state.keep = state.lit_hit();
                }
                due.push(project);
            }
        }
        for project in due {
            self.restart_search_because(project, Why::Refreshed, cx);
            moved = true;
        }
        if moved {
            cx.notify();
        }
    }

    /// Whether a project's Search panel is on screen: the project in
    /// front, and the tool up in its dock.
    fn search_visible(&self, project: ProjectId) -> bool {
        self.active_project == Some(project) && self.tool_is_up(guild_session::Tool::Search)
    }

    /// What the disk said about a project, heard by its search: a batch
    /// that touches a file inside the query's scope marks the answer
    /// stale, and every file it names is remembered as touched. Called by
    /// the watch loop for every batch.
    ///
    /// **Marked here, asked in `poll_search`.** A batch arrives every
    /// 100 ms while an agent writes; this only moves the clock.
    pub(crate) fn search_heard(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        if state.query.is_empty() || !search::touches(&state.query, batch.files()) {
            return;
        }
        let generation = state.walk.generation();
        for file in batch.files() {
            state.touched.insert(file.clone(), generation);
        }
        let now = Instant::now();
        state.stale = Some(match state.stale {
            Some((first, _)) => (first, now),
            None => (now, now),
        });
    }

    /// A key pressed with the Search panel focused. Answers whether the
    /// panel took it.
    pub(crate) fn search_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self.active_project else {
            return false;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return false;
        };
        // An arrow moves something other than the query, and so does Tab:
        // the light on the query is dropped and the text is kept.
        if matches!(
            event.keystroke.key.as_str(),
            "up" | "down" | "left" | "right" | "tab"
        ) && state.replacing
        {
            state.replacing = false;
            cx.notify();
        }
        match event.keystroke.key.as_str() {
            key @ ("down" | "up" | "home" | "end") => self.step_search(key, cx),
            // `enter` is the row's click: a hit opens its file, a file or
            // a directory folds. The Commit panel's keys and the Log's,
            // on the same arithmetic.
            "enter" => match self.selected_row() {
                Some((_, crate::search_tree::Row::Hit { at, .. })) => {
                    self.open_search_hit(at, window, cx)
                }
                Some((_, row)) => {
                    if let Some(path) = crate::search_tree::foldable(&row) {
                        self.toggle_search_fold(path, cx);
                    }
                }
                None => {}
            },
            "tab" => {
                let next = match self.searches[&project].typing {
                    Typing::Text => Typing::Mask,
                    Typing::Mask => Typing::Text,
                };
                self.type_search_into(next, cx);
            }
            "escape" if self.searches[&project].replacing => {
                // A lit query is put down before anything is undone: the
                // light was the stroke's offer, and escape refuses the
                // offer, not the words.
                if let Some(state) = self.searches.get_mut(&project) {
                    state.replacing = false;
                }
                cx.notify();
            }
            "escape" => {
                // The query first, the focus second: escape means *undo
                // the last narrowing*, and a person who has typed and
                // pressed escape means what they typed.
                let empty = {
                    let state = &self.searches[&project];
                    match state.typing {
                        Typing::Text => state.query.text.is_empty(),
                        Typing::Mask => state.query.mask.is_empty(),
                    }
                };
                if empty {
                    let focus = self
                        .active_body()
                        .and_then(|body| body.focus_handle(cx))
                        .unwrap_or_else(|| self.focus.clone());
                    window.focus(&focus, cx);
                    cx.notify();
                } else {
                    self.clear_search_field(cx);
                }
            }
            _ => return false,
        }
        true
    }

    /// Shared rows stay current until the result data or folds change.
    pub(crate) fn search_rows(&self) -> Rc<Vec<crate::search_tree::Row>> {
        self.search_state()
            .map(|state| state.presentation().rows.clone())
            .unwrap_or_default()
    }

    /// Fold or unfold whatever this row is about. A hit does not fold —
    /// it is the thing the folding is about — so a click on one opens
    /// its file instead.
    pub(crate) fn toggle_search_fold(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        // A click on a row is a choice, and a kept hit arriving later
        // must not take the highlight away from it.
        state.keep = None;
        state.replacing = false;
        let before = state.presentation().rows.clone();
        let lit = state
            .selected
            .and_then(|index| crate::search_tree::lit_of(&before, state.walk.hits(), index));
        if !state.folded.remove(&path) {
            state.folded.insert(path);
        }
        state.presentation.get_mut().take();
        // The list just changed under the highlight: it stays on its row
        // when that row is still drawn, and otherwise settles, because a
        // highlight past the end answers nothing to `enter` — the rule
        // the branch popup learned. Either way it is the person's now.
        let rows = state.presentation().rows.clone();
        state.selected = crate::search_tree::lit_row(&rows, state.walk.hits(), lit.as_ref())
            .filter(|_| lit.is_some())
            .or_else(|| crate::pick::settle(rows.len(), state.selected));
        let had_file_cursor = state.files.focus.is_some();
        state.files.retain(&search_file_paths(&rows));
        if had_file_cursor && state.files.focus.is_none() {
            state.selected = None;
        }
        state.held = state
            .selected
            .and_then(|index| crate::search_tree::lit_of(&rows, state.walk.hits(), index));
        cx.notify();
    }

    /// Open a hit: its file, at its match, with the match selected.
    ///
    /// **The caret goes through `go_to_place` to the match's column**, so
    /// Back returns from it and a restored tab lands too. **The selection
    /// is the component's own `ui::SelectRight`, dispatched by its name**
    /// once per character of the match — M5.11, decision 6. Every
    /// `select_*` on `InputState` is `pub(super)` in 0.5.1 and the action
    /// type sits in a `pub(crate)` module, so GPUI's registry is the one
    /// public way to it; the price is a string an upgrade can break
    /// silently, which is why it is one constant, `editor::SELECT_RIGHT`,
    /// with a test that finds it registered. It is sent on a later frame,
    /// to the editor's own node rather than through the keymap, and only
    /// while the caret stands where the tab was sent (`select_on_landing`).
    ///
    /// The count is characters, which is what `select_right` moves by. A
    /// match that begins or ends inside a combining sequence selects one
    /// character more or less than the eye expects; that is not worked
    /// around.
    pub(crate) fn open_search_hit(
        &mut self,
        at: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        state.keep = None;
        state.replacing = false;
        let Some(hit) = state.walk.hits().get(at) else {
            return;
        };
        // **A hit whose file moved since the answer lands where its text
        // is now** — M5.11, decision 4. The refresh is coming, but a click
        // in the pause before it is a click on a place that may have
        // moved; one file, read on the click, which the tab is about to
        // read anyway.
        let (mut line, mut column) = (hit.line, hit.column);
        if state.touched.contains_key(&hit.path)
            && let Ok(text) = std::fs::read_to_string(root.join(&hit.path))
            && let Some(now) = search::relocate(&text, &state.query, hit)
        {
            (line, column) = now;
        }
        let chars = hit
            .ranges
            .first()
            .and_then(|range| hit.text.get(range.clone()))
            .map_or(0, |matched| matched.chars().count());
        let path = hit.path.clone();
        self.open_file_at(path.clone(), line as usize, column, window, cx);
        let place = crate::root::history::Place::new(path, line.saturating_sub(1), column);
        self.select_on_landing(project, place, chars, window, cx);
    }

    /// Select `chars` characters from the caret, once the tab a hit opened
    /// has its caret at `place`.
    ///
    /// **On a later frame, and outside `Root`'s update.** The callback
    /// runs between frames: it looks the editor up and reads its caret
    /// inside `Root`'s update, and dispatches outside it, so no entity is
    /// updated inside itself. `dispatch_action` reaches a node of the frame
    /// already drawn, so a tab that has not been drawn yet takes nothing;
    /// the selection is read back, and an empty one is tried again on the
    /// next frame. So is a tab with no editor yet — a restored tab gets
    /// one at its first render — and a caret not yet at the place, because
    /// `go_to_place` lands a restored tab's caret at that same render.
    /// Source still loading in the background retains its selection with the
    /// destination, without consuming this frame budget. Once it has landed,
    /// after `tries` frames a caret elsewhere is a person who has moved.
    pub(crate) fn select_on_landing(
        &mut self,
        project: ProjectId,
        place: crate::root::history::Place,
        chars: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.retain_loading_selection(project, &place, chars, cx) {
            return;
        }
        self.select_on_landing_tries(project, place, chars, LANDING_TRIES, window, cx);
    }

    fn select_on_landing_tries(
        &mut self,
        project: ProjectId,
        place: crate::root::history::Place,
        chars: usize,
        tries: u8,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if chars == 0 || tries == 0 {
            return;
        }
        let root = cx.entity().downgrade();
        window.on_next_frame(move |window, cx| {
            let Some(root) = root.upgrade() else {
                return;
            };
            let editor = root.update(cx, |root, cx| {
                root.can_land_in_file(project, &place.path, cx)
                    .then(|| root.file_editor_at(project, &place.path, cx))
                    .flatten()
            });
            let ready = editor.filter(|editor| {
                let state = editor.read(cx);
                let at = state.cursor_position();
                at.line == place.line && at.character == place.column
            });
            let selected = ready.is_some_and(|editor| {
                let action = match cx.build_action(crate::editor::SELECT_RIGHT, None) {
                    Ok(action) => action,
                    Err(err) => {
                        if !SELECT_RIGHT_MISSING.swap(true, Ordering::Relaxed) {
                            tracing::warn!(
                                %err,
                                action = crate::editor::SELECT_RIGHT,
                                "a search hit's match cannot be selected"
                            );
                        }
                        // Nothing to try again with: the caret stays.
                        return true;
                    }
                };
                let handle = gpui::Focusable::focus_handle(editor.read(cx), cx);
                for _ in 0..chars {
                    handle.dispatch_action(action.as_ref(), window, cx);
                }
                editor.update(cx, |state, cx| {
                    use gpui::EntityInputHandler;
                    state
                        .selected_text_range(false, window, cx)
                        .is_some_and(|selection| !selection.range.is_empty())
                })
            });
            if !selected {
                root.update(cx, |root, cx| {
                    root.select_on_landing_tries(project, place, chars, tries - 1, window, cx);
                });
            }
        });
    }

    /// Where the highlight is, and what it is on.
    fn selected_row(&self) -> Option<(usize, crate::search_tree::Row)> {
        let state = self.search_state()?;
        let index = state.selected?;
        let rows = self.search_rows();
        rows.get(index).cloned().map(|row| (index, row))
    }

    /// Move the highlight, and take the list with it.
    fn step_search(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let rows = self.search_rows();
        let Some(state) = self.searches.get_mut(&project) else {
            return;
        };
        let next = match key {
            "home" => (!rows.is_empty()).then_some(0),
            "end" => rows.len().checked_sub(1),
            _ => crate::pick::step(rows.len(), state.selected, key == "down"),
        };
        let Some(index) = next else {
            return;
        };
        state.selected = Some(index);
        state.files = Default::default();
        state.explicit_file_selection = false;
        if let crate::search_tree::Row::File { path, .. } = &rows[index] {
            state
                .files
                .click(path, &search_file_paths(&rows), false, false);
        }
        // The person has chosen: a kept hit arriving later stays where
        // it lands and does not take the highlight, and a batch landing
        // later finds this row wherever it moves.
        state.keep = None;
        state.held = crate::search_tree::lit_of(&rows, state.walk.hits(), index);
        // Applied during prepaint, so it can be asked for straight from
        // the key handler — before the new row heights exist.
        state
            .scroll
            .scroll_to_item(index, gpui::ScrollStrategy::Top);
        cx.notify();
    }

    /// Forget a project's search when the project closes — and stop its
    /// thread, which is what dropping the worker does.
    pub(crate) fn forget_search(&mut self, project: ProjectId) {
        self.searches.remove(&project);
    }

    /// Stop every walk. Called where every other process the window owns
    /// is let go of, because a thread reading ninety thousand files is
    /// one of them.
    pub(crate) fn stop_every_search(&mut self) {
        for state in self.searches.values_mut() {
            state.walk.stop();
        }
    }
}

/// How many frames a hit's selection waits for its tab's editor and caret
/// before it gives up. M5.11, decision 6.
const LANDING_TRIES: u8 = 5;

/// Whether the component's select-right has already been found missing, so
/// that the warning is said once and not per click.
static SELECT_RIGHT_MISSING: AtomicBool = AtomicBool::new(false);

/// Give the query the keyboard — now, and again on the next frame.
///
/// **The second time is for a tool that has never been drawn.** With no tab
/// open, the root takes the focus back at its render unless a dock holds it,
/// and "holds" is asked of the frame already drawn — which a Search tool
/// revealed by this very stroke is not in yet. So the first `cmd-5` of a
/// window with no tab lit the field and every letter went to the root. By
/// the next frame the field is drawn, the dock holds the focus, and the
/// root leaves it there. Found by M5.11's window checks.
fn focus_query(focus: &gpui::FocusHandle, window: &mut Window, cx: &mut gpui::App) {
    window.focus(focus, cx);
    let focus = focus.clone();
    window.on_next_frame(move |window, cx| window.focus(&focus, cx));
}

impl SearchState {
    pub(crate) fn presentation(&self) -> Rc<SearchPresentation> {
        self.presentation
            .borrow_mut()
            .get_or_insert_with(|| {
                Rc::new(SearchPresentation {
                    hits: self.walk.hits().to_vec(),
                    rows: Rc::new(crate::search_tree::of(self.walk.hits(), &self.folded)),
                    folded: self.folded.clone(),
                })
            })
            .clone()
    }

    /// The hit the highlight is on, as something a new answer can find
    /// again — `None` when the lit row is a directory or a file.
    fn lit_hit(&self) -> Option<Kept> {
        let rows = self.presentation().rows.clone();
        match crate::search_tree::lit_of(&rows, self.walk.hits(), self.selected?)? {
            Lit::Hit(kept) => Some(kept),
            Lit::Path(_) => None,
        }
    }

    /// Everything this project's walk has said since the last look, and
    /// the highlight carried through it. Answers whether anything changed.
    fn take(&mut self) -> bool {
        let before = self.walk.answered();
        if !self.walk.take() {
            return false;
        }
        self.presentation.get_mut().take();
        if self.walk.answered() != before {
            // **The new answer's rows replace the old ones**: the lit row
            // was a row of the old list, and what was heard about the old
            // answer's files before this walk was asked for is no longer
            // news about anything on screen.
            self.selected = None;
            self.files = Default::default();
            self.explicit_file_selection = false;
            self.held = None;
            let generation = self.walk.generation();
            self.touched.retain(|_, heard| *heard >= generation);
        }
        if self.walk.refused().is_none() {
            let rows = self.presentation().rows.clone();
            // **A refresh keeps the highlight on its hit**, found by path
            // and text in whichever batch brings it — M5.11, decision 4 —
            // and from then on the highlight is held on it. Until something
            // holds it, **the walk's first hit is lit on every batch**, not
            // row 0, which is a directory or a file and folds on Enter —
            // decision 2.
            //
            // **Found again by what it is, never kept by its number.** Each
            // batch is one file, the walk's order is not the tree's, and a
            // later file can put rows in front of the lit one or split a
            // collapsed chain; the number would then be on a file. Only a
            // held row that is not drawn — folded away — settles.
            if let Some(kept) = self.keep.take_if(|kept| {
                crate::search_tree::row_of_kept(&rows, self.walk.hits(), kept).is_some()
            }) {
                self.held = Some(Lit::Hit(kept));
            }
            self.selected =
                crate::search_tree::lit_row(&rows, self.walk.hits(), self.held.as_ref())
                    .or_else(|| crate::pick::settle(rows.len(), self.selected));
        }
        if !self.walk.running() {
            // A kept hit this walk never brought back is gone; the
            // highlight stays where it is.
            self.keep = None;
        }
        true
    }
}

/// Every project's search, by project.
pub(crate) type Searches = HashMap<ProjectId, SearchState>;

fn search_file_paths(rows: &[crate::search_tree::Row]) -> Vec<PathBuf> {
    rows.iter()
        .filter_map(|r| match r {
            crate::search_tree::Row::File { path, .. } => Some(path.clone()),
            _ => None,
        })
        .collect()
}

impl Root {
    pub(crate) fn search_file_click(
        &mut self,
        path: PathBuf,
        mods: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let rows = self.search_rows();
        let Some(state) = self.active_project.and_then(|p| self.searches.get_mut(&p)) else {
            return;
        };
        state
            .files
            .click(&path, &search_file_paths(&rows), mods.platform, mods.shift);
        state.selected = rows
            .iter()
            .position(|r| matches!(r, crate::search_tree::Row::File { path: p, .. } if p == &path));
        state.explicit_file_selection = true;
        state.held = Some(Lit::Path(path.clone()));
        state.keep = None;
        window.focus(&state.results_focus, cx);
        if !mods.platform && !mods.shift {
            self.toggle_search_fold(path, cx);
        }
        cx.notify();
    }

    pub(crate) fn copy_search_files(&self, cx: &mut Context<Self>) {
        if let Some(state) = self.search_state() {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(state.files.copied_paths()));
        }
    }

    pub(crate) fn select_all_search_files(&mut self, cx: &mut Context<Self>) {
        let order = search_file_paths(&self.search_rows());
        if let Some(state) = self.active_project.and_then(|p| self.searches.get_mut(&p)) {
            state.files.key("a", &order, true, false);
            state.explicit_file_selection = true;
        }
        cx.notify();
    }

    pub(crate) fn search_results_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let rows = self.search_rows();
        let Some(state) = self.active_project.and_then(|p| self.searches.get_mut(&p)) else {
            return false;
        };
        let order = search_file_paths(&rows);
        state.files.retain(&order);
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        // Command with an arrow is the plain arrow below — decision 54.
        if mods.shift && state.files.key(key, &order, false, true) {
            state.explicit_file_selection = true;
            state.selected = state.files.focus.as_ref().and_then(|path| {
                rows.iter().position(
                    |r| matches!(r, crate::search_tree::Row::File { path: p, .. } if p == path),
                )
            });
            state.held = state.files.focus.clone().map(Lit::Path);
            state.keep = None;
            if let Some(i) = state.selected {
                state.scroll.scroll_to_item(i, gpui::ScrollStrategy::Top);
            }
            cx.notify();
            return true;
        }
        if key == "enter" && state.explicit_file_selection && state.files.focus.is_some() {
            let files = state.files.selected.clone();
            for path in files {
                self.open_file(&path, window, cx);
            }
            return true;
        }
        if key == "escape" {
            window.focus(&state.focus, cx);
            return true;
        }
        // Results focus never sends printable keys into the query.
        if matches!(key, "up" | "down" | "home" | "end" | "enter") {
            return self.search_key(event, window, cx);
        }
        true
    }
}

impl Root {
    pub(crate) fn clear_search_file_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.active_project.and_then(|p| self.searches.get_mut(&p)) {
            state.files = Default::default();
            state.explicit_file_selection = false;
        }
        cx.notify();
    }
}

impl Root {
    pub(crate) fn relocate_search(
        &mut self,
        project: ProjectId,
        from: &std::path::Path,
        to: &std::path::Path,
    ) {
        if let Some(state) = self.searches.get_mut(&project) {
            state.walk.relocate_paths(from, to);
            state.presentation.get_mut().take();
            state.files.remap(|path| {
                if let Some(next) = crate::file_operations::remap(path, from, to) {
                    *path = next;
                }
            });
            state.folded = state
                .folded
                .drain()
                .map(|p| crate::file_operations::remap(&p, from, to).unwrap_or(p))
                .collect();
            state.held = None;
            let now = Instant::now();
            state.stale = Some((now, now));
        }
    }
}
