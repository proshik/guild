//! What every window beside the main one is inside — M8.3.6, decision 1:
//! the keys that close it, the focus that lets it hear them, and its
//! ground.

use gpui::prelude::*;
use gpui::{App, Div, FocusHandle, Window, div, rgb};
use gpui_component::ActiveTheme as _;

use crate::actions::CloseTab;
use crate::theme;

/// The satellite's outermost element. `cmd-w` is `CloseTab` in every
/// window, and a satellite answers it by closing itself; Escape is read
/// here, since nothing binds it outside a panel.
///
/// The view focuses `focus` itself on its first render: a window nothing
/// focuses hears no keys at all — the main window's trap, met again in
/// each new one.
pub(crate) fn frame(
    focus: &FocusHandle,
    theme: theme::Theme,
    window: &mut Window,
    cx: &App,
) -> Div {
    // The component root sets its rem to the raw interface font size.
    // Guild's tokens are based on a 16px rem at a 13px UI size, just as
    // in Root::render. Restore that scale before building any children.
    window.set_rem_size(crate::scale::rem_size(f32::from(cx.theme().font_size)));
    div()
        .track_focus(focus)
        .key_context("satellite")
        .size_full()
        .flex()
        .flex_col()
        .bg(rgb(theme.surface.panel))
        .text_color(rgb(theme.text.primary))
        .text_size(theme::size::UI)
        .font_family(cx.theme().font_family.clone())
        .on_action(|_: &CloseTab, window, _cx| window.remove_window())
        .on_key_down(|event: &gpui::KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" && !event.keystroke.modifiers.modified() {
                cx.stop_propagation();
                window.remove_window();
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Context, Entity, Render, SharedString, TestAppContext, Window, px};

    struct Probe {
        size: gpui::Pixels,
        family: SharedString,
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            let style = window.text_style();
            self.size = style.font_size.to_pixels(window.rem_size());
            self.family = style.font_family;
            div().child("Interface text")
        }
    }

    struct Frame {
        focus: FocusHandle,
        probe: Entity<Probe>,
        popup_probe: Entity<Probe>,
    }

    impl Render for Frame {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            frame(&self.focus, theme::Theme::of(theme::Kind::Dark), window, cx)
                .child(div().text_size(theme::size::BODY).child(self.probe.clone()))
                .child(gpui::deferred(
                    gpui::anchored()
                        .position(gpui::point(px(20.0), px(80.0)))
                        .child(
                            crate::view::popup::card(theme::Theme::of(theme::Kind::Dark))
                                .child(self.popup_probe.clone()),
                        ),
                ))
        }
    }

    /// The component root sets its own rem before our view. A satellite
    /// must restore Guild's scale, including when the setting changes
    /// while it is open, and use the same UI family as the main window.
    #[gpui::test]
    fn interface_text_matches_the_setting_under_the_component_root(cx: &mut TestAppContext) {
        let mut probe = None;
        let mut popup_probe = None;
        let (_view, cx) = cx.add_window_view(|window, cx| {
            gpui_component::init(cx);
            gpui_component::Theme::global_mut(cx).font_size = px(13.0);
            gpui_component::Theme::global_mut(cx).font_family = "monospace".into();
            let text = cx.new(|_cx| Probe {
                size: px(0.0),
                family: "".into(),
            });
            probe = Some(text.clone());
            let popup_text = cx.new(|_cx| Probe {
                size: px(0.0),
                family: "".into(),
            });
            popup_probe = Some(popup_text.clone());
            let view = cx.new(|cx| Frame {
                focus: cx.focus_handle(),
                probe: text,
                popup_probe: popup_text,
            });
            gpui_component::Root::new(view, window, cx)
        });
        let probe = probe.unwrap();
        let popup_probe = popup_probe.unwrap();
        for size in [13.0, 16.0, 20.0, 10.0] {
            cx.update(|window, cx| {
                gpui_component::Theme::global_mut(cx).font_size = px(size);
                window.refresh();
            });
            cx.update(|_, cx| {
                assert_eq!(
                    probe.read(cx).size,
                    px(size),
                    "the body text is the chosen interface size"
                );
                assert_eq!(probe.read(cx).family.as_ref(), "monospace");
                assert_eq!(
                    popup_probe.read(cx).size,
                    px(size),
                    "a deferred popup keeps the same scale"
                );
                assert_eq!(popup_probe.read(cx).family.as_ref(), "monospace");
            });
        }
    }
}
