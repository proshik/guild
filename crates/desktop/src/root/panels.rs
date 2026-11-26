//! Opening, closing and sizing the panels.
//!
//! **The state is the window's since M12.2**, in its settings: it was each
//! project's until then — kept in `<project>/.guild/` on the reasoning that
//! a tree is given the width its repository's paths need — and switching
//! projects rebuilt the window around a person who had only asked to look
//! at other code. What a tool *shows* is still the project's.

use gpui::{Context, Window};
use guild_session::{Panel, Panels};

use crate::Root;
use crate::view::panels::{Column, Side};
// The actions this file's binder handles; see `root::render`.
use crate::actions::*;
use gpui::prelude::*;

/// How much a stroke moves a border.
///
/// Bigger than a nudge and smaller than a jump: a person reaching for the
/// keyboard rather than the border wants to arrive in a few presses, not
/// twenty and not one.
const RESIZE_STEP: f32 = 24.0;
impl Root {
    fn resize_held(&self) -> bool {
        self.dragging.is_some()
            || self.column_drag.is_some()
            || self.column_edge_drag
            || self.area_drag.is_some()
            || self.pane_drag.is_some()
            || self.find_state().is_some_and(|state| state.drag.is_some())
    }

    pub(crate) fn resize_motion(
        &mut self,
        event: &gpui::MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.resize_held() {
            return false;
        }
        if !self.drag_find_split(event, cx) {
            self.follow_pointer(event, window, cx);
        }
        true
    }

    pub(crate) fn resize_release(
        &mut self,
        event: &gpui::MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.button != gpui::MouseButton::Left || !self.resize_held() {
            return false;
        }
        // MouseUp can be the only sample at the final pointer position.
        self.resize_motion(
            &gpui::MouseMoveEvent {
                position: event.position,
                pressed_button: Some(gpui::MouseButton::Left),
                modifiers: event.modifiers,
            },
            window,
            cx,
        );
        self.stop_dragging(cx);
        self.end_find_split(cx);
        true
    }

    /// The window's panels — M12.2.
    ///
    /// Before a layout has been adopted — the first frame of a window whose
    /// settings predate M12.2 — the defaults, which is what a window with no
    /// project drew anyway.
    pub(crate) fn panels(&self) -> Panels {
        self.settings.panels.clone().unwrap_or_default()
    }

    /// Change the window's panels, and remember it.
    ///
    /// Written on every change rather than on exit, for the reason the
    /// workspace already is — a window that saves only on a clean shutdown
    /// loses what it knew at the first crash — and **queued** rather than
    /// synced at once: a border dragged is a change per frame, and a synced
    /// write is twenty milliseconds. The queue is flushed before a quit.
    pub(crate) fn with_panels(&mut self, change: impl FnOnce(&mut Panels)) {
        let mut panels = self.panels();
        change(&mut panels);
        self.settings.panels = Some(panels);
        self.store_settings();
    }

    /// Every resize starts from the displayed dock after oversized restoration.
    fn resize_panel_by(&mut self, side: Side, by: f32) {
        let bottom_height = (side == Side::Bottom).then(|| f32::from(self.bottom_dock_height()));
        let viewport = self.viewport;
        let taken = self.taken_beside(side);
        self.with_panels(|panels| {
            let panel = pick(panels, side);
            if let Some(height) = bottom_height {
                panel.size = height;
            }
            panel.resize(by, bounds_of(side, viewport, taken));
        });
    }

    /// Show or hide one panel.
    pub(crate) fn toggle_panel(&mut self, side: Side, cx: &mut Context<Self>) {
        self.with_panels(|panels| {
            let panel = pick(panels, side);
            panel.open = !panel.open;
        });
        // The terminal is not told anything here, and that is deliberate:
        // its bounds change, `fit` recomputes the grid from them, and
        // `Emulator::resize` reaches the pty. The road was built in M5.2.
        self.last_panel = side;
        cx.notify();
    }

    /// Make a panel bigger or smaller.
    ///
    /// **Which panel:** the one with the focus, when one has it — the tree
    /// takes focus when it is clicked — and the one last opened otherwise.
    /// The panels were empty until M8.1, and "the focused panel" had
    /// nothing to appeal to; the note here said the rule would change when
    /// they had content, and it has.
    pub(crate) fn resize_panel(&mut self, by: f32, window: &Window, cx: &mut Context<Self>) {
        let side = if self.tree_has_focus(window) {
            Side::Left
        } else {
            self.last_panel
        };
        self.resize_panel_by(side, by * side.towards());
        cx.notify();
    }

