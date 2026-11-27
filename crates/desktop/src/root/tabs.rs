//! Opening, closing and moving between tabs.

use gpui::Context;
use gpui::Window;

use crate::Body;
use crate::Root;
use crate::Tab;
use crate::TabId;
use crate::closed::ClosedTab;
use crate::root::files::CloseTarget;
use guild_session::Conversation;
use guild_session::OpenTab;
use guild_session::PaneId;
use guild_session::ProjectId;
use guild_session::Runs;
use guild_session::Seat;
use guild_session::SessionId;
use guild_session::SessionRecord;
use guild_session::Shows;
use guild_session::layout::{Axis, Direction};
use guild_session::panes::Closed;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

/// The tabs that can come back: a file that is no longer on disk is
/// dropped from the list and named, and the index of the one in front
/// moves with the others — or goes, if it was the one dropped. A diff
/// tab is kept whatever the disk says: a deleted file's diff is exactly
/// what one wants to see.
///
/// **The rule is a pane's** — M12.8, `OpenTab::keeping`: a divided tab
/// loses the pane whose file is gone and keeps the others, and goes only
/// when none is left. Each file named says which it cost, so the log
/// does not say a tab was lost that came back without one pane.
pub(crate) fn restorable(
    tabs: &[OpenTab],
    active: Option<usize>,
    root: &Path,
) -> (Vec<OpenTab>, Option<usize>, Vec<(PathBuf, Lost)>) {
    let mut kept = Vec::with_capacity(tabs.len());
    let mut dropped = Vec::new();
    let mut front = None;
    let gone = |shows: &Shows| matches!(shows, Shows::File(path) if !root.join(path).is_file());
    for (index, tab) in tabs.iter().enumerate() {
        let files: Vec<PathBuf> = tab
            .panes_or_one()
            .bodies()
            .into_iter()
            .filter_map(|(_, shows)| match shows {
                Shows::File(path) if gone(shows) => Some(path.clone()),
                _ => None,
            })
            .collect();
        let tab = tab
            .without_native_review()
            .and_then(|tab| tab.keeping(|shows| !gone(shows)));
        let lost = if tab.is_some() { Lost::Pane } else { Lost::Tab };
        dropped.extend(files.into_iter().map(|file| (file, lost)));
        let Some(tab) = tab else {
            continue;
        };
        if active == Some(index) {
            front = Some(kept.len());
        }
        kept.push(tab);
    }
    (kept, front, dropped)
}

/// What a file gone from the disk cost a restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lost {
    /// The tab: nothing it showed is left.
    Tab,
    /// One pane of a divided tab, which comes back without it.
    Pane,
}

/// What one pane's body is written down as, or `None` for a body that is
/// not written down at all.
fn shows_of(body: &Body) -> Option<Shows> {
    Some(match body {
        Body::Settings { .. } => Shows::Settings,
        Body::Process { run } => Shows::Process { run: *run },
        // **A tab under a dependency's root is not written down.**
        // `Shows::File` is a path relative to the project, and a restart
        // would read a crate's `src/lib.rs` as this repository's — the
        // same file name, a different file. The roots survive in
        // `ProjectState.external_roots`, so the jump can be made again;
        // the tab is a place a person went to read, not work they left
        // behind. M8.8.3. A file outside the project, opened from a
        // terminal's link, is the same kind of place and is not written
        // down either: its path is relative to `/`, not to the project.
        Body::File {
            origin: crate::Origin::External(_) | crate::Origin::Outside,
            ..
        } => return None,
        Body::File { path, .. } => Shows::File(path.clone()),
        // A proposal's new side exists only in this window's memory — the
        // agent sent it and no revision holds it. Restoring such a tab
        // would show a diff against nothing, so it is not written down.
        // M8.6.
        Body::Diff {
            base: guild_session::Base::Proposal,
            ..
        } => return None,
        Body::Diff { path, base, .. } => Shows::Diff {
            path: path.clone(),
            base: base.clone(),
        },
        Body::Annotate { path, .. } => Shows::Annotate(path.clone()),
        _ => Shows::Session(body.session()?),
    })
}

/// What a new tab of `tab` runs — `NewSession`'s, resolved when it is
/// invoked from the current settings and the agents installed then
/// (`Root::runs_for_default_tab`). Every harness runs in a terminal.
fn runs_of(tab: guild_session::DefaultTab) -> Runs {
    use guild_session::{DefaultTab, SessionMode};
    match tab {
        DefaultTab::Claude => Runs::Claude(SessionMode::Terminal),
        DefaultTab::OpenCode => Runs::OpenCode(SessionMode::Terminal),
        DefaultTab::Codex => Runs::Codex(SessionMode::Terminal),
        DefaultTab::Terminal => Runs::Shell,
    }
}

/// Raw input is needed only for the repeat bit, which GPUI actions do not carry.
#[derive(Default)]
pub(crate) struct TabShortcuts {
    pending: std::rc::Rc<std::cell::Cell<Option<bool>>>,
    ear: Option<gpui::Subscription>,
    last_switch: Option<std::time::Instant>,
}

