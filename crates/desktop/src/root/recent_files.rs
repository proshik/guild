//! The files opened lately, newest first — `cmd-e`'s list.
//!
//! Files is a selector: a named header, edited-only choice and project MRU
//! rows. Printable keys never create a hidden query or reach the covered editor.
//! Locations retains its existing text query and navigation-history excerpts.
//! The opening highlight skips the current file when another recent file exists;
//! Backspace uses the ordinary guarded close and forgets a row only on success.
//!
//! A tool-window column is deliberately absent: Guild already has its
//! own tool boundaries and shortcuts. Floating movement is shared with the other
//! pickers; the first-focus and drag adapters own no file history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use guild_session::ProjectId;

/// How many files a project remembers.
pub const REMEMBERED: usize = 50;

/// What the popup is a list of.
///
/// Two faces of one popup rather than two popups. The query, the
/// highlight, the arrows and Enter mean the same thing in both, and a
/// second copy of that is a second chance for them to disagree — which
/// is the failure this crate has a named example of.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Face {
    /// The files opened lately — `cmd-e`.
    #[default]
    Files,
    /// The places stood in lately, with the code around each —
    /// `cmd-shift-e`. **The same stack `Back` walks**, read rather than
    /// stepped through.
    Locations,
}

/// How many lines of code a location shows.
pub const EXCERPT: usize = 5;

/// One row of the Locations face: a place and the code around it.
#[derive(Debug, Clone)]
pub struct Location {
    /// Where it is.
    pub place: crate::root::history::Place,
    /// The lines around it, in order.
    pub lines: Vec<String>,
    /// The number of the first of them, counted from one.
    pub first: u32,
}

/// What the popup keeps of its own — the query and the highlight are the
/// picker's.
#[derive(Debug, Default)]
pub struct Popup {
    /// Which list is being shown.
    pub face: Face,
    /// The rows of the Locations face, taken once when it opens.
    ///
    /// A snapshot rather than a live read: an excerpt costs a file read
    /// for every row not open in a tab, and a popup that did that per
    /// frame would read twenty-five files sixty times a second.
    pub locations: Vec<Location>,
    /// Only the files this session has edited — the second `cmd-e`.
    pub edited: bool,
    /// Backspace forgets a row only after its guarded close actually succeeds.
    pending_close: Option<PathBuf>,
}

/// The rows a query leaves, each carrying its place in the full list.
///
/// The place travels with the row because everything the popup does to a
/// row counts the full list, and recomputing the index from the path
/// afterwards would be the same lookup done twice with two chances to
/// disagree. The Recent Projects popup keeps its rows the same way and
/// for the same reason.
///
/// **Found by scattered letters and ranked best first** since M8.9 —
/// `crate::fuzzy` in path mode, so `rhis` is `root/history.rs`. Nothing
/// typed keeps the list's own order, newest first, which is the answer to
/// *nothing typed*.
pub fn shown<'a>(query: &str, files: &'a [PathBuf]) -> Vec<(usize, &'a Path)> {
    let haystacks: Vec<String> = files
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    crate::fuzzy::rank(query, &haystacks, String::as_str, crate::fuzzy::Shape::Path)
        .into_iter()
        .map(|(index, _)| (index, files[index].as_path()))
        .collect()
}

/// Which row the highlight opens on.
///
/// **Row 1 when row 0 is the file already in front.** That one line is
/// what makes `cmd-e` Enter a switch to the last file instead of a way
/// of reopening the file being looked at — and it is the first thing a
/// person notices missing.
pub fn opening_row(rows: usize, first_is_front: bool) -> usize {
    if first_is_front && rows > 1 { 1 } else { 0 }
}

/// Put a path at the head of the list, once, within the limit.
pub fn remember(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|kept| kept != path);
    list.insert(0, path.to_path_buf());
    list.truncate(REMEMBERED);
}