    /// Stretch the panel with the focus the arrow's way — Stretch to
    /// Left, Right, Top and Bottom.
    ///
    /// Reached only from the panel's own element, so it is the panel the
    /// focus is in. An arrow along an edge the panel does not have goes on
    /// to whatever else would take it, as a disabled action does.
    /// The step is five of the interface's characters across and five of
    /// its lines down, measured as a label holding *W* measures.
    pub(crate) fn stretch_panel(
        &mut self,
        side: Side,
        toward: guild_session::Stretch,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sign) = side.dock().stretched(toward) else {
            cx.propagate();
            return;
        };
        let step = self.stretch_step(toward, window);
        self.resize_panel_by(side, sign * step);
        self.last_panel = side;
        cx.notify();
    }

    /// Stretch the sidebar — M12.3. It stands at the window's left edge,
    /// so it moves as the left panel does: right widens it, left narrows
    /// it, and up and down go on to whatever else would take them.
    pub(crate) fn stretch_sidebar(
        &mut self,
        toward: guild_session::Stretch,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sign) = guild_session::Dock::Left.stretched(toward) else {
            cx.propagate();
            return;
        };
        let step = self.stretch_step(toward, window);
        // Its own bounds, and never past what the panels beside the work
        // leave — 2026-09-24, the panels' rule.
        let panels = self.panels();
        let open = |panel: &Panel| if panel.open { panel.size } else { 0.0 };
        let room = Panel::room(
            f32::from(self.viewport.width),
            open(&panels.left) + open(&panels.right),
            crate::column::narrowest(self.settings.ui_font_size),
        );
        self.settings.column_width = crate::column::width(
            self.settings.column_width + sign * step,
            self.settings.ui_font_size,
        )
        .min(room);
        self.save_settings();
        cx.notify();
    }

    /// One stretch's move: five of the interface's characters across or
    /// five of its lines down, measured as a label holding *W* measures.
    fn stretch_step(&self, toward: guild_session::Stretch, window: &Window) -> f32 {
        let family = self
            .settings
            .ui_font
            .clone()
            .unwrap_or_else(|| crate::fonts::DEFAULT.ui.to_owned());
        let size = crate::theme::size::UI.to_pixels(window.rem_size());
        let text = window.text_system();
        let font = text.resolve_font(&gpui::font(family));
        let character = text
            .advance(font, size, 'W')
            .map_or(f32::from(size), |advance| f32::from(advance.width));
        let line = f32::from(text.ascent(font, size) + text.descent(font, size).abs());
        toward.step(character, line)
    }

    /// The focus handle a dock's panel carries.
    pub(crate) fn dock_focus(&self, side: Side) -> &gpui::FocusHandle {
        match side {
            Side::Left => &self.dock_focus[0],
            Side::Right => &self.dock_focus[1],
            Side::Bottom => &self.dock_focus[2],
        }
    }

    /// Put one border back where the design system put it.
    ///
    /// A border dragged to a width that turns out wrong has no way back:
    /// the size that was there is not written down anywhere a person can
    /// read, and creeping back to it by hand never lands. Double-clicking
    /// a divider to reset it is what every editor with panels does.
    pub(crate) fn reset_panel(&mut self, side: Side, cx: &mut Context<Self>) {
        let size = pick(&mut Panels::default(), side).size;
        self.with_panels(|panels| pick(panels, side).size = size);
        self.last_panel = side;
        cx.notify();
    }

    /// Follow the pointer while a border is being dragged.
    ///
    /// The move is measured against where the pointer was last seen rather
    /// than accumulated: a dropped frame would otherwise leave the border
    /// somewhere the hand is not, and it never catches up.
    pub(crate) fn drag_panel(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let (Some(side), Some(from)) = (self.dragging, self.drag_from) else {
            return;
        };

        let moved = match side {
            Side::Bottom => f32::from(at.y - from.y),
            _ => f32::from(at.x - from.x),
        };
        if moved == 0.0 {
            return;
        }

        self.drag_from = Some(at);
        self.resize_panel_by(side, moved * side.towards());
        cx.notify();
    }

    /// Take hold of a border inside a tool.
    pub(crate) fn grab_column(
        &mut self,
        column: Column,
        at: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.column_drag = Some(column);
        self.drag_from = Some(at);
        cx.notify();
    }

    /// Follow the pointer while one of them is being dragged, measured
    /// against where the pointer was last seen — `drag_panel`'s reason.
    pub(crate) fn drag_column(&mut self, at: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        let (Some(column), Some(from)) = (self.column_drag, self.drag_from) else {
            return;
        };
        // A height is dragged down the screen and a width across it.
        let moved = if column == Column::LogMessage {
            f32::from(at.y - from.y)
        } else {
            f32::from(at.x - from.x)
        } * column.towards();
        if moved == 0.0 {
            return;
        }
        self.drag_from = Some(at);
        self.with_panels(|panels| match column {
            Column::LogBranches => panels.columns.resize_branches(moved),
            Column::LogDetails => panels.columns.resize_details(moved),
            Column::LogMessage => panels.columns.resize_message(moved),
        });
        cx.notify();
    }

    /// Put one of them back where the design system put it.
    pub(crate) fn reset_column(&mut self, column: Column, cx: &mut Context<Self>) {
        let fresh = guild_session::Columns::default();
        self.with_panels(|panels| match column {
            Column::LogBranches => panels.columns.branches = fresh.branches,
            Column::LogDetails => panels.columns.details = fresh.details,
            Column::LogMessage => panels.columns.message = fresh.message,
        });
        cx.notify();
    }

    /// The hand let go of the border.
    pub(crate) fn stop_dragging(&mut self, cx: &mut Context<Self>) {
        let panel = self.dragging.take().is_some();
        let column = self.column_drag.take().is_some();
        if panel || column {
            self.drag_from = None;
            cx.notify();
        }
        // The column's width is the person's, and it lives in the
        // settings — written once, here, rather than on every one of the
        // sixty moves a second a drag makes: a synced write is twenty
        // milliseconds, which is the lesson `Store::save_soon` exists for.
        if std::mem::take(&mut self.column_edge_drag) {
            self.save_settings();
            cx.notify();
        }
        // An area's border is the seat's, written with its tabs — once, at
        // the release, for the same reason; a pane's is its tab's. M12.8.
        let area = self.area_drag.take().is_some();
        let pane = self.pane_drag.take().is_some();
        if area || pane {
            self.drag_from = None;
            self.save();
            cx.notify();
        }
    }
}

