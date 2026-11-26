//! The column's half of the right-button menu — M12.
//!
//! **A worktree's row has one way out** — the
//! person, 2026-09-22. *Remove Worktree…* is git's: the tabs, the watcher
//! and the terminals go, then `git worktree remove` takes the checkout off
//! the disk, after a sheet that names what goes. Until then a *Close
//! Worktree* sat beside it, the window's gesture that left the folder, and
//! the two read as twins. A root whose checkout was already removed from
//! outside is closed by the same item, with the questions closing a
//! project asks — there is nothing left for git to remove — and when it
//! holds nothing to ask about the window closes it by itself
//! (`column::removed_outside`).
//!
//! **The branch is never touched.** `git worktree remove` leaves it where
//! it is, which is what makes the gesture as reversible as it is — `git
//! worktree add` brings the same branch back. Removing the branch as well
//! would need `branch -d`, a write of its own with its own refusal, and
//! the sheet says as much rather than leaving a person to guess what went.

use gpui::{Context, Window};

use super::{Deed, Entry, Target};
use crate::Root;
use crate::i18n::Text;

/// What a project — a repository's group in the column — offers: M12.2,
/// the project menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectDeed {
    /// Call it something of the person's own.
    Rename,
    /// Make a worktree of it, from the `HEAD` of one of its open roots.
    NewWorktree,
    /// Its folder on the clipboard.
    CopyPath,
    /// Ask the platform to show its folder.
    Reveal,
    /// Move the repository and all of its open worktrees to another workspace.
    MoveTo(guild_session::WorkspaceId),
    /// Put the repository into one of this workspace's project groups.
    MoveToGroup(guild_session::project_group::ProjectGroupId),
    /// Ask for a name, and make a project group with the repository in it.
    NewGroup,
    /// Take the repository out of its project group.
    RemoveFromGroup,
    /// Take it out of the window: every root of it closes, and nothing on
    /// the disk is touched.
    Remove,
}

/// What a project group's header offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectGroupDeed {
    /// A session in the group's folder — GLD-055; asked for the folder
    /// first when there is none.
    NewSession,
    /// Bind the group to a folder, or change the one it is bound to.
    BindFolder,
    /// Call it something else.
    Rename,
    /// Delete it: its repositories return to the ungrouped list, and its
    /// folder's root is asked to close; nothing on disk is touched.
    Delete,
    /// The header's `+`: put the project at this place of the target's
    /// choices into the group.
    Add(usize),
}

/// One project the header's `+` offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddChoice {
    /// Its key.
    pub key: std::path::PathBuf,
    /// What the column calls it.
    pub name: String,
    /// The group it is in now, by name; `None` when it is in none.
    pub group: Option<String>,
}

/// A project group header's menu — GLD-055 decision 15: a session in its
/// folder and the folder's binding, then Rename and Delete.
pub fn entries_for_project_group(bound: bool) -> Vec<Entry> {
    vec![
        Entry::Item(
            if bound {
                Text::MenuGroupNewSession
            } else {
                Text::MenuGroupNewSessionAsk
            },
            Deed::ProjectGroup(ProjectGroupDeed::NewSession),
        ),
        Entry::Item(
            if bound {
                Text::MenuGroupChangeFolder
            } else {
                Text::MenuGroupBindFolder
            },
            Deed::ProjectGroup(ProjectGroupDeed::BindFolder),
        ),
        Entry::Separator,
        Entry::Item(
            Text::MenuRenameProjectGroup,
            Deed::ProjectGroup(ProjectGroupDeed::Rename),
        ),
        Entry::Separator,
        Entry::Item(
            Text::MenuDeleteProjectGroup,
            Deed::ProjectGroup(ProjectGroupDeed::Delete),
        ),
    ]
}

/// The header's `+` — decision 16: each project not in the group, with its
/// letter tile, and the group it is in now beside it.
pub fn add_project_entries(
    choices: &[AddChoice],
    member_of: impl Fn(&str) -> String,
) -> Vec<Entry> {
    choices
        .iter()
        .enumerate()
        .map(|(at, choice)| {
            Entry::Choice(super::Choice {
                label: super::Label::Named(choice.name.clone()),
                mark: super::Mark::Tile(choice.name.clone()),
                deed: Deed::ProjectGroup(ProjectGroupDeed::Add(at)),
                check: None,
                enabled: true,
                detail: choice
                    .group
                    .as_deref()
                    .map(|group| super::Label::Named(member_of(group))),
                trailing: None,
            })
        })
        .collect()
}

