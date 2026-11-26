//! Find Usages: `alt-f7`, the Find tool's searches, and the balloon over
//! the caret. M8.15.
//!
//! The language server finds the usages — decision 1 of the plan — and the
//! grammar says first whether the caret is on anything that can have any
//! (decision 2, `usages::target_at`). What is here is the asking, the tabs
//! the answers become, and the two things said at the caret instead of
//! opening a tab: *Cannot search for usages from this location* and
//! *Nothing found in 'Project Files'*.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    AppContext as _, Bounds, Context, FocusHandle, KeyDownEvent, Pixels, Point,
    UniformListScrollHandle, Window,
};
use guild_session::{ProjectId, Tool};
use lsp_types::{LocationLink, Position};

use crate::Root;
use crate::highlight::Language;
use crate::i18n::Text;
use crate::split::Split;
use crate::usages::{self, Folds, Found, Row};

/// How long a search may take before it is given up on.
///
/// **A minute, not `f12`'s five seconds**: task 0 measured jdtls at 2.8 s
/// for a name in a fresh project, and a search over a large one is a walk
/// of every file the server has indexed. The tab says *Searching…*
/// meanwhile, so the wait is seen.
pub(super) const DEADLINE: Duration = Duration::from_secs(60);

/// How long a search may run before its tab is shown. An answer that
/// lands sooner and is empty, or one usage, never opens the dock at all —
/// which is what makes the balloon the whole answer.
const QUIET: Duration = Duration::from_millis(400);

/// How long a balloon stays when nothing takes it away.
const BALLOON: Duration = Duration::from_secs(8);

/// How long after it is put up a balloon ignores the keyboard: the stroke
/// that asked for it may still be passing through the capture phase.
const BALLOON_DEAF: Duration = Duration::from_millis(150);

/// Every project's Find tool, and the numbers the searches are told apart
/// by.
#[derive(Default)]
pub(crate) struct Finds {
    by_project: HashMap<ProjectId, FindState>,
    serial: u64,
}

/// One project's Find tool: its searches, a tab each.
pub(crate) struct FindState {
    pub tabs: Vec<FindTab>,
    /// Which tab is in front.
    pub active: usize,
    pub focus: FocusHandle,
    /// The list beside the preview, as the share the list takes.
    pub split: Split,
    /// Where the body was drawn last frame, for the split's drag to measure.
    pub body_at: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The split being dragged, and where the pointer was.
    pub drag: Option<(Point<Pixels>, Split)>,
}

/// One search: what was asked and what came back.
pub(crate) struct FindTab {
    /// The search's number, which an answer carries back.
    pub serial: u64,
    /// What was asked — kept, so that *Rerun* asks it again.
    pub ask: Ask,
    /// Still waiting, or answered.
    pub searching: bool,
    requests: Vec<crate::lsp::client::Cancellation>,
    /// The usages, when they have landed.
    pub found: Found,
    /// Where the name is declared, when the server said.
    pub target: Option<TargetRow>,
    pub folds: Folds,
    /// Which row the keyboard is on.
    pub selected: Option<usize>,
    pub files: crate::file_selection::Selection<(usages::Group, std::path::PathBuf)>,
    /// Enter opens an explicit file selection; ordinary row navigation folds groups.
    pub explicit_file_selection: bool,
    pub scroll: UniformListScrollHandle,
}

/// The first row of a Find tab: the name, and where it is declared.
pub(crate) struct TargetRow {
    /// The declaration's file and line, as the row says it.
    pub shown: String,
    /// Where it is.
    pub link: LocationLink,
}

/// What a search asked.
#[derive(Clone)]
pub(crate) struct Ask {
    pub project: ProjectId,
    /// The file, in the project.
    pub path: std::path::PathBuf,
    pub language: &'static Language,
    /// Where the name starts, as the protocol counts.
    pub position: Position,
    /// The name, as it is written.
    pub name: String,
}

/// What a balloon says, and in what colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mood {
    /// A red card with `!`: the gesture cannot be done from here.
    Refusal,
    /// A grey card with the magnifier: it was done, and found nothing.
    Report,
}

/// The card over the caret.
pub(crate) struct Balloon {
    pub mood: Mood,
    pub lines: Vec<String>,
    /// The name's rectangle in the window, which the card points at.
    pub at: Bounds<Pixels>,
    born: Instant,
    serial: u64,
}

impl FindState {
    fn new(split: f32, cx: &mut Context<Root>) -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            focus: cx.focus_handle(),
            split: Split::new(split),
            body_at: Default::default(),
            drag: None,
        }
    }

    /// The tab in front, if there is one.
    pub fn tab(&self) -> Option<&FindTab> {
        self.tabs.get(self.active)
    }

    fn tab_mut(&mut self) -> Option<&mut FindTab> {
        self.tabs.get_mut(self.active)
    }
}

