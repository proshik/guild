//! The Recent Projects popup: what it lists, and what taking a row does.
//!
//! The list itself is the workspace's, and it is already in the File menu.
//! Since M8.9 the *choosing* — the query, the highlight, the arrows, the
//! drag — is the picker's (`crate::picker`, `root::picker`), and this is
//! the first popup drawn with it: a `Holds::RecentProjects` that answers
//! the picker's seven questions. What stays here is what only this popup
//! knows: which rows a query leaves, and that a row opens a project.

use std::path::{Path, PathBuf};

/// The rows the popup shows, each carrying its place in the full list.
///
/// The place travels with the row because opening is `open_recent(index)`,
/// and that index counts the list on disk rather than the one on screen.
/// Recomputing it from the path afterwards would be the same lookup done
/// twice, with two chances to disagree. Ranked best first once something
/// is typed, and in the list's own order — newest first — while nothing is.
pub fn shown<'a>(query: &str, recent: &'a [PathBuf]) -> Vec<(usize, &'a Path)> {
    let haystacks: Vec<String> = recent.iter().map(|path| haystack(path)).collect();

    crate::fuzzy::rank(query, &haystacks, String::as_str, crate::fuzzy::Shape::Path)
        .into_iter()
        .map(|(index, _)| (index, recent[index].as_path()))
        .collect()
}

/// What a query is matched against.
///
/// The whole path, not only the last component: two checkouts of the same
/// repository differ by the directory above them, and a person who has
/// both is exactly the person typing to tell them apart. Matched by its
/// scattered letters since M8.9, case ignored — `crate::fuzzy`.
fn haystack(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

/// How far down an unmoved popup starts.
///
/// Below the title bar, because the eye is already up there reading the
/// project's name and this popup answers a question about that name.
pub(crate) fn picker_top() -> f32 {
    64.0
}

/// A path as a person writes it: `~` for the home directory.
///
/// Not cosmetic. Every project a person has is usually under one home
/// directory, so the first twenty characters of every row are the same
/// twenty characters — the part that tells two rows apart is pushed off
/// the end of a popup that has to fit on screen.
pub fn home_relative(path: &Path, home: Option<&Path>) -> String {
    let full = path.to_string_lossy().into_owned();
    let Some(home) = home else {
        return full;
    };

    match path.strip_prefix(home) {
        // The home directory itself, which has no tail to append.
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy()),
        Err(_) => full,
    }
}

use gpui::{Context, Window};

use crate::Root;
use crate::picker::{Key, Lit, Take};
use crate::root::picker::{Focus, Holds, Scroll};

impl Root {
    /// Put every field of ours away.
    ///
    /// **Only one popup can be up because there is one field for it** —
    /// `Root.picker`, M8.9's decision 1 — so the lists need no line each
    /// here any more, and a list added later cannot be left out of it
    /// (`R7`). What is left are the fields that are not lists: the menu,
    /// the `+` field, a tab's name, and Go to Line. A dock head's ⋮, the
    /// counter's agents list, the title bar's project popup and the Log's
    /// filter dropdowns used to be menus of their own, the last reading
    /// Backspace in the root's capture phase ahead of a popup's card;
    /// since M12.7 all four are triggers of the right button's engine,
    /// and `self.context_menu = None` below puts any of them away the
    /// same way every other menu of the engine's is.
    ///
    /// The focus is not given back here: whatever is opening takes it
    /// next, and a window focused on its own root in between is a frame
    /// nobody sees. A field closed *without* another opening goes
    /// through its own `close_…`, which does refocus.
    pub(crate) fn put_fields_away(&mut self) {
        if let Some(open) = self.clone_project.as_mut() {
            open.visible = false;
        }
        self.context_menu = None;
        self.tab_launcher.visible = false;
        self.open_in_picker = None;
        self.open_in_new_custom = None;
        self.new_worktree = None;
        self.workspace_field = None;
        self.workspace_icon_picker = None;
        self.workspace_icon_lit = None;
        self.terminal_list = None;
        self.showing_servers = false;
        self.showing_usage = false;
        self.renaming_tab = None;
        self.picker = None;
        self.shut_goto_line();
        self.update.dismiss_greeting();
    }

