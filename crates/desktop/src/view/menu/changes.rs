//! The Commit panel's half of the right-button menu.
//!
//! A changed file, or a section head. The two diffs that are only
//! reachable from here — against the index, and the index against the
//! disk — have had no other door since M8.3.1 took the panel's three
//! groups away.
//!
//! **Since M8.8.4 three of its items change something**: *Rollback…*,
//! *Delete* and *Add to VCS*. The two that can
//! lose work ask first; the third is `git add`, the write the panel has
//! made since M8.3 under a different name.

use gpui::{Context, Window};

use super::{Deed, Entry, PathDeed, Target};
use crate::Root;
use crate::i18n::{CommitLabel, Text, Ui};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

/// What only a row or a head of the Commit panel can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeDeed {
    /// The selected group against HEAD, shared with ShowSelectedDiff.
    ShowDiff,
    /// The diff of what is staged: `HEAD` against the index.
    ShowDiffStaged,
    /// The diff of what is not staged: the index against the disk.
    ShowDiffUnstaged,
    /// Put the row back to what `HEAD` holds — Rollback, and the
    /// first item in any menu of ours that changes the working tree.
    ///
    /// **It asks first, and the sheet names the file.** git will not
    /// argue about a `restore`, so the window must: every other write
    /// this crate makes can be refused by git or undone with a command
    /// a person knows, and this one throws away text that was never
    /// committed. `changes::rollback_of` decides what it actually
    /// becomes; M8.8.4.
    Rollback,
    /// Move the file to the Trash — decision 13's route, shared with
    /// the file tree's own item. Never `unlink`.
    Delete,
    /// `git add` on a file git has never seen, under the common IDE
    /// name for it. The same write a tick and a commit already make; what
    /// is new is that it is reachable without ticking anything.
    AddToVcs,
    /// Put this change into the commit, or take it out.
    ToggleTick,
    Include,
    Exclude,
    CopyPaths,
    CopyFullPaths,
    ToggleDirectory,
    Unavailable,
    /// Every change into the commit.
    TickAll,
    /// None of them.
    UntickAll,
    /// Fold both sections of the panel.
    CollapseSections,
    /// Unfold them.
    ExpandSections,
}

/// A changed file's menu.
///
/// A directory git did not descend into has no diff to show, and a
/// conflict is not a thing a commit can take — so neither is offered
/// what it could not honour.
///
/// **The two git items point opposite ways over `unversioned`.** *Add
/// to VCS* is for a file git has never seen and nothing else; *Rollback*
/// is for a file git has, because an unversioned row has nothing in
/// `HEAD` or in the index to go back to and git would refuse the path.
/// Offering either where it cannot work would be a menu item that opens
/// a sheet whose only ending is a refusal — which is the *Branch: HEAD*
/// shape again, a state a person can reach and not leave.
///
/// **And *Rollback* goes on a conflicted row as well**, for the opposite
/// reason: git would not refuse it. Restoring an unmerged path succeeds
/// and takes the path out of the conflict in `HEAD`'s favour, leaving
/// the merge standing with the incoming side gone — which is not what
/// the word says, so the guard is ours rather than git's.
/// `changes::rollback_of` refuses it too; this is the half a person
/// sees.
///
/// *Delete* is offered on every row, an untracked directory included —
/// what it does is move the file to the Trash, which is true of a
/// tracked file exactly as much as of an unversioned one — **except a
/// row git marks `D`**, whose file is by definition no longer there:
/// the item would open a warning sheet whose only ending is the
/// platform refusing a path that does not exist.
pub fn entries_for_change(
    ticked: bool,
    unmerged: bool,
    is_dir: bool,
    unversioned: bool,
    gone: bool,
) -> Vec<Entry> {
    let mut entries = Vec::new();
    if !is_dir {
        entries.push(Entry::Item(
            Text::MenuShowDiff,
            Deed::Path(PathDeed::ShowDiff),
        ));
        entries.push(Entry::Item(
            Text::MenuDiffStaged,
            Deed::Change(ChangeDeed::ShowDiffStaged),
        ));
        entries.push(Entry::Item(
            Text::MenuDiffUnstaged,
            Deed::Change(ChangeDeed::ShowDiffUnstaged),
        ));
        entries.push(Entry::Separator);
    }
    if !unmerged {
        entries.push(Entry::Item(
            if ticked {
                Text::MenuExclude
            } else {
                Text::MenuInclude
            },
            Deed::Change(ChangeDeed::ToggleTick),
        ));
        entries.push(Entry::Separator);
    }
    if unversioned {
        entries.push(Entry::Item(
            Text::MenuAddToVcs,
            Deed::Change(ChangeDeed::AddToVcs),
        ));
    } else if !unmerged {
        entries.push(Entry::Item(
            Text::MenuRollback,
            Deed::Change(ChangeDeed::Rollback),
        ));
    }
    if !gone {
        entries.push(Entry::Item(
            Text::MenuDelete,
            Deed::Change(ChangeDeed::Delete),
        ));
    }
    entries.push(Entry::Separator);
    entries.push(Entry::Item(
        Text::MenuCopyPath,
        Deed::Path(PathDeed::CopyPath),
    ));
    entries.push(Entry::Item(
        Text::MenuCopyFullPath,
        Deed::Path(PathDeed::CopyFullPath),
    ));
    entries.push(Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal)));
    entries
}