impl FindTab {
    /// The rows it draws.
    pub fn rows(&self) -> Vec<Row> {
        usages::rows(&self.found, &self.folds)
    }
}

impl Root {
    /// The Find tool of the project in front.
    pub(crate) fn find_state(&self) -> Option<&FindState> {
        self.finds.by_project.get(&self.active_project?)
    }

    fn find_state_mut(&mut self) -> Option<&mut FindState> {
        let project = self.active_project?;
        self.finds.by_project.get_mut(&project)
    }

    /// `alt-f7`: the usages of the name under the caret.
    pub(crate) fn find_usages(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((project, path)) = self.file_in_front() else {
            return;
        };
        let Some(language) = Language::of(&path) else {
            self.refuse_here(window, cx);
            return;
        };
        let Some(editor) = self.editor_of(project, &path, cx) else {
            return;
        };
        let (text, offset) =
            editor.read_with(cx, |state, _cx| (state.text().to_string(), state.cursor()));

        // **The grammar before any server** — decision 2. A keyword, a
        // literal, a comment or a space is refused at once.
        let Some(target) = usages::target_at(language.grammar, &text, offset) else {
            self.refuse_here(window, cx);
            return;
        };
        let Some(command) = language.server.first().copied() else {
            let said = self.ui().no_server_for(language.name);
            self.balloon_over(Mood::Refusal, vec![said], target.range, window, cx);
            return;
        };
        let key = (project, language.name);
        if let Some(fault) = self.serverless.get(&key) {
            let said = self.ui().server_fault(command, fault);
            self.balloon_over(Mood::Refusal, vec![said], target.range, window, cx);
            return;
        }
        self.ensure_server(project, language, cx);
        if !self.servers.get(&key).is_some_and(|server| server.ready) {
            let said = self.ui().usages_server_starting(command);
            self.balloon_over(Mood::Report, vec![said], target.range, window, cx);
            return;
        }

        let ask = Ask {
            project,
            path,
            language,
            position: crate::lsp::providers::position_in(&text, target.range.start),
            name: target.name,
        };
        self.search_usages(ask, None, window, cx);
    }

    /// Ask the server, and make the answer a tab — `replacing` names a tab
    /// to fill again rather than a new one, which is *Rerun*.
    fn search_usages(
        &mut self,
        ask: Ask,
        replacing: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(uri) = self.uri_of(ask.project, &ask.path) else {
            return;
        };
        let at = serde_json::json!({
            "textDocument": {"uri": uri},
            "position": ask.position,
        });
        let mut references = at.clone();
        // **Without the declaration**, decision 6: task 0 found `true` adds
        // it without saying which location it is, and the target row
        // already names it.
        references["context"] = serde_json::json!({"includeDeclaration": false});
        let name = ask.language.name;
        let Some(asked) = self.ask_server(ask.project, name, "textDocument/references", references)
        else {
            return;
        };
        let defined = self.ask_server(ask.project, name, "textDocument/definition", at);

        self.finds.serial += 1;
        let serial = self.finds.serial;
        let split = self.settings.find_split.unwrap_or(0.5);
        let project = ask.project;
        let state = self
            .finds
            .by_project
            .entry(project)
            .or_insert_with(|| FindState::new(split, cx));
        let tab = FindTab {
            serial,
            ask,
            searching: true,
            requests: std::iter::once(asked.cancellation())
                .chain(defined.as_ref().map(|r| r.cancellation()))
                .collect(),
            found: Found::default(),
            target: None,
            folds: Folds::default(),
            selected: None,
            files: Default::default(),
            explicit_file_selection: false,
            scroll: UniformListScrollHandle::new(),
        };
        match replacing.and_then(|old| state.tabs.iter().position(|tab| tab.serial == old)) {
            Some(index) => {
                state.tabs[index] = tab;
                state.active = index;
            }
            None => {
                state.tabs.push(tab);
                state.active = state.tabs.len() - 1;
            }
        }
        let shown_at_once = replacing.is_some();
        cx.notify();

        cx.spawn_in(window, async move |root, cx| {
            let begun = Instant::now();
            let mut revealed = shown_at_once;
            let mut definition = defined;
            loop {
                let taken = root
                    .update(cx, |root, _cx| {
                        root.find_tab(project, serial)?;
                        if let Some(id) = &definition
                            && let Some(answer) = root.server_answer(project, name, id)
                        {
                            definition = None;
                            root.definition_landed(project, serial, answer);
                        }
                        let alive = root.server_alive(project, name);
                        Some((root.server_answer(project, name, &asked), alive))
                    })
                    .ok()
                    .flatten();
                let Some((answer, alive)) = taken else {
                    return;
                };
                let answer = match answer {
                    Some(answer) => Some(answer),
                    None if !alive => Some(Err(String::new())),
                    None if begun.elapsed() > DEADLINE => None,
                    None => {
                        if !revealed && begun.elapsed() > QUIET {
                            revealed = true;
                            root.update_in(cx, |root, window, cx| {
                                root.show_find(project, window, cx)
                            })
                            .ok();
                        }
                        cx.background_executor()
                            .timer(Duration::from_millis(16))
                            .await;
                        continue;
                    }
                };
                root.update_in(cx, |root, window, cx| {
                    if let Some(tab) = root.find_tab_mut(project, serial) {
                        tab.requests.clear();
                    }
                    root.usages_answered(project, serial, answer, window, cx)
                })
                .ok();
                return;
            }
        })
        .detach();
    }