impl Root {
    pub(crate) fn hear_tab_shortcuts(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.tab_shortcuts.ear.is_some() {
            return;
        }
        let owner = window.window_handle();
        let pending = self.tab_shortcuts.pending.clone();
        self.tab_shortcuts.ear = Some(cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() != owner {
                return;
            }
            pending.set(None);
            if event.input_preference != gpui::InputPreference::KeyBindings
                // Standalone modifiers resolve on release, without a KeyDown.
                || matches!(event.keystroke.key.as_str(), "shift" | "control" | "alt" | "platform" | "function")
                || window.pending_input().is_some()
                || !event
                    .context_stack
                    .iter()
                    .any(|context| context.contains("guild"))
            {
                return;
            }
            let (bindings, partial) = cx
                .key_bindings()
                .borrow()
                .bindings_for_input(std::slice::from_ref(&event.keystroke), &event.context_stack);
            if partial {
                return;
            }
            let Some(binding) = bindings.first() else {
                return;
            };
            let direction = if binding.action().as_any().is::<NextTab>() {
                true
            } else if binding.action().as_any().is::<PreviousTab>() {
                false
            } else {
                return;
            };
            // Consume the binding, then let Root's raw capture handler read
            // is_held. Resolving the existing keymap preserves overrides/nulls.
            pending.set(Some(direction));
            cx.stop_propagation();
        }));
    }

    pub(crate) fn tab_shortcut_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(forward) = self.tab_shortcuts.pending.take() else {
            return false;
        };
        let now = cx.background_executor().now();
        let too_soon = self
            .tab_shortcuts
            .last_switch
            .is_some_and(|last| now.duration_since(last) < std::time::Duration::from_millis(125));
        if !event.is_held || !too_soon {
            self.tab_shortcuts.last_switch = Some(now);
            self.step_tab(forward, cx);
        }
        // Excess repeats are consumed, never queued or sent to the editor.
        cx.stop_propagation();
        true
    }

    /// Confirm an accepted drop even if the live pass already placed the tab.
    pub(crate) fn land_tab(
        &mut self,
        id: crate::TabId,
        from: gpui::Point<gpui::Pixels>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.tabs.iter().any(|tab| tab.id == id) {
            self.tab_landing = Some((id, crate::view::tabs::landing::Landing::new(from)));
            cx.notify();
        }
    }

    /// Open a tab in a project.
    ///
    /// **And the tab takes the keys at the next frame.** A terminal takes
    /// the focus on render only when nobody has it, and from a Settings
    /// tab, or after a click on one of its buttons, the window's own
    /// handle has it — so `cmd-t` opened a session whose prompt nothing
    /// typed reached. Every caller here is a person's gesture; a session
    /// the command line starts goes through `open_tab_with` and leaves
    /// the keys where they are. Seen in M5.12's window check 12.
    pub(crate) fn open_tab(&mut self, project: ProjectId, runs: Runs, cx: &mut Context<Root>) {
        if self.open_tab_with(project, runs, None, true, cx).is_some() {
            self.focusing_front = true;
        }
    }

    /// Open a new harness tab the way a person's gesture does — the menu's
    /// rows and the **+**'s default tab.
    ///
    /// Refuses an OpenCode whose installed CLI is below its floor (that is,
    /// not V2) with a notice that says so: an opencode before V2 launches
    /// but never reports status, which is a harness offered as working when
    /// it is not. Claude, Codex and a shell have no floor and open at once;
    /// restoring an existing session does not come through here, so a
    /// tab already open still comes back.
    pub(crate) fn open_harness_tab(
        &mut self,
        project: ProjectId,
        runs: Runs,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        // A harness not on the `PATH` would open a tab that dies at once;
        // every gesture that starts one meets here, so each says how to
        // install it instead (GLD-081).
        if let Some(kind) = Self::harness_missing(runs) {
            return self.say_missing(kind, cx);
        }
        if let Runs::OpenCode(_) = runs
            && Self::opencode_floor_missed()
        {
            let text = self
                .ui()
                .text(crate::i18n::Text::OpenCodeNeedsV2)
                .to_owned();
            self.warn(text);
            cx.notify();
            return;
        }
        self.open_tab(project, runs, cx);
        self.refocus(window, cx);
    }

    /// Whether the opencode on this machine is below the floor Guild's status
    /// work needs — probed once per window, then remembered.
    ///
    /// The probe answers a `--version` subprocess, so it is a few hundred
    /// milliseconds of somebody's click; asked once and cached, it cannot
    /// become that per click. `None` (unknown) refuses the same as `true`
    /// would: an opencode whose version cannot be told is not offered as if
    /// it worked.
    pub(crate) fn opencode_floor_missed() -> bool {
        static MISSED: OnceLock<bool> = OnceLock::new();
        *MISSED.get_or_init(|| {
            let path = guild_session::login_path::path_or_inherited();
            guild_session::HarnessKind::OpenCode
                .floor_miss(&path)
                .is_some()
        })
    }

    /// The `Runs` a harness picked from the add button runs — M14. Claude
    /// and the other harnesses always run in terminals.
    pub(crate) fn runs_for_pick(&self, kind: guild_session::HarnessKind) -> Runs {
        use guild_session::{HarnessKind, SessionMode};
        match kind {
            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
        }
    }

    /// The **+** button and start screen use the same current default as
    /// New Session. Every harness runs in a terminal.
    ///
    /// **A default nobody chose is resolved at the press** — GLD-081's
    /// review: the agents installed are asked now, not read off a picture
    /// that may still be waiting for its first probe or be ten seconds
    /// old (`root::availability`).
    pub(crate) fn runs_for_default_tab(&mut self, cx: &mut Context<Root>) -> Runs {
        runs_of(self.default_tab_now(cx))
    }

    /// Move one visible tab before or after another — M14, drag-and-drop in
    /// the strip, and since M12.8 across the areas: a tab dragged over
    /// another area's tab moves into that area beside it (decision 8,
    /// `stage::reorder`), and comes forward there with the keys.
    ///
    /// Only tabs of the active seat move, and a move that lands back where
    /// the tab was is left alone so a drag that ends where it began does
    /// not dirty the state. The result is the tab order, which a restart
    /// shows: `Root.tabs` is part of [`crate::ProjectState`].
    pub(crate) fn reorder_tab(
        &mut self,
        dragged: crate::TabId,
        target: crate::TabId,
        before: bool,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let seat = self.active_seat();
        let area_of = |id: crate::TabId| {
            self.tabs
                .iter()
                .find(|tab| tab.id == id && Some(tab.seat) == seat)
                .map(|tab| tab.area)
        };
        let (Some(_), Some(area)) = (area_of(dragged), area_of(target)) else {
            return;
        };
        self.move_tab_to_area(dragged, area, Some((target, before)), window, cx);
    }

    /// Open a tab for a new session, asked `first` when it starts, and
    /// brought to the front only when `front` says so — M13.
    ///
    /// **`front` is false for what the command line starts in the
    /// background.** An agent that starts three sessions in three worktrees
    /// must not pull the person out of what they are doing three times; the
    /// column's dots are where those sessions show themselves. A person's
    /// own new tab is always in front, which is what `open_tab` passes.
    pub(crate) fn open_tab_with(
        &mut self,
        project: ProjectId,
        runs: Runs,
        first: Option<String>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> Option<guild_session::SessionId> {
        self.open_tab_with_selection(project, runs, first, front, None, cx)
    }

    /// Workflow choices are saved before the same native record/PTY path opens.
    pub(crate) fn open_tab_with_selection(
        &mut self,
        project: ProjectId,
        runs: Runs,
        first: Option<String>,
        front: bool,
        selection: Option<guild_session::harness::LaunchSelection>,
        cx: &mut Context<Root>,
    ) -> Option<guild_session::SessionId> {
        let seat = self.seat_for_project(project);
        self.open_tab_from(seat, runs, first, front, selection, true, cx)
    }

    /// [`Self::open_tab_with_selection`] in a seat named by the caller,
    /// which need not be the one in front — GLD-058: a command's session
    /// behind the person's work goes into its own workspace without that
    /// workspace being made active, so nothing of the person's place moves
    /// and no save writes a workspace from tabs it has not restored.
    pub(crate) fn open_tab_in_seat(
        &mut self,
        seat: Seat,
        runs: Runs,
        first: Option<String>,
        front: bool,
        selection: Option<guild_session::harness::LaunchSelection>,
        cx: &mut Context<Root>,
    ) -> Option<guild_session::SessionId> {
        self.open_tab_from(seat, runs, first, front, selection, true, cx)
    }

    /// A tab in the root's own folder, never in the directory a shell in
    /// front has moved to — a project group folder's `+` (GLD-055): its
    /// session runs in the group's folder, whatever the terminal did.
    pub(crate) fn open_tab_in_root(
        &mut self,
        project: ProjectId,
        runs: Runs,
        cx: &mut Context<Root>,
    ) -> Option<guild_session::SessionId> {
        let seat = self.seat_for_project(project);
        self.open_tab_from(seat, runs, None, true, None, false, cx)
    }

    #[allow(clippy::too_many_arguments)]
    fn open_tab_from(
        &mut self,
        seat: Seat,
        runs: Runs,
        first: Option<String>,
        front: bool,
        selection: Option<guild_session::harness::LaunchSelection>,
        inherit_shell_cwd: bool,
        cx: &mut Context<Root>,
    ) -> Option<guild_session::SessionId> {
        let project = seat.project;
        let cwd = self.desk.project(project).map(|p| p.path.clone())?;
        // A project group's folder pins its tabs to itself, from every `+`.
        let cwd = if inherit_shell_cwd
            && matches!(runs, Runs::Shell)
            && self.active_seat() == Some(seat)
            && !self.desk.is_project_group_root(project)
        {
            match self.active_body() {
                Some(crate::Body::Terminal { view, .. }) => view
                    .read(cx)
                    .live_shell_cwd()
                    .filter(|path| path.is_dir())
                    .unwrap_or(cwd),
                _ => cwd,
            }
        } else {
            cwd
        };

        let title = self
            .desk
            .project(project)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "session".to_owned());

        let mut record = SessionRecord::new(title, seat, cwd, runs);
        record.launch_selection = selection;
        let conversation = runs
            .is_agent()
            .then(|| Conversation::New(record.id.to_string()));
        Some(self.open_record(project, record, conversation, first, front, cx))
    }

    /// Open a tab for a record made by the caller, with the conversation it
    /// runs — `open_tab_with`'s second half, and the start screen's when it
    /// resumes a session nobody has a record of (M14.5): that record carries
    /// the conversation's own id, which `open_tab_with` would not.
    pub(crate) fn open_record(
        &mut self,
        project: ProjectId,
        record: SessionRecord,
        conversation: Option<Conversation>,
        first: Option<String>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> SessionId {
        self.note_place(cx);
        self.history.suspended += 1;
        let result =
            self.open_record_without_history(project, record, conversation, first, front, cx);
        self.history.suspended -= 1;
        result
    }

    fn open_record_without_history(
        &mut self,
        project: ProjectId,
        record: SessionRecord,
        conversation: Option<Conversation>,
        first: Option<String>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> SessionId {
        let mut record = record;
        record
            .launch_permission
            .get_or_insert(self.settings.permission_mode);
        // The record's own seat: the one in front unless its maker named
        // another (`open_tab_in_seat`).
        if record.workspace.is_nil() {
            record.workspace = self.active_workspace;
        }
        let seat = Seat {
            workspace: record.workspace,
            project,
        };
        let (runs, cwd) = (record.runs, record.cwd.clone());
        let id = self.desk.add_session(record);

        // A tab runs in the directory of **its own** project. While the
        // directory came from current_dir, everything ran in the same
        // place regardless of what was being worked on.
        let (ide, hooks) = (self.ide_port(), self.hook_launch());
        let body = Body::open(
            id,
            runs,
            &cwd,
            conversation,
            &self.session_launch_settings(id),
            self.theme,
            ide,
            hooks,
            first,
            self.desk
                .session(id)
                .and_then(|record| record.launch_selection.as_ref()),
            &self.start_size,
            cx,
        );

        let tab = self.adopt(seat, project, body, cx);
        if front {
            self.active_tab = Some(tab);
        }
        self.save();
        cx.notify();
        id
    }

    /// Open a tab for a session that already exists.
    ///
    /// A session that cannot be continued is forgotten rather than
    /// revived: its tab would open dead — `claude` exits at once, and the
    /// person sees an empty rectangle instead of an explanation.
    pub(crate) fn restore_tab(&mut self, id: SessionId, cx: &mut Context<Root>) {
        // A session already in a tab — a command's, started behind the
        // person's work before its workspace was restored (GLD-058) — is
        // not started a second time on the same conversation.
        if self.tab_holding(id).is_some() {
            return;
        }
        let Some(record) = self.desk.session(id) else {
            return;
        };
        // A closed record a saved strip still names — a task stopped in a
        // workspace not shown — stays closed: a restart reopens what was
        // open, and nothing else.
        if !record.open {
            return;
        }
        let (runs, project, workspace, cwd) = (
            record.runs,
            record.project,
            record.workspace,
            record.cwd.clone(),
        );

        // A shell has no continuation: the process did not survive the
        // restart, so a new one opens in the same directory. That is
        // honest — there is nothing to pretend with. An agent continues
        // its conversation by its own harness's id — `continue_or_forget`,
        // which a reopen asks too.
        let Some(conversation) = self.continue_or_forget(id) else {
            return;
        };
        let (ide, hooks) = (self.ide_port(), self.hook_launch());
        // A session coming back is never asked its first prompt again:
        // that turn is already in its transcript.
        let body = Body::open(
            id,
            runs,
            &cwd,
            conversation,
            &self.session_launch_settings(id),
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

        self.desk.reopen_session(id);
        self.closed.forget(id);
        self.adopt(Seat { workspace, project }, project, body, cx);
    }

    /// Write each project's tab list and the one in front, the way the
    /// panels are written: on every change, so that a crash loses
    /// nothing. Skipped for a project whose file already says the same.
    pub(crate) fn store_tabs(&self) {
        self.store_tabs_of(self.active_workspace);
    }

    /// [`Self::store_tabs`] for one workspace, in front or not. Only a
    /// workspace this run has restored: another's tabs are on disk alone.
    fn store_tabs_of(&self, workspace: guild_session::WorkspaceId) {
        let first = self.first_workspace();
        for project in self.desk.projects_of(workspace) {
            if !project.path.is_dir() {
                continue;
            }
            let seat = Seat {
                workspace,
                project: project.id,
            };
            // A seat the stage never heard of has no tabs, and one area
            // is what it would be read back as.
            let areas = self
                .stage
                .areas(seat)
                .cloned()
                .unwrap_or_else(guild_session::Areas::one);
            let mine: Vec<(TabId, guild_session::AreaId, OpenTab)> = self
                .tabs
                .iter()
                .filter(|tab| tab.seat == seat)
                .filter_map(|tab| {
                    // Every pane, the division with its fractions, and
                    // the focus — M12.8. A pane that is not written down
                    // loses its place in the tree (the ruling of
                    // 2026-09-29), an empty pane is a leaf with no entry
                    // (decision 13), and a tab with nothing left is not
                    // written. `shows` mirrors the focused pane for an
                    // older build (decision 5).
                    let modes = tab
                        .panes
                        .bodies()
                        .into_iter()
                        .filter_map(|(id, body)| match body {
                            Body::File {
                                presentation: Some(p),
                                ..
                            } => Some((id, p.mode())),
                            _ => None,
                        })
                        .collect();
                    let written = OpenTab::from_panes_with_modes(tab.panes.map(shows_of)?, modes)?;
                    // A tab whose area the seat no longer has — which
                    // nothing should make — is written into the focused
                    // one rather than lost.
                    let area = if areas.contains(tab.area) {
                        tab.area
                    } else {
                        areas.focused()
                    };
                    Some((tab.id, area, written))
                })
                .collect();
            let mut saved = guild_session::SavedAreas {
                areas: areas.clone(),
                strips: Default::default(),
            };
            let order: Vec<(TabId, guild_session::AreaId)> =
                mine.iter().map(|(id, area, _)| (*id, *area)).collect();
            for area in areas.ids() {
                let ids = crate::stage::strip(&order, area);
                // The tab in front when it is this area's, and otherwise
                // the one the person left here. It was only the first, so
                // every project but the one in front was written back
                // with no front at all, and a restart brought up its last
                // tab.
                let front = self
                    .active_tab
                    .filter(|active| ids.contains(active))
                    .or_else(|| self.stage.remembered(area, &ids));
                let active = front.and_then(|front| ids.iter().position(|id| *id == front));
                let tabs: Vec<OpenTab> = mine
                    .iter()
                    .filter(|(_, own, _)| *own == area)
                    .map(|(_, _, tab)| tab.clone())
                    .collect();
                *saved.strip_mut(area) = guild_session::AreaState { tabs, active };
            }

            let mut state = guild_session::project_state::load(&project.path);
            let mut origins = state.process_origins.clone();
            for (run, snapshot) in &self.processes.runs {
                if !self
                    .tabs
                    .iter()
                    .filter(|t| t.project == project.id)
                    .flat_map(Tab::bodies)
                    .any(|body| matches!(body, Body::Process {run: open} if open == run))
                {
                    continue;
                }
                if let Some(origin) = snapshot
                    .value
                    .as_ref()
                    .and_then(|v| v["origin"]["session"].as_str())
                    .and_then(|s| s.parse().ok())
                    .or(snapshot.origin_hint)
                {
                    origins.insert(*run, origin);
                }
            }
            let old = state.seat(workspace, first);
            let new = guild_session::SeatState::from_areas(saved, old.expanded.clone());
            // Placement hints belong only to open views, including saved seats
            // not currently restored in this window. Runtime history is separate.
            state.set_seat(workspace, first, new.clone());
            let mut open = std::collections::HashSet::new();
            for seat in state.seats.values() {
                for strip in seat.areas_or_one().strips.values() {
                    for tab in &strip.tabs {
                        for (_, body) in tab.panes_or_one().bodies() {
                            if let Shows::Process { run } = body {
                                open.insert(*run);
                            }
                        }
                    }
                }
            }
            origins.retain(|run, _| open.contains(run));
            if new == old && origins == state.process_origins {
                continue;
            }
            state.process_origins = origins;
            if let Err(err) = guild_session::project_state::save(&project.path, &state) {
                tracing::warn!(project = %project.path.display(), %err, "the tabs were not saved");
            }
        }
    }

    /// Write down a tab a command opened behind the person's work —
    /// GLD-058. A restored workspace is written from memory, as on every
    /// change. One this run has not restored has nothing in memory to write
    /// from: the session's tab is added to what its own project's file
    /// says, at the end of the seat's focused area, and no other project's
    /// file is touched — they are read, whole, when the workspace is
    /// visited, and the tab already running takes its place there
    /// (`restore_shows`).
    ///
    /// A seat written before tab lists — open records and nothing else —
    /// has its strip written first, as the restore would show it. And a
    /// file that cannot be read to build on (`load_for_change`) is left
    /// alone: the tab stays in memory, and the visit writes it.
    pub(crate) fn store_behind(&self, seat: Seat, session: SessionId) {
        if self.restored_workspaces.contains(&seat.workspace) {
            self.store_tabs_of(seat.workspace);
            return;
        }
        let shows = Shows::Session(session);
        self.change_behind(seat, |saved, old| {
            if saved
                .strips
                .values()
                .any(|strip| strip.tabs.iter().any(|tab| tab.shows == shows))
            {
                return false;
            }
            let focused = saved.areas.focused();
            let strip = saved.strip_mut(focused);
            // Neither tabs nor areas: written before tab lists, and its
            // open records are what the restore shows (`restore_tabs`).
            if old.tabs.is_empty() && old.areas.is_none() {
                strip.tabs.extend(
                    Self::open_before_tab_lists(&self.desk, seat)
                        .filter(|id| *id != session)
                        .map(|id| OpenTab::showing(Shows::Session(id))),
                );
            }
            strip.tabs.push(OpenTab::showing(shows));
            true
        });
    }

    /// Take a closed session's tab out of its seat's file, when the seat is
    /// of a workspace this run has not restored — a task stopped there by
    /// `guild agent stop` — keeping every other tab, area and front. A
    /// restored workspace writes its seats from memory, which no longer has
    /// the tab.
    pub(crate) fn forget_behind(&self, seat: Seat, session: SessionId) {
        if seat.workspace == self.active_workspace
            || self.restored_workspaces.contains(&seat.workspace)
        {
            return;
        }
        let shows = Shows::Session(session);
        self.change_behind(seat, |saved, _| {
            let mut changed = false;
            for strip in saved.strips.values_mut() {
                while let Some(at) = strip.tabs.iter().position(|tab| tab.shows == shows) {
                    strip.tabs.remove(at);
                    strip.active = match strip.active {
                        Some(front) if front > at => Some(front - 1),
                        Some(front) if front == at => {
                            (!strip.tabs.is_empty()).then(|| at.min(strip.tabs.len() - 1))
                        }
                        other => other,
                    };
                    changed = true;
                }
            }
            changed
        });
    }

    /// Change one seat's saved areas in its project's file, from what the
    /// file says, and write it when `change` says it changed something.
    fn change_behind(
        &self,
        seat: Seat,
        change: impl FnOnce(&mut guild_session::SavedAreas, &guild_session::SeatState) -> bool,
    ) {
        let Some(path) = self.desk.project(seat.project).map(|p| p.path.clone()) else {
            return;
        };
        let Some(mut state) = guild_session::project_state::load_for_change(&path) else {
            tracing::info!(project = %path.display(), "the folder's state cannot be read; its tabs are written when it is shown");
            return;
        };
        let first = self.first_workspace();
        let old = state.seat(seat.workspace, first);
        let mut saved = old.areas_or_one();
        if !change(&mut saved, &old) {
            return;
        }
        state.set_seat(
            seat.workspace,
            first,
            guild_session::SeatState::from_areas(saved, old.expanded),
        );
        if let Err(err) = guild_session::project_state::save(&path, &state) {
            tracing::warn!(project = %path.display(), %err, "the tab was not saved");
        }
    }

    /// The sessions a seat written before tab lists shows: its open
    /// records, in their order — what `restore_tabs` restores for it.
    fn open_before_tab_lists(
        desk: &guild_session::Desk,
        seat: Seat,
    ) -> impl Iterator<Item = SessionId> + '_ {
        desk.sessions()
            .filter(move |session| {
                session.open
                    && session.project == seat.project
                    && session.workspace == seat.workspace
            })
            .map(|session| session.id)
    }

    /// The workspace a file written before workspaces belongs to.
    fn first_workspace(&self) -> guild_session::WorkspaceId {
        self.desk
            .workspaces()
            .first()
            .map(|workspace| workspace.id)
            .unwrap_or(self.active_workspace)
    }

    /// Where in `Root.tabs` the tab showing `session` is.
    fn tab_holding(&self, session: SessionId) -> Option<usize> {
        self.tabs
            .iter()
            .position(|tab| tab.sessions().any(|held| held == session))
    }

    /// Reopen the tabs that were open last time.
    ///
    /// The processes did not survive the restart, but the conversations
    /// did: every session has its own identifier, and `--resume` on it
    /// brings the conversation back. Without this, "saving" was only half
    /// done: sessions stayed in the list while the layout had to be
    /// assembled by hand.
    ///
    /// Each project's own list comes first, in its order, with the one
    /// that was in front. A project whose file has no list — written
    /// before M8.1 — falls back to the sessions marked open, which is
    /// what the list was made of before there was one.
    pub(crate) fn restore_tabs(&mut self, cx: &mut Context<Root>) {
        self.history.suspended += 1;
        self.restore_tabs_without_history(cx);
        self.history.suspended -= 1;
    }

    fn restore_tabs_without_history(&mut self, cx: &mut Context<Root>) {
        if !self.restored_workspaces.insert(self.active_workspace) {
            return;
        }
        // A seat held but never read — a root opened again by a command,
        // a folder away when the desk was read — has its records read now,
        // so that its strip finds them (GLD-060).
        guild_session::persist::read_unread(&mut self.desk);
        let first = self
            .desk
            .workspaces()
            .first()
            .map(|workspace| workspace.id)
            .unwrap_or(self.active_workspace);
        let workspace = self.active_workspace;
        let projects: Vec<(ProjectId, PathBuf)> = self
            .desk
            .projects_of(workspace)
            .map(|project| (project.id, project.path.clone()))
            .collect();
        let mut front = None;

        for (project, path) in projects {
            let seat = Seat { workspace, project };
            let file = guild_session::project_state::load(&path);
            let state = file.seat(workspace, first);
            // **The areas are read before this fallback**: a divided
            // seat whose areas are all empty writes `tabs: []`, and those
            // areas come back (invariant 2). Only a seat with neither is
            // one written before M8.1.
            if state.tabs.is_empty() && state.areas.is_none() {
                let open: Vec<SessionId> = Self::open_before_tab_lists(&self.desk, seat).collect();
                for id in open {
                    self.restore_tab(id, cx);
                }
                continue;
            }

            let saved = state.areas_or_one();
            // Legacy maps may contain closed views. Seed only bodies actually
            // restored in this seat, before adoption or asynchronous inspection.
            for strip in saved.strips.values() {
                for tab in &strip.tabs {
                    for (_, body) in tab.panes_or_one().bodies() {
                        if let Shows::Process { run } = body {
                            self.processes.runs.entry(*run).or_default().origin_hint =
                                file.process_origins.get(run).copied();
                        }
                    }
                }
            }

            let focused = saved.areas.focused();
            self.stage.set_areas(seat, saved.areas.clone());
            // A tab already in the seat keeps a place on the screen, even
            // though the areas it was put in are replaced.
            for tab in self
                .tabs
                .iter_mut()
                .filter(|tab| tab.seat == seat && !saved.areas.contains(tab.area))
            {
                tab.area = focused;
            }
            let mut seat_front = None;
            for area in saved.areas.ids() {
                let Some(strip) = saved.strip(area) else {
                    continue;
                };
                self.stage.focus(seat, area);
                let remembered = self.restore_strip(seat, &path, area, strip, cx);
                // Every area's front is remembered, not only the focused
                // one's, and every project's, not only the one in front:
                // the others are where the person goes next.
                if let Some(tab) = remembered {
                    self.stage.bring_forward(seat, area, tab);
                }
                if area == focused {
                    seat_front = remembered;
                }
            }
            // The keys go back to the area that had them.
            self.stage.focus(seat, focused);
            if Some(project) == self.active_project {
                front = seat_front;
            }
        }

        // There was nothing to restore, and nothing is started to greet
        // the eye with: a session begins when the person asks for one, and
        // the empty body says what an opening offers.
        if self.tabs.is_empty() {
            return;
        }

        // The tab the person was looking at, not the first one to
        // hand.
        // The workspace remembers a session; the tab showing it is what
        // gets focus. Two different identities, and this is the seam
        // between them.
        // And the pane showing it, when the tab is divided and it sits in
        // one without the keys — M12.8: a tab that came forward for its
        // session showing something else is the wrong thing in front.
        let by_session = self.desk.active_session().and_then(|session| {
            self.tabs.iter().find_map(|tab| {
                tab.panes
                    .find(|body| body.session() == Some(session))
                    .map(|pane| (tab.id, pane))
            })
        });
        self.active_tab = front
            .or_else(|| {
                let (tab, pane) = by_session?;
                self.focus_pane(tab, pane, cx);
                Some(tab)
            })
            .or_else(|| self.visible_tabs().last().map(|tab| tab.id));
        // **The keys are not moved here.** At a start the first frame
        // finds nothing focused and gives them to the focused pane before
        // any body is drawn (M12.8's second window check, 12.2,
        // `Root::render`); at a workspace switch a body of the seat left
        // loses them and `refocus` gives them to this one — and a dock or
        // the sidebar that holds them keeps them, `guild`'s switches
        // included.

        self.save();
    }

    /// Bring back one area's strip, each tab into `area`, answering the
    /// tab that was in front of it.
    fn restore_strip(
        &mut self,
        seat: Seat,
        path: &Path,
        area: guild_session::AreaId,
        strip: &guild_session::AreaState,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        // A hand-edited file may point past its strip; `SavedAreas`
        // does not put that right, so it is put right here.
        let active = strip.active.filter(|index| *index < strip.tabs.len());
        let (kept, active, dropped) = restorable(&strip.tabs, active, path);
        for (file, lost) in dropped {
            match lost {
                Lost::Tab => tracing::info!(
                    project = %path.display(),
                    file = %file.display(),
                    "a tab's file is gone; the tab is not restored"
                ),
                Lost::Pane => tracing::info!(
                    project = %path.display(),
                    file = %file.display(),
                    "a pane's file is gone; its tab is restored without that pane"
                ),
            }
        }
        // Which tab each entry became — none, when a session could not
        // be brought back — so that the index still points at the right
        // one.
        let mut became: Vec<Option<TabId>> = Vec::with_capacity(kept.len());
        for tab in kept {
            let panes = tab.panes_or_one();
            let mode = tab.markdown_mode;
            let modes = tab.markdown_modes.clone();
            let divided = panes.ids().len() > 1;
            let made = if !divided {
                self.restore_shows(seat, path, tab.shows, cx)
            } else {
                self.restore_divided(seat, path, &panes, cx)
            };
            // `adopt` landed it in the area the stage has focused — this
            // one — unless a tab of this seat was already in front; the
            // area it was saved in is the one it goes back to either way.
            // **Only a tab of this seat**: `restore_tab` makes a record's
            // tab in the record's own seat, and a tab of another seat
            // given this area would be in an area its seat does not have.
            let made = made.and_then(|id| {
                let made = self.tabs.iter_mut().find(|open| open.id == id)?;
                (made.seat == seat).then(|| {
                    made.area = area;
                    for (id, body) in made.panes.bodies() {
                        if let Body::File {
                            presentation: Some(p),
                            ..
                        } = body
                        {
                            let stored = if divided {
                                modes.get(&id).copied()
                            } else {
                                mode
                            };
                            if let Some(mode) = stored {
                                p.restore(mode, cx);
                            }
                        }
                    }
                    made.id
                })
            });
            became.push(made);
        }
        active.and_then(|index| became.get(index).copied().flatten())
    }

    /// Bring back what one pane showed, as a tab of its own — the tab it
    /// became, or `None` when it could not come back (a session nobody can
    /// resume).
    fn restore_shows(
        &mut self,
        seat: Seat,
        path: &Path,
        shows: Shows,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        let project = seat.project;
        let before = self.tabs.len();
        match shows {
            Shows::Session(id) => {
                // Running already (`restore_tab`): the tab it is in takes
                // its saved place, last of what is restored so far.
                if let Some(at) = self.tab_holding(id) {
                    let tab = self.tabs.remove(at);
                    let made = tab.id;
                    self.tabs.push(tab);
                    return Some(made);
                }
                self.restore_tab(id, cx)
            }
            Shows::Process { run } => {
                // Legacy records lack association metadata until their first reply.
                self.processes.runs.entry(run).or_default();
                self.adopt(seat, project, Body::Process { run }, cx);
            }
            Shows::LegacyReview => return None,
            Shows::Settings => {
                self.restore_settings_tab(project, cx);
            }
            Shows::File(relative) => {
                self.restore_file_tab(project, path, relative, cx);
            }
            Shows::Diff { path: target, base } => {
                self.restore_diff_tab(project, path, target, base, cx);
            }
            Shows::Annotate(relative) => {
                self.restore_annotate_tab(project, relative, cx);
            }
        }
        // Every `restore_*` adopts, and `adopt` pushes at the end.
        self.tabs.get(before).map(|made| made.id)
    }

    /// Bring back a divided tab — M12.8: its division, fractions and
    /// focus as written, each pane's body brought back by the same
    /// `restore_*` a tab of one pane is and moved into its pane. A pane
    /// whose body cannot come back stays empty (decision 13), and a tab
    /// none of whose panes came back is not made at all (invariant 3).
    ///
    /// **Built whole and adopted once, through `adopt_panes`**, rather
    /// than adopted empty and poured into with `Root::pour`: `pour` saves,
    /// and a save in the middle of a restore writes every project not yet
    /// restored back with no tabs. Nor does a tab showing nothing ever
    /// stand in `Root.tabs`, even for a moment.
    fn restore_divided(
        &mut self,
        seat: Seat,
        path: &Path,
        panes: &guild_session::Panes<Shows>,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        let mut filled: guild_session::Panes<Body> = panes.emptied();
        for (pane, shows) in panes.bodies() {
            let Some(made) = self.restore_shows(seat, path, shows.clone(), cx) else {
                continue;
            };
            // The one-pane tab `restore_*` made, taken apart for its body.
            // A tab of another seat — a record that names another — is
            // left as it is, and this pane stays empty.
            let Some(at) = self
                .tabs
                .iter()
                .position(|open| open.id == made && open.seat == seat)
            else {
                continue;
            };
            let mut one = self.tabs.remove(at);
            let shown = one.panes.focused();
            let Some(body) = one.panes.take(shown) else {
                continue;
            };
            // `pane` is one of `filled`'s and empty, so `fill` does not
            // refuse; were it to, the body goes back into its own tab
            // rather than being dropped with its process.
            if let Err(body) = filled.fill(pane, body)
                && one.panes.fill(shown, body).is_ok()
            {
                self.tabs.insert(at, one);
            }
        }
        (filled.showing() > 0).then(|| self.adopt_panes(seat, seat.project, filled, cx))
    }

    /// Return to a closed session.
    pub(crate) fn reopen(&mut self, id: SessionId, cx: &mut Context<Root>) {
        self.reopen_with(id, None, cx);
    }

    /// Return to a closed session, running `conversation` when it is given
    /// — the start screen's resume of an OpenCode or Codex record names the
    /// harness's own id (M14.5). `None` asks `continue_or_forget`, as a
    /// restore does — each harness by its own id.
    pub(crate) fn reopen_with(
        &mut self,
        id: SessionId,
        conversation: Option<Conversation>,
        cx: &mut Context<Root>,
    ) {
        self.reopen_into(id, conversation, true, cx);
    }

    /// [`Self::reopen_with`], and the tab that holds the session now —
    /// taking the front only when `front` says so. M13.3: a person's click
    /// wants the tab in front; `guild session resume` from an agent does
    /// not, and the person stays where they were. `None` when nothing could
    /// bring the session back and its record was forgotten.
    pub(crate) fn reopen_into(
        &mut self,
        id: SessionId,
        conversation: Option<Conversation>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        self.reopen_prompt(id, conversation, None, front, cx)
    }

    /// Reopen through the same tab/workspace path, optionally supplying literal startup text.
    pub(super) fn reopen_prompt(
        &mut self,
        id: SessionId,
        conversation: Option<Conversation>,
        first: Option<String>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        self.note_place(cx);
        self.history.suspended += 1;
        let result = self.reopen_prompt_without_history(id, conversation, first, front, cx);
        self.history.suspended -= 1;
        result
    }

    /// Replace only a positively quiescent retained task body, keeping its pane.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn reopen_quiescent_task(
        &mut self,
        id: SessionId,
        entity: gpui::EntityId,
        leader: u32,
        conversation: &str,
        first: String,
        front: bool,
        cx: &mut Context<Root>,
    ) -> Result<bool, String> {
        let (tab, pane, view) = self
            .tabs
            .iter()
            .find_map(|t| {
                t.panes.bodies().into_iter().find_map(|(p, b)| match b {
                    Body::Terminal { session, view, .. }
                        if *session == id && view.entity_id() == entity =>
                    {
                        Some((t.id, p, view.clone()))
                    }
                    _ => None,
                })
            })
            .ok_or("Retained task body disappeared")?;
        let record = self.desk.session(id).ok_or("Task disappeared")?.clone();
        view.update(cx, |v, _| v.release_quiescent(leader))?;
        let body = Body::open(
            id,
            record.runs,
            &record.cwd,
            Some(Conversation::Resume(conversation.into())),
            &self.session_launch_settings(id),
            self.theme,
            self.ide_port(),
            self.hook_launch(),
            Some(first),
            record.launch_selection.as_ref(),
            &self.start_size,
            cx,
        );
        let launched = matches!(body, Body::Terminal { .. });
        let target = self
            .tabs
            .iter_mut()
            .find(|t| t.id == tab)
            .ok_or("Retained tab disappeared")?;
        target.panes.take(pane);
        target
            .panes
            .fill(pane, body)
            .map_err(|_| "Retained pane changed")?;
        self.desk.reopen_session(id);
        self.forget_status(id);
        if front {
            self.active_workspace = record.workspace;
            self.desk.select_workspace(record.workspace);
            self.active_project = Some(record.project);
            self.active_tab = Some(tab);
            self.focus_pane(tab, pane, cx);
            self.focusing_front = true;
        }
        self.save();
        cx.notify();
        Ok(launched)
    }

    pub(super) fn reopen_prompt_without_history(
        &mut self,
        id: SessionId,
        conversation: Option<Conversation>,
        first: Option<String>,
        front: bool,
        cx: &mut Context<Root>,
    ) -> Option<TabId> {
        let record = self.desk.session(id)?;
        let (runs, project, workspace, cwd) = (
            record.runs,
            record.project,
            record.workspace,
            record.cwd.clone(),
        );
        let before = (self.active_workspace, self.active_project, self.active_tab);

        // A workspace not restored yet is restored before a tab goes into
        // it: `restore_tabs` asks nothing of the tabs already there, so a
        // tab pushed first is restored a second time on the next visit —
        // two processes on one transcript — and the seat stored meanwhile
        // holds this tab alone, losing the ones left there.
        if workspace != self.active_workspace && !self.restored_workspaces.contains(&workspace) {
            self.select_workspace(workspace, cx);
            let seat = Seat { workspace, project };
            if let Some((tab, pane)) = self.find_showing(seat, |body| body.session() == Some(id)) {
                if first.is_some() {
                    self.put_back(before, cx);
                    return None;
                }
                if front {
                    self.focus_pane(tab, pane, cx);
                    // A person's gesture: the keys go with the outline.
                    self.focusing_front = true;
                    self.active_project = Some(project);
                    self.active_tab = Some(tab);
                } else {
                    self.put_back(before, cx);
                }
                self.save();
                cx.notify();
                return Some(tab);
            }
        }

        // The same check as when restoring, unless the caller knows.
        let conversation = match conversation {
            Some(known) => Some(known),
            None => match self.continue_or_forget(id) {
                Some(conversation) => conversation,
                None => {
                    if !front {
                        self.put_back(before, cx);
                    }
                    cx.notify();
                    return None;
                }
            },
        };
        let (ide, hooks) = (self.ide_port(), self.hook_launch());
        let body = Body::open(
            id,
            runs,
            &cwd,
            conversation,
            &self.session_launch_settings(id),
            self.theme,
            ide,
            hooks,
            first,
            self.desk
                .session(id)
                .and_then(|record| record.launch_selection.as_ref()),
            &self.start_size,
            cx,
        );

        self.desk.reopen_session(id);
        self.closed.forget(id);
        let tab = self.adopt(Seat { workspace, project }, project, body, cx);
        if front {
            // A closed session can come back into a project or workspace
            // other than whichever a menu open right now was about. M12.7
            // final fix.
            self.close_menu_for_switch();
            self.active_workspace = workspace;
            self.desk.select_workspace(workspace);
            self.active_project = Some(project);
            self.unfold_group_of(project);
            self.active_tab = Some(tab);
        } else {
            self.put_back(before, cx);
        }
        self.save();
        cx.notify();
        Some(tab)
    }

    /// The record this conversation is, and where it stands against `tab`
    /// — decision 52's question, asked by both witnesses. M13.3.
    pub(crate) fn found_for(
        &self,
        tab: TabId,
        harness: guild_session::HarnessKind,
        conversation: &str,
    ) -> Option<crate::binding::Found> {
        let seat = self.tabs.iter().find(|open| open.id == tab)?.seat;
        let records: Vec<crate::binding::Record<'_>> = self
            .desk
            .sessions()
            .map(|record| crate::binding::Record {
                id: record.id,
                harness: record.runs.harness(),
                harness_session: record.harness_session.as_deref(),
            })
            .collect();
        let record = crate::binding::record_of(harness, conversation, &records)?;
        let held = self.desk.session(record)?;
        let in_another_tab = self
            .tabs
            .iter()
            .any(|open| open.sessions().any(|shown| shown == record));
        Some(crate::binding::Found {
            record,
            same_seat: held.project == seat.project && held.workspace == seat.workspace,
            running_elsewhere: crate::start::running(
                in_another_tab,
                held.open,
                self.restored_workspaces.contains(&held.workspace),
            ),
        })
    }

    /// The tab now holds `to`'s conversation, and its process goes on —
    /// M13.3, a `/resume` typed in a harness tab.
    pub(crate) fn rebind(
        &mut self,
        tab: TabId,
        current: SessionId,
        to: SessionId,
        cx: &mut Context<Root>,
    ) {
        let Some(open) = self.tabs.iter_mut().find(|open| open.id == tab) else {
            return;
        };
        let area = open.area;
        let Some(pane) = open
            .panes
            .bodies()
            .into_iter()
            .find(|(_, body)| body.session() == Some(current))
            .map(|(pane, _)| pane)
        else {
            return;
        };
        let focused = open.panes.focused();
        let _ = open.panes.focus(pane);
        let Some(Body::Terminal { session, view, .. }) = open.panes.body_mut() else {
            return;
        };
        let view = view.clone();
        let from = std::mem::replace(session, to);
        let _ = open.panes.focus(focused);
        // The id the process was given: what `from` was started as, when
        // its tab was itself rebound before.
        let launched = self
            .started_as
            .iter()
            .find(|(_, now)| **now == from)
            .map(|(launched, _)| *launched)
            .unwrap_or(from);
        self.started_as.insert(launched, to);
        // Its process now holds `to`'s conversation: closing it later is
        // `to`'s retirement, which a continuation of `to` waits for.
        view.update(cx, |view, _| view.set_retire_tag(to.to_string()));
        self.retire_session(from, Some(area));
        self.forget_status(to);
        // The program goes on, and its record stands: `to` starts from it, as
        // a fact rather than a transition. What the terminal queued before
        // now was `from`'s, and a repeat of the record would be no change in
        // the terminal, so nothing else would ever tell `to` (review r1, 1).
        let standing = view.update(cx, |view, _| {
            let _ = view.take_status_changes();
            view.status_root()
        });
        self.board
            .seed(to, standing.as_ref().and_then(|r| r.reported()));
        self.desk.reopen_session(to);
        self.closed.forget(to);
        if self.active_tab == Some(tab)
            && self
                .tabs
                .iter()
                .find(|open| open.id == tab)
                .and_then(Tab::session)
                == Some(to)
        {
            self.desk.look_at(self.active_project, Some(to));
        }
        tracing::info!(%from, %to, "a tab continued another session's conversation");
        self.save();
        cx.notify();
    }

    /// A terminal tab's program resumed `to`'s conversation: end it, and
    /// run the window's own resume in the same tab once it has gone —
    /// M13.3, decision 52. The shell under it goes too; the tab is a
    /// harness tab from here, as it would be had the window opened it.
    pub(crate) fn hand_over(
        &mut self,
        tab: TabId,
        current: SessionId,
        to: SessionId,
        cx: &mut Context<Root>,
    ) {
        let Some((shell, view)) = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .and_then(|open| {
                open.bodies().find_map(|body| match body {
                    Body::Terminal { session, view, .. } if *session == current => {
                        Some((*session, view.clone()))
                    }
                    _ => None,
                })
            })
        else {
            return;
        };
        if !self.handing_over.insert(tab) {
            return;
        }
        let over = view.update(cx, |view, _| view.let_go());
        cx.spawn(async move |root, cx| {
            if let Some(over) = over {
                cx.background_executor()
                    .spawn(async move {
                        let _ = over.recv_timeout(std::time::Duration::from_secs(3));
                    })
                    .await;
            }
            let _ = root.update(cx, |root, cx| root.finish_hand_over(tab, shell, to, cx));
        })
        .detach();
    }

    /// The typed program is gone: open `to` in the tab it was in.
    fn finish_hand_over(
        &mut self,
        tab: TabId,
        shell: SessionId,
        to: SessionId,
        cx: &mut Context<Root>,
    ) {
        self.handing_over.remove(&tab);
        let Some(at) = self.tabs.iter().position(|open| open.id == tab) else {
            return;
        };
        let Some(pane) = self.tabs[at]
            .panes
            .bodies()
            .into_iter()
            .find(|(_, body)| body.session() == Some(shell))
            .map(|(pane, _)| pane)
        else {
            return;
        };
        // Asked again: a person or a command may have reopened it meanwhile,
        // and a second process on its transcript is worse than a dead tab.
        if self
            .tabs
            .iter()
            .any(|open| open.sessions().any(|shown| shown == to))
        {
            return;
        }
        let Some(Some(conversation)) = self.continue_or_forget(to) else {
            return;
        };
        let Some(record) = self.desk.session(to) else {
            return;
        };
        let runs = record.runs;
        let cwd = self
            .desk
            .session(to)
            .map(|record| record.cwd.clone())
            .unwrap_or_default();
        let (ide, hooks) = (self.ide_port(), self.hook_launch());
        let body = Body::open(
            to,
            runs,
            &cwd,
            Some(conversation),
            &self.session_launch_settings(to),
            self.theme,
            ide,
            hooks,
            None,
            self.desk
                .session(to)
                .and_then(|record| record.launch_selection.as_ref()),
            &self.start_size,
            cx,
        );
        let _ = self.tabs[at].panes.take(pane);
        let _ = self.tabs[at].panes.fill(pane, body);
        // A shell has nothing to come back to: forgotten, as a closed
        // shell is, and not offered by cmd-shift-t.
        self.desk.remove_session(shell);
        self.desk.reopen_session(to);
        self.closed.forget(to);
        if self.active_tab == Some(tab)
            && self
                .tabs
                .iter()
                .find(|open| open.id == tab)
                .and_then(Tab::session)
                == Some(to)
        {
            self.desk.look_at(self.active_project, Some(to));
            self.focusing_front = true;
        }
        tracing::info!(%shell, %to, "a terminal tab handed its conversation to the window");
        self.save();
        cx.notify();
    }

    /// Say once why a tab was not bound — M13.3.
    pub(crate) fn tell_binding(&mut self, tab: TabId, bind: crate::binding::Bind) {
        use crate::binding::Bind;
        let (record, elsewhere) = match bind {
            Bind::Elsewhere(record) => (record, true),
            Bind::OtherSeat(record) => (record, false),
            Bind::To(_) | Bind::Nothing => return,
        };
        if !self.told.first(tab, record) {
            return;
        }
        let Some(held) = self.desk.session(record) else {
            return;
        };
        let name = crate::title::chosen(held.name.as_deref(), held.topic.clone())
            .unwrap_or_else(|| record.to_string());
        let place = held.cwd.display().to_string();
        let text = if elsewhere {
            self.ui().notice_open_elsewhere(&name, &place)
        } else {
            self.ui().notice_other_seat(&name, &place)
        };
        self.warn(text);
    }

    /// Return the person to where they were — a command's reopen restores
    /// a workspace to find its tab, and must not leave them there. M13.3,
    /// `command_session_new`'s dance as one function.
    fn put_back(
        &mut self,
        (workspace, project, tab): (guild_session::WorkspaceId, Option<ProjectId>, Option<TabId>),
        cx: &mut Context<Root>,
    ) {
        if self.active_workspace != workspace {
            self.select_workspace(workspace, cx);
        }
        self.active_project = project;
        self.active_tab = tab;
    }

    /// Close a tab, keeping the session.
    /// Bring back the tab that was closed last.
    ///
    /// An agent's session comes back as itself, conversation and all —
    /// Claude's, OpenCode's or Codex's; a shell comes back as a new shell
    /// in the same directory, because a closed shell has nothing to
    /// resume. Before this the only gesture people reached for was "new
    /// tab", which opened a Claude session whatever had been closed.
    pub(crate) fn reopen_last(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.reopen_last_without_history(window, cx);
        self.history.suspended -= 1;
    }

    fn reopen_last_without_history(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        let Some(project) = self.active_project else {
            return;
        };
        let seat = Seat {
            workspace: self.active_workspace,
            project,
        };
        let Some((tab, from)) = self.closed.take_last_from(seat) else {
            return;
        };
        // **Back into the area it was closed from** — M12.8 — while the
        // seat still has it, else the focused one. `reopening` answers and
        // does not focus, and `adopt` lands in the focused area, which is
        // read from the tab in front (decision 7): so the area is given
        // the keys here, with its own front tab, before anything opens.
        self.note_front();
        let area = self.stage.reopening(seat, from);
        if self.focused_area() != Some(area) {
            let front = self.area_front(Some(area));
            self.stage.focus(seat, area);
            self.active_tab = front;
        }

        match tab {
            ClosedTab::Session {
                session: Some(id),
                runs,
                ..
            } => {
                self.reopen(id, cx);
                // The conversation may have turned out to be unresumable,
                // and then `reopen` forgets it. An empty window is not
                // what the gesture promises, so a fresh tab of the same
                // kind takes its place.
                if !self
                    .tabs
                    .iter()
                    .any(|open| open.sessions().any(|shown| shown == id))
                {
                    self.open_tab(project, runs, cx);
                }
            }
            ClosedTab::Session { runs, .. } => self.open_tab(project, runs, cx),
            ClosedTab::File { path, .. } => self.reopen_file(path, window, cx),
        }
    }

    /// Close a tab.
    ///
    /// Takes a tab rather than a session: they are no longer the same
    /// thing. A tab with no session — settings today, a file from M8 —
    /// simply goes, with nothing to record and nothing to forget.
    pub(crate) fn close_tab(&mut self, tab: TabId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.close_tab_without_history(tab, cx);
        self.history.suspended -= 1;
    }

    fn close_tab_without_history(&mut self, tab: TabId, cx: &mut Context<Root>) {
        self.sync_from_views(cx);
        // The stage's focus is made current before anything goes: when
        // the closing tab empties its area, the focus is read from the
        // stage below (decision 16 — the emptied area keeps the keys),
        // and the stage hears of the tab in front only at a save, so an
        // arrow or a click that moved the keys since would be undone.
        self.note_front();

        // The area it is closed from, which `cmd-shift-t` puts each of
        // its panes back into — M12.8.
        let area = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| open.area);
        // Every pane's session goes with the tab — M12.8.
        let sessions: Vec<SessionId> = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| open.sessions().collect())
            .unwrap_or_default();
        for id in &sessions {
            if !self
                .tabs
                .iter()
                .any(|open| open.id != tab && open.sessions().any(|s| s == *id))
            {
                self.retire_session(*id, area);
                self.started_as.retain(|_, now| now != id);
            }
        }
        self.told.forget(&tab);
        self.handing_over.remove(&tab);
        // A file is remembered by its path, so that cmd-shift-t brings
        // it back the way it brings a session back — every pane's.
        let closing_files: Vec<(ProjectId, PathBuf)> = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| {
                open.bodies()
                    .filter_map(|body| match body {
                        // **The project's own files only.** `reopen_tab`
                        // goes through `open_file`, which is
                        // `Origin::Project`, so remembering a dependency's
                        // tab would make `cmd-shift-t` open this
                        // repository's file of the same name — the reason
                        // `store_tabs` above refuses the same tab.
                        Body::File {
                            path,
                            origin: crate::Origin::Project,
                            ..
                        } => Some((open.project, path.clone())),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        // A proposal put away without an answer is a refusal: the
        // agent is blocked until it hears one, and closing the tab is
        // the person saying no. M8.6.
        self.proposal_closing(tab);
        self.let_go_of_files(closing_files, area, Some(tab), cx);

        // Its own area's strip as it stood, for the neighbour: read
        // before the tab goes, or the closed one has no place left to be
        // beside. The same strip gives the front remembered there: a tab
        // closed while the person is in another project — by `guild`, or
        // its process ending — leaves its neighbour to come back to.
        let own = self
            .tabs
            .iter()
            .find(|open| open.id == tab)
            .map(|open| (open.seat, open.area))
            .map(|(seat, area)| {
                let strip: Vec<TabId> = self
                    .tabs
                    .iter()
                    .filter(|open| open.seat == seat && open.area == area)
                    .map(|open| open.id)
                    .collect();
                (seat, area, strip)
            });
        if let Some((_, area, strip)) = &own {
            self.stage.closing(*area, strip, tab);
        }
        self.tabs.retain(|open| open.id != tab);
        self.close_origin_process_views(&sessions, cx);

        if self.active_tab == Some(tab) {
            self.active_tab = match own {
                // Its neighbour in its own area; none when the area is
                // left empty, which keeps the keys and shows the start
                // screen (decision 16).
                Some((seat, _, strip)) if self.active_seat() == Some(seat) => {
                    let remaining: Vec<_> = strip
                        .into_iter()
                        .filter(|id| *id == tab || self.tabs.iter().any(|t| t.id == *id))
                        .collect();
                    crate::closing::neighbour(&remaining, tab)
                }
                // A tab in front that the strip does not show — another
                // seat's — has no neighbour here; the strip's last stands
                // in, as it did for every close before.
                _ => self.visible_tabs().last().map(|open| open.id),
            };
        }

        self.save();
        cx.notify();
    }

    /// Remember each closed file for `cmd-shift-t` — with `area`, the one
    /// it was closed from — and let the language server and the diffs go
    /// of one no other tab or pane holds any more. `except` is a tab
    /// about to go that still holds it.
    fn let_go_of_files(
        &mut self,
        files: Vec<(ProjectId, PathBuf)>,
        area: Option<guild_session::AreaId>,
        except: Option<TabId>,
        cx: &mut Context<Root>,
    ) {
        for (project, path) in files {
            self.remember_closed_file(project, path.clone(), area);
            let last_holder = !self.tabs.iter().any(|open| {
                Some(open.id) != except
                    && open.project == project
                    && open.bodies().any(|body| matches!(body,
                        Body::File { path: other, origin: crate::Origin::Project, .. } if other == &path))
            });
            if last_holder {
                self.unfollow_in_diffs(project, &path, cx);
                self.close_document(project, &path);
            } else {
                self.retain_document_holder(project, &path, except, cx);
            }
        }
    }

    /// Close what `target` names, with no question — the walk has asked.
    pub(crate) fn close_target(&mut self, target: CloseTarget, cx: &mut Context<Root>) {
        match target {
            CloseTarget::Tab(tab) => self.close_tab(tab, cx),
            CloseTarget::Pane(tab, pane) => self.close_pane(tab, pane, cx),
        }
    }

    /// Close one pane of `tab` — M12.8, decision 14 — with no question.
    ///
    /// The last pane that shows anything closes the tab instead, through
    /// `close_tab` (invariant 3), and a pane the tab does not have closes
    /// nothing. Otherwise the pane goes the way a tab goes: its session
    /// retired, its file remembered — `cmd-shift-t` brings either back as
    /// a tab — and a proposal parked in it refused, since the agent is
    /// blocked until it hears (M8.6). Its heir takes the keys within the
    /// tab **when the closed body had them**; the caller that has a window
    /// puts them there (traps.md: *…the focus-lost rule does not cover a
    /// closed tab*), and the next frame does for one that has not — and
    /// leaves them alone when a dock, a field or a popup holds them.
    pub(crate) fn close_pane(&mut self, tab: TabId, pane: PaneId, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.close_pane_without_history(tab, pane, cx);
        self.history.suspended -= 1;
    }

    fn close_pane_without_history(&mut self, tab: TabId, pane: PaneId, cx: &mut Context<Root>) {
        self.sync_from_views(cx);
        // Read before it goes: the pane the outline is on, whose body may
        // have the keys — and whether it does is the window's to say.
        let had_keys = self.holds_the_keys(CloseTarget::Pane(tab, pane));
        let Some(open) = self.tabs.iter_mut().find(|open| open.id == tab) else {
            return;
        };
        let (project, area) = (open.project, open.area);
        let body = match open.panes.close(pane) {
            // A stale id: nothing is here to close, and the tab stays.
            Closed::Missing => return,
            Closed::Tab => {
                self.close_tab(tab, cx);
                return;
            }
            Closed::Pane(body) => body,
        };
        self.proposal_pane_closing(tab, pane);
        if had_keys {
            // What the body was focused by; Settings, which has nothing to
            // type into, is focused by the window's own handle (12.2).
            let handle = body
                .as_ref()
                .and_then(|body| body.focus_handle(cx))
                .or_else(|| {
                    matches!(body, Some(Body::Settings { .. })).then(|| self.focus.clone())
                });
            self.closed_focus.extend(handle);
        }
        if let Some(body) = body {
            if let Some(session) = body.session() {
                if !self
                    .tabs
                    .iter()
                    .any(|open| open.sessions().any(|s| s == session))
                {
                    self.retire_session(session, Some(area));
                }
                self.close_origin_process_views(&[session], cx);
            }
            // The project's own files only — `close_tab`'s reason.
            if let Body::File {
                path,
                origin: crate::Origin::Project,
                ..
            } = &body
            {
                // The body is out of the tab already: every holder left
                // is another's.
                self.let_go_of_files(vec![(project, path.clone())], Some(area), None, cx);
            }
        }
        // **No flag that moves the keys whatever holds them** — the final
        // review's Important 1. An agent closes a pane with no window, and
        // a flag took the person's keys out of the Commit box into the
        // terminal beside the closed pane. The next frame moves them only
        // off the closed body (`closed_focus`); a caller with a window
        // refocuses at once, as `cmd-w` does.
        self.save();
        cx.notify();
    }

    /// What a session's tab is called when it is not called by its place —
    /// the name a person gave it, or else its topic, or else what its
    /// program wrote into the terminal's title. `None` for a tab that is not
    /// a session's, or has none of the three.
    ///
    /// **No `App` on purpose**: this runs on every frame, for the strip and
    /// the agents counter, and without one it cannot read a terminal's
    /// entity — which would tie the window to that terminal's every
    /// printed line. The title comes from its facts. `terminal::facts`.
    pub(crate) fn session_tab_name(&self, tab: &Tab) -> Option<String> {
        // The name is the strip's, so it is the face's.
        self.session_body_name(tab.face()?)
    }

    /// [`Root::session_tab_name`] for one pane's body — what the agents
    /// list calls each pane's session. No `App`, for the same reason.
    pub(crate) fn session_body_name(&self, body: &Body) -> Option<String> {
        let record = self.desk.session(body.session()?)?;
        // What this conversation is about, from its own transcript — and
        // only then what the program wrote into the terminal's title. For
        // a Claude session that title is not reliably its own: one was
        // measured showing another conversation's name entirely. For a
        // shell it is exactly right, and a shell has no transcript.
        // A topic kept before slash commands were read as commands is
        // their markup's first line, and is read as the command here.
        let automatic = record
            .topic
            .as_deref()
            .map(|topic| {
                guild_session::transcript::slash_command(topic).unwrap_or(topic.to_owned())
            })
            .as_deref()
            .and_then(crate::title::of)
            .or_else(|| match body {
                Body::Terminal { facts, .. } if !record.runs.is_claude() => {
                    facts.title().as_deref().and_then(crate::title::of)
                }
                _ => None,
            });
        crate::title::chosen(record.name.as_deref(), automatic)
    }

    /// Open the field that names a session's tab — M12.1, from a
    /// double-click on the tab or from its menu.
    ///
    /// **Filled with what the tab is called now**, so a small change is a
    /// small edit and `cmd-backspace` is one stroke from a new name. It
    /// takes the focus while it is up: a terminal holding the focus would
    /// have every letter, and an editor would have Return.
    pub(crate) fn start_renaming_tab(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        // The session the strip names the tab by — its face's.
        if tab.face().and_then(Body::session).is_none() {
            return;
        }
        let now = self.session_tab_name(tab).unwrap_or_default();
        self.put_fields_away();
        self.renaming_tab = Some((Renamed::Tab(id), now));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Open the field that names a project, on its head in the column —
    /// M12.2, *Rename Project*. The same field as a tab's, filled with what
    /// the project is called now.
    pub(crate) fn start_renaming_project(
        &mut self,
        key: std::path::PathBuf,
        now: String,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        self.put_fields_away();
        self.renaming_tab = Some((Renamed::Project(key), now));
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn rename_input(
        &self,
        value: String,
        cx: &mut Context<Root>,
    ) -> crate::text_field::TextField {
        crate::text_field::TextField::new(
            "rename-input",
            value,
            cx.listener(|root, value: &String, _, cx| {
                if let Some((_, typed)) = root.renaming_tab.as_mut() {
                    *typed = value.clone();
                }
                cx.notify();
            }),
        )
        .focus(&self.focus)
        .autofocus()
        .selected(true)
        .on_key(cx.listener(|root, event, window, cx| {
            root.rename_tab_key(event, window, cx);
        }))
    }

    /// The tab being renamed and what has been typed, if one is.
    pub(crate) fn renaming(&self) -> Option<(TabId, &str)> {
        match &self.renaming_tab {
            Some((Renamed::Tab(id), typed)) => Some((*id, typed.as_str())),
            _ => None,
        }
    }

    /// The project being renamed, by its key, and what has been typed.
    pub(crate) fn renaming_project(&self) -> Option<(&std::path::Path, &str)> {
        match &self.renaming_tab {
            Some((Renamed::Project(key), typed)) => Some((key.as_path(), typed.as_str())),
            _ => None,
        }
    }

    /// A key pressed while a tab's name is being typed.
    ///
    /// Return keeps the name — an empty one gives the tab back its own —
    /// and Escape leaves it as it was; everything else is
    /// `desktop::edit`'s, as in every field of ours.
    pub(crate) fn rename_tab_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) -> bool {
        if self.renaming_tab.is_none() {
            return false;
        }
        let stroke = &event.keystroke;
        match stroke.key.as_str() {
            "escape" => self.finish_renaming_tab(false, window, cx),
            "enter" => self.finish_renaming_tab(true, window, cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Put the field away — keeping what was typed, or not.
    ///
    /// Return keeps it and so does a click anywhere else, as Finder and
    /// every editor's rename-in-place do: a person who typed a name and
    /// clicked on is done, not cancelling. Escape is the one way out that
    /// changes nothing.
    pub(crate) fn finish_renaming_tab(
        &mut self,
        keep: bool,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Some((renamed, typed)) = self.renaming_tab.take() else {
            return;
        };
        if keep {
            match renamed {
                Renamed::Tab(id) => {
                    if let Some(session) = self
                        .tabs
                        .iter()
                        .find(|tab| tab.id == id)
                        .and_then(Tab::face)
                        .and_then(Body::session)
                    {
                        self.desk.rename_session(session, &typed);
                        self.save();
                    }
                }
                Renamed::Project(key) => {
                    self.desk.rename_repository(&key, &typed);
                    self.save();
                }
            }
        }
        self.refocus(window, cx);
        cx.notify();
    }

    /// Put the focus back on what is drawn.
    ///
    /// A tab that goes behind another — switching projects with `cmd-1`,
    /// stepping tabs — keeps the focus handle it had, and a focused
    /// element that nobody draws is a dispatch path with nothing on it:
    /// no shortcut works until a click. The window says when that
    /// happens, and this is the answer: the body in front, the start
    /// screen when a project has none, or the root itself when a panel
    /// covers the middle or there is no project.
    pub(crate) fn refocus(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        if let Some(open) = self.clone_project.as_ref().filter(|open| open.visible) {
            window.focus(&open.focus, cx);
            return;
        }
        // Rows can replace a focused action as a launch completes or scope changes.
        // An open recovery popup still owns the keys after that control disappears.
        if let Some(list) = self.terminal_list.as_ref() {
            if matches!(
                self.context_menu.as_ref().map(|menu| &menu.target),
                Some(crate::view::menu::Target::TerminalJobScopes { .. })
            ) {
                window.focus(&self.focus, cx);
            } else {
                window.focus(&list.focus, cx);
            }
            self.focusing_front = false;
            cx.notify();
            return;
        }
        let body = self.active_body().and_then(|body| body.focus_handle(cx));
        match body {
            Some(handle) => window.focus(&handle, cx),
            // The start screen reads its own keys, on its own handle — M14.5.
            None if self.start_showing().is_some() => window.focus(&self.start_focus, cx),
            None => window.focus(&self.focus, cx),
        }
        cx.notify();
    }

    /// Close the current tab — asking first if it holds unsaved text.
    ///
    /// **A divided tab loses its focused pane instead** — M12.8, decision
    /// 14: `cmd-w` closes the pane with the keys while its tab has another,
    /// and the tab only when the pane is all of it, as an editor's split
    /// closes before its window does. The pane is asked about as a tab is.
    ///
    /// With no tab, `cmd-w` puts away the Settings page the window draws
    /// when no project is in front — the one thing in the middle a person
    /// would close. M5.11's decision 15.
    pub(crate) fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Root>) {
        if let Some(id) = self.active_tab {
            let divided = self
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .filter(|tab| tab.panes.ids().len() > 1)
                .map(|tab| tab.panes.focused());
            match divided {
                Some(pane) => self.close_pane_asking(id, pane, window, cx),
                None => self.close_tab_asking(id, window, cx),
            }
        } else if self.settings_page.is_some() {
            self.close_settings(crate::view::settings::SettingsHost::Window, cx);
        }
    }

    /// Move to the neighbouring tab.
    pub(crate) fn step_tab(&mut self, forward: bool, cx: &mut Context<Root>) {
        self.note_place(cx);
        self.history.suspended += 1;
        self.step_tab_without_history(forward, cx);
        self.history.suspended -= 1;
    }

    fn step_tab_without_history(&mut self, forward: bool, cx: &mut Context<Root>) {
        let ids: Vec<TabId> = self.visible_tabs().map(|tab| tab.id).collect();
        if ids.len() < 2 {
            return;
        }

        let current = self
            .active_tab
            .and_then(|id| ids.iter().position(|x| *x == id));
        let next = match current {
            // Cycling: at the end, the next one is the first. Hitting a
            // wall while switching tabs is inconvenient and unfamiliar.
            Some(index) if forward => (index + 1) % ids.len(),
            Some(index) => (index + ids.len() - 1) % ids.len(),
            None => 0,
        };

        self.active_tab = Some(ids[next]);
        // Transfer the keys before drawing the next tab. Waiting for focus-lost
        // can leave a hidden body's handle with an empty dispatch path.
        self.focusing_front = true;
        self.save();
        cx.notify();
    }
}

impl Root {
    /// The handlers for the tabs and the session list.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_tabs(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &NewSession, window, cx| {
            if let Some(project) = root.active_project {
                let runs = root.runs_for_default_tab(cx);
                root.open_harness_tab(project, runs, window, cx);
            }
        }))
        .on_action(cx.listener(|root, _: &NewTerminal, _window, cx| {
            if let Some(project) = root.active_project {
                root.open_tab(project, Runs::Shell, cx);
            }
        }))
        .on_action(cx.listener(|root, _: &CloseTab, window, cx| root.close_active_tab(window, cx)))
        .on_action(cx.listener(|root, _: &CloseOtherTabs, window, cx| {
            let keep = root.active_tab;
            let tabs: Vec<_> = root
                .visible_tabs()
                .map(|tab| tab.id)
                .filter(|id| Some(*id) != keep)
                .collect();
            root.close_tabs_asking(tabs, window, cx);
        }))
        .on_action(cx.listener(|root, _: &CloseAllTabs, window, cx| {
            let tabs: Vec<_> = root.visible_tabs().map(|tab| tab.id).collect();
            root.close_tabs_asking(tabs, window, cx);
        }))
        .on_action(cx.listener(|root, _: &ReopenTab, window, cx| root.reopen_last(window, cx)))
        .on_action(cx.listener(|root, _: &NextTab, _window, cx| root.step_tab(true, cx)))
        .on_action(cx.listener(|root, _: &PreviousTab, _window, cx| root.step_tab(false, cx)))
        .on_action(cx.listener(|root, _: &ToggleSessions, _window, cx| {
            root.toggle_column_mode(cx);
        }))
        .on_action(cx.listener(|root, _: &ToggleTerminalJobs, window, cx| {
            root.toggle_terminal_list(window, cx);
        }))
        .on_action(cx.listener(|root, _: &ToggleUsage, _, cx| {
            root.toggle_usage(cx);
        }))
        .on_action(cx.listener(
            |root, _: &crate::actions::GoToWaitingSession, _window, cx| root.go_to_waiting(cx),
        ))
        // The areas — M12.8.
        .on_action(cx.listener(|root, _: &SplitAreaRight, window, cx| {
            root.split_area(Axis::Horizontal, window, cx)
        }))
        .on_action(cx.listener(|root, _: &SplitAreaDown, window, cx| {
            root.split_area(Axis::Vertical, window, cx)
        }))
        .on_action(cx.listener(|root, _: &SplitPaneRight, window, cx| {
            root.split_pane(Axis::Horizontal, window, cx)
        }))
        .on_action(cx.listener(|root, _: &SplitPaneDown, window, cx| {
            root.split_pane(Axis::Vertical, window, cx)
        }))
        .on_action(cx.listener(|root, _: &CloseArea, window, cx| root.close_area(window, cx)))
        .on_action(cx.listener(|root, _: &FocusLeft, window, cx| {
            root.focus_toward(Direction::Left, window, cx)
        }))
        .on_action(cx.listener(|root, _: &FocusRight, window, cx| {
            root.focus_toward(Direction::Right, window, cx)
        }))
        .on_action(
            cx.listener(|root, _: &FocusUp, window, cx| {
                root.focus_toward(Direction::Up, window, cx)
            }),
        )
        .on_action(cx.listener(|root, _: &FocusDown, window, cx| {
            root.focus_toward(Direction::Down, window, cx)
        }))
    }
}