    /// Show the Recent Projects popup, or put it away.
    ///
    /// The popup always opens on the whole list: a query left over from
    /// last time filters a list the person did not filter, and looks like
    /// projects going missing.
    pub(crate) fn toggle_recent_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(
            self.picker.as_ref().map(|open| &open.holds),
            Some(Holds::RecentProjects)
        ) {
            self.close_picker(Focus::Back, window, cx);
            return;
        }
        // **The popup takes the focus, or Enter never reaches it** — a
        // binding is dispatched before both key phases, and the editor
        // binds `enter`. `open_picker` takes it, on the card's own handle.
        self.open_picker(Holds::RecentProjects, None, Scroll::children(), window, cx);
    }

    /// The rows the popup is showing, best first once something is typed.
    ///
    /// Owned rather than borrowed: the caller is a render method holding
    /// `&self`, and the paths have to outlive the closures it builds.
    pub(crate) fn recent_rows(&self) -> Vec<(usize, PathBuf)> {
        if !matches!(
            self.picker.as_ref().map(|open| &open.holds),
            Some(Holds::RecentProjects)
        ) {
            return Vec::new();
        }
        let all: Vec<PathBuf> = self.desk.recent().map(Path::to_path_buf).collect();

        shown(self.picker_query(), &all)
            .into_iter()
            .map(|(index, path)| (index, path.to_path_buf()))
            .collect()
    }

    /// Recent Projects' rows, for lighting: every one a row.
    pub(crate) fn recent_projects_lit(&self) -> Vec<Lit> {
        vec![Lit::Row; self.recent_rows().len()]
    }

    /// Recent Projects reads no key of its own.
    pub(crate) fn recent_projects_key(
        &mut self,
        _key: &Key,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> bool {
        false
    }

    /// Narrowing is the whole of what an edit does here.
    pub(crate) fn recent_projects_edited(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
    }

    /// Open the row taken.
    ///
    /// The row carries its place in the list on disk, which is what
    /// `open_recent` counts — the popup's own order is not that list once
    /// a query has narrowed it.
    ///
    /// **Open first, then close.** The keyboard then goes to what the
    /// window shows, and until the project has been opened that is the
    /// project being left — so the focus is the new front body's, not the
    /// one the popup was opened over.
    pub(crate) fn recent_projects_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.recent_rows().get(row).map(|(index, _)| *index) else {
            self.close_picker(Focus::Back, window, cx);
            return;
        };
        self.open_recent(index, cx);
        self.close_picker(Focus::Kept, window, cx);
        self.refocus(window, cx);
    }

    /// What a pointer move means, wherever it was heard.
    ///
    /// **A drag with no button held is over.** The release can go missing:
    /// let go outside the window and no hitbox is hovered, so neither the
    /// window nor the popup hears it, and the drag lives for ever — the
    /// popup then follows the pointer with nothing pressed, and goes on
    /// doing it after it is closed and opened again. Every move carries
    /// which button is down, so the state can be corrected rather than
    /// trusted.
    pub(crate) fn follow_pointer(
        &mut self,
        event: &gpui::MouseMoveEvent,
        _window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        if event.pressed_button != Some(gpui::MouseButton::Left) {
            self.stop_dragging(cx);
            self.drop_picker(cx);
            return;
        }

        if self.dragging.is_some() {
            self.drag_panel(event.position, cx);
        }
        if self.column_drag.is_some() {
            self.drag_column(event.position, cx);
        }
        // The projects column's own border — M12 task 11. Its own flag:
        // `dragging` is the docks' and `column_drag` is the Git log's
        // columns.
        if self.column_edge_drag {
            self.drag_column_edge(event.position, cx);
        }
        // The border between two areas — M12.8.
        if self.area_drag.is_some() {
            self.drag_area_border(event.position, cx);
        }
        // …and between two panes of a tab.
        if self.pane_drag.is_some() {
            self.drag_pane_border(event.position, cx);
        }
        self.drag_picker(event.position, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn recent() -> Vec<PathBuf> {
        vec![
            PathBuf::from("/Users/person/work/guild"),
            PathBuf::from("/Users/person/fun/orca"),
            PathBuf::from("/Users/person/work/guild-spike"),
        ]
    }

    /// Nothing typed means nothing hidden, in the order the list is in.
    #[test]
    fn an_empty_query_keeps_every_project() {
        let all = recent();
        let rows = shown("", &all);

        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].0, 0, "the newest stays first");
        assert_eq!(rows[2].1, Path::new("/Users/person/work/guild-spike"));
    }

    /// A query narrows by the whole path, so the directory above counts.
    #[test]
    fn a_query_matches_the_path_not_only_the_name() {
        let all = recent();

        assert_eq!(shown("guild", &all).len(), 2, "two checkouts match");
        assert_eq!(shown("fun", &all).len(), 1, "the parent directory counts");
        assert_eq!(shown("spike", &all).len(), 1);
    }

    /// Nobody holds shift to search, and nobody should have to.
    #[test]
    fn matching_ignores_case_and_surrounding_space() {
        let all = recent();

        assert_eq!(shown("ORCA", &all).len(), 1);
        assert_eq!(shown("  orca ", &all).len(), 1);
    }

    /// A row carries where it sits in the list on disk, because that is
    /// what opens it.
    #[test]
    fn a_row_knows_its_place_in_the_full_list() {
        let all = recent();
        let rows = shown("orca", &all);

        assert_eq!(rows[0].0, 1, "orca is the second recent directory");
    }

    /// The rows differ in their tail; the head is the same on all of them.
    #[test]
    fn a_path_under_home_is_written_with_a_tilde() {
        let home = Path::new("/Users/person");

        assert_eq!(
            home_relative(Path::new("/Users/person/work/guild"), Some(home)),
            "~/work/guild"
        );
        assert_eq!(home_relative(home, Some(home)), "~");
    }

    /// A path outside the home directory is shown as it is, and so is
    /// every path when there is no home directory to compare against.
    #[test]
    fn a_path_elsewhere_is_left_alone() {
        assert_eq!(
            home_relative(
                Path::new("/opt/src/guild"),
                Some(Path::new("/Users/person"))
            ),
            "/opt/src/guild"
        );
        assert_eq!(
            home_relative(Path::new("/Users/person/work/guild"), None),
            "/Users/person/work/guild"
        );
    }
}
