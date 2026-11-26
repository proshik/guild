//! What the pointer does.
//!
//! `claude` reads the mouse itself, so the wheel is forwarded to it rather
//! than acted on. In the alternate screen without mouse reporting it
//! becomes cursor keys, as xterm does; only ordinary output moves the
//! history.
//!
//! Moved out of `input` verbatim in M5.12's seams, so that selection by
//! clicks (task 10) and links under the pointer (task 12) have a file that
//! is theirs and not the keyboard's.

use super::Side;
use super::TerminalTab;
use crate::actions::SelectAll;
use crate::terminal::{keys, mouse, scroll, select};
use gpui::{
    Context, InteractiveElement as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    Pixels, Point as GpuiPoint, ScrollWheelEvent, px,
};

impl TerminalTab {
    /// Scrolling through history with the wheel or the trackpad.
    ///
    /// The remainder is carried between events. A wheel reports whole
    /// notches, but a trackpad reports two or three pixels at a time:
    /// rounding each event on its own gave zero every time, and the
    /// terminal did not scroll from a trackpad at all.
    pub(super) fn on_scroll(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        self.dismiss_restore(cx);
        let cell_height = self.cell.map_or(px(16.0), |cell| cell.height);
        let delta = event.delta.pixel_delta(cell_height).y;

        let rows = self.scroll.rows(f32::from(delta), f32::from(cell_height));
        let mode = self.emulator.mode();

        match scroll::action(mode, rows) {
            scroll::Action::Nothing => {}
            scroll::Action::Grid(rows) => {
                self.emulator.scroll(rows);
                self.scrolled_at = Some(std::time::Instant::now());
                cx.notify();
            }
            // A full-screen program keeps its own view and has no history
            // for us to move over: the cursor keys are how it is asked to
            // scroll. Without this a trackpad did nothing at all in
            // `claude` — the rows were counted correctly and applied to a
            // grid that cannot move.
            scroll::Action::Keys { up, count } => {
                let bytes = keys::cursor_key(up, mode);
                for _ in 0..count {
                    if !self.send_visible(bytes, cx) {
                        break;
                    }
                    self.raw_input();
                }
            }
            // The program asked to read the mouse — `claude` asks at
            // startup — so the wheel is forwarded and it decides what
            // scrolling means in its own interface. Acting on it here
            // instead is what left the trackpad silent.
            scroll::Action::Report { up, count } => {
                let (line, column) = self
                    .cell_at(event.position)
                    .map_or((0, 0), |(line, column, _)| (line, column));
                for _ in 0..count {
                    if !self.send_visible(&mouse::wheel(up, column, line, mode), cx) {
                        break;
                    }
                    self.raw_input();
                }
            }
        }
    }

    /// A press of the left button: a selection begun by one, two or three
    /// clicks, or the one there is stretched by a shift-click — M5.12,
    /// task 10.
    ///
    /// The selection is set through the grid's lock **here, in the
    /// handler**, which is not the paint's lock and is let go of before the
    /// frame takes it.
    pub(super) fn on_mouse_down(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        // A `cmd`-click on a link is the link's, and begins no selection —
        // M5.12, task 12.
        if self.press_link(event.position, event.modifiers.platform, cx) {
            // Its release must not copy a selection it did not make, even
            // if an earlier release went missing and left one being made.
            self.grab = select::Grab::None;
            return;
        }
        // **A program that reads the mouse gets the press** — 2026-09-24.
        // `claude` selects, autoscrolls and copies by itself, and has
        // buttons to click; our selection on its alternate screen could
        // reach no further than the screen. Option keeps it ours.
        let mode = self.emulator.mode();
        if let select::Grab::Report(button) =
            select::grab_for(mode, event.modifiers.alt, mouse::Button::Left)
        {
            let Some((line, column, _)) = self.cell_clamped(event.position) else {
                return;
            };
            self.emulator.clear_selection();
            if !self.send_visible(
                &mouse::press(button, column, line, mods_of(event.modifiers), mode),
                cx,
            ) {
                return;
            }
            self.raw_input();
            self.grab = select::Grab::Report(button);
            self.reported_at = Some((line, column));
            cx.notify();
            return;
        }
        let Some((line, column, side)) = self.cell_at(event.position) else {
            return;
        };

        let point = self.emulator.point_at(line, column);
        // Whether something is selected matters only to a shift-click, and
        // asking builds the selection's text — the whole history after
        // Select All — so an ordinary click does not ask.
        let press = select::press(
            event.click_count,
            event.modifiers.shift,
            event.modifiers.shift && self.has_selection(),
        );
        if let Ok(mut term) = self.emulator.term().lock() {
            match press {
                // A drag after this extends by the kind begun — whole words
                // after a double-click — which `Selection::update` does.
                select::Press::Begin(kind) => {
                    term.selection = Some(select::selection_at(&term, point, side, kind));
                }
                select::Press::Extend => {
                    if let Some(selection) = term.selection.as_mut() {
                        selection.update(point, side);
                    }
                }
            }
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.emulator.grid_changed();
        }
        self.grab = select::Grab::Select;
        cx.notify();
    }

