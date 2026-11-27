//! The tree in the left panel: what the window does with it.
//!
//! The model is `crate::tree`, and it does not know about windows. This
//! is the other half — which project's tree, what is selected in it,
//! where it is scrolled to, who has the focus — and the answers to a
//! click and to the arrows.

use std::path::{Path, PathBuf};

use gpui::{Context, FocusHandle, KeyDownEvent, ScrollStrategy, UniformListScrollHandle, Window};

use crate::Root;
use crate::tree::Tree;

/// A file's own name, for a sentence about it. The whole path when it
/// has no last component, which the project's own row is.
fn name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// Make an empty file, and the directories on the way to it.
///
/// **`create_new`, not `create`.** `File::create` truncates what is
/// already there, and *New File…* typed over an existing name would then
/// empty it without a word — the shape of the `std::fs::write` trap this
/// repository already has a note about. An existing name is an error
/// here, and the person sees the disk's own words for it.
fn make_file(path: &Path) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map(|_| ())
}

/// Rename relative to pinned project directories, with kernel-enforced no-replace.
/// APFS permits case-only changes with RENAME_EXCL. Never fall back to a
/// replacing rename after a userspace inode check: the destination may race it.
fn rename_without_replacing(root: &Path, from: &Path, to: &Path) -> std::io::Result<()> {
    crate::file_operations::rename_beneath(root, from, to)
}

/// One project's tree as the window holds it.
pub(crate) struct TreeState {
    /// The directories read and expanded.
    pub tree: Tree,
    /// The project cursor for existing single-row actions. A file is a
    /// target only while highlighted; directories retain their navigation cursor.
    pub selected: Option<PathBuf>,
    pub files: crate::file_selection::Selection<(crate::Origin, PathBuf)>,
    /// Where the list is scrolled to.
    pub scroll: UniformListScrollHandle,
    /// The panel's focus: with it the arrows are the tree's.
    pub focus: FocusHandle,
    /// A name being typed, when one is.
    pub naming: Option<Naming>,
    pub speed: Option<crate::speed_search::Search<(crate::Origin, PathBuf)>>,
    /// Whether the External Libraries section is unfolded.
    pub libraries_open: bool,
    /// A tree per external root, by its index in the project's list.
    ///
    /// **Made when the root is first unfolded, and not before**, and left
    /// out of an expand-all: walking every dependency
    /// of a project is thousands of directories nobody asked to see.
    pub library_trees: std::collections::HashMap<usize, Tree>,
    /// The highlighted external cursor for single-file navigation. The
    /// multi-file set above is qualified by origin, so project writes never
    /// receive the relative path of an external namesake.
    pub library_selected: Option<(usize, PathBuf)>,
}

impl TreeState {
    fn navigation_tree(&self, origin: crate::Origin) -> Option<&Tree> {
        match origin {
            crate::Origin::Project => Some(&self.tree),
            crate::Origin::External(at) => self.library_trees.get(&at),
            // The Explorer has no rows for a file outside the project.
            crate::Origin::Outside => None,
        }
    }

    fn navigation_tree_mut(&mut self, origin: crate::Origin) -> Option<&mut Tree> {
        match origin {
            crate::Origin::Project => Some(&mut self.tree),
            crate::Origin::External(at) => self.library_trees.get_mut(&at),
            crate::Origin::Outside => None,
        }
    }

    /// Move the selection onto the row that draws it, after anything that
    /// folds.
    ///
    /// The selection is a path "so that it survives the list changing
    /// shape under it" — and since M8.10 a fold can change a row's *name*:
    /// expanding `cfg` makes the row `cfg/guild`, collapsing it makes it
    /// `cfg` again. A selection left on a path no row bears lit nothing,
    /// and the next arrow started from the top. The row whose chain runs
    /// through the path is the one; for a path folded away, its nearest
    /// drawn ancestor.
    pub(crate) fn settle_selection(&mut self) {
        let mut visible: Vec<_> = self
            .tree
            .rows()
            .iter()
            .filter(|r| writable_row(r.path()))
            .map(|r| (crate::Origin::Project, r.path().to_path_buf()))
            .collect();
        if self.libraries_open {
            for (at, tree) in &self.library_trees {
                visible.extend(
                    tree.rows()
                        .iter()
                        .filter(|r| !r.is_dir())
                        .map(|r| (crate::Origin::External(*at), r.path().to_path_buf())),
                );
            }
        }
        let previous_focus = self.files.focus.clone();
        let mut surviving = visible.clone();
        surviving.extend(
            self.files
                .selected
                .iter()
                .filter(|(origin, path)| {
                    *origin == crate::Origin::Project
                        && writable_row(path)
                        && std::fs::symlink_metadata(self.tree.root().join(path)).is_ok()
                })
                .cloned(),
        );
        self.files.retain(&surviving);
        if self
            .files
            .focus
            .as_ref()
            .is_some_and(|key| !visible.contains(key))
        {
            self.files.focus = None;
        }
        if self.files.focus.is_some() {
            self.sync_file_cursor();
            return;
        }
        // A hidden file is no longer actionable, but its source and path
        // still identify the nearest visible directory for navigation.
        if let Some((origin, path)) = previous_focus {
            self.selected = None;
            self.library_selected = None;
            match origin {
                crate::Origin::Project => self.selected = Some(path),
                crate::Origin::External(at) => self.library_selected = Some((at, path)),
                crate::Origin::Outside => {}
            }
        }

        if let Some((at, selected)) = self.library_selected.clone() {
            // A root is drawn before its tree is read, and selected as such
            // by speed search and the right button — decision 54.
            if self.libraries_open
                && selected.as_os_str().is_empty()
                && !self.library_trees.contains_key(&at)
            {
                return;
            }
            self.library_selected = self
                .libraries_open
                .then(|| {
                    let tree = self.library_trees.get(&at)?;
                    selected
                        .ancestors()
                        .find_map(|path| tree.row_covering(path))
                        .map(|index| (at, tree.rows()[index].path().to_path_buf()))
                })
                .flatten();
            return;
        }
        let Some(selected) = self.selected.clone() else {
            return;
        };
        let settled = selected
            .ancestors()
            .find_map(|path| self.tree.row_covering(path))
            .map(|index| self.tree.rows()[index].path().to_path_buf());
        if settled.is_some() {
            self.selected = settled;
        }
    }
    pub(crate) fn sync_file_cursor(&mut self) {
        self.selected = None;
        self.library_selected = None;
        if let Some((origin, path)) = self.files.action_target() {
            match origin {
                crate::Origin::Project => self.selected = Some(path.clone()),
                crate::Origin::External(at) => self.library_selected = Some((*at, path.clone())),
                crate::Origin::Outside => {}
            }
        }
    }
}

/// What a name is being typed for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum For {
    /// A file to make, and to open once it is there.
    File,
    /// A directory to make.
    Directory,
    /// Another name for a row that already exists.
    Rename,
}

/// Whether a row of the tree stands for something the window may write
/// to — renamed, or moved to the Trash.
///
/// **The project's own row is the tree's empty path**, and it is drawn
/// as an ordinary directory: it folds, it takes a new file, it is
/// searched. What it must not take is either gesture that acts on the
/// directory *itself*, because that directory is the one the window has
/// open — `system::trash` would take the project away with the window
/// still showing it, and a rename would move it out from under every
/// open tab. No confirmation makes either a reasonable thing to offer.
///
/// The menu stops offering the two items (`entries_for_directory`'s
/// `root`) **and** the two gestures refuse, because an item that is
/// merely not drawn is one keymap entry away from being reachable again.
pub(crate) fn writable_row(relative: &Path) -> bool {
    !relative.as_os_str().is_empty()
}

/// The root Guild has open that one of `sources` is, or holds — GLD-057 —
/// by its path in the tree whose root is `tree_root`.
///
/// **`writable_row`'s rule, for the roots below the tree's own.** A group
/// folder's Explorer shows its open members as ordinary rows, and a
/// checkout shows a worktree opened from inside it; renaming, moving or
/// trashing either takes a project away from under the tabs and sessions
/// Guild holds in it, which is what `writable_row` refuses for the tree's
/// own root. A source holding several answers with the shallowest, and
/// the tree's own root is not a source here: `writable_row` keeps it.
pub(crate) fn open_root_among<'a>(
    tree_root: &Path,
    sources: &[PathBuf],
    open: impl IntoIterator<Item = &'a Path>,
) -> Option<PathBuf> {
    let open: Vec<&Path> = open.into_iter().collect();
    sources
        .iter()
        .filter(|source| writable_row(source))
        .find_map(|source| {
            let at = tree_root.join(source);
            open.iter()
                .filter(|root| root.starts_with(&at))
                .filter_map(|root| root.strip_prefix(tree_root).ok())
                .min_by_key(|relative| relative.components().count())
                .map(Path::to_path_buf)
        })
}

