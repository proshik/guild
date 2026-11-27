//! The file tree, drawn in the left panel.
//!
//! One row per entry of `Tree::rows`, through `uniform_list` so that a
//! directory of thousands costs what is on screen. The row is section 6.4 of
//! the design system: square and `space::TREE_ROW` high, indented
//! `space::TREE_INDENT` per level, the name in the interface face at
//! `size::UI`; the chevron in `text.faint` on a directory only; the icon 16;
//! the name in `text.primary`, or `text.faint` when `.gitignore` names it,
//! with the icon at half opacity beside it.
//!
//! The row's look and behaviour — decision 54: the selection is
//! one solid fill with no outline, `accent.patina_bg` while the tree has
//! the keys and a neutral grey while they are elsewhere; the row under the
//! pointer takes a softer grey, which is not the highlight's colour (the
//! M5.8 trap); the pointer is the arrow; the name is in the interface face.
//! [`ground`] is that look, for every list of files.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Context, div, rgb, uniform_list};

use crate::i18n::Text;
use crate::theme::{Mark, Selected};
use crate::{Root, icons, theme};

/// Whether a list has the keys: its window is the active one and the focus
/// is inside `handle`. An inactive window's lists draw the grey selection
/// — decision 54.
pub(crate) fn keys_in(handle: &gpui::FocusHandle, window: &gpui::Window, cx: &gpui::App) -> bool {
    window.is_window_active() && handle.contains_focused(window, cx)
}

/// A file-list row's ground — decision 54: its selection, else its Git
/// tint, and under the pointer a soft grey over an unselected row. The
/// Explorer, the Commit panel, a commit's files, Push's files, Find and
/// Find Usages all draw their rows through it.
pub(crate) fn ground<E: InteractiveElement + Styled>(
    row: E,
    theme: theme::Theme,
    mark: Option<Mark>,
    selected: Selected,
    ground: u32,
) -> E {
    let row = row.bg(theme.file_row(mark, selected, ground));
    if selected == Selected::No {
        row.hover(move |style| style.bg(theme.file_row_hover(mark, ground)))
    } else {
        row
    }
}

/// What a row needs to be drawn, copied out of the model so that the
/// list's closure owns it.
struct RowData {
    mark: Option<crate::theme::Mark>,
    path: PathBuf,
    /// Where folding this row starts: its own path, or the top of the
    /// chain it stands for — M8.10.
    fold: PathBuf,
    /// What the row says: a name, or a chain's names joined.
    name: String,
    depth: usize,
    is_dir: bool,
    expanded: bool,
    ignored: bool,
    stem: String,
    selected: bool,
    /// Which list this row belongs to. M8.8.3.
    of: Of,
}

/// Which of the panel's two lists a row is in.
///
/// **One `uniform_list` and not two.** The panel scrolls as one thing,
/// and two lists stacked in it would each have a scrollbar and a
/// keyboard of its own — the section is a continuation of the tree, a
/// child of the project root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Of {
    /// The project's own tree.
    Project,
    /// The *External Libraries* head, which folds the section.
    LibrariesHead,
    /// A row inside the root at this index — its own row included,
    /// which is the one with the crate's name on it.
    Library(usize),
    /// A dependency the build system listed that has not been unfolded,
    /// by its place in `library_entries`. M8.14.
    Dependency(usize),
}

impl Root {
    /// Visible file identities in exactly the order of the shared project/library list.
    pub(crate) fn tree_file_rows(&self) -> Vec<(usize, (crate::Origin, PathBuf))> {
        let Some(state) = self.tree() else {
            return Vec::new();
        };
        let project = state.tree.rows();
        let mut files: Vec<_> = project
            .iter()
            .enumerate()
            .filter(|(_, r)| crate::root::tree::writable_row(r.path()))
            .map(|(i, r)| (i, (crate::Origin::Project, r.path().to_path_buf())))
            .collect();
        files.extend(
            self.library_rows(icons::Palette::from(self.theme.kind))
                .into_iter()
                .enumerate()
                .filter_map(|(i, r)| match r.of {
                    Of::Library(at) if !r.is_dir => {
                        Some((project.len() + i, (crate::Origin::External(at), r.path)))
                    }
                    _ => None,
                }),
        );
        files
    }