/// The panel a side names.
fn pick(panels: &mut Panels, side: Side) -> &mut Panel {
    match side {
        Side::Left => &mut panels.left,
        Side::Right => &mut panels.right,
        Side::Bottom => &mut panels.bottom,
    }
}

/// What that panel may be resized between.
///
/// The bottom one is a height and has its own pair: a height clamped to a
/// side panel's ceiling would leave no terminal above it.
fn bounds_of(side: Side, viewport: gpui::Size<gpui::Pixels>, taken: f32) -> (f32, f32) {
    match side {
        Side::Bottom => (
            Panel::MIN_HEIGHT,
            Panel::room(f32::from(viewport.height), taken, Panel::MIN_HEIGHT),
        ),
        _ => (
            Panel::MIN_WIDTH,
            Panel::room(f32::from(viewport.width), taken, Panel::MIN_WIDTH),
        ),
    }
}

impl Root {
    /// What stands beside a panel on its own axis and is not the work:
    /// the other side panel when it is open, and the sidebar when it is
    /// pinned. The bottom panel shares its height with nothing of the kind.
    fn taken_beside(&self, side: Side) -> f32 {
        let panels = self.panels();
        let column = if self.settings.column_folded {
            0.0
        } else {
            self.settings.column_width
        };
        let open = |panel: &Panel| if panel.open { panel.size } else { 0.0 };
        match side {
            Side::Left => open(&panels.right) + column,
            Side::Right => open(&panels.left) + column,
            Side::Bottom => self.bottom_frame_height(),
        }
    }
}