/// A section head is about the whole panel: the four things its toolbar
/// does, where the hand already is.
pub fn entries_for_section() -> Vec<Entry> {
    vec![
        Entry::Item(Text::TickAll, Deed::Change(ChangeDeed::TickAll)),
        Entry::Item(Text::UntickAll, Deed::Change(ChangeDeed::UntickAll)),
        Entry::Separator,
        Entry::Item(
            Text::CollapseAll,
            Deed::Change(ChangeDeed::CollapseSections),
        ),
        Entry::Item(Text::ExpandAll, Deed::Change(ChangeDeed::ExpandSections)),
    ]
}

/// The production group menu; shared directly with model acceptance tests.
#[allow(clippy::too_many_arguments)]
pub fn entries_for_selection(
    list: &[crate::changes::Listed],
    directories: &HashMap<PathBuf, Result<crate::changes::Directory, String>>,
    selected: &[PathBuf],
    ticked: &HashSet<PathBuf>,
    partial: &HashSet<PathBuf>,
    expanded: &HashSet<PathBuf>,
    ui: Ui,
    space_available: bool,
) -> Vec<Entry> {
    let n = selected.len();
    if n == 0 {
        return Vec::new();
    }
    let item = |label, deed| Entry::NamedItem(ui.commit_menu_label(label, n), Deed::Change(deed));
    let unavailable = |label, detail| {
        Entry::Choice(super::Choice {
            label: super::Label::Text(label),
            mark: super::Mark::None,
            deed: Deed::Change(ChangeDeed::Unavailable),
            check: None,
            enabled: false,
            detail: Some(super::Label::Text(detail)),
            trailing: None,
        })
    };
    let dirs = selected
        .iter()
        .filter(|p| list.iter().any(|l| &l.row.path == *p && l.row.is_dir))
        .count();
    let mut entries = Vec::new();
    if dirs == 0 {
        entries.push(Entry::NamedItem(
            ui.commit_menu_label(CommitLabel::Diff, n),
            Deed::Change(ChangeDeed::ShowDiff),
        ));
    } else if n == 1 {
        entries.push(Entry::Item(
            if expanded.contains(&selected[0]) {
                Text::CommitCollapseDirectory
            } else {
                Text::CommitExpandDirectory
            },
            Deed::Change(ChangeDeed::ToggleDirectory),
        ));
    } else {
        entries.push(unavailable(
            Text::CommitDiffUnavailable,
            Text::CommitDiffDirectories,
        ));
    }
    if n == 1 && dirs == 0 {
        entries.push(Entry::Item(
            Text::MenuDiffStaged,
            Deed::Change(ChangeDeed::ShowDiffStaged),
        ));
        entries.push(Entry::Item(
            Text::MenuDiffUnstaged,
            Deed::Change(ChangeDeed::ShowDiffUnstaged),
        ));
    }
    entries.push(Entry::Separator);
    let targets = crate::changes::eligible_targets(list, directories, selected);
    match &targets {
        Ok(paths) => {
            if paths.iter().any(|p| !ticked.contains(p)) {
                entries.push(item(CommitLabel::Include, ChangeDeed::Include));
            }
            // A partial row is in the commit too: it can be taken out, and
            // Include above takes it whole. Review r3 `bugs+impl-7`.
            if paths
                .iter()
                .any(|p| ticked.contains(p) || partial.contains(p))
            {
                entries.push(item(CommitLabel::Exclude, ChangeDeed::Exclude));
            }
            if selected.iter().all(|p| {
                list.iter()
                    .any(|l| &l.row.path == p && l.section == crate::changes::Section::Unversioned)
            }) {
                entries.push(Entry::NamedItem(
                    ui.commit_menu_label(CommitLabel::Add, paths.len()),
                    Deed::Change(ChangeDeed::AddToVcs),
                ));
            }
        }
        Err(error) => entries.push(Entry::Choice(super::Choice {
            label: super::Label::Text(Text::CommitInclusionUnavailable),
            mark: super::Mark::None,
            deed: Deed::Change(ChangeDeed::Unavailable),
            check: None,
            enabled: false,
            detail: Some(super::Label::Named(ui.commit_eligibility(error))),
            trailing: None,
        })),
    }
    if selected
        .iter()
        .all(|p| crate::changes::rollback_of(list, p).is_some())
    {
        entries.push(item(CommitLabel::Rollback, ChangeDeed::Rollback));
    } else if selected
        .iter()
        .any(|p| crate::changes::rollback_of(list, p).is_some())
    {
        entries.push(unavailable(
            Text::CommitRollbackUnavailable,
            Text::CommitRollbackMixed,
        ));
    }
    let untracked = selected
        .iter()
        .filter(|p| {
            list.iter()
                .any(|l| &l.row.path == *p && l.section == crate::changes::Section::Unversioned)
        })
        .count();
    if untracked > 0 && untracked < n {
        entries.push(unavailable(
            Text::CommitAddUnavailable,
            Text::CommitAddMixed,
        ));
    }
    if selected.iter().any(|p| !crate::root::tree::writable_row(p)) {
        entries.push(unavailable(
            Text::CommitTrashUnavailable,
            Text::CommitTrashRoot,
        ));
    } else if selected.iter().all(|p| {
        list.iter()
            .any(|l| &l.row.path == p && l.row.mark != crate::theme::Mark::Deleted)
    }) {
        entries.push(item(CommitLabel::Trash, ChangeDeed::Delete));
    } else {
        entries.push(unavailable(
            Text::CommitTrashUnavailable,
            Text::CommitTrashMissing,
        ));
    }
    entries.push(Entry::Separator);
    entries.push(item(CommitLabel::CopyPaths, ChangeDeed::CopyPaths));
    entries.push(item(CommitLabel::CopyFullPaths, ChangeDeed::CopyFullPaths));
    if n == 1 {
        entries.push(Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal)));
    }
    if space_available && targets.as_ref().is_ok_and(|p| !p.is_empty()) {
        entries.push(Entry::Note(super::Label::Text(Text::CommitSpaceHint)));
    }
    entries
}

