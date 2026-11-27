//! The file tree's half of the right-button menu.
//!
//! A row of the Explorer: a file, or a directory that folds. What it
//! offers *about a path* — copying it, revealing it, its diff — is
//! [`super::PathDeed`], which every surface with a path shares; what is
//! here is what only a tree row can do.
//!
//! **The order is the common project view's**, and one thing about
//! it is not decoration: the destructive item sits **inside** the
//! transform group, next to *Rename…*, rather than at the bottom of the
//! menu where a stray click lands. M8.8.3, request 16.
//!
//! File clipboard and Move share the Explorer transfer worker and collision policy.

use gpui::{Context, Window};

use super::{Choice, Deed, Entry, Label, Mark, OpenRoot, PathDeed, Target};
use crate::Root;
use crate::i18n::Text;

/// What only a row of the file tree can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeDeed {
    /// Open the file in a tab.
    Open,
    /// Fold a directory, or unfold it.
    ToggleDirectory,
    /// Make a file in this directory — in the row's own when it is a
    /// directory, in the one holding it when it is a file — the only
    /// rule that is not surprising.
    NewFile,
    /// Make a directory there, by the same rule.
    NewDirectory,
    /// Give the row another name. The open tab follows it.
    Rename,
    /// Move the selected project entries.
    Move,
    /// Arm a session-local cut payload.
    Cut,
    /// Copy selected files and directories.
    Copy,
    /// Paste the owned file payload here.
    Paste,
    /// Move the row to the Trash — never `unlink`. Decision 13.
    Delete,
    /// A terminal tab in this directory.
    Terminal,
    /// Search inside this directory, and nowhere else.
    FindInFiles,
    /// Who wrote each line of this file, and when.
    Annotate,
    /// `git add` on the row: start tracking it.
    AddToVcs,
}

/// What both kinds of row offer for making something new.
fn new_group() -> Entry {
    Entry::Submenu(
        Text::MenuNew,
        vec![
            Entry::Item(Text::MenuNewFile, Deed::Tree(TreeDeed::NewFile)),
            Entry::Item(Text::MenuNewDirectory, Deed::Tree(TreeDeed::NewDirectory)),
        ],
    )
}

/// Where a row can be opened that is not a tab of ours.
///
/// **Not a path handed to another terminal application**: we own the
/// terminal, so this is a tab in the same window at that directory.
fn open_in_group() -> Entry {
    Entry::Submenu(
        Text::MenuOpenIn,
        vec![
            Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal)),
            Entry::Item(Text::MenuOpenTerminal, Deed::Tree(TreeDeed::Terminal)),
        ],
    )
}

/// The two that change the row, the destructive one **inside** the
/// group rather than at the foot of the menu.
fn transform_group() -> [Entry; 6] {
    [
        Entry::Item(Text::MenuMove, Deed::Tree(TreeDeed::Move)),
        Entry::Item(Text::MenuCutFiles, Deed::Tree(TreeDeed::Cut)),
        Entry::Item(Text::MenuCopyFiles, Deed::Tree(TreeDeed::Copy)),
        Entry::Item(Text::MenuPasteFiles, Deed::Tree(TreeDeed::Paste)),
        Entry::Item(Text::MenuRename, Deed::Tree(TreeDeed::Rename)),
        Entry::Item(Text::MenuDelete, Deed::Tree(TreeDeed::Delete)),
    ]
}

/// The transform group of a directory that is, or holds, a root Guild
/// has open — GLD-057: drawn whole, so the menu reads the same, but the
/// four that write the directory itself are drawn and never taken. *Copy*
/// does not write its source and *Paste* writes into the directory, not
/// over it, so both stay. The handlers refuse as well
/// ([`Root::refuse_open_root`]): an item that is merely disabled is one
/// key binding away from being reachable.
fn transform_group_refused() -> [Entry; 6] {
    let refused = |text, deed| {
        Entry::Choice(Choice {
            label: Label::Text(text),
            mark: Mark::None,
            deed: Deed::Tree(deed),
            check: None,
            enabled: false,
            detail: None,
            trailing: None,
        })
    };
    [
        refused(Text::MenuMove, TreeDeed::Move),
        refused(Text::MenuCutFiles, TreeDeed::Cut),
        Entry::Item(Text::MenuCopyFiles, Deed::Tree(TreeDeed::Copy)),
        Entry::Item(Text::MenuPasteFiles, Deed::Tree(TreeDeed::Paste)),
        refused(Text::MenuRename, TreeDeed::Rename),
        refused(Text::MenuDelete, TreeDeed::Delete),
    ]
}

