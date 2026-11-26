//! One Explorer gesture owns its source snapshot, hover clock and scrolling task.
//!
//! A gesture that leaves the window is offered to the platform as files, and
//! another application copies them (GPUI's source mask outside the
//! application is copy only). From then on it never drops inside the project.
//! Files another application drags in are a gesture of their own here, with
//! the same hover clock, and are copied into the folder they are dropped on.
use crate::{
    Root,
    file_operations::{
        Destination, Dropped, Operation, admits, destination_of, examine, outbound,
        validate_destination,
    },
};
use gpui::{Context, Window};
use guild_session::ProjectId;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct FileDrag {
    pub project: ProjectId,
    pub source: PathBuf,
    pub sources: Rc<RefCell<Vec<PathBuf>>>,
    pub token: uuid::Uuid,
}
impl FileDrag {
    pub fn new(project: ProjectId, source: PathBuf) -> Self {
        Self {
            project,
            source,
            sources: Default::default(),
            token: uuid::Uuid::new_v4(),
        }
    }
}
pub(crate) struct Drag {
    token: uuid::Uuid,
    pub project: ProjectId,
    sources: Vec<PathBuf>,
    hover: Option<(PathBuf, Instant)>,
    /// Handed to the platform: inert inside the window until it ends.
    outbound: bool,
    /// Dragged in by another application: `sources` are absolute.
    inbound: Option<Inbound>,
    _clock: gpui::Task<()>,
}
/// What the window knows of a drag from another application, read from the
/// disk once per gesture and once per folder hovered: every move and every
/// frame only compares. The dropped paths are read off the window's thread —
/// they can be thousands, or on a volume that does not answer.
pub(crate) struct Inbound {
    examined: Option<Option<Vec<Dropped>>>,
    destinations: std::collections::HashMap<PathBuf, Option<Destination>>,
    _examine: gpui::Task<()>,
}
impl Root {
    pub(crate) fn begin_tree_drag(
        &mut self,
        drag: &FileDrag,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<PathBuf> {
        if self.file_operations.busy.is_some() || self.active_project != Some(drag.project) {
            return Vec::new();
        }
        self.select_operation_target(&drag.source);
        let sources = self.operation_sources();
        *drag.sources.borrow_mut() = sources.clone();
        if sources.is_empty() {
            return sources;
        }
        if let Some(tree) = self.tree() {
            window.focus(&tree.focus, cx);
        }
        let clock = Self::drag_clock(drag.token, window, cx);
        self.file_operations.drag = Some(Drag {
            token: drag.token,
            project: drag.project,
            sources: sources.clone(),
            hover: None,
            outbound: false,
            inbound: None,
            _clock: clock,
        });
        cx.notify();
        sources
    }
    /// One gesture's clock: hover expansion, edge scrolling, and the end of a
    /// gesture whose release never reached the window.
    fn drag_clock(
        token: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> gpui::Task<()> {
        let root = cx.weak_entity();
        window.spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let alive = cx
                    .update(|window, cx| {
                        root.update(cx, |root, cx| root.tick_tree_drag(token, window, cx))
                    })
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        })
    }
    /// Whether the gesture in progress is a drag from another application.
    pub(crate) fn tree_drag_inbound(&self) -> bool {
        self.file_operations
            .drag
            .as_ref()
            .is_some_and(|active| active.inbound.is_some())
    }
    pub(crate) fn clear_tree_hover(&mut self) {
        if let Some(active) = &mut self.file_operations.drag {
            active.hover = None;
        }
    }
    pub(crate) fn clear_tree_drag(&mut self, cx: &mut Context<Self>) {
        if self.file_operations.drag.take().is_some() {
            cx.notify();
        }
    }
    pub(crate) fn tree_drop_allowed(&self, drag: &FileDrag, destination: &Path) -> bool {
        self.file_operations.drag.as_ref().is_some_and(|active| {
            active.token == drag.token
                && active.project == drag.project
                && !active.outbound
                && active.inbound.is_none()
        }) && self.active_project == Some(drag.project)
            && self.desk.project(drag.project).is_some_and(|p| {
                validate_destination(&p.path, &drag.sources.borrow(), destination).is_ok()
            })
    }
    /// The files a drag leaving the window offers other applications: the
    /// gesture's own snapshot, absolute. Marks the gesture outbound whether
    /// or not the platform then starts its session, so the outcome inside
    /// the window does not depend on it; the clock still ends it.
    pub(crate) fn hand_tree_drag_out(
        &mut self,
        drag: &FileDrag,
        cx: &mut Context<Self>,
    ) -> Option<gpui::ExternalDragPayload> {
        let root = self.desk.project(drag.project)?.path.clone();
        let active = self
            .file_operations
            .drag
            .as_mut()
            .filter(|active| active.token == drag.token && active.project == drag.project)?;
        active.outbound = true;
        active.hover = None;
        cx.notify();
        let files = outbound(&root, &active.sources);
        (!files.is_empty())
            .then(|| gpui::ExternalDragPayload::Files(gpui::FileDragPaths::new(files)))
    }
    pub(crate) fn hover_tree_drag(
        &mut self,
        drag: &FileDrag,
        target: &Path,
        cx: &mut Context<Self>,
    ) {
        if !self.tree_drop_allowed(drag, target) {
            if let Some(active) = &mut self.file_operations.drag {
                active.hover = None;
            }
            return;
        }
        if let Some(active) = &mut self.file_operations.drag
            && active.hover.as_ref().is_none_or(|(path, _)| path != target)
        {
            active.hover = Some((target.into(), cx.background_executor().now()));
            cx.notify();
        }
    }
    /// Whether files another application drags in may be copied into
    /// `destination` now, by what the gesture has already read — no disk.
    /// Never while an Explorer gesture of the window's own is live, out of the
    /// window or back in it: that one stays inert.
    pub(crate) fn inbound_allowed(&self, paths: &gpui::ExternalPaths, destination: &Path) -> bool {
        self.inbound_verdict(paths, destination) == Some(true)
    }
    /// `Some` once both the dropped paths and `destination` have been read;
    /// `Some(false)` at once while busy or while an Explorer gesture is live.
    fn inbound_verdict(&self, paths: &gpui::ExternalPaths, destination: &Path) -> Option<bool> {
        if self.file_operations.busy.is_some() {
            return Some(false);
        }
        let active = self.file_operations.drag.as_ref()?;
        let Some(inbound) = &active.inbound else {
            return Some(false);
        };
        if active.sources != paths.paths() {
            return None;
        }
        let dropped = inbound.examined.as_ref()?;
        let destination = inbound.destinations.get(destination)?;
        Some(match (dropped, destination) {
            (Some(dropped), Some(destination)) => admits(dropped, destination).is_ok(),
            _ => false,
        })
    }
    /// A drag from another application over the tree: its gesture begins on
    /// the first move — the dropped paths read once, on a worker — and each
    /// folder it hovers is read once, from the project.
    pub(crate) fn hover_inbound(
        &mut self,
        paths: &gpui::ExternalPaths,
        target: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let stale = self
            .file_operations
            .drag
            .as_ref()
            .is_some_and(|active| active.inbound.is_some() && active.sources != paths.paths());
        if stale {
            self.file_operations.drag = None;
        }
        if self.file_operations.drag.is_none()
            && self.file_operations.busy.is_none()
            && let Some(project) = self.active_project
        {
            let token = uuid::Uuid::new_v4();
            let sources = paths.paths().to_vec();
            let clock = Self::drag_clock(token, window, cx);
            let root = cx.weak_entity();
            let read = sources.clone();
            let examine = window.spawn(cx, async move |cx| {
                let examined = cx
                    .background_executor()
                    .spawn(async move { examine(&read).ok() })
                    .await;
                let _ = cx
                    .update(|_, cx| root.update(cx, |root, cx| root.examined(token, examined, cx)));
            });
            self.file_operations.drag = Some(Drag {
                token,
                project,
                sources,
                hover: None,
                outbound: false,
                inbound: Some(Inbound {
                    examined: None,
                    destinations: Default::default(),
                    _examine: examine,
                }),
                _clock: clock,
            });
        }
        let root = self
            .active_project
            .and_then(|project| self.desk.project(project))
            .map(|p| p.path.clone());
        if let (Some(root), Some(inbound)) = (
            root,
            self.file_operations
                .drag
                .as_mut()
                .and_then(|active| active.inbound.as_mut()),
        ) && !inbound.destinations.contains_key(target)
        {
            inbound
                .destinations
                .insert(target.into(), destination_of(&root, target).ok());
        }
        let allowed = self.inbound_allowed(paths, target);
        let Some(active) = self
            .file_operations
            .drag
            .as_mut()
            .filter(|active| active.inbound.is_some())
        else {
            return;
        };
        if !allowed {
            active.hover = None;
        } else if active.hover.as_ref().is_none_or(|(path, _)| path != target) {
            active.hover = Some((target.into(), cx.background_executor().now()));
            cx.notify();
        }
    }
    fn examined(
        &mut self,
        token: uuid::Uuid,
        examined: Option<Vec<Dropped>>,
        cx: &mut Context<Self>,
    ) {
        if let Some(inbound) = self
            .file_operations
            .drag
            .as_mut()
            .filter(|active| active.token == token)
            .and_then(|active| active.inbound.as_mut())
        {
            inbound.examined = Some(examined);
            cx.notify();
        }
    }
    /// Files another application dropped: refused at once when the gesture
    /// already knows they may not land here, otherwise handed to the import,
    /// whose worker has the final word before anything is copied.
    pub(crate) fn drop_inbound(
        &mut self,
        paths: &gpui::ExternalPaths,
        target: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let verdict = self.inbound_verdict(paths, &target);
        let own = self
            .file_operations
            .drag
            .as_ref()
            .is_some_and(|active| active.inbound.is_none());
        if !own {
            self.file_operations.drag = None;
        }
        if !own
            && verdict != Some(false)
            && let Some(project) = self.active_project
        {
            self.start_import(project, paths.paths().to_vec(), target, window, cx);
        }
        cx.notify();
    }
    pub(crate) fn drop_tree_drag(
        &mut self,
        drag: &FileDrag,
        target: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let allowed = self.tree_drop_allowed(drag, &target);
        let sources = self
            .file_operations
            .drag
            .take()
            .map(|active| active.sources)
            .unwrap_or_default();
        if allowed {
            self.start_transfer(drag.project, sources, target, Operation::Move, window, cx);
        }
        cx.notify();
    }
    fn tick_tree_drag(
        &mut self,
        token: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = &self.file_operations.drag else {
            return false;
        };
        if active.token != token {
            return false;
        }
        let inbound = active.inbound.is_some();
        let verdict = tick(
            inbound,
            cx.has_active_drag(),
            cfg!(test) || inbound || crate::system::primary_button_down(),
            self.active_project == Some(active.project)
                && self.desk.project(active.project).is_some()
                && self.tree().is_some(),
        );
        if verdict != Tick::Go {
            self.clear_tree_drag(cx);
            if verdict == Tick::EndAndStop {
                cx.stop_active_drag(window);
            }
            return false;
        }
        if active.outbound {
            return true;
        }
        let Some(tree) = self.tree() else {
            return false;
        };
        let scroll = tree.scroll.0.borrow().base_handle.clone();
        let bounds = scroll.bounds();
        let pointer = window.mouse_position();
        if !bounds.contains(&pointer) {
            if let Some(active) = &mut self.file_operations.drag {
                active.hover = None;
            }
            return true;
        }
        let row = gpui::px(crate::theme::space::TREE_ROW.0 * f32::from(window.rem_size()));
        let edge = if pointer.y < bounds.top() + row {
            -row
        } else if pointer.y > bounds.bottom() - row {
            row
        } else {
            gpui::px(0.)
        };
        if edge != gpui::px(0.) {
            let mut offset = scroll.offset();
            offset.y -= edge;
            scroll.set_offset(offset);
            if let Some(active) = &mut self.file_operations.drag {
                active.hover = None;
            }
            cx.notify();
            return true;
        }
        let expand = active
            .hover
            .as_ref()
            .filter(|(_, since)| {
                cx.background_executor().now().duration_since(*since) >= Duration::from_millis(600)
            })
            .map(|(p, _)| p.clone());
        if let Some(path) = expand {
            if let Some(tree) = self.tree_mut() {
                tree.tree.expand(&path);
            }
            if let Some(active) = &mut self.file_operations.drag {
                active.hover = None;
            }
            cx.notify();
        }
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tick {
    Go,
    /// The gesture's state goes; GPUI's drag is left to GPUI.
    End,
    /// The gesture's state goes, and GPUI's drag with it.
    EndAndStop,
}

/// What a gesture's clock does on a tick. An Explorer drag ends when its
/// button is up — the release may never reach the window — and takes GPUI's
/// drag with it. A drag from another application ends only when GPUI's does:
/// AppKit delivers its drop after the button is up, and GPUI's drag is the
/// one the drop is matched against.
pub(crate) fn tick(inbound: bool, gpui_drag: bool, button_down: bool, project: bool) -> Tick {
    match (inbound, gpui_drag && project, button_down) {
        (true, true, _) | (false, true, true) => Tick::Go,
        (true, false, _) => Tick::End,
        (false, _, _) => Tick::EndAndStop,
    }
}

#[cfg(test)]
mod tests {
    use super::{Tick, tick};

    /// A drag from another application is AppKit's until it drops: the
    /// button comes up in the other process before `performDragOperation`
    /// reaches the window, so the button says nothing about it, and GPUI's
    /// drag is never the clock's to stop — stopping it in that gap loses the
    /// drop, the terminal's included.
    #[test]
    fn a_drag_from_outside_ends_only_with_gpuis_and_is_never_stopped() {
        for button in [true, false] {
            assert_eq!(tick(true, true, button, true), Tick::Go, "button {button}");
            assert_eq!(tick(true, false, button, true), Tick::End);
            assert_eq!(tick(true, true, button, false), Tick::End);
        }
    }

    /// An Explorer drag whose release never reached the window is ended by
    /// the physical button, and GPUI's drag with it (`docs/traps.md`).
    #[test]
    fn an_explorer_drag_ends_with_the_button_and_stops_gpuis() {
        assert_eq!(tick(false, true, true, true), Tick::Go);
        assert_eq!(tick(false, true, false, true), Tick::EndAndStop);
        assert_eq!(tick(false, false, true, true), Tick::EndAndStop);
        assert_eq!(tick(false, true, true, false), Tick::EndAndStop);
    }
}