/// Take a path out of the list.
pub fn forget(list: &mut Vec<PathBuf>, path: &Path) {
    list.retain(|kept| kept != path);
}

/// Whether this keystroke closes the file the lit row names, rather than
/// being a key for the query.
///
/// While nothing is typed on the Files face — the Switcher's gesture.
///
/// **The Backspace key, by its name**, with whatever modifier: that is
/// what the popup matched before M8.9. Read by what it means to the line
/// instead, `ctrl-w` and `ctrl-u` — readline's *take a word* and *take the
/// line*, which a shell hand presses on an empty field out of habit —
/// closed a file too.
pub fn closes_the_lit_file(stroke: &gpui::Keystroke, files_face: bool, query: &str) -> bool {
    stroke.key == "backspace" && files_face && query.is_empty()
}

impl Location {
    /// Does the query touch this row?
    ///
    /// The path **and** the code: a person looking for where they saw a
    /// `TODO` is typing the line, not the file it is in.
    pub fn matches(&self, needle: &str) -> bool {
        self.place
            .path
            .to_string_lossy()
            .to_lowercase()
            .contains(needle)
            || self
                .lines
                .iter()
                .any(|line| line.to_lowercase().contains(needle))
    }
}

/// The lines around a place, and the number of the first of them.
///
/// [`EXCERPT`] lines centred on the place, clamped at both ends of the
/// file — near the top there is nothing above to show, and padding the
/// gap with blank rows would draw a file that does not exist. The number
/// comes back with them because a row without its line numbers is an
/// excerpt of nowhere.
///
/// Pure, and given the text rather than the path: the caller knows
/// whether to read a buffer or the disk, and this does not want to know.
pub fn excerpt(text: Option<&str>, line: u32) -> (Vec<String>, u32) {
    let Some(text) = text else {
        return (Vec::new(), line + 1);
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return (Vec::new(), 1);
    }
    let above = (EXCERPT - 1) / 2;
    let last = lines.len() - 1;
    let at = (line as usize).min(last);
    let first = at.saturating_sub(above);
    let first = first.min(lines.len().saturating_sub(EXCERPT));
    let end = (first + EXCERPT).min(lines.len());

    (
        lines[first..end]
            .iter()
            .map(|line| line.to_string())
            .collect(),
        first as u32 + 1,
    )
}

/// The files opened lately, newest first — `cmd-e`'s list.
///
/// **Persisted**, unlike the navigation history beside it — decision 9 of
/// M8.8. This is paths, and a path survives a restart in a way a line
/// number does not.
#[derive(Debug, Default)]
pub(crate) struct RecentFiles {
    /// The list per project, newest first, read from `ProjectState` on
    /// first use.
    lists: HashMap<ProjectId, Vec<PathBuf>>,
    /// Which file was last seen in front, so a change can be noticed.
    ///
    /// Opens/reopens record immediately; the watch sampler also notices
    /// restored tabs and other routes to the front.
    front: Option<(ProjectId, PathBuf)>,
}

impl RecentFiles {
    /// A project's list, read with `load` the first time anything asks.
    ///
    /// **`N1`**: the list was read only when a project file came to the
    /// front, so a window restored onto a terminal had read nothing and
    /// `cmd-e` said no file had ever been opened. Every reader goes
    /// through here, and the first one reads the state file.
    fn lists_for(
        &mut self,
        project: ProjectId,
        load: impl FnOnce() -> Vec<PathBuf>,
    ) -> &mut Vec<PathBuf> {
        self.lists.entry(project).or_insert_with(|| {
            let mut paths = load();
            let mut seen = std::collections::HashSet::new();
            paths.retain(|path| seen.insert(path.clone()));
            paths.truncate(REMEMBERED);
            paths
        })
    }
}

use gpui::prelude::*;
use gpui::{Context, Window};