/// What can be copied off any row that stands for a path.
///
/// Paths only. Content root, source root, repository root and a hosting
/// URL are project-model words, and we have no project model to say
/// them with.
fn copy_group() -> [Entry; 2] {
    [
        Entry::Item(Text::MenuCopyPath, Deed::Path(PathDeed::CopyPath)),
        Entry::Item(Text::MenuCopyFullPath, Deed::Path(PathDeed::CopyFullPath)),
    ]
}

/// A file's menu: what can be made here, what the row is for, where to
/// look inside it, what changes it, what git says about it, and what can
/// be copied off it.
pub fn entries_for_file() -> Vec<Entry> {
    let mut items = vec![
        new_group(),
        Entry::Separator,
        Entry::Item(Text::MenuOpen, Deed::Tree(TreeDeed::Open)),
        open_in_group(),
        Entry::Separator,
        Entry::Item(Text::MenuFindInFiles, Deed::Tree(TreeDeed::FindInFiles)),
        Entry::Separator,
    ];
    items.extend(transform_group());
    items.push(Entry::Separator);
    // Second doors onto what M8.2 and M8.3 built, aimed at the row under
    // the pointer rather than at the file in front.
    // `ToolGit` rather than a label of its own: it is the same word for
    // the same thing, and the second copy would be the one that goes
    // untranslated.
    items.push(Entry::Submenu(
        Text::ToolGit,
        vec![
            Entry::Item(Text::MenuShowDiff, Deed::Path(PathDeed::ShowDiff)),
            Entry::Item(Text::MenuAnnotate, Deed::Tree(TreeDeed::Annotate)),
            Entry::Item(Text::MenuAddToVcs, Deed::Tree(TreeDeed::AddToVcs)),
        ],
    ));
    items.push(Entry::Separator);
    items.extend(copy_group());
    items
}

/// A directory has nothing to open and no diff of its own: it folds, and
/// the item says what pressing it will do rather than what the row is.
///
/// *Add to VCS* is flat here rather than in a *Git ▸* of one item: a
/// submenu holding a single line is a press bought for nothing.
///
/// **The project's own row loses the two that write to it.** Its path is
/// the tree's empty one, so *Delete* would move the directory the window
/// has open to the Trash and *Rename…* would rename it — and `root` is
/// how a menu that sees only rows tells that row apart. The refusal
/// itself lives beside the gestures as well (`delete_asking` and
/// `Naming::rename`), because an item that is merely not offered is one
/// keymap away from being reachable again.
///
/// **A directory that is, or holds, another open root keeps its items
/// but cannot take the writes** — GLD-057: they are drawn disabled, and a
/// footnote at the menu's foot says why, in the one line a footnote has.
pub fn entries_for_directory(
    expanded: bool,
    root: bool,
    open_root: Option<OpenRoot>,
) -> Vec<Entry> {
    let mut items = vec![
        new_group(),
        Entry::Separator,
        Entry::Item(
            if expanded {
                Text::MenuCollapse
            } else {
                Text::MenuExpand
            },
            Deed::Tree(TreeDeed::ToggleDirectory),
        ),
        open_in_group(),
        Entry::Separator,
        Entry::Item(Text::MenuFindInFiles, Deed::Tree(TreeDeed::FindInFiles)),
        Entry::Separator,
    ];
    if root {
        items.push(Entry::Item(
            Text::MenuPasteFiles,
            Deed::Tree(TreeDeed::Paste),
        ));
    }
    if !root {
        if open_root.is_some() {
            items.extend(transform_group_refused());
        } else {
            items.extend(transform_group());
        }
        items.push(Entry::Separator);
    }
    items.push(Entry::Item(
        Text::MenuAddToVcs,
        Deed::Tree(TreeDeed::AddToVcs),
    ));
    items.push(Entry::Separator);
    items.extend(copy_group());
    if !root && let Some(open_root) = open_root {
        items.push(Entry::Note(Label::Text(match open_root {
            OpenRoot::Is => Text::MenuOpenAsProject,
            OpenRoot::Holds => Text::MenuHoldsOpenProject,
        })));
    }
    items
}

