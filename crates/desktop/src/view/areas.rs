//! The areas, drawn — M12.8, decision 18, the person's A1 of the mockups.
//!
//! **Each area is a card of its own**: §4.1's card — `surface.app`, a 1
//! border in `divider`, `radius::WORK`, a point of padding, and its corners
//! drawn over what it holds by `panels::card_corners_edged` — once per area, the
//! cards `space::WORK_BETWEEN` apart on the window's `surface.panel`, the
//! same gap as between the top cards and the bottom dock's. Each holds its
//! own strip above its front tab's body.
//!
//! **The border between two areas is that gap.** Its grip lies over it and
//! reaches a point and a half into each card, draws nothing at rest or
//! under the hand — the resize cursor is the only sign — and a double
//! click puts the split back at a half.
//!
//! The room the areas divide is measured by a `canvas` into `work_at` and
//! cut into frames by the seat's tree (`Root::area_frames`); a card is
//! placed at its frame **as a share of the room**, so a window being
//! resized moves the cards with it in the frame it is resized in, and only
//! the floors wait a frame for the new measure. The strips stay inline in
//! `Root::render`, as `docs/traps.md` says they must — they read file and
//! editor entities, and a cached region is notified by the root alone.

use std::hash::{DefaultHasher, Hash, Hasher};

use gpui::prelude::*;
use gpui::{Context, CursorStyle, ElementId, Window, div, px, relative, rgb};
use guild_session::layout::{Axis, Border, Frame};
use guild_session::{AreaId, Seat};

use crate::Root;
use crate::theme;
use crate::view::panels::{GRIP_REACH, card_corners_edged};
use crate::view::tabs::TabDrag;

/// How far an area's card stands in from its frame, on each side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Insets {
    /// From the frame's top edge.
    pub top: f32,
    /// From its right edge.
    pub right: f32,
    /// From its bottom edge.
    pub bottom: f32,
    /// From its left edge.
    pub left: f32,
}

/// How far a frame may miss the room's edge and still be on it: frames
/// are sums of floating-point shares.
const SLACK: f32 = 0.5;

/// Where an area's card stands inside its `frame` in the `room`: **half
/// the gap in from every edge it shares with a neighbour**, so two cards
/// are one gap apart whoever owns it, and flush with the room at the
/// room's own edges — the gap there is the row's, to a dock's card or the
/// window.
pub fn insets(frame: Frame, room: Frame, gap: f32) -> Insets {
    let half = gap / 2.0;
    let inner = |edge: f32, rooms: f32| {
        if (edge - rooms).abs() > SLACK {
            half
        } else {
            0.0
        }
    };
    Insets {
        top: inner(frame.y, room.y),
        right: inner(frame.x + frame.width, room.x + room.width),
        bottom: inner(frame.y + frame.height, room.y + room.height),
        left: inner(frame.x, room.x),
    }
}

/// An element's id for one area's copy of something — its card, its
/// strip — so that two areas' copies never share GPUI's element state.
/// An area's id is a `Uuid` that `guild_session` keeps to itself; its
/// hash is as unique among a window's handful of areas.
pub(crate) fn element_id(name: &'static str, area: impl Hash) -> ElementId {
    let mut hasher = DefaultHasher::new();
    area.hash(&mut hasher);
    ElementId::NamedInteger(name.into(), hasher.finish())
}