/// *Move to Group ▸*: the workspace's groups in the order they were made,
/// the one the repository is in checked and not offered; then *New Group…*,
/// and *Remove from Group* when it is in one.
pub fn move_to_group_entries(
    groups: &[(guild_session::project_group::ProjectGroupId, String)],
    current: Option<guild_session::project_group::ProjectGroupId>,
) -> Vec<Entry> {
    let mut entries: Vec<Entry> = groups
        .iter()
        .map(|(id, name)| {
            let here = current == Some(*id);
            Entry::Choice(super::Choice {
                label: super::Label::Named(name.clone()),
                mark: super::Mark::None,
                deed: Deed::Project(ProjectDeed::MoveToGroup(*id)),
                check: Some(here),
                enabled: !here,
                detail: None,
                trailing: None,
            })
        })
        .collect();
    if !entries.is_empty() {
        entries.push(Entry::Separator);
    }
    entries.push(Entry::Item(
        Text::MenuNewProjectGroup,
        Deed::Project(ProjectDeed::NewGroup),
    ));
    if current.is_some() {
        entries.push(Entry::Item(
            Text::MenuRemoveFromProjectGroup,
            Deed::Project(ProjectDeed::RemoveFromGroup),
        ));
    }
    entries
}

/// What only the column's rows can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeDeed {
    /// Run this worktree's own check, as `cmd-r` does for the root in
    /// front.
    Run,
    /// A shell in this worktree's folder, in a tab of its own.
    Terminal,
    /// Its folder on the clipboard.
    CopyPath,
    /// Ask the platform to show the folder.
    Reveal,
    /// Take git's lock off it, so that it can be removed at all.
    Unlock,
    /// Take the checkout off the disk, asking first — or, when it was
    /// already taken off from outside, stop holding the root open.
    Remove,
    /// Open its branch's merge request in the browser — GLD-063.
    ViewMr,
    /// Its branch's merge request's URL on the clipboard — GLD-063.
    CopyMrUrl,
}

/// A project's menu — M12.2 task 9: what it is called,
/// a worktree of it, where it is, and taking it out of the window.
///
/// **M12 had no *Close Project* here**, on the reasoning that closing every
/// root of a repository is one question per root, and the window asks one
/// question at a time. The person asked for it, and *Remove Project* asks
/// one question for all of them — what it will stop, named in the sheet.
///
/// Without an open root there is no `HEAD` to start a worktree from, so
/// that item goes.
pub fn entries_for_repository(opened: bool) -> Vec<Entry> {
    let mut entries = vec![Entry::Item(
        Text::MenuRenameProject,
        Deed::Project(ProjectDeed::Rename),
    )];
    if opened {
        entries.push(Entry::Item(
            Text::ColumnNewWorktree,
            Deed::Project(ProjectDeed::NewWorktree),
        ));
    }
    entries.push(Entry::Separator);
    entries.push(Entry::Item(
        Text::MenuCopyPath,
        Deed::Project(ProjectDeed::CopyPath),
    ));
    entries.push(Entry::Item(
        Text::MenuReveal,
        Deed::Project(ProjectDeed::Reveal),
    ));
    entries.push(Entry::Separator);
    entries.push(Entry::Item(
        Text::MenuRemoveProject,
        Deed::Project(ProjectDeed::Remove),
    ));
    entries
}

/// A worktree's menu.
///
/// Its items turn on facts the row already carries. The repository's own
/// checkout is not removable — `column::removal` answers `Primary` and git
/// refuses it in its own words — so the item is not offered rather than
/// offered and refused; the repository leaves the window by *Remove
/// Project*.
pub fn entries_for_worktree(primary: bool, opened: bool, locked: bool, mr: bool) -> Vec<Entry> {
    let mut entries = Vec::new();
    // **What a person came to the row to do, first.** A worktree is a
    // place work happens, not a label: running its check and opening a
    // shell in it are the two things asked for by name, and both need a
    // root — a worktree git lists that nobody has opened has no watcher,
    // no git thread and no runner behind it, so it offers neither.
    if opened {
        entries.push(Entry::Item(Text::MenuRun, Deed::Column(WorktreeDeed::Run)));
        entries.push(Entry::Item(
            Text::HintNewTerminal,
            Deed::Column(WorktreeDeed::Terminal),
        ));
        entries.push(Entry::Separator);
    }
    // Its branch's merge request, a group of its own after the work —
    // GLD-063. The MR is the branch's, so an unopened row offers it too.
    if mr {
        entries.push(Entry::Item(
            Text::MenuViewMr,
            Deed::Column(WorktreeDeed::ViewMr),
        ));
        entries.push(Entry::Item(
            Text::MenuCopyMrUrl,
            Deed::Column(WorktreeDeed::CopyMrUrl),
        ));
        entries.push(Entry::Separator);
    }
    // The folder itself, which every row has whether it is open or not.
    entries.push(Entry::Item(
        Text::MenuCopyPath,
        Deed::Column(WorktreeDeed::CopyPath),
    ));
    entries.push(Entry::Item(
        Text::MenuReveal,
        Deed::Column(WorktreeDeed::Reveal),
    ));
    if !primary || locked {
        entries.push(Entry::Separator);
    }
    // **Only where there is a lock to take off.** git refuses a locked
    // worktree before anything else, and the lock outlives whoever took it
    // — a dead session's note otherwise refuses every removal for ever,
    // with `remove -f -f` as git's own advice and this repository's one
    // barred spelling. M12 task 18.
    if locked {
        entries.push(Entry::Item(
            Text::MenuUnlockWorktree,
            Deed::Column(WorktreeDeed::Unlock),
        ));
    }
    if !primary {
        entries.push(Entry::Item(
            Text::MenuRemoveWorktree,
            Deed::Column(WorktreeDeed::Remove),
        ));
    }
    entries
}

