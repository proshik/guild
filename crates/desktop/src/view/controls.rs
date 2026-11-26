//! Palette-bound interaction states shared by native navigation controls.
use crate::theme;
use gpui::{App, div, prelude::*, rgb};
use gpui_component::button::ButtonCustomVariant;

pub(super) fn ghost(palette: theme::Theme, selected: bool, cx: &App) -> ButtonCustomVariant {
    ButtonCustomVariant::new(cx)
        .foreground(
            rgb(if selected {
                palette.accent.patina
            } else {
                palette.text.primary
            })
            .into(),
        )
        .hover(if selected {
            rgb(palette.surface.raised).into()
        } else {
            crate::theme::control_hover(palette, palette.surface.card).into()
        })
        .active(if selected {
            rgb(palette.surface.raised).into()
        } else {
            crate::theme::control_hover(palette, palette.surface.card).into()
        })
}

/// A settings button: the setting's value, lit when it is the one in
/// force.
pub(crate) fn value(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    value_content(palette, id, label, on)
}

/// A settings button with one composed content group and no implicit label.
pub(crate) fn value_content(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    content: impl IntoElement,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .tab_index(0)
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .min_h(theme::settings::CONTROL)
        .px(theme::space::CARD_GAP)
        .border_1()
        .border_color(rgb(palette.surface.divider))
        .focus(|style| style.border_color(rgb(palette.accent.patina)))
        .rounded(theme::radius::ROW)
        .cursor_pointer()
        .bg(rgb(if on {
            palette.surface.raised
        } else {
            palette.surface.inset
        }))
        .text_color(rgb(if on {
            palette.accent.patina
        } else {
            palette.text.primary
        }))
        .text_size(theme::size::BODY)
        .when(!on, |button| {
            button.hover(|style| {
                style.bg(crate::theme::control_hover(palette, palette.surface.inset))
            })
        })
        .when(!on, |button| {
            button.active(|style| {
                style.bg(crate::theme::control_hover(palette, palette.surface.inset))
            })
        })
        .child(content)
}

/// Component button labels use Guild's body scale rather than the dependency's size preset.
pub(super) trait ButtonText {
    fn ui_label(self, label: impl Into<gpui::SharedString>) -> Self;
}
impl ButtonText for gpui_component::button::Button {
    fn ui_label(self, label: impl Into<gpui::SharedString>) -> Self {
        let label = label.into();
        self.when(!label.is_empty(), |button| {
            button.child(
                div()
                    .text_size(theme::size::BODY)
                    .debug_selector(|| "ui-button-text".into())
                    .child(label),
            )
        })
    }
}

/// Boolean settings share one switch geometry across personal and project pages.
pub(super) fn toggle(
    theme: theme::Theme,
    id: impl Into<gpui::ElementId>,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .tab_index(0)
        .flex()
        .flex_none()
        .items_center()
        .w(theme::settings::SWITCH)
        .h(theme::space::HEAD_BUTTON)
        .p(gpui::px(3.0))
        .rounded_full()
        .border_1()
        .border_color(rgb(theme.surface.divider))
        .focus(|style| style.border_color(rgb(theme.accent.patina)))
        .bg(rgb(if on {
            theme.accent.patina
        } else {
            theme.surface.inset
        }))
        .when(on, |track| track.justify_end())
        .cursor_pointer()
        .child(div().size(theme::size::BODY).rounded_full().bg(rgb(if on {
            theme.surface.app
        } else {
            theme.text.secondary
        })))
}

pub(crate) fn is_tab_navigation(event: &gpui::KeyDownEvent) -> bool {
    let stroke = &event.keystroke;
    stroke.key == "tab"
        && !stroke.modifiers.control
        && !stroke.modifiers.alt
        && !stroke.modifiers.platform
}

/// Native cards restore control traversal inside Guild's terminal-oriented key context.
pub(crate) fn tab_navigation(
    event: &gpui::KeyDownEvent,
    window: &mut gpui::Window,
    cx: &mut gpui::App,
) -> bool {
    if !is_tab_navigation(event) {
        return false;
    }
    let stroke = &event.keystroke;
    if stroke.modifiers.shift {
        window.focus_prev(cx);
    } else {
        window.focus_next(cx);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, FocusHandle, Render, TestAppContext, Window, px};
    use gpui_component::button::Button;

    struct Card {
        focus: FocusHandle,
        activated: Option<usize>,
    }
    impl Render for Card {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div().key_context("guild").child(
                div()
                    .id("card")
                    .track_focus(&self.focus)
                    .on_key_down(|event, window, cx| {
                        if tab_navigation(event, window, cx) {
                            cx.stop_propagation();
                        }
                    })
                    .children((0usize..2).map(|i| {
                        Button::new(("control", i))
                            .ui_label(format!("Control {i}"))
                            .on_click(cx.listener(move |card, _, _, cx| {
                                card.activated = Some(i);
                                cx.notify();
                            }))
                    })),
            )
        }
    }
    #[gpui::test]
    fn card_traversal_survives_the_terminal_tab_unbinding(cx: &mut TestAppContext) {
        let mut card = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let view = cx.new(|cx| Card {
                focus: cx.focus_handle(),
                activated: None,
            });
            view.read(cx).focus.clone().focus(window, cx);
            card = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        cx.simulate_resize(gpui::size(px(400.), px(200.)));
        let card = card.unwrap();
        cx.simulate_keystrokes("tab enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        cx.update(|_, cx| {
            assert_eq!(
                card.read(cx).activated,
                Some(0),
                "Tab then Enter must activate the first card control under Guild's real keymap"
            )
        });
        cx.simulate_keystrokes("tab enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        cx.update(|_, cx| assert_eq!(card.read(cx).activated, Some(1)));
        cx.simulate_keystrokes("shift-tab enter");
        cx.simulate_event(gpui::KeyUpEvent {
            keystroke: gpui::Keystroke::parse("enter").unwrap(),
        });
        cx.update(|_, cx| assert_eq!(card.read(cx).activated, Some(0)));
    }
}

