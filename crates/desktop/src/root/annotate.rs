//! Inline annotation of editable files, and restoration of historical Annotate bodies.

use std::path::{Path, PathBuf};

use gpui::prelude::*;
use gpui::{Context, Entity, Window};
use guild_session::ProjectId;

use crate::annotate_tab::{AnnotateEvent, AnnotateTab};
use crate::git::GitError;
use crate::git::blame::Run;
use crate::{Body, Root, Tab, TabId};
#[cfg(test)]
mod tests;

impl Root {
    /// *Annotate*, on the file in front: its tab, or the one already open.
    pub(crate) fn annotate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        // A dependency's file is not in this repository, and git would
        // annotate a namesake of it that is. M8.8.3.
        let path = match self.active_body() {
            Some(Body::File {
                path,
                origin: crate::Origin::Project,
                ..
            }) => path.clone(),
            Some(Body::Annotate { path, .. }) => path.clone(),
            _ => return,
        };
        self.open_annotate(project, path, window, cx);
    }

    /// Open a file's annotation, or go to the tab that has it.
    pub(crate) fn open_annotate(
        &mut self,
        project: ProjectId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_project != Some(project) {
            return;
        }
        if !matches!(self.active_body(),Some(Body::File {path:shown,origin:crate::Origin::Project,..}) if *shown==path)
        {
            self.open_file(&path, window, cx);
        }
        if let Some(Body::File {
            tab,
            origin: crate::Origin::Project,
            ..
        }) = self.active_body()
        {
            let view = tab.clone();
            view.update(cx, |file, cx| file.toggle_blame(cx));
        }
    }

    /// An annotate tab brought back after a restart: pushed where it was,
    /// its blame asked for once the project's git thread exists.
    pub(crate) fn restore_annotate_tab(
        &mut self,
        project: ProjectId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> TabId {
        let id = self.push_annotate_tab(project, path.clone(), cx);
        self.ask_blame(project, &path);
        id
    }

    fn push_annotate_tab(
        &mut self,
        project: ProjectId,
        path: PathBuf,
        cx: &mut Context<Self>,
    ) -> TabId {
        let look = self.file_look();
        let ui = self.ui();
        let view = cx.new(|cx| AnnotateTab::new(path.clone(), look, ui, cx));
        cx.subscribe(&view, |root, _tab, event: &AnnotateEvent, cx| match event {
            AnnotateEvent::OpenCommit(sha) => root.open_commit_in_log(sha, cx),
        })
        .detach();
        self.adopt(
            self.seat_for_project(project),
            project,
            Body::Annotate { path, tab: view },
            cx,
        )
    }

    /// Ask the project's git thread for a file's blame. A thread that is
    /// not there yet — a restart — is asked by `poll_git`'s first walk
    /// through `refresh_blames`.
    fn ask_blame(&self, project: ProjectId, path: &Path) {
        if let Some(worker) = self.status_workers.get(&project) {
            worker.blame(path.to_path_buf());
        }
    }

    /// `HEAD` moved: every annotate tab of the project reads again.
    pub(crate) fn refresh_blames(&self, project: ProjectId, cx: &mut Context<Self>) {
        for tab in self.tabs.iter().filter(|tab| tab.project == project) {
            for body in tab.bodies() {
                if let Body::File {
                    tab,
                    origin: crate::Origin::Project,
                    ..
                } = body
                {
                    tab.update(cx, |file, cx| {
                        if file.blame_enabled() {
                            file.refresh_blame(cx)
                        }
                    });
                }
                if let Body::Annotate { path, .. } = body {
                    self.ask_blame(project, path);
                }
            }
        }
    }

    /// A blame landed: into every annotate tab of the project that shows
    /// the path.
    pub(crate) fn blame_landed(
        &mut self,
        project: ProjectId,
        path: &Path,
        result: Result<Vec<Run>, GitError>,
        cx: &mut Context<Self>,
    ) {
        let views: Vec<Entity<AnnotateTab>> = self
            .tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(Tab::bodies)
            .filter_map(|body| match body {
                Body::Annotate { path: shown, tab } if shown == path => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            view.update(cx, |tab, cx| tab.set_runs(result.clone(), cx));
        }
    }

    /// A run's commit, in the Log: the panel opens on the Log face, the
    /// commit is opened below the list and scrolled to when it is among
    /// the pages loaded.
    pub(crate) fn open_commit_in_log(&mut self, sha: &str, cx: &mut Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        self.reveal_commit(project, sha, None, None, cx);
    }

    /// The theme, the font or the language changed: every annotate tab.
    pub(crate) fn restyle_annotate_tabs(&mut self, cx: &mut Context<Self>) {
        let look = self.file_look();
        let ui = self.ui();
        let views: Vec<Entity<AnnotateTab>> = self
            .tabs
            .iter()
            .flat_map(Tab::bodies)
            .filter_map(|body| match body {
                Body::Annotate { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .collect();
        for view in views {
            view.update(cx, |tab, cx| tab.restyle(look.clone(), ui, cx));
        }
    }
}
