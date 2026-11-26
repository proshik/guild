//! A held project or project group owns one cancellable clock; leaving an
//! edge only pauses it.
use crate::column::{Dragged, DropHint, DropRow, Listed, Zone};
use crate::{Root, view::drag::EdgeMotion};
use gpui::{Bounds, Context, IntoElement, Pixels, ScrollHandle, Styled, Window, point, px};
use guild_session::WorkspaceId;
use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Surface {
    Column,
    Footer,
    List(WorkspaceId),
    /// The *Projects* heading above the list: the place before its first
    /// item for a drag, reached a little above the first row.
    Heading(WorkspaceId),
}

/// Each drop row's painted bounds, by workspace and row.
type RowBounds = Rc<RefCell<HashMap<(WorkspaceId, Listed), Bounds<Pixels>>>>;

#[derive(Default)]
pub(crate) struct State {
    pub active: Option<Gesture>,
    scrolls: RefCell<HashMap<WorkspaceId, ScrollHandle>>,
    bounds: Rc<RefCell<HashMap<Surface, Bounds<Pixels>>>>,
    /// Each drop row's painted bounds, clipped to what can be seen: a
    /// release reads its zone from them, as a move does.
    rows: RowBounds,
    /// The column as it stood when the last drag began, and what was
    /// dragged. Kept past the gesture: the root's capture-phase release
    /// ends the gesture before a row's drop reads it.
    order: Option<DragOrder>,
}

/// The column's order when a drag began: its top-level items, each group's
/// repositories, and what was dragged — a group, or a repository by its key
/// and, when it is in no group, as a top-level item.
pub(crate) struct DragOrder {
    tops: Vec<crate::column::Top>,
    groups: Vec<(
        guild_session::project_group::ProjectGroupId,
        Vec<std::path::PathBuf>,
    )>,
    dragged: Option<crate::column::Top>,
    key: Option<std::path::PathBuf>,
}

pub(crate) struct Gesture {
    token: uuid::Uuid,
    workspace: WorkspaceId,
    motion: EdgeMotion,
    _clock: gpui::Task<()>,
}

impl State {
    pub fn retain(&self, workspaces: &[guild_session::Workspace]) {
        self.scrolls
            .borrow_mut()
            .retain(|id, _| workspaces.iter().any(|w| w.id == *id));
        self.bounds.borrow_mut().retain(|surface, _| match surface {
            Surface::List(id) | Surface::Heading(id) => workspaces.iter().any(|w| w.id == *id),
            _ => true,
        });
    }

    pub fn scroll(&self, workspace: WorkspaceId) -> ScrollHandle {
        self.scrolls
            .borrow_mut()
            .entry(workspace)
            .or_default()
            .clone()
    }

    /// A row's measure, for the zone a release lands in.
    pub fn measure_row(&self, workspace: WorkspaceId, row: Listed) -> impl IntoElement {
        let rows = self.rows.clone();
        gpui::canvas(
            |_, _, _| {},
            move |rect, _, _, _| {
                rows.borrow_mut().insert((workspace, row.clone()), rect);
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }

    /// The column list's visible bounds, when it has been painted.
    fn list_bounds(&self, workspace: WorkspaceId) -> Option<Bounds<Pixels>> {
        self.bounds.borrow().get(&Surface::List(workspace)).copied()
    }

    /// Canvas paint sees the native content mask, unlike drag listeners' raw bounds.
    pub fn measure(&self, surface: Surface) -> impl IntoElement {
        let bounds = self.bounds.clone();
        gpui::canvas(
            |_, _, _| {},
            move |rect, _, window, _| {
                let viewport = Bounds::new(point(px(0.), px(0.)), window.viewport_size());
                bounds.borrow_mut().insert(
                    surface,
                    rect.intersect(&window.content_mask().bounds)
                        .intersect(&viewport),
                );
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full()
    }
}

impl Root {
    /// A project's or a project group's header began to drag: the edge
    /// clock, and the `column-drag` context Escape cancels it in.
    pub(crate) fn begin_project_drag(
        &mut self,
        workspace: WorkspaceId,
        token: uuid::Uuid,
        dragged: Option<crate::column::Top>,
        key: Option<std::path::PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.active_workspace != workspace
            || (self.settings.column_folded && !self.column_peeking)
        {
            return;
        }
        let root = cx.weak_entity();
        let clock = window.spawn(cx, async move |cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(80))
                    .await;
                let alive = cx
                    .update(|window, cx| {
                        root.update(cx, |root, cx| {
                            if root
                                .project_drag
                                .active
                                .as_ref()
                                .is_none_or(|g| g.token != token)
                            {
                                return false;
                            }
                            root.wake_project_drag(window, cx);
                            root.project_drag.active.is_some()
                        })
                    })
                    .ok()
                    .and_then(Result::ok)
                    .unwrap_or(false);
                if !alive {
                    break;
                }
            }
        });
        // The order and what is dragged: a group, a repository in no group,
        // or `None` for a project inside a group, which a top-level line
        // takes out — with its key, for the lines inside a group.
        let sections = self.column_sections_for(workspace);
        self.project_drag.order = Some(DragOrder {
            tops: crate::column::tops(&sections),
            groups: sections
                .iter()
                .filter_map(|section| {
                    let head = section.head.as_ref()?;
                    Some((
                        head.id,
                        section.repositories.iter().map(|r| r.key.clone()).collect(),
                    ))
                })
                .collect(),
            dragged,
            key,
        });
        self.project_drag.active = Some(Gesture {
            token,
            workspace,
            motion: EdgeMotion::new(cx.background_executor().now()),
            _clock: clock,
        });
        self.schedule_project_frame(window, cx);
        cx.notify();
    }

    pub(crate) fn clear_project_drag(&mut self, cx: &mut Context<Self>) {
        let hinted = self.column_drop_hint.take().is_some();
        if self.project_drag.active.take().is_some() || hinted {
            cx.notify();
        }
    }

    /// A drag moved over a drop row: the zone under the pointer becomes the
    /// column's hint — a line for a place between items, the lit row for
    /// its middle. `noop` names the zones where a drop would change nothing,
    /// which draw nothing either.
    ///
    /// **Leaving a row keeps its hint**: the line is drawn in the gap
    /// between rows, so a pointer crossing that gap would otherwise put it
    /// out and back. The next row the pointer reaches replaces it, and the
    /// list takes it away when the pointer leaves the list.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn hover_column_drop(
        &mut self,
        workspace: WorkspaceId,
        row: Listed,
        beside: crate::column::Top,
        kind: DropRow,
        dragged: Dragged,
        noop: impl Fn(Zone) -> bool,
        bounds: Bounds<Pixels>,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let visible = self.project_drag.list_bounds(workspace).unwrap_or(bounds);
        if !crate::view::tabs::answers_drag(bounds, visible, at) {
            return;
        }
        let hint = crate::column::drop_zone(
            kind,
            dragged,
            f32::from(bounds.top()),
            f32::from(bounds.bottom()),
            f32::from(at.y),
        )
        .filter(|zone| !noop(*zone) && !self.column_drop_stays(&row, &beside, kind, *zone))
        .map(|zone| DropHint {
            workspace,
            row,
            zone,
            beside,
            kind,
        });
        self.set_column_drop_hint(hint, cx);
    }

