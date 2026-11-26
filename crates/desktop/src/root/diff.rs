//! Diffs in tabs: opening one for a file or for the working tree, the
//! working text a diff is computed against — an open tab's buffer, or
//! the disk — and the jump from a hunk's line back into the file.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{Context, Entity, Focusable, Window};
use gpui_component::input::EditorState;
use guild_session::{Base, ProjectId};

use crate::diff_tab::{DiffEvent, DiffTab, tree_files};
use crate::watch::Batch;
use crate::{Body, Root, Tab, TabId};

impl Root {
    /// `cmd-d`: the diff of the file in front. Nothing for a tab that is
    /// not a file.
    pub(crate) fn show_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // **The project's own files only.** A tab under a dependency's
        // root has a path relative to that root, and asking git for a
        // diff of it would name a file in this repository that happens
        // to have the same name. M8.8.3.
        let Some(Body::File {
            path,
            origin: crate::Origin::Project,
            ..
        }) = self.active_body()
        else {
            return;
        };
        let path = path.clone();
        self.open_diff(Some(path), Base::Head, window, cx);
    }

    /// `f7` and `shift-f7`: the diff in front steps to its next or its
    /// previous hunk.
    pub(crate) fn step_diff_change(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(Body::Diff { tab, .. }) = self.active_body() else {
            return;
        };
        let tab = tab.clone();
        tab.update(cx, |diff, cx| diff.step_change(forward, cx));
    }

    /// Open the diff of one file, or of the working tree — or go to the
    /// tab that already shows it. One tab per subject, like files.
    pub(crate) fn open_diff(
        &mut self,
        target: Option<PathBuf>,
        base: Base,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.open_diff_without_history(target, base, window, cx);
        self.history.suspended -= 1;
    }

    fn open_diff_without_history(
        &mut self,
        target: Option<PathBuf>,
        base: Base,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };

        let existing = self.find_showing(self.seat_for_project(project), |body| {
            matches!(body, Body::Diff { path, base: shown, .. } if *path == target && *shown == base)
        });
        if let Some((tab, pane)) = existing {
            self.active_tab = Some(tab);
            if let Some(focus) = self
                .focus_pane(tab, pane, cx)
                .and_then(|body| body.focus_handle(cx))
            {
                window.focus(&focus, cx);
            }
            self.save();
            cx.notify();
            return;
        }

        let look = self.file_look();
        let ui = self.ui();
        let mode = self.settings.diff_view;
        let view = match target.clone() {
            Some(path) => {
                // Both sides of the Staged diff are git's blobs: the working
                // text is not read at all.
                let working = (!matches!(base, Base::Index))
                    .then(|| self.working_text(&root, project, &path, cx))
                    .flatten();
                let base = base.clone();
                cx.new(|cx| DiffTab::open_file(&root, path, base, mode, working, look, ui, cx))
            }
            None => {
                let files = self.tree_working_texts(&root, project, cx);
                cx.new(|cx| DiffTab::open_tree(&root, files, mode, look, ui, cx))
            }
        };
        for path in view.read(cx).paths() {
            if let Some((editor, file)) = self.file_editor(project, &path, cx) {
                view.update(cx, |tab, cx| {
                    tab.follow(path, &editor, ending_of(&file), cx)
                });
            }
        }
        self.hear_diff(&view, window, cx);

        let focus = view.read(cx).focus_handle(cx);
        let id = self.adopt(
            self.seat_for_project(project),
            project,
            Body::Diff {
                path: target,
                base,
                tab: view,
            },
            cx,
        );
        self.active_tab = Some(id);
        window.focus(&focus, cx);
        self.save();
        cx.notify();
    }

    /// Listen to a diff tab's one request of the window: the jump into a
    /// file, which needs the window and so cannot be wired without one.
    pub(crate) fn hear_diff(
        &mut self,
        view: &Entity<DiffTab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.subscribe_in(
            view,
            window,
            |root, tab, event: &DiffEvent, window, cx| match event {
                // A hunk names a line and no column: its start.
                DiffEvent::OpenAt { path, line } => {
                    root.open_file_at(path.clone(), *line, 0, window, cx)
                }
                DiffEvent::NeedsWorking { .. } => {}
                // M8.6: which tab it came from is the whole answer — the
                // agent waiting is the one parked against that tab, not
                // whichever tab happens to be in front.
                DiffEvent::Accept => {
                    if let Some(id) = root.tab_of_diff(tab) {
                        root.accept_proposal(id, cx);
                    }
                }
                // Only the diff window steps through files. M8.3.6.
                DiffEvent::StepFile { .. } | DiffEvent::SelectFile { .. } => {}
                DiffEvent::Reject => {
                    if let Some(id) = root.tab_of_diff(tab) {
                        root.reject_proposal(id, cx);
                    }
                }
                // GLD-026: the write goes to the git thread of the project
                // the diff belongs to, counted until its status lands.
                DiffEvent::Hunk(write) => {
                    let project = root
                        .tab_of_diff(tab)
                        .and_then(|id| root.tabs.iter().find(|t| t.id == id))
                        .map(|t| t.project);
                    if let Some(project) = project {
                        root.send_hunk(project, write.clone(), cx);
                    }
                }
                DiffEvent::HunkStale => {
                    let text = root
                        .ui()
                        .text(crate::i18n::Text::NoticeHunkStale)
                        .to_owned();
                    root.warn(text);
                    tab.update(cx, |diff, cx| diff.head_moved(cx));
                    cx.notify();
                }
            },
        )
        .detach();
    }

    /// Which tab shows this diff.
    fn tab_of_diff(&self, view: &Entity<DiffTab>) -> Option<TabId> {
        self.tabs
            .iter()
            .find(|tab| {
                tab.bodies()
                    .any(|body| matches!(body, Body::Diff { tab: shown, .. } if shown == view))
            })
            .map(|tab| tab.id)
    }

    /// A diff tab brought back after a restart: pushed where it was, not
    /// brought to the front, with the disk as its working text — the
    /// file tabs restored beside it are followed once every tab is back
    /// — and its listener wired at the first render, when there is a
    /// window to wire it with.
    pub(crate) fn restore_diff_tab(
        &mut self,
        project: ProjectId,
        root: &Path,
        target: Option<PathBuf>,
        base: Base,
        cx: &mut Context<Self>,
    ) -> TabId {
        let look = self.file_look();
        let ui = self.ui();
        let mode = self.settings.diff_view;
        let view = match target.clone() {
            Some(path) => {
                // The open path's rule: the Staged diff's sides are both
                // git's blobs, and no working text is read for them —
                // review r4 `bugs+impl-6`.
                let working = (!matches!(base, Base::Index))
                    .then(|| crate::file_operations::read_regular_file(&root.join(&path)))
                    .flatten();
                let base = base.clone();
                cx.new(|cx| DiffTab::open_file(root, path, base, mode, working, look, ui, cx))
            }
            // The status map is not here yet at start-up; the list fills
            // when it lands.
            None => cx.new(|cx| DiffTab::open_tree(root, Vec::new(), mode, look, ui, cx)),
        };
        self.unheard_diffs.push(view.clone());
        self.adopt(
            self.seat_for_project(project),
            project,
            Body::Diff {
                path: target,
                base,
                tab: view,
            },
            cx,
        )
    }

    /// Wire the listeners of the diff tabs restored before there was a
    /// window, and put every open file's buffer under the diffs that
    /// show it. From the first render.
    pub(crate) fn hear_restored_diffs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let unheard = std::mem::take(&mut self.unheard_diffs);
        if unheard.is_empty() {
            return;
        }
        for view in &unheard {
            self.hear_diff(view, window, cx);
        }
        let files: Vec<(ProjectId, PathBuf)> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.bodies().map(move |body| (tab.project, body)))
            .filter_map(|(project, body)| match body {
                Body::File {
                    path,
                    origin: crate::Origin::Project,
                    ..
                } => Some((project, path.clone())),
                _ => None,
            })
            .collect();
        for (project, path) in files {
            self.follow_in_diffs(project, &path, cx);
        }
    }

    /// The watcher's batch, for the diff tabs of a project: a file named
    /// in it that a tab shows and does not follow gets the disk as its
    /// working text again; a change under `.git` means `HEAD` may have
    /// moved, and every diff asks for it again.
    pub(crate) fn apply_batch_to_diffs(
        &mut self,
        project: ProjectId,
        batch: &Batch,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let views: Vec<Entity<DiffTab>> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
            .filter_map(|body| match body {
                Body::Diff { tab: view, .. } => Some(view.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            // A Staged diff shows no working text: a batch reads none for it.
            let shows_working = !matches!(view.read(cx).base(), Base::Index);
            for path in view.read(cx).paths() {
                if shows_working && batch.files().contains(&path) && !view.read(cx).follows(&path) {
                    let from_disk = crate::file_operations::read_regular_file(&root.join(&path));
                    view.update(cx, |diff, cx| diff.set_working(path, from_disk, cx));
                }
            }
            if batch.git() {
                view.update(cx, |diff, cx| diff.head_moved(cx));
            }
        }
    }

    /// The working tree's changed files with their working texts, from
    /// the project's last status map — nothing when there is none yet.
    fn tree_working_texts(
        &self,
        root: &Path,
        project: ProjectId,
        cx: &Context<Self>,
    ) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        let Some(Ok(map)) = self.git_status.get(&project) else {
            return Vec::new();
        };
        tree_files(map)
            .into_iter()
            .map(|entry| {
                let working = self.working_text(root, project, &entry.path, cx);
                (entry.path, working)
            })
            .collect()
    }

    /// A new status map landed for a project: its working-tree diff tab,
    /// if open, lists the files again and asks `HEAD` for the new ones.
    pub(crate) fn refresh_tree_diffs(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        let views: Vec<Entity<DiffTab>> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
            .filter_map(|body| match body {
                Body::Diff {
                    path: None,
                    tab: view,
                    ..
                } => Some(view.clone()),
                _ => None,
            })
            .collect();
        if views.is_empty() {
            return;
        }
        let files = self.tree_working_texts(&root, project, cx);
        for view in views {
            view.update(cx, |tab, cx| {
                tab.set_files(files.clone());
                cx.notify();
            });
            for path in view.read(cx).paths() {
                if !view.read(cx).follows(&path)
                    && let Some((editor, file)) = self.file_editor(project, &path, cx)
                {
                    view.update(cx, |tab, cx| {
                        tab.follow(path, &editor, ending_of(&file), cx)
                    });
                }
            }
        }
    }

    /// The editor of an open file tab for this path, if there is one.
    fn file_editor(
        &self,
        project: ProjectId,
        path: &Path,
        cx: &Context<Self>,
    ) -> Option<(Entity<EditorState>, Entity<crate::file_tab::FileTab>)> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
            .find_map(|body| match body {
                Body::File {
                    path: open,
                    tab: view,
                    origin: crate::Origin::Project,
                    ..
                } if open == path => view.read(cx).editor().cloned().map(|e| (e, view.clone())),
                _ => None,
            })
    }

    /// What the working tree holds for a path, as the window sees it:
    /// an open tab's buffer first, the disk otherwise, `None` when the
    /// file is not there.
    fn working_text(
        &self,
        root: &Path,
        project: ProjectId,
        path: &Path,
        cx: &Context<Self>,
    ) -> Option<Vec<u8>> {
        if let Some((editor, file)) = self.file_editor(project, path, cx) {
            let ending = file.read(cx).line_ending();
            return Some(
                ending
                    .apply(&editor.read(cx).text().to_string())
                    .into_bytes(),
            );
        }
        crate::file_operations::read_regular_file(&root.join(path))
    }

    /// Async realization can supply an editor after the opening gesture.
    /// Resolve current holders rather than capturing a path that may have moved
    /// or the active project, which may have changed while reading.
    pub(crate) fn follow_file_in_diffs(
        &mut self,
        file: &Entity<crate::file_tab::FileTab>,
        cx: &mut Context<Self>,
    ) {
        if file.read(cx).editor().is_none() {
            return;
        }
        let holders: Vec<_> = self
            .tabs
            .iter()
            .flat_map(|tab| {
                tab.bodies().filter_map(move |body| match body {
                    Body::File {
                        path,
                        tab: view,
                        origin: crate::Origin::Project,
                        ..
                    } if view == file => Some((tab.project, path.clone())),
                    _ => None,
                })
            })
            .collect();
        for (project, path) in holders {
            self.follow_in_diffs(project, &path, cx);
        }
    }

    /// A file tab opened: every diff tab of the project that shows this
    /// path follows its buffer from now on.
    pub(crate) fn follow_in_diffs(
        &mut self,
        project: ProjectId,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let Some((editor, file)) = self.file_editor(project, path, cx) else {
            return;
        };
        for body in self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
        {
            if let Body::Diff { tab: view, .. } = body
                && view.read(cx).files().iter().any(|f| f.path == path)
            {
                let followed = path.to_path_buf();
                let editor = editor.clone();
                view.update(cx, |diff, cx| {
                    diff.follow(followed, &editor, ending_of(&file), cx)
                });
            }
        }
    }

    /// A file tab closed: the diff tabs that followed its buffer go back
    /// to the disk.
    pub(crate) fn unfollow_in_diffs(
        &mut self,
        project: ProjectId,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let Some(root) = self.desk.project(project).map(|p| p.path.clone()) else {
            return;
        };
        for body in self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
        {
            if let Body::Diff { tab: view, .. } = body
                && view.read(cx).follows(path)
            {
                let from_disk = crate::file_operations::read_regular_file(&root.join(path));
                view.update(cx, |diff, cx| diff.unfollow(path, from_disk, cx));
            }
        }
    }

    /// Take what the diff tabs' threads have answered. On the window's
    /// clock, with the watcher and git.
    pub(crate) fn poll_diffs(&mut self, cx: &mut Context<Self>) {
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::Diff { tab: view, .. } = body {
                view.update(cx, |diff, cx| {
                    diff.poll(cx);
                });
            }
        }
    }

    /// Open a file with the caret at a place — the jump from a hunk and
    /// from a search hit. `line` is one-based, as a hunk and a hit count;
    /// `column` is characters from the line's start.
    ///
    /// **Through `go_to_place`** — M5.11, decision 6. That is the door for
    /// a jump a person asked for by naming somewhere: Back returns to
    /// where they were, and a tab restored with no editor yet gets its
    /// caret at its first render. Setting the caret here by hand did
    /// neither — the jump left nothing on Back's stack, and a hit into a
    /// restored tab landed at its top.
    pub(crate) fn open_file_at(
        &mut self,
        path: PathBuf,
        line: usize,
        column: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.active_project else {
            return;
        };
        let place = crate::root::history::Place::new(path, line.saturating_sub(1) as u32, column);
        self.go_to_place(project, place, window, cx);
    }

    /// Tell every diff tab the palette or the font changed.
    pub(crate) fn restyle_diff_tabs(&mut self, cx: &mut Context<Self>) {
        let look = self.file_look();
        let ui = self.ui();
        for body in self.tabs.iter().flat_map(Tab::bodies) {
            if let Body::Diff { tab: view, .. } = body {
                let look = look.clone();
                view.update(cx, |diff, cx| diff.restyle(look, ui, cx));
            }
        }
    }
}

/// The file's line endings as they are at each read — a reload keeps the
/// editor and can change them. GLD-026.
fn ending_of(
    file: &Entity<crate::file_tab::FileTab>,
) -> impl Fn(&gpui::App) -> crate::highlight::LineEnding + 'static {
    let file = file.downgrade();
    move |cx| {
        file.upgrade()
            .map_or(crate::highlight::LineEnding::Lf, |f| {
                f.read(cx).line_ending()
            })
    }
}