impl Root {
    /// The room between the side docks, divided into the seat's areas: a
    /// card per area at its frame, and a grip over every border between
    /// two of them, drawn last so it takes the press from both cards.
    ///
    /// Until the room has been measured once — the first frame — or while
    /// the seat has no areas yet, the focused area is drawn alone, filling
    /// the room (decision 17).
    pub(crate) fn render_areas(&self, window: &Window, cx: &mut Context<Root>) -> gpui::AnyElement {
        let work_at = self.work_at.clone();
        let focused = self.focused_area();
        let frames = self.area_frames();
        let room = self
            .work_room()
            .filter(|room| room.width > 0.0 && room.height > 0.0);

        let place = div()
            .relative()
            .flex_1()
            .min_w_0()
            .min_h_0()
            // The room, as it was laid out, for the next frame to divide.
            // A move asks for that frame: a place written at prepaint is
            // read a frame late (traps.md).
            .child(
                gpui::canvas(
                    move |bounds, window, _cx| {
                        if work_at.get() != Some(bounds) {
                            work_at.set(Some(bounds));
                            window.refresh();
                        }
                    },
                    |_, _, _, _| {},
                )
                // The trap: an absolute box with no inset sits where it
                // would have flowed.
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );

        let (Some(room), false) = (room, frames.is_empty()) else {
            return place
                .child(
                    self.render_area(focused, true, false, window, cx)
                        .size_full(),
                )
                .into_any_element();
        };
        let gap = self.rem_px(theme::space::WORK_BETWEEN);
        let cards = frames.iter().map(|(area, frame)| {
            let inset = insets(*frame, room, gap);
            div()
                .absolute()
                .left(relative(frame.x / room.width))
                .top(relative(frame.y / room.height))
                .w(relative(frame.width / room.width))
                .h(relative(frame.height / room.height))
                .child(
                    self.render_area(
                        Some(*area),
                        Some(*area) == focused,
                        frames.len() > 1,
                        window,
                        cx,
                    )
                    .absolute()
                    .top(px(inset.top))
                    .right(px(inset.right))
                    .bottom(px(inset.bottom))
                    .left(px(inset.left)),
                )
        });
        let place = place.children(cards.collect::<Vec<_>>());
        let grips = match self
            .active_seat()
            .and_then(|seat| Some((seat, self.stage.areas(seat)?)))
        {
            Some((seat, areas)) => areas
                .tree()
                .borders(room, self.area_floor())
                .into_iter()
                .map(|border| self.render_area_grip(seat, border, room, cx))
                .collect(),
            None => Vec::new(),
        };
        place.children(grips).into_any_element()
    }

    /// One area's card: its strip above its front tab's body, or the
    /// sessions list in the focused one when it is up.
    ///
    /// **A press in an area that is not focused focuses it first** —
    /// heard in the capture phase, before whatever was pressed acts, so a
    /// tile, a tab or a terminal pressed there acts in its own area
    /// (decision 16's rule for the start screen, which a strip and a body
    /// need as much).
    fn render_area(
        &self,
        area: Option<AreaId>,
        focused: bool,
        divided: bool,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        // One outline, on the innermost place that has the keys: with
        // more than one area the focused one's edge is the patina the
        // focused pane's outline is drawn in — unless its front tab is
        // divided, where the pane's outline alone says it (§6.36).
        let undivided = self
            .area_front(area)
            .and_then(|front| self.tabs.iter().find(|tab| tab.id == front))
            .is_none_or(|tab| tab.panes.ids().len() <= 1);
        let lit = focused && divided && undivided;
        let mut edge: gpui::Hsla = rgb(if lit {
            theme.accent.patina
        } else {
            theme.surface.divider
        })
        .into();
        if lit {
            edge.a = theme::alpha::LIT_OUTLINE;
        }
        div()
            .id(element_id("area", area))
            .flex()
            .flex_col()
            .rounded(theme::radius::WORK)
            // The border is `card_corners`', drawn over the children; this
            // is the room it takes.
            .p(px(1.0))
            .bg(rgb(theme.surface.app))
            .overflow_hidden()
            .relative()
            .when_some(area.filter(|_| !focused), |card, area| {
                card.capture_any_mouse_down(cx.listener(
                    move |root, event: &gpui::MouseDownEvent, _window, cx| {
                        root.focus_area(area, cx);
                        // **A press in the strip does not scroll it.**
                        // Focusing brings the area's front tab forward,
                        // and the render brings the tab in front into
                        // view; a strip that scrolls between the press
                        // and the release puts another tab under the
                        // hand, the release lands there, and the click —
                        // which GPUI counts only when both land on one
                        // tab — is lost (M12.8's window check, F1). So
                        // the front counts as shown: the tab pressed
                        // comes forward on the release, and it is that
                        // one the strip then brings into view. A press
                        // on the body still reveals the front — nothing
                        // under the hand moves.
                        let in_strip = root
                            .tab_scroll
                            .get(&Some(area))
                            .is_some_and(|strip| strip.bounds().contains(&event.position));
                        if in_strip {
                            root.tab_scrolled.insert(Some(area), root.active_tab);
                        }
                    },
                ))
            })
            .child(self.render_tabs(area, cx))
            .child(self.render_area_body(area, window, cx))
            // The card closes its own corners over what it holds —
            // `card_corners` says why.
            .children(card_corners_edged(&theme, edge))
    }