/// A row under External Libraries: somebody else's source.
///
/// **Nothing here writes.** The rest of the tree's menu makes, renames
/// and deletes; a crate unpacked under `~/.cargo/registry` is not a
/// person's own file, and offering to rename one would offer to break
/// every build on the machine. What is left is what a person actually
/// wants there — open it, find it on the disk, take its path.
pub fn entries_for_library(is_dir: bool, expanded: bool) -> Vec<Entry> {
    vec![
        if is_dir {
            Entry::Item(
                if expanded {
                    Text::MenuCollapse
                } else {
                    Text::MenuExpand
                },
                Deed::Tree(TreeDeed::ToggleDirectory),
            )
        } else {
            Entry::Item(Text::MenuOpen, Deed::Tree(TreeDeed::Open))
        },
        Entry::Separator,
        // The full path only: the short one is relative to a root that
        // is not the project, so it would paste as a lie.
        Entry::Item(Text::MenuCopyFullPath, Deed::Path(PathDeed::CopyFullPath)),
        Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal)),
    ]
}

impl Root {
    /// Carry out a tree deed on the row the pointer was on.
    pub(super) fn run_tree_deed(
        &mut self,
        deed: TreeDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        // A row under a dependency's root reaches only the two deeds its
        // own menu offers, and both mean *that* root — never the
        // project.
        if let Target::LibraryRow {
            at, path, is_dir, ..
        } = target
        {
            let (at, path, is_dir) = (*at, path.clone(), *is_dir);
            match deed {
                TreeDeed::Open => {
                    self.open_file_from(crate::Origin::External(at), &path, window, cx)
                }
                TreeDeed::ToggleDirectory => {
                    // Root and child rows share loading, selection and focus with clicks.
                    self.library_click(at, &path, is_dir, gpui::Modifiers::default(), window, cx);
                }
                _ => {}
            }
            cx.notify();
            return;
        }
        let Target::TreeRow { path, is_dir, .. } = target else {
            return;
        };
        let (path, is_dir) = (path.clone(), *is_dir);
        if !crate::root::tree::writable_row(&path)
            && matches!(
                deed,
                TreeDeed::Rename
                    | TreeDeed::Move
                    | TreeDeed::Cut
                    | TreeDeed::Copy
                    | TreeDeed::Delete
            )
        {
            return;
        }
        if matches!(
            deed,
            TreeDeed::Move
                | TreeDeed::Cut
                | TreeDeed::Copy
                | TreeDeed::Paste
                | TreeDeed::Delete
                | TreeDeed::Rename
        ) {
            self.select_operation_target(&path);
        }
        match deed {
            TreeDeed::Move => self.move_files(window, cx),
            TreeDeed::Cut => self.file_clipboard(true, cx),
            TreeDeed::Copy => self.file_clipboard(false, cx),
            TreeDeed::Paste => self.paste_files(window, cx),
            TreeDeed::Open => self.open_file(&path, window, cx),
            TreeDeed::ToggleDirectory => {
                self.tree_click(&path, &path, true, gpui::Modifiers::default(), window, cx)
            }
            TreeDeed::NewFile => {
                self.start_naming(crate::root::tree::Naming::file(&path, is_dir), window, cx);
            }
            TreeDeed::NewDirectory => {
                self.start_naming(
                    crate::root::tree::Naming::directory(&path, is_dir),
                    window,
                    cx,
                );
            }
            TreeDeed::Rename => {
                if self.operation_sources().len() > 1 {
                    return;
                }
                if let Some(naming) = crate::root::tree::Naming::rename(&path) {
                    self.start_naming(naming, window, cx);
                }
            }
            TreeDeed::Delete => self.delete_asking(path, window, cx),
            TreeDeed::Terminal => self.open_terminal_at(&path, is_dir, cx),
            TreeDeed::FindInFiles => self.find_in_directory(&path, is_dir, window, cx),
            TreeDeed::Annotate => {
                if let Some(project) = self.active_project {
                    self.open_annotate(project, path, window, cx);
                }
            }
            TreeDeed::AddToVcs => self.stage_path(path),
        }
        cx.notify();
    }
}