/// [`open_root_among`] with every path resolved on the disk — GLD-057's
/// review: a tree opened through a symlinked ancestor spells its rows
/// `/alias/acme/member` while the member was opened as `/real/acme/member`,
/// and the spelling alone misses that one holds the other.
///
/// **A source resolves only up to its parent.** Renaming or trashing a
/// symlink moves the link, not what it points at, so the link's own name
/// is kept; an open root is resolved whole, since moving what its path
/// leads to takes it away. A path that cannot be resolved is left out —
/// the spelling check has already had its say. It reads the disk: for the
/// handlers, not for drawing.
pub(crate) fn open_root_resolved<'a>(
    tree_root: &Path,
    sources: &[PathBuf],
    open: impl IntoIterator<Item = &'a Path>,
) -> Option<PathBuf> {
    let open: Vec<PathBuf> = open
        .into_iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect();
    let resolved_tree = std::fs::canonicalize(tree_root).ok();
    sources
        .iter()
        .filter(|source| writable_row(source))
        .find_map(|source| {
            let at = tree_root.join(source);
            let at = std::fs::canonicalize(at.parent()?)
                .ok()?
                .join(at.file_name()?);
            open.iter()
                .filter(|root| root.starts_with(&at))
                .min_by_key(|root| root.components().count())
                .map(|root| {
                    resolved_tree
                        .as_deref()
                        .and_then(|tree| root.strip_prefix(tree).ok())
                        .map_or_else(|| root.clone(), Path::to_path_buf)
                })
        })
}

/// A name being typed into the tree.
///
/// **A field of the tree's own, not a sheet.** `window.prompt` has no
/// text in it — it is buttons — and a card of ours over the window would
/// be a third occluding surface with its own focus, dismissal and drag
/// rules, all of which this crate has learned the hard way twice. The
/// field is drawn in the panel, the panel keeps the focus it already has,
/// and the keys reach it through `tree_key` because that is where every
/// key of a focused tree already goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Naming {
    /// What will happen when Enter is pressed.
    pub what: For,
    /// The directory the name is under, relative to the project.
    pub dir: PathBuf,
    /// The row being renamed, when one is.
    pub from: Option<PathBuf>,
    /// What has been typed, obeying `desktop::edit` like every field of
    /// ours.
    pub typed: String,
    pub initial_selection: std::ops::Range<usize>,
}

impl Naming {
    /// The directory a row makes things in: its own when it is a
    /// directory, the one holding it when it is a file — the only rule
    /// that is not surprising.
    fn dir_of(path: &Path, is_dir: bool) -> PathBuf {
        if is_dir {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(Path::new("")).to_path_buf()
        }
    }

    /// A file to make under the row the pointer was on.
    pub(crate) fn file(path: &Path, is_dir: bool) -> Self {
        Self {
            what: For::File,
            dir: Self::dir_of(path, is_dir),
            from: None,
            typed: String::new(),
            initial_selection: 0..0,
        }
    }

    /// A directory to make there.
    pub(crate) fn directory(path: &Path, is_dir: bool) -> Self {
        Self {
            what: For::Directory,
            dir: Self::dir_of(path, is_dir),
            from: None,
            typed: String::new(),
            initial_selection: 0..0,
        }
    }

    /// Another name for the row itself — which starts as the name it
    /// has, because a rename is usually an edit of one.
    ///
    /// **`None` for the project's own row**, which is [`writable_row`]'s
    /// subject: that row's path is the empty one, its parent is the
    /// empty one too, and the name it starts from is nothing at all — so
    /// the field would open with an empty box aimed at the directory the
    /// window has open. The refusal is here rather than at the three
    /// call sites because this is where they meet.
    pub(crate) fn rename(path: &Path) -> Option<Self> {
        if !writable_row(path) {
            return None;
        }
        Some(Self {
            what: For::Rename,
            dir: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            from: Some(path.to_path_buf()),
            initial_selection: 0..0,
            typed: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        })
    }

    /// The path this name asks for, or `None` while it asks for nothing
    /// that may be written.
    pub(crate) fn asks_for(&self) -> Option<PathBuf> {
        if self.what == For::Rename
            && (self.typed.contains('/') || self.typed == "." || self.typed == "..")
        {
            return None;
        }
        crate::tree::under(&self.dir, &self.typed)
    }
}

/// What a key asks the tree to do, decided while the rows are borrowed
/// and carried out after they are let go.
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Select(usize),
    Expand(PathBuf),
    Collapse(PathBuf),
    Toggle(PathBuf),
    Parent(PathBuf),
    /// A file to open — with the keyboard going to it, or staying in the
    /// tree.
    Open {
        path: PathBuf,
        keyboard_to_file: bool,
    },
    Blur,
    Nothing,
    /// Not a key the tree reads: it goes on to whoever reads it next.
    NotTheTrees,
}

/// What a key does to the tree, given the lit row as (its path, the path
/// a fold must start at, whether it is a directory, whether it is
/// expanded), its place, and the last place there is.
///
/// **The two paths differ on a chain** — M8.10. A row saying
/// `com/example/inaction` is opened and closed at `com`; folding it at
/// `inaction` would leave the two above it open and the row would come
/// back as `com/example` with a child, which is not what closing it
/// looked like.
///
/// **Enter and Right on a file open it** — the M8.1 plan promised *enter
/// opens* and nothing drove it. Enter is the gesture that goes to the
/// file, so the keyboard goes there too; Right is a movement key, and a
/// person walking the tree with the arrows expects the next arrow to
/// stay in the tree.
fn step_for(
    key: &str,
    row: Option<(&Path, &Path, bool, bool)>,
    current: Option<usize>,
    last: usize,
) -> Step {
    match key {
        "home" => Step::Select(0),
        "end" => Step::Select(last),
        "down" => Step::Select(current.map_or(0, |i| (i + 1).min(last))),
        "up" => Step::Select(current.map_or(0, |i| i.saturating_sub(1))),
        "right" => match row {
            Some((_, fold, true, false)) => Step::Expand(fold.to_path_buf()),
            Some((_, _, true, true)) => Step::Select((current.unwrap_or(0) + 1).min(last)),
            Some((path, _, false, _)) => Step::Open {
                path: path.to_path_buf(),
                keyboard_to_file: false,
            },
            None => Step::Nothing,
        },
        "left" => match row {
            Some((_, fold, true, true)) => Step::Collapse(fold.to_path_buf()),
            // The empty parent is the root's own row now.
            Some((_, fold, _, _)) => match fold.parent() {
                Some(parent) => Step::Parent(parent.to_path_buf()),
                None => Step::Nothing,
            },
            None => Step::Nothing,
        },
        "enter" => match row {
            Some((_, fold, true, _)) => Step::Toggle(fold.to_path_buf()),
            Some((path, _, false, _)) => Step::Open {
                path: path.to_path_buf(),
                keyboard_to_file: true,
            },
            None => Step::Nothing,
        },
        "escape" => Step::Blur,
        _ => Step::NotTheTrees,
    }
}

impl Root {
    /// The active project's tree, read the first time the panel shows it.
    pub(crate) fn ensure_active_tree(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let seat = self.seat_for_project(project);
        if self.trees.contains_key(&seat) {
            return;
        }
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let stored = guild_session::project_state::load(&path);
        let first = self
            .desk
            .workspaces()
            .first()
            .map(|workspace| workspace.id)
            .unwrap_or(self.active_workspace);
        let expanded = stored.seat(self.active_workspace, first).expanded;
        let tree = Tree::with_expanded(&path, &expanded);
        // The roots this project has already landed in, so the section
        // under the tree has something to list on the first frame. Neither
        // for a project group's folder — GLD-055 decision 12: they serve
        // language tooling, which is off for it.
        let tooling = self.integrations.external() && !self.desk.is_project_group_root(project);
        if tooling {
            self.ensure_libraries(project);
        }
        // And everything its build systems say it depends on — asked on
        // a thread, and listed as it arrives. M8.14.
        if tooling {
            self.ask_dependencies(project);
        }
        self.trees.insert(
            seat,
            TreeState {
                tree,
                selected: None,
                files: Default::default(),
                scroll: UniformListScrollHandle::new(),
                focus: cx.focus_handle(),
                naming: None,
                speed: None,
                libraries_open: false,
                library_trees: std::collections::HashMap::new(),
                library_selected: None,
            },
        );
    }

    /// The active project's tree, if it has been read.
    pub(crate) fn tree(&self) -> Option<&TreeState> {
        self.active_seat().and_then(|seat| self.trees.get(&seat))
    }