    pub(crate) fn tree_speed_rows(
        &self,
    ) -> Vec<crate::speed_search::Candidate<(crate::Origin, PathBuf)>> {
        let Some(state) = self.tree() else {
            return Vec::new();
        };
        let rows = state.tree.rows();
        let mut candidates: Vec<_> = rows
            .iter()
            .enumerate()
            .map(|(i, r)| {
                crate::speed_search::Candidate::new(
                    (crate::Origin::Project, r.path().to_path_buf()),
                    r.label(),
                    i,
                )
            })
            .collect();
        candidates.extend(
            self.library_rows(icons::Palette::from(self.theme.kind))
                .into_iter()
                .enumerate()
                .filter_map(|(i, r)| match r.of {
                    Of::Library(at) => Some(crate::speed_search::Candidate::new(
                        (crate::Origin::External(at), r.path),
                        r.name,
                        rows.len() + i,
                    )),
                    _ => None,
                }),
        );
        candidates
    }

    /// The tree of the active project as the panel's body, or `None` when
    /// there is no project to show.
    pub(crate) fn render_tree(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let state = self.tree()?;
        let theme = self.theme;
        let palette = icons::Palette::from(theme.kind);
        let status = self
            .active_project
            .and_then(|project| self.git_status.get(&project))
            .and_then(|answer| answer.as_ref().ok());
        // What a Kotlin or Java file declares, where it has been read —
        // the row draws the class, not the language. M8.13.
        let declared = |path: &Path| {
            self.active_project
                .and_then(|project| self.declarations.of(project, path))
                .map(|kind| icons::for_declaration(kind, palette))
        };
        let mut rows: Vec<RowData> = state
            .tree
            .rows()
            .iter()
            .map(|row| RowData {
                mark: status.and_then(|map| map.mark_for(row.path())),
                path: row.path().to_path_buf(),
                fold: row.chain_root().to_path_buf(),
                name: row.label().to_owned(),
                depth: row.depth(),
                is_dir: row.is_dir(),
                expanded: row.is_expanded(),
                ignored: row.is_ignored(),
                stem: if row.is_dir() {
                    icons::for_dir(row.path(), row.is_expanded(), palette)
                } else {
                    declared(row.path()).unwrap_or_else(|| icons::for_file(row.name(), palette))
                },
                // The project's row, and a directory the selection fell
                // back to when its file went away and nothing else stayed
                // selected, are selected by the navigation cursor alone —
                // `settle_selection`.
                selected: state
                    .files
                    .contains(&(crate::Origin::Project, row.path().to_path_buf()))
                    || (state.files.selected.is_empty()
                        && state.selected.as_deref() == Some(row.path())),
                of: Of::Project,
            })
            .collect();
        rows.extend(self.library_rows(palette));
        let cut_sources = self.cut_sources(cx);
        let cut: Vec<_> = rows
            .iter()
            .map(|r| {
                r.of == Of::Project && cut_sources.iter().any(|source| r.path.starts_with(source))
            })
            .collect();
        let query = state
            .speed
            .as_ref()
            .map(|s| s.query.clone())
            .unwrap_or_default();
        let error = state.tree.error(Path::new("")).map(str::to_owned);
        let count = rows.len();
        let rows = Rc::new(rows);
        let focus = state.focus.clone();
        let row_focus = focus.clone();
        // A Rename or Move field takes the keys inside the tree's focus;
        // the speed-search field is the list's own.
        let speed_up = state.speed.is_some();
        let scroll = state.scroll.clone();

        let body: gpui::AnyElement = if let Some(error) = error {
            self.tree_note(Text::TreeCannotRead, Some(error))
        } else if count == 0 {
            self.tree_note(Text::TreeEmpty, None)
        } else {
            uniform_list(
                "tree-rows",
                count,
                cx.processor(move |_root: &mut Root, range: Range<usize>, window, cx| {
                    // Blue while the tree has the keys, or its own menu
                    // has them; grey otherwise — decision 54.
                    let focused = (keys_in(&row_focus, window, cx)
                        && (speed_up || !crate::text_field::focused(window, cx)))
                        || (window.is_window_active()
                            && _root.menu_over(|target| {
                                matches!(
                                    target,
                                    crate::view::menu::Target::TreeRow { .. }
                                        | crate::view::menu::Target::LibraryRow { .. }
                                )
                            }));
                    let dim = |cut: bool| if cut { 0.5 } else { 1.0 };
                    let tree_root = _root.project_root();
                    range
                        .map(|index| {
                            let row = &rows[index];
                            let path = row.path.clone();
                            let is_dir = row.is_dir;
                            let selector = format!("explorer-row-{}", path.display());
                            let drag_path = path.clone();
                            div()
                                .id(("tree-row", index))
                                .map(|element| {
                                    let holds_open_root = is_dir
                                        && row.of == Of::Project
                                        && tree_root.as_deref().is_some_and(|tree_root| {
                                            _root
                                                .open_root_in(
                                                    tree_root,
                                                    std::slice::from_ref(&drag_path),
                                                )
                                                .is_some()
                                        });
                                    super::tree_drag::row(
                                        element,
                                        drag_path,
                                        is_dir,
                                        row.of == Of::Project,
                                        _root.active_project.filter(|_| !holds_open_root),
                                        theme,
                                        cx,
                                    )
                                })
                                .debug_selector(move || selector.clone())
                                // Selection and Git tint span the list's
                                // width, including the empty trailing space.
                                .w_full()
                                .h(theme::space::TREE_ROW)
                                .flex()
                                .items_center()
                                .gap(theme::space::TREE_GAP)
                                .pl(theme::space::TREE_PADDING
                                    + theme::space::TREE_INSET
                                    + theme::space::TREE_INDENT * row.depth as f32)
                                .pr(theme::space::TREE_PADDING + theme::space::TREE_INSET)
                                // A cut row dims what it shows, never its
                                // ground: the selection and the hover stay whole.
                                .map(|this| {
                                    ground(
                                        this,
                                        theme,
                                        row.mark,
                                        Selected::of(row.selected, focused),
                                        theme.surface.app,
                                    )
                                })
                                .child(
                                    div()
                                        .flex_none()
                                        .size(theme::space::CHEVRON)
                                        .opacity(dim(cut[index]))
                                        .when(is_dir, |this| {
                                            this.child(
                                                icons::chevron(row.expanded)
                                                    .text_color(rgb(theme.text.faint)),
                                            )
                                        }),
                                )
                                .child(
                                    icons::icon(&row.stem).flex_none().opacity(
                                        dim(cut[index]) * if row.ignored { 0.5 } else { 1.0 },
                                    ),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .opacity(dim(cut[index]))
                                        .text_size(theme::size::UI)
                                        .text_color(rgb(if row.ignored {
                                            theme.text.faint
                                        } else {
                                            theme.text.primary
                                        }))
                                        .child(crate::view::speed_search::label(
                                            row.name.clone(),
                                            if matches!(row.of, Of::Project | Of::Library(_)) {
                                                &query
                                            } else {
                                                ""
                                            },
                                            theme,
                                        )),
                                )
                                // The right button selects the row, or
                                // keeps a group it is in, and says what
                                // the menu about to be built is about —
                                // decision 54; it never opens. **A row
                                // under a dependency's root says a
                                // different target**, whose menu cannot
                                // write — M8.8.3.
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener({
                                        let path = path.clone();
                                        let expanded = row.expanded;
                                        let of = row.of;
                                        move |root: &mut Root, _event, window, cx| {
                                            let origin = match of {
                                                Of::Library(at) => crate::Origin::External(at),
                                                _ => crate::Origin::Project,
                                            };
                                            let target = match of {
                                                // Neither has a menu: the head
                                                // is not the project's root row,
                                                // whose menu it used to open.
                                                Of::LibrariesHead | Of::Dependency(_) => return,
                                                Of::Library(at) => {
                                                    crate::view::menu::Target::LibraryRow {
                                                        at,
                                                        path: path.clone(),
                                                        is_dir,
                                                        expanded,
                                                    }
                                                }
                                                _ => crate::view::menu::Target::TreeRow {
                                                    path: path.clone(),
                                                    is_dir,
                                                    expanded,
                                                },
                                            };
                                            root.tree_context(origin, &path, window, cx);
                                            root.note_menu_target(target);
                                        }
                                    }),
                                )
                                .on_click(cx.listener({
                                    let of = row.of;
                                    let fold = row.fold.clone();
                                    move |root: &mut Root, event: &gpui::ClickEvent, window, cx| {
                                        cx.stop_propagation();
                                        match of {
                                            Of::Project => {
                                                root.tree_click(
                                                    &path,
                                                    &fold,
                                                    is_dir,
                                                    event.modifiers(),
                                                    window,
                                                    cx,
                                                );
                                            }
                                            Of::LibrariesHead => root.toggle_libraries(cx),
                                            Of::Dependency(index) => {
                                                root.open_dependency(index, cx);
                                            }
                                            Of::Library(at) => {
                                                root.library_click(
                                                    at,
                                                    &path,
                                                    is_dir,
                                                    event.modifiers(),
                                                    window,
                                                    cx,
                                                );
                                            }
                                        }
                                    }
                                }))
                        })
                        .collect::<Vec<_>>()
                }),
            )
            .track_scroll(&scroll)
            .size_full()
            .into_any_element()
        };