    /// Whether a line is where the dragged item already stands — just
    /// before the item after it, just after the one before it, at the top
    /// level or among its group's projects — so letting go there would
    /// change nothing, and none is drawn.
    pub(crate) fn column_drop_stays(
        &self,
        row: &Listed,
        beside: &crate::column::Top,
        kind: DropRow,
        zone: Zone,
    ) -> bool {
        let Some(order) = &self.project_drag.order else {
            return false;
        };
        let adjacent = |from: usize, to: usize| match zone {
            Zone::Before => to == from || to == from + 1,
            Zone::After => to == from || to + 1 == from,
            Zone::Middle => false,
        };
        // A line inside a group: beside one of its projects.
        if crate::column::inside_group(kind, zone) {
            let (crate::column::Top::Group(group), Listed::Repository(member), Some(key)) =
                (beside, row, &order.key)
            else {
                return false;
            };
            let Some((_, members)) = order.groups.iter().find(|(id, _)| id == group) else {
                return false;
            };
            let at = |key: &std::path::PathBuf| members.iter().position(|m| m == key);
            return match (at(key), at(member)) {
                (Some(from), Some(to)) => adjacent(from, to),
                _ => false,
            };
        }
        let Some(dragged) = &order.dragged else {
            return false;
        };
        let at = |top: &crate::column::Top| order.tops.iter().position(|t| t == top);
        match (at(dragged), at(beside)) {
            (Some(from), Some(to)) => adjacent(from, to),
            _ => false,
        }
    }

    /// A drag moved over the heading above the list: the place before the
    /// list's first item, drawn as that item's line.
    pub(crate) fn hover_heading_drop(
        &mut self,
        workspace: WorkspaceId,
        bounds: Bounds<Pixels>,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if !bounds.contains(&at) {
            return;
        }
        let first = self
            .project_drag
            .active
            .as_ref()
            .filter(|gesture| gesture.workspace == workspace)
            .and(self.project_drag.order.as_ref())
            .and_then(|order| order.tops.first().cloned());
        let hint = first
            .map(|first| {
                let (row, kind) = match &first {
                    crate::column::Top::Group(id) => {
                        (Listed::Group(*id), DropRow::Head { collapsed: false })
                    }
                    crate::column::Top::Repository(key) => {
                        (Listed::Repository(key.clone()), DropRow::Repository)
                    }
                };
                (first, row, kind)
            })
            .filter(|(first, row, kind)| !self.column_drop_stays(row, first, *kind, Zone::Before))
            .map(|(first, row, kind)| DropHint {
                workspace,
                row,
                zone: Zone::Before,
                beside: first,
                kind,
            });
        self.set_column_drop_hint(hint, cx);
    }

    /// Put the column's drop hint, notifying only when it changed.
    pub(crate) fn set_column_drop_hint(&mut self, hint: Option<DropHint>, cx: &mut Context<Self>) {
        if self.column_drop_hint != hint {
            self.column_drop_hint = hint;
            cx.notify();
        }
    }