/// A card owns both its action order and its intended group until it closes.
pub(super) struct Snapshot {
    pub entries: Vec<Entry>,
    project: Option<guild_session::ProjectId>,
    selected: Vec<PathBuf>,
    rows: Vec<crate::changes::Listed>,
    eligible: Result<Vec<PathBuf>, crate::changes::EligibilityError>,
}
impl Snapshot {
    pub fn capture(root: &Root, entries: Vec<Entry>) -> Option<Self> {
        let state = root.changes()?;
        Some(Self {
            entries,
            project: root.active_project,
            selected: state.selection.selected.clone(),
            rows: state
                .list
                .iter()
                .filter(|l| state.selection.selected.contains(&l.row.path))
                .cloned()
                .collect(),
            eligible: crate::changes::eligible_targets(
                &state.list,
                &state.directories,
                &state.selection.selected,
            ),
        })
    }
    pub fn valid(&self, root: &Root, deed: Deed) -> bool {
        let Some(state) = root.changes() else {
            return false;
        };
        self.project == root.active_project
            && self.selected == state.selection.selected
            && self.rows.len() == self.selected.len()
            && self.rows.iter().all(|row| state.list.contains(row))
            && (!matches!(
                deed,
                Deed::Change(ChangeDeed::Include | ChangeDeed::Exclude | ChangeDeed::AddToVcs)
            ) || self.eligible
                == crate::changes::eligible_targets(
                    &state.list,
                    &state.directories,
                    &self.selected,
                ))
    }
}