    /// The target row, from the definition's answer.
    fn definition_landed(
        &mut self,
        project: ProjectId,
        serial: u64,
        answer: Result<serde_json::Value, String>,
    ) {
        let Ok(answer) = answer else {
            return;
        };
        let Some(link) = crate::lsp::providers::links_in(answer).into_iter().next() else {
            return;
        };
        let root = self.desk.project(project).map(|p| p.path.clone());
        let uri = link.target_uri.to_string();
        let place = match crate::lsp::path_of(&uri) {
            Some(path) => root
                .as_deref()
                .and_then(|root| path.strip_prefix(root).ok())
                .map(std::path::Path::to_path_buf)
                .unwrap_or(path)
                .display()
                .to_string(),
            None => crate::jars::parse(&uri)
                .map(|entry| entry.inside.trim_start_matches('/').to_owned())
                .unwrap_or(uri),
        };
        let shown = format!("{place}:{}", link.target_selection_range.start.line + 1);
        if let Some(tab) = self
            .finds
            .by_project
            .get_mut(&project)
            .and_then(|state| state.tabs.iter_mut().find(|tab| tab.serial == serial))
        {
            tab.target = Some(TargetRow { shown, link });
        }
    }

    /// The server has answered a search, or will not.
    ///
    /// `None` is a search that ran out of time; an `Err` with no words is a
    /// server that went away while it was asked.
    fn usages_answered(
        &mut self,
        project: ProjectId,
        serial: u64,
        answer: Option<Result<serde_json::Value, String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(ask) = self.find_tab(project, serial).map(|tab| tab.ask.clone()) else {
            return;
        };
        let command = ask
            .language
            .server
            .first()
            .copied()
            .unwrap_or(ask.language.name);
        let links = match answer {
            Some(Ok(answer)) => crate::lsp::providers::links_in(answer),
            Some(Err(said)) => {
                self.drop_find_tab(project, serial, cx);
                let said = if said.is_empty() {
                    self.ui().usages_timed_out(command)
                } else {
                    self.ui().usages_refused(command, &said)
                };
                self.balloon_at_ask(&ask, Mood::Refusal, vec![said], window, cx);
                return;
            }
            None => {
                self.drop_find_tab(project, serial, cx);
                let said = self.ui().usages_timed_out(command);
                self.balloon_at_ask(&ask, Mood::Refusal, vec![said], window, cx);
                return;
            }
        };
        if links.is_empty() {
            self.drop_find_tab(project, serial, cx);
            let (first, second) = self.ui().usages_nothing_found(&ask.name);
            self.balloon_at_ask(&ask, Mood::Report, vec![first, second], window, cx);
            return;
        }
        // **One usage is a jump**, decision 4 — no results tab for a
        // single usage.
        if let [only] = links.as_slice() {
            self.drop_find_tab(project, serial, cx);
            self.note_place(cx);
            self.land_on(only, window, cx);
            return;
        }

        // The lines are read off the window's thread: a buffer that is open
        // is copied here, where the editor can be read, and every other
        // file — the disk, a jar — is read on the background executor.
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let buffers = self.open_buffers(project, cx);
        let work = cx.background_spawn(async move {
            usages::found(links, &root, |uri| {
                buffers.get(uri).cloned().or_else(|| read_uri(uri))
            })
        });
        cx.spawn_in(window, async move |root, cx| {
            let found = work.await;
            root.update_in(cx, |root, window, cx| {
                let Some(tab) = root.find_tab_mut(project, serial) else {
                    return;
                };
                tab.found = found;
                tab.files = Default::default();
                tab.explicit_file_selection = false;
                tab.searching = false;
                let rows = tab.rows();
                tab.selected = rows.iter().position(|row| usages::usage_of(row).is_some());
                root.show_find(project, window, cx);
                root.ask_find_preview(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Every open buffer of a project, by the URI a server names it with.
    fn open_buffers(&self, project: ProjectId, cx: &Context<Self>) -> HashMap<String, String> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File {
                    path,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } => {
                    let text = view.read(cx).editor()?.read(cx).value().to_string();
                    Some((self.uri_of(project, path)?, text))
                }
                _ => None,
            })
            .collect()
    }

    fn find_tab(&self, project: ProjectId, serial: u64) -> Option<&FindTab> {
        self.finds
            .by_project
            .get(&project)?
            .tabs
            .iter()
            .find(|tab| tab.serial == serial)
    }

    fn find_tab_mut(&mut self, project: ProjectId, serial: u64) -> Option<&mut FindTab> {
        self.finds
            .by_project
            .get_mut(&project)?
            .tabs
            .iter_mut()
            .find(|tab| tab.serial == serial)
    }

    /// Take a search's tab away — it answered with a balloon or a jump.
    fn drop_find_tab(&mut self, project: ProjectId, serial: u64, cx: &mut Context<Self>) {
        let Some(state) = self.finds.by_project.get_mut(&project) else {
            return;
        };
        if let Some(index) = state.tabs.iter().position(|tab| tab.serial == serial) {
            self.close_find_tab_at(project, index, cx);
        }
    }

    /// Close a Find tab; the last one closing puts the dock away.
    pub(crate) fn close_find_tab_at(
        &mut self,
        project: ProjectId,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.finds.by_project.get_mut(&project) else {
            return;
        };
        if index >= state.tabs.len() {
            return;
        }
        state.tabs.remove(index);
        if state.active > index || state.active >= state.tabs.len() {
            state.active = state.active.saturating_sub(1);
        }
        let empty = state.tabs.is_empty();
        if empty && Some(project) == self.active_project && self.tool_is_up(Tool::Find) {
            self.show_tool(Tool::Find, cx);
        }
        self.ask_find_preview(cx);
        cx.notify();
    }

    /// Bring a Find tab to the front.
    pub(crate) fn select_find_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(state) = self.find_state_mut()
            && index < state.tabs.len()
        {
            state.active = index;
        }
        self.ask_find_preview(cx);
        cx.notify();
    }

    /// Open the Find tool and give it the keyboard.
    fn show_find(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        if Some(project) != self.active_project {
            return;
        }
        self.reveal_tool(Tool::Find, cx);
        if let Some(state) = self.find_state() {
            let focus = state.focus.clone();
            window.focus(&focus, cx);
            // Again on the next frame, for a tool this very reveal draws
            // for the first time — the Search tool's `focus_query`.
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        cx.notify();
    }

    /// `cmd-3`: the Find tool, or its note when it has no search.
    pub(crate) fn show_find_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tool_is_up(Tool::Find) {
            self.show_tool(Tool::Find, cx);
            return;
        }
        match self.active_project {
            Some(project) if self.find_state().is_some() => self.show_find(project, window, cx),
            _ => self.show_tool(Tool::Find, cx),
        }
    }

    /// *Rerun*: the same place asked again, into the same tab.
    pub(crate) fn rerun_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.find_state().and_then(FindState::tab) else {
            return;
        };
        let (ask, serial) = (tab.ask.clone(), tab.serial);
        self.search_usages(ask, Some(serial), window, cx);
    }

    /// Open or shut every group, directory and file of the tab in front.
    pub(crate) fn fold_every_find_row(&mut self, shut: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) else {
            return;
        };
        if shut {
            let found = tab.found.clone();
            tab.folds.fold_all(&found);
        } else {
            tab.folds = Folds::default();
        }
        let rows = tab.rows();
        tab.selected = None;
        tab.files
            .retain(&rows.iter().filter_map(usages::file_of).collect::<Vec<_>>());
        self.ask_find_preview(cx);
        cx.notify();
    }