    pub(crate) fn tree_mut(&mut self) -> Option<&mut TreeState> {
        self.active_seat()
            .and_then(|seat| self.trees.get_mut(&seat))
    }

    /// Whether the tree holds the focus — the arrows are its, and so is a
    /// resize stroke.
    pub(crate) fn tree_has_focus(&self, window: &Window) -> bool {
        self.tree().is_some_and(|t| t.focus.is_focused(window))
    }

    /// A click on a row: it is selected, a directory toggles, and the
    /// panel takes the focus so that the arrows continue from here.
    pub(crate) fn tree_click(
        &mut self,
        path: &Path,
        fold: &Path,
        is_dir: bool,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !is_dir || ((modifiers.platform || modifiers.shift) && writable_row(path)) {
            self.tree_file_click(crate::Origin::Project, path, modifiers, window, cx);
            return;
        }
        let Some(state) = self.tree_mut() else {
            return;
        };
        state.files = Default::default();
        if writable_row(path) {
            let key = (crate::Origin::Project, path.to_path_buf());
            state
                .files
                .click(&key, std::slice::from_ref(&key), false, false);
        }
        state.selected = Some(path.to_path_buf());
        state.library_selected = None;
        if is_dir {
            // A click on a folder ends speed search before the fold:
            // folding changes the candidates, and a query left up would
            // reconcile to a match elsewhere and select it — decision 54.
            state.speed = None;
            // A chain folds from its top — M8.10, the reason a row
            // carries two paths.
            state.tree.toggle(fold);
            state.settle_selection();
        }
        let focus = state.focus.clone();
        window.focus(&focus, cx);
        if is_dir {
            self.store_expanded();
        } else {
            self.open_file(path, window, cx);
        }
        cx.notify();
    }

    /// A key pressed with the tree focused. Answers whether the tree took
    /// it. The arrows move the selection and clamp at the ends — a tree
    /// has a top; `right` opens a directory or steps into it and opens a
    /// file with the keyboard left in the tree, `left` closes a directory
    /// or steps to its parent, `enter` toggles a directory and opens a
    /// file with the keyboard going to it, `escape` gives the focus back
    /// to the tab. What each key does is [`step_for`].
    pub(crate) fn tree_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.keystroke.key == "escape" && self.file_operations.drag.is_some() {
            self.clear_tree_drag(cx);
            cx.stop_active_drag(window);
            return true;
        }

        // **A field takes every key while it is up.** The panel is the
        // focused element either way, so this is the same handler; what
        // changes is that a letter is a letter rather than nothing, and
        // the arrows must not move a selection behind an open field.
        if self.tree().is_some_and(|state| state.naming.is_some()) {
            return self.naming_key(event, window, cx);
        }
        if crate::text_field::focused(window, cx) {
            return false;
        }
        if self.tree_files_key(event, window, cx) {
            return true;
        }
        let Some(state) = self.tree_mut() else {
            return false;
        };
        // Whatever folded since — a batch from the disk included — the
        // current row is the one that draws the selection.
        state.settle_selection();
        // The file cursor can be unselected by a Command-click. Navigation
        // still starts there; file actions continue to require selection.
        let origin = state
            .files
            .focus
            .as_ref()
            .map(|(origin, _)| *origin)
            .or_else(|| {
                state
                    .library_selected
                    .as_ref()
                    .map(|(at, _)| crate::Origin::External(*at))
            })
            .unwrap_or(crate::Origin::Project);
        let selected = state
            .files
            .focus
            .as_ref()
            .map(|(_, path)| path.as_path())
            .or_else(|| match origin {
                crate::Origin::Project => state.selected.as_deref(),
                crate::Origin::External(_) => state
                    .library_selected
                    .as_ref()
                    .map(|(_, path)| path.as_path()),
                crate::Origin::Outside => None,
            });
        let Some(tree) = state.navigation_tree(origin) else {
            // A library root selected before its tree was read: the keys
            // that open a directory read and unfold it, as a click does.
            if let crate::Origin::External(at) = origin
                && selected.is_some_and(|path| path.as_os_str().is_empty())
                && matches!(event.keystroke.key.as_str(), "right" | "enter")
            {
                self.toggle_library(at, cx);
                return true;
            }
            return false;
        };
        let rows = tree.rows();
        let count = rows.len();
        let last = count.saturating_sub(1);
        let current =
            selected.and_then(|selected| rows.iter().position(|row| row.path() == selected));
        let row = current.and_then(|i| rows.get(i)).map(|row| {
            (
                row.path(),
                row.chain_root(),
                row.is_dir(),
                row.is_expanded(),
            )
        });

        let step = step_for(event.keystroke.key.as_str(), row, current, last);
        if step == Step::NotTheTrees {
            return false;
        }
        if count == 0 && !matches!(step, Step::Blur) {
            return true;
        }
        drop(rows);

