//! Search Everywhere as a popup the picker holds — M8.9, decisions 6 to 8
//! and 15.
//!
//! **One popup behind its named actions**: Go to File opens it on *Files*,
//! `cmd-alt-o` on *Symbols*, `cmd-shift-a` on *Actions*, and a double
//! Shift on *All*; a tab's stroke pressed while it is up means what
//! `everywhere::again` says. The seven questions every popup answers
//! (decision 1) are answered here, and the card is `view::everywhere`.
//!
//! A method added to `Root` here starts with `everywhere_`: method names on
//! `Root` are a namespace three branches share and nothing warns about.

use std::hash::{Hash, Hasher};

use gpui::prelude::*;
use gpui::{Context, Window};
use guild_session::ProjectId;

use crate::Root;
use crate::actions::keymap::Named;
use crate::actions::{FindAction, GoToFile, GoToSymbol, SearchEverywhere};
use crate::everywhere::{Among, Asked, Slot};
use crate::picker::{Jump, Key, Lit, Take};
use crate::root::picker::{Focus, Holds, Scroll};

/// What Search Everywhere keeps while it is up.
#[derive(Default)]
pub(crate) struct Everywhere {
    /// The tab showing.
    pub among: Among,
    /// *Include non-project items*: the dependencies' files and symbols
    /// too.
    pub libraries: bool,
    /// The *Text* tab's walk.
    pub text: crate::search::Walk,
    /// The toggles and the mask the *Text* tab searches with — the ones
    /// the project remembers, read once when the popup opens.
    pub toggles: crate::search::Query,
    /// What the *Text* tab's walk was last started with, so asking again
    /// for the same query — a switch flipped, the tab shown again while its
    /// walk is kept — does not walk the tree again. Boxed, because it is
    /// asked about only when the query changes and `Holds` is sized by its
    /// largest popup.
    walked: Option<Box<crate::search::Query>>,
    /// What the preview was last asked for, so the lit row is asked about
    /// once and not on every tick.
    previewed: Option<Previewed>,
    /// Every action Find Action offers, and whether the focus the popup was
    /// opened over handles it — **asked once, before the popup takes the
    /// focus** (task 0): the focus it will give back is the one that
    /// answers, and the card's own would answer for none of a terminal's.
    pub actions: Vec<(Named, bool)>,
    /// The actions the query finds, best first, with their label's marks —
    /// ranked when the query changes, not once per row drawn.
    pub found: Vec<(Named, Vec<u32>)>,
    /// The stroke each action is bound to, printed — read once when the
    /// popup opens, because `actions::effective` reads the keymap file and
    /// a row is drawn sixty times a second.
    pub strokes: std::collections::BTreeMap<Named, String>,
    /// What the rows looked like at the last poll, so a poll that brings
    /// nothing new does not redraw the window.
    seen: u64,
}

/// What a row of the list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    /// A section's title in *All* — never lit.
    Heading(Among),
    /// The file at this index of `ranked_files`.
    File(usize),
    /// The symbol at this index of `ranked_symbols`.
    Symbol(usize),
    /// The action at this index of `Everywhere.found`.
    Action(usize),
    /// The hit at this index of the *Text* tab's walk.
    Hit(usize),
    /// *N more* under a section of *All*.
    More(Among),
}

/// The row a preview was asked for: where, and on which line.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Previewed {
    origin: crate::Origin,
    path: std::path::PathBuf,
    line: u32,
}

