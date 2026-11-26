//! What is waiting to go to a remote, and the tab that shows it. M8.8.4.
//!
//! The model is `git::Ahead` and `git::Destination`, and neither knows
//! about windows. This is the other half — which project's commits,
//! which remote was chosen, which commit is opened and what its files
//! are, and whether a push is out at the moment.
//!
//! **The reads are one ask of the project's git thread**, answered on
//! the watch loop's clock like everything else git says: `git remote`,
//! `git symbolic-ref`, the upstream and `git log <upstream>..HEAD` are
//! one question, and a head that named a remote from one walk while
//! counting commits from another would be two truths free to disagree.
//!
//! **The write goes the same way**, and the thread re-reads after it —
//! so the list empties itself when the push lands and stays where it is
//! when git refuses, with git's own words in the notice. That also
//! means a push against a slow network holds that project's git thread
//! for as long as it takes: the status walks behind it queue. The
//! window does not, which is the part that matters, and the button says
//! it is going.

use std::collections::HashMap;

use gpui::AppContext as _;
use guild_session::ProjectId;

use crate::Root;
use crate::git::log::{Commit, FileChange};
use crate::git::{Ahead, Destination, GitError};

/// How many of the waiting commits are read.
///
/// The Log's own page. A branch a hundred commits above its upstream is
/// already unusual; two hundred is far past the point where a person is
/// reading the list rather than counting it.
pub(crate) const PAGE: usize = 200;

/// One project's push as the window holds it.
#[derive(Debug, Default)]
pub(crate) struct PushState {
    /// What git last said, or why it said nothing.
    pub ahead: Option<Result<Ahead, GitError>>,
    /// The remote chosen in the head, when a person has chosen one.
    ///
    /// A name `git remote` printed, never typing: `Ahead::destination`
    /// drops one that is no longer in the list rather than passing it
    /// on.
    pub chosen: Option<String>,
    /// An edited destination branch, validated before it reaches git.
    pub target: Option<String>,
    /// Which commits are inspected; none means the branch.
    pub selected: crate::push_dialog::Selection,
    /// The immutable commit inspection key whose files are shown.
    pub opened: Option<String>,
    /// Those files, once the thread has answered.
    pub files: HashMap<String, Result<Vec<FileChange>, GitError>>,
    /// A push has gone to the thread and nothing has come back yet.
    ///
    /// It is what greys the button and what makes it say *Pushing…* —
    /// this is the one write in the crate that takes seconds, and a
    /// button that looks pressable throughout is a second push queued
    /// behind the first.
    pub running: bool,
    /// What git said when it last refused a push — shown in Push Commits
    /// above its buttons, M8.3.6. Cleared when a push is sent.
    pub refused: Option<String>,
}

impl PushState {
    /// The commits waiting, or none while git has not answered.
    pub fn commits(&self) -> &[Commit] {
        match &self.ahead {
            Some(Ok(ahead)) => &ahead.commits,
            _ => &[],
        }
    }

    /// Where the push would go, or nothing when there is nowhere.
    pub fn destination(&self) -> Option<Destination> {
        match &self.ahead {
            Some(Ok(ahead)) => {
                let mut to = ahead.destination(self.chosen.as_deref())?;
                if let Some(target) = &self.target {
                    if !crate::push_dialog::valid_target(target) {
                        return None;
                    }
                    to.set_upstream |= *target != to.onto;
                    to.onto = target.clone();
                }
                Some(to)
            }
            _ => None,
        }
    }
}

/// Every project's push state.
///
/// **Declared by the seams commit, built by M8.8.4.** One entry per
/// project that has opened the tab: a repository nobody is pushing is
/// not asked about.
#[derive(Debug, Default)]
pub(crate) struct Push {
    states: HashMap<ProjectId, PushState>,
}

impl Push {
    /// One project's state, once its tab has been opened.
    pub(crate) fn of(&self, project: ProjectId) -> Option<&PushState> {
        self.states.get(&project)
    }

    /// One project's state, made if this is the first time.
    pub(crate) fn of_mut(&mut self, project: ProjectId) -> &mut PushState {
        self.states.entry(project).or_default()
    }

    /// Whether a project has ever opened the tab.
    pub(crate) fn knows(&self, project: ProjectId) -> bool {
        self.states.contains_key(&project)
    }

