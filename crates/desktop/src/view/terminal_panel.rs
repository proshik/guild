//! Ordinary shells in the shared dock card, with horizontally scrollable tabs.
use crate::view::tooltip::Hint as _;
use crate::{Root, i18n::Text, icons, theme};
use gpui::{Context, MouseButton, div, prelude::*, rgb};

impl Root {
    pub(crate) fn render_terminal_panel_tabs(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let project = self.active_project?;
        let tabs = self.panel_terminals.get(&project)?;
        let theme = self.theme;
        let active = tabs.active().map(|entry| entry.id);
        let ui = self.ui();
        let plus = div()
            .id("new-panel-terminal")
            .flex_none()
            .size(theme::space::STATUS_BUTTON)
            .flex()
            .items_center()
            .justify_center()
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .hover(|s| s.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .hint_delay()
            .tooltip(move |_, cx| {
                cx.new(|_| {
                    crate::view::tooltip::Tooltip::text(ui.text(Text::NewPanelTerminal), theme)
                })
                .into()
            })
            .on_click(
                cx.listener(|root, _, window, cx| root.new_panel_terminal_by_hand(window, cx)),
            )
            .child(
                icons::glyph("ui/head-plus.svg", theme::size::EYEBROW)
                    .text_color(rgb(theme.text.faint)),
            );
        let width = self.panel_terminal_width.clone();
        let scroll = self.panel_terminal_scroll.clone();
        let selected = tabs
            .entries()
            .iter()
            .position(|entry| Some(entry.id) == active);
        let strip = div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .id("panel-terminal-tabs")
                    .flex()
                    .min_w_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.panel_terminal_scroll)
                    .relative()
                    .child(
                        gpui::canvas(
                            move |bounds, window, _| {
                                if width.replace(bounds.size.width) != bounds.size.width {
                                    if let Some(index) = selected {
                                        scroll.scroll_to_item(index + 1);
                                    }
                                    window.refresh();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    )
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .children(tabs.entries().iter().map(|entry| {
                        let id = entry.id;
                        div()
                            .id(("panel-terminal", id))
                            .flex()
                            .flex_none()
                            .items_center()
                            .gap(theme::space::CARD_GAP)
                            .px(theme::space::CARD_GAP)
                            .h(theme::space::STATUS_BUTTON)
                            .rounded(theme::radius::ROW)
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            .when(active == Some(id), |s| {
                                s.bg(rgb(theme.surface.raised))
                                    .text_color(rgb(theme.text.primary))
                            })
                            // The head's buttons' answer to the pointer.
                            .when(active != Some(id), |s| {
                                s.hover(|s| {
                                    s.bg(crate::theme::control_hover(theme, theme.surface.panel))
                                })
                            })
                            .cursor_pointer()
                            .on_click(cx.listener(move |root, _, window, cx| {
                                root.select_panel_terminal(project, id, window, cx)
                            }))
                            .child(format!("{} {id}", ui.text(Text::PanelTerminal)))
                            .child(
                                div()
                                    .id(("close-panel-terminal", id))
                                    .size(theme::space::STATUS_BUTTON)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::radius::ROW)
                                    // Over the tab, which the pointer
                                    // lights too — or the raised one in front.
                                    .hover(move |s| {
                                        s.bg(theme::close_hover(theme, active == Some(id)))
                                    })
                                    .hint_delay()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            crate::view::tooltip::Tooltip::text(
                                                ui.text(Text::ClosePanelTerminal),
                                                theme,
                                            )
                                        })
                                        .into()
                                    })
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        cx.stop_propagation();
                                        root.close_panel_terminal(project, id, window, cx);
                                    }))
                                    .child(
                                        icons::toolbar("icons/close.svg")
                                            .text_color(rgb(theme.text.tertiary)),
                                    ),
                            )
                    })),
            )
            .child(plus);
        Some(strip.into_any_element())
    }

    pub(crate) fn render_terminal_panel(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let project = self.active_project?;
        let tabs = self.panel_terminals.get(&project)?;
        let theme = self.theme;
        let ui = self.ui();
        Some(
            div()
                .flex()
                .flex_col()
                .size_full()
                .min_h_0()
                .child(match tabs.active() {
                    Some(entry) => {
                        let id = entry.id;
                        div()
                            .flex_1()
                            .min_h_0()
                            .min_w_0()
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(
                                    move |root, event: &gpui::MouseDownEvent, window, cx| {
                                        root.note_terminal_menu(
                                            crate::view::menu::TerminalAt::Panel(project, id),
                                            cx,
                                        );
                                        root.open_context_menu(event.position, window, cx);
                                    },
                                ),
                            )
                            .child(entry.value.clone())
                            .into_any_element()
                    }
                    None => div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .justify_center()
                        .text_size(theme::size::UI)
                        .text_color(rgb(theme.text.faint))
                        .child(ui.text(Text::PanelTerminalEmpty))
                        .into_any_element(),
                })
                .into_any_element(),
        )
    }
}