impl Root {
    pub(super) fn changes_selection_entries(&self) -> Vec<Entry> {
        let Some(state) = self.changes() else {
            return Vec::new();
        };
        let space_available = !crate::actions::effective().iter().any(|(stroke, _)| {
            gpui::Keystroke::parse(stroke)
                .is_ok_and(|key| key.key == "space" && key.modifiers == gpui::Modifiers::default())
        });
        entries_for_selection(
            &state.list,
            &state.directories,
            &state.selection.selected,
            &state.ticked,
            &state.partial,
            &state.expanded,
            self.ui(),
            space_available,
        )
    }
    /// Carry out a Commit-panel deed on the selection established by context click.
    pub(super) fn run_change_deed(
        &mut self,
        deed: ChangeDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        match deed {
            ChangeDeed::ShowDiff => self.show_selected_diff(window, cx),
            ChangeDeed::Unavailable => {}
            ChangeDeed::ShowDiffStaged => {
                self.open_diff(
                    target.path().cloned(),
                    guild_session::Base::Index,
                    window,
                    cx,
                );
            }
            ChangeDeed::ShowDiffUnstaged => {
                self.open_diff(
                    target.path().cloned(),
                    guild_session::Base::Unstaged,
                    window,
                    cx,
                );
            }
            // Context click establishes the same group keyboard actions use.
            ChangeDeed::Rollback => {
                self.rollback_selected(window, cx);
            }
            ChangeDeed::Delete => {
                self.changes_trash_selected(window, cx);
            }
            ChangeDeed::AddToVcs => {
                self.add_to_vcs_selected(cx);
            }
            ChangeDeed::Include => self.changes_include_selected(Some(true), cx),
            ChangeDeed::Exclude => self.changes_include_selected(Some(false), cx),
            ChangeDeed::CopyPaths => self.run_path_deed(PathDeed::CopyPath, target, window, cx),
            ChangeDeed::CopyFullPaths => {
                self.run_path_deed(PathDeed::CopyFullPath, target, window, cx)
            }
            ChangeDeed::ToggleDirectory => {
                if let Some(state) = self.changes_mut()
                    && let Some(path) = state.selection.selected.first().cloned()
                {
                    if !state.expanded.remove(&path) {
                        state.expanded.insert(path);
                    }
                    state.settle_selection();
                }
                self.reveal_changes_selection();
                cx.notify();
            }
            ChangeDeed::ToggleTick => {
                if let Target::Change { path, .. } = target {
                    self.changes_toggle_row(&path.clone(), cx);
                }
            }
            ChangeDeed::TickAll => self.tick_all(true, cx),
            ChangeDeed::UntickAll => self.tick_all(false, cx),
            ChangeDeed::CollapseSections => self.fold_all(true, cx),
            ChangeDeed::ExpandSections => self.fold_all(false, cx),
        }
    }
}