        let naming = self.render_naming(cx);

        Some(
            div()
                .id("tree")
                .on_drop(cx.listener(
                    |root, drag: &crate::root::tree_drag::FileDrag, window, cx| {
                        root.drop_tree_drag(drag, PathBuf::new(), window, cx)
                    },
                ))
                .on_drop(
                    cx.listener(|root, paths: &gpui::ExternalPaths, window, cx| {
                        root.drop_inbound(paths, PathBuf::new(), window, cx)
                    }),
                )
                .track_focus(&focus)
                .key_context("tree")
                .on_action(cx.listener(|root, _: &crate::actions::Copy, window, cx| {
                    if crate::text_field::focused(window, cx) {
                        return;
                    }
                    root.file_clipboard(false, cx);
                    cx.stop_propagation();
                }))
                .on_action(
                    cx.listener(|root, _: &crate::actions::SelectAll, _window, cx| {
                        root.select_all_tree_files(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_action(
                    cx.listener(|root, _: &crate::actions::CutFiles, window, cx| {
                        if root.tree_has_focus(window) {
                            root.file_clipboard(true, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_action(cx.listener(|root, _: &crate::actions::Paste, window, cx| {
                    if root.tree_has_focus(window) {
                        root.paste_files(window, cx);
                    }
                    cx.stop_propagation();
                }))
                .on_action(
                    cx.listener(|root, _: &crate::actions::MoveFile, window, cx| {
                        if root.tree_has_focus(window) {
                            root.move_files(window, cx);
                        }
                        cx.stop_propagation();
                    }),
                )
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if root.tree_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .py(theme::space::CARD_GAP)
                // **Above the rows, not among them.** A field inserted
                // into the list would be a row `uniform_list` has to
                // count, and every index the keyboard, the scroll and the
                // selection use would be one out while it was up.
                .children(
                    self.tree_speed_field(cx)
                        .map(|field| div().px(theme::space::TREE_INSET).child(field)),
                )
                .children(naming.map(|field| div().px(theme::space::TREE_INSET).child(field)))
                .children(
                    self.render_transfer_form(cx)
                        .map(|form| div().px(theme::space::TREE_INSET).child(form)),
                )
                .child(body)
                // **Over the bottom of the list, not beside it.** Above the
                // rows it pushed the list down for as long as a copy ran;
                // below them it took its height from the viewport, and a
                // list scrolled to its end dropped when it went. Laid over
                // the last rows, it moves nothing — and it takes the mouse
                // itself, but for the wheel: a click on it reached the row
                // hidden under it.
                .children(self.render_transfer_busy(cx).map(|notice| {
                    div()
                        .id("explorer-transfer-notice")
                        .debug_selector(|| "explorer-transfer-notice".into())
                        .block_mouse_except_scroll()
                        .absolute()
                        .left_0()
                        .right_0()
                        .bottom_0()
                        .px(theme::space::TREE_INSET)
                        .bg(rgb(theme.surface.app))
                        .border_t_1()
                        .border_color(rgb(theme.surface.divider))
                        .child(notice)
                }))
                .on_mouse_down(
                    gpui::MouseButton::Right,
                    cx.listener(|root, event: &gpui::MouseDownEvent, window, cx| {
                        root.open_context_menu(event.position, window, cx)
                    }),
                )
                .into_any_element(),
        )
    }

    /// The *External Libraries* section: the head, the roots this
    /// project has landed in, and whatever is unfolded inside them.
    ///
    /// **Placed after the project's rows** — after the content roots.
    /// Nothing walks into it wholesale — an expand-all leaves it out — but
    /// since 2026-09-28 the way to one file is opened: the tab in front,
    /// when it is a dependency's. Arrows walk the selected source's
    /// visible rows, including directories.
    ///
    /// Nothing is listed until a `f12` has landed somewhere: the roots
    /// arrive one at a time, from jumps a person actually made.
    /// Where a row under a library root is in the panel's one list — the
    /// project's rows first, then the section's — to scroll it into view.
    ///
    /// Found by drawing the section's rows rather than by counting them
    /// here: their order is the build system's entries and then the
    /// roots `f12` reached, and a second copy of that rule would drift.
    pub(crate) fn library_row_index(&self, at: usize, path: &Path) -> Option<usize> {
        let state = self.tree()?;
        let palette = icons::Palette::from(self.theme.kind);
        let offset = self
            .library_rows(palette)
            .iter()
            .position(|row| row.of == Of::Library(at) && row.path == path)?;
        Some(state.tree.rows().len() + offset)
    }

    fn library_rows(&self, palette: icons::Palette) -> Vec<RowData> {
        let Some(project) = self.active_project else {
            return Vec::new();
        };
        let Some(state) = self.tree() else {
            return Vec::new();
        };
        let roots = self.libraries.of(project);
        // **What the build system resolved, then what `f12` reached that
        // it did not** — M8.14. Until a first answer arrives the section
        // is what it was before: the roots a person went to.
        let entries = self.library_entries(project);
        // A build system being asked is said on the head, so the section
        // is there — and busy — from the first frame rather than appearing
        // five seconds later out of nothing.
        let asking = self
            .dependencies_of(project)
            .is_some_and(|known| known.asking());
        if roots.is_empty() && entries.is_empty() && !asking {
            return Vec::new();
        }
        let head = self.ui().text(Text::ExternalLibraries);
        let mut rows = vec![RowData {
            mark: None,
            path: PathBuf::new(),
            fold: PathBuf::new(),
            name: if asking {
                format!("{head} …")
            } else {
                head.to_owned()
            },
            depth: 0,
            is_dir: true,
            expanded: state.libraries_open,
            ignored: false,
            stem: icons::for_dir(
                std::path::Path::new("library"),
                state.libraries_open,
                palette,
            ),
            selected: false,
            of: Of::LibrariesHead,
        }];
        if !state.libraries_open {
            return rows;
        }
        let mut covered: Vec<usize> = Vec::new();
        for (index, entry) in entries.iter().enumerate() {
            let at = entry
                .root()
                .and_then(|root| roots.iter().position(|known| known == root));
            covered.extend(at);
            let stem = icons::for_library(entry.jdk, palette);
            let busy = entry.root().is_some_and(|root| self.unpacking(root));
            match at {
                Some(at) => rows.extend(self.library_root_rows(at, &entry.label, &stem, palette)),
                None => rows.push(RowData {
                    mark: None,
                    path: PathBuf::new(),
                    fold: PathBuf::new(),
                    // A row whose archive is being opened says so, rather
                    // than looking like a click that did nothing.
                    name: if busy {
                        format!("{} …", entry.label)
                    } else {
                        entry.label.clone()
                    },
                    depth: 1,
                    is_dir: true,
                    expanded: false,
                    ignored: false,
                    stem,
                    selected: false,
                    of: Of::Dependency(index),
                }),
            }
        }
        for (at, root) in roots.iter().enumerate() {
            if covered.contains(&at) {
                continue;
            }
            let name = root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| root.display().to_string());
            let stem = icons::for_dir(std::path::Path::new(""), false, palette);
            rows.extend(self.library_root_rows(at, &name, &stem, palette));
        }
        rows
    }

    /// One library root's rows: its own row, called `name` with `stem`'s
    /// icon, and — once it has been read — everything unfolded under it.
    fn library_root_rows(
        &self,
        at: usize,
        name: &str,
        stem: &str,
        palette: icons::Palette,
    ) -> Vec<RowData> {
        let opened = self.tree().and_then(|state| state.library_trees.get(&at));
        match opened {
            // Read once it has been asked for, and then its own rows —
            // the first of which is the root itself, carrying the name a
            // person knows the library by.
            Some(tree) => tree
                .rows()
                .iter()
                .map(|row| RowData {
                    mark: None,
                    path: row.path().to_path_buf(),
                    fold: row.chain_root().to_path_buf(),
                    name: if row.depth() == 0 {
                        name.to_owned()
                    } else {
                        row.label().to_owned()
                    },
                    depth: row.depth() + 1,
                    is_dir: row.is_dir(),
                    expanded: row.is_expanded(),
                    ignored: row.is_ignored(),
                    stem: if row.depth() == 0 {
                        stem.to_owned()
                    } else if row.is_dir() {
                        icons::for_dir(row.path(), row.is_expanded(), palette)
                    } else {
                        icons::for_file(row.name(), palette)
                    },
                    selected: self.tree().is_some_and(|state| {
                        if row.is_dir() {
                            state.library_selected.as_ref() == Some(&(at, row.path().to_path_buf()))
                        } else {
                            state
                                .files
                                .contains(&(crate::Origin::External(at), row.path().to_path_buf()))
                        }
                    }),
                    of: Of::Library(at),
                })
                .collect(),
            None => vec![RowData {
                mark: None,
                path: PathBuf::new(),
                fold: PathBuf::new(),
                name: name.to_owned(),
                depth: 1,
                is_dir: true,
                expanded: false,
                ignored: false,
                stem: stem.to_owned(),
                // A root is a row before its tree is read, and speed
                // search can land on it.
                selected: self.tree().is_some_and(|state| {
                    state.library_selected.as_ref() == Some(&(at, PathBuf::new()))
                }),
                of: Of::Library(at),
            }],
        }
    }

    /// The field a name is typed into: what it is for, where it will go,
    /// and what has been typed so far.
    ///
    /// Drawn only while a name is being asked for. The shared input owns
    /// editing and focus; the tree handles accepting or dismissing the name.
    fn render_naming(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let ui = self.ui();
        let naming = self.tree()?.naming.as_ref()?;
        let title = match naming.what {
            crate::root::tree::For::File => Text::TreeNewFile,
            crate::root::tree::For::Directory => Text::TreeNewDirectory,
            crate::root::tree::For::Rename => Text::TreeRename,
        };
        // Where it will land, so that *New File…* invoked from a row
        // nobody meant is visible before Enter and not after it.
        let under = naming.dir.display().to_string();
        let empty = naming.typed.is_empty();
        Some(
            div()
                .flex_none()
                .flex()
                .flex_col()
                .gap(theme::space::CARD_INNER_GAP)
                .p(theme::space::CARD_INNER_GAP)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::space::CARD_INNER_GAP)
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(ui.text(title).to_owned())
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .child(under),
                        ),
                )
                .child(
                    div()
                        .h(theme::space::HEAD_TAB)
                        .flex()
                        .items_center()
                        .px(theme::space::CARD_GAP)
                        .rounded(theme::radius::ROW)
                        .bg(rgb(theme.surface.inset))
                        .border_1()
                        .border_color(rgb(theme.accent.brass))
                        // In the face the rows' names are in — decision 54.
                        .text_size(theme::size::UI)
                        .text_color(rgb(if empty {
                            theme.text.faint
                        } else {
                            theme.text.primary
                        }))
                        .child(
                            crate::text_field::TextField::new(
                                "tree-name-input",
                                naming.typed.clone(),
                                cx.listener(|root, value: &String, _, cx| {
                                    if let Some(state) = root.tree_mut()
                                        && let Some(naming) = state.naming.as_mut()
                                    {
                                        naming.typed = value.clone();
                                    }
                                    cx.notify();
                                }),
                            )
                            .autofocus()
                            .initial_selection(naming.initial_selection.clone())
                            .on_key(cx.listener(
                                |root, event, window, cx| {
                                    root.naming_key(event, window, cx);
                                },
                            )),
                        ),
                )
                .into_any_element(),
        )
    }

    /// One quiet line where the rows would be: nothing here, or a
    /// directory that could not be read, with the system's words under it.
    pub(crate) fn tree_note(&self, text: Text, detail: Option<String>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        div()
            .p(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(ui.text(text))
            .when_some(detail, |this, detail| {
                this.child(div().text_size(theme::size::META).child(detail))
            })
            .into_any_element()
    }
}
