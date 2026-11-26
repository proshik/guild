//! The Changes panel: what the window does with it.
//!
//! The model is `crate::changes`, and it does not know about windows.
//! This is the other half — which project's list, what is ticked and
//! what is selected in it, where the list is scrolled to, who has the
//! focus — and the answers to a click, to a checkbox, and to the keys.
//! The ticks are the panel's own — decision 4 of M8.3.1:
//! nothing is written to git until the commit, which brings the index
//! to the ticks first.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{
    Context, Entity, FocusHandle, KeyDownEvent, ScrollStrategy, Subscription,
    UniformListScrollHandle, Window,
};
use gpui_component::input::{InputEvent, TextareaState};
use guild_session::{Base, ProjectId};

use crate::autosave::Trigger;
use crate::changes::{Listed, Section, can_commit, commit_plan, list, rows_in_order};
use crate::git::Write;
use crate::i18n::Text;
use crate::{Body, Root};

/// One project's changes as the window holds them.
pub(crate) struct ChangesState {
    /// The list, rebuilt each time a map lands.
    pub list: Vec<Listed>,
    /// What goes into the commit: a tracked change is ticked when it
    /// arrives, an unversioned file is not, and a tick is the panel's
    /// until the commit.
    pub ticked: HashSet<PathBuf>,
    /// Rows committed as their index stands: staged and unstaged at once
    /// — GLD-026. Never also in `ticked`.
    pub partial: HashSet<PathBuf>,
    /// The selected row, by section and path so that it survives the
    /// list changing shape under it.
    pub selected: Option<(Section, PathBuf)>,
    pub selection: crate::file_selection::FileSelection,
    pub directories: std::collections::HashMap<PathBuf, Result<crate::changes::Directory, String>>,
    pub expanded: HashSet<PathBuf>,
    discovery: Option<gpui::Task<()>>,
    pub speed: Option<crate::speed_search::Search<PathBuf>>,
    /// The sections folded by their chevron: the head stays, the rows go.
    pub folded: HashSet<Section>,
    /// Where the list is scrolled to.
    pub scroll: UniformListScrollHandle,
    /// The panel's focus: with it the keys are the panel's.
    pub focus: FocusHandle,
    /// The message box above the groups — the editor component in plain
    /// mode, decision 7 — made at the first render with a window, and
    /// its listener for `cmd-enter`.
    pub message: Option<Entity<TextareaState>>,
    message_listener: Option<Subscription>,
    /// A commit landed without a window in hand: the box is cleared at
    /// the next render, which has one.
    clear_pending: bool,
}