use crate::actions::{RecentFiles as RecentFilesAction, RecentLocations};
use crate::picker::{Key, Lit, Picker, Take};
use crate::root::picker::{Focus, Holds, Scroll};
use crate::{Body, Root, Tab};

impl Root {
    /// The Recent Files and Recent Locations handlers.
    pub(crate) fn bind_recent_files(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &RecentFilesAction, window, cx| {
            root.toggle_recent(Face::Files, window, cx)
        }))
        .on_action(cx.listener(|root, _: &RecentLocations, window, cx| {
            root.toggle_recent(Face::Locations, window, cx)
        }))
    }

    /// The places lately stood in, with the code around each.
    ///
    /// Taken once, when the face opens. The excerpt comes from the open
    /// buffer when there is one and from the disk otherwise — the same
    /// order a diff reads a working tree in, and for the same reason: a
    /// line a person is looking at is the line they last typed, not the
    /// one on disk.
    fn locations_now(&self, cx: &Context<Self>) -> Vec<Location> {
        let Some(project) = self.active_project else {
            return Vec::new();
        };
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return Vec::new();
        };
        self.history
            .locations(
                self.active_workspace,
                project,
                self.history.standing.as_ref(),
                crate::root::history::LOCATIONS,
            )
            .into_iter()
            .map(|place| {
                let text = match self.file_editor_at(project, &place.path, cx) {
                    Some(editor) => Some(editor.read(cx).text().to_string()),
                    None => std::fs::read_to_string(root.join(&place.path)).ok(),
                };
                let (lines, first) = excerpt(text.as_deref(), place.line);
                Location {
                    place,
                    lines,
                    first,
                }
            })
            .collect()
    }

    /// Notice which file is in front, and remember it if it changed.
    ///
    /// On the watch loop's clock beside the caret sampler, and for the
    /// same reason: there is no one place a file becomes the front one,
    /// and a list kept from `open_file` alone would miss a tab restored
    /// at start-up and a file `Back` reopened.
    pub(crate) fn note_front_file(&mut self) {
        // **`file_in_front` and not a second reader of the same tab.**
        // It answers `None` for a tab under a dependency's root, which
        // is the whole of the fix: this list is written into the
        // project's own `state.json` as project-relative paths, and a
        // crate's `src/lib.rs` in it comes back as this repository's.
        // M8.8.2 had a `front_file` of its own here, written in a
        // worktree where `Origin` did not exist yet.
        let Some((project, path)) = self.file_in_front() else {
            return;
        };
        if self.recent_files.front.as_ref() == Some(&(project, path.clone())) {
            return;
        }
        self.recent_files.front = Some((project, path.clone()));
        remember(self.recent_files_list(project), &path);
        self.store_recent_files(project);
    }

    /// This project's list, read from its state file on first use.
    fn recent_files_list(&mut self, project: ProjectId) -> &mut Vec<PathBuf> {
        let root = self
            .desk
            .project(project)
            .map(|project| project.path.clone());
        self.recent_files.lists_for(project, || {
            root.map(|root| guild_session::project_state::load(&root).recent_files)
                .unwrap_or_default()
        })
    }

    /// This project's recent files, newest first — what Go to File's
    /// empty query asks (M8.9 decision 10), read from the state file if
    /// nothing has read it yet.
    #[allow(
        dead_code,
        reason = "Go to File's empty query asks it — M8.9 task 8, group A"
    )]
    pub(crate) fn recent_files_of(&mut self, project: ProjectId) -> Vec<PathBuf> {
        self.recent_files_list(project).clone()
    }

    /// Write this project's list back.
    ///
    /// The whole file is read again before it is written, because `save`
    /// serialises all of `ProjectState` and a stale copy would drop the
    /// tabs, the sessions and the panels. `remember_search` has the same
    /// shape for the same reason.
    fn store_recent_files(&mut self, project: ProjectId) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(list) = self.recent_files.lists.get(&project) else {
            return;
        };
        let mut file = guild_session::project_state::load(&root);
        if file.recent_files == *list {
            return;
        }
        file.recent_files = list.clone();
        if let Err(err) = guild_session::project_state::save(&root, &file) {
            tracing::warn!(project = %root.display(), %err, "the recent files were not saved");
        }
    }

    /// Show a face of the popup, put it away, or narrow it.
    ///
    /// **A second `cmd-e` turns *edited only* on**, the same list
    /// narrowed; a third closes it, because by then the gesture
    /// has nothing left to add. `cmd-shift-e` has no second face and so
    /// its second press closes; and either stroke pressed over the other
    /// one's face swaps it, which is what a person means by pressing it.
    /// A swap and *edited only* start the picker afresh: the query was
    /// typed at the other list, and the opening row applies to this one.
    pub(crate) fn toggle_recent(
        &mut self,
        face: Face,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_front_file();
        let locations = matches!(face, Face::Locations).then(|| self.locations_now(cx));
        // **Read the list before anything is lit** — `N1`: a window
        // restored onto a terminal has not had a file in front to read it.
        if let Some(project) = self.active_project {
            self.recent_files_list(project);
        }
        let Some(open) = self.picker.as_mut() else {
            self.open_picker(
                Holds::RecentFiles(Popup {
                    face,
                    locations: locations.unwrap_or_default(),
                    edited: false,
                    pending_close: None,
                }),
                self.active_project,
                Scroll::children(),
                window,
                cx,
            );
            return;
        };
        let Holds::RecentFiles(popup) = &mut open.holds else {
            // Another popup is up: this one replaces it.
            self.open_picker(
                Holds::RecentFiles(Popup {
                    face,
                    locations: locations.unwrap_or_default(),
                    edited: false,
                    pending_close: None,
                }),
                self.active_project,
                Scroll::children(),
                window,
                cx,
            );
            return;
        };
        if popup.face != face {
            popup.face = face;
            popup.edited = false;
            popup.locations = locations.unwrap_or_default();
        } else if face == Face::Files && !popup.edited {
            popup.edited = true;
        } else {
            self.close_picker(Focus::Back, window, cx);
            return;
        }
        open.picker = Picker::default();
        self.settle_picker();
        cx.notify();
    }

    /// The popup's own state, while it is the popup that is up.
    fn recent_files_popup(&self) -> Option<&Popup> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::RecentFiles(popup)) => Some(popup),
            _ => None,
        }
    }

    /// The list this popup is narrowing, before the query.
    ///
    /// *Edited only* is the same list filtered by the edit deque, not a
    /// list of its own: two orders would disagree about what "lately"
    /// means, and the edit deque is where Last Edit Location reads it
    /// from too.
    fn recent_files_all(&self) -> Vec<PathBuf> {
        let Some(project) = self.active_project else {
            return Vec::new();
        };
        // Read by `toggle_recent` before the popup opened, so an absent
        // list is a project nothing has been opened in.
        let Some(list) = self.recent_files.lists.get(&project) else {
            return Vec::new();
        };
        if !self.recent_files_popup().is_some_and(|it| it.edited) {
            return list.clone();
        }
        let Some(edited) = self.history.peek(project).map(|trail| trail.edited()) else {
            return Vec::new();
        };
        list.iter()
            .filter(|path| edited.contains(path))
            .cloned()
            .collect()
    }

    /// The file rows the popup is showing, if it is showing files.
    pub(crate) fn recent_files_rows(&self) -> Vec<(usize, PathBuf)> {
        let Some(popup) = self.recent_files_popup() else {
            return Vec::new();
        };
        if popup.face != Face::Files {
            return Vec::new();
        }
        let all = self.recent_files_all();
        all.into_iter().enumerate().collect()
    }

    /// The location rows the popup is showing, if it is showing places.
    ///
    /// Narrowed by the same query, matched against the path **and the
    /// code** — a person looking for where they saw a `TODO` is typing
    /// the line, not the file. **By substring, not by scattered letters**:
    /// five lines of code hold most of the alphabet, and a fuzzy match
    /// over them would leave every row standing.
    pub(crate) fn recent_location_rows(&self) -> Vec<Location> {
        let Some(popup) = self.recent_files_popup() else {
            return Vec::new();
        };
        if popup.face != Face::Locations {
            return Vec::new();
        }
        let needle = self.picker_query().trim().to_lowercase();
        popup
            .locations
            .iter()
            .filter(|row| needle.is_empty() || row.matches(&needle))
            .cloned()
            .collect()
    }

    /// How many rows are on screen, whichever face is up.
    fn recent_row_count(&self) -> usize {
        match self.recent_files_popup().map(|popup| popup.face) {
            Some(Face::Locations) => self.recent_location_rows().len(),
            Some(Face::Files) => self.recent_files_rows().len(),
            None => 0,
        }
    }

    /// Recent Files' rows, for lighting: every one a row.
    pub(crate) fn recent_files_lit(&self) -> Vec<Lit> {
        vec![Lit::Row; self.recent_row_count()]
    }

    /// The row the popup opens on — the Files face's rule, applied once by the
    /// picker.
    ///
    /// **The opening row is the Files face's rule only.** A location list
    /// opens on its first row: the place a person is standing in is *in*
    /// that list, and skipping past it would skip the code they can
    /// already see, which is not the same offer at all.
    pub(crate) fn recent_files_opening(&self) -> usize {
        match self.recent_files_popup().map(|popup| popup.face) {
            Some(Face::Files) => {
                let rows = self.recent_files_rows();
                let front = self.file_in_front().map(|(_, path)| path);
                let first_is_front = rows
                    .first()
                    .zip(front.as_ref())
                    .is_some_and(|((_, path), front)| path == front);
                opening_row(rows.len(), first_is_front)
            }
            Some(Face::Locations) | None => 0,
        }
    }

    /// Recent Files' own keys.
    ///
    /// Files consumes typing without a query; Backspace requests the ordinary
    /// guarded close. Locations keeps its query editing behavior.
    pub(crate) fn recent_files_key(
        &mut self,
        stroke: &gpui::Keystroke,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let files = self
            .recent_files_popup()
            .is_some_and(|popup| popup.face == Face::Files);
        if closes_the_lit_file(stroke, files, self.picker_query()) {
            self.close_recent_file(window, cx);
            return true;
        }
        // Files is a modal selector, not a text field. Ordinary typing is
        // consumed without a hidden query or delivery to the covered editor.
        files
            && (matches!(crate::picker::key(stroke), Key::Edit(_))
                || (matches!(crate::picker::key(stroke), Key::Other)
                    && !stroke.modifiers.platform
                    && !stroke.modifiers.control
                    && !stroke.modifiers.alt))
    }

    pub(crate) fn recent_files_set_edited(&mut self, edited: bool, cx: &mut Context<Self>) {
        if let Some(open) = self.picker.as_mut()
            && let Holds::RecentFiles(popup) = &mut open.holds
            && popup.face == Face::Files
        {
            popup.edited = edited;
            open.picker = Picker::default();
            self.settle_picker();
            cx.notify();
        }
    }

    pub(crate) fn recent_files_poll(&mut self) -> bool {
        let Some(path) = self
            .recent_files_popup()
            .and_then(|popup| popup.pending_close.clone())
        else {
            return false;
        };
        let Some(project) = self.picker.as_ref().and_then(|open| open.project) else {
            return false;
        };
        if self.tabs.iter().filter(|tab| tab.project == project).flat_map(Tab::bodies).any(|body| matches!(body, Body::File { path: open, origin: crate::Origin::Project, .. } if *open == path)) {
            // The close flow has completed without removing the file (Cancel,
            // failed save or refusal). Never retain intent for a later close.
            if !self.asking
                && let Some(open) = self.picker.as_mut()
                && let Holds::RecentFiles(popup) = &mut open.holds
            {
                popup.pending_close = None;
            }
            return false;
        }
        forget(self.recent_files_list(project), &path);
        self.store_recent_files(project);
        if let Some(open) = self.picker.as_mut()
            && let Holds::RecentFiles(popup) = &mut open.holds
        {
            popup.pending_close = None;
        }
        true
    }

    /// Narrowing is the whole of what an edit does here.
    pub(crate) fn recent_files_edited(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {}

    /// Open the row taken.
    ///
    /// A file goes to wherever its tab was, so the list is a way back to
    /// work rather than to the top of a file; a location carries a line
    /// and goes through `go_to_place`, which records the departure — a
    /// jump `cmd-[` could not undo would be the one movement in this
    /// window with no way back.
    ///
    /// **The popup closes with `Focus::Kept`, before the deed**: opening
    /// the tab focuses it, and the focus the popup was opened over is not
    /// where the keyboard belongs afterwards.
    pub(crate) fn recent_files_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(face) = self.recent_files_popup().map(|popup| popup.face) else {
            return;
        };
        match face {
            Face::Files => {
                let Some((_, path)) = self.recent_files_rows().get(row).cloned() else {
                    self.close_picker(Focus::Back, window, cx);
                    return;
                };
                self.close_picker(Focus::Kept, window, cx);
                self.note_place(cx);
                self.open_file(&path, window, cx);
            }
            Face::Locations => {
                let Some(place) = self
                    .recent_location_rows()
                    .get(row)
                    .map(|location| location.place.clone())
                else {
                    self.close_picker(Focus::Back, window, cx);
                    return;
                };
                let Some(project) = self.active_project else {
                    self.close_picker(Focus::Back, window, cx);
                    return;
                };
                self.close_picker(Focus::Kept, window, cx);
                self.go_to_place(project, place, window, cx);
            }
        }
    }

    /// **Backspace closes the file**, as a switcher's does.
    ///
    /// The tab goes if there is one, and the row goes with it: a list
    /// that kept the row after a gesture named *close* would read as the
    /// key having done nothing.
    ///
    /// **Through the asking door**, not `close_tab`. A dirty buffer
    /// closed by a key nobody had to aim is unsaved text lost to one
    /// keystroke, and this window asks about that everywhere else it can
    /// happen. If the question is answered *cancel* the tab stays and
    /// its row remains and the pending intent ends with the close flow.
    fn close_recent_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.asking {
            return;
        }
        let Some(row) = self.picker_lit_row() else {
            return;
        };
        let Some((_, path)) = self.recent_files_rows().get(row).cloned() else {
            return;
        };
        let Some(project) = self.active_project else {
            return;
        };

        // The pane showing the file, and its tab only when that pane is
        // all of it — M12.8: the file is what the key closes.
        let open = self.tabs.iter().find_map(|tab| {
            (tab.project == project)
                .then(|| {
                    tab.panes.find(|body| {
                        matches!(
                            body,
                            Body::File {
                                path: open,
                                origin: crate::Origin::Project,
                                ..
                            } if *open == path
                        )
                    })
                })
                .flatten()
                .map(|pane| (tab.id, pane))
        });
        if let Some((id, pane)) = open {
            if let Some(open) = self.picker.as_mut()
                && let Holds::RecentFiles(popup) = &mut open.holds
            {
                popup.pending_close = Some(path);
            }
            self.close_pane_asking(id, pane, window, cx);
            self.recent_files_poll();
        } else {
            forget(self.recent_files_list(project), &path);
            self.store_recent_files(project);
        }
        // The front file is noticed again on the next tick; until then
        // the highlight must not be pointing past the list's own end.
        self.settle_picker();
        cx.notify();
    }
}