        match step {
            Step::Select(index) => self.select_tree_row(origin, index),
            Step::Expand(ref path) | Step::Collapse(ref path) | Step::Toggle(ref path) => {
                let tree = state
                    .navigation_tree_mut(origin)
                    .expect("navigation tree exists");
                match &step {
                    Step::Expand(_) => tree.expand(path),
                    Step::Collapse(_) => tree.collapse(path),
                    _ => tree.toggle(path),
                };
                state.settle_selection();
                if origin == crate::Origin::Project {
                    self.store_expanded();
                }
            }
            Step::Parent(parent) => {
                // The parent may sit in the middle of a chain above.
                let index = state
                    .navigation_tree(origin)
                    .and_then(|tree| tree.row_covering(&parent));
                if let Some(index) = index {
                    self.select_tree_row(origin, index);
                }
            }
            Step::Open {
                path,
                keyboard_to_file,
            } => {
                // A deselected file cursor cannot open; an explicit row left
                // by renaming a closed file still can.
                if state.files.focus.is_some() && state.files.action_target().is_none() {
                    return true;
                }
                let focus = state.focus.clone();
                self.open_file_from(origin, &path, window, cx);
                // **Opening a tab focuses it, so the tree is focused
                // after, not before** — the trap the Changes panel met.
                if !keyboard_to_file {
                    window.focus(&focus, cx);
                }
            }
            Step::Blur => {
                self.cancel_file_operation(cx);
                // Back to the tab in front — its terminal or its prompt —
                // and to the window's own handle when the tab has nothing
                // to type into.
                let focus = self
                    .active_body()
                    .and_then(|body| body.focus_handle(cx))
                    .unwrap_or_else(|| self.focus.clone());
                window.focus(&focus, cx);
            }
            Step::Nothing | Step::NotTheTrees => {}
        }
        cx.notify();
        true
    }

    /// Open the selected file and send the keyboard to it — `f4`,
    /// *Jump to Source*. Enter's gesture on a file, reached by a
    /// stroke of the keymap's rather than by the tree's own keys, so that
    /// a person can move it. A directory has no source to jump to, and
    /// the stroke leaves it alone: folding is Enter's and the arrows'.
    pub(crate) fn jump_to_source(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.tree() else {
            return;
        };
        if let Some((origin, path)) = state.files.action_target().cloned() {
            if state
                .navigation_tree(origin)
                .is_none_or(|tree| tree.root().join(&path).is_dir())
            {
                return;
            }
            self.open_file_from(origin, &path, window, cx);
            return;
        }
        let Some(selected) = state.selected.clone() else {
            return;
        };
        let is_file = state
            .tree
            .rows()
            .iter()
            .any(|row| row.path() == selected && !row.is_dir());
        if is_file {
            self.open_file(&selected, window, cx);
        }
    }

    /// Select the row standing for `path`, as an arrow landing on it does —
    /// the one selection, nothing opened. Speed search's match and the
    /// right button's row both arrive here — decision 54.
    pub(crate) fn select_tree_path(&mut self, origin: crate::Origin, path: &Path) {
        let Some(state) = self.tree_mut() else {
            return;
        };
        let index = state
            .navigation_tree(origin)
            .and_then(|tree| tree.rows().iter().position(|row| row.path() == path));
        match (index, origin) {
            (Some(index), _) => self.select_tree_row(origin, index),
            // A library's root is a row before its tree has been read.
            (None, crate::Origin::External(at)) if path.as_os_str().is_empty() => {
                state.files = Default::default();
                state.selected = None;
                state.library_selected = Some((at, PathBuf::new()));
            }
            _ => {}
        }
    }

    /// The right button on a row: it selects the row before its menu opens,
    /// unless the row is already one of the selected — then the group stays
    /// and the row becomes its cursor. The tree takes the keys, as a left
    /// click gives them. Decision 54.
    pub(crate) fn tree_context(
        &mut self,
        origin: crate::Origin,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.tree_mut() else {
            return;
        };
        let key = (origin, path.to_path_buf());
        if state.files.contains(&key) {
            state.files.focus = Some(key);
            state.sync_file_cursor();
        } else {
            self.select_tree_path(origin, path);
        }
        if let Some(state) = self.tree() {
            window.focus(&state.focus, cx);
        }
        cx.notify();
    }

    /// Select a row by its place and bring it into view.
    ///
    /// `scroll_to_item` is applied during prepaint, so it can be asked
    /// here, before the rows of the new list exist — the popup's list
    /// learned that in M5.8.
    fn select_tree_row(&mut self, origin: crate::Origin, index: usize) {
        let Some(state) = self.tree_mut() else {
            return;
        };
        let row = state.navigation_tree(origin).and_then(|tree| {
            tree.rows()
                .get(index)
                .map(|row| (row.path().to_path_buf(), row.is_dir()))
        });
        let Some((path, is_dir)) = row else {
            return;
        };
        state.files = Default::default();
        if !is_dir || (origin == crate::Origin::Project && writable_row(&path)) {
            let key = (origin, path.clone());
            state
                .files
                .click(&key, std::slice::from_ref(&key), false, false);
        }
        state.selected = None;
        state.library_selected = None;
        match origin {
            crate::Origin::Project => state.selected = Some(path.clone()),
            crate::Origin::External(at) => state.library_selected = Some((at, path.clone())),
            crate::Origin::Outside => {}
        }
        let scroll = state.scroll.clone();
        let drawn = match origin {
            crate::Origin::Project => Some(index),
            crate::Origin::External(at) => self.library_row_index(at, &path),
            crate::Origin::Outside => None,
        };
        if let Some(index) = drawn {
            scroll.scroll_to_item(index, ScrollStrategy::Top);
        }
    }

    /// Write which directories are expanded into the project's state
    /// file, on every change, for the reason the panels are: a window
    /// that saves only on a clean shutdown loses what it knew at the
    /// first crash.
    /// *Reveal Active File* — the crosshair: open the way to the file
    /// in front, select its row, bring it into view and put the focus on
    /// the tree; open the panel first if it is closed. The tree follows
    /// the tab in front on its own, so this is for after a person has
    /// scrolled away or folded the way shut.
    pub(crate) fn reveal_active_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(crate::Body::File { path, origin, .. }) = self.active_body() else {
            return;
        };
        // A file outside the project has no row anywhere in the tree, and
        // its path is relative to `/`: revealed in the project's tree it
        // would name a different file.
        if *origin == crate::Origin::Outside {
            return;
        }
        let (path, origin) = (path.clone(), *origin);
        if !self.panels().left.open {
            self.toggle_panel(crate::view::panels::Side::Left, cx);
        }
        self.ensure_active_tree(cx);
        // A dependency's file is revealed in External Libraries — since
        // 2026-09-28; M8.8.3 had no rows there to reveal it in.
        if let crate::Origin::External(at) = origin {
            if self.reveal_in_library(at, &path)
                && let Some(state) = self.tree()
            {
                window.focus(&state.focus, cx);
            }
            cx.notify();
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        let seat = self.seat_for_project(project);
        let Some(state) = self.trees.get_mut(&seat) else {
            return;
        };
        if let Some(row) = state.tree.reveal(&path) {
            let key = (crate::Origin::Project, path.clone());
            state
                .files
                .click(&key, std::slice::from_ref(&key), false, false);
            state.selected = Some(path);
            state.library_selected = None;
            state
                .scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
            window.focus(&state.focus, cx);
        }
        self.store_expanded();
        cx.notify();
    }

    /// Open the way to a file under a library root and light its row:
    /// the section and the root unfolded, the root read if it had not
    /// been, the row brought into view. `false` where the file is not
    /// there to reveal.
    ///
    /// The project's action target is cleared. Subsequent navigation stays
    /// in this source tree, even when the project has a namesake path.
    pub(crate) fn reveal_in_library(&mut self, at: usize, path: &Path) -> bool {
        let Some(project) = self.active_project else {
            return false;
        };
        let Some(root) = self.libraries.of(project).get(at).cloned() else {
            return false;
        };
        let source = self.library_source(project, &root);
        let seat = self.seat_for_project(project);
        let Some(state) = self.trees.get_mut(&seat) else {
            return false;
        };
        state.libraries_open = true;
        let tree = state
            .library_trees
            .entry(at)
            .or_insert_with(|| source.open_tree().expect("directory source"));
        if tree.reveal(path).is_none() {
            return false;
        }
        let key = (crate::Origin::External(at), path.to_path_buf());
        if !state.files.contains(&key) {
            state
                .files
                .click(&key, std::slice::from_ref(&key), false, false);
        }
        state.selected = None;
        state.library_selected = Some((at, path.to_path_buf()));
        let Some(row) = self.library_row_index(at, path) else {
            return true;
        };
        if let Some(state) = self.trees.get(&seat) {
            state
                .scroll
                .scroll_to_item(row, gpui::ScrollStrategy::Center);
        }
        true
    }

    /// Put a field in the tree and wait for a name.
    ///
    /// The directory it will be written in is expanded first, so that the
    /// field is drawn where the thing will appear rather than under a
    /// folded row.
    ///
    /// **And the tree takes the focus.** The field has no editor of its
    /// own: the keys reach it through `tree_key`, which runs only while
    /// the panel is focused, and an item chosen from the right-button
    /// menu leaves the focus wherever the pointer found it. Without this
    /// the field appeared, took no letter, and the letters went to the
    /// terminal behind it — found in the window, and it is the same
    /// shape as the Log's toolbar that lit up and never saw a key.
    pub(crate) fn start_naming(
        &mut self,
        mut naming: Naming,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        if self.file_operations.busy.is_some() {
            return;
        }
        if naming.what == For::Rename
            && let (Some(tree_root), Some(from)) = (self.project_root(), naming.from.clone())
            && self.refuse_open_root(&tree_root, &[from])
        {
            return;
        }
        if let (Some(project), Some(from)) = (
            self.active_project,
            self.tree()
                .and_then(|s| s.naming.as_ref())
                .and_then(|n| n.from.clone()),
        ) {
            self.freeze_moving_editors(project, &[from], false, window, cx);
        }
        if naming.what == For::Rename
            && let (Some(project), Some(from)) = (self.active_project, naming.from.as_ref())
        {
            self.freeze_moving_editors(project, std::slice::from_ref(from), true, window, cx);
        }
        let Some(state) = self.tree_mut() else {
            return;
        };
        let end = if naming.what == For::Rename
            && naming
                .from
                .as_ref()
                .is_some_and(|from| !state.tree.root().join(from).is_dir())
        {
            naming
                .typed
                .rfind('.')
                .filter(|dot| *dot > 0)
                .unwrap_or(naming.typed.len())
        } else {
            naming.typed.len()
        };
        naming.initial_selection = 0..end;
        if naming.what != For::Rename {
            state.tree.expand(&naming.dir.clone());
        }
        state.naming = Some(naming);
        let focus = state.focus.clone();
        window.focus(&focus, cx);
        self.store_expanded();
    }

    /// `cmd-n` and its neighbours: the same field, aimed at whatever the
    /// tree has selected — the project's own row when nothing is.
    pub(crate) fn name_something(
        &mut self,
        what: For,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.tree_has_focus(window) || crate::text_field::focused(window, cx) {
            return;
        }
        self.ensure_active_tree(cx);
        if !self.panels().left.open {
            self.toggle_panel(crate::view::panels::Side::Left, cx);
        }
        let Some(state) = self.tree() else {
            return;
        };
        if what == For::Rename && state.files.selected.len() > 1 {
            return;
        }
        let selected = state.selected.clone().unwrap_or_default();
        let is_dir = state
            .tree
            .rows()
            .iter()
            .find(|row| row.path() == selected)
            .is_none_or(|row| row.is_dir());
        // **Nothing selected is nothing to rename**, and neither is the
        // project's own row — `Naming::rename` answers `None` for it now,
        // so this route and the menu's refuse in one place rather than
        // one of them remembering to.
        let naming = match what {
            For::File => Naming::file(&selected, is_dir),
            For::Directory => Naming::directory(&selected, is_dir),
            For::Rename => match Naming::rename(&selected) {
                Some(naming) => naming,
                None => return,
            },
        };
        self.start_naming(naming, window, cx);
        cx.notify();
    }

    /// A key while the field is up: the letters are `desktop::edit`'s,
    /// `enter` writes and `escape` puts the field away.
    pub(crate) fn naming_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match event.keystroke.key.as_str() {
            "escape" => {
                if let (Some(project), Some(from)) = (
                    self.active_project,
                    self.tree()
                        .and_then(|s| s.naming.as_ref())
                        .and_then(|n| n.from.clone()),
                ) {
                    self.freeze_moving_editors(project, &[from], false, window, cx);
                }
                if let Some(state) = self.tree_mut() {
                    state.naming = None;
                    window.focus(&state.focus, cx);
                }
                cx.notify();
                return true;
            }
            "enter" => {
                self.finish_naming(window, cx);
                return true;
            }
            _ => {}
        }
        false
    }

    /// Write what the field asks for: a file, a directory, or a rename.
    ///
    /// A name that asks for nothing writable leaves the field where it
    /// is — there is nothing to say and nothing to close.
    fn finish_naming(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(naming) = self.tree().and_then(|state| state.naming.clone()) else {
            return;
        };
        let Some(relative) = naming.asks_for() else {
            return;
        };
        let Some(root) = self.project_root() else {
            return;
        };
        // A root opened while the field was up is refused at the write,
        // and the field goes away as Escape would take it.
        if naming.what == For::Rename
            && let Some(from) = naming.from.clone()
            && self.refuse_open_root(&root, std::slice::from_ref(&from))
        {
            if let Some(project) = self.active_project {
                self.freeze_moving_editors(project, &[from], false, window, cx);
            }
            if let Some(state) = self.tree_mut() {
                state.naming = None;
                window.focus(&state.focus, cx);
            }
            cx.notify();
            return;
        }
        let target = root.join(&relative);
        let ui = self.ui();

        let made = match naming.what {
            For::Rename => {
                let Some(from) = naming.from.as_ref() else {
                    return;
                };
                rename_without_replacing(&root, from, &relative).map_err(|err| {
                    // A taken name keeps the field up with the name as
                    // typed, so the person changes a character rather
                    // than typing it all again.
                    if err.kind() == std::io::ErrorKind::AlreadyExists {
                        ui.notice_name_taken(&naming.typed)
                    } else {
                        ui.notice_not_renamed(&name_of(from), &err.to_string())
                    }
                })
            }
            For::Directory => std::fs::create_dir_all(&target).map_err(|err| {
                ui.notice_not_created(&relative.display().to_string(), &err.to_string())
            }),
            For::File => make_file(&target).map_err(|err| {
                ui.notice_not_created(&relative.display().to_string(), &err.to_string())
            }),
        };
        if let Err(said) = made {
            tracing::warn!(path = %target.display(), %said, "the tree could not write that");
            self.fail(said);
            cx.notify();
            return;
        }

        if let Some(state) = self.tree_mut() {
            state.naming = None;
            // The watcher brings the row in within a second; re-reading
            // here is what makes it appear in the frame that made it.
            state.tree.refresh(&naming.dir);
            if let Some(parent) = relative.parent() {
                state.tree.refresh(parent);
            }
            state.files = Default::default();
            state.selected = Some(relative.clone());
            state.library_selected = None;
            // Hand the keys back before the focused name input disappears.
            // Opening a new/renamed editor below may then take focus normally.
            window.focus(&state.focus, cx);
        }
        match naming.what {
            // **A new file opens.** It is what a person asked for by
            // naming it, and an empty row in a tree is not.
            For::File => self.open_file(&relative, window, cx),
            // A renamed file's tab follows it: the tab is a path, and
            // the path moved.
            For::Rename => self.follow_rename(naming.from.as_deref(), &relative, window, cx),
            For::Directory => {}
        }
        cx.notify();
    }

    fn follow_rename(
        &mut self,
        from: Option<&Path>,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let (Some(from), Some(project)) = (from, self.active_project) {
            self.relocate_project_paths(project, from, to, window, cx);
        }
    }

    /// *Delete*: the row goes to the Trash, and it asks first.
    ///
    /// **It asks even though the Trash is a way back.** The gesture came
    /// from a row under a pointer, and the row a person meant is not
    /// always the row they hit.
    pub(crate) fn delete_asking(
        &mut self,
        relative: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !writable_row(&relative) || self.file_operations.busy.is_some() {
            return;
        }
        self.select_operation_target(&relative);
        let sources = self.operation_sources();
        self.trash_paths_asking(sources, false, window, cx);
    }

    /// Shared batch confirmation and Trash service for Explorer and Commit.
    pub(crate) fn trash_paths_asking(
        &mut self,
        paths: Vec<PathBuf>,
        commit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.asking || self.file_operations.busy.is_some() {
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        let Some(project_root) = self.project_root() else {
            return;
        };
        let sources = crate::file_operations::normalize(&paths);
        if sources.iter().any(|p| !writable_row(p)) {
            return;
        }
        if self.refuse_open_root(&project_root, &sources) {
            cx.notify();
            return;
        }
        if sources.is_empty() {
            return;
        }
        let views: Vec<_> = self
            .tabs
            .iter()
            .filter(|t| t.project == project)
            .flat_map(|t| t.bodies())
            .filter_map(|body| match body {
                crate::Body::File {
                    path,
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } if sources.iter().any(|source| path.starts_with(source)) => Some(tab.clone()),
                _ => None,
            })
            .collect();
        let dirty = views.iter().any(|view| view.read(cx).is_dirty());
        for view in &views {
            view.update(cx, |file, cx| file.relocating(true, window, cx));
        }
        let ui = self.ui();
        let names = sources
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join("\n");
        let mut detail = if dirty {
            format!("{names}\n\n{}", ui.text(crate::i18n::Text::TrashUnsaved))
        } else {
            names.clone()
        };
        detail.push_str(&format!("\n\n{}", ui.trash_selection_detail(sources.len())));
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            &ui.delete_title(&names),
            Some(&detail),
            &[
                gpui::PromptButton::new(ui.text(crate::i18n::Text::Delete)),
                gpui::PromptButton::cancel(ui.text(crate::i18n::Text::Cancel)),
            ],
            cx,
        );
        let owner = cx.weak_entity();
        self.asking = true;
        window
            .spawn(cx, async move |cx| {
                let accepted = asked.await == Ok(0);
                let _ = cx.update(|window, cx| {
                    owner.update(cx, |root, cx| {
                        root.asking = false;
                        let mut errors = Vec::new();
                        let mut completed = Vec::new();
                        // A root opened while the question was up is
                        // refused at the write — GLD-057.
                        if accepted
                            && root
                                .desk
                                .project(project)
                                .is_some_and(|p| p.path == project_root)
                            && !root.refuse_open_root(&project_root, &sources)
                        {
                            for path in &sources {
                                let result =
                                    crate::file_operations::validate_source(&project_root, path)
                                        .map_err(|e| e.to_string())
                                        .and_then(|()| {
                                            crate::system::trash(&[project_root.join(path)])
                                        });
                                match result {
                                    Ok(()) => completed.push(path.clone()),
                                    Err(error) => {
                                        errors.push(format!("{}: {error}", path.display()))
                                    }
                                }
                            }
                        }
                        for view in &views {
                            view.update(cx, |file, cx| {
                                file.relocating(false, window, cx);
                                if completed.iter().any(|p| file.relative().starts_with(p)) {
                                    file.disk_changed(window, cx);
                                }
                            });
                        }
                        for (seat, state) in &mut root.trees {
                            if seat.project == project {
                                for path in &completed {
                                    state.tree.refresh(path.parent().unwrap_or(Path::new("")));
                                }
                                state.settle_selection();
                            }
                        }
                        if !errors.is_empty() {
                            root.fail(ui.notice_not_trashed(&errors.join("\n")));
                        }
                        if commit && let Some(state) = root.changes.get_mut(&project) {
                            state.selection.remove_completed(&completed);
                        }
                        cx.notify();
                    })
                });
            })
            .detach();
    }

    /// Unfold the External Libraries section, or fold it.
    pub(crate) fn toggle_libraries(&mut self, cx: &mut Context<Self>) {
        if let Some(state) = self.tree_mut() {
            state.libraries_open = !state.libraries_open;
            state.settle_selection();
        }
        cx.notify();
    }

    /// Unfold one root, reading its top directory the first time — and
    /// fold it again, which keeps the tree it read.
    pub(crate) fn toggle_library(&mut self, at: usize, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(root) = self.libraries.of(project).get(at).cloned() else {
            return;
        };
        let source = self.library_source(project, &root);
        let seat = self.seat_for_project(project);
        if let Some(state) = self.trees.get_mut(&seat) {
            match state.library_trees.get_mut(&at) {
                // A root's own row is its tree's `""`, the same row the
                // project has: toggling it folds everything inside.
                Some(tree) => tree.toggle(Path::new("")),
                // **Read here and nowhere earlier.** A root exists in the
                // list from the moment `f12` landed in it; the directory
                // is walked when somebody asks to see it.
                None => {
                    state
                        .library_trees
                        .insert(at, source.open_tree().expect("directory source"));
                }
            }
            state.settle_selection();
        }
        cx.notify();
    }

    /// A click inside a root's tree: a directory folds, a file opens
    /// under that root.
    pub(crate) fn library_click(
        &mut self,
        at: usize,
        path: &Path,
        is_dir: bool,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if is_dir {
            if path.as_os_str().is_empty() {
                // The root exists as a row before its tree has been read.
                self.toggle_library(at, cx);
            } else if let Some(state) = self.tree_mut()
                && let Some(tree) = state.library_trees.get_mut(&at)
            {
                let fold = tree
                    .rows()
                    .iter()
                    .find(|row| row.path() == path)
                    .map(|row| row.chain_root().to_path_buf());
                if let Some(fold) = fold {
                    tree.toggle(&fold);
                }
            }
            if let Some(state) = self.tree_mut() {
                state.files = Default::default();
                state.selected = None;
                state.library_selected = Some((at, path.to_path_buf()));
                state.settle_selection();
                window.focus(&state.focus, cx);
            }
            cx.notify();
            return;
        }
        self.tree_file_click(crate::Origin::External(at), path, modifiers, window, cx);
    }

    /// A path inside a root, from the root of the disk — what *Reveal in
    /// Finder* and *Copy Full Path* need.
    pub(crate) fn library_path(&self, at: usize, path: &Path) -> Option<PathBuf> {
        let project = self.active_project?;
        Some(self.libraries.of(project).get(at)?.join(path))
    }

    /// `DeleteFile`: the row the tree has selected, to the Trash.
    ///
    /// **The project's own row is not deletable** — `writable_row`, and
    /// the refusal is `delete_asking`'s now rather than this route's
    /// alone: the menu reaches the same gesture and had no such check.
    pub(crate) fn delete_selected_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected) = self.tree().and_then(|state| state.selected.clone()) else {
            return;
        };
        self.delete_asking(selected, window, cx);
    }

    /// *Open In ▸ Terminal*: a shell tab in this directory.
    ///
    /// **A tab of Guild's own**, rather than the path handed to another
    /// terminal application. The session's own `cwd` is what carries it:
    /// `open_tab` would use the project's, which is the one thing this
    /// item exists not to do.
    pub(crate) fn open_terminal_at(&mut self, path: &Path, is_dir: bool, cx: &mut Context<Self>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.open_terminal_at_without_history(path, is_dir, cx);
        self.history.suspended -= 1;
    }

    fn open_terminal_at_without_history(
        &mut self,
        path: &Path,
        is_dir: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(root) = self.project_root() else {
            return;
        };
        let relative = if is_dir {
            path.to_path_buf()
        } else {
            path.parent().unwrap_or(Path::new("")).to_path_buf()
        };
        let cwd = root.join(&relative);
        let title = relative
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .or_else(|| self.desk.project(project).map(|p| p.name.clone()))
            .unwrap_or_else(|| "shell".to_owned());

        let runs = guild_session::Runs::Shell;
        let id = self.desk.add_session(guild_session::SessionRecord::new(
            title,
            project,
            cwd.clone(),
            runs,
        ));
        let (ide, hooks) = (self.ide_port(), self.hook_launch());
        let body = crate::Body::open(
            id,
            runs,
            &cwd,
            None,
            &self.settings,
            self.theme,
            ide,
            hooks,
            None,
            self.desk
                .session(id)
                .and_then(|record| record.launch_selection.as_ref()),
            &self.start_size,
            cx,
        );
        let tab = self.adopt(self.seat_for_project(project), project, body, cx);
        self.active_tab = Some(tab);
        self.save();
        cx.notify();
    }

    /// *Find in Files…* on a row: the Find in Files popup, its *Directory*
    /// chip on this row's directory — M8.9. The project's own row is the
    /// project.
    pub(crate) fn find_in_directory(
        &mut self,
        path: &Path,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let scope = crate::find_in_files::scope_of_row(path, is_dir);
        self.open_find_in_files(window, cx);
        self.find_in_files_scope(scope, cx);
    }

    /// *Add to VCS* on a row: `git add`, the write M8.3 already has.
    pub(crate) fn stage_path(&mut self, relative: PathBuf) {
        let Some(project) = self.active_project else {
            return;
        };
        self.run_git_write(project, crate::git::Write::Stage(vec![relative]));
    }

    /// The root Guild has open that a source under `tree_root` is, or
    /// holds — [`open_root_among`] over every project the desk holds, in
    /// every workspace, since each of them may hold tabs and sessions.
    ///
    /// **By spelling only**, so the menu and the rows can ask it on every
    /// frame without touching the disk; the handlers' refusal also asks
    /// [`open_root_resolved`], which follows symlinks.
    pub(crate) fn open_root_in(&self, tree_root: &Path, sources: &[PathBuf]) -> Option<PathBuf> {
        open_root_among(
            tree_root,
            sources,
            self.desk.projects().map(|project| project.path.as_path()),
        )
    }

    /// Refuse a rename, a move or a Trash whose sources are, or hold, an
    /// open root, and say why — GLD-057. `true` when refused.
    ///
    /// **Each gesture's handler asks, not only its menu item.** A key
    /// binding reaches the handler without the menu, and a drop reaches
    /// the transfer without either.
    pub(crate) fn refuse_open_root(&mut self, tree_root: &Path, sources: &[PathBuf]) -> bool {
        let Some(open) = self.open_root_in(tree_root, sources).or_else(|| {
            open_root_resolved(
                tree_root,
                sources,
                self.desk.projects().map(|project| project.path.as_path()),
            )
        }) else {
            return false;
        };
        let said = self.ui().notice_open_root(&open.display().to_string());
        self.warn(said);
        true
    }

    /// How a row of the active tree stands to the open roots, for its
    /// menu — GLD-057.
    pub(crate) fn open_root_of_row(&self, path: &Path) -> Option<crate::view::menu::OpenRoot> {
        let tree_root = self.project_root()?;
        let open = self.open_root_in(&tree_root, &[path.to_path_buf()])?;
        Some(if open == path {
            crate::view::menu::OpenRoot::Is
        } else {
            crate::view::menu::OpenRoot::Holds
        })
    }

    /// The active project's directory.
    pub(crate) fn project_root(&self) -> Option<PathBuf> {
        self.desk
            .project(self.active_project?)
            .map(|project| project.path.clone())
    }

    pub(crate) fn store_expanded(&mut self) {
        let Some(project) = self.active_project else {
            return;
        };
        let seat = self.seat_for_project(project);
        let Some(state) = self.trees.get(&seat) else {
            return;
        };
        let expanded: Vec<PathBuf> = state.tree.expanded().map(Path::to_path_buf).collect();
        let Some(path) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let mut stored = guild_session::project_state::load(&path);
        let first = self
            .desk
            .workspaces()
            .first()
            .map(|workspace| workspace.id)
            .unwrap_or(self.active_workspace);
        let mut seat = stored.seat(self.active_workspace, first);
        seat.expanded = expanded;
        stored.set_seat(self.active_workspace, first, seat);
        if let Err(err) = guild_session::project_state::save(&path, &stored) {
            tracing::warn!(project = %path.display(), %err, "the expanded directories were not saved");
        }
    }
}

