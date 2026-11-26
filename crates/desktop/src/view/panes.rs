//! A tab's panes, drawn — M12.8, decision 19, the person's P3 of the
//! mockups.
//!
//! **Inside a tab, panes are divided by a hairline**: 1 point of
//! `divider` on the border, the grip's nine over it drawing nothing more
//! under the hand — the resize cursor is the only sign, as on every handle
//! that sizes something (§4.1) — and a double click puts the border back at
//! a half. **The focused pane carries an outline**, 1 point of
//! `accent.patina` at `alpha::LIT_OUTLINE`, `radius::PANE`, drawn
//! `space::PANE_INSET` in from the pane's edges over what it shows, only
//! while the tab has more than one pane — a tab of one is drawn by
//! `view::body` alone and never comes here.
//!
//! The panes are placed in the area's body **as shares of it**, the way
//! `view::areas` places the areas in the room: the body is measured by a
//! `canvas` into `body_at`, the tab's tree cuts it into frames with the
//! floors of decision 17, and a pane stands at its frame's share, so a
//! window being resized moves the panes with it in the frame it is resized
//! in. A pane that shows something is drawn by `view::body`'s
//! `render_shown`, which stays the one view that binds a session's view;
//! an empty one is its start screen, fitted to its height (decision 20).

use gpui::prelude::*;
use gpui::{Context, CursorStyle, Hsla, Window, div, px, relative, rgb};
use guild_session::AreaId;
use guild_session::layout::{Axis, Border, Floor, Frame};

use crate::start::Fits;
use crate::theme;
use crate::view::areas::element_id;
use crate::view::panels::GRIP_REACH;
use crate::{Root, Tab, TabId};

/// The line between two panes: one point, decision 19. The second side
/// draws after it (`Frame::inside_lines`).
const HAIRLINE: f32 = 1.0;

impl Root {
    /// The panes of `tab`, the tab in front of `area`: each at its frame
    /// in the area's body, the hairlines between them, the focused one's
    /// outline while `area` has the keys.
    ///
    /// **A press in a pane that has not the keys gives them to it first**
    /// — heard in the capture phase, before whatever was pressed acts, as
    /// a press in another area is (`view::areas`), so a settings row, a
    /// tile or a terminal pressed there acts in its own pane.
    ///
    /// Before the area's first frame there is no room to measure floors
    /// against, and the panes stand at their fractions alone.
    pub(crate) fn render_panes(
        &self,
        tab: &Tab,
        area: Option<AreaId>,
        area_focused: bool,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let measured = self.body_room(area);
        let (room, floor) = match measured {
            Some(room) => (room, self.area_floor()),
            None => (
                Frame::new(0.0, 0.0, 1.0, 1.0),
                Floor {
                    width: 0.0,
                    height: 0.0,
                },
            ),
        };
        let tree = tab.panes.tree();
        let keys = tab.panes.focused();
        let id = tab.id;
        // The outline says where the keys are, as the patina underline of
        // the focused area's front tab does; `alpha::LIT_OUTLINE` is the
        // start card's lit outline (§6.35), the one outline on a screen.
        let mut outline: Hsla = rgb(theme.accent.patina).into();
        outline.a = theme::alpha::LIT_OUTLINE;

        let places: Vec<gpui::AnyElement> = tree
            .frames(room, floor)
            .into_iter()
            .map(|(pane, frame)| {
                let front = pane == keys;
                // **After the hairline, not under it** — the second
                // window check's O4: the line at a border is the second
                // side's first point, so a pane drawn from its frame had
                // its outline a point nearer the line than its
                // neighbour's. Measured frames only: before the first,
                // the room is a unit square and a point is all of it.
                let frame = match measured {
                    Some(_) => frame.inside_lines(room, HAIRLINE),
                    None => frame,
                };
                let content = match tab.panes.body_of(pane) {
                    Some(body) => self.render_shown(id, pane, body, cx),
                    None => {
                        let fits = match measured {
                            Some(_) => self.start_fits(frame.width, frame.height),
                            None => Fits::ALL,
                        };
                        // Only the pane with the keys, in the area with
                        // them, has the lit copy (decision 16).
                        self.render_start(area_focused && front, fits, window, cx)
                    }
                };
                div()
                    // Each place's copy of what it draws keeps its own
                    // element state — two start screens never merge.
                    .id(element_id("pane", pane))
                    .absolute()
                    .left(relative(frame.x / room.width))
                    .top(relative(frame.y / room.height))
                    .w(relative(frame.width / room.width))
                    .h(relative(frame.height / room.height))
                    .flex()
                    .flex_col()
                    .overflow_hidden()
                    .when(!front, |place| {
                        place.capture_any_mouse_down(cx.listener(
                            move |root, _event: &gpui::MouseDownEvent, _window, cx| {
                                root.press_pane(id, pane, cx);
                            },
                        ))
                    })
                    .child(content)
                    .when(front && area_focused, |place| {
                        place.child(
                            div()
                                .absolute()
                                .top(theme::space::PANE_INSET)
                                .right(theme::space::PANE_INSET)
                                .bottom(theme::space::PANE_INSET)
                                .left(theme::space::PANE_INSET)
                                .rounded(theme::radius::PANE)
                                .border_1()
                                .border_color(outline),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let borders: Vec<gpui::AnyElement> = tree
            .borders(room, floor)
            .into_iter()
            .map(|border| self.render_pane_border(id, border, room, cx))
            .collect();

        div()
            .relative()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .children(places)
            // Drawn last, so a grip takes the press from both panes.
            .children(borders)
            .into_any_element()
    }

    /// The hairline between two panes and the grip over it: the line on
    /// the border, across the node it divides, as a share of the room;
    /// the grip `GRIP_REACH` to the hand, centred on the line and drawing
    /// nothing — the resize cursor is the only sign. A press takes hold,
    /// a second click puts the border back at a half.
    fn render_pane_border(
        &self,
        tab: TabId,
        border: Border,
        room: Frame,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let sideways = border.axis == Axis::Horizontal;
        let span = border.span;
        let line = if sideways {
            div()
                .absolute()
                .left(relative(border.at / room.width))
                .top(relative(span.y / room.height))
                .h(relative(span.height / room.height))
                .w(px(HAIRLINE))
        } else {
            div()
                .absolute()
                .top(relative(border.at / room.height))
                .left(relative(span.x / room.width))
                .w(relative(span.width / room.width))
                .h(px(HAIRLINE))
        };
        // The grip's reach beyond the line's one point, on either side.
        let beside = -(GRIP_REACH - HAIRLINE) / 2.0;
        let path = border.path;
        line.bg(rgb(theme.surface.divider))
            .child(
                div()
                    .id(element_id("pane-grip", (tab, &path)))
                    .debug_selector(move || {
                        if sideways {
                            "pane-grip-horizontal"
                        } else {
                            "pane-grip-vertical"
                        }
                        .into()
                    })
                    .absolute()
                    .when(sideways, |grip| {
                        grip.left(px(beside)).top_0().bottom_0().w(px(GRIP_REACH))
                    })
                    .when(!sideways, |grip| {
                        grip.top(px(beside)).left_0().right_0().h(px(GRIP_REACH))
                    })
                    // Over both panes: a press on it never reaches a
                    // terminal.
                    .occlude()
                    .cursor(if sideways {
                        CursorStyle::ResizeLeftRight
                    } else {
                        CursorStyle::ResizeUpDown
                    })
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(move |root, event: &gpui::MouseDownEvent, _window, cx| {
                            root.grab_pane_border(tab, path.clone(), event, cx);
                        }),
                    ),
            )
            .into_any_element()
    }
}
