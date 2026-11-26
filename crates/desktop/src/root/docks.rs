//! Where the tools are, and how one is shown or moved.
//!
//! The model is `guild_session::Docks`, kept in the project's panel
//! state. This is the window's half: which dock's panel to open for a
//! tool, the one gesture the status bar's buttons make — show, or close
//! when already showing — and the move a menu item or a drop asks for.
//! Decisions 1 and 2 of M8.3.1.

use gpui::Context;
use guild_session::{Dock, Docks, Panel, Panels, Tool};

use crate::Root;
use crate::view::panels::Side;

impl Root {
    /// Where every tool of the active project is.
    pub(crate) fn docks(&self) -> Docks {
        self.panels().docks()
    }

    /// The tool a dock shows, or would show if open.
    pub(crate) fn tool_shown(&self, dock: Dock) -> Option<Tool> {
        self.docks().shown(dock)
    }

    /// Whether a tool is on screen: its dock's panel open and showing it.
    pub(crate) fn tool_is_up(&self, tool: Tool) -> bool {
        let panels = self.panels();
        let docks = panels.docks();
        let dock = docks.dock_of(tool);
        dock_panel(&panels, dock).open && docks.shown(dock) == Some(tool)
    }

    /// The status bar's gesture: open the dock on the
    /// tool, or, pressed on the tool already up, close the dock — so that
    /// `cmd-b`, `cmd-j` and `cmd-l` keep their meaning.
    pub(crate) fn show_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        if self.tool_is_up(tool) {
            let dock = self.docks().dock_of(tool);
            self.toggle_panel(side_of(dock), cx);
            return;
        }
        self.reveal_tool(tool, cx);
        // A person's gesture activates what it opens: the
        // focus goes in, so its keys and a stretch reach it. `reveal_tool`
        // alone does not — a command and a click elsewhere open a tool
        // without taking a person's typing away from the tab. M12.3.
        self.activate_dock(self.docks().dock_of(tool));
    }

    /// Open or close a panel by its stroke, and activate it when it opens.
    pub(crate) fn toggle_panel_by_hand(&mut self, side: Side, cx: &mut Context<Self>) {
        self.toggle_panel(side, cx);
        if pick_open(&self.panels(), side) {
            self.activate_dock(side.dock());
        }
    }

    /// Put the focus into a dock at the next frame.
    pub(crate) fn activate_dock(&mut self, dock: Dock) {
        self.activating = Some((dock, ACTIVATION_FRAMES));
    }

    /// Carry an activation on, at a render — where there is a window.
    ///
    /// **The panel first, then its tool.** The tool's own element may not
    /// be drawn yet — a tree still being read, a message box made at the
    /// first render — and a handle focused before its element exists is a
    /// focus path with nothing on it, which the window answers by giving
    /// the focus back to the tab. The panel's handle is always drawn; the
    /// tool's is taken once the last frame shows it inside the panel.
    pub(crate) fn carry_activation(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some((dock, left)) = self.activating.take() else {
            return;
        };
        let panel = self.dock_focus(side_of(dock)).clone();
        if left < ACTIVATION_FRAMES && !panel.is_focused(window) {
            // A later gesture owns the keys, even if the pending tool has
            // joined the focus tree by now.
            return;
        }
        let own = self
            .tool_shown(dock)
            .and_then(|tool| self.tool_focus(tool, cx));
        if let Some(own) = own.filter(|own| panel.contains(own, window)) {
            window.focus(&own, cx);
            return;
        }
        if left == ACTIVATION_FRAMES {
            window.focus(&panel, cx);
        }
        if left > 0 {
            self.activating = Some((dock, left - 1));
            // Notifying during render is consumed by that same draw. Ask for
            // the next frame explicitly, then inspect its completed ancestry.
            let root = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = root.update(cx, |root, cx| {
                    if root.activating.is_some() {
                        cx.notify();
                    }
                });
            });
        }
    }

    /// What a tool takes the focus with, when it has something of its own.
    fn tool_focus(&self, tool: Tool, cx: &gpui::App) -> Option<gpui::FocusHandle> {
        match tool {
            Tool::Explorer => self.tree().map(|tree| tree.focus.clone()),
            Tool::Commit => self.changes().map(|changes| changes.focus.clone()),
            Tool::Git => self
                .active_project
                .and_then(|project| self.logs.get(&project))
                .map(|log| log.focus.clone()),
            Tool::Search => self.search_state().map(|search| search.focus.clone()),
            Tool::Terminal => self
                .active_panel_terminal()
                .map(|view| gpui::Focusable::focus_handle(view.read(cx), cx)),
            Tool::Output => self
                .active_run()
                .and_then(|run| run.view.as_ref())
                .map(|view| gpui::Focusable::focus_handle(view.read(cx), cx)),
            Tool::Find => self.find_state().map(|find| find.focus.clone()),
            Tool::Problems | Tool::Push => None,
        }
    }

    /// Show a tool and open its dock, never closing anything — what a
    /// click that reaches a tool from elsewhere wants.
    pub(crate) fn reveal_tool(&mut self, tool: Tool, cx: &mut Context<Self>) {
        let dock = self.docks().dock_of(tool);
        self.with_panels(|panels| panels.docks_mut().show(tool));
        if !dock_panel(&self.panels(), dock).open {
            self.toggle_panel(side_of(dock), cx);
        }
        cx.notify();
    }

    /// Move a tool to a dock and open it there. A dock left with no tool
    /// closes: an open panel that says *Nothing here* is a hole where the
    /// tool was, not a place.
    pub(crate) fn move_tool(&mut self, tool: Tool, dock: Dock, cx: &mut Context<Self>) {
        // A retired tool is moved nowhere — M8.3.6.
        if Tool::RETIRED.contains(&tool) {
            return;
        }
        let from = self.docks().dock_of(tool);
        self.with_panels(|panels| panels.docks_mut().move_tool(tool, dock));
        let panels = self.panels();
        if !dock_panel(&panels, dock).open {
            self.toggle_panel(side_of(dock), cx);
        }
        if from != dock
            && dock_panel(&self.panels(), from).open
            && self.docks().tools_in(from).is_empty()
        {
            self.toggle_panel(side_of(from), cx);
        }
        cx.notify();
    }
}