    /// A closed project takes its state with it.
    pub(crate) fn forget(&mut self, project: ProjectId) {
        self.states.remove(&project);
    }
}

impl Root {
    /// `cmd-shift-k`: Push Commits for the active project.
    ///
    /// **One window, brought forward rather than stacked.**
    ///
    /// **It opens whatever git will say.** A branch with nothing waiting
    /// opens the tab all the same, with an empty list and a button that
    /// cannot be pressed, and no *nothing to push* string, because the
    /// emptiness says it.
    pub(crate) fn open_push(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(project) = self.active_project else {
            return;
        };
        // **A window of its own** — M8.3.6. Asked every
        // time: what a push would send is true only until somebody commits.
        self.ask_push(project);
        // A refusal belongs to the window it was said in.
        self.push.of_mut(project).refused = None;
        self.push.of_mut(project).selected = Default::default();
        let root = cx.entity().downgrade();
        let theme = self.theme;
        let title = format!(
            "{} — {}",
            self.ui().text(crate::i18n::Text::PushDialogTitle),
            self.desk.project(project).map_or("", |p| p.name.as_str())
        );
        let dialog = self.push_dialog.clone();
        self.open_satellite(
            crate::windows::Satellite::Push,
            title,
            (1000.0, 620.0),
            move |_window, cx| {
                let view = cx.new(|cx| {
                    crate::view::push_dialog::PushDialog::new(root.clone(), project, theme, cx)
                });
                *dialog.borrow_mut() = Some(view.downgrade());
                view.into()
            },
            cx,
        );
    }

    /// The handler for `Push`. One binder of its own rather than a line
    /// in `bind_git`, so that this branch adds nothing to a chain five
    /// others are editing — the whole point of M8.8.0's split.
    pub(crate) fn bind_push(&self, root: gpui::Div, cx: &mut gpui::Context<Root>) -> gpui::Div {
        use gpui::prelude::*;
        root.on_action(cx.listener(|root, _: &crate::actions::Push, _window, cx| {
            root.open_push(cx);
        }))
    }

    /// Ask the project's git thread what a push would send.
    ///
    /// Never waits: the answer comes back through `poll_git` on the
    /// watch loop's clock, like everything else git says. The state is
    /// made here so that a second ask before the first lands does not
    /// make two.
    pub(crate) fn ask_push(&mut self, project: ProjectId) {
        if self.push.of(project).is_some_and(|s| s.running) {
            return;
        }
        let chosen = self.push.of(project).and_then(|s| s.chosen.clone());
        let target = self.push.of(project).and_then(|s| s.target.clone());
        let state = self.push.of_mut(project);
        state.ahead = None;
        state.opened = None;
        if let Some(worker) = self.status_workers.get(&project) {
            worker.push_read(chosen, target);
        }
    }

    /// A push read landed: the head, the list and the button all follow
    /// from it, and a push that was out is out no longer.
    pub(crate) fn push_landed(
        &mut self,
        project: ProjectId,
        ahead: Result<crate::git::Ahead, GitError>,
        cx: &mut gpui::Context<Self>,
    ) {
        // Only for a project that has opened the tab: the read is asked
        // for by the tab and by nothing else, so an entry made here
        // would be a state nobody wanted.
        if !self.push.knows(project) {
            return;
        }
        let state = self.push.of_mut(project);
        let was_running = state.running;
        state.ahead = Some(ahead);
        state.running = false;
        let shas: Vec<String> = state.commits().iter().map(|c| c.sha.clone()).collect();
        let shas: Vec<&str> = shas.iter().map(String::as_str).collect();
        state.selected.retain(&shas);
        let selected = crate::push_dialog::comparison(&state.selected.shas, &shas);
        let wanted = selected
            .clone()
            .filter(|sha| !state.files.contains_key(sha));
        state.opened = selected;
        // **A push that landed closes the window**; a
        // refused one leaves it up with git's words in it.
        let pushed = was_running && state.refused.is_none();
        if let Some(sha) = wanted
            && let Some(worker) = self.status_workers.get(&project)
        {
            worker.files(sha);
        }
        if pushed && let Some(handle) = self.satellite(crate::windows::Satellite::Push, cx) {
            let _ = handle.update(cx, |_, window, _| window.remove_window());
        }
        self.notify_push_dialog(cx);
        cx.notify();
    }

