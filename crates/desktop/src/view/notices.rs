//! Viewport-bounded notices with fixed actions and independently scrolling text.

use gpui::prelude::*;
use gpui::{Context, Window, div, px, rgb};
use std::{cell::Cell, rc::Rc, time::Instant};

use crate::{Root, notify::Level, theme};

#[derive(Clone, Default)]
pub(crate) struct NoticeView {
    scroll: gpui::ScrollHandle,
    bounds: Rc<Cell<gpui::Bounds<gpui::Pixels>>>,
}

impl Root {
    /// Pointer truth includes child controls and changes beneath a stationary pointer.
    pub(crate) fn sync_notice_hover(&mut self, window: &Window, now: Instant) {
        for (id, view) in self.notice_views.borrow().iter() {
            self.notices.hover(
                *id,
                !self.notice_pointer_exited
                    && window.is_window_hovered()
                    && view.bounds.get().contains(&window.mouse_position()),
                now,
            );
        }
    }

    /// The whole stack shares the current viewport budget. Nothing here takes focus.
    pub(crate) fn render_notices(
        &self,
        window: &Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let notices: Vec<_> = self
            .notices
            .visible()
            .map(|(id, n)| (id, n.text.clone(), n.level, n.copied, n.terminal.is_some()))
            .collect();
        let rem = window.rem_size();
        let inset = theme::space::NOTICE_INSET.to_pixels(rem);
        let gap = theme::space::CARD_GAP.to_pixels(rem);
        let padding = theme::space::CARD_PADDING.to_pixels(rem);
        let width = theme::space::NOTICE
            .to_pixels(rem)
            .min((window.viewport_size().width - inset * 2.).max(px(0.)));
        let height = ((window.viewport_size().height
            - inset * 2.
            - gap * notices.len().saturating_sub(1) as f32)
            / notices.len().max(1) as f32)
            .max(px(0.));
        let height = px(f32::from(height).floor());
        let text_height = (height - padding * 2. - px(2.)).max(px(0.));
        let mut views = self.notice_views.borrow_mut();
        views.retain(|id, _| notices.iter().any(|n| n.0 == *id));

        // macOS keeps the last in-window coordinates on MouseExited. Remember
        // that event across redraws and watch ticks, even when no cards remain.
        let root = cx.weak_entity();
        let pointer_events = gpui::canvas(
            |_, _, _| (),
            move |_, _, window, _| {
                let moved = root.clone();
                window.on_mouse_event(move |_: &gpui::MouseMoveEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture {
                        let _ = moved.update(cx, |root, _| {
                            root.notice_pointer_exited = false;
                            root.sync_notice_hover(window, Instant::now());
                        });
                    }
                });
                let exited = root.clone();
                window.on_mouse_event(move |_: &gpui::MouseExitEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture {
                        let _ = exited.update(cx, |root, _| {
                            root.notice_pointer_exited = true;
                            root.sync_notice_hover(window, Instant::now());
                        });
                    }
                });
            },
        )
        .absolute();
        div().absolute().bottom(inset).right(inset).w(width)
            .flex().flex_col().gap(gap)
            .child(pointer_events)
            .children(notices.into_iter().map(|(id, text, level, copied, terminal)| {
                let view = views.entry(id).or_default().clone();
                let bounds_cell = view.bounds.clone();
                let root = cx.weak_entity();
                let pointer = gpui::canvas(move |bounds, _, _| { bounds_cell.set(bounds); bounds }, move |bounds, _, window, cx| {
                    let inside = window.is_window_hovered() && bounds.contains(&window.mouse_position());
                    let deferred = root.clone();
                    cx.defer(move |cx| { let _ = deferred.update(cx, |root, _| root.notices.hover(id, inside && !root.notice_pointer_exited, Instant::now())); });
                }).absolute().top_0().left_0().size_full();
                let (ground, edge) = match level {
                    Level::Error => (theme.accent.rust_bg, theme.accent.rust),
                    Level::Warning => (theme.accent.brass_bg, theme.accent.brass),
                    Level::Info => (theme.surface.raised, theme.surface.divider),
                };
                let body_height = (text_height - if terminal { theme::space::STATUS_BUTTON.to_pixels(rem) + gap } else { px(0.) }).max(px(0.));
                let scroll = view.scroll.clone();
                div().id(("notice", id)).debug_selector(move || format!("notice-{id}"))
                    .relative().w(width).max_h(height).flex_none().p(padding)
                    .bg(rgb(ground)).border_1().border_color(rgb(edge)).rounded(theme::radius::CARD)
                    .text_size(theme::size::UI).text_color(rgb(theme.text.primary))
                    .occlude()
                    // The native scroller handles the wheel before this ancestor,
                    // which also consumes gestures at either end of its range.
                    .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| { window.prevent_default(); cx.stop_propagation(); })
                    .on_mouse_down(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(gpui::MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(gpui::MouseButton::Right, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(gpui::MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                    .on_mouse_move(|_, _, cx| cx.stop_propagation())
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(pointer)
                    .flex().items_start().gap(gap)
                    .child(div().flex_1().min_w_0().flex().flex_col()
                        .child(div().relative().min_w_0()
                            .child(div().id(("notice-text", id)).debug_selector(move || format!("notice-text-{id}"))
                                .w_full().max_h(body_height).overflow_y_scroll().track_scroll(&scroll)
                                .child(div().w_full().debug_selector(move || format!("notice-message-{id}")).child(text)))
                            .when_some(super::thumb(&scroll), |body, (top, length)| body.child(
                                div().absolute().right_0().top(px(top)).w(theme::space::SCROLLBAR).h(px(length))
                                    .rounded_full().bg(rgb(theme.text.faint)))))
                        .when(terminal, |body| body.child(div().id(("notice-terminal", id))
                            .debug_selector(move || format!("notice-terminal-{id}"))
                            .mt(gap).h(theme::space::STATUS_BUTTON).flex_none().cursor_pointer()
                            .rounded(theme::radius::ROW).bg(rgb(theme.surface.card))
                            .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.card)))
                            .child(self.ui().text(crate::i18n::Text::UpdateTerminal))
                            .on_click(cx.listener(move |root, _, _, cx| {
                                cx.stop_propagation();
                                let terminal = root.notices.visible().find(|(key, _)| *key == id).and_then(|(_, n)| n.terminal.clone());
                                if let Some((project, path)) = terminal {
                                    root.select_project(project, cx);
                                    if root.active_project == Some(project) { root.open_terminal_at(&path, true, cx); }
                                    else { root.warn(root.ui().text(crate::i18n::Text::UpdateTerminalUnavailable).into()); }
                                }
                            })))))
                    .child(div().flex_none().flex().items_center().gap(gap)
                        .child(div().id(("notice-copy", id)).debug_selector(move || format!("notice-copy-{id}"))
                            .flex_none().cursor_pointer()
                            .on_click(cx.listener(move |root, _, _, cx| {
                                cx.stop_propagation();
                                if let Some(text) = root.notices.copy(id) { cx.write_to_clipboard(gpui::ClipboardItem::new_string(text)); }
                                cx.notify();
                            }))
                            .child(crate::icons::toolbar(if copied { "ui/check.svg" } else { "ui/menu-copy.svg" })
                                .size(theme::space::STATUS_ICON).text_color(rgb(if copied { theme.accent.green } else { theme.text.muted }))))
                        .child(div().id(("notice-close", id)).debug_selector(move || format!("notice-close-{id}"))
                            .flex_none().cursor_pointer().group("notice-close")
                            .on_click(cx.listener(move |root, _, _, cx| { cx.stop_propagation(); root.notices.dismiss(id); cx.notify(); }))
                            .child(crate::icons::glyph("icons/close.svg", theme::size::UI)
                                .text_color(rgb(theme.text.muted))
                                .group_hover("notice-close", |cross| cross.text_color(rgb(theme.text.primary))))))
            })).into_any_element()
    }
}

#[cfg(test)]
mod tests;