#[cfg(test)]
mod button_text_geometry {
    use super::*;
    use gpui::{Context, Render, TestAppContext, Window, px};
    use gpui_component::{Disableable, Sizable, button::Button};

    struct Sample {
        mode: usize,
        font: f32,
        activated: bool,
    }
    impl Render for Sample {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            window.set_rem_size(crate::scale::rem_size(self.font));
            let button = Button::new("shared-text-sample")
                .small()
                .debug_selector(|| "shared-button".into())
                .accessibility_label("Explicit action name before helper")
                .when(self.mode == 1 || self.mode == 2, |button| {
                    button.icon(gpui_component::IconName::Check)
                })
                .ui_label(if self.mode == 2 || self.mode == 3 {
                    ""
                } else {
                    "Run"
                })
                .when(self.mode == 3 || self.mode == 4, |button| {
                    button.child(
                        div()
                            .debug_selector(|| "custom-content".into())
                            .size(theme::space::STATUS_ICON),
                    )
                })
                .tooltip("Safe action hint")
                .disabled(self.mode == 5)
                .on_click(cx.listener(|sample, _, _, cx| {
                    sample.activated = true;
                    cx.notify();
                }));
            div().flex().flex_col().items_start().child(button).child(
                div()
                    .debug_selector(|| "body-reference".into())
                    .text_size(theme::size::BODY)
                    .child("Run"),
            )
        }
    }
    #[gpui::test]
    fn shared_labels_have_no_phantom_cells_and_keep_body_scale(cx: &mut TestAppContext) {
        cx.executor().allow_parking();
        let mut made = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let sample = cx.new(|_| Sample {
                mode: 0,
                font: 13.,
                activated: false,
            });
            made = Some(sample.clone());
            gpui_component::Root::new(sample, window, cx)
        });
        let sample = made.unwrap();
        cx.simulate_resize(gpui::size(px(500.), px(300.)));
        for font in [13., 20.] {
            for mode in 0..6 {
                sample.update(cx, |sample, cx| {
                    sample.font = font;
                    sample.mode = mode;
                    sample.activated = false;
                    cx.notify();
                });
                cx.run_until_parked();
                let button = cx.debug_bounds("shared-button").unwrap();
                if mode == 5 {
                    cx.simulate_click(button.center(), Default::default());
                    assert!(
                        !sample.read_with(cx, |sample, _| sample.activated),
                        "disabled helper consumer cannot activate"
                    );
                }
                let rem = crate::scale::rem_size(font);
                // Component Small keeps its own 4-point gap and 14-point icon.
                // Native compact-control tokens no longer describe this preset.
                let gap = gpui::rems(4. / crate::scale::BASE).to_pixels(rem);
                let icon = gpui::rems(14. / crate::scale::BASE).to_pixels(rem);
                if mode == 2 {
                    assert!(cx.debug_bounds("ui-button-text").is_none());
                    assert!(
                        (button.size.width - button.size.height).abs() <= px(1.),
                        "empty icon-only label adds no trailing cell: {button:?}"
                    );
                } else if mode == 3 {
                    assert!(cx.debug_bounds("ui-button-text").is_none());
                    let content = cx.debug_bounds("custom-content").unwrap();
                    assert!(
                        (content.center().x - button.center().x).abs() <= px(1.),
                        "empty label must not displace custom content"
                    );
                } else {
                    let text = cx.debug_bounds("ui-button-text").unwrap();
                    let reference = cx.debug_bounds("body-reference").unwrap();
                    assert_eq!(
                        text.size.width, reference.size.width,
                        "helper retains BODY glyph scale independently of button line height"
                    );
                    let left = text.left() - button.left();
                    let right = button.right() - text.right();
                    if mode == 0 || mode == 5 {
                        assert!(
                            (left - right).abs() <= px(1.),
                            "text-only insets: left={left:?}, right={right:?}"
                        );
                    } else if mode == 1 {
                        assert!(
                            (left - right - icon - gap).abs() <= px(1.),
                            "icon+text uses one real gap: left={left:?}, right={right:?}, icon={icon:?}, gap={gap:?}"
                        );
                    } else {
                        let content = cx.debug_bounds("custom-content").unwrap();
                        assert!(
                            (left - (button.right() - content.right())).abs() <= px(1.),
                            "text/custom group centered"
                        );
                        assert!(
                            (content.left() - text.right() - gap).abs() <= px(1.),
                            "custom content retains ordinary inter-child gap"
                        );
                    }
                }
            }
        }
    }
}
