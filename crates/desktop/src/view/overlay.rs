//! A shared native overlay host: content never owns the surrounding window.
use crate::view::tooltip::Hint as _;
use crate::{Root, theme};
use gpui::{Context, div, prelude::*, rgb};
use guild_session::overlay::{Content, Target};

/// Overlay controls use Guild's palette for both the control and its tooltip.
fn header_control(
    id: impl Into<gpui::ElementId>,
    label: &'static str,
    palette: theme::Theme,
) -> gpui_kit::base::Button {
    gpui_kit::base::Button::new(id)
        .accessibility_label(label)
        .flex_none()
        .h(theme::space::HEAD_BUTTON)
        .min_w(theme::space::STATUS_BUTTON)
        .px(theme::space::CARD_GAP)
        .rounded(theme::radius::ROW)
        .text_size(theme::size::BODY)
        .text_color(rgb(palette.text.primary))
        .cursor_pointer()
        .hover(|style| style.bg(crate::theme::control_hover(palette, palette.surface.card)))
        .active(|style| style.bg(theme::control_hover(palette, palette.surface.card)))
        .focus(|style| style.bg(theme::control_hover(palette, palette.surface.card)))
        .hint_delay()
        .tooltip(move |_, cx| {
            cx.new(|_| super::tooltip::Tooltip::text(label, palette))
                .into()
        })
}