    /// Fold or unfold the row at `index`.
    pub(crate) fn toggle_find_row(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) else {
            return;
        };
        let rows = tab.rows();
        let Some(row) = rows.get(index) else {
            return;
        };
        if tab.folds.toggle(row) {
            tab.files.retain(
                &tab.rows()
                    .iter()
                    .filter_map(usages::file_of)
                    .collect::<Vec<_>>(),
            );
            // The folded row keeps the highlight: it is still drawn, at the
            // same index, since only what is below it went.
            tab.selected = Some(index);
        }
        cx.notify();
    }

    /// Light the row at `index`, and preview it.
    pub(crate) fn select_find_row(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) {
            tab.selected = Some(index);
            tab.files = Default::default();
            tab.explicit_file_selection = false;
        }
        self.ask_find_preview(cx);
        cx.notify();
    }

    /// Open the usage at `at` of the tab in front — its file, at the name.
    pub(crate) fn open_usage(&mut self, at: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self
            .find_state()
            .and_then(FindState::tab)
            .and_then(|tab| tab.found.links.get(at).cloned())
        else {
            return;
        };
        self.note_place(cx);
        self.land_on(&link, window, cx);
    }

    /// Open the declaration the target row names.
    pub(crate) fn open_find_target(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(link) = self
            .find_state()
            .and_then(FindState::tab)
            .and_then(|tab| tab.target.as_ref())
            .map(|target| target.link.clone())
        else {
            return;
        };
        self.note_place(cx);
        self.land_on(&link, window, cx);
    }

    /// Enter or a double click on a row: a usage or the target opens, and
    /// anything else folds.
    pub(crate) fn activate_find_row(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .find_state()
            .and_then(FindState::tab)
            .and_then(|tab| tab.rows().get(index).cloned())
        else {
            return;
        };
        match row {
            Row::Target => self.open_find_target(window, cx),
            row => match usages::usage_of(&row) {
                Some(at) => self.open_usage(at, window, cx),
                None => self.toggle_find_row(index, cx),
            },
        }
    }

    /// `cmd-alt-down` and `cmd-alt-up`: the next usage of the tab in front,
    /// lit and opened — from the editor as much as from the tool, which is
    /// what *Next Occurrence* is for.
    pub(crate) fn step_usage(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) else {
            return;
        };
        let rows = tab.rows();
        let usages: Vec<usize> = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| usages::usage_of(row).is_some())
            .map(|(index, _)| index)
            .collect();
        let next = match tab.selected {
            Some(now) if down => usages.iter().copied().find(|index| *index > now),
            Some(now) => usages.iter().copied().rev().find(|index| *index < now),
            None if down => usages.first().copied(),
            None => usages.last().copied(),
        };
        let Some(index) = next else {
            return;
        };
        tab.files = Default::default();
        tab.explicit_file_selection = false;
        tab.selected = Some(index);
        tab.scroll
            .scroll_to_item(index, gpui::ScrollStrategy::Center);
        let at = usages::usage_of(&rows[index]);
        self.ask_find_preview(cx);
        if let Some(at) = at {
            self.open_usage(at, window, cx);
        }
        cx.notify();
    }

    /// A key pressed with the Find tool focused. Answers whether it took it.
    pub(crate) fn find_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) else {
            return false;
        };
        let rows = tab.rows();
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        let order: Vec<_> = rows.iter().filter_map(usages::file_of).collect();
        tab.files.retain(&order);
        // Command with an arrow is the plain arrow below — decision 54.
        if mods.shift && tab.files.key(key, &order, false, true) {
            tab.explicit_file_selection = true;
            tab.selected = tab.files.focus.as_ref().and_then(|k| {
                rows.iter()
                    .position(|r| usages::file_of(r).as_ref() == Some(k))
            });
            if let Some(i) = tab.selected {
                tab.scroll.scroll_to_item(i, gpui::ScrollStrategy::Top);
            }
            cx.notify();
            return true;
        }
        if key == "enter" && tab.explicit_file_selection && tab.files.focus.is_some() {
            self.open_find_files(window, cx);
            return true;
        }
        match key {
            key @ ("down" | "up" | "home" | "end") => {
                let next = match key {
                    "home" => (!rows.is_empty()).then_some(0),
                    "end" => rows.len().checked_sub(1),
                    _ => crate::pick::step(rows.len(), tab.selected, key == "down"),
                };
                if let Some(index) = next {
                    tab.files = Default::default();
                    tab.explicit_file_selection = false;
                    if let Some(file) = usages::file_of(&rows[index]) {
                        tab.files.click(&file, &order, false, false);
                    }
                    tab.selected = Some(index);
                    tab.scroll.scroll_to_item(index, gpui::ScrollStrategy::Top);
                    self.ask_find_preview(cx);
                    cx.notify();
                }
            }
            // Left shuts an open row and right opens a shut one, as every
            // tree does; on a row that does not fold they do nothing.
            key @ ("left" | "right") => {
                if let Some(index) = tab.selected
                    && let Some(row) = rows.get(index)
                    && tab.folds.is_folded(row) == (key == "right")
                {
                    self.toggle_find_row(index, cx);
                }
            }
            "enter" => {
                if let Some(index) = tab.selected {
                    self.activate_find_row(index, window, cx);
                }
            }
            "escape" => {
                let focus = self
                    .active_body()
                    .and_then(|body| body.focus_handle(cx))
                    .unwrap_or_else(|| self.focus.clone());
                window.focus(&focus, cx);
                cx.notify();
            }
            _ => return false,
        }
        true
    }

    /// Ask for the preview of the row lit in the tab in front.
    ///
    /// **A project's usage only**: a library's file has no root in the
    /// window until it is opened, and the pane says so instead.
    pub(crate) fn ask_find_preview(&mut self, cx: &mut Context<Self>) {
        let asked = (|| {
            let project = self.active_project?;
            let tab = self.find_state()?.tab()?;
            let row = tab.rows().get(tab.selected?)?.clone();
            let at = usages::usage_of(&row)?;
            if at >= tab.found.project {
                return None;
            }
            let hit = tab.found.hits.get(at)?;
            Some(crate::root::preview::Asked {
                project,
                origin: crate::Origin::Project,
                path: hit.path.clone(),
                line: hit.line.saturating_sub(1),
                hit: Some((hit.text.clone(), hit.ranges.clone())),
            })
        })();
        let around = self.find_preview_around();
        self.ask_preview_in(crate::root::preview::Slot::Find, asked, around, cx);
    }

    /// How many lines above and below the usage fill the preview's pane.
    fn find_preview_around(&self) -> usize {
        let dock = self.docks().dock_of(Tool::Find);
        let panels = self.panels();
        let size = match dock {
            guild_session::Dock::Bottom => panels.bottom.size,
            guild_session::Dock::Left => panels.left.size,
            guild_session::Dock::Right => panels.right.size,
        };
        // A row of the code face is about eighteen points; overflow is
        // clipped, so erring long costs nothing but a parse already done.
        ((size / 18.0) as usize / 2).clamp(3, 60)
    }

    /// Forget a project's searches when the project closes.
    pub(crate) fn forget_finds(&mut self, project: ProjectId) {
        self.finds.by_project.remove(&project);
    }

    /// The list-and-preview split being dragged.
    pub(crate) fn drag_find_split(
        &mut self,
        event: &gpui::MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.find_state_mut() else {
            return false;
        };
        let Some((from, held)) = state.drag else {
            return false;
        };
        // A release outside the window can go missing: no button held
        // means the drag is over.
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            state.drag = None;
            let split = state.split.fraction();
            self.remember_find_split(split);
            cx.notify();
            return true;
        }
        let total = state
            .body_at
            .get()
            .map_or(0.0, |bounds| f32::from(bounds.size.width));
        let delta = f32::from(event.position.x - from.x);
        // Keep sub-percent moves: Split rounds, so measure from the press,
        // as area and pane borders do, rather than forgetting every small step.
        state.split = held;
        state.split.drag(delta, total, 160.0);
        cx.notify();
        true
    }

    /// The split taken hold of.
    pub(crate) fn start_find_split(&mut self, at: Point<Pixels>, cx: &mut Context<Self>) {
        if let Some(state) = self.find_state_mut() {
            state.drag = Some((at, state.split));
            cx.notify();
        }
    }

    /// The split let go of.
    pub(crate) fn end_find_split(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.find_state_mut() else {
            return;
        };
        if state.drag.take().is_some() {
            let split = state.split.fraction();
            self.remember_find_split(split);
            cx.notify();
        }
    }

    fn remember_find_split(&mut self, split: f32) {
        if self.settings.find_split == Some(split) {
            return;
        }
        self.settings.find_split = Some(split);
        self.store_settings();
    }

    // ── The balloon ────────────────────────────────────────────────────

    /// The red card: nothing under the caret can be searched for.
    fn refuse_here(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ui = self.ui();
        let lines = vec![
            ui.text(Text::UsagesCannotSearch).to_owned(),
            ui.text(Text::UsagesCannotSearchHint).to_owned(),
        ];
        let Some((project, path)) = self.file_in_front() else {
            return;
        };
        let offset = self
            .editor_of(project, &path, cx)
            .map(|editor| editor.read(cx).cursor())
            .unwrap_or(0);
        self.balloon_over(Mood::Refusal, lines, offset..offset, window, cx);
    }

    /// A balloon over where a search was asked, if that file is in front.
    fn balloon_at_ask(
        &mut self,
        ask: &Ask,
        mood: Mood,
        lines: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let in_front = self
            .file_in_front()
            .is_some_and(|(project, path)| project == ask.project && path == ask.path);
        let range = in_front
            .then(|| self.editor_of(ask.project, &ask.path, cx))
            .flatten()
            .map(|editor| {
                let text = editor.read(cx).text().to_string();
                let start = crate::lsp::providers::offset_of(&text, ask.position);
                start..start + ask.name.len()
            });
        match range {
            Some(range) => self.balloon_over(mood, lines, range, window, cx),
            // The person has moved on: a card over another file's text
            // would point at nothing it is about.
            None => self.say_as_notice(mood, lines),
        }
    }

    /// A balloon that has nowhere to hang, said in the corner instead: a
    /// refusal as a warning, the red card, and a report as information,
    /// its grey one.
    fn say_as_notice(&mut self, mood: Mood, lines: Vec<String>) {
        let text = lines.join(" ");
        match mood {
            Mood::Refusal => self.warn(text),
            Mood::Report => self.inform(text),
        }
    }

    /// Put a balloon over a range of the file in front; a notice when the
    /// editor has not been drawn to say where that is.
    fn balloon_over(
        &mut self,
        mood: Mood,
        lines: Vec<String>,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = self
            .file_tab_in_front()
            .and_then(|view| view.update(cx, |tab, cx| tab.bounds_of(range, window, cx)));
        let Some(at) = at else {
            self.say_as_notice(mood, lines);
            return;
        };
        tracing::debug!(?at, "a balloon over the caret");
        self.finds.serial += 1;
        let serial = self.finds.serial;
        self.balloon = Some(Balloon {
            mood,
            lines,
            at,
            born: Instant::now(),
            serial,
        });
        cx.spawn(async move |root, cx| {
            cx.background_executor().timer(BALLOON).await;
            root.update(cx, |root, cx| {
                if root.balloon.as_ref().is_some_and(|b| b.serial == serial) {
                    root.balloon = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// The file tab in front, as a view.
    fn file_tab_in_front(&self) -> Option<gpui::Entity<crate::file_tab::FileTab>> {
        let tab = self
            .tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active_tab)?;
        match tab.body()? {
            crate::Body::File { tab: view, .. } => Some(view.clone()),
            _ => None,
        }
    }

    /// Take the balloon down, for any key or press — answers whether it was
    /// up and the key was Escape, which it then keeps.
    pub(crate) fn balloon_heard(&mut self, key: Option<&str>, cx: &mut Context<Self>) -> bool {
        let Some(balloon) = &self.balloon else {
            return false;
        };
        if key.is_some() && balloon.born.elapsed() < BALLOON_DEAF {
            return false;
        }
        self.balloon = None;
        cx.notify();
        key == Some("escape")
    }
}

/// A file a server named, read: the disk for a `file:` URI, the sources
/// beside a jar for a `jar:` one. `None` for anything else, or a file too
/// large for a tab.
fn read_uri(uri: &str) -> Option<String> {
    let path = match crate::lsp::path_of(uri) {
        Some(path) => path,
        None => {
            let landed = crate::jars::open_source(uri, &crate::jars::sources_dir())?;
            landed.root.join(landed.relative)
        }
    };
    let size = std::fs::metadata(&path).ok()?.len();
    if size > crate::file_tab::OPEN_LIMIT {
        return None;
    }
    std::fs::read_to_string(path).ok()
}

impl Root {
    pub(crate) fn find_row_click(
        &mut self,
        index: usize,
        mods: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.find_state_mut() else {
            return;
        };
        let focus = state.focus.clone();
        let Some(tab) = state.tab_mut() else {
            return;
        };
        let rows = tab.rows();
        if let Some(key) = rows.get(index).and_then(usages::file_of) {
            tab.files.click(
                &key,
                &rows.iter().filter_map(usages::file_of).collect::<Vec<_>>(),
                mods.platform,
                mods.shift,
            );
            tab.explicit_file_selection = true;
            tab.selected = Some(index);
            cx.notify();
        } else {
            self.select_find_row(index, cx);
        }
        self.ask_find_preview(cx);
        window.focus(&focus, cx);
    }

    pub(crate) fn select_all_find_files(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.find_state_mut().and_then(FindState::tab_mut) {
            tab.explicit_file_selection = true;
            tab.files.key(
                "a",
                &tab.rows()
                    .iter()
                    .filter_map(usages::file_of)
                    .collect::<Vec<_>>(),
                true,
                false,
            );
        }
        cx.notify();
    }

    pub(crate) fn copy_find_files(&self, cx: &mut Context<Self>) {
        if let Some(tab) = self.find_state().and_then(FindState::tab) {
            let paths = tab
                .files
                .selected
                .iter()
                .map(|(_, p)| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n");
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(paths));
        }
    }

    fn open_find_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.find_state().and_then(FindState::tab) else {
            return;
        };
        // Existing usage links retain the full source URI, including jar/dependency origins.
        let indices: Vec<_> = tab
            .files
            .selected
            .iter()
            .filter_map(|(group, path)| {
                tab.found.hits.iter().enumerate().position(|(i, h)| {
                    &h.path == path && (i < tab.found.project) == (*group == usages::Group::Project)
                })
            })
            .collect();
        for at in indices {
            self.open_usage(at, window, cx);
        }
    }
}

#[cfg(test)]
#[path = "usages/navigation_tests.rs"]
mod navigation_tests;

impl Root {
    pub(crate) fn relocate_usages(
        &mut self,
        project: ProjectId,
        from: &std::path::Path,
        to: &std::path::Path,
    ) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let map_link = |link: &mut LocationLink| {
            if let Some(full) = crate::lsp::path_of(link.target_uri.as_str())
                && let Ok(relative) = full.strip_prefix(&root)
                && let Some(next) = crate::file_operations::remap(relative, from, to)
                && let Ok(uri) = crate::lsp::uri(&root.join(next)).parse()
            {
                link.target_uri = uri;
            }
        };
        if let Some(state) = self.finds.by_project.get_mut(&project) {
            for tab in &mut state.tabs {
                // In-flight server answers still refer to the old filesystem.
                self.finds.serial += 1;
                tab.requests.clear();
                tab.serial = self.finds.serial;
                tab.searching = false;
                if let Some(next) = crate::file_operations::remap(&tab.ask.path, from, to) {
                    tab.ask.path = next;
                }
                for hit in tab.found.hits.iter_mut().take(tab.found.project) {
                    if let Some(next) = crate::file_operations::remap(&hit.path, from, to) {
                        hit.path = next;
                    }
                }
                for link in tab.found.links.iter_mut().take(tab.found.project) {
                    map_link(link);
                }
                if let Some(target) = &mut tab.target {
                    map_link(&mut target.link);
                }
                let map = |(group, path): &mut (usages::Group, std::path::PathBuf)| {
                    if *group == usages::Group::Project
                        && let Some(next) = crate::file_operations::remap(path, from, to)
                    {
                        *path = next;
                    }
                };
                tab.files.remap(map);
                tab.folds.paths = tab
                    .folds
                    .paths
                    .drain()
                    .map(|mut pair| {
                        map(&mut pair);
                        pair
                    })
                    .collect();
            }
        }
    }
}

/// Seed the real Find host from a captured local references answer, without LSP.
#[cfg(feature = "native-preview")]
impl Root {
    pub(crate) fn native_occurrence_usages(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.active_project.expect("owned fixture project");
        let folder = self.desk.project(project).unwrap().path.clone();
        let uri: lsp_types::Uri = crate::lsp::uri(&folder.join("example.rs")).parse().unwrap();
        let links = crate::source_occurrences::mentions(text, "total")
            .into_iter()
            .map(|range| {
                let prefix = &text[..range.start];
                let line = prefix.bytes().filter(|b| *b == b'\n').count() as u32;
                let character = prefix
                    .rsplit('\n')
                    .next()
                    .unwrap_or("")
                    .encode_utf16()
                    .count() as u32;
                let range = lsp_types::Range::new(
                    Position::new(line, character),
                    Position::new(line, character + 5),
                );
                LocationLink {
                    origin_selection_range: None,
                    target_uri: uri.clone(),
                    target_range: range,
                    target_selection_range: range,
                }
            })
            .collect::<Vec<_>>();
        let found = usages::found(links.clone(), &folder, |_| Some(text.to_owned()));
        self.finds.serial += 1;
        let serial = self.finds.serial;
        let mut state = FindState::new(0.48, cx);
        state.tabs.push(FindTab {
            serial,
            ask: Ask {
                project,
                path: "example.rs".into(),
                language: Language::of(std::path::Path::new("example.rs")).unwrap(),
                position: Position::new(1, 12),
                name: "total".into(),
            },
            searching: false,
            requests: Vec::new(),
            found,
            target: links.first().cloned().map(|link| TargetRow {
                shown: "example.rs:2".into(),
                link,
            }),
            folds: Default::default(),
            selected: None,
            files: Default::default(),
            explicit_file_selection: false,
            scroll: UniformListScrollHandle::new(),
        });
        let rows = state.tabs[0].rows();
        state.tabs[0].selected = rows.iter().position(|row| usages::usage_of(row).is_some());
        self.finds.by_project.insert(project, state);
        self.show_find(project, window, cx);
        self.ask_find_preview(cx);
    }
}