impl Root {
    /// Resolve only for an explicit menu interaction, never while drawing a header.
    /// Rechecking at the deed also covers a checkout disappearing while the menu is up.
    pub(super) fn repository_menu_folder(&self, target: &Target) -> std::path::PathBuf {
        let Target::Repository {
            key,
            projects,
            folder,
            ..
        } = target
        else {
            unreachable!("repository menu target")
        };
        crate::column::main_root(key)
            .filter(|main| main.is_dir())
            .or_else(|| {
                projects
                    .iter()
                    .filter_map(|id| self.desk.project(*id))
                    .find(|project| project.path.is_dir())
                    .map(|project| project.path.clone())
            })
            // If every root is gone, retain the old best-known path behavior.
            .or_else(|| {
                projects
                    .iter()
                    .find_map(|id| self.desk.project(*id).map(|p| p.path.clone()))
            })
            .unwrap_or_else(|| folder.clone())
    }
    /// Carry out a project deed on the head the pointer was on — M12.2.
    /// The title bar's project popup had two deeds of its own until
    /// M12.10 took its trigger out of the bar.
    pub(super) fn run_project_deed(
        &mut self,
        deed: ProjectDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Target::Repository {
            workspace,
            projects,
            key,
            name,
            ..
        } = target
        else {
            return;
        };
        match deed {
            ProjectDeed::Rename => {
                self.start_renaming_project(key.clone(), name.clone(), window, cx)
            }
            ProjectDeed::NewWorktree => {
                if let Some(project) = projects.first() {
                    self.new_worktree_here(*project, window, cx);
                }
            }
            ProjectDeed::CopyPath => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    self.repository_menu_folder(target).display().to_string(),
                ));
            }
            ProjectDeed::Reveal => cx.reveal_path(&self.repository_menu_folder(target)),
            ProjectDeed::MoveTo(destination) => {
                self.move_repository_to_workspace(*workspace, destination, projects, cx)
            }
            ProjectDeed::MoveToGroup(group) => {
                self.move_repository_to_group(*workspace, key, Some(group), cx)
            }
            ProjectDeed::NewGroup => self.open_new_project_group(
                *workspace,
                key.clone(),
                window.mouse_position(),
                window,
                cx,
            ),
            ProjectDeed::RemoveFromGroup => {
                self.move_repository_to_group(*workspace, key, None, cx)
            }
            ProjectDeed::Remove => {
                self.remove_project_asking(*workspace, key, name.clone(), window, cx)
            }
        }
    }

    /// Carry out a project group header's deed.
    pub(super) fn run_project_group_deed(
        &mut self,
        deed: ProjectGroupDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        if let (
            ProjectGroupDeed::Add(at),
            Target::ProjectGroupAdd {
                workspace,
                id,
                choices,
            },
        ) = (deed, target)
        {
            if let Some(choice) = choices.get(at) {
                self.add_project_to_group(*workspace, &choice.key, *id, cx);
            }
            return;
        }
        let Target::ProjectGroup { workspace, id } = target else {
            return;
        };
        let at = window.mouse_position();
        match deed {
            ProjectGroupDeed::NewSession => self.new_group_session(*workspace, *id, at, window, cx),
            ProjectGroupDeed::BindFolder => {
                self.open_bind_group_folder(*workspace, *id, false, at, window, cx)
            }
            ProjectGroupDeed::Rename => {
                self.open_rename_project_group(*workspace, *id, at, window, cx)
            }
            ProjectGroupDeed::Delete => self.delete_project_group(*workspace, *id, window, cx),
            ProjectGroupDeed::Add(_) => {}
        }
    }

    /// Carry out a worktree deed on the row the pointer was on.
    pub(super) fn run_worktree_deed(
        &mut self,
        deed: WorktreeDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        match (deed, target) {
            (WorktreeDeed::CopyPath, Target::Worktree { path, .. }) => {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(path.display().to_string()));
            }
            (WorktreeDeed::Reveal, Target::Worktree { path, .. }) => {
                cx.reveal_path(path);
            }
            // **The root comes to the front first.** Both of these act on
            // the project in front — the runner is the project's and a new
            // tab opens in it — so a deed aimed at another row would
            // otherwise run in the wrong worktree, which is the whole
            // reason a menu item carries its target.
            (
                WorktreeDeed::Terminal,
                Target::Worktree {
                    project: Some(project),
                    ..
                },
            ) => {
                if self.active_project != Some(*project) {
                    self.select_project(*project, cx);
                }
                self.open_tab(*project, guild_session::Runs::Shell, cx);
            }
            (
                WorktreeDeed::Run,
                Target::Worktree {
                    project: Some(project),
                    ..
                },
            ) => {
                if self.active_project != Some(*project) {
                    self.select_project(*project, cx);
                }
                self.run_or_choose(window, cx);
            }
            // The URL the row shows now, not the one it showed when it was
            // pressed: its destination may have moved since (GLD-063, code
            // review r1, bugs+impl-6).
            (WorktreeDeed::ViewMr, Target::Worktree { path, .. }) => {
                if let Some(url) = self.forge_url(path) {
                    cx.open_url(&url);
                }
            }
            (WorktreeDeed::CopyMrUrl, Target::Worktree { path, .. }) => {
                if let Some(url) = self.forge_url(path) {
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(url));
                }
            }
            (WorktreeDeed::Unlock, Target::Worktree { path, .. }) => {
                if let Some(runner) = self.removal_runner(path) {
                    self.run_git_write(runner, crate::git::Write::WorktreeUnlock(path.clone()));
                }
            }
            (
                WorktreeDeed::Remove,
                Target::Worktree {
                    path,
                    project,
                    primary,
                    ..
                },
            ) => {
                self.remove_worktree_asking(path.clone(), *project, *primary, window, cx);
            }
            _ => {}
        }
    }
}

