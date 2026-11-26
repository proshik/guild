//! Find in Files as a popup the picker holds — M8.9, decisions 12 to 14
//! and 16.
//!
//! **One engine, two holders** (decision 12): the popup owns a
//! `search::Walk` of its own, the thread M8.7 wrote for the Search tool and
//! moved into `desktop::search`, and it is refreshed by the tool's rules —
//! `search::touches` marks it stale and `search::refresh_due` asks again.
//! The popup and the tool share the query a project remembers.
//!
//! **Escape belongs to whatever holds the focus** (decision 14). The popup
//! reads its keys on its own card, so Escape closes it only while it holds
//! the focus; nothing is added to the root's capture phase, which would
//! take Escape from an editor or from `claude` in a terminal while a pinned
//! popup lingered. `cmd-shift-f` with the popup up and unfocused gives it
//! the focus back.
//!
//! The seven questions every popup answers (decision 1) are here; its card
//! is `view::find_in_files`. A method added to `Root` here starts with
//! `find_in_files_`.

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use gpui::prelude::*;
use gpui::{Context, Window};
use guild_session::ProjectId;

use crate::Root;
use crate::actions::{FindInFiles, ReplaceInFiles};
use crate::find_in_files::{self as rules, Field, Recall};
use crate::picker::{Key, Lit, Take};
use crate::root::picker::{Focus, Holds, Scroll};
use crate::search::{self, Query};

/// The head's three lines and the foot, in pixels, for the first size.
const HEAD: f32 = 152.0;

/// What the Find in Files popup keeps while it is up.
#[derive(Default)]
pub(crate) struct FindPopup {
    /// What is being looked for. Its text is the picker's query, copied
    /// here on every edit so that the walk and the toggles have one
    /// value to read.
    pub query: Query,
    /// Which field the letters go to.
    pub typing: Field,
    /// The popup's own walk.
    pub walk: search::Walk,
    /// Whether Enter leaves the popup up — the pin, for reading hits one
    /// after another without opening the popup again. A pinned popup has no
    /// backdrop.
    pub pinned: bool,
    /// When the disk first and last said something the answer on screen has
    /// not seen — M5.11's rule, the tool's.
    pub stale: Option<(Instant, Instant)>,
    /// Whether the mask is turned off: not applied, and kept in its field.
    pub mask_off: bool,
    /// The queries used before, newest first — the project's.
    pub history: Vec<String>,
    /// The masks used before, newest first.
    pub masks: Vec<String>,
    /// Where `alt-up` and `alt-down` are in the history of the field the
    /// letters go to — put back by an edit, a click or Tab.
    pub recall: Recall,
    pub replace_mode: bool,
    pub replacement_focus: bool,
    pub multiline: Option<gpui::Entity<gpui_component::input::TextareaState>>,
    pub multiline_subscription: Option<gpui::Subscription>,
    pub replacement: String,
    pub preserve_case: bool,
    pub excluded: std::collections::HashSet<std::path::PathBuf>,
    pub replace_status: String,
    pub applying: bool,
    pub apply_remaining: usize,
    pub apply_task: Option<gpui::Task<()>>,
    #[cfg(test)]
    validation_gate: Option<parity_tests::ValidationGate>,
}

// Paths and editor identities only: no filesystem reads or buffer copies on UI.
#[derive(Clone, PartialEq)]
struct OpenSearchAlias {
    path: PathBuf,
    file: gpui::Entity<crate::file_tab::FileTab>,
}

struct StopReplacement(Arc<AtomicBool>);
impl Drop for StopReplacement {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

fn resolve_search_aliases(
    paths: &[PathBuf],
    canonical: &Path,
    stop: &AtomicBool,
) -> Result<Vec<usize>, String> {
    let mut matching = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            return Err("Replacement cancelled".into());
        }
        let identity = path
            .canonicalize()
            .map_err(|error| format!("Could not validate open file {}: {error}", path.display()))?;
        if identity == canonical {
            matching.push(index);
        }
    }
    if stop.load(Ordering::Relaxed) {
        return Err("Replacement cancelled".into());
    }
    Ok(matching)
}

fn prepare_document_edits(
    text: &str,
    query: &Query,
    replacement: &str,
    preserve: bool,
    line: Option<u32>,
    cancelled: impl FnMut() -> bool,
) -> Result<Option<Vec<crate::document_search::Match>>, String> {
    match line {
        Some(line) => crate::document_search::first_on_line(
            text,
            query,
            replacement,
            preserve,
            line,
            cancelled,
        ),
        None => crate::document_search::matches_cancellable(
            text,
            query,
            replacement,
            preserve,
            cancelled,
        ),
    }
}

