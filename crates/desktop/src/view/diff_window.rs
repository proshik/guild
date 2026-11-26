//! A commit's file as a diff in its own window — M8.3.6, decision 7.
//!
//! The title is the file with its full path, the toolbar steps
//! between the changes and between the commit's files, and the pencil
//! opens the file in the main window. The body is [`DiffTab`] as it is,
//! told to show the stepping in its own toolbar so there is one.

use std::path::PathBuf;
use std::time::Duration;

use gpui::prelude::*;
use gpui::{Context, Entity, FocusHandle, Subscription, Task, WeakEntity, Window, div};
use guild_session::ProjectId;

use crate::actions::{NextChange, NextDiffFile, PreviousChange, PreviousDiffFile};
use crate::diff_tab::{DiffEvent, DiffTab};
use crate::windows::Satellite;
use crate::{Root, theme};

/// What the window shows: a commit, its files in the tree's order, and
/// which of them is open.
#[derive(Debug, Clone)]
pub(crate) struct Showing {
    pub project: ProjectId,
    pub folder: PathBuf,
    pub sha: String,
    pub files: Vec<PathBuf>,
    pub index: usize,
}

pub(crate) struct DiffWindow {
    root: WeakEntity<Root>,
    showing: Showing,
    theme: theme::Theme,
    tab: Entity<DiffTab>,
    focus: FocusHandle,
    focused: bool,
    retitle: bool,
    _heard: Subscription,
    _poll: Task<()>,
}

impl DiffWindow {
    pub(crate) fn new(
        root: WeakEntity<Root>,
        showing: Showing,
        theme: theme::Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let tab = Self::tab_for(&root, &showing, cx);
        let heard = Self::hear(&tab, cx);
        // Not the watch loop's: that one is the main window's. A diff
        // reads its blobs on threads and has to be asked what landed.
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let alive = this.update(cx, |this, cx| {
                    this.tab.update(cx, |tab, cx| tab.poll(cx));
                });
                if alive.is_err() {
                    break;
                }
            }
        });
        Self {
            root,
            showing,
            theme,
            tab,
            focus: cx.focus_handle(),
            focused: false,
            retitle: true,
            _heard: heard,
            _poll: poll,
        }
    }

    fn tab_for(
        root: &WeakEntity<Root>,
        showing: &Showing,
        cx: &mut Context<Self>,
    ) -> Entity<DiffTab> {
        let (look, ui, mode) = root
            .upgrade()
            .map(|root| {
                let root = root.read(cx);
                (root.file_look(), root.ui(), root.settings.diff_view)
            })
            .expect("the main window outlives its satellites");
        let path = showing
            .files
            .get(showing.index)
            .cloned()
            .unwrap_or_default();
        let key = showing.sha.clone();
        let folder = showing.folder.clone();
        cx.new(|cx| {
            let mut tab = DiffTab::open_inspection(&folder, path, key, mode, look, ui, cx);
            tab.set_inspection_files(showing.files.clone(), showing.index, cx);
            tab
        })
    }

    fn hear(tab: &Entity<DiffTab>, cx: &mut Context<Self>) -> Subscription {
        cx.subscribe(tab, |this, _tab, event: &DiffEvent, cx| match event {
            DiffEvent::StepFile { forward } => this.step(*forward, cx),
            DiffEvent::SelectFile { index, key } => {
                if key == &this.showing.sha && *index < this.showing.files.len() {
                    this.showing.index = *index;
                    this.reopen(cx);
                }
            }
            DiffEvent::OpenAt { path, line } => {
                let project = this.showing.project;
                let (path, line) = (path.clone(), *line);
                if let Some(root) = this.root.upgrade() {
                    root.update(cx, |root, cx| root.open_in_main(project, path, line, cx));
                }
            }
            _ => {}
        })
    }

    /// Show another commit, or another file of it — a second click in Push
    /// Commits reuses this window.
    pub(crate) fn show(&mut self, showing: Showing, cx: &mut Context<Self>) {
        self.showing = showing;
        self.reopen(cx);
    }

    fn step(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(index) =
            crate::diff_window::stepped(self.showing.index, self.showing.files.len(), forward)
        else {
            return;
        };
        self.showing.index = index;
        self.reopen(cx);
    }

    /// The palette the window is drawn with.
    #[cfg(test)]
    pub(crate) fn theme(&self) -> theme::Theme {
        self.theme
    }

    /// The diff the window shows.
    #[cfg(test)]
    pub(crate) fn tab_for_test(&self) -> &Entity<DiffTab> {
        &self.tab
    }

    /// Draw with the main window's palette in effect, its diff included.
    pub(crate) fn set_theme(
        &mut self,
        theme: theme::Theme,
        look: crate::file_tab::Look,
        ui: crate::i18n::Ui,
        cx: &mut Context<Self>,
    ) {
        self.theme = theme;
        self.tab.update(cx, |tab, cx| tab.restyle(look, ui, cx));
        cx.notify();
    }

    fn reopen(&mut self, cx: &mut Context<Self>) {
        let style = self.root.upgrade().map(|root| {
            let root = root.read(cx);
            (root.file_look(), root.ui())
        });
        let showing = &self.showing;
        if let Some(path) = showing.files.get(showing.index) {
            self.tab.update(cx, |tab, cx| {
                tab.show_inspection(&showing.folder, path.clone(), showing.sha.clone(), cx);
                if let Some((look, ui)) = style {
                    tab.restyle(look, ui, cx);
                }
                tab.set_inspection_files(showing.files.clone(), showing.index, cx);
            });
        }
        self.retitle = true;
        cx.notify();
    }

    fn title(&self) -> String {
        let path = self
            .showing
            .files
            .get(self.showing.index)
            .cloned()
            .unwrap_or_default();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let full = self.showing.folder.join(&path);
        let parent = full
            .parent()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        format!("{name} ({parent})")
    }
}

impl gpui::Render for DiffWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        if self.retitle {
            self.retitle = false;
            window.set_window_title(&self.title());
        }
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, _| {
                root.remember_satellite(Satellite::Diff, window)
            });
        }
        crate::view::satellite::frame(&self.focus, self.theme, window, cx)
            .key_context("diff-window")
            .on_action(cx.listener(|this, _: &NextChange, _window, cx| {
                this.tab.update(cx, |tab, cx| tab.step_change(true, cx));
            }))
            .on_action(cx.listener(|this, _: &PreviousChange, _window, cx| {
                this.tab.update(cx, |tab, cx| tab.step_change(false, cx));
            }))
            .on_action(cx.listener(|this, _: &NextDiffFile, _window, cx| this.step(true, cx)))
            .on_action(cx.listener(|this, _: &PreviousDiffFile, _window, cx| this.step(false, cx)))
            .child(div().flex_1().min_h_0().child(self.tab.clone()))
    }
}

#[cfg(test)]
mod tests;