impl Root {
    /// A project's list, rebuilt from the map that just landed. A change
    /// that was not listed before arrives ticked, and so does one staged
    /// outside the panel since the last map (`changes::to_tick`); a row that went takes
    /// its tick with it; a selection whose row went is dropped rather
    /// than left pointing at nothing.
    pub(crate) fn refresh_changes(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let mut listed = match self.git_status.get(&project) {
            Some(Ok(map)) => list(map),
            _ => Vec::new(),
        };
        let state = self.changes.entry(project).or_insert_with(|| ChangesState {
            list: Vec::new(),
            ticked: HashSet::new(),
            partial: HashSet::new(),
            selected: None,
            selection: Default::default(),
            directories: Default::default(),
            expanded: Default::default(),
            discovery: None,
            speed: None,
            folded: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            message: None,
            message_listener: None,
            clear_pending: false,
        });
        // A status read that failed shows its error in place of the list and
        // changes nothing behind it — review r3 `bugs+impl-2`: emptying the
        // list would clear the ticks, and the next good map would tick every
        // row as new.
        let Some(Ok(map)) = self.git_status.get(&project) else {
            cx.notify();
            return;
        };
        let included: HashSet<PathBuf> = state.ticked.union(&state.partial).cloned().collect();
        for path in crate::changes::ticks_for(&state.list, &listed, &included) {
            state.ticked.insert(path);
        }
        let now = crate::changes::sides(map);
        crate::changes::retick(&now, &mut state.ticked, &mut state.partial);
        let roots: Vec<_> = listed
            .iter()
            .filter(|l| l.row.is_dir)
            .map(|l| l.row.path.clone())
            .collect();
        // Keep the previous subtree visible until its replacement lands, just as
        // Log keeps its current page during a refresh. Path identities survive.
        let root_paths: HashSet<_> = roots.iter().map(PathBuf::as_path).collect();
        let under_root = |path: &Path| path.ancestors().any(|p| root_paths.contains(p));
        let now: HashSet<_> = listed.iter().map(|l| l.row.path.clone()).collect();
        if !roots.is_empty() {
            listed.extend(
                state
                    .list
                    .iter()
                    .filter(|l| !now.contains(&l.row.path) && under_root(&l.row.path))
                    .cloned(),
            );
        }
        let now: HashSet<_> = listed.iter().map(|l| l.row.path.as_path()).collect();
        state
            .ticked
            .retain(|path| now.contains(path.as_path()) || under_root(path));
        state.list = listed;
        state.list.sort_by(|a, b| {
            (a.section == Section::Unversioned, &a.row.path)
                .cmp(&(b.section == Section::Unversioned, &b.row.path))
        });
        state.settle_selection();
        state.directories.retain(|path, _| under_root(path));
        if let Some(root) = self.desk.project(project).map(|p| p.path.clone()) {
            let task = cx.background_spawn(async move {
                let mut directories = std::collections::HashMap::new();
                for path in &roots {
                    directories.extend(crate::changes::discover_directory_tree(&root, path));
                }
                (roots, directories)
            });
            let task = cx.spawn(async move |owner, cx| {
                let (roots, directories) = task.await;
                let _ = owner.update(cx, |owner, cx| {
                    if let Some(state) = owner.changes.get_mut(&project) {
                        let roots: HashSet<_> = roots.into_iter().collect();
                        state.list.retain(|l| {
                            !l.row
                                .path
                                .ancestors()
                                .skip(1)
                                .any(|parent| roots.contains(parent))
                        });
                        state.directories = directories;
                        state.rebuild_children();
                        let files: HashSet<_> = state
                            .list
                            .iter()
                            .filter(|l| !l.row.is_dir && !l.row.unmerged)
                            .map(|l| &l.row.path)
                            .collect();
                        state.ticked.retain(|p| files.contains(p));
                        state.settle_selection();
                    }
                    cx.notify();
                });
            });
            self.changes.get_mut(&project).unwrap().discovery = Some(task);
        }
    }