impl Root {
    /// `FindInFiles` opens the popup, or gives it the focus back.
    pub(crate) fn bind_find_in_files(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &FindInFiles, window, cx| {
            root.open_find_in_files(window, cx);
            if let Some(popup) = root.find_in_files_popup_mut() {
                popup.replace_mode = false;
            }
        }))
        .on_action(cx.listener(|root, _: &ReplaceInFiles, window, cx| {
            root.open_find_in_files(window, cx);
            if let Some(popup) = root.find_in_files_popup_mut() {
                popup.replace_mode = true;
            }
            cx.notify();
        }))
    }

    /// The popup, if it is the one up.
    pub(crate) fn find_in_files_popup(&self) -> Option<&FindPopup> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::FindInFiles(popup)) => Some(popup),
            _ => None,
        }
    }

    pub(crate) fn find_in_files_popup_mut(&mut self) -> Option<&mut FindPopup> {
        match self.picker.as_mut().map(|open| &mut open.holds) {
            Some(Holds::FindInFiles(popup)) => Some(popup),
            _ => None,
        }
    }

    /// `cmd-shift-f`.
    ///
    /// **With the popup up it gives it the focus back**, with its query lit
    /// whole so the first letter replaces it — decision 14: a pinned popup
    /// the person clicked away from is reached again by the stroke that
    /// opened it. Otherwise it opens on **the selection in the file in
    /// front** when that is one line (M5.11's `selection_for_search`), and
    /// on the query the project remembers when it is not — either way as a
    /// prefill.
    pub(crate) fn open_find_in_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_in_files_popup().is_some() {
            let text = self
                .find_in_files_popup()
                .map(|popup| popup.query.text.clone())
                .unwrap_or_default();
            if let Some(open) = self.picker.as_mut() {
                let focus = open.focus.clone();
                window.focus(&focus, cx);
                open.picker.prefill(text);
            }
            if let Some(popup) = self.find_in_files_popup_mut() {
                popup.typing = Field::Text;
                popup.replacement_focus = false;
                if let Some(input) = popup.multiline.clone() {
                    input.update(cx, |input, cx| input.select_all(window, cx));
                }
            }
            self.find_in_files_focus(window, cx);
            self.settle_picker();
            self.find_in_files_restart(cx);
            cx.notify();
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        // Read before the popup takes the focus: the selection is taken
        // only while the editor holds it.
        let selection = self.selection_for_search(window, cx);
        let mut popup = self.find_in_files_remembered(project);
        if let Some(text) = selection {
            popup.query.text = text;
        }
        let text = popup.query.text.clone();
        self.find_in_files_first_size();
        self.open_picker(
            Holds::FindInFiles(Box::new(popup)),
            Some(project),
            Scroll::uniform(),
            window,
            cx,
        );
        if let Some(open) = self.picker.as_mut() {
            open.picker.prefill(text);
        }
        // A preview left from the last popup is of a file this one has not
        // found yet.
        self.ask_preview(None, cx);
        self.find_in_files_restart(cx);
    }

    /// A card nobody has moved or sized opens at section 6.34's size rather
    /// than at `PickerPlace`'s default, which is Recent Projects'.
    ///
    /// **In memory only**: the settings are written when a hand lets go of
    /// the card, and a size nobody chose is not worth a write. A card a
    /// person has sized keeps what they chose.
    fn find_in_files_first_size(&mut self) {
        if self.settings.find_in_files != guild_session::PickerPlace::default() {
            return;
        }
        let rem = f32::from(crate::scale::rem_size(self.settings.ui_font_size));
        let space = |rems: gpui::Rems| rems.0 * rem;
        let place = &mut self.settings.find_in_files;
        place.width = space(crate::theme::space::FIND_IN_FILES).clamp(
            guild_session::PickerPlace::MIN_WIDTH,
            guild_session::PickerPlace::MAX_WIDTH,
        );
        place.height = (space(crate::theme::space::FIND_IN_FILES_LIST)
            + space(crate::theme::space::PREVIEW)
            + HEAD)
            .clamp(
                guild_session::PickerPlace::MIN_HEIGHT,
                guild_session::PickerPlace::MAX_HEIGHT,
            );
    }

    /// What the popup opens with: the project's remembered search — its
    /// history, its masks and whether the mask is off — with the Search
    /// tool's query over it when the tool has one in this window, which is
    /// the fresher of the two. **Never a scope** — M8.7's reason stands
    /// (decision 16).
    fn find_in_files_remembered(&self, project: ProjectId) -> FindPopup {
        let file = self
            .desk
            .project(project)
            .map(|p| guild_session::project_state::load(&p.path).search)
            .unwrap_or_default();
        let (query, mask_off) = match self.searches.get(&project) {
            Some(state) => {
                // The tool walks with the mask as applied; a mask the popup
                // turned off reaches it empty and is still the file's.
                let (mask, off) = rules::tool_mask(&file.mask, file.mask_off, &state.query.mask);
                (
                    Query {
                        mask,
                        scope: None,
                        ..state.query.clone()
                    },
                    off,
                )
            }
            None => (
                Query {
                    text: file.query,
                    case_sensitive: file.match_case,
                    whole_word: file.whole_word,
                    regular_expression: file.regex,
                    mask: file.mask,
                    scope: None,
                },
                file.mask_off,
            ),
        };
        FindPopup {
            query,
            mask_off,
            history: file.history,
            masks: file.masks,
            ..FindPopup::default()
        }
    }

    /// Keep what the popup has used with its project — the query, the
    /// toggles, the mask and whether it is off, and the two histories —
    /// through the one door the Search tool writes by too, which keeps
    /// what it does not set.
    ///
    /// `used` puts the query and the mask at the head of their histories:
    /// **a query is remembered when it is used**, by Enter or *Open in Find
    /// Window*, not per keystroke. A tick of the mask's checkbox writes
    /// without it, so that a restart finds the mask as it was left.
    pub(crate) fn find_in_files_remember(&mut self, used: bool) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        if used {
            rules::remember(&mut popup.history, &popup.query.text, rules::QUERIES);
            if !popup.mask_off {
                rules::remember(&mut popup.masks, &popup.query.mask, rules::MASKS);
            }
        }
        let query = popup.query.clone();
        let (mask_off, history, masks) =
            (popup.mask_off, popup.history.clone(), popup.masks.clone());
        self.write_search(project, |search| {
            search.query = query.text;
            search.match_case = query.case_sensitive;
            search.whole_word = query.whole_word;
            search.regex = query.regular_expression;
            search.mask = query.mask;
            search.mask_off = mask_off;
            search.history = history;
            search.masks = masks;
        });
    }

    /// Stop the popup's walk and start it again from its query.
    ///
    /// What is on screen stays until the new walk answers — the walk's own
    /// rule.
    pub(crate) fn find_in_files_restart(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let buffers = self.project_search_buffers(project, cx);
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.stale = None;
            popup.apply_task = None;
            popup.applying = false;
            let query = rules::query_for(&popup.query, popup.mask_off);
            popup.walk.restart_documents(&root, query, buffers);
        }
        cx.notify();
    }

    /// The popup's rows, as far as lighting them goes: every hit is a row.
    pub(crate) fn find_in_files_lit(&self) -> Vec<Lit> {
        self.find_in_files_popup()
            .map_or_else(Vec::new, |popup| vec![Lit::Row; popup.walk.hits().len()])
    }

    /// A key only this popup reads. Answers whether it took it.
    ///
    /// The toggles by their strokes; Tab between the two fields; `alt-up`
    /// and `alt-down` through the history of the field the letters go to;
    /// the letters themselves while they go to the mask, whose line is not
    /// the picker's. **Escape is the picker's** — it closes the popup, and
    /// the picker hears a key only while the card holds the focus (decision
    /// 14). A chevron's history is a menu of the engine since M12.7 task
    /// 7, and the root's capture phase closes it first, ahead of this.
    pub(crate) fn find_in_files_key(
        &mut self,
        key: &Key,
        stroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some(toggle) = rules::toggle(stroke) {
            self.find_in_files_toggle(toggle, cx);
            return true;
        }
        let Some(popup) = self.find_in_files_popup_mut() else {
            return false;
        };
        match key {
            // *Open in Find Window* whether or not a row is lit: it is the
            // question that is moved, not a hit.
            Key::Take(Take::Aside) => {
                self.find_in_files_to_window(window, cx);
                true
            }
            Key::NextTab | Key::PreviousTab => {
                let fields = if popup.replace_mode { 3 } else { 2 };
                let at = if popup.typing == Field::Mask {
                    fields - 1
                } else if popup.replacement_focus {
                    1
                } else {
                    0
                };
                let next = if *key == Key::NextTab {
                    (at + 1) % fields
                } else {
                    (at + fields - 1) % fields
                };
                popup.typing = if next == fields - 1 {
                    Field::Mask
                } else {
                    Field::Text
                };
                popup.replacement_focus = popup.replace_mode && next == 1;
                popup.recall = Recall::default();
                self.find_in_files_focus(window, cx);
                cx.notify();
                true
            }
            Key::History(by) => {
                self.find_in_files_recall(*by, window, cx);
                true
            }
            _ => false,
        }
    }

    /// Give the card the focus — from a press on one of its controls.
    ///
    /// **A control stops the press**, so that it is not also the head's grab
    /// of the card; and the card's own focus-on-press is a listener of the
    /// card's, which a stopped press never reaches. A pinned card the person
    /// clicked away from would otherwise light a field and hear none of
    /// its letters — the Log toolbar's trap.
    pub(crate) fn find_in_files_focus(&self, window: &mut Window, cx: &mut gpui::App) {
        if let Some(popup) = self.find_in_files_popup()
            && popup.typing == Field::Text
            && !popup.replacement_focus
            && let Some(input) = &popup.multiline
        {
            input.update(cx, |input, cx| input.focus(window, cx));
        } else if let Some(open) = self.picker.as_ref() {
            window.focus(&open.focus, cx);
        }
    }

    /// Put the letters in one of the two fields.
    pub(crate) fn find_in_files_type_into(&mut self, field: Field, cx: &mut Context<Self>) {
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.replacement_focus = false;
            popup.typing = field;
            popup.recall = Recall::default();
            cx.notify();
        }
    }

    /// `alt-up` (`by` negative) or `alt-down` through the history of the
    /// field the letters go to.
    fn find_in_files_recall(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.picker_query().to_owned();
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        let (list, current) = match popup.typing {
            Field::Text => (&popup.history, typed),
            Field::Mask => (&popup.masks, popup.query.mask.clone()),
        };
        let found = if by < 0 {
            popup.recall.older(list, &current)
        } else {
            popup.recall.newer(list)
        };
        let Some(text) = found else {
            return;
        };
        let field = popup.typing;
        self.find_in_files_put(field, text, false, window, cx);
    }

    /// Put a text in a field, as if typed — from the history, by a key or
    /// from a chevron's menu — and search again.
    ///
    /// `fresh` starts the field's walk through its history again, which a
    /// row of the menu does and a step of `alt-up` must not.
    pub(crate) fn find_in_files_put(
        &mut self,
        field: Field,
        text: String,
        fresh: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match field {
            Field::Text => {
                if let Some(open) = self.picker.as_mut() {
                    open.picker.set_query(text.clone());
                }
                if let Some(popup) = self.find_in_files_popup_mut() {
                    if let Some(input) = &popup.multiline {
                        input.update(cx, |input, cx| {
                            if input.value().as_ref() != text {
                                input.set_value(text.clone(), window, cx);
                            }
                            input.set_selected_range(0..text.len(), cx);
                        });
                    }
                    popup.query.text = text;
                }
            }
            Field::Mask => {
                if let Some(popup) = self.find_in_files_popup_mut() {
                    popup.query.mask = text;
                }
            }
        }
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.replacement_focus = false;
            popup.typing = field;
            if fresh {
                popup.recall = Recall::default();
            }
        }

        self.find_in_files_restart(cx);
        self.settle_picker();
    }

    /// Tick the mask's checkbox, or untick it: the walk is asked again, and
    /// the project keeps the choice — a restart finds the mask as it was
    /// left, applied or not.
    pub(crate) fn find_in_files_use_mask(&mut self, cx: &mut Context<Self>) {
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.mask_off = !popup.mask_off;
            self.find_in_files_remember(false);
            self.find_in_files_restart(cx);
        }
    }

    /// Turn one of the three toggles, and search again.
    pub(crate) fn find_in_files_toggle(&mut self, toggle: search::Toggle, cx: &mut Context<Self>) {
        if let Some(popup) = self.find_in_files_popup_mut() {
            toggle.turn(&mut popup.query);
            self.find_in_files_restart(cx);
        }
    }

    /// Pin the popup, or take the pin out.
    pub(crate) fn find_in_files_pin(&mut self, cx: &mut Context<Self>) {
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.pinned = !popup.pinned;
            cx.notify();
        }
    }

    /// The query changed.
    pub(crate) fn find_in_files_edited(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let text = self.picker_query().to_owned();
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        if popup.query.text == text {
            return;
        }
        popup.query.text = text;
        popup.recall = Recall::default();
        self.find_in_files_restart(cx);
    }

    /// A hit taken: its file, at its match.
    ///
    /// **Through `open_file_at`**, M5.11's door to `go_to_place`, so Back
    /// returns from it and a restored tab lands too; the match is selected
    /// as the Search tool selects it. **Opening a tab focuses it**, so an
    /// unpinned popup closes with `Focus::Kept`; a pinned one stays and
    /// takes the focus back after the tab took it, so the next Down is the
    /// next hit.
    pub(crate) fn find_in_files_take(
        &mut self,
        row: usize,
        take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if take == Take::Aside {
            self.find_in_files_to_window(window, cx);
            return;
        }
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        if self.active_project != Some(project) {
            return;
        }
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(popup) = self.find_in_files_popup() else {
            return;
        };
        let Some(hit) = popup.walk.hits().get(row).cloned() else {
            return;
        };
        // **A hit whose file may have moved since the answer lands where its
        // text is now** — the tool's rule, M5.11's decision 4.
        let (mut line, mut column) = (hit.line, hit.column);
        if popup.stale.is_some()
            && let Ok(text) = std::fs::read_to_string(root.join(&hit.path))
            && let Some(now) = search::relocate(&text, &popup.query, &hit)
        {
            (line, column) = now;
        }
        let pinned = popup.pinned;
        let chars = hit
            .ranges
            .first()
            .and_then(|range| hit.text.get(range.clone()))
            .map_or(0, |matched| matched.chars().count());

        self.find_in_files_remember(true);
        self.open_file_at(hit.path.clone(), line as usize, column, window, cx);
        let place = crate::root::history::Place::new(hit.path, line.saturating_sub(1), column);
        self.select_on_landing(project, place, chars, window, cx);
        if pinned {
            if let Some(open) = self.picker.as_ref() {
                let focus = open.focus.clone();
                window.focus(&focus, cx);
            }
            cx.notify();
        } else {
            self.close_picker(Focus::Kept, window, cx);
        }
    }

    /// What the walk has said. Answers whether the rows changed.
    ///
    /// **And the one place a stale answer is asked again**, by the tool's
    /// rule: quiet for 300 ms, or stale for two seconds, and never over a
    /// walk still running.
    pub(crate) fn find_in_files_poll(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(popup) = self.find_in_files_popup_mut() else {
            return false;
        };
        let moved = popup.walk.take();
        let due = !popup.applying
            && popup.stale.is_some_and(|(first, last)| {
                search::refresh_due(first, last, Instant::now(), popup.walk.running())
            });
        if due {
            self.find_in_files_restart(cx);
        }
        if moved {
            self.settle_picker();
        }
        // **The preview follows the lit row** — asked on this clock, which
        // is where every change of the lit row has landed by: an arrow, the
        // pointer, a new answer. The same ask again costs nothing.
        let asked = self.find_in_files_asked();
        self.ask_preview(asked, cx);
        moved
    }

    /// What the lit row wants previewed: its file at its line, with the hit
    /// so its match can be marked.
    fn find_in_files_asked(&self) -> Option<crate::root::preview::Asked> {
        let open = self.picker.as_ref()?;
        let project = open.project?;
        let hit = self
            .find_in_files_popup()?
            .walk
            .hits()
            .get(open.picker.lit()?)?;
        Some(crate::root::preview::Asked {
            project,
            origin: crate::Origin::Project,
            path: hit.path.clone(),
            line: hit.line.saturating_sub(1),
            hit: Some((hit.text.clone(), hit.ranges.clone())),
        })
    }

    /// A watcher batch, while the popup is up: a change inside the query's
    /// scope makes the answer stale.
    pub(crate) fn find_in_files_heard(&mut self, project: ProjectId, batch: &crate::watch::Batch) {
        if self.picker.as_ref().and_then(|open| open.project) != Some(project) {
            return;
        }
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        if popup.query.is_empty() || !search::touches(&popup.query, batch.files()) {
            return;
        }
        let now = Instant::now();
        popup.stale = Some(match popup.stale {
            Some((first, _)) => (first, now),
            None => (now, now),
        });
    }

    /// Look in one directory, or in the whole project again.
    pub(crate) fn find_in_files_scope(&mut self, scope: rules::Scope, cx: &mut Context<Self>) {
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        let scope = scope.relative().map(std::path::Path::to_path_buf);
        if popup.query.scope == scope {
            return;
        }
        popup.query.scope = scope;
        self.find_in_files_restart(cx);
    }

    /// The *Directory* chip: the Explorer's selected row — a directory
    /// itself, a file its directory — and the platform's chooser when the
    /// Explorer has nothing selected but the project.
    pub(crate) fn find_in_files_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        let root = self.desk.project(project).map(|p| p.path.clone());
        let selected = self
            .trees
            .get(&self.seat_for_project(project))
            .and_then(|tree| tree.selected.clone());
        let scope = match (root, selected) {
            (Some(root), Some(path)) => rules::scope_of_row(&path, root.join(&path).is_dir()),
            _ => rules::Scope::Project,
        };
        match scope {
            rules::Scope::Project => self.find_in_files_choose(window, cx),
            scope => self.find_in_files_scope(scope, cx),
        }
    }

    /// `…`: a folder from the platform's chooser, **refused outside the
    /// project** in M5.12's words for a path outside it — a search's hits are
    /// the project's paths, and a folder beside the project has none.
    pub(crate) fn find_in_files_choose(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        let Some(project_root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        // The chooser is the system's and does not return until it is
        // answered, so it is awaited in a task; the answer comes back to the
        // popup only while it is still up over the same project.
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(chosen))) = paths.await else {
                return;
            };
            let Some(chosen) = chosen.into_iter().next() else {
                return;
            };
            let _ = this.update_in(cx, |root, window, cx| {
                if root.picker.as_ref().and_then(|open| open.project) != Some(project)
                    || root.find_in_files_popup().is_none()
                {
                    return;
                }
                match rules::scope_of_chosen(&project_root, &chosen) {
                    Some(scope) => root.find_in_files_scope(scope, cx),
                    None => {
                        let said = root.ui().notice_link_outside(&chosen.display().to_string());
                        root.inform(said);
                    }
                }
                root.find_in_files_focus(window, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// `cmd-enter`, *Open in Find Window*: the question goes to the Search
    /// tool — the query, the toggles, the mask as applied and the scope — its
    /// walk starts, the tool is revealed with the keyboard in its field, and
    /// the popup closes.
    ///
    /// **The query, not the hits** (decision 12): a hit list moved between
    /// two owners is two generations to keep in step, and a walk is
    /// milliseconds. It is a use, so the query is remembered first.
    pub(crate) fn find_in_files_to_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        let Some(popup) = self.find_in_files_popup() else {
            return;
        };
        let query = rules::query_for(&popup.query, popup.mask_off);
        self.find_in_files_remember(true);
        self.close_picker(Focus::Kept, window, cx);
        self.search_in_window(project, query, window, cx);
    }
}

impl Root {
    pub(crate) fn project_search_buffers(
        &self,
        project: ProjectId,
        cx: &gpui::App,
    ) -> std::collections::HashMap<std::path::PathBuf, crate::project_search::Buffer> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .filter_map(|body| match body {
                crate::Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } => tab
                    .read(cx)
                    .search_buffer(cx)
                    .map(|buffer| (path.clone(), buffer)),
                _ => None,
            })
            .collect()
    }
    fn search_alias_roster(&self) -> Vec<OpenSearchAlias> {
        let mut aliases = Vec::new();
        for tab in &self.tabs {
            let Some(project) = self.desk.project(tab.project) else {
                continue;
            };
            for body in tab.bodies() {
                if let crate::Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } = body
                {
                    aliases.push(OpenSearchAlias {
                        path: project.path.join(path),
                        file: tab.clone(),
                    });
                }
            }
        }
        aliases
    }
    pub(crate) fn find_in_files_identity(
        &self,
        path: &std::path::Path,
    ) -> Option<&std::path::PathBuf> {
        self.find_in_files_popup()?
            .walk
            .documents()
            .iter()
            .find(|document| document.path == path)
            .map(|document| &document.canonical)
    }
    pub(crate) fn find_in_files_replace(
        &mut self,
        all: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return;
        };
        if self.active_project != Some(project) {
            return;
        }
        let Some(root_path) = self
            .desk
            .project(project)
            .map(|project| project.path.clone())
        else {
            return;
        };
        let selected = self.picker.as_ref().and_then(|open| open.picker.lit());
        let Some(popup) = self.find_in_files_popup() else {
            return;
        };
        if popup.query.is_empty()
            || popup.applying
            || popup.walk.running()
            || popup.walk.capped()
            || popup.walk.refused().is_some()
            || popup.walk.answered() != popup.walk.generation()
        {
            return;
        }
        let generation = popup.walk.generation();
        let query = popup.query.clone();
        let replacement = popup.replacement.clone();
        let preserve = popup.preserve_case;
        let hit = selected.and_then(|row| popup.walk.hits().get(row)).cloned();
        if !all && hit.is_none() {
            return;
        }
        let snapshots: Vec<_> = popup
            .walk
            .documents()
            .iter()
            .filter(|document| {
                !popup.excluded.contains(&document.canonical)
                    && (all || hit.as_ref().is_some_and(|hit| hit.path == document.path))
            })
            .cloned()
            .collect();
        if snapshots.is_empty() {
            return;
        }
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.applying = true;
            popup.apply_remaining = snapshots.len();
            popup.replace_status = "0 replaced; 0 refused".into();
        }
        let task = cx.spawn_in(window, async move |this, cx| {
            let stop = Arc::new(AtomicBool::new(false));
            let _stop_on_drop = StopReplacement(stop.clone());
            let prepare_stop = stop.clone();
            let validation_root = root_path.clone();
            let prepared = cx
                .background_executor()
                .spawn(async move {
                    let mut prepared_bytes = 0usize;
                    snapshots
                        .into_iter()
                        .take_while(|_| !prepare_stop.load(Ordering::Relaxed))
                        .map(|snapshot| {
                            let result = snapshot.validate(&root_path).and_then(|()| {
                                let found = prepare_document_edits(&snapshot.text, &query, &replacement, preserve,
                                    if all { None } else { Some(hit.as_ref().expect("selected hit").line) },
                                    || prepare_stop.load(Ordering::Relaxed))?
                                    .ok_or_else(|| "Replacement cancelled".to_owned())?;
                                let count = found.len();
                                let removed: usize = found.iter().map(|edit| edit.range.end - edit.range.start).sum();
                                let inserted: usize = found.iter().map(|edit| edit.replacement.len()).sum();
                                let output = snapshot.text.len().saturating_sub(removed).saturating_add(inserted);
                                if prepared_bytes.saturating_add(output) > 64 * 1024 * 1024 {
                                    return Err("Replacement exceeds retained editor budget; narrow the search".into());
                                }
                                prepared_bytes += output;
                                Ok((found, count))
                            });
                            (snapshot, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let mut applied = 0usize;
            let mut refused = Vec::new();
            for (snapshot, result) in prepared {
                // Capture only paths/entities on UI. Resolve identities off-thread,
                // then validate the target disk last, immediately before the UI edit.
                let Some(aliases) = this.update(cx, |root, _| {
                    root.find_in_files_popup().filter(|popup| popup.applying && popup.walk.generation() == generation)
                        .map(|_| root.search_alias_roster())
                }).ok().flatten() else { return; };
                let paths: Vec<_> = aliases.iter().map(|alias| alias.path.clone()).collect();
                let disk_root = validation_root.clone();
                let validation_stop = stop.clone();
                #[cfg(test)]
                let gate = this.update(cx, |root, _| root.find_in_files_popup_mut().and_then(|popup| popup.validation_gate.take())).ok().flatten();
                let (snapshot, validation) = cx.background_executor().spawn(async move {
                    #[cfg(test)]
                    if let Some(gate) = gate {
                        let _ = gate.entered.send(validation_stop.clone());
                        let _ = gate.release.await;
                    }
                    let validation = resolve_search_aliases(&paths, &snapshot.canonical, &validation_stop)
                        .and_then(|matching| snapshot.validate(&disk_root).map(|()| matching));
                    (snapshot, validation)
                }).await;
                let result = validation.and_then(|matching| result.map(|plan| (matching, plan)));
                let continuing = this
                    .update_in(cx, |root, window, cx| {
                        if root.active_project != Some(project)
                            || root.picker.as_ref().and_then(|open| open.project) != Some(project)
                            || !root.find_in_files_popup().is_some_and(|popup| {
                                popup.walk.generation() == generation && popup.applying
                            })
                        {
                            return false;
                        }
                        let result = result.and_then(|(matching, (edits, count))| {
                            if root.desk.project(project).map(|project| &project.path) != Some(&validation_root)
                                || root.search_alias_roster() != aliases
                            {
                                return Err("Open files changed during validation; search again".into());
                            }
                            // Only the resolved aliases need live content comparison. No
                            // filesystem calls, clean-buffer copies, or stale editor choice.
                            for index in matching {
                                if !aliases[index].file.read(cx).search_text_is(&snapshot.text, cx) {
                                    return Err("An open buffer alias changed since this search; search again".into());
                                }
                            }
                            let existing = root
                                .tabs
                                .iter()
                                .filter(|tab| tab.project == project)
                                .flat_map(crate::Tab::bodies)
                                .find_map(|body| match body {
                                    crate::Body::File {
                                        path,
                                        tab,
                                        origin: crate::Origin::Project,
                                        ..
                                    } if *path == snapshot.path => Some(tab.clone()),
                                    _ => None,
                                });
                            let file = if let Some(file) = existing {
                                file
                            } else {
                                let active_tab = root.active_tab;
                                root.open_file(&snapshot.path, window, cx);
                                root.active_tab = active_tab;
                                root.save();
                                let Some(file) = root
                                    .tabs
                                    .iter()
                                    .filter(|tab| tab.project == project)
                                    .flat_map(crate::Tab::bodies)
                                    .find_map(|body| match body {
                                        crate::Body::File {
                                            path,
                                            tab,
                                            origin: crate::Origin::Project,
                                            ..
                                        } if *path == snapshot.path => Some(tab.clone()),
                                        _ => None,
                                    })
                                else {
                                    return Err("Could not open editable file".into());
                                };
                                file
                            };
                            file.update(cx, |file, cx| {
                                file.replace_snapshot(&snapshot, edits, window, cx)
                            })?;
                            applied += count;
                            Ok(())
                        });
                        if let Err(error) = result {
                            refused.push(format!("{}: {error}", snapshot.path.display()));
                        }
                        if let Some(open) = root.picker.as_ref() {
                            window.focus(&open.focus, cx);
                        }
                        if let Some(popup) = root.find_in_files_popup_mut() {
                            popup.apply_remaining = popup.apply_remaining.saturating_sub(1);
                            popup.replace_status =
                                format!("{applied} replaced; {} refused; {} files unprocessed", refused.len(), popup.apply_remaining);
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !continuing {
                    return;
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(1))
                    .await;
            }
            let _ = this.update_in(cx, |root, _, cx| {
                if let Some(popup) = root
                    .find_in_files_popup_mut()
                    .filter(|popup| popup.walk.generation() == generation)
                {
                    popup.applying = false;
                    popup.replace_status = format!(
                        "{applied} replaced in editor buffers; {} refused{}",
                        refused.len(),
                        if refused.is_empty() {
                            String::new()
                        } else {
                            format!(" — {}", refused.join("; "))
                        }
                    );
                }
                root.find_in_files_restart(cx);
            });
        });
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.apply_task = Some(task);
        }
        cx.notify();
    }
}

impl Root {
    pub(crate) fn find_in_files_multiline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(popup) = self.find_in_files_popup_mut() else {
            return;
        };
        if popup.multiline.take().is_some() {
            popup.multiline_subscription = None;
            self.find_in_files_focus(window, cx);
            cx.notify();
            return;
        }
        let text = popup.query.text.clone();
        let state =
            cx.new(|cx| gpui_component::input::TextareaState::new(window, cx).default_value(text));
        let subscription = cx.subscribe_in(
            &state,
            window,
            |root, input, event: &gpui_component::input::InputEvent, window, cx| {
                if matches!(event, gpui_component::input::InputEvent::Change) {
                    if let Some(open) = root.picker.as_mut() {
                        open.picker.set_query(input.read(cx).value().to_string());
                    }
                    root.find_in_files_edited(window, cx);
                }
            },
        );
        state.update(cx, |state, cx| state.focus(window, cx));
        if let Some(popup) = self.find_in_files_popup_mut() {
            popup.multiline = Some(state);
            popup.multiline_subscription = Some(subscription);
        }
        cx.notify();
    }
}

#[cfg(test)]
mod parity_tests;