impl Root {
    /// The actions that open Search Everywhere on a tab.
    ///
    /// `SearchEverywhere` is among them though its default is a double
    /// Shift and no binding: the Go menu dispatches it, and a stroke a
    /// person records for it is a binding like any other.
    pub(crate) fn bind_everywhere(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &GoToFile, window, cx| {
            root.everywhere_pressed(Among::Files, window, cx)
        }))
        .on_action(cx.listener(|root, _: &GoToSymbol, window, cx| {
            root.everywhere_pressed(Among::Symbols, window, cx)
        }))
        .on_action(cx.listener(|root, _: &FindAction, window, cx| {
            root.everywhere_pressed(Among::Actions, window, cx)
        }))
        .on_action(cx.listener(|root, _: &SearchEverywhere, window, cx| {
            root.everywhere_pressed(Among::All, window, cx)
        }))
    }

    /// The modifiers changed — the double Shift's ear on the root, decision
    /// 8. The other two ears only count; this hands the count to the
    /// detector and, when a pair completes, opens *All* or does what a
    /// double Shift means over the tab showing.
    ///
    /// **Not while a stroke is being recorded** — the `!recording` rule
    /// every binding carries, for a default that is not a binding — and not
    /// once a stroke of its own has been recorded for `SearchEverywhere`.
    /// The keymap file is read only when a pair completes, which is rare,
    /// rather than on every Shift.
    pub(crate) fn shift_heard(
        &mut self,
        event: &gpui::ModifiersChangedEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let held = event.modifiers;
        let shift_alone =
            held.shift && !held.control && !held.alt && !held.platform && !held.function;
        let none = !held.modified();
        let fired = self.double_shift.modifiers(
            shift_alone,
            none,
            self.heard.get(),
            std::time::Instant::now(),
        );
        if !fired
            || self.recording.is_some()
            || !crate::double_tap::bound(&crate::actions::keymap::overrides())
        {
            return;
        }
        self.everywhere_pressed(Among::All, window, cx);
    }

    /// A tab's stroke: open the popup on it, or — while it is up — what
    /// `everywhere::again` says the stroke means there.
    pub(crate) fn everywhere_pressed(
        &mut self,
        pressed: Among,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(showing) = self.everywhere().map(|popup| popup.among) else {
            self.everywhere_open(pressed, window, cx);
            return;
        };
        match crate::everywhere::again(showing, pressed) {
            Asked::Open(among) => self.everywhere_show(among, cx),
            Asked::ToAll => self.everywhere_show(Among::All, cx),
            Asked::Libraries => self.everywhere_toggle_libraries(cx),
        }
    }

    /// A tab clicked.
    pub(crate) fn everywhere_choose(&mut self, among: Among, cx: &mut Context<Self>) {
        self.everywhere_show(among, cx);
    }

    /// *Include non-project items* turned on or off — by its checkbox, or
    /// by a tab's stroke pressed again.
    pub(crate) fn everywhere_toggle_libraries(&mut self, cx: &mut Context<Self>) {
        if let Some(popup) = self.everywhere_mut() {
            popup.libraries = !popup.libraries;
        }
        self.everywhere_ask(cx);
        self.settle_picker();
        cx.notify();
    }

    /// Put the popup up on a tab.
    fn everywhere_open(&mut self, among: Among, window: &mut Window, cx: &mut Context<Self>) {
        // Another popup's card holds the focus; the actions are asked of
        // what that popup was opened over, which closing it gives back.
        if self.picker.is_some() {
            self.close_picker(Focus::Back, window, cx);
        }
        let actions = Named::ALL
            .iter()
            .copied()
            .filter(|action| crate::actions::labels::offered(*action))
            .map(|action| {
                let available = window.is_action_available(action.action().as_ref(), cx);
                (action, available)
            })
            .collect();
        let mut strokes = std::collections::BTreeMap::new();
        for (stroke, action) in crate::actions::effective() {
            strokes
                .entry(action)
                .or_insert_with(|| crate::actions::printed(&stroke));
        }
        // The double Shift is no binding, so `effective` does not name it;
        // it is in force until a stroke is recorded for the action.
        strokes
            .entry(Named::SearchEverywhere)
            .or_insert_with(|| crate::actions::printed(crate::double_tap::DOUBLE_SHIFT));
        self.everywhere_size();
        let toggles = self.everywhere_toggles();
        self.open_picker(
            Holds::Everywhere(Box::new(Everywhere {
                among,
                actions,
                strokes,
                toggles,
                ..Everywhere::default()
            })),
            self.active_project,
            Scroll::uniform(),
            window,
            cx,
        );
        self.everywhere_ask(cx);
        self.settle_picker();
        cx.notify();
    }

    /// **The card opens at section 6.33's size the first time**, and at
    /// whatever a hand made it after. It is resized and remembered like
    /// Recent Projects, whose default size the settings field starts with
    /// — a place never moved or resized is that default, and is given this
    /// card's own.
    fn everywhere_size(&mut self) {
        if self.settings.everywhere != guild_session::PickerPlace::default() {
            return;
        }
        let rem = f32::from(crate::scale::rem_size(self.settings.ui_font_size));
        (
            self.settings.everywhere.width,
            self.settings.everywhere.height,
        ) = crate::everywhere::first_size(rem);
    }

    /// Show another tab, keeping what was typed; the first row of the new
    /// list is lit and the list starts at its top.
    fn everywhere_show(&mut self, among: Among, cx: &mut Context<Self>) {
        if let Some(popup) = self.everywhere_mut() {
            popup.among = among;
        }
        self.everywhere_ask(cx);
        let rows = self.everywhere_lit();
        if let Some(open) = self.picker.as_mut() {
            open.picker.jump(Jump::First, &rows);
        }
        self.settle_picker();
        cx.notify();
    }

    /// The toggles and the mask the project remembers from the Search tool
    /// and the Find in Files popup, for the *Text* tab — read from the
    /// state file once, when the popup opens, rather than on every letter.
    fn everywhere_toggles(&self) -> crate::search::Query {
        let Some(root) = self
            .active_project
            .and_then(|project| self.desk.project(project))
            .map(|project| project.path.clone())
        else {
            return crate::search::Query::default();
        };
        let remembered = guild_session::project_state::load(&root).search;
        crate::search::Query {
            text: String::new(),
            case_sensitive: remembered.match_case,
            whole_word: remembered.whole_word,
            regular_expression: remembered.regex,
            mask: if remembered.mask_off {
                String::new()
            } else {
                remembered.mask
            },
            scope: None,
        }
    }

    /// Ask for what the tab showing lists, for the query as it now is.
    ///
    /// `everywhere::wanted` says what: the actions are ranked here and now
    /// — ninety-odd labels; the files and the symbols on the indexes'
    /// threads, and the text on the tab's own walk, all answering on the
    /// watch loop's clock. **The rows on screen stay until the answer
    /// lands**, M8.3.4's rule: a list that blanks on every letter blinks.
    /// Files are asked for by the name alone — `history.rs:120` ranks
    /// `history.rs` and keeps the place for Enter.
    fn everywhere_ask(&mut self, cx: &mut Context<Self>) {
        let query = self.picker_query().to_owned();
        let ui = self.ui();
        let project = self.everywhere_project();
        let root = project
            .and_then(|project| self.desk.project(project))
            .map(|project| project.path.clone());
        let buffers = if self
            .everywhere()
            .is_some_and(|popup| crate::everywhere::wanted(popup.among, &query).text)
        {
            project
                .map(|project| self.project_search_buffers(project, cx))
                .unwrap_or_default()
        } else {
            Default::default()
        };
        let Some(popup) = self.everywhere_mut() else {
            return;
        };
        let (among, libraries) = (popup.among, popup.libraries);
        let wanted = crate::everywhere::wanted(among, &query);
        if wanted.actions {
            let named: Vec<(Named, String)> = popup
                .actions
                .iter()
                .map(|(action, _)| (*action, crate::actions::labels::label(*action, ui)))
                .collect();
            popup.found = crate::everywhere::rank_actions(&query, &named);
        }
        match &root {
            Some(root) if wanted.text => {
                let search = crate::search::Query {
                    text: query.clone(),
                    ..popup.toggles.clone()
                };
                if popup.walked.as_deref() != Some(&search) {
                    popup.text.restart_documents(root, search.clone(), buffers);
                    popup.walked = Some(Box::new(search));
                }
            }
            // A walk nothing is waiting for is let go of — a dropped walk
            // stops its thread — rather than left to walk a tree for a
            // query that was rubbed out or a tab that was left.
            _ => {
                popup.text = crate::search::Walk::default();
                popup.walked = None;
            }
        }
        let Some(project) = project else {
            return;
        };
        if wanted.files {
            let (name, _) = crate::everywhere::split_line(&query);
            self.rank_files(project, name.trim(), libraries, cx);
        }
        if wanted.symbols {
            self.rank_symbols(project, &query, libraries, cx);
        }
    }

    /// The popup's own state, while it is the popup that is up.
    pub(crate) fn everywhere(&self) -> Option<&Everywhere> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::Everywhere(popup)) => Some(popup),
            _ => None,
        }
    }

    fn everywhere_mut(&mut self) -> Option<&mut Everywhere> {
        match self.picker.as_mut().map(|open| &mut open.holds) {
            Some(Holds::Everywhere(popup)) => Some(popup),
            _ => None,
        }
    }

    /// The project the popup is about.
    pub(crate) fn everywhere_project(&self) -> Option<ProjectId> {
        self.picker.as_ref().and_then(|open| open.project)
    }

    /// How many rows each of the tab's lists holds that it wants for what
    /// has been typed: files, symbols, actions, hits.
    pub(crate) fn everywhere_section_lengths(&self) -> (usize, usize, usize, usize) {
        let Some(popup) = self.everywhere() else {
            return (0, 0, 0, 0);
        };
        let wanted = crate::everywhere::wanted(popup.among, self.picker_query());
        let project = self.everywhere_project();
        let files = match project {
            Some(project) if wanted.files => self.ranked_files(project).len(),
            _ => 0,
        };
        let symbols = match project {
            Some(project) if wanted.symbols => self.ranked_symbols(project).len(),
            _ => 0,
        };
        let actions = if wanted.actions { popup.found.len() } else { 0 };
        let hits = if wanted.text {
            popup.text.hits().len()
        } else {
            0
        };
        (files, symbols, actions, hits)
    }

    /// How many rows the list has.
    pub(crate) fn everywhere_count(&self) -> usize {
        let Some(among) = self.everywhere().map(|popup| popup.among) else {
            return 0;
        };
        let (files, symbols, actions, hits) = self.everywhere_section_lengths();
        match among {
            Among::All => crate::everywhere::all(files, symbols, actions).len(),
            Among::Files => files,
            Among::Symbols => symbols,
            Among::Actions => actions,
            Among::Text => hits,
        }
    }

    /// What a row of the list is — asked once per row drawn, so a tab
    /// of thousands is counted rather than built.
    pub(crate) fn everywhere_item(&self, row: usize) -> Option<Item> {
        let among = self.everywhere()?.among;
        let (files, symbols, actions, hits) = self.everywhere_section_lengths();
        let within = |count: usize, item: fn(usize) -> Item| (row < count).then(|| item(row));
        match among {
            Among::All => crate::everywhere::all(files, symbols, actions)
                .get(row)
                .map(|slot| match *slot {
                    Slot::Heading(among) => Item::Heading(among),
                    Slot::Row(Among::Files, index) => Item::File(index),
                    Slot::Row(Among::Symbols, index) => Item::Symbol(index),
                    Slot::Row(_, index) => Item::Action(index),
                    Slot::More(among) => Item::More(among),
                }),
            Among::Files => within(files, Item::File),
            Among::Symbols => within(symbols, Item::Symbol),
            Among::Actions => within(actions, Item::Action),
            Among::Text => within(hits, Item::Hit),
        }
    }

    /// The popup's rows, as far as lighting them goes: a heading is never
    /// lit, and everything else is a row.
    pub(crate) fn everywhere_lit(&self) -> Vec<Lit> {
        let Some(among) = self.everywhere().map(|popup| popup.among) else {
            return Vec::new();
        };
        if among != Among::All {
            return vec![Lit::Row; self.everywhere_count()];
        }
        let (files, symbols, actions, _) = self.everywhere_section_lengths();
        crate::everywhere::all(files, symbols, actions)
            .into_iter()
            .map(|slot| match slot {
                Slot::Heading(_) => Lit::Heading,
                Slot::Row(..) | Slot::More(_) => Lit::Row,
            })
            .collect()
    }

    /// A key only this popup reads — Tab between its tabs. Answers whether
    /// it took it.
    pub(crate) fn everywhere_key(
        &mut self,
        key: &Key,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(among) = self.everywhere().map(|popup| popup.among) else {
            return false;
        };
        match key {
            Key::NextTab => self.everywhere_show(among.next(), cx),
            Key::PreviousTab => self.everywhere_show(among.previous(), cx),
            _ => return false,
        }
        true
    }

    /// The query changed: ask again.
    pub(crate) fn everywhere_edited(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.everywhere_ask(cx);
    }

    /// A row taken — through the door its place needs. Decision 17:
    ///
    /// - **A file in the project with a place typed after it**
    ///   (`history.rs:120:4`) goes through `go_to_place` alone, which records
    ///   the departure and lands the caret, a restored tab included.
    /// - **A file in the project without one** is `note_place`, then
    ///   `open_file` — Recent Files' shape: the file comes back where its tab
    ///   was.
    /// - **A file under a dependency** goes through `go_to_library`, with the
    ///   place as a caret in characters when one was typed; **never
    ///   `go_to_place`**, whose `stand_at` opens a project path, so
    ///   `serde/src/lib.rs` would open this repository's `src/lib.rs`.
    /// - **A symbol** goes through the same doors by its `Where`, and one
    ///   outside every root through `land_outside`, which finds its root as
    ///   `f12` does.
    /// - **A hit of *Text*** goes through `open_file_at` — `go_to_place` at
    ///   the hit's line and column.
    /// - **An action is dispatched after the popup has given the focus
    ///   back** — decision 15: `Window::dispatch_action` reads the focused
    ///   element when it is called and defers only the dispatch, so an
    ///   action dispatched first would be dispatched on a card that no
    ///   longer exists, and *Stretch to Right* would stretch nothing. A row
    ///   the focus behind the popup does not handle takes nothing.
    /// - ***More*** shows its tab.
    ///
    /// **`open_file_from` is never followed by `go_to_place`**: the second's
    /// own `note_place` would read the file just opened at 0:0 and record a
    /// place nobody stood in. Everything that opens something closes the
    /// popup with `Focus::Kept` first — the tab takes the focus.
    pub(crate) fn everywhere_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::file_index::Under;
        use crate::root::history::{Caret, Place};
        use crate::symbols::Where;

        let Some(item) = self.everywhere_item(row) else {
            return;
        };
        // A window with no project still has actions to find.
        let Some(project) = self.everywhere_project() else {
            return self.everywhere_take_unowned(item, window, cx);
        };
        match item {
            Item::Heading(_) => {}
            Item::Action(_) | Item::More(_) => self.everywhere_take_unowned(item, window, cx),
            Item::File(index) => {
                let Some(file) = self.ranked_files(project).get(index).cloned() else {
                    return;
                };
                let (_, place) = crate::everywhere::split_line(self.picker_query());
                // One-based as typed, zero-based as a place counts; a place
                // needs a line, and a column alone is no place.
                let place = place.and_then(|target| {
                    target.line.map(|line| {
                        (
                            line.saturating_sub(1),
                            target.column.map_or(0, |column| column.saturating_sub(1)),
                        )
                    })
                });
                self.close_picker(Focus::Kept, window, cx);
                match (file.under, place) {
                    (Under::Project, Some((line, column))) => {
                        self.go_to_place(project, Place::new(file.path, line, column), window, cx)
                    }
                    (Under::Project, None) => {
                        self.note_place(cx);
                        self.open_file(&file.path, window, cx);
                    }
                    (Under::Library(under), place) => self.go_to_library(
                        crate::Origin::External(under),
                        &file.path,
                        place.map(|(line, column)| Caret::Characters { line, column }),
                        window,
                        cx,
                    ),
                }
            }
            Item::Symbol(index) => {
                let Some(symbol) = self.ranked_symbols(project).get(index).cloned() else {
                    return;
                };
                self.close_picker(Focus::Kept, window, cx);
                match symbol.at {
                    Where::Project { path, line, column } => {
                        self.go_to_place(project, Place::new(path, line, column), window, cx)
                    }
                    Where::Library {
                        under,
                        path,
                        line,
                        column,
                    } => self.go_to_library(
                        crate::Origin::External(under),
                        &path,
                        Some(Caret::Characters { line, column }),
                        window,
                        cx,
                    ),
                    Where::Outside { path, line, column } => {
                        self.land_outside(&path, Caret::Characters { line, column }, window, cx)
                    }
                }
            }
            Item::Hit(index) => {
                let Some(hit) = self
                    .everywhere()
                    .and_then(|popup| popup.text.hits().get(index).cloned())
                else {
                    return;
                };
                self.close_picker(Focus::Kept, window, cx);
                self.open_file_at(hit.path, hit.line as usize, hit.column, window, cx);
            }
        }
    }

    /// The rows that belong to no project: an action, and *More*.
    fn everywhere_take_unowned(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        match item {
            Item::Action(index) => {
                let Some((action, available)) = self.everywhere_action(index) else {
                    return;
                };
                if !available {
                    return;
                }
                self.close_picker(Focus::Back, window, cx);
                window.dispatch_action(action.action(), cx);
            }
            Item::More(among) => self.everywhere_show(among, cx),
            Item::Heading(_) | Item::File(_) | Item::Symbol(_) | Item::Hit(_) => {}
        }
    }

    /// The action a row of *Actions* names, and whether it is available.
    pub(crate) fn everywhere_action(&self, index: usize) -> Option<(Named, bool)> {
        let popup = self.everywhere()?;
        let (action, _) = popup.found.get(index)?;
        let available = popup
            .actions
            .iter()
            .find(|(offered, _)| offered == action)
            .is_some_and(|(_, available)| *available);
        Some((*action, available))
    }

    /// What the indexes and the walk have said since the last look.
    /// Answers whether the rows changed.
    ///
    /// **The indexes answer on their own threads**, through
    /// `poll_indexes`, which runs just before this on the same clock; the
    /// walk is taken here. The popup keeps a fingerprint of what it last
    /// drew and says *changed* only when the rows, or whether an index is
    /// ready, are different. The preview is asked for the lit row here
    /// too, because the picker moves the highlight without telling the
    /// popup; a tick late is a twentieth of a second.
    pub(crate) fn everywhere_poll(&mut self, cx: &mut Context<Self>) -> bool {
        let walked = self.everywhere_mut().is_some_and(|popup| popup.text.take());
        let now = self.everywhere_fingerprint();
        let Some(popup) = self.everywhere_mut() else {
            return false;
        };
        let changed = walked || popup.seen != now;
        popup.seen = now;
        self.everywhere_preview(cx);
        changed
    }

    fn everywhere_fingerprint(&self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let Some(popup) = self.everywhere() else {
            return 0;
        };
        let wanted = crate::everywhere::wanted(popup.among, self.picker_query());
        if let Some(project) = self.everywhere_project() {
            if wanted.files {
                self.index_ready(project).hash(&mut hasher);
                for row in self.ranked_files(project) {
                    (&row.path, &row.marks, row.under, row.ignored).hash(&mut hasher);
                }
            }
            if wanted.symbols {
                self.symbols_ready(project).hash(&mut hasher);
                for row in self.ranked_symbols(project) {
                    (&row.name, &row.at, &row.marks).hash(&mut hasher);
                }
            }
        }
        if wanted.text {
            (popup.text.hits().len(), popup.text.running()).hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Ask the preview for the lit row, when the preview is on and the row
    /// is a place: a file at its first line, a symbol at its line, a hit
    /// with its matches. An action, a heading or *More* asks for nothing.
    fn everywhere_preview(&mut self, cx: &mut Context<Self>) {
        use crate::file_index::Under;
        use crate::symbols::Where;

        let Some(project) = self.everywhere_project() else {
            return;
        };
        let item = if self.settings.everywhere_preview {
            self.picker_lit_row()
                .and_then(|row| self.everywhere_item(row))
        } else {
            None
        };
        let asked = match item {
            Some(Item::File(index)) => self.ranked_files(project).get(index).map(|file| {
                let origin = match file.under {
                    Under::Project => crate::Origin::Project,
                    Under::Library(under) => crate::Origin::External(under),
                };
                crate::root::preview::Asked {
                    project,
                    origin,
                    path: file.path.clone(),
                    line: 0,
                    hit: None,
                }
            }),
            Some(Item::Symbol(index)) => {
                self.ranked_symbols(project)
                    .get(index)
                    .and_then(|symbol| match &symbol.at {
                        Where::Project { path, line, .. } => Some(crate::root::preview::Asked {
                            project,
                            origin: crate::Origin::Project,
                            path: path.clone(),
                            line: *line,
                            hit: None,
                        }),
                        Where::Library {
                            under, path, line, ..
                        } => Some(crate::root::preview::Asked {
                            project,
                            origin: crate::Origin::External(*under),
                            path: path.clone(),
                            line: *line,
                            hit: None,
                        }),
                        // Outside every root there is no origin to read
                        // it under.
                        Where::Outside { .. } => None,
                    })
            }
            Some(Item::Hit(index)) => self
                .everywhere()
                .and_then(|popup| popup.text.hits().get(index))
                .map(|hit| crate::root::preview::Asked {
                    project,
                    origin: crate::Origin::Project,
                    path: hit.path.clone(),
                    line: hit.line.saturating_sub(1),
                    hit: Some((hit.text.clone(), hit.ranges.clone())),
                }),
            _ => None,
        };
        let key = asked.as_ref().map(|asked| Previewed {
            origin: asked.origin,
            path: asked.path.clone(),
            line: asked.line,
        });
        let Some(popup) = self.everywhere_mut() else {
            return;
        };
        if popup.previewed == key {
            return;
        }
        popup.previewed = key;
        self.ask_preview(asked, cx);
    }

    /// The preview toggle at the head's right, remembered in the settings.
    pub(crate) fn everywhere_toggle_preview(&mut self, cx: &mut Context<Self>) {
        self.settings.everywhere_preview = !self.settings.everywhere_preview;
        self.store_settings();
        self.everywhere_preview(cx);
        cx.notify();
    }

    /// A watcher batch, while the popup is up: nothing.
    ///
    /// **The indexes follow the disk themselves** (`index_batch`, called
    /// beside this), so *Files* and *Symbols* are kept by their own
    /// threads. The *Text* tab's walk is not refreshed: a Search Everywhere
    /// popup is open for seconds, and the Find in Files popup is where a
    /// search is kept and follows the disk.
    pub(crate) fn everywhere_heard(&mut self, _project: ProjectId, _batch: &crate::watch::Batch) {}
}
