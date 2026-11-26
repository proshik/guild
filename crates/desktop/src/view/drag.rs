//! A tool dragged from the status bar to a dock.
//!
//! GPUI's drag: a button carries `on_drag` with a `DragTool` and a view
//! drawn under the pointer; while the drag lives the root draws a zone
//! at each of the three edges, each an `on_drop::<DragTool>` target lit
//! by `drag_over` — task 0 of M8.3.1 showed a zone drawn during the
//! drag receives the drop. Decision 3, section 6.15.
//!
//! M5.8's lesson applies twice. A release outside the window never
//! arrives, so the root forgets the drag on any move with no button
//! held; and a zone drawn over the editor occludes it only while the
//! drag lives, which is the moment nothing under it is wanted.

use gpui::prelude::*;
use gpui::{Context, IntoElement, Pixels, Point, Render, Size, Window, div, px, rgb};
use guild_session::{Dock, Tool};

use crate::{Root, icons, theme};

/// What is dragged: a tool, by its button.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DragTool(pub Tool);

/// What the pointer carries while a tool is dragged: the button itself,
/// picked up.
pub struct DragPreview {
    pub tool: Tool,
    pub theme: theme::Theme,
}

impl Render for DragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .bg(rgb(theme.surface.raised))
            .child(icons::tool(self.tool).text_color(rgb(theme.text.primary)))
    }
}

/// Which dock a point over the window is dropped into: the left 52, the
/// right 52, the panel heading's 42 above the status bar's 26 — and
/// nowhere in the middle or on the bar itself. A corner belongs to the
/// side. The zones' geometry, shared with the view so that what is
/// drawn and what is tested are one thing.
pub fn drop_target(at: Point<Pixels>, window: Size<Pixels>) -> Option<Dock> {
    let zone = base_px(theme::space::DROP_ZONE);
    let head = base_px(theme::space::PANEL_HEAD);
    let bar = base_px(theme::space::STATUS_BAR);
    if at.y >= window.height - bar {
        return None;
    }
    if at.x < zone {
        return Some(Dock::Left);
    }
    if at.x >= window.width - zone {
        return Some(Dock::Right);
    }
    if at.y >= window.height - bar - head {
        return Some(Dock::Bottom);
    }
    None
}

/// A design-system number as the pixels it is at the base scale — the
/// scale the zones' geometry is reasoned about in; the window's rem size
/// scales the zones themselves.
fn base_px(rems: gpui::Rems) -> Pixels {
    px(rems.0 * crate::scale::BASE)
}

impl Root {
    /// The zone at one edge, drawn while a tool is dragged: on the
    /// panel's surface with a hairline, lit brass while the tool is over
    /// it, and the drop that moves the tool there.
    pub(crate) fn render_drop_zone(&self, dock: Dock, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let base = div()
            .id(("drop-zone", dock as usize))
            .flex_none()
            .bg(rgb(theme.surface.panel))
            .border_color(rgb(theme.surface.divider))
            .drag_over::<DragTool>(move |style, _, _, _| style.bg(rgb(theme.accent.brass_bg)))
            .on_drop(cx.listener(move |root, drag: &DragTool, _window, cx| {
                root.tool_drag = None;
                root.move_tool(drag.0, dock, cx);
            }));
        match dock {
            Dock::Left => base.w(theme::space::DROP_ZONE).h_full().border_r_1(),
            Dock::Right => base.w(theme::space::DROP_ZONE).h_full().border_l_1(),
            Dock::Bottom => base.w_full().h(theme::space::PANEL_HEAD).border_t_1(),
        }
        .into_any_element()
    }
}

/// Timing shared by the two held edge gestures; only a painted frame moves them.
/// Pauses reset sampling, and a delayed frame catches up by at most 32 ms.
pub(crate) struct EdgeMotion {
    sampled_at: std::time::Instant,
    pub pending: bool,
}
impl EdgeMotion {
    pub fn new(now: std::time::Instant) -> Self {
        Self {
            sampled_at: now,
            pending: false,
        }
    }
    pub fn pause(&mut self, now: std::time::Instant) {
        self.sampled_at = now;
    }
    pub fn distance(&mut self, now: std::time::Instant, row: Pixels) -> Pixels {
        let elapsed = now
            .saturating_duration_since(self.sampled_at)
            .min(std::time::Duration::from_millis(32));
        self.sampled_at = now;
        row * (20. * elapsed.as_secs_f32())
    }
}