    /// Push Commits reads this state; tell it something changed.
    ///
    /// **By id, never by `update`**: the ask often comes from the dialog
    /// itself — its Push button calls into `Root` — and updating an entity
    /// inside its own update is a panic (measured, 2026-09-16).
    pub(crate) fn notify_push_dialog(&self, cx: &mut gpui::App) {
        if let Some(dialog) = self.push_dialog.borrow().as_ref() {
            cx.notify(dialog.entity_id());
        }
    }

    /// Inspection selection; it never limits the branch's Push action.
    pub(crate) fn push_select(
        &mut self,
        project: ProjectId,
        sha: Option<&str>,
        toggle: bool,
        extend: bool,
        cx: &mut gpui::App,
    ) {
        let wanted = {
            let state = self.push.of_mut(project);
            let shas: Vec<String> = state.commits().iter().map(|c| c.sha.clone()).collect();
            let shas: Vec<&str> = shas.iter().map(String::as_str).collect();
            match sha {
                Some(sha) => state.selected.click(sha, &shas, toggle, extend),
                None => state.selected = Default::default(),
            }
            state.opened = crate::push_dialog::comparison(&state.selected.shas, &shas);
            state
                .opened
                .clone()
                .filter(|key| !state.files.contains_key(key))
        };
        if let Some(sha) = wanted
            && let Some(worker) = self.status_workers.get(&project)
        {
            worker.files(sha);
        }
        self.notify_push_dialog(cx);
    }

    /// *Push* in Push Commits: what the dialog's project would send, with
    /// the tags the foot chose.
    pub(crate) fn push_from_dialog(
        &mut self,
        project: ProjectId,
        tags: crate::git::PushTags,
        run_hooks: bool,
        cx: &mut gpui::App,
    ) {
        let Some(state) = self.push.of(project) else {
            return;
        };
        if !crate::push_dialog::can_push(
            state.commits().len(),
            state.destination().is_some(),
            state.running,
        ) {
            return;
        }
        let Some(to) = state.destination() else {
            return;
        };
        {
            let state = self.push.of_mut(project);
            state.running = true;
            state.refused = None;
        }
        self.run_git_write(
            project,
            crate::git::Write::push_with_hooks(&to, tags, run_hooks),
        );
        self.notify_push_dialog(cx);
    }

    /// A remote chosen in Push Commits: the list is read again against it.
    pub(crate) fn push_remote_for(
        &mut self,
        project: ProjectId,
        remote: String,
        cx: &mut gpui::App,
    ) {
        if self.push.of(project).is_some_and(|s| s.running) {
            return;
        }
        {
            let state = self.push.of_mut(project);
            state.chosen = Some(remote);
            state.target = None;
            state.selected = Default::default();
            state.refused = None;
        }
        self.ask_push(project);
        self.notify_push_dialog(cx);
    }

    /// An edited target re-reads the commits against that destination.
    pub(crate) fn push_target_for(
        &mut self,
        project: ProjectId,
        target: String,
        cx: &mut gpui::App,
    ) {
        if !crate::push_dialog::valid_target(&target) {
            return;
        }
        let state = self.push.of_mut(project);
        if state.running {
            return;
        }
        state.target = Some(target);
        state.selected = Default::default();
        state.refused = None;
        self.ask_push(project);
        self.notify_push_dialog(cx);
    }

    /// git refused a push of `project`: its words go into Push Commits.
    pub(crate) fn push_refused(&mut self, project: ProjectId, words: String, cx: &mut gpui::App) {
        if self.push.knows(project) {
            self.push.of_mut(project).refused = Some(words);
            self.notify_push_dialog(cx);
        }
    }

    /// A commit's files landed. The Log reads the same answer; Push
    /// Commits is told, since nothing else would redraw it.
    pub(crate) fn push_files_landed(
        &mut self,
        project: ProjectId,
        sha: &str,
        files: &Result<Vec<FileChange>, GitError>,
        cx: &mut gpui::App,
    ) {
        if !self.push.knows(project) {
            return;
        }
        let state = self.push.of_mut(project);
        if state.opened.as_deref() == Some(sha) || !state.files.contains_key(sha) {
            state.files.insert(
                sha.to_owned(),
                match files {
                    Ok(files) => Ok(files.clone()),
                    Err(err) => Err(err.clone()),
                },
            );
        }
        self.notify_push_dialog(cx);
    }
}