impl Root {
    pub(crate) fn tree_file_click(
        &mut self,
        origin: crate::Origin,
        path: &Path,
        mods: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let order: Vec<_> = self
            .tree_file_rows()
            .into_iter()
            .map(|(_, key)| key)
            .collect();
        let Some(state) = self.tree_mut() else {
            return;
        };
        let hidden: Vec<_> = state
            .files
            .selected
            .iter()
            .filter(|key| !order.contains(key))
            .cloned()
            .collect();
        state.files.click(
            &(origin, path.to_path_buf()),
            &order,
            mods.platform,
            mods.shift,
        );
        if mods.platform {
            for key in hidden {
                if !state.files.contains(&key) {
                    state.files.selected.push(key);
                }
            }
        }
        state.sync_file_cursor();
        let focus = state.focus.clone();
        if !mods.platform && !mods.shift {
            self.open_file_from(origin, path, window, cx);
        }
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(crate) fn select_all_tree_files(&mut self, cx: &mut Context<Self>) {
        let order: Vec<_> = self
            .tree_file_rows()
            .into_iter()
            .filter(|(_, (origin, _))| *origin == crate::Origin::Project)
            .map(|(_, key)| key)
            .collect();
        if let Some(state) = self.tree_mut() {
            if state.naming.is_some() {
                return;
            }
            state.files.key("a", &order, true, false);
            state.sync_file_cursor();
        }
        cx.notify();
    }

    fn tree_files_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let mut rows = self.tree_file_rows();
        if event.keystroke.key == "a" && event.keystroke.modifiers.platform {
            rows.retain(|(_, (origin, _))| *origin == crate::Origin::Project);
        }
        let order: Vec<_> = rows.iter().map(|(_, key)| key.clone()).collect();
        let Some(state) = self.tree_mut() else {
            return false;
        };
        state.settle_selection();
        let key = event.keystroke.key.as_str();
        let mods = event.keystroke.modifiers;
        if key == "enter"
            && state.files.focus.as_ref().is_some_and(|(origin, path)| {
                state
                    .navigation_tree(*origin)
                    .is_some_and(|tree| !tree.root().join(path).is_dir())
            })
        {
            let selected = state.files.selected.clone();
            let Some(from) = self.active_project else {
                return true;
            };
            // Each path is the Explorer's root's, resolved before any opens:
            // in a project group's folder a member's file brings the member
            // forward, and the next path would be read against it.
            let mut opens = Vec::new();
            for (origin, path) in selected {
                if self
                    .root_of(from, origin)
                    .is_some_and(|root| root.join(&path).is_dir())
                {
                    continue;
                }
                let routed = (origin == crate::Origin::Project)
                    .then(|| self.member_file(from, &path))
                    .flatten();
                opens.push(match routed {
                    Some((owner, inner)) => (owner, origin, inner),
                    None => (from, origin, path),
                });
            }
            for (owner, origin, path) in opens {
                if self.active_project != Some(owner) {
                    self.select_project(owner, cx);
                }
                if self.active_project == Some(owner) {
                    self.open_file_from(origin, &path, window, cx);
                }
            }
        } else if (mods.platform && key == "a")
            || (mods.shift && matches!(key, "up" | "down" | "home" | "end"))
        {
            // Command with an arrow is the plain arrow — decision 54 — and
            // walks the tree below, directories and all.
            state.files.key(key, &order, mods.platform, mods.shift);
            state.sync_file_cursor();
            if let Some(index) = state
                .files
                .focus
                .as_ref()
                .and_then(|key| rows.iter().find(|(_, k)| k == key).map(|(i, _)| *i))
            {
                state.scroll.scroll_to_item(index, ScrollStrategy::Top);
            }
        } else if key == "c" && mods.platform {
            self.file_clipboard(false, cx);
        } else {
            return false;
        }
        cx.notify();
        true
    }
}

