//! One window-wide, in-memory trail of what was in front.
//!
//! File places outlive tabs and reopen by path (M8.8 decisions 7 and 9).
//! Other bodies live only while shown in their seat. Every front switch counts,
//! including switches between workspaces; edit locations remain per project.

use crate::editor::EditorReveal as _;
use guild_session::{AreaId, Base, PaneId, ProjectId, Seat, SessionId, WorkspaceId};
use std::collections::HashMap;
use std::path::PathBuf;

/// A place a caret stood: a file of the project, a line and a column.
///
/// The enclosing Step or edit trail supplies the project, keeping the
/// path-based place API shared by every existing jump caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    /// Relative to the project, as every file tab's path is.
    pub path: PathBuf,
    /// Counted from zero, as the editor counts.
    pub line: u32,
    /// Counted from zero.
    pub column: u32,
}

/// How near two places in one file have to be to count as one place.
///
/// **The single load-bearing constant of the whole feature.** Without the
/// merge, Back walks one caret movement at a time and is useless; too coarse,
/// and Back cannot move inside a long file at all. Four lines is the distance
/// at which two positions stop being *the same piece of code* — near enough
/// that scrolling through a function never fills the stack, far enough that
/// the two ends of a short one are still two places.
///
/// The comparison is strictly less than: three lines apart is one place,
/// four lines apart is two.
pub const MERGE: u32 = 4;

/// How many places each deque holds.
pub const DEPTH: usize = 150;

/// How many rows Recent Locations offers out of the depth.
pub const LOCATIONS: usize = 25;

impl Place {
    /// A place, from the parts every caller has.
    pub fn new(path: impl Into<PathBuf>, line: u32, column: u32) -> Self {
        Self {
            path: path.into(),
            line,
            column,
        }
    }

    /// Are these two the same place?
    ///
    /// The same file and fewer than [`MERGE`] lines apart. The column is
    /// not in it: two positions on one line are the same place to anyone
    /// pressing Back.
    pub fn same_as(&self, other: &Place) -> bool {
        self.path == other.path && self.line.abs_diff(other.line) < MERGE
    }
}

/// The content in front, independent of its tab except for empty locations
/// and IDE proposals, whose view is their only durable identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Front {
    File(Place),
    Session(SessionId),
    Diff {
        path: Option<PathBuf>,
        base: Base,
    },
    Proposal(gpui::EntityId),
    Annotate {
        path: PathBuf,
    },
    Settings,
    Process(guild_session::workflow::RunId),
    Empty {
        area: Option<AreaId>,
        pane: Option<(crate::TabId, PaneId)>,
    },
}

/// A front in a workspace, which may have no project at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub workspace: WorkspaceId,
    pub project: Option<ProjectId>,
    pub front: Front,
}
impl Step {
    pub fn same_as(&self, other: &Self) -> bool {
        self.workspace == other.workspace
            && self.project == other.project
            && match (&self.front, &other.front) {
                (Front::File(a), Front::File(b)) => a.same_as(b),
                (a, b) => a == b,
            }
    }
}

/// One project's edits and the cursor of an unbroken Last Edit run.
#[derive(Debug, Default)]
pub struct Trail {
    changes: Vec<Place>,
    cursor: usize,
}
impl Trail {
    pub fn changed(&mut self, at: Place) {
        put_last_or_merge(&mut self.changes, at);
        self.settle_cursor();
    }
    pub fn last_edit(&mut self, standing: Option<&Place>) -> Option<Place> {
        while self.cursor > 0 {
            self.cursor -= 1;
            let candidate = &self.changes[self.cursor];
            if !standing.is_some_and(|here| here.same_as(candidate)) {
                return Some(candidate.clone());
            }
        }
        None
    }
    pub fn edited(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for place in self.changes.iter().rev() {
            if !files.contains(&place.path) {
                files.push(place.path.clone());
            }
        }
        files
    }
    fn settle_cursor(&mut self) {
        self.cursor = self.changes.len();
    }
}
fn put_last_or_merge(list: &mut Vec<Place>, next: Place) {
    if list.last().is_some_and(|last| last.same_as(&next)) {
        list.pop();
    }
    push_capped(list, next);
}
fn push_capped<T>(list: &mut Vec<T>, next: T) {
    list.push(next);
    if list.len() > DEPTH {
        list.remove(0);
    }
}
fn put_step(list: &mut Vec<Step>, next: Step) {
    if list.last().is_some_and(|last| last.same_as(&next)) {
        list.pop();
    }
    push_capped(list, next);
}
/// Where `cmd-g` was asked to go.
///
/// **Three cases, not two.** The field takes `12`, `12:5` and `:5`,
/// and the third is the one that is forgotten: a person who wants column
/// 40 of the line they are on should not have to type the line number
/// they are already standing on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    /// One-based, as a person counts and as the field is typed.
    pub line: Option<u32>,
    /// One-based likewise.
    pub column: Option<u32>,
}