impl RecentFiles {
    pub(crate) fn relocate(&mut self, project: ProjectId, from: &Path, to: &Path) {
        if let Some(paths) = self.lists.get_mut(&project) {
            for path in paths {
                if let Some(next) = crate::file_operations::remap(path, from, to) {
                    *path = next;
                }
            }
        }
        if let Some((owner, path)) = &mut self.front
            && *owner == project
            && let Some(next) = crate::file_operations::remap(path, from, to)
        {
            *path = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<PathBuf> {
        vec![
            PathBuf::from("src/root/history.rs"),
            PathBuf::from("src/lib.rs"),
            PathBuf::from("docs/plan.md"),
        ]
    }

    /// Nothing typed hides nothing, and the order is the list's.
    #[test]
    fn an_empty_query_keeps_every_file() {
        let all = files();
        let rows = shown("  ", &all);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 0, "the newest stays first");
    }

    /// The query matches the whole path, because a name alone does not
    /// tell two `mod.rs` apart.
    #[test]
    fn a_query_matches_the_path_not_only_the_name() {
        let all = files();

        assert_eq!(shown("root", &all).len(), 1);
        assert_eq!(shown("RS", &all).len(), 2, "case does not matter");
        assert_eq!(shown("docs/", &all)[0].0, 2, "a row knows its place");
    }

    /// **The trap that is the whole feature.** Opening on row 1 when row
    /// 0 is the file in front makes `cmd-e` Enter a switch to the last
    /// file rather than a way of reopening this one.
    #[test]
    fn the_highlight_opens_below_the_file_in_front() {
        assert_eq!(opening_row(3, true), 1);
        assert_eq!(opening_row(3, false), 0);
        assert_eq!(
            opening_row(1, true),
            0,
            "with one row there is nowhere else to be"
        );
        assert_eq!(opening_row(0, true), 0);
    }

    /// A file opened again moves to the head rather than appearing twice.
    #[test]
    fn remembering_moves_a_file_to_the_head() {
        let mut list = files();
        remember(&mut list, Path::new("docs/plan.md"));

        assert_eq!(list[0], PathBuf::from("docs/plan.md"));
        assert_eq!(list.len(), 3, "it moved, it was not added");
    }

    /// The list is capped at fifty, oldest out.
    #[test]
    fn the_list_holds_fifty() {
        let mut list = Vec::new();
        for index in 0..80 {
            remember(&mut list, Path::new(&format!("f{index}.rs")));
        }

        assert_eq!(list.len(), REMEMBERED);
        assert_eq!(list[0], PathBuf::from("f79.rs"), "the newest is first");
        assert_eq!(list[REMEMBERED - 1], PathBuf::from("f30.rs"));
    }

    /// **`N1`.** A window restored onto a terminal has never had a
    /// project file in front, so nothing had read the project's list and
    /// `cmd-e` answered *No file of this project has been opened yet* over
    /// fifty remembered files. Asking for the list reads it.
    #[test]
    fn a_list_nobody_has_noted_yet_is_read_when_asked() {
        let mut recent = RecentFiles::default();
        let project = ProjectId::new();
        let stored = vec![PathBuf::from("src/lib.rs"), PathBuf::from("docs/plan.md")];

        let list = recent.lists_for(project, || stored.clone());
        assert_eq!(*list, stored, "the list on disk is what is answered");

        let list = recent.lists_for(project, || panic!("read once, not per ask"));
        assert_eq!(*list, stored, "and it is held afterwards");
    }

    /// Backspace on a row takes it out of the list.
    #[test]
    fn forgetting_takes_a_file_out() {
        let mut list = files();
        forget(&mut list, Path::new("src/lib.rs"));

        assert_eq!(list.len(), 2);
        assert!(!list.contains(&PathBuf::from("src/lib.rs")));
    }
}

#[cfg(test)]
#[path = "recent_files/tests.rs"]
mod window_tests;