// These are here rather than in `tests/all/` because `Naming` and
// `writable_row` are `pub(crate)`: what the crate promises outside is
// the menu that stops offering the two items, and `tests/all/menu.rs`
// checks that. This is the other half — the refusal beside the gesture.
#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn library_fold_prunes_only_that_source_and_deselection_disarms_legacy_actions(
        cx: &mut gpui::TestAppContext,
    ) {
        let project = temp("selection-project");
        let library = temp("selection-library");
        for folder in [&project, &library] {
            std::fs::write(folder.path().join("same.rs"), "file").unwrap();
        }
        cx.update(|cx| {
            let mut state = TreeState {
                tree: Tree::open(project.path()),
                selected: None,
                files: Default::default(),
                scroll: UniformListScrollHandle::new(),
                focus: cx.focus_handle(),
                naming: None,
                speed: None,
                libraries_open: true,
                library_trees: [(0, Tree::open(library.path()))].into(),
                library_selected: None,
            };
            let rows = [
                (crate::Origin::Project, PathBuf::from("same.rs")),
                (crate::Origin::External(0), PathBuf::from("same.rs")),
            ];
            state.files.click(&rows[0], &rows, false, false);
            state.files.click(&rows[1], &rows, true, false);
            state.sync_file_cursor();
            assert!(
                state.selected.is_none(),
                "external files never aim project writes"
            );
            assert_eq!(state.files.selected.len(), 2);
            state.files.click(&rows[1], &rows, true, false);
            state.sync_file_cursor();
            assert!(state.selected.is_none());
            assert!(state.library_selected.is_none());
            state.files.click(&rows[1], &rows, true, false);
            state.libraries_open = false;
            state.settle_selection();
            assert_eq!(state.files.selected, rows[..1]);
            assert!(
                state.selected.is_none(),
                "a hidden library cursor cannot retarget the project namesake"
            );
            assert!(state.library_selected.is_none());
        });
    }

    #[gpui::test]
    fn an_unread_library_root_keeps_its_selection(cx: &mut gpui::TestAppContext) {
        let project = temp("unread-root-project");
        cx.update(|cx| {
            let mut state = TreeState {
                tree: Tree::open(project.path()),
                selected: None,
                files: Default::default(),
                scroll: UniformListScrollHandle::new(),
                focus: cx.focus_handle(),
                naming: None,
                speed: None,
                libraries_open: true,
                library_trees: Default::default(),
                library_selected: Some((0, PathBuf::new())),
            };
            // A project refresh settles the selection; the root is still drawn.
            state.settle_selection();
            assert_eq!(state.library_selected, Some((0, PathBuf::new())));
            state.libraries_open = false;
            state.settle_selection();
            assert_eq!(
                state.library_selected, None,
                "a folded section shows no root"
            );
        });
    }

    #[gpui::test]
    fn hidden_file_cursor_recovers_ancestor_in_its_source(cx: &mut gpui::TestAppContext) {
        let project = temp("cursor-project");
        let library = temp("cursor-library");
        for folder in [&project, &library] {
            std::fs::create_dir(folder.path().join("docs")).unwrap();
            std::fs::write(folder.path().join("docs/a.rs"), "file").unwrap();
        }
        cx.update(|cx| {
            let mut state = TreeState {
                tree: Tree::open(project.path()),
                selected: None,
                files: Default::default(),
                scroll: UniformListScrollHandle::new(),
                focus: cx.focus_handle(),
                naming: None,
                speed: None,
                libraries_open: true,
                library_trees: [(0, Tree::open(library.path()))].into(),
                library_selected: None,
            };
            for origin in [crate::Origin::Project, crate::Origin::External(0)] {
                for removed in [false, true] {
                    let tree = state.navigation_tree_mut(origin).unwrap();
                    tree.expand(Path::new("docs"));
                    let file = (origin, PathBuf::from("docs/a.rs"));
                    state
                        .files
                        .click(&file, std::slice::from_ref(&file), false, false);
                    state.sync_file_cursor();
                    let tree = state.navigation_tree_mut(origin).unwrap();
                    if removed {
                        let folder = if origin == crate::Origin::Project {
                            &project
                        } else {
                            &library
                        };
                        std::fs::remove_file(folder.path().join("docs/a.rs")).unwrap();
                        tree.refresh(Path::new("docs"));
                    } else {
                        tree.collapse(Path::new("docs"));
                    }
                    state.settle_selection();
                    state.settle_selection(); // A later redraw must keep the recovered directory.
                    assert_eq!(
                        state.files.selected.is_empty(),
                        removed || origin != crate::Origin::Project
                    );
                    assert!(state.files.focus.is_none());
                    assert_eq!(
                        state.selected,
                        (origin == crate::Origin::Project).then(|| "docs".into())
                    );
                    assert_eq!(
                        state.library_selected,
                        (origin == crate::Origin::External(0)).then(|| (0, "docs".into()))
                    );
                }
            }
        });
    }

    /// **The project's own row cannot be renamed, by any route.**
    ///
    /// Its path is the tree's empty one, and every route to a rename
    /// goes through `Naming::rename`: the keyboard's `name_something`
    /// and the right button's `TreeDeed::Rename`. Until M8.8 the
    /// keyboard route refused it and the menu did not exist; the menu
    /// arrived with the same gesture and no such check.
    #[test]
    fn a_rename_changes_only_the_basename() {
        let mut naming = Naming::rename(Path::new("src/file.rs")).unwrap();
        for invalid in ["../escape.rs", "nested/file.rs", "/absolute", ".", ".."] {
            naming.typed = invalid.into();
            assert!(naming.asks_for().is_none(), "{invalid}");
        }
        naming.typed = "renamed.rs".into();
        assert_eq!(naming.asks_for(), Some("src/renamed.rs".into()));
    }

    #[test]
    fn the_projects_own_row_is_not_renameable() {
        assert!(
            Naming::rename(Path::new("")).is_none(),
            "the empty path is the directory the window has open"
        );
        let named = Naming::rename(Path::new("crates/desktop/src/lib.rs"))
            .expect("an ordinary row renames");
        assert_eq!(named.typed, "lib.rs", "and it starts from the name it has");
        assert_eq!(named.dir, Path::new("crates/desktop/src"));
    }

    /// The same rule as a predicate, which is what `delete_asking` reads:
    /// a row is writable when it is not the project itself.
    #[test]
    fn only_a_row_under_the_project_may_be_written_to() {
        assert!(!writable_row(Path::new("")));
        assert!(writable_row(Path::new("src")));
        assert!(writable_row(Path::new("src/lib.rs")));
    }

    /// GLD-057's review: a tree opened through a symlinked ancestor still
    /// finds a member opened by its real path, and a symlink row that
    /// points at an open root is the link, not the root.
    #[test]
    fn an_open_root_is_found_through_a_symlinked_ancestor() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real/acme");
        std::fs::create_dir_all(real.join("member")).unwrap();
        std::fs::create_dir_all(real.join("plain")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("real"), dir.path().join("alias")).unwrap();
        std::os::unix::fs::symlink(real.join("member"), real.join("link")).unwrap();
        let tree = dir.path().join("alias/acme");
        let member = real.join("member");
        let open = [member.as_path()];
        let find = |source: &str| open_root_resolved(&tree, &[PathBuf::from(source)], open);
        assert_eq!(
            open_root_among(&tree, &[PathBuf::from("member")], open),
            None,
            "the spelling alone misses it"
        );
        assert_eq!(find("member"), Some(PathBuf::from("member")));
        assert_eq!(find("plain"), None);
        assert_eq!(find("link"), None, "the link is renamed, not its target");
    }

    /// GLD-057: a source is refused when it is, or holds, an open root —
    /// by components, so a sibling whose name merely begins the same way
    /// is not; and the tree's own root is `writable_row`'s, not this.
    #[test]
    fn an_open_root_is_found_at_or_under_a_source_and_nowhere_else() {
        let tree = Path::new("/w/acme");
        let open = [
            Path::new("/w/acme"),
            Path::new("/w/acme/member"),
            Path::new("/w/acme/outer/deep/inner"),
            Path::new("/w/acme/outer/shallow"),
            Path::new("/w/elsewhere"),
        ];
        let find = |source: &str| open_root_among(tree, &[PathBuf::from(source)], open);
        assert_eq!(find("member"), Some(PathBuf::from("member")));
        assert_eq!(find("outer"), Some(PathBuf::from("outer/shallow")));
        assert_eq!(find("outer/deep"), Some(PathBuf::from("outer/deep/inner")));
        assert_eq!(find("member-fix"), None);
        assert_eq!(find("member/src"), None);
        assert_eq!(find("plain"), None);
        assert_eq!(find(""), None);
        assert_eq!(
            open_root_among(
                tree,
                &[PathBuf::from("plain"), PathBuf::from("member")],
                open
            ),
            Some(PathBuf::from("member"))
        );
    }

    /// A folder of the test's own, gone when the test lets go of it.
    fn temp(name: &str) -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix(&format!("guild-rename-{name}-"))
            .tempdir()
            .expect("a scratch folder")
    }

    /// **A rename never replaces.** `rename(2)` puts the moved file where
    /// the other one was and says nothing; a person who renamed `a` to a
    /// name they did not know was taken lost `b` for good.
    #[test]
    fn a_rename_onto_a_name_that_exists_is_refused_and_both_files_stay() {
        let folder = temp("taken");
        let dir = folder.path();
        std::fs::write(dir.join("a.txt"), "one").unwrap();
        std::fs::write(dir.join("b.txt"), "two").unwrap();
        let err = rename_without_replacing(dir, Path::new("a.txt"), Path::new("b.txt"))
            .expect_err("b.txt is taken");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one");
        assert_eq!(std::fs::read_to_string(dir.join("b.txt")).unwrap(), "two");

        // And an empty directory, which `rename(2)` replaces too.
        std::fs::create_dir(dir.join("empty")).unwrap();
        let err = rename_without_replacing(dir, Path::new("a.txt"), Path::new("empty"))
            .expect_err("the directory is taken");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        assert!(dir.join("empty").is_dir());
        assert_eq!(std::fs::read_to_string(dir.join("a.txt")).unwrap(), "one");
    }

    /// On a case-insensitive volume `Readme.md` and `README.md` are one
    /// file, and changing the case is a thing a person does every week.
    #[test]
    fn a_rename_that_changes_only_the_case_goes_through() {
        let folder = temp("case");
        let dir = folder.path();
        std::fs::write(dir.join("probe"), "").unwrap();
        let insensitive = dir.join("PROBE").exists();
        std::fs::write(dir.join("readme.md"), "text").unwrap();
        rename_without_replacing(dir, Path::new("readme.md"), Path::new("README.md"))
            .unwrap_or_else(|err| {
                panic!(
                    "a case-only rename on a case-{} volume was refused: {err}",
                    if insensitive {
                        "insensitive"
                    } else {
                        "sensitive"
                    }
                )
            });
        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names.contains(&"README.md".to_owned()) && !names.contains(&"readme.md".to_owned()),
            "checked on a case-{} volume, the directory holds {names:?}",
            if insensitive {
                "insensitive"
            } else {
                "sensitive"
            }
        );
        assert_eq!(
            std::fs::read_to_string(dir.join("README.md")).unwrap(),
            "text"
        );
    }

    /// A link followed to its target is the same file as the target, and
    /// a rename judged by that would replace `a` with the link.
    #[test]
    fn a_link_renamed_onto_its_target_is_refused_and_the_target_stays() {
        let folder = temp("symlink");
        let dir = folder.path();
        std::fs::write(dir.join("a"), "one").unwrap();
        std::os::unix::fs::symlink(dir.join("a"), dir.join("b")).unwrap();
        let err =
            rename_without_replacing(dir, Path::new("b"), Path::new("a")).expect_err("a is taken");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        let meta = std::fs::symlink_metadata(dir.join("a")).unwrap();
        assert!(meta.file_type().is_file(), "a is still a regular file");
        assert_eq!(std::fs::read_to_string(dir.join("a")).unwrap(), "one");
    }

    /// Task 0.2: a hard link is the same device and inode under another
    /// name, and `rename(2)` of one onto the other answers success and
    /// does nothing — a rename reported that never happened.
    #[test]
    fn a_hard_link_renamed_onto_its_other_name_is_refused_and_both_names_stay() {
        let folder = temp("hardlink");
        let dir = folder.path();
        std::fs::write(dir.join("a"), "one").unwrap();
        std::fs::hard_link(dir.join("a"), dir.join("b")).unwrap();
        let err =
            rename_without_replacing(dir, Path::new("b"), Path::new("a")).expect_err("a is taken");
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists, "{err}");
        assert!(dir.join("a").exists() && dir.join("b").exists());
    }

    /// **Enter on a file opens it, and the keyboard goes with it.** The
    /// M8.1 plan promised *enter opens* and nothing drove it: Enter on a
    /// file answered `Nothing`, and a person walking the tree with the
    /// arrows had to reach for the pointer to read what they had found.
    #[test]
    fn enter_on_a_file_opens_it_and_takes_the_keyboard_there() {
        let file = Path::new("src/lib.rs");
        assert_eq!(
            step_for("enter", Some((file, file, false, false)), Some(3), 9),
            Step::Open {
                path: file.to_path_buf(),
                keyboard_to_file: true,
            }
        );
    }

    /// Right is a movement key: a person stepping through the tree with
    /// the arrows expects the next arrow to be the tree's, so the file
    /// opens and the keyboard stays where it was pressed.
    #[test]
    fn right_on_a_file_opens_it_and_leaves_the_keyboard_in_the_tree() {
        let file = Path::new("README.md");
        assert_eq!(
            step_for("right", Some((file, file, false, false)), Some(0), 9),
            Step::Open {
                path: file.to_path_buf(),
                keyboard_to_file: false,
            }
        );
    }

    /// The directory arms are the ones `tree_key` had, moved and not
    /// changed: Right opens a folded directory and steps into an open
    /// one, Enter folds and unfolds.
    #[test]
    fn enter_and_right_on_a_directory_do_what_they_did() {
        let dir = Path::new("src");
        assert_eq!(
            step_for("right", Some((dir, dir, true, false)), Some(2), 9),
            Step::Expand(dir.to_path_buf())
        );
        assert_eq!(
            step_for("right", Some((dir, dir, true, true)), Some(2), 9),
            Step::Select(3)
        );
        assert_eq!(
            step_for("right", Some((dir, dir, true, true)), Some(9), 9),
            Step::Select(9),
            "an open directory on the last row stays there"
        );
        assert_eq!(
            step_for("enter", Some((dir, dir, true, false)), Some(2), 9),
            Step::Toggle(dir.to_path_buf())
        );
        assert_eq!(
            step_for("enter", Some((dir, dir, true, true)), Some(2), 9),
            Step::Toggle(dir.to_path_buf())
        );
        assert_eq!(step_for("enter", None, None, 9), Step::Nothing);
        assert_eq!(step_for("right", None, None, 9), Step::Nothing);
        assert_eq!(step_for("q", None, None, 9), Step::NotTheTrees);
    }
}