    /// Whether anything is selected that a Copy would take.
    ///
    /// An ordinary click leaves an empty selection behind it, which is not
    /// one: a menu offering *Copy* over it would copy nothing.
    pub fn has_selection(&self) -> bool {
        self.emulator.selected_text().is_some()
    }

    /// Select the history and the screen — Edit's *Select All*, `cmd-a` and
    /// the right button's. Copied at once when the person copies on select,
    /// as every other selection is.
    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        if let Ok(mut term) = self.emulator.term().lock() {
            term.selection = Some(select::all(&term));
            #[cfg(feature = "adaptive-terminal-refresh")]
            self.emulator.grid_changed();
        }
        if self.copy_on_select {
            self.copy(cx);
        }
        cx.notify();
    }

    pub(super) fn on_mouse_up(&mut self, event: &MouseUpEvent, cx: &mut Context<Self>) {
        // The release lands where the button came up, as the program was
        // told the press and the drag.
        if matches!(self.grab, select::Grab::Report(_))
            && let Some((line, column, _)) = self.cell_clamped(event.position)
        {
            self.reported_at = Some((line, column));
        }
        self.finish_selection(cx);
    }

    /// Whether our own selection is being made — the find bar asks too.
    pub(super) fn selecting(&self) -> bool {
        self.grab == select::Grab::Select
    }

    /// The selection is let go of — by its release, or by the platform
    /// saying the button is up when the release never came.
    pub(super) fn finish_selection(&mut self, cx: &mut Context<Self>) {
        let grab = std::mem::replace(&mut self.grab, select::Grab::None);
        // **A drag the program was told about ends with a release**, on
        // every path — a release heard, a move with the button up, the
        // platform saying so — or `claude` stays in the middle of a drag.
        if let select::Grab::Report(button) = grab {
            let (line, column) = self.reported_at.take().unwrap_or((0, 0));
            let mode = self.emulator.mode();
            if mode.intersects(alacritty_terminal::term::TermMode::MOUSE_MODE) {
                let admitted = self.send_visible(
                    &mouse::release(button, column, line, mouse::Mods::default(), mode),
                    cx,
                );
                if admitted {
                    self.raw_input();
                }
            }
            self.drag_at = None;
            cx.notify();
            return;
        }
        let pressed = grab == select::Grab::Select;
        self.drag_at = None;

        // An empty selection is an ordinary click, and a press that made no
        // selection — a `cmd`-click on a link — leaves the one before it.
        // Copying on either would overwrite the clipboard with nothing new.
        if select::release_copies(pressed, self.copy_on_select, || {
            self.emulator.selected_text().is_some()
        }) {
            self.copy(cx);
        }
        cx.notify();
    }

    pub(super) fn on_mouse_move(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        // The link under the pointer first, selecting or not: a move is how
        // an unfocused terminal learns `cmd` is held. M5.12, task 12.
        self.hover_at(Some(event.position), event.modifiers.platform, cx);
        if self.report_motion(event, cx) {
            return;
        }
        match select::drag(
            self.selecting(),
            event.pressed_button == Some(MouseButton::Left),
        ) {
            select::Drag::Idle => return,
            // The button came up where nobody heard it — outside the
            // window: the drag ends here, and does not go on stretching
            // with nothing held. The find bar asks the same question.
            select::Drag::Release => {
                self.finish_selection(cx);
                return;
            }
            select::Drag::Stretch => {}
        }
        // **Dragging out of the terminal scrolls it**, M12 task 17 — and
        // **keeps scrolling while the pointer is held still there**, M12.1:
        // a line per move stopped the moment the hand stopped, which is
        // exactly when a person holding the pointer below the window is
        // waiting for more. The moves only aim; a clock of the tab's own
        // does the scrolling.
        self.drag_at = Some(event.position);
        if self.past_edge(event.position).is_some() && self.history_to_scroll() {
            self.keep_scrolling_past_edge(cx);
        }
        self.extend_selection_to(event.position, cx);
    }

    /// Tell the program a move, when it is the program's to hear: a drag
    /// it was told the press of, or a move with nothing held when it asked
    /// for every motion. Once per cell. Answers whether the move was the
    /// program's, so our selection leaves it alone.
    fn report_motion(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) -> bool {
        use alacritty_terminal::term::TermMode;
        let mode = self.emulator.mode();
        let held = match self.grab {
            select::Grab::Report(button) => {
                // The program let go of the mouse mid-drag — `claude`
                // exited with the button down: the drag is over, and the
                // shell after it is told nothing.
                if !mode.intersects(TermMode::MOUSE_MODE) {
                    self.grab = select::Grab::None;
                    self.reported_at = None;
                    return true;
                }
                // The button came up where nobody heard it.
                if event.pressed_button != Some(MouseButton::Left) {
                    self.finish_selection(cx);
                    return true;
                }
                Some(button)
            }
            select::Grab::None if mouse::reported(mode, event.modifiers.alt) => None,
            _ => return false,
        };
        let Some((line, column, _)) = self.cell_clamped(event.position) else {
            return held.is_some();
        };
        if self.reported_at == Some((line, column)) {
            return held.is_some();
        }
        if let Some(bytes) = mouse::motion(held, column, line, mods_of(event.modifiers), mode) {
            if !self.send_visible(&bytes, cx) {
                return held.is_some();
            }
            self.raw_input();
            self.reported_at = Some((line, column));
        }
        held.is_some()
    }

    /// Whether the grid has a history a selection held past the edge can
    /// scroll into — none on the alternate screen.
    fn history_to_scroll(&self) -> bool {
        use alacritty_terminal::grid::Dimensions as _;
        let history = self
            .emulator
            .term()
            .lock()
            .map(|term| term.grid().history_size())
            .unwrap_or(0);
        select::edge_scrolls(self.emulator.mode(), history)
    }

    /// Stretch the selection to the cell under a point, held inside the grid.
    fn extend_selection_to(&mut self, position: GpuiPoint<Pixels>, cx: &mut Context<Self>) {
        let Some((line, column, side)) = self.cell_clamped(position) else {
            return;
        };
        let point = self.emulator.point_at(line, column);
        self.emulator.extend_selection(point, side);
        cx.notify();
    }

    /// Which way to scroll while the pointer is outside the terminal, and
    /// how far outside it is: `1` into the history above, `-1` back below,
    /// with the pixels past the edge.
    fn past_edge(&self, position: GpuiPoint<Pixels>) -> Option<(i32, f32)> {
        let bounds = self.bounds?;
        let content = crate::terminal::geometry::content(bounds);
        let top = content.origin.y;
        let bottom = content.bottom();
        if position.y < top {
            Some((1, f32::from(top - position.y)))
        } else if position.y > bottom {
            Some((-1, f32::from(position.y - bottom)))
        } else {
            None
        }
    }

    /// Start the clock that scrolls a selection held past the edge, unless
    /// it is running.
    fn keep_scrolling_past_edge(&mut self, cx: &mut Context<Self>) {
        if self.edge_ticking {
            return;
        }
        self.edge_ticking = true;
        cx.spawn(async move |tab, cx| {
            loop {
                cx.background_executor().timer(scroll::EDGE_TICK).await;
                let going = tab.update(cx, |tab, cx| tab.edge_tick(cx));
                if !matches!(going, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    /// One tick of that clock: scroll by how far out the pointer is held,
    /// and stretch the selection with it — or stop.
    ///
    /// **It stops when the button is up**, asked of the platform rather
    /// than waited for: a release outside the window can go missing, and a
    /// clock nobody stops scrolls the history to its end. It stops, too,
    /// when the pointer is back inside, where the moves do the work again.
    fn edge_tick(&mut self, cx: &mut Context<Self>) -> bool {
        if self.selecting() && !crate::system::primary_button_down() {
            self.finish_selection(cx);
        }
        let held = self
            .drag_at
            .filter(|_| self.selecting() && self.history_to_scroll())
            .and_then(|at| self.past_edge(at).map(|edge| (at, edge)));
        let Some((at, (towards, beyond))) = held else {
            self.edge_ticking = false;
            return false;
        };
        let cell = self.cell.map_or(0.0, |cell| f32::from(cell.height));
        let rows = scroll::edge_rows(beyond, cell) as i32;
        self.emulator.scroll(towards * rows);
        self.scrolled_at = Some(std::time::Instant::now());
        self.extend_selection_to(at, cx);
        true
    }

    /// The cell under a point, held inside the grid.
    ///
    /// [`Self::cell_at`] answers `None` for a point outside, which is
    /// right for a press — a click lands where it lands — and wrong for a
    /// drag, where the pointer being past the edge means *the last cell in
    /// that direction* rather than *nowhere*.
    fn cell_clamped(&self, position: GpuiPoint<Pixels>) -> Option<(usize, usize, Side)> {
        let (bounds, cell) = (self.bounds?, self.cell?);
        let content = crate::terminal::geometry::content(bounds);
        let inside =
            |size: Pixels, step: Pixels| (f32::from(size) / f32::from(step)).floor().max(1.0) - 1.0;

        let x = f32::from(position.x - content.origin.x).max(0.0);
        let y = f32::from(position.y - content.origin.y).max(0.0);
        let column_exact = (x / f32::from(cell.width)).min(inside(content.size.width, cell.width));
        let line = (y / f32::from(cell.height))
            .floor()
            .min(inside(content.size.height, cell.height)) as usize;

        // Half a cell decides which side of the character the pointer is
        // on, as it does for a press.
        let side = if column_exact.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };
        Some((line, column_exact.floor() as usize, side))
    }

    /// The grid cell under a window point.
    pub(super) fn cell_at(&self, position: GpuiPoint<Pixels>) -> Option<(usize, usize, Side)> {
        let (bounds, cell) = (self.bounds?, self.cell?);
        let content = crate::terminal::geometry::content(bounds);

        let x = f32::from(position.x - content.origin.x);
        let y = f32::from(position.y - content.origin.y);
        if x < 0.0 || y < 0.0 {
            return None;
        }

        let column_exact = x / f32::from(cell.width);
        let column = column_exact.floor() as usize;
        let line = (y / f32::from(cell.height)).floor() as usize;

        // Half a cell decides which side of the character the cursor
        // landed on: without it the selection loses the last character or
        // grabs an extra one.
        let side = if column_exact.fract() < 0.5 {
            Side::Left
        } else {
            Side::Right
        };

        Some((line, column, side))
    }

    /// What the pointer's actions do on the terminal — M5.12, task 10:
    /// Select All, from `cmd-a` and from the Edit menu, which is enabled
    /// because this handler is on the focus path.
    pub(super) fn bind_select(&self, el: gpui::Div, cx: &mut Context<Self>) -> gpui::Div {
        el.on_action(cx.listener(|tab, _: &SelectAll, _window, cx| tab.select_all(cx)))
    }
}

/// The modifiers a report carries, from GPUI's.
fn mods_of(modifiers: gpui::Modifiers) -> mouse::Mods {
    mouse::Mods {
        shift: modifiers.shift,
        alt: modifiers.alt,
        control: modifiers.control,
    }
}

#[cfg(test)]
mod geometry_tests {
    use super::*;
    use crate::terminal::emulator::{Emulator, GridSize};
    use crate::terminal::view::Look;
    use gpui::{AppContext, Bounds, point, size};

    #[gpui::test]
    fn compact_fit_and_pointer_coordinates_agree_through_resize(cx: &mut gpui::TestAppContext) {
        let mut command = portable_pty::CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 2"]);
        let emulator =
            Emulator::spawn_job(command, std::path::Path::new("/"), GridSize::new(80, 24)).unwrap();
        cx.update(|cx| {
            let tab = cx.new(|cx| {
                TerminalTab::new(
                    emulator,
                    Look::of(&Default::default(), Default::default()),
                    "/".into(),
                    None,
                    cx,
                )
            });
            tab.update(cx, |tab, _| {
                let cell = size(px(8.), px(16.));
                let bounds = Bounds::new(point(px(40.), px(70.)), size(px(826.), px(202.)));
                tab.bounds = Some(bounds);
                tab.cell = Some(cell);
                tab.fit(bounds, cell);
                assert_eq!(tab.emulator.size(), GridSize::new(100, 12));
                assert_eq!(
                    tab.cell_at(point(px(54.), px(76.))),
                    Some((0, 0, Side::Left))
                );
                assert_eq!(
                    tab.cell_at(point(px(849.), px(266.))),
                    Some((11, 99, Side::Right))
                );
                assert_eq!(
                    tab.cell_clamped(point(px(900.), px(400.)))
                        .map(|(r, c, _)| (r, c)),
                    Some((11, 99))
                );
                assert_eq!(tab.past_edge(point(px(54.), px(74.))), Some((1, 1.)));
                assert_eq!(tab.past_edge(point(px(54.), px(268.))), Some((-1, 1.)));
                let shorter = Bounds::new(bounds.origin, size(bounds.size.width, px(106.)));
                tab.bounds = Some(shorter);
                tab.fit(shorter, cell);
                assert_eq!(
                    tab.pending_size.as_ref().unwrap().size,
                    GridSize::new(100, 6)
                );
                assert_eq!(
                    tab.cell_clamped(point(px(900.), px(400.)))
                        .map(|(r, c, _)| (r, c)),
                    Some((5, 99))
                );
            });
        });
    }
}