    /// The message box of the active project, made the first time the
    /// Changes face is drawn: an input needs a window, and a map lands
    /// without one. `cmd-enter` in it is the commit; `enter` a newline.
    pub(crate) fn ensure_message_box(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        let ui = self.ui();
        let Some(state) = self.changes.get_mut(&project) else {
            return;
        };
        if let Some(message) = &state.message {
            if state.clear_pending {
                message.update(cx, |input, cx| input.set_value("", window, cx));
                state.clear_pending = false;
            }
            // `cmd-k` asked for the box — here, because this is where
            // the box and a window are both in hand.
            if std::mem::take(&mut self.commit_message_wanted) {
                message.update(cx, |input, cx| input.focus(window, cx));
            }
            return;
        }
        let message = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(4, 12)
                .placeholder(ui.text(Text::CommitPlaceholder))
        });
        let listener = cx.subscribe_in(
            &message,
            window,
            |root, _box, event: &InputEvent, _window, cx| {
                if let InputEvent::PressEnter {
                    secondary: true, ..
                } = event
                {
                    root.commit(cx);
                }
            },
        );
        if std::mem::take(&mut self.commit_message_wanted) {
            message.update(cx, |input, cx| input.focus(window, cx));
        }
        state.message = Some(message);
        state.message_listener = Some(listener);
    }

    /// `cmd-k`: the Commit panel, open, with the caret in its message.
    ///
    /// **`cmd-k` commits nothing** — it brings the Commit tool window up
    /// with the message field focused, and the commit is `cmd-enter`
    /// there or the button. Until 2026-09-18 Guild's committed on the
    /// stroke, which a person used to the common IDE convention presses
    /// to *start* writing a message; the person asked for the convention.
    ///
    /// The panel is revealed and never toggled — a second `cmd-k` with
    /// it up must not close it — and the box is focused where it is
    /// made, at the Changes face's render, because on a panel just
    /// opened it does not exist yet.
    pub(crate) fn open_commit_message(&mut self, cx: &mut Context<Self>) {
        self.reveal_tool(guild_session::Tool::Commit, cx);
        self.commit_message_wanted = true;
        cx.notify();
    }

    /// Why a commit cannot be made now, or nothing when it can — what
    /// the button's tooltip says and whether it is lit.
    pub(crate) fn commit_blocked_by(&self, cx: &Context<Self>) -> Option<Text> {
        self.commit_blocked_in(self.active_project?, cx)
    }

    /// [`Self::commit_blocked_by`] for one project.
    pub(crate) fn commit_blocked_in(&self, project: ProjectId, cx: &Context<Self>) -> Option<Text> {
        let state = self.changes.get(&project)?;
        let message = state
            .message
            .as_ref()
            .map(|m| m.read(cx).value().to_string())
            .unwrap_or_default();
        // A commit pressed while a hunk write is pending is held and planned
        // from the status that write produces, so nothing ticked *yet* does
        // not grey the button — review r3 `bugs+impl-3`; the plan is asked
        // again when it runs.
        if self.hunks_pending(project) {
            return message.trim().is_empty().then_some(Text::CommitNoMessage);
        }
        if state.ticked.is_empty() && state.partial.is_empty() {
            return Some(Text::CommitNothingStaged);
        }
        if !can_commit(&state.ticked, &state.partial, &message) {
            return Some(Text::CommitNoMessage);
        }
        None
    }

    /// The rows hunk writes changed take their ticks from the index, after
    /// the map those writes produced has landed. GLD-026.
    pub(crate) fn ticks_follow_index(
        &mut self,
        project: ProjectId,
        written: &std::collections::HashSet<PathBuf>,
    ) {
        if written.is_empty() {
            return;
        }
        let sides = match self.git_status.get(&project) {
            Some(Ok(map)) => crate::changes::sides(map),
            _ => return,
        };
        if let Some(state) = self.changes.get_mut(&project) {
            for path in written {
                crate::changes::tick_from_index(
                    path,
                    sides.get(path).copied(),
                    &mut state.ticked,
                    &mut state.partial,
                );
            }
        }
    }

    /// The commit: every dirty tab of the project written first — a
    /// commit takes the disk, and the disk must be what is on screen —
    /// and refused while a tab cannot be written, naming it; then the
    /// message into an owned file in the project state directory and
    /// one serial preparation/Commit request to the git thread. Its answer
    /// comes through `poll_git`: the notice and cleared box on success,
    /// git's words otherwise with
    /// the message left in the box.
    pub(crate) fn commit(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.commit_in(project, cx);
    }

    /// The commit of one project, whichever is in front — a commit held for
    /// a hunk write runs in its own project when its status lands. GLD-026.
    pub(crate) fn commit_in(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        // The panel has never looked at this project, so there is nothing
        // to commit *yet* rather than nothing to commit: open it and let
        // the person see the list and the box. Neither route here can
        // reach it any more — the button and `cmd-enter` both live in the
        // panel — and it stays as the answer if one ever does.
        if !self.changes.contains_key(&project) {
            if self.active_project == Some(project) {
                self.reveal_tool(guild_session::Tool::Commit, cx);
            }
            return;
        }
        if self.hold_commit_for_hunks(project) {
            return;
        }
        // **Whatever stops a commit has to be said.** The button carries
        // its reason in a tooltip and is greyed; a stroke has neither, so
        // since M8.3.2 the same reason is a notice. Silence here read as
        // "cmd-k does not work", while `cmd-k` still committed; `cmd-enter`
        // in the box is a stroke with no tooltip too.
        if let Some(why) = self.commit_blocked_in(project, cx) {
            let text = self.ui().text(why).to_owned();
            self.inform(text);
            cx.notify();
            return;
        }
        // **A folder inside its repository commits only what it shows.**
        // git's index is the repository's, and a change staged outside the
        // folder would ride along. Refused rather than run with a pathspec:
        // `git commit -- .` takes the working tree of every tracked path
        // under the folder, unticked changes included, which is not the
        // commit the ticks describe. Here, where the button and
        // `cmd-enter` meet. Decision 18 of M5.11.
        if let Some(Ok(map)) = self.git_status.get(&project)
            && !map.staged_elsewhere().is_empty()
        {
            let paths: Vec<String> = map
                .staged_elsewhere()
                .iter()
                .map(|path| path.display().to_string())
                .collect();
            let text = self.ui().commit_refused_outside(&paths);
            self.warn(text);
            cx.notify();
            return;
        }
        self.autosave_files(Trigger::Commit, cx);
        if let Some(name) = self.unwritable_tab(project, cx) {
            let text = self.ui().commit_refused_by(&name);
            self.warn(text);
            cx.notify();
            return;
        }
        let Some(state) = self.changes.get(&project) else {
            return;
        };
        let Some(message) = state.message.as_ref() else {
            return;
        };
        let contents = crate::changes::message_file_contents(&message.read(cx).value());
        // The index brought to the ticks, then Commit in one owned serial
        // request. A preparation refusal stops the request — decision 4.
        let plan = match self.git_status.get(&project) {
            Some(Ok(map)) => commit_plan(map, &state.ticked, &state.partial),
            _ => return,
        };
        let dir = guild_session::project_state::dir(&root);
        let message = (|| -> std::io::Result<tempfile::NamedTempFile> {
            use std::io::Write as _;
            std::fs::create_dir_all(&dir)?;
            let mut file = tempfile::NamedTempFile::new_in(&dir)?;
            file.write_all(contents.as_bytes())?;
            Ok(file)
        })();
        let message = match message {
            Ok(file) => file,
            Err(err) => {
                self.fail(self.ui().notice_message_not_written(&err.to_string()));
                cx.notify();
                return;
            }
        };
        if let Some(worker) = self.status_workers.get(&project) {
            worker.prepare_commit(plan.stage, plan.unstage, message);
        }
    }

    /// A commit landed: the box is cleared and the notice says which.
    pub(crate) fn commit_landed(
        &mut self,
        project: ProjectId,
        short: &str,
        summary: &str,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.changes.get_mut(&project) {
            state.clear_pending = true;
        }
        let text = self.ui().notice_committed(short, summary);
        self.inform(text);
        cx.notify();
    }

    /// The toolbar's refresh: git asked again for the project's changes.
    ///
    /// Everything, log included: a person who presses refresh is asking
    /// for what git says now, not for part of it.
    pub(crate) fn refresh_changes_now(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.active_project {
            self.ask_git(project, true, cx);
        }
    }

    /// The toolbar and Enter open the diffs of the selected files.
    pub(crate) fn show_selected_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.changes() else {
            return;
        };
        let paths: Vec<_> = state
            .selection
            .selected
            .iter()
            .filter(|path| {
                state.list.iter().any(|l| {
                    &l.row.path == *path && !l.row.is_dir && !state.folded.contains(&l.section)
                })
            })
            .cloned()
            .collect();
        for path in paths {
            self.open_diff(Some(path), Base::Head, window, cx);
        }
    }

    /// The toolbar's tick all and untick all: every row of both sections,
    /// a conflict excepted.
    pub(crate) fn tick_all(&mut self, ticked: bool, cx: &mut Context<Self>) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        if ticked {
            let selected: Vec<_> = state
                .list
                .iter()
                .filter(|l| !l.row.unmerged)
                .map(|l| l.row.path.clone())
                .collect();
            match crate::changes::eligible_targets(&state.list, &state.directories, &selected) {
                Ok(paths) => {
                    state.partial.clear();
                    state.ticked.extend(paths)
                }
                Err(error) => self.warn(self.ui().commit_eligibility(&error)),
            }
        } else {
            state.ticked.clear();
            state.partial.clear();
        }
        cx.notify();
    }

    /// The toolbar's collapse all and expand all.
    pub(crate) fn fold_all(&mut self, folded: bool, cx: &mut Context<Self>) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        if folded {
            state
                .folded
                .extend([Section::Changes, Section::Unversioned]);
        } else {
            state.folded.clear();
        }
        state.settle_selection();
        cx.notify();
    }

    /// A section head's chevron: folded, or unfolded.
    pub(crate) fn toggle_fold(&mut self, section: Section, cx: &mut Context<Self>) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        if !state.folded.remove(&section) {
            state.folded.insert(section);
        }
        state.settle_selection();
        cx.notify();
    }

    /// The active project's changes, once a map has landed for it.
    pub(crate) fn changes(&self) -> Option<&ChangesState> {
        self.active_project.and_then(|p| self.changes.get(&p))
    }

    pub(crate) fn changes_mut(&mut self) -> Option<&mut ChangesState> {
        self.active_project.and_then(|p| self.changes.get_mut(&p))
    }

    /// A click on a row: it is selected, the panel takes the focus, and a
    /// file's diff — the whole change, against `HEAD` — opens beside it.
    /// Plain directory activation discloses its eligible children; modifiers only select.
    pub(crate) fn changes_click(
        &mut self,
        section: Section,
        path: &Path,
        is_dir: bool,
        modifiers: gpui::Modifiers,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        state.selection.click(
            path,
            &state.visible_paths(),
            modifiers.platform,
            modifiers.shift,
        );
        state.selected = Some((section, path.to_path_buf()));
        let focus = state.focus.clone();
        if is_dir && !modifiers.platform && !modifiers.shift && !state.expanded.remove(path) {
            state.expanded.insert(path.to_path_buf());
        }
        if !is_dir && !modifiers.platform && !modifiers.shift {
            self.open_diff(Some(path.to_path_buf()), Base::Head, window, cx);
        }
        // After the diff, not before: opening a tab focuses it, and a
        // click in the panel is a look, not a move — the space bar and
        // the arrows are still the panel's. Enter is how one goes there.
        window.focus(&focus, cx);
        cx.notify();
    }

    /// *Rollback…*: the row back to what `HEAD` holds, once the person
    /// has said so to a sheet that names the file.
    ///
    /// **The window asks because git will not.** Every other write this
    /// crate makes is either undone by a command a person already knows
    /// or refused by git in its own words; a `restore` is neither — it
    /// takes text that was never committed and there is nowhere to look
    /// for it afterwards. That is the guard decision 21 asks for where
    /// git has none of its own to lend.
    ///
    /// What it actually becomes is `changes::rollback_of`'s to say: an
    /// added file is unstaged and left on the disk, and an unversioned
    /// row is not offered the item at all.
    pub(crate) fn rollback_asking(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(state) = self.changes() else {
            return;
        };
        let Some(write) = crate::changes::rollback_of(&state.list, &path) else {
            return;
        };
        // One question at a time. The menu stays live under a sheet, and
        // a second question queued for a row that has already gone is a
        // bug this crate has had once already.
        if self.asking {
            return;
        }
        let ui = self.ui();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        // The two writes lose different things and so do the two sheets:
        // one throws away text and says so; the other moves a file out of
        // the index and leaves it where it is, and says nothing under the
        // question — 2026-09-28, a line only where something is lost.
        let title = ui.rollback_title(&name);
        let detail = (!matches!(write, Write::Unstage(_))).then(|| ui.confirm_rollback());
        let answers = [
            gpui::PromptButton::new(ui.text(Text::Rollback)),
            // Typed rather than a plain string: the platform binds
            // Escape to the button it knows as Cancel, and from a `&str`
            // it knows the English word only.
            gpui::PromptButton::cancel(ui.text(Text::Cancel)),
        ];
        let asked = window.prompt(gpui::PromptLevel::Warning, &title, detail, &answers, cx);
        self.asking = true;
        cx.spawn(async move |root, cx| {
            let answer = asked.await;
            let _ = root.update(cx, |root, cx| {
                root.asking = false;
                if answer == Ok(0) {
                    // To the project's git thread, and a status walk
                    // after it: nothing about a write happens here.
                    root.run_git_write(project, write);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Menu and keyboard rollback share the selected group. Refuse the entire
    /// group when any member is unversioned or conflicted, then ask once.
    pub(crate) fn rollback_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.changes() else {
            self.reveal_tool(guild_session::Tool::Commit, cx);
            return;
        };
        let paths = state.selection.selected.clone();
        if paths.is_empty() {
            self.inform(self.ui().text(Text::ChangesSelectRow).to_owned());
            cx.notify();
            return;
        }
        let writes: Option<Vec<_>> = paths
            .iter()
            .map(|p| crate::changes::rollback_of(&state.list, p))
            .collect();
        let Some(writes) = writes else {
            self.warn(self.ui().text(Text::CommitRollbackMixed).to_owned());
            cx.notify();
            return;
        };
        if paths.len() == 1 {
            self.rollback_asking(paths[0].clone(), window, cx);
            return;
        }
        if self.asking {
            return;
        }
        let Some(project) = self.active_project else {
            return;
        };
        let detail = format!(
            "{}\n\n{}",
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
            self.ui().text(Text::CommitRollbackGroupDetail)
        );
        let asked = window.prompt(
            gpui::PromptLevel::Warning,
            &self.ui().commit_rollback_title(paths.len()),
            Some(&detail),
            &[
                gpui::PromptButton::new(self.ui().text(Text::Rollback)),
                gpui::PromptButton::cancel(self.ui().text(Text::Cancel)),
            ],
            cx,
        );
        self.asking = true;
        cx.spawn(async move |owner, cx| {
            let accepted = asked.await == Ok(0);
            let _ = owner.update(cx, |root, cx| {
                root.asking = false;
                if accepted {
                    for write in writes {
                        root.run_git_write(project, write);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Immediate staging of the selected eligible files, distinct from inclusion.
    pub(crate) fn add_to_vcs_selected(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.changes() else {
            self.inform(self.ui().text(Text::ChangesSelectRow).to_owned());
            cx.notify();
            return;
        };
        if state.selection.selected.is_empty() {
            self.inform(self.ui().text(Text::ChangesSelectRow).to_owned());
            cx.notify();
            return;
        }
        if state.selection.selected.iter().any(|p| {
            state
                .list
                .iter()
                .any(|l| &l.row.path == p && l.section != Section::Unversioned)
        }) {
            self.warn(self.ui().text(Text::CommitAddMixed).to_owned());
            cx.notify();
            return;
        }
        match crate::changes::eligible_targets(
            &state.list,
            &state.directories,
            &state.selection.selected,
        ) {
            Ok(paths) => self.git_write(Write::Stage(paths), cx),
            Err(error) => self.warn(self.ui().commit_eligibility(&error)),
        }
        cx.notify();
    }

    pub(crate) fn changes_include_selected(
        &mut self,
        included: Option<bool>,
        cx: &mut Context<Self>,
    ) {
        if self
            .changes()
            .is_none_or(|s| s.selection.selected.is_empty())
        {
            self.inform(self.ui().text(Text::ChangesSelectRow).to_owned());
            cx.notify();
            return;
        }
        let result = self.changes().map(|s| {
            crate::changes::eligible_targets(&s.list, &s.directories, &s.selection.selected)
        });
        match result {
            Some(Ok(paths)) => {
                let state = self.changes_mut().unwrap();
                let include =
                    included.unwrap_or_else(|| paths.iter().any(|p| !state.ticked.contains(p)));
                for path in paths {
                    state.partial.remove(&path);
                    if include {
                        state.ticked.insert(path);
                    } else {
                        state.ticked.remove(&path);
                    }
                }
            }
            Some(Err(error)) => self.warn(self.ui().commit_eligibility(&error)),
            None => {}
        }
        cx.notify();
    }

    pub(crate) fn changes_context(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(state) = self.changes_mut() {
            state
                .selection
                .context_click(&path.to_path_buf(), &state.visible_paths());
            state.selected = state
                .list
                .iter()
                .find(|l| l.row.path == path)
                .map(|l| (l.section, path.into()));
            window.focus(&state.focus, cx);
        }
    }

    pub(crate) fn changes_trash_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = self
            .changes()
            .map(|s| s.selection.selected.clone())
            .unwrap_or_default();
        if paths.is_empty() {
            self.warn(self.ui().text(Text::ChangesSelectRow).to_owned());
            cx.notify();
            return;
        }
        if paths.iter().any(|p| !super::tree::writable_row(p)) {
            self.warn(self.ui().text(Text::CommitTrashRoot).to_owned());
            cx.notify();
            return;
        }
        if self.changes().is_some_and(|s| {
            paths.iter().any(|p| {
                s.list
                    .iter()
                    .any(|l| &l.row.path == p && l.row.mark == crate::theme::Mark::Deleted)
            })
        }) {
            self.warn(self.ui().text(Text::CommitTrashMissing).to_owned());
            cx.notify();
            return;
        }
        self.trash_paths_asking(paths, true, window, cx);
    }

    /// A row's checkbox: in the commit, or not. Nothing is written until
    /// the commit — decision 4.
    pub(crate) fn changes_toggle_row(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        let paths = crate::changes::eligible_targets(
            &state.list,
            &state.directories,
            &[path.to_path_buf()],
        );
        match paths {
            Ok(paths) => {
                // A partial tick becomes full, as a full one clears.
                let tick = paths.iter().any(|p| !state.ticked.contains(p));
                for path in paths {
                    state.partial.remove(&path);
                    if tick {
                        state.ticked.insert(path);
                    } else {
                        state.ticked.remove(&path);
                    }
                }
            }
            Err(error) => self.warn(self.ui().commit_eligibility(&error)),
        }
        cx.notify();
    }

    /// A section head's checkbox: every row of the section ticked, or,
    /// when every one already is, none.
    pub(crate) fn changes_toggle_group(&mut self, section: Section, cx: &mut Context<Self>) {
        let Some(state) = self.changes_mut() else {
            return;
        };
        let selected: Vec<_> = state
            .list
            .iter()
            .filter(|l| l.section == section && !l.row.unmerged)
            .map(|l| l.row.path.clone())
            .collect();
        let paths =
            match crate::changes::eligible_targets(&state.list, &state.directories, &selected) {
                Ok(paths) => paths,
                Err(error) => {
                    self.warn(self.ui().commit_eligibility(&error));
                    cx.notify();
                    return;
                }
            };
        if paths.is_empty() {
            return;
        }
        // Every row in the commit — partial ones included — clears the
        // group; otherwise every row is ticked whole.
        let all = paths
            .iter()
            .all(|p| state.ticked.contains(p) || state.partial.contains(p));
        for path in &paths {
            state.partial.remove(path);
        }
        if all {
            for path in &paths {
                state.ticked.remove(path);
            }
        } else {
            state.ticked.extend(paths);
        }
        cx.notify();
    }

    /// Hand a write to the project's git thread. Nothing changes here:
    /// the map that shows it, or git's refusal, comes back through
    /// `poll_git` like everything else git says.
    fn git_write(&mut self, write: Write, _cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.run_git_write(project, write);
    }

    /// Send a write to a project's git thread.
    ///
    /// Every write goes through here, and every write is answered the
    /// same way: a status walk, and git's refusal in its own words if
    /// there is one — decision 18's shape, which decision 21 kept when
    /// it lifted the count.
    pub(crate) fn run_git_write(&mut self, project: ProjectId, write: Write) {
        if let Some(worker) = self.status_workers.get(&project) {
            worker.run(write);
        }
    }

    /// A dirty tab of this project that could not be written, by name.
    ///
    /// Dirty tabs only: a tab not yet shown has no editor and so no text
    /// to refuse with, and a clean tab has nothing a write needs from
    /// it. Whatever takes the working tree — a commit, a switch — asks
    /// this first, because git must see what the window shows.
    pub(crate) fn unwritable_tab(&self, project: ProjectId, cx: &Context<Self>) -> Option<String> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                Body::File { tab: file, .. } => {
                    let file = file.read(cx);
                    (file.is_dirty()
                        && (file.refuses_autosave().is_some() || file.write_failure().is_some()))
                    .then(|| file.name())
                }
                _ => None,
            })
    }

    pub(crate) fn reveal_changes_selection(&self) {
        let Some(state) = self.changes() else {
            return;
        };
        let Some(path) = state.selection.action_target() else {
            return;
        };
        if let Some(row) = self
            .file_speed_rows(crate::root::speed_search::FileHost::Commit)
            .iter()
            .find(|row| &row.key == path)
        {
            state
                .scroll
                .scroll_to_item(row.row, ScrollStrategy::Nearest);
        }
    }

    /// A key pressed with the panel focused. Answers whether the panel
    /// took it. The arrows move the selection over every group's rows
    /// and clamp at the ends; Enter opens file diffs or discloses a directory,
    /// Left/Right navigate the hierarchy, Space toggles group inclusion and
    /// Escape gives the focus back to the tab.
    pub(crate) fn changes_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.changes().is_some_and(|s| s.focus.is_focused(window)) {
            return false;
        }
        let visible_rows = self.file_speed_rows(crate::root::speed_search::FileHost::Commit);
        let Some(state) = self.changes_mut() else {
            return false;
        };
        let paths = state.visible_paths();
        let key = event.keystroke.key.as_str();
        let modifiers = event.keystroke.modifiers;
        if state
            .selection
            .key(key, &paths, modifiers.platform, modifiers.shift)
        {
            state.selected = state.selection.focus.as_ref().and_then(|path| {
                state
                    .list
                    .iter()
                    .find(|l| &l.row.path == path)
                    .map(|l| (l.section, path.clone()))
            });
            if let Some((_, path)) = &state.selected
                && let Some(line) = visible_rows
                    .iter()
                    .find(|row| &row.key == path)
                    .map(|row| row.row)
            {
                state.scroll.scroll_to_item(line, ScrollStrategy::Top);
            }
            cx.notify();
            return true;
        }
        match key {
            "space" if !modifiers.platform && !modifiers.alt && !modifiers.control => {
                self.changes_include_selected(None, cx)
            }
            "enter" | "right" | "left"
                if !modifiers.platform && !modifiers.alt && !modifiers.control =>
            {
                if let Some(path) = state.selection.action_target().cloned() {
                    let directory = state
                        .list
                        .iter()
                        .any(|l| l.row.path == path && l.row.is_dir);
                    if directory {
                        if key == "left" && state.expanded.remove(&path) {
                        } else if key == "left" {
                            if let Some(parent) =
                                path.parent().filter(|p| paths.contains(&p.to_path_buf()))
                            {
                                state.selection.click(parent, &paths, false, false);
                            }
                        } else if key == "right" && state.expanded.contains(&path) {
                            if let Some(child) =
                                paths.iter().find(|p| *p != &path && p.starts_with(&path))
                            {
                                state.selection.click(child, &paths, false, false);
                            }
                        } else if key == "enter" && state.expanded.contains(&path) {
                            state.expanded.remove(&path);
                        } else {
                            state.expanded.insert(path);
                        }
                    } else if key == "left" {
                        if let Some(parent) =
                            path.parent().filter(|p| paths.contains(&p.to_path_buf()))
                        {
                            state.selection.click(parent, &paths, false, false);
                        }
                    } else if key == "enter" {
                        self.show_selected_diff(window, cx);
                    }
                }
            }
            "escape" => {
                let focus = self
                    .active_body()
                    .and_then(|body| body.focus_handle(cx))
                    .unwrap_or_else(|| self.focus.clone());
                window.focus(&focus, cx);
            }
            _ => return false,
        }
        if let Some(state) = self.changes_mut() {
            state.settle_selection();
        }
        if matches!(key, "left" | "right" | "enter") {
            self.reveal_changes_selection();
        }
        cx.notify();
        true
    }
}

impl ChangesState {
    fn rebuild_children(&mut self) {
        let mut known: HashSet<PathBuf> = self.list.iter().map(|l| l.row.path.clone()).collect();
        for directory in self.directories.values().filter_map(|d| d.as_ref().ok()) {
            for (path, is_dir) in directory
                .directories
                .iter()
                .map(|p| (p, true))
                .chain(directory.files.iter().map(|p| (p, false)))
            {
                if known.insert(path.clone()) {
                    self.list.push(Listed {
                        section: Section::Unversioned,
                        row: crate::changes::Row {
                            path: path.clone(),
                            mark: crate::theme::Mark::Untracked,
                            from: None,
                            is_dir,
                            unmerged: false,
                            staged: false,
                        },
                    });
                }
            }
        }
        self.list.sort_by(|a, b| {
            (a.section == Section::Unversioned, &a.row.path)
                .cmp(&(b.section == Section::Unversioned, &b.row.path))
        });
    }

    pub(crate) fn row_visible(&self, path: &Path) -> bool {
        !path
            .ancestors()
            .skip(1)
            .any(|p| self.directories.contains_key(p) && !self.expanded.contains(p))
    }

    pub(crate) fn membership(&self, path: &Path) -> (bool, bool) {
        if let Some(Ok(dir)) = self.directories.get(path) {
            let n = dir
                .files
                .iter()
                .filter(|p| self.ticked.contains(*p))
                .count();
            (
                !dir.files.is_empty() && n == dir.files.len(),
                n > 0 && n < dir.files.len(),
            )
        } else {
            (self.ticked.contains(path), false)
        }
    }

    pub(crate) fn directory_label(&self, path: &Path, ui: crate::i18n::Ui) -> String {
        match self.directories.get(path) {
            None => ui.text(Text::CommitDirectoryLoading).to_owned(),
            Some(Err(error)) => ui.commit_directory_error(error),
            Some(Ok(dir)) if dir.files.is_empty() && dir.ignored > 0 => {
                ui.commit_ignored_only(dir.ignored)
            }
            Some(Ok(dir)) if dir.empty => ui.text(Text::CommitDirectoryEmpty).to_owned(),
            Some(Ok(dir)) if dir.files.is_empty() => {
                ui.text(Text::CommitDirectoryNoEligible).to_owned()
            }
            Some(Ok(dir)) => ui.commit_directory_count(dir.files.len(), dir.ignored),
        }
    }
    pub(crate) fn settle_selection(&mut self) {
        self.selection.retain(&self.visible_paths());
        self.selected = self.selection.focus.as_ref().and_then(|path| {
            self.list
                .iter()
                .find(|l| &l.row.path == path)
                .map(|l| (l.section, path.clone()))
        });
    }

    pub(crate) fn visible_paths(&self) -> Vec<PathBuf> {
        rows_in_order(&self.list)
            .into_iter()
            .filter(|l| !self.folded.contains(&l.section))
            .filter(|l| self.row_visible(&l.row.path))
            .map(|l| l.row.path.clone())
            .collect()
    }
}