impl Root {
    /// *Move to* from a head's menu: the tool that head is showing, which
    /// is the one the person is looking at.
    ///
    /// **The menu itself is closed by `take_menu_item`, before this
    /// runs** — M12.7: the ⋮ is a trigger of the right button's engine
    /// now, and every deed it takes is closed and refocused the same way
    /// every other menu's is.
    pub(crate) fn move_shown_tool(&mut self, from: Dock, to: Dock, cx: &mut Context<Self>) {
        if let Some(tool) = self.tool_shown(from) {
            self.move_tool(tool, to, cx);
        }
    }

    /// *Hide*: what the bar beside the ⋮ does, and what pressing the
    /// tool's own lit button in the status bar does.
    pub(crate) fn hide_dock(&mut self, dock: Dock, cx: &mut Context<Self>) {
        self.toggle_panel(side_of(dock), cx);
    }
}

/// The panel a dock is drawn in.
/// How many frames an activation waits for a tool's own element.
const ACTIVATION_FRAMES: u8 = 5;

/// Whether a side's panel is open.
fn pick_open(panels: &Panels, side: Side) -> bool {
    match side {
        Side::Left => panels.left.open,
        Side::Right => panels.right.open,
        Side::Bottom => panels.bottom.open,
    }
}

pub(crate) fn side_of(dock: Dock) -> Side {
    match dock {
        Dock::Left => Side::Left,
        Dock::Right => Side::Right,
        Dock::Bottom => Side::Bottom,
    }
}

fn dock_panel(panels: &Panels, dock: Dock) -> &Panel {
    match dock {
        Dock::Left => &panels.left,
        Dock::Right => &panels.right,
        Dock::Bottom => &panels.bottom,
    }
}