impl Root {
    /// The handlers for the docks, the tools in them and their sizes.
    ///
    /// One of seven binders the window's `render` pipes its root element
    /// through. They were one chain of sixty-nine in `root::render`, and
    /// M8.8 cut six branches that each had to add to it — one file, one
    /// conflict site, six times. A handler now goes in the file that owns
    /// the subject, beside the method it calls.
    pub(crate) fn bind_panels(&self, root: gpui::Div, cx: &mut Context<Root>) -> gpui::Div {
        root.on_action(cx.listener(|root, _: &ShowExplorerPanel, _window, cx| {
            root.show_tool(guild_session::Tool::Explorer, cx)
        }))
        .on_action(cx.listener(|root, _: &ShowCommitPanel, _window, cx| {
            root.show_tool(guild_session::Tool::Commit, cx)
        }))
        .on_action(cx.listener(|root, _: &ShowLogPanel, _window, cx| {
            root.show_tool(guild_session::Tool::Git, cx)
        }))
        .on_action(cx.listener(|root, _: &ShowTerminalPanel, _window, cx| {
            root.show_tool(guild_session::Tool::Terminal, cx)
        }))
        .on_action(cx.listener(|root, _: &ShowOutputPanel, _window, cx| {
            root.show_tool(guild_session::Tool::Output, cx)
        }))
        .on_action(cx.listener(|root, _: &ShowProblems, _window, cx| {
            root.show_tool(guild_session::Tool::Problems, cx)
        }))
        .on_action(
            cx.listener(|root, _: &ShowFindPanel, window, cx| root.show_find_panel(window, cx)),
        )
        // The Search tool — `FindInFiles` until M8.9, which gave that
        // name to the popup (`root::find_in_files`).
        .on_action(
            cx.listener(|root, _: &ShowSearchPanel, window, cx| root.find_in_files(window, cx)),
        )
        .on_action(cx.listener(|root, _: &MoveExplorerToLeft, _window, cx| {
            root.move_tool(guild_session::Tool::Explorer, guild_session::Dock::Left, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveExplorerToRight, _window, cx| {
            root.move_tool(
                guild_session::Tool::Explorer,
                guild_session::Dock::Right,
                cx,
            )
        }))
        .on_action(cx.listener(|root, _: &MoveExplorerToBottom, _window, cx| {
            root.move_tool(
                guild_session::Tool::Explorer,
                guild_session::Dock::Bottom,
                cx,
            )
        }))
        .on_action(cx.listener(|root, _: &MoveCommitToLeft, _window, cx| {
            root.move_tool(guild_session::Tool::Commit, guild_session::Dock::Left, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveCommitToRight, _window, cx| {
            root.move_tool(guild_session::Tool::Commit, guild_session::Dock::Right, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveCommitToBottom, _window, cx| {
            root.move_tool(guild_session::Tool::Commit, guild_session::Dock::Bottom, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveLogToLeft, _window, cx| {
            root.move_tool(guild_session::Tool::Git, guild_session::Dock::Left, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveLogToRight, _window, cx| {
            root.move_tool(guild_session::Tool::Git, guild_session::Dock::Right, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveLogToBottom, _window, cx| {
            root.move_tool(guild_session::Tool::Git, guild_session::Dock::Bottom, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveOutputToLeft, _window, cx| {
            root.move_tool(guild_session::Tool::Output, guild_session::Dock::Left, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveOutputToRight, _window, cx| {
            root.move_tool(guild_session::Tool::Output, guild_session::Dock::Right, cx)
        }))
        .on_action(cx.listener(|root, _: &MoveOutputToBottom, _window, cx| {
            root.move_tool(guild_session::Tool::Output, guild_session::Dock::Bottom, cx)
        }))
        // The column folds beside the panels: it is the other thing
        // down that edge, and it is the window's own choice rather
        // than a project's. M12.
        .on_action(
            cx.listener(|root, _: &crate::actions::ToggleSidebar, _window, cx| {
                root.toggle_column(cx);
            }),
        )
        .on_action(cx.listener(|root, _: &ToggleLeftPanel, _window, cx| {
            root.toggle_panel_by_hand(crate::view::panels::Side::Left, cx)
        }))
        .on_action(cx.listener(|root, _: &ToggleRightPanel, _window, cx| {
            root.toggle_panel_by_hand(crate::view::panels::Side::Right, cx)
        }))
        .on_action(cx.listener(|root, _: &ToggleBottomPanel, _window, cx| {
            root.toggle_panel_by_hand(crate::view::panels::Side::Bottom, cx)
        }))
        .on_action(cx.listener(|root, _: &WidenPanel, window, cx| {
            root.resize_panel(RESIZE_STEP, window, cx)
        }))
        .on_action(cx.listener(|root, _: &NarrowPanel, window, cx| {
            root.resize_panel(-RESIZE_STEP, window, cx)
        }))
    }
}