impl Root {
    pub(crate) fn render_overlay_host(
        &self,
        target: Target,
        content: gpui::AnyElement,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let entries = self.overlays.at(&target);
        if entries.is_empty() {
            return content;
        }
        let mut host = div()
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .child(content);
        let mut strip = div()
            .absolute()
            .top(theme::space::CARD_GAP)
            .right(theme::space::CARD_GAP)
            .flex()
            .gap(theme::space::CARD_GAP);
        for entry in entries {
            let id = entry.id.clone();
            let owner = entry.owner.clone();
            self.ensure_overlay_focus(&id, cx);
            let viewport = self.overlay_focus.borrow()[&id].viewport.clone();
            host = host.child(super::overlay_viewport::measure(viewport, cx));
            if entry.visible {
                host = host.child(gpui::deferred(self.render_overlay_card(&entry, cx)));
            } else if !matches!(entry.content, Content::Terminal { .. }) {
                // Terminal Jobs owns recovery for hidden programs; corner chips
                // would duplicate that list and cover the terminal's output.
                strip = strip.child(
                    header_control(
                        gpui::ElementId::Name(format!("overlay-chip-{id}").into()),
                        "Show view",
                        self.theme,
                    )
                    .child(entry.title.clone())
                    .on_click(cx.listener(move |root, _, window, cx| {
                        let _ = root.overlays.show(&id, &owner);
                        if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&id) {
                            f.previous = window.focused(cx);
                            f.handle.focus(window, cx);
                        }
                        cx.notify();
                    })),
                );
            }
        }
        host.child(strip).into_any_element()
    }
    pub(crate) fn observer_minimum(&self) -> [[f32; 2]; 2] {
        let width = self.rem_px(theme::space::TERMINAL_OVERLAY_MIN_WIDTH);
        [
            [
                width,
                self.rem_px(theme::space::TERMINAL_OVERLAY_COMPACT_MIN_HEIGHT),
            ],
            [
                width,
                self.rem_px(theme::space::TERMINAL_OVERLAY_EXPANDED_MIN_HEIGHT),
            ],
        ]
    }
    pub(crate) fn ensure_overlay_focus(&self, id: &str, cx: &mut Context<Self>) {
        self.overlay_focus
            .borrow_mut()
            .entry(id.into())
            .or_insert_with(|| {
                crate::root::overlay::Focus::new(
                    cx.focus_handle(),
                    self.rem_px(theme::space::TERMINAL_OVERLAY_COMPACT_WIDTH),
                    self.rem_px(theme::space::TERMINAL_OVERLAY_COMPACT_HEIGHT),
                    self.rem_px(theme::space::TERMINAL_OVERLAY_OBSERVER_WIDTH),
                    self.rem_px(theme::space::TERMINAL_OVERLAY_OBSERVER_HEIGHT),
                    self.observer_minimum(),
                )
            });
    }
    fn render_overlay_card(
        &self,
        entry: &guild_session::overlay::Overlay,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let id = entry.id.clone();
        let owner = entry.owner.clone();
        let terminal = matches!(entry.content, Content::Terminal { .. });
        let resizable = terminal;
        let mut focuses = self.overlay_focus.borrow_mut();
        let focus = focuses.get_mut(&id).expect("overlay focus");
        if let Content::Terminal { job } = entry.content
            && !focus.terminal_geometry_set
            && focus.viewport.get()[0] > 0.
        {
            let viewport = focus.viewport.get();
            focus.presentation.terminal_geometry(
                self.terminal_geometry.get(&job).and_then(Option::as_ref),
                viewport,
            );
            focus.terminal_geometry_set = true;
        }
        let viewport = focus.viewport.get();
        let bounds = if viewport[0] == 0. {
            let v = self.viewport();
            [f32::from(v.width), f32::from(v.height)]
        } else {
            viewport
        };
        let geometry = focus.presentation.geometry(bounds);
        let handle = focus.handle.clone();

        let escape_id = id.clone();
        let escape_owner = owner.clone();
        let close_id = id.clone();
        let close_owner = owner.clone();
        let move_id = id.clone();
        drop(focuses);
        let body = match &entry.content {
            Content::Terminal { job } => {
                use crate::{i18n::Text, terminal_job::OutputState};
                let state = self.terminal_jobs.output_state(job, cx);
                let explanation = match state {
                    Some(OutputState::Empty) => Some(Text::TerminalOutputEmpty),
                    Some(OutputState::Deleted) => Some(Text::TerminalOutputDeleted),
                    Some(OutputState::Unavailable) | None => Some(Text::TerminalOutputUnavailable),
                    _ => None,
                };
                let mut body = div().flex().flex_col().size_full().min_h_0();
                if let Some(receipt) = self.terminal_jobs.receipt(job)
                    && receipt.state.is_terminal()
                {
                    body = body.child(
                        div()
                            .flex_none()
                            .px(theme::space::MENU_INSET)
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            .child(self.ui().terminal_status(&receipt)),
                    );
                }
                if let Some(text) = explanation {
                    body.child(
                        div()
                            .p(theme::space::CARD_PADDING)
                            .text_color(rgb(theme.text.secondary))
                            .child(self.ui().text(text)),
                    )
                    .into_any_element()
                } else {
                    body.children(
                        self.terminal_jobs
                            .terminal(job)
                            .map(|terminal| div().flex_1().min_h_0().child(terminal)),
                    )
                    .into_any_element()
                }
            }
            Content::Text { text } => div()
                .id("overlay-text")
                .overflow_y_scroll()
                .p(theme::space::PANEL_PADDING)
                .child(text.clone())
                .into_any_element(),
        };
        let mut header = div()
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .h(theme::space::PANEL_HEAD)
            .px(theme::space::CARD_PADDING)
            .font_family(
                self.settings
                    .ui_font
                    .clone()
                    .unwrap_or_else(|| crate::fonts::DEFAULT.ui.into()),
            )
            .text_size(theme::size::BODY)
            .line_height(gpui::relative(theme::leading::TEXT))
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(entry.title.clone()),
            )
            .when(resizable, |header| {
                header.cursor(gpui::CursorStyle::OpenHand).on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |root, event: &gpui::MouseDownEvent, _, cx| {
                        if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&move_id) {
                            f.presentation.gesture = Some(crate::observer::Gesture::Move {
                                pointer: [f32::from(event.position.x), f32::from(event.position.y)],
                                start: f.presentation.geometry(f.viewport.get()),
                            });
                        }
                        cx.notify();
                    }),
                )
            });

        header = header.child(
            header_control(
                "overlay-close",
                if terminal {
                    self.ui().text(crate::i18n::Text::TerminalHideView)
                } else {
                    "Close view; work continues"
                },
                theme,
            )
            .size(theme::space::STATUS_BUTTON)
            .p_0()
            .child(
                gpui_component::Icon::new(gpui_component::IconName::Close)
                    .size(theme::space::STATUS_ICON),
            )
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |root, _, window, cx| {
                root.dismiss_overlay(&close_id, &close_owner, !terminal, window, cx);
                cx.stop_propagation();
            })),
        );
        let root = cx.entity().downgrade();
        let motion_id = id.clone();
        let release_id = id.clone();
        let focus_id = id.clone();
        let events = gpui::canvas(
            move |bounds, _, _| bounds,
            move |bounds, _, window, _| {
                let focus_root = root.clone();
                let focus_id = focus_id.clone();
                window.on_mouse_event(move |event: &gpui::MouseDownEvent, phase, window, cx| {
                    if phase == gpui::DispatchPhase::Capture && bounds.contains(&event.position) {
                        let _ = focus_root.update(cx, |root, cx| {
                            if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&focus_id)
                                && !f.handle.contains_focused(window, cx)
                            {
                                f.previous = window.focused(cx);
                            }
                        });
                    }
                });
                let motion_root = root.clone();
                let id = motion_id.clone();
                window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                    if phase != gpui::DispatchPhase::Capture {
                        return;
                    }
                    let _ = motion_root.update(cx, |root, cx| {
                        if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&id)
                            && f.presentation.gesture.is_some()
                        {
                            f.presentation.motion(
                                [f32::from(event.position.x), f32::from(event.position.y)],
                                event.pressed_button == Some(gpui::MouseButton::Left),
                                f.viewport.get(),
                            );
                            cx.notify();
                        }
                    });
                });
                let root = root.clone();
                let id = release_id.clone();
                window.on_mouse_event(move |event: &gpui::MouseUpEvent, phase, _, cx| {
                    if phase == gpui::DispatchPhase::Capture
                        && event.button == gpui::MouseButton::Left
                    {
                        let _ = root.update(cx, |root, cx| {
                            if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&id)
                                && f.presentation.gesture.is_some()
                            {
                                f.presentation.release(
                                    [f32::from(event.position.x), f32::from(event.position.y)],
                                    f.viewport.get(),
                                );
                                cx.notify();
                            }
                        });
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let mut card = div()
            .id(gpui::ElementId::Name(format!("overlay-card-{id}").into()))
            .absolute()
            .when(resizable, |card| {
                card.left(gpui::px(geometry.x))
                    .top(gpui::px(geometry.y))
                    .w(gpui::px(geometry.width))
                    .h(gpui::px(geometry.height))
            })
            .when(!resizable, |card| {
                card.inset(theme::space::PANEL_PADDING).mx_auto().my_auto()
            })
            .occlude()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.surface.card))
            .border_1()
            .border_color(theme::card_outline(theme))
            .rounded(theme::radius::CARD)
            .track_focus(&handle)
            .on_key_down(
                cx.listener(move |root, event: &gpui::KeyDownEvent, window, cx| {
                    if !terminal && event.keystroke.key == "escape" {
                        root.dismiss_overlay(&escape_id, &escape_owner, false, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(events)
            .child(header)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(body),
            );
        if resizable {
            use crate::observer::ResizeEdge;
            // Reach out from the one-point frame; do not reserve body space or
            // intercept the terminal/header interior. Corners paint last.
            let reach = gpui::px(super::panels::GRIP_REACH);
            let outside = gpui::px(1. - super::panels::GRIP_REACH);
            let frame = gpui::px(1.);
            for (edge, cursor, target) in [
                (
                    ResizeEdge::Left,
                    gpui::CursorStyle::ResizeLeftRight,
                    div()
                        .absolute()
                        .left(outside)
                        .top(frame)
                        .bottom(frame)
                        .w(reach),
                ),
                (
                    ResizeEdge::Right,
                    gpui::CursorStyle::ResizeLeftRight,
                    div()
                        .absolute()
                        .right(outside)
                        .top(frame)
                        .bottom(frame)
                        .w(reach),
                ),
                (
                    ResizeEdge::Top,
                    gpui::CursorStyle::ResizeUpDown,
                    div()
                        .absolute()
                        .top(outside)
                        .left(frame)
                        .right(frame)
                        .h(reach),
                ),
                (
                    ResizeEdge::Bottom,
                    gpui::CursorStyle::ResizeUpDown,
                    div()
                        .absolute()
                        .bottom(outside)
                        .left(frame)
                        .right(frame)
                        .h(reach),
                ),
                (
                    ResizeEdge::TopLeft,
                    gpui::CursorStyle::ResizeUpLeftDownRight,
                    div().absolute().left(outside).top(outside).size(reach),
                ),
                (
                    ResizeEdge::TopRight,
                    gpui::CursorStyle::ResizeUpRightDownLeft,
                    div().absolute().right(outside).top(outside).size(reach),
                ),
                (
                    ResizeEdge::BottomLeft,
                    gpui::CursorStyle::ResizeUpRightDownLeft,
                    div().absolute().left(outside).bottom(outside).size(reach),
                ),
                (
                    ResizeEdge::BottomRight,
                    gpui::CursorStyle::ResizeUpLeftDownRight,
                    div().absolute().right(outside).bottom(outside).size(reach),
                ),
            ] {
                let resize_id = id.clone();
                card = card.child(target.occlude().cursor(cursor).on_mouse_down(
                    gpui::MouseButton::Left,
                    cx.listener(move |root, event: &gpui::MouseDownEvent, _, cx| {
                        if let Some(f) = root.overlay_focus.borrow_mut().get_mut(&resize_id) {
                            f.presentation.gesture = Some(crate::observer::Gesture::Resize {
                                edge,
                                pointer: [f32::from(event.position.x), f32::from(event.position.y)],
                                start: f.presentation.geometry(f.viewport.get()),
                            });
                        }
                        cx.stop_propagation();
                        cx.notify();
                    }),
                ));
            }
        }
        card.into_any_element()
    }
}