/// Read what was typed into the Go to Line field.
///
/// `None` for anything that is not one of the three cases — a field that
/// jumped to line 1 on a typo would be worse than one that does nothing,
/// because the place it left is then on the back stack and the place it
/// went to is a lie.
pub fn parse_target(text: &str) -> Option<Target> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let number = |part: &str| -> Option<Option<u32>> {
        match part.trim() {
            "" => Some(None),
            digits => digits.parse::<u32>().ok().filter(|n| *n > 0).map(Some),
        }
    };

    let (line, column) = match text.split_once(':') {
        Some((line, column)) => (number(line)?, number(column)?),
        None => (number(text)?, None),
    };
    if line.is_none() && column.is_none() {
        return None;
    }
    Some(Target { line, column })
}

/// Window-wide Back/Forward, project-owned edits, and transient landing state.
/// None of this is persisted: a restart begins with empty navigation stacks.
#[derive(Debug, Default)]
pub struct History {
    back: Vec<Step>,
    forward: Vec<Step>,
    trails: HashMap<ProjectId, Trail>,
    pub(crate) standing: Option<Step>,
    landing: Option<(ProjectId, Place)>,
    selection: Option<(ProjectId, Place, usize)>,
    library_landing: Option<(crate::TabId, crate::Origin, PathBuf, Caret)>,
    pub(crate) suspended: usize,
    goto: Option<String>,
    heard: HashMap<gpui::EntityId, gpui::Subscription>,
}
impl History {
    pub fn trail(&mut self, project: ProjectId) -> &mut Trail {
        self.trails.entry(project).or_default()
    }
    pub(crate) fn peek(&self, project: ProjectId) -> Option<&Trail> {
        self.trails.get(&project)
    }
    pub(crate) fn standing_in(&self, project: ProjectId) -> Option<&Place> {
        match &self.standing {
            Some(Step {
                project: Some(owner),
                front: Front::File(place),
                ..
            }) if *owner == project => Some(place),
            _ => None,
        }
    }
    fn settle_cursors(&mut self) {
        for trail in self.trails.values_mut() {
            trail.settle_cursor();
        }
    }
    pub fn moved(&mut self, from: Step) {
        put_step(&mut self.back, from);
        self.forward.clear();
        self.settle_cursors();
    }
    pub fn has_back(&self, standing: Option<&Step>, alive: impl Fn(&Step) -> bool) -> bool {
        self.back
            .iter()
            .any(|step| alive(step) && !standing.is_some_and(|here| here.same_as(step)))
    }
    pub fn has_forward(&self, standing: Option<&Step>, alive: impl Fn(&Step) -> bool) -> bool {
        self.forward
            .iter()
            .any(|step| alive(step) && !standing.is_some_and(|here| here.same_as(step)))
    }
    pub fn back(&mut self, standing: Option<&Step>, alive: impl Fn(&Step) -> bool) -> Option<Step> {
        self.settle_cursors();
        let target = take_step(&mut self.back, standing, alive)?;
        if let Some(here) = standing {
            push_capped(&mut self.forward, here.clone());
        }
        Some(target)
    }
    pub fn forward(
        &mut self,
        standing: Option<&Step>,
        alive: impl Fn(&Step) -> bool,
    ) -> Option<Step> {
        self.settle_cursors();
        let target = take_step(&mut self.forward, standing, alive)?;
        if let Some(here) = standing {
            put_step(&mut self.back, here.clone());
        }
        Some(target)
    }
    pub fn locations(
        &self,
        workspace: WorkspaceId,
        project: ProjectId,
        standing: Option<&Step>,
        limit: usize,
    ) -> Vec<Place> {
        let mut rows: Vec<Place> = Vec::new();
        for step in standing.into_iter().chain(self.back.iter().rev()) {
            if rows.len() >= limit {
                break;
            }
            if step.workspace == workspace
                && step.project == Some(project)
                && let Front::File(place) = &step.front
                && !rows.last().is_some_and(|last| last.same_as(place))
            {
                rows.push(place.clone());
            }
        }
        rows
    }
    pub(crate) fn leave(&mut self, project: ProjectId) {
        if self
            .standing
            .as_ref()
            .is_some_and(|step| step.project == Some(project))
            && let Some(here) = self.standing.take()
        {
            self.moved(here);
        }
    }
}
fn take_step(
    list: &mut Vec<Step>,
    standing: Option<&Step>,
    alive: impl Fn(&Step) -> bool,
) -> Option<Step> {
    // Prune dead targets even when there is no live destination. Keep an
    // equivalent standing target until a real step consumes it.
    list.retain(|step| alive(step));
    let index = list
        .iter()
        .rposition(|step| !standing.is_some_and(|here| here.same_as(step)))?;
    let target = list[index].clone();
    list.truncate(index);
    Some(target)
}