/// Four mutually exclusive repository sorting choices using Guild's shared menu.
pub fn sort_entries(selected: guild_session::project_sort::Mode) -> Vec<Entry> {
    use guild_session::project_sort::Mode;
    Mode::ALL
        .into_iter()
        .map(|mode| {
            Entry::Choice(super::Choice {
                label: super::Label::Text(match mode {
                    Mode::RecentActivity => Text::ProjectSortRecent,
                    Mode::MostActivity => Text::ProjectSortMost,
                    Mode::LastUserMessage => Text::ProjectSortUser,
                    Mode::Manual => Text::ProjectSortManual,
                }),
                mark: super::Mark::None,
                deed: Deed::ProjectSort(mode),
                check: Some(mode == selected),
                enabled: true,
                detail: None,
                trailing: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    use gpui::{AppContext, TestAppContext};

    #[gpui::test]
    fn repository_folder_actions_follow_missing_and_returning_main(cx: &mut TestAppContext) {
        let dir = tempfile::tempdir().unwrap();
        let main = dir.path().join("main");
        let sibling = dir.path().join("sibling");
        std::fs::create_dir(&main).unwrap();
        std::fs::create_dir(&sibling).unwrap();
        let mut desk = guild_session::Desk::default();
        let workspace = desk.active_workspace();
        let main_project = desk.open_in(workspace, &main);
        let project = desk.open_in(workspace, &sibling);
        let root = cx.new(|cx| Root::diagnostic(desk, dir.path().into(), cx));
        let window = cx.add_window(|_, _| gpui::Empty);
        let mut target = Target::Repository {
            workspace,
            projects: vec![main_project, project],
            key: main.join(".git"),
            folder: main.clone(),
            name: "fixture".into(),
        };
        // Main stays first in Desk even while a buffer/session retains its missing root.
        for (main_exists, sibling_exists, expected) in [
            (true, true, &main),
            (false, true, &sibling),
            (true, true, &main),
            (false, false, &main),
        ] {
            for (path, exists) in [(&main, main_exists), (&sibling, sibling_exists)] {
                if exists {
                    std::fs::create_dir_all(path).unwrap();
                } else if path.exists() {
                    std::fs::remove_dir(path).unwrap();
                }
            }
            window
                .update(cx, |_, window, cx| {
                    root.update(cx, |root, cx| {
                        root.run_project_deed(ProjectDeed::CopyPath, &target, window, cx)
                    });
                    assert_eq!(
                        cx.read_from_clipboard().unwrap().text().unwrap(),
                        expected.display().to_string()
                    );
                })
                .unwrap();
            // The opposite stale target: a fallback captured before main returned.
            if let Target::Repository { folder, .. } = &mut target {
                *folder = sibling.clone();
            }
        }
        // With no retained roots either, preserve the captured target as before.
        if let Target::Repository { projects, .. } = &mut target {
            projects.clear();
        }
        root.read_with(cx, |root, _| {
            assert_eq!(root.repository_menu_folder(&target), sibling);
        });
    }
}