    /// A drag moved anywhere over the column's list: off the list, its hint
    /// goes.
    pub(crate) fn leave_column_drop(
        &mut self,
        workspace: WorkspaceId,
        at: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.project_drag.bounds.borrow();
        let inside = [Surface::List(workspace), Surface::Heading(workspace)]
            .iter()
            .any(|surface| bounds.get(surface).is_some_and(|b| b.contains(&at)));
        drop(bounds);
        if !inside
            && self
                .column_drop_hint
                .as_ref()
                .is_some_and(|hint| hint.workspace == workspace)
        {
            self.set_column_drop_hint(None, cx);
        }
    }

    /// Where a release over a drop row lands: from the pointer and the row's
    /// painted bounds, so a quick release lands where it was let go of; the
    /// last move's hint when the row was not measured.
    pub(crate) fn column_drop_zone(
        &self,
        workspace: WorkspaceId,
        row: &Listed,
        kind: DropRow,
        dragged: Dragged,
        window: &Window,
    ) -> Option<Zone> {
        let at = window.mouse_position();
        let measured = self
            .project_drag
            .rows
            .borrow()
            .get(&(workspace, row.clone()))
            .copied()
            .filter(|bounds| bounds.contains(&at));
        match measured {
            Some(bounds) => crate::column::drop_zone(
                kind,
                dragged,
                f32::from(bounds.top()),
                f32::from(bounds.bottom()),
                f32::from(at.y),
            ),
            None => self
                .column_drop_hint
                .as_ref()
                .filter(|hint| hint.workspace == workspace && &hint.row == row)
                .map(|hint| hint.zone),
        }
    }

    fn project_drag_valid(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(active) = &mut self.project_drag.active else {
            return false;
        };
        if !cx.has_active_drag()
            || (!cfg!(test) && !crate::system::primary_button_down())
            || active.workspace != self.active_workspace
            || self.desk.workspace(active.workspace).is_none()
            || (self.settings.column_folded && !self.column_peeking)
            || self.workspace_swipe_offset != 0.
        {
            self.clear_project_drag(cx);
            cx.stop_active_drag(window);
            return false;
        }
        true
    }

    fn project_edge(&self, window: &Window) -> Option<(ScrollHandle, Pixels)> {
        let workspace = self.project_drag.active.as_ref()?.workspace;
        let bounds = self.project_drag.bounds.borrow();
        let pointer = window.mouse_position();
        let inside = [Surface::Column, Surface::Footer]
            .iter()
            .any(|surface| bounds.get(surface).is_some_and(|b| b.contains(&pointer)));
        let list = bounds.get(&Surface::List(workspace))?;
        if !inside || list.size.height <= px(0.) {
            return None;
        }
        let step = px(crate::theme::space::TREE_ROW.0 * f32::from(window.rem_size()));
        let row = if pointer.y < list.top() + step {
            step
        } else if pointer.y > list.bottom() - step {
            -step
        } else {
            return None;
        };
        let scroll = self.project_drag.scroll(workspace);
        let y = scroll.offset().y;
        if (row > px(0.) && y >= px(0.)) || (row < px(0.) && y <= -scroll.max_offset().y) {
            return None;
        }
        Some((scroll, row))
    }

    pub(crate) fn wake_project_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.project_drag_valid(window, cx) {
            return;
        }
        if self.project_drag.active.as_ref().unwrap().motion.pending {
            return;
        }
        self.project_drag
            .active
            .as_mut()
            .unwrap()
            .motion
            .pause(cx.background_executor().now());
        if self.project_edge(window).is_some() {
            self.schedule_project_frame(window, cx);
        }
    }

    fn schedule_project_frame(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(g) = &mut self.project_drag.active else {
            return;
        };
        if g.motion.pending {
            return;
        }
        g.motion.pending = true;
        let token = g.token;
        let weak = cx.weak_entity();
        window.on_next_frame(move |window, cx| {
            let _ = weak.update(cx, |root, cx| {
                if root
                    .project_drag
                    .active
                    .as_ref()
                    .is_none_or(|g| g.token != token)
                {
                    return;
                }
                root.project_drag.active.as_mut().unwrap().motion.pending = false;
                if !root.project_drag_valid(window, cx) {
                    return;
                }
                let now = cx.background_executor().now();
                let Some((scroll, row)) = root.project_edge(window) else {
                    root.project_drag.active.as_mut().unwrap().motion.pause(now);
                    return;
                };
                let distance = root
                    .project_drag
                    .active
                    .as_mut()
                    .unwrap()
                    .motion
                    .distance(now, row);
                let mut offset = scroll.offset();
                offset.y = (offset.y + distance).clamp(-scroll.max_offset().y, px(0.));
                if offset != scroll.offset() {
                    scroll.set_offset(offset);
                    // Root invalidates the cached workspace Region too.
                    cx.notify();
                }
                if root.project_edge(window).is_some() {
                    root.schedule_project_frame(window, cx);
                }
            });
        });
    }
}