/// Where a caret goes in a file opened by a jump, in whichever counting
/// the jump arrived with.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Caret {
    /// The editor's own counting — a line and a character column, zero
    /// based, as `Place` counts — a terminal's `path:line:column` outside
    /// the project lands with it.
    Characters { line: u32, column: u32 },
    /// A language server's position, in UTF-16 units, converted against
    /// the text once the editor has it.
    Protocol(lsp_types::Position),
}

use gpui::prelude::*;
use gpui::{Context, Window};

use crate::actions::{Back, Forward, GoToLine, LastEditLocation};
use crate::{Body, Root};

impl Root {
    /// Every navigation handler, on the window's root.
    ///
    /// One binder per subject, in the subject's own file — the shape
    /// M8.8.0 gave the chain so that six branches add their handlers
    /// without meeting in `render.rs`.
    pub(crate) fn bind_navigation(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &Back, window, cx| root.go_back(window, cx)))
            .on_action(cx.listener(|root, _: &Forward, window, cx| root.go_forward(window, cx)))
            .on_action(cx.listener(|root, _: &LastEditLocation, window, cx| {
                root.go_to_last_edit(window, cx)
            }))
            .on_action(
                cx.listener(|root, _: &GoToLine, window, cx| root.toggle_goto_line(window, cx)),
            )
    }

    /// Show the Go to Line field, or put it away.
    pub(crate) fn toggle_goto_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.history.goto.take() {
            Some(_) => self.refocus(window, cx),
            None => {
                // Nothing to go to in a terminal or a diff, and a field
                // that opens over one is a field whose Enter does
                // nothing.
                if self.caret_now(cx).is_none() {
                    return;
                }
                // Every other field of ours goes, or this one is drawn
                // over one that still has the keys — `put_fields_away`.
                self.put_fields_away();

                self.history.goto = Some(String::new());
                // The same focus rule the Recent Files popup learned:
                // `enter` is bound in the editor's own context and a
                // binding is dispatched before both key phases, so a
                // field drawn over a focused editor never sees Return.
                window.focus(&self.focus, cx);
            }
        }
        cx.notify();
    }

    /// Put the field away and nothing else — no focus, no notify.
    ///
    /// What `put_fields_away` calls; `History`'s fields are this
    /// module's, and a sibling cannot reach them.
    pub(crate) fn shut_goto_line(&mut self) {
        self.history.goto = None;
    }

    /// What has been typed into the Go to Line field, if it is up.
    pub(crate) fn set_goto_line_query(&mut self, value: String) {
        self.history.goto = Some(value);
    }
    pub(crate) fn goto_line_query(&self) -> Option<&str> {
        self.history.goto.as_deref()
    }

    /// A key pressed while the Go to Line field is up.
    pub(crate) fn goto_line_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.history.goto.is_none() {
            return false;
        }
        let stroke = &event.keystroke;

        match stroke.key.as_str() {
            "escape" => {
                self.history.goto = None;
                self.refocus(window, cx);
            }
            "enter" => self.jump_to_typed_place(window, cx),
            _ => return false,
        }
        cx.notify();
        true
    }

    /// Go where the field says.
    ///
    /// **Through the door that records places**, like every other jump
    /// of ours — a `cmd-g` that `cmd-[` could not undo would be the one
    /// movement in the window with no way back. A line alone keeps the
    /// column, a column alone keeps the line, and nothing understood
    /// leaves the field where it is rather than jumping somewhere
    /// invented.
    fn jump_to_typed_place(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.history.goto.as_deref().and_then(parse_target) else {
            return;
        };
        let Some((project, here)) = self.caret_now(cx) else {
            return;
        };
        self.history.goto = None;
        let place = Place::new(
            here.path.clone(),
            target.line.map(|line| line - 1).unwrap_or(here.line),
            target
                .column
                .map(|column| column - 1)
                .unwrap_or(here.column),
        );
        self.go_to_place(project, place, window, cx);
    }

    /// Listen to every open buffer, so that an edit is a place.
    ///
    /// From the render rather than from where a tab is made, because a
    /// **restored tab has no editor until it is first drawn** — the trap
    /// the autosave table met as "no editor means not text" and M8.5 met
    /// as a file its server was never told about.
    ///
    /// The subscription is the *only* way an edit is noticed: the caret
    /// sampler cannot tell typing from moving, and Last Edit Location
    /// walks edits.
    pub(crate) fn hear_edits(&mut self, cx: &mut Context<Self>) {
        let live: Vec<(
            ProjectId,
            PathBuf,
            gpui::Entity<gpui_component::input::EditorState>,
        )> = self
            .tabs
            .iter()
            .flat_map(|tab| tab.bodies().map(move |body| (tab.project, body)))
            .filter_map(|(project, body)| match body {
                // **The project's own files only**, here as everywhere a
                // path leaves a file tab: an edit inside a dependency's
                // source is not a place in this project, and recording
                // it would put a crate's `src/lib.rs` on this project's
                // trail under this project's root.
                Body::File {
                    path,
                    origin: crate::Origin::Project,
                    tab: view,
                    ..
                } => Some((project, path.clone(), view.read(cx).editor()?.clone())),
                _ => None,
            })
            .collect();

        let alive: std::collections::HashSet<gpui::EntityId> = live
            .iter()
            .map(|(_, _, editor)| editor.entity_id())
            .collect();
        self.history.heard.retain(|id, _| alive.contains(id));

        for (project, path, editor) in live {
            if self.history.heard.contains_key(&editor.entity_id()) {
                continue;
            }
            let id = editor.entity_id();
            let heard = cx.subscribe(
                &editor,
                move |root, editor, event: &gpui_component::input::InputEvent, cx| {
                    if !matches!(event, gpui_component::input::InputEvent::Change) {
                        return;
                    }
                    let at = editor.read(cx).cursor_position();
                    root.history.trail(project).changed(Place::new(
                        path.clone(),
                        at.line,
                        at.character,
                    ));
                },
            );
            self.history.heard.insert(id, heard);
        }
    }

    /// Last Edit remains a project-local walk; Back undoes it from any body.
    pub(crate) fn go_to_last_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.note_place(cx);
        let Some(project) = self.active_project else {
            return;
        };
        let standing = self.history.standing_in(project).cloned();
        let Some(target) = self.history.trail(project).last_edit(standing.as_ref()) else {
            return;
        };
        if let Some(here) = self.history.standing.take() {
            put_step(&mut self.history.back, here);
            self.history.forward.clear();
        }
        self.stand_at(project, target, window, cx);
    }

    fn caret_now(&self, cx: &gpui::App) -> Option<(ProjectId, Place)> {
        let step = self.front_now(cx)?;
        match step.front {
            Front::File(place) => Some((step.project?, place)),
            _ => None,
        }
    }

    /// The content actually in front; an undrawn editor has no invented caret.
    fn front_now(&self, cx: &gpui::App) -> Option<Step> {
        let front = match self.active_body() {
            Some(Body::File {
                path,
                origin: crate::Origin::Project,
                tab,
                ..
            }) => {
                let editor = tab.read(cx).editor()?;
                let at = editor.read(cx).cursor_position();
                Front::File(Place::new(path.clone(), at.line, at.character))
            }
            Some(Body::File { .. }) => return None,
            Some(Body::Terminal { session, .. } | Body::Broken { session, .. }) => {
                Front::Session(*session)
            }
            Some(Body::Diff {
                base: Base::Proposal,
                tab,
                ..
            }) => Front::Proposal(tab.entity_id()),
            Some(Body::Diff { path, base, .. }) => Front::Diff {
                path: path.clone(),
                base: base.clone(),
            },
            Some(Body::Annotate { path, .. }) => Front::Annotate { path: path.clone() },
            Some(Body::Settings { .. }) => Front::Settings,
            Some(Body::Process { run }) => Front::Process(*run),
            None => Front::Empty {
                area: self.focused_area(),
                pane: self
                    .active_tab
                    .and_then(|id| self.tabs.iter().find(|tab| tab.id == id))
                    .map(|tab| (tab.id, tab.panes.focused())),
            },
        };
        Some(Step {
            workspace: self.active_workspace,
            project: self.active_project,
            front,
        })
    }

    pub(crate) fn note_place(&mut self, cx: &gpui::App) {
        if self.history.suspended > 0 || self.history.landing.is_some() || self.landing.is_some() {
            return;
        }
        let Some(now) = self.front_now(cx) else {
            return;
        };
        match self.history.standing.take() {
            Some(before) if before == now => {}
            Some(before) => self.history.moved(before),
            None => {}
        }
        self.history.standing = Some(now);
    }

    fn body_is_front(body: &Body, front: &Front) -> bool {
        match (body, front) {
            (
                Body::Terminal { session, .. } | Body::Broken { session, .. },
                Front::Session(wanted),
            ) => session == wanted,
            (
                Body::Diff {
                    base: Base::Proposal,
                    tab,
                    ..
                },
                Front::Proposal(wanted),
            ) => tab.entity_id() == *wanted,
            (
                Body::Diff { path, base, .. },
                Front::Diff {
                    path: wanted,
                    base: wanted_base,
                },
            ) => path == wanted && base == wanted_base,
            (Body::Annotate { path, .. }, Front::Annotate { path: wanted }) => path == wanted,
            (Body::Process { run }, Front::Process(wanted)) => run == wanted,
            (Body::Settings { .. }, Front::Settings) => true,
            _ => false,
        }
    }
    fn showing_step(&self, step: &Step) -> Option<(crate::TabId, PaneId)> {
        let seat = Seat {
            workspace: step.workspace,
            project: step.project?,
        };
        self.find_showing(seat, |body| Self::body_is_front(body, &step.front))
    }
    fn step_alive(&self, step: &Step) -> bool {
        if self.desk.workspace(step.workspace).is_none() {
            return false;
        }
        if let Some(project) = step.project {
            if !self.desk.holders(project).contains(&step.workspace) {
                return false;
            }
        } else {
            return matches!(
                step.front,
                Front::Empty {
                    area: None,
                    pane: None
                }
            ) && self
                .desk
                .workspace(step.workspace)
                .is_some_and(|workspace| workspace.front.is_none());
        }
        match &step.front {
            Front::File(_) => true,
            Front::Session(session) => self.desk.session(*session).is_some_and(|record| {
                record.open
                    && record.workspace == step.workspace
                    && Some(record.project) == step.project
                    && (!self.restored_workspaces.contains(&step.workspace)
                        || self.showing_step(step).is_some())
            }),
            Front::Empty { area, pane } => {
                let seat = Seat {
                    workspace: step.workspace,
                    project: step.project.unwrap(),
                };
                match pane {
                    Some((tab, pane)) => self.tabs.iter().any(|t| {
                        t.id == *tab
                            && t.seat == seat
                            && Some(t.area) == *area
                            && t.panes.ids().contains(pane)
                            && t.panes.body_of(*pane).is_none()
                    }),
                    None => match area {
                        Some(area) => {
                            self.stage
                                .areas(seat)
                                .is_some_and(|areas| areas.contains(*area))
                                && !self
                                    .tabs
                                    .iter()
                                    .any(|tab| tab.seat == seat && tab.area == *area)
                        }
                        None => {
                            self.stage.areas(seat).is_none()
                                && !self.tabs.iter().any(|tab| tab.seat == seat)
                        }
                    },
                }
            }
            _ => self.showing_step(step).is_some(),
        }
    }
    pub(crate) fn history_directions(&self) -> (bool, bool) {
        let alive = |step: &Step| self.step_alive(step);
        (
            self.history.has_back(self.history.standing.as_ref(), alive),
            self.history
                .has_forward(self.history.standing.as_ref(), alive),
        )
    }
    pub(crate) fn go_back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step_history(true, window, cx);
    }
    pub(crate) fn go_forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.step_history(false, window, cx);
    }
    fn step_history(&mut self, back: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.note_place(cx);
        let mut history = std::mem::take(&mut self.history);
        let standing = history.standing.clone();
        let target = if back {
            history.back(standing.as_ref(), |step| self.step_alive(step))
        } else {
            history.forward(standing.as_ref(), |step| self.step_alive(step))
        };
        self.history = history;
        if let Some(target) = target {
            self.stand_step(target, window, cx);
        }
    }
    fn stand_step(&mut self, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        self.history.suspended += 1;
        if let Some(project) = step.project {
            if self.active_seat()
                != Some(Seat {
                    workspace: step.workspace,
                    project,
                })
            {
                self.select_seat(
                    Seat {
                        workspace: step.workspace,
                        project,
                    },
                    cx,
                );
            }
        } else {
            self.select_workspace(step.workspace, cx);
        }
        match &step.front {
            Front::File(place) => {
                if let Some(project) = step.project {
                    self.stand_at(project, place.clone(), window, cx);
                }
            }
            Front::Session(session) => {
                self.go_to_session_in_workspace(*session, cx);
            }
            Front::Empty { area, pane } => {
                if let Some(area) = area {
                    self.focus_area(*area, cx);
                }
                if let Some((tab, pane)) = pane {
                    self.focus_pane(*tab, *pane, cx);
                    self.show_tab(*tab, cx);
                }
            }
            _ => {
                if let Some((tab, pane)) = self.showing_step(&step) {
                    self.focus_pane(tab, pane, cx);
                    self.show_tab(tab, cx);
                }
            }
        }
        self.history.suspended -= 1;
        if self.history.landing.is_none() {
            self.history.standing = self.front_now(cx);
        }
        self.focusing_front = true;
        cx.notify();
    }
    pub(crate) fn go_to_place(
        &mut self,
        project: ProjectId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.note_place(cx);
        if let Some(here) = self.history.standing.take() {
            self.history.moved(here);
        }
        self.stand_at(project, place, window, cx);
    }
    fn stand_at(
        &mut self,
        project: ProjectId,
        place: Place,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cancel_file_landings();
        self.history.suspended += 1;
        if self.active_project != Some(project) {
            self.select_project(project, cx);
        }
        self.open_file(&place.path, window, cx);
        self.history.suspended -= 1;
        self.history.landing = Some((project, place));
        self.land_history(window, cx);
    }
    /// A deferred jump may wait only for the text file still in front.
    pub(crate) fn can_land_in_file(
        &self,
        project: ProjectId,
        relative: &std::path::Path,
        cx: &gpui::App,
    ) -> bool {
        self.active_project == Some(project)
            && matches!(self.active_body(),
            Some(Body::File { path, origin: crate::Origin::Project, tab, .. })
                if path == relative && (matches!(tab.read(cx).state(),crate::file_tab::FileState::Text) || tab.read(cx).source_loading()))
    }

    pub(crate) fn land_history(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((project, place)) = self.history.landing.clone() else {
            return;
        };
        if !self.can_land_in_file(project, &place.path, cx) {
            self.history.landing = None;
            self.history.selection = None;
            self.history.standing = self.front_now(cx);
            return;
        }
        let Some(editor) = self.file_editor_at(project, &place.path, cx) else {
            return;
        };
        self.history.landing = None;
        self.reveal_markdown_source(window, cx);
        editor.update(cx, |editor, cx| {
            editor.set_cursor_position_centred(
                gpui_component::input::Position::new(place.line, place.column),
                window,
                cx,
            )
        });
        if let Some((owner, at, chars)) = self.history.selection.take()
            && owner == project
            && at == place
        {
            self.select_on_landing(project, place, chars, window, cx);
        }
        self.history.standing = self.front_now(cx);
    }

    pub(crate) fn cancel_file_landings(&mut self) {
        self.landing = None;
        self.history.landing = None;
        self.history.selection = None;
        self.history.library_landing = None;
    }

    pub(crate) fn file_landing_pending(&self) -> bool {
        self.landing.is_some()
            || self.history.landing.is_some()
            || self.history.library_landing.is_some()
    }

    pub(crate) fn retain_loading_selection(
        &mut self,
        project: ProjectId,
        place: &Place,
        chars: usize,
        cx: &gpui::App,
    ) -> bool {
        let waiting = self
            .history
            .landing
            .as_ref()
            .is_some_and(|(owner, at)| *owner == project && at == place)
            && self.can_land_in_file(project, &place.path, cx)
            && self.active_body().is_some_and(
                |body| matches!(body, Body::File{tab,..} if tab.read(cx).source_loading()),
            );
        if waiting {
            self.history.selection = Some((project, place.clone(), chars));
        }
        waiting
    }

    /// Sample where the caret is, then leave the project's places — what a
    /// jump out of the project does before it opens anything.
    pub(crate) fn leave_place(&mut self, cx: &Context<Self>) {
        self.note_place(cx);
        if let Some(project) = self.active_project {
            self.history.leave(project);
        }
    }

    /// Open a file under a dependency, and put the caret where the jump
    /// asked — **the door for a file under a dependency** (M8.9 decision
    /// 17). A dependency's path never goes through `go_to_place`, whose
    /// `stand_at` opens a *project* path with `open_file`.
    ///
    /// The place the jump was made from is left first, so Back comes back
    /// to it (`N2`).
    ///
    /// **The caret is placed here rather than through a landing.** A
    /// landing is keyed by a project path, and a dependency's `src/lib.rs`
    /// and the project's own are the same string — a landing keyed by it
    /// would put the caret in whichever tab was found first. The tab just
    /// opened is the one in front. SVG source can arrive asynchronously, so
    /// its destination retains the exact tab, origin and relative path.
    pub(crate) fn go_to_library(
        &mut self,
        origin: crate::Origin,
        relative: &std::path::Path,
        caret: Option<Caret>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_place(cx);
        self.cancel_file_landings();
        self.history.suspended += 1;
        self.open_file_from(origin, relative, window, cx);
        self.history.suspended -= 1;
        let Some(caret) = caret else {
            return;
        };
        if let Some(tab) = self.active_tab {
            self.history.library_landing = Some((tab, origin, relative.to_owned(), caret));
            self.land_library(window, cx);
        }
    }

    pub(crate) fn land_library(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((tab, origin, path, caret)) = self.history.library_landing.clone() else {
            return;
        };
        let file = match self.active_body() {
            Some(Body::File {
                path: actual,
                origin: source,
                tab: file,
                ..
            }) if self.active_tab == Some(tab) && *source == origin && *actual == path => {
                file.clone()
            }
            _ => {
                self.history.library_landing = None;
                return;
            }
        };
        let Some(editor) = file.read(cx).editor().cloned() else {
            if !file.read(cx).source_loading() {
                self.history.library_landing = None;
            }
            return;
        };
        self.history.library_landing = None;
        self.reveal_markdown_source(window, cx);
        editor.update(cx, |state, cx| {
            let (line, column) = match caret {
                Caret::Characters { line, column } => (line, column),
                Caret::Protocol(at) => {
                    crate::lsp::providers::caret_for(&state.text().to_string(), at)
                }
            };
            state.set_cursor_position_centred(
                gpui_component::input::Position::new(line, column),
                window,
                cx,
            );
        });
    }

    /// The editor of a file tab of this project, if it has one yet.
    pub(crate) fn file_editor_at(
        &self,
        project: ProjectId,
        relative: &std::path::Path,
        cx: &Context<Self>,
    ) -> Option<gpui::Entity<gpui_component::input::EditorState>> {
        self.tabs
            .iter()
            .filter(|tab| tab.project == project)
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                // A place is the project's, so the editor a landing wants
                // is the project's tab and not a dependency's of the same
                // name.
                Body::File {
                    path,
                    origin: crate::Origin::Project,
                    tab: view,
                    ..
                } if path == relative => view.read(cx).editor().cloned(),
                _ => None,
            })
    }
}

impl History {
    pub(crate) fn forget_editor(&mut self, id: gpui::EntityId) {
        self.heard.remove(&id);
    }
    pub(crate) fn relocate(
        &mut self,
        project: ProjectId,
        from: &std::path::Path,
        to: &std::path::Path,
    ) {
        let map = |place: &mut Place| {
            if let Some(path) = crate::file_operations::remap(&place.path, from, to) {
                place.path = path;
            }
        };
        if let Some(trail) = self.trails.get_mut(&project) {
            for place in &mut trail.changes {
                map(place);
            }
        }
        for step in self
            .back
            .iter_mut()
            .chain(&mut self.forward)
            .chain(&mut self.standing)
        {
            if step.project == Some(project)
                && let Front::File(place) = &mut step.front
            {
                map(place);
            }
        }
        if let Some((owner, place)) = &mut self.landing
            && *owner == project
        {
            map(place);
        }
        if let Some((owner, place, _)) = &mut self.selection
            && *owner == project
        {
            map(place);
        }
    }
}
#[cfg(test)]
mod surface_tests;

#[cfg(test)]
mod image_tests;