    /// An area's body — and, when the area is empty, the place a dragged
    /// tab is let go of: its start screen takes the drop and the tab goes
    /// there (decision 8), lit `accent.brass_bg` while the tab is over it,
    /// as a dock's drop zone is (§6.15).
    ///
    /// **Measured, as the room the areas divide is** — M12.8: a `canvas`
    /// writes the body's bounds into `body_at`, the room its front tab's
    /// panes divide (`view::panes`) and the height an empty area's start
    /// screen is fitted to (decision 20). A move asks for the next frame,
    /// `render_areas`' reason.
    fn render_area_body(
        &self,
        area: Option<AreaId>,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let body_at = self.body_at.clone();
        let measure = gpui::canvas(
            move |bounds, window, _cx| {
                let mut at = body_at.borrow_mut();
                if at.get(&area) != Some(&bounds) {
                    at.insert(area, bounds);
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        // The trap: an absolute box with no inset sits where it would
        // have flowed.
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let body = self.render_body(area, window, cx);
        let empty = area.filter(|area| self.area_tabs(Some(*area)).next().is_none());
        let theme = self.theme;
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(measure)
            .when_some(empty, |place, area| {
                place
                    .drag_over::<TabDrag>(move |style, _, _, _| {
                        style.bg(rgb(theme.accent.brass_bg))
                    })
                    .on_drop(cx.listener(move |root, drag: &TabDrag, window, cx| {
                        root.move_tab_to_area(drag.id, area, None, window, cx);
                        root.land_tab(drag.id, drag.origin(window), cx);
                    }))
            })
            .child(body)
            .into_any_element()
    }

    /// The grip over the gap between two areas: centred on the border,
    /// across the node it divides, `GRIP_REACH` to the hand, drawing
    /// nothing — the resize cursor is the only sign. A press takes hold, a
    /// second click puts the border back at a half.
    fn render_area_grip(
        &self,
        seat: Seat,
        border: Border,
        room: Frame,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let sideways = border.axis == Axis::Horizontal;
        let span = border.span;
        // The line the border is, as a share of the room, with no width of
        // its own; the grip hangs over it.
        let line = if sideways {
            div()
                .absolute()
                .left(relative(border.at / room.width))
                .top(relative(span.y / room.height))
                .h(relative(span.height / room.height))
                .w(px(0.0))
        } else {
            div()
                .absolute()
                .top(relative(border.at / room.height))
                .left(relative(span.x / room.width))
                .w(relative(span.width / room.width))
                .h(px(0.0))
        };
        let path = border.path;
        line.child(
            div()
                .id(element_id("area-grip", &path))
                .debug_selector(move || {
                    if sideways {
                        "area-grip-horizontal"
                    } else {
                        "area-grip-vertical"
                    }
                    .into()
                })
                .absolute()
                .when(sideways, |grip| {
                    grip.left(px(-GRIP_REACH / 2.0))
                        .top_0()
                        .bottom_0()
                        .w(px(GRIP_REACH))
                })
                .when(!sideways, |grip| {
                    grip.top(px(-GRIP_REACH / 2.0))
                        .left_0()
                        .right_0()
                        .h(px(GRIP_REACH))
                })
                // Over both cards: a press on it never reaches a terminal.
                .occlude()
                .cursor(if sideways {
                    CursorStyle::ResizeLeftRight
                } else {
                    CursorStyle::ResizeUpDown
                })
                .on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |root, event: &gpui::MouseDownEvent, _window, cx| {
                        root.grab_area_border(seat, path.clone(), event, cx);
                    }),
                ),
        )
        .into_any_element()
    }
}