/// What a name is being typed for — M12.1's tab, or M12.2's project.
pub(crate) enum Renamed {
    /// A session's tab.
    Tab(TabId),
    /// A project in the column, by its key.
    Project(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use guild_session::{OpenTab, Shows};

    #[test]
    fn isolated_repeated_tab_shortcuts() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::tabs::tests::repeated_tab_shortcuts",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_TAB_CYCLE_TEST", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("2 passed; 0 failed"),
            "both directions must execute"
        );
    }

    #[gpui::test]
    #[ignore = "run through isolated_repeated_tab_shortcuts"]
    fn repeated_tab_shortcuts_forward(cx: &mut gpui::TestAppContext) {
        check_repeated_tab_shortcuts(cx, "cmd-shift-]", true);
    }

    #[gpui::test]
    #[ignore = "run through isolated_repeated_tab_shortcuts"]
    fn repeated_tab_shortcuts_backward(cx: &mut gpui::TestAppContext) {
        check_repeated_tab_shortcuts(cx, "cmd-shift-[", false);
    }

    fn check_repeated_tab_shortcuts(cx: &mut gpui::TestAppContext, stroke: &str, forward: bool) {
        use gpui::AppContext;
        assert_eq!(std::env::var("GUILD_TAB_CYCLE_TEST").as_deref(), Ok("1"));
        cx.executor().allow_parking();
        let dir = tempfile::tempdir().unwrap();
        for name in ["a.rs", "b.rs", "c.rs"] {
            std::fs::write(dir.path().join(name), "fn main() {}\n").unwrap();
        }
        let mut entity = None;
        let (_, visual) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let mut desk = guild_session::Desk::default();
            desk.normalise();
            let project = desk.open_in(desk.active_workspace(), dir.path());
            let root = cx.new(|cx| {
                let mut root = Root::diagnostic(desk, dir.path().into(), cx);
                root.active_project = Some(project);
                root.isolated_render = true;
                root.restored_workspaces.insert(root.active_workspace);
                // Tabs outside the active strip must never join the cycle.
                let other_path = dir.path().join("other-project");
                std::fs::create_dir(&other_path).unwrap();
                let other = root.desk.open_in(root.active_workspace, &other_path);
                root.restore_settings_tab(other, cx);
                let seat = root.seat_for_project(project);
                let area = root.stage.landing(seat);
                root.stage.split(seat, area, Axis::Horizontal).unwrap();
                root.restore_settings_tab(project, cx);
                root.stage.focus(seat, area);
                for name in ["a.rs", "b.rs", "c.rs"] {
                    root.open_file(Path::new(name), window, cx);
                }
                // A tab's focused pane must also survive every circuit.
                let first = root
                    .tabs
                    .iter_mut()
                    .find(|tab| tab.id == root.active_tab.unwrap())
                    .unwrap();
                let pane = first.panes.focused();
                first.panes.split(pane, Axis::Horizontal).unwrap();
                first.panes.focus(pane);
                root
            });
            entity = Some(root.clone());
            gpui_component::Root::new(root, window, cx)
        });
        let root = entity.unwrap();
        visual.simulate_resize(gpui::size(gpui::px(1200.), gpui::px(800.)));
        visual.run_until_parked();
        let ids = visual.update(|_, cx| {
            root.read(cx)
                .visible_tabs()
                .map(|tab| tab.id)
                .collect::<Vec<_>>()
        });
        assert_eq!(ids.len(), 3);
        let panes = visual.update(|_, cx| {
            let root = root.read(cx);
            assert_eq!(root.tabs.len(), 5);
            root.tabs
                .iter()
                .map(|tab| (tab.id, tab.panes.focused()))
                .collect::<Vec<_>>()
        });
        let mut index = 2;
        for press in 0..2 {
            for step in 0..10 {
                if step != 0 {
                    visual
                        .executor()
                        .advance_clock(std::time::Duration::from_millis(1));
                }
                visual.simulate_event(gpui::KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(stroke).unwrap(),
                    is_held: step != 0,
                    prefer_character_input: false,
                });
                visual.run_until_parked();
                index = if forward {
                    (index + 1) % ids.len()
                } else {
                    (index + ids.len() - 1) % ids.len()
                };
                visual.update(|window, cx| {
                    let root = root.read(cx);
                    assert_eq!(
                        root.active_tab,
                        Some(ids[index]),
                        "{stroke}, press {press}, step {step}"
                    );
                    let focus = root.active_body().unwrap().focus_handle(cx).unwrap();
                    assert!(
                        focus.contains_focused(window, cx),
                        "the displayed editor must receive the keys"
                    );
                    assert_eq!(
                        root.tabs
                            .iter()
                            .map(|tab| (tab.id, tab.panes.focused()))
                            .collect::<Vec<_>>(),
                        panes
                    );
                });
                // Repeats must be discarded, including just before 125 ms.
                for elapsed in [0, 124] {
                    visual
                        .executor()
                        .advance_clock(std::time::Duration::from_millis(elapsed));
                    visual.simulate_event(gpui::KeyDownEvent {
                        keystroke: gpui::Keystroke::parse(stroke).unwrap(),
                        is_held: true,
                        prefer_character_input: false,
                    });
                    visual.update(|_, cx| {
                        assert_eq!(
                            root.read(cx).active_tab,
                            Some(ids[index]),
                            "repeat at +{elapsed} ms must be discarded"
                        )
                    });
                }
            }
            visual.simulate_event(gpui::KeyUpEvent {
                keystroke: gpui::Keystroke::parse(stroke).unwrap(),
            });
            visual.run_until_parked();
        }
        // No delayed switch may happen after release.
        visual
            .executor()
            .advance_clock(std::time::Duration::from_secs(1));
        visual.run_until_parked();
        visual.update(|_, cx| assert_eq!(root.read(cx).active_tab, Some(ids[index])));
        // Menu/programmatic actions must not be rate limited.
        for _ in 0..2 {
            visual.update(|window, cx| {
                window.dispatch_action(
                    if forward {
                        Box::new(NextTab)
                    } else {
                        Box::new(PreviousTab)
                    },
                    cx,
                )
            });
            visual.run_until_parked();
            index = if forward {
                (index + 1) % ids.len()
            } else {
                (index + ids.len() - 1) % ids.len()
            };
            visual.update(|_, cx| assert_eq!(root.read(cx).active_tab, Some(ids[index])));
        }
        // Project navigation remains unrestricted, even for rapid repeats.
        let project = visual.update(|_, cx| root.read(cx).active_project);
        for same in [false, true] {
            visual.simulate_event(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse("ctrl-tab").unwrap(),
                is_held: true,
                prefer_character_input: false,
            });
            visual.update(|_, cx| assert_eq!(root.read(cx).active_project == project, same));
        }
        // A pending sequence must use GPUI's sequence resolution, not the
        // single-stroke tab binding on its final key.
        visual.update(|_, cx| {
            cx.bind_keys([gpui::KeyBinding::new(
                &format!("cmd-k {stroke}"),
                NextProject,
                None,
            )])
        });
        visual.run_until_parked();
        visual.simulate_keystrokes("cmd-k");
        visual.simulate_event(gpui::KeyDownEvent {
            keystroke: gpui::Keystroke::parse(stroke).unwrap(),
            is_held: true,
            prefer_character_input: false,
        });
        visual.update(|_, cx| assert_ne!(root.read(cx).active_project, project));
        visual.simulate_keystrokes("ctrl-tab");
        visual.update(|_, cx| assert_eq!(root.read(cx).active_project, project));
        // A null binding must really disable the key; no hardcoded shortcut.
        visual.update(|_, cx| cx.bind_keys([gpui::KeyBinding::new(stroke, gpui::NoAction, None)]));
        visual.run_until_parked();
        for is_held in [false, true] {
            visual.simulate_event(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse(stroke).unwrap(),
                is_held,
                prefer_character_input: false,
            });
            visual.update(|_, cx| assert_eq!(root.read(cx).active_tab, Some(ids[index])));
        }
        // Reassigning this stroke must invoke its new action without a limit.
        visual.update(|_, cx| cx.bind_keys([gpui::KeyBinding::new(stroke, ToggleSessions, None)]));
        visual.run_until_parked();
        for mode in [
            guild_session::settings::ColumnMode::Sessions,
            guild_session::settings::ColumnMode::Projects,
        ] {
            visual.simulate_event(gpui::KeyDownEvent {
                keystroke: gpui::Keystroke::parse(stroke).unwrap(),
                is_held: true,
                prefer_character_input: false,
            });
            visual.update(|_, cx| assert_eq!(root.read(cx).settings.column_mode, mode));
        }
        guild_session::store::flush();
    }

    // Diagnostic Root blocks external integrations, but save() still resolves
    // global files through paths. Isolate both scenarios before creating Root.
    #[test]
    fn isolated_markdown_default_view_panes() {
        let state = tempfile::Builder::new()
            .prefix("guild-markdown-panes-")
            .tempdir()
            .unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::tabs::tests::owned_markdown_default_view_",
                "--ignored",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GIT_CEILING_DIRECTORIES", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("GUILD_MARKDOWN_PANES_TEST_STATE", state.path())
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&result.stdout);
        assert!(
            result.status.success(),
            "{stdout}\n{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(
            stdout.contains("2 passed; 0 failed"),
            "both ignored scenarios must execute: {stdout}"
        );
        // Read actual disk bytes after the child's flush, not the store queue.
        // This also proves isolation when an unrelated outer state is supplied.
        let saved: guild_session::Desk = serde_json::from_slice(
            &std::fs::read(state.path().join("opened-files/workspace.json")).unwrap(),
        )
        .unwrap();
        let projects: Vec<_> = saved.projects().map(|p| p.path.clone()).collect();
        assert_eq!(
            projects,
            (0..3)
                .map(|index| state
                    .path()
                    .join("opened-files")
                    .join(format!("project-{index}")))
                .collect::<Vec<_>>()
        );
    }

    fn markdown_panes_test_state() -> PathBuf {
        let state = PathBuf::from(
            std::env::var_os("GUILD_MARKDOWN_PANES_TEST_STATE")
                .expect("run through isolated_markdown_default_view_panes"),
        );
        assert!(state.is_absolute());
        assert_eq!(
            crate::paths::workspace_file(),
            state.join("workspace.json"),
            "global workspace must resolve inside the wrapper-owned state before any Root action"
        );
        assert_eq!(crate::paths::settings_file(), state.join("settings.json"));
        assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref(), Ok("1"));
        state
    }
    #[gpui::test]
    #[ignore = "run through isolated_markdown_default_view_panes"]
    fn owned_markdown_default_view_reaches_new_panes_across_projects(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;
        use guild_session::markdown::Mode;
        let isolated = markdown_panes_test_state();
        let state = isolated.join("opened-files");
        std::fs::create_dir(&state).unwrap();
        let root = cx.new(|cx| Root::diagnostic(Default::default(), state.as_path().into(), cx));
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                crate::editor::init(cx);
                root.update(cx, |root, cx| {
                    let mut previous: Vec<(crate::markdown::Presentation, Mode)> = Vec::new();
                    for (index, mode) in [Mode::Editor, Mode::Split, Mode::Preview]
                        .into_iter()
                        .enumerate()
                    {
                        root.settings = serde_json::from_value(
                            serde_json::json!({"markdown_default_view":mode}),
                        )
                        .unwrap();
                        let path = state.as_path().join(format!("project-{index}"));
                        std::fs::create_dir(&path).unwrap();
                        let project = root.desk.open_in(root.desk.active_workspace(), &path);
                        for name in ["README.MD", "guide.MarkDown", "source.rs"] {
                            std::fs::write(path.join(name), "# Local fixture").unwrap();
                            root.active_project = Some(project);
                            root.open_file(Path::new(name), window, cx);
                            let id = root.active_tab.unwrap();
                            let tab = root.tabs.iter().find(|t| t.id == id).unwrap();
                            let Body::File { presentation, .. } = tab.bodies().next().unwrap()
                            else {
                                panic!("file body")
                            };
                            if name == "source.rs" {
                                assert!(presentation.is_none());
                            } else {
                                let p = presentation.as_ref().unwrap();
                                assert_eq!(p.mode(), mode, "{name} in project {index}");
                                previous.push((p.clone(), mode));
                            }
                        }
                        for (pane, expected) in &previous {
                            assert_eq!(
                                pane.mode(),
                                *expected,
                                "a changed preference must leave open panes alone"
                            );
                        }
                    }
                });
            })
            .unwrap();
        guild_session::store::flush();
    }

    #[gpui::test]
    #[ignore = "run through isolated_markdown_default_view_panes"]
    fn owned_markdown_default_view_yields_to_saved_single_and_divided_modes(
        cx: &mut gpui::TestAppContext,
    ) {
        use gpui::AppContext;
        use guild_session::markdown::Mode;
        let isolated = markdown_panes_test_state();
        let before = std::fs::read(crate::paths::workspace_file()).ok();
        let state = isolated.join("restored-files");
        std::fs::create_dir(&state).unwrap();
        for name in ["legacy.md", "saved.md", "left.md", "right.md"] {
            std::fs::write(state.as_path().join(name), "# Restore fixture").unwrap();
        }
        let root = cx.new(|cx| Root::diagnostic(Default::default(), state.as_path().into(), cx));
        root.update(cx, |root, cx| {
            root.settings =
                serde_json::from_value(serde_json::json!({"markdown_default_view":"preview"}))
                    .unwrap();
            let project = root
                .desk
                .open_in(root.desk.active_workspace(), state.as_path());
            let seat = root.seat_for_project(project);
            let area = root.stage.areas_mut(seat).focused();
            let mut panes = guild_session::Panes::one(Shows::File("left.md".into()));
            let first = panes.focused();
            let second = panes
                .split(first, guild_session::layout::Axis::Horizontal)
                .unwrap();
            panes.fill(second, Shows::File("right.md".into())).unwrap();
            let mut divided = OpenTab::from_panes(panes).unwrap();
            divided.markdown_modes.insert(first, Mode::Editor);
            // The second pane is legacy and must inherit Preview.
            let mut saved = OpenTab::showing(Shows::File("saved.md".into()));
            saved.markdown_mode = Some(Mode::Split);
            let strip = guild_session::AreaState {
                tabs: vec![
                    OpenTab::showing(Shows::File("legacy.md".into())),
                    saved,
                    divided,
                ],
                active: Some(0),
            };
            root.restore_strip(seat, state.as_path(), area, &strip, cx);
            let modes: Vec<_> = root
                .tabs
                .iter()
                .flat_map(Tab::bodies)
                .filter_map(|b| match b {
                    Body::File {
                        path,
                        presentation: Some(p),
                        ..
                    } => Some((path.to_str().unwrap(), p.mode())),
                    _ => None,
                })
                .collect();
            assert_eq!(
                modes,
                vec![
                    ("legacy.md", Mode::Preview),
                    ("saved.md", Mode::Split),
                    ("left.md", Mode::Editor),
                    ("right.md", Mode::Preview)
                ]
            );
        });
        guild_session::store::flush();
        assert_eq!(
            std::fs::read(crate::paths::workspace_file()).ok(),
            before,
            "restoring panes must not save global state"
        );
    }

    // The action's private routing is tested here without constructing a
    // window or launching a real agent.
    #[test]
    fn new_session_follows_settings_changed_between_invocations() {
        use guild_session::{DefaultTab, HarnessKind, SessionMode, Settings};

        let all = HarnessKind::ALL;
        let mut settings = Settings::default();
        assert_eq!(
            runs_of(settings.default_tab_among(&all)),
            Runs::Claude(SessionMode::Terminal)
        );

        settings.default_tab = Some(DefaultTab::Codex);
        assert_eq!(
            runs_of(settings.default_tab_among(&all)),
            Runs::Codex(SessionMode::Terminal)
        );
        assert_eq!(
            runs_of(settings.default_tab_among(&all)),
            Runs::Codex(SessionMode::Terminal)
        );

        settings.default_tab = Some(DefaultTab::OpenCode);
        assert_eq!(
            runs_of(settings.default_tab_among(&all)),
            Runs::OpenCode(SessionMode::Terminal)
        );

        settings.default_tab = Some(DefaultTab::Terminal);
        assert_eq!(runs_of(settings.default_tab_among(&all)), Runs::Shell);

        settings.default_tab = Some(DefaultTab::Claude);
        assert_eq!(
            runs_of(settings.default_tab_among(&all)),
            Runs::Claude(SessionMode::Terminal)
        );
    }

    /// GLD-081: `cmd-t` with no default chosen opens the first agent
    /// installed, and a chosen one even when it is missing.
    #[test]
    fn new_session_without_a_choice_opens_the_first_installed_agent() {
        use guild_session::{DefaultTab, HarnessKind, SessionMode, Settings};

        let mut settings = Settings::default();
        let codex_and_opencode = [HarnessKind::OpenCode, HarnessKind::Codex];
        assert_eq!(
            runs_of(settings.default_tab_among(&codex_and_opencode)),
            Runs::Codex(SessionMode::Terminal)
        );
        assert_eq!(
            runs_of(settings.default_tab_among(&[HarnessKind::OpenCode])),
            Runs::OpenCode(SessionMode::Terminal)
        );
        assert_eq!(runs_of(settings.default_tab_among(&[])), Runs::Shell);

        settings.default_tab = Some(DefaultTab::Claude);
        assert_eq!(
            runs_of(settings.default_tab_among(&codex_and_opencode)),
            Runs::Claude(SessionMode::Terminal)
        );
    }

    #[test]
    fn legacy_review_restoration_preserves_mixed_panes_and_surviving_front() {
        let folder = tempfile::tempdir().unwrap();
        let legacy: Shows = serde_json::from_str("\"review\"").unwrap();
        let mut panes = guild_session::Panes::one(Shows::Settings);
        let first = panes.focused();
        let review = panes.split(first, Axis::Horizontal).unwrap();
        panes.fill(review, legacy.clone()).unwrap();
        panes.focus(review);
        let ids = panes.ids();
        let tree = panes.tree().clone();
        let mixed = OpenTab::from_panes(panes).unwrap();
        assert_eq!(mixed.shows, legacy);
        let tabs = vec![
            OpenTab::showing(legacy.clone()),
            mixed,
            OpenTab::showing(legacy),
        ];
        let (kept, front, missing) = restorable(&tabs, Some(1), folder.path());
        assert_eq!(kept.len(), 1);
        assert_eq!(front, Some(0));
        assert!(missing.is_empty());
        let panes = kept[0].panes_or_one();
        assert_eq!(panes.ids(), ids);
        assert_eq!(panes.tree(), &tree);
        assert_eq!(panes.focused(), first);
        assert_eq!(panes.body_of(first), Some(&Shows::Settings));
        assert!(panes.body_of(review).is_none());
        let wire = serde_json::to_string(&kept).unwrap();
        assert!(!wire.contains("review"));
        let saved: Vec<OpenTab> = serde_json::from_str(&wire).unwrap();
        assert_eq!(restorable(&saved, front, folder.path()).0, kept);
        assert!(
            restorable(&[tabs[0].clone(), tabs[2].clone()], Some(0), folder.path())
                .0
                .is_empty()
        );
    }

    /// A file that is no longer on disk is dropped from the list and
    /// named; the others open, and the index of the one in front moves
    /// with them — or goes, if it was the one dropped.
    #[test]
    fn a_tab_whose_file_is_gone_is_dropped_and_named() {
        let folder = tempfile::Builder::new()
            .prefix("guild-restore-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        std::fs::write(dir.join("a.rs"), "").unwrap();

        let id = SessionId::new();
        let tabs = vec![
            OpenTab::showing(Shows::Session(id)),
            OpenTab::showing(Shows::File(PathBuf::from("gone.rs"))),
            OpenTab::showing(Shows::File(PathBuf::from("a.rs"))),
            OpenTab::showing(Shows::Settings),
            OpenTab::showing(Shows::Diff {
                path: Some(PathBuf::from("gone.rs")),
                base: guild_session::Base::Head,
            }),
            OpenTab::showing(Shows::Diff {
                path: None,
                base: guild_session::Base::Head,
            }),
        ];

        let (kept, active, dropped) = restorable(&tabs, Some(3), dir);
        assert_eq!(
            kept.iter().map(|tab| &tab.shows).collect::<Vec<_>>(),
            vec![
                &Shows::Session(id),
                &Shows::File(PathBuf::from("a.rs")),
                &Shows::Settings,
                &Shows::Diff {
                    path: Some(PathBuf::from("gone.rs")),
                    base: guild_session::Base::Head,
                },
                &Shows::Diff {
                    path: None,
                    base: guild_session::Base::Head,
                },
            ],
            "a diff of a gone file is kept: that diff is the point"
        );
        assert_eq!(active, Some(2), "the front one moved up with the list");
        assert_eq!(dropped, vec![(PathBuf::from("gone.rs"), Lost::Tab)]);

        let (_, active, _) = restorable(&tabs, Some(1), dir);
        assert_eq!(active, None, "the front one was the one dropped");
    }

    /// M12.8: the rule is a pane's, not a tab's. A divided tab loses the
    /// pane whose file is gone and keeps the rest; a divided tab none of
    /// whose files is left goes, and the front index moves past it.
    #[test]
    fn a_divided_tab_loses_only_the_pane_whose_file_is_gone() {
        use guild_session::Panes;
        use guild_session::layout::Axis;
        let folder = tempfile::Builder::new()
            .prefix("guild-restore-")
            .tempdir()
            .expect("a scratch folder");
        let dir = folder.path();
        std::fs::write(dir.join("a.rs"), "").unwrap();

        let divided = |one: Shows, two: Shows| {
            let mut panes = Panes::one(one);
            let second = panes.split(panes.focused(), Axis::Horizontal).unwrap();
            panes.fill(second, two).unwrap();
            OpenTab::from_panes(panes).unwrap()
        };
        let tabs = vec![
            divided(
                Shows::File(PathBuf::from("gone.rs")),
                Shows::File(PathBuf::from("a.rs")),
            ),
            divided(
                Shows::File(PathBuf::from("lost.rs")),
                Shows::File(PathBuf::from("gone.rs")),
            ),
            OpenTab::showing(Shows::Settings),
        ];

        let (kept, active, dropped) = restorable(&tabs, Some(2), dir);
        assert_eq!(kept.len(), 2, "the tab with nothing left is not kept");
        assert_eq!(kept[0].shows, Shows::File(PathBuf::from("a.rs")));
        assert_eq!(kept[0].panes_or_one().ids().len(), 1);
        assert_eq!(active, Some(1), "the front one moved up past it");
        assert_eq!(
            dropped,
            vec![
                (PathBuf::from("gone.rs"), Lost::Pane),
                (PathBuf::from("lost.rs"), Lost::Tab),
                (PathBuf::from("gone.rs"), Lost::Tab),
            ],
            "every pane's lost file is named, with what it cost: the log \
             says a tab was lost only when it was (M12.8's second window \
             check, O8)"
        );
    }
}
