//! Fixed Workspaces/history and content-anchored branch controls.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, div, px, rgb};
use gpui_kit::base::ElementExt as _;

use crate::Root;
use crate::actions::keymap::Named;
use crate::i18n::Text;
use crate::system;
use crate::{icons, theme};

impl Root {
    fn render_column_button(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let out = !self.settings.column_folded || self.column_peeking;
        div()
            .id("column-button")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .debug_selector(|| "column-button".into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .cursor_pointer()
            .when(out, |b| b.bg(rgb(theme.surface.raised)))
            .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.card)))
            .child(icons::toolbar("ui/rail-column.svg").text_color(rgb(if out {
                theme.text.primary
            } else {
                theme.text.tertiary
            })))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| {
                    crate::view::tooltip::Tooltip::for_action(
                        ui.text(Text::MenuToggleColumn),
                        Named::ToggleSidebar,
                        theme,
                    )
                })
                .into()
            })
            .on_click(cx.listener(|root, _event, _window, cx| {
                cx.stop_propagation();
                if root.column_peeking {
                    root.pin_column(cx);
                } else {
                    root.toggle_column(cx);
                }
            }))
            .into_any_element()
    }

    /// What the column lists, Projects or Sessions — one square button at
    /// the column's right edge, quiet in Projects and pressed in Sessions,
    /// as a lit tool button is. Its icon says the mode it is in.
    fn render_column_mode_button(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let sessions = self.settings.column_mode == guild_session::settings::ColumnMode::Sessions;
        let (icon, tip) = if sessions {
            ("ui/column-sessions.svg", Text::ColumnShowProjects)
        } else {
            ("ui/column-projects.svg", Text::ColumnShowSessions)
        };
        div()
            .id("column-mode")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .debug_selector(|| "column-mode".into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .cursor_pointer()
            .when(sessions, |b| b.bg(rgb(theme.surface.raised)))
            .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.card)))
            .child(
                icons::toolbar(icon)
                    .text_color(rgb(if sessions {
                        theme.text.primary
                    } else {
                        theme.text.tertiary
                    }))
                    .when(sessions, |icon| {
                        icon.debug_selector(|| "column-mode-on".into())
                    }),
            )
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| {
                    crate::view::tooltip::Tooltip::for_action(
                        ui.text(tip),
                        Named::ToggleSessions,
                        theme,
                    )
                })
                .into()
            })
            .on_click(cx.listener(|root, _event, _window, cx| {
                cx.stop_propagation();
                root.toggle_column_mode(cx);
            }))
            .into_any_element()
    }

    pub(crate) fn titlebar_traffic_inset(&self) -> f32 {
        if self.fullscreen {
            self.rem_px(theme::space::WORK_INSET)
        } else {
            self.rem_px(theme::space::TRAFFIC_LIGHTS)
                .max(theme::space::TRAFFIC_LIGHTS_MIN)
        }
    }

    pub(crate) fn titlebar_history_left(&self) -> f32 {
        self.panel_geometry().history_left(
            self.titlebar_traffic_inset()
                + self.rem_px(theme::space::STATUS_BUTTON)
                + self.rem_px(theme::space::CARD_GAP),
        )
    }

    /// Clamp navigation around traffic lights without consuming work-surface width.
    pub(crate) fn render_titlebar(
        &mut self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let traffic = self.titlebar_traffic_inset();
        let gap = self.rem_px(theme::space::CARD_GAP);
        let column_shown = !self.settings.column_folded || self.column_peeking;
        // The mode button stands in the column's corner: its right edge one
        // gap between areas short of the column's edge, where the work's
        // card begins. Over a folded column brought out by the pointer the
        // edge is the overlay's, which has no width in the geometry.
        let between = self.rem_px(theme::space::WORK_BETWEEN);
        let peeking = self.settings.column_folded && self.column_peeking;
        let column_edge = if peeking {
            crate::column::width(self.settings.column_width, self.settings.ui_font_size)
        } else {
            self.panel_geometry().column
        };
        let history_end = self.titlebar_history_left()
            + 2.0 * self.rem_px(theme::space::STATUS_BUTTON)
            + self.rem_px(theme::space::STATUS_BUTTON_GAP)
            + gap;
        // The column's floor holds the mode button at its edge
        // (`column::narrowest`), so the branch has nothing more to clear —
        // except the peeking overlay, which stands over the work.
        let fixed_end = if peeking {
            history_end.max(column_edge)
        } else {
            history_end
        };
        let scale = self.settings.ui_font_size / crate::scale::BASE_UI_FONT_SIZE;
        // Branch labels reserve the fixed controls and the right action bar.
        let anchor = self.panel_geometry().content_left(fixed_end);
        let release_width = if self.update.staged().is_some() {
            self.footer_text_width(
                self.ui().text(Text::UpdateButton),
                theme::size::META,
                window,
            ) + 2.0 * self.rem_px(theme::space::PILL_PADDING)
                + 2.0
                + gap
        } else {
            0.0
        };
        let labels = ((f32::from(self.viewport.width) - anchor - 300.0 * scale - release_width)
            / 2.0)
            .max(0.0)
            .min(self.rem_px(theme::space::MENU_MIN));
        div()
            .id("titlebar")
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .h(theme::space::TITLEBAR)
            // A point below the bar's centre line — the person's optical
            // centring, section 6.9: exactly centred read as high.
            .pt(theme::space::TITLEBAR_OFFSET)
            // No hairline under the window's head: the work's card below
            // is the separation — section 4.1, M12.6.
            // AppKit must not claim control presses as native titlebar drags.
            // Interactive children stop presses before this handler; once a
            // background press starts a move, the OS owns the entire gesture.
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|root, event: &gpui::MouseDownEvent, window, _cx| {
                    if event.click_count != 1 {
                        return;
                    }
                    #[cfg(test)]
                    if root.isolated_render {
                        root.titlebar_drag_starts += 1;
                        return;
                    }
                    #[cfg(not(test))]
                    let _ = root;
                    window.start_window_move();
                }),
            )
            // Double-clicking a title bar is a system gesture, and a window
            // with the system's own title bar gets it for nothing. Guild
            // draws its own, so it has to ask what the gesture means — the
            // person chooses that in System Settings, and the default is
            // Zoom rather than full screen. Full screen is ctrl-cmd-f, and
            // making both do the same thing would waste one of them.
            .on_click(cx.listener(|root, event: &gpui::ClickEvent, window, _cx| {
                if event.click_count() < 2 {
                    return;
                }
                // Isolated fixtures observe delivery before any macOS preference
                // or unsupported TestWindow zoom/minimize can affect the result.
                #[cfg(test)]
                if root.isolated_render {
                    root.titlebar_double_clicks += 1;
                    return;
                }
                #[cfg(not(test))]
                let _ = root;
                match system::double_click_action() {
                    system::DoubleClick::Zoom => window.zoom_window(),
                    system::DoubleClick::Minimize => window.minimize_window(),
                    system::DoubleClick::Nothing => {}
                }
            }))
            // The inset exists for the traffic lights. In full screen
            // there are none, and holding 82 pixels for buttons that are
            // not there is a hole in the corner of the window.
            .pl(px(traffic))
            .pr(theme::space::CARD_GAP)
            .bg(rgb(theme.surface.panel))
            // History stays beside Workspaces. The minimum only adds trailing
            // air before the content-boundary branch controls.
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    // To one card gap short of the column's edge, which the
                    // branch's gap after it reaches.
                    .min_w(px((column_edge - traffic - gap).max(0.0)))
                    .child(self.render_column_button(cx))
                    .child(self.render_history_buttons(cx))
                    // In the column's corner, against the work's card:
                    // hidden with the column.
                    .when(column_shown, |row| {
                        row.child(
                            div()
                                .flex_none()
                                .ml_auto()
                                // The row ends a card gap short of the edge;
                                // the corner is the smaller gap between areas.
                                .mr(px(between - gap))
                                .child(self.render_column_mode_button(cx)),
                        )
                    }),
            )
            .when(self.branch.is_some(), |bar| {
                bar.child(self.render_current_branch(labels, cx))
                    .child(self.render_branch_update(cx))
            })
            // **Update, while a nightly is offered** — M5.16, a pill
            // right after the way back and
            // forth, a word rather than an icon, because it appears and
            // then it is gone and an icon nobody has seen before says
            // nothing.
            .when_some(self.update.staged().cloned(), |bar, build| {
                bar.child(self.render_update_pill(build, cx))
                    .child(self.notes_overlay(cx))
            })
            .child(div().flex_1())
            // **The three layout buttons are gone** — M12 task 12. They
            // said "show this side" and named no tool, while the status
            // bar's own buttons open a side *and* say what will be in it,
            // and every panel now carries the button that puts it away in
            // its own heading. Three controls for one gesture is two too
            // many, and the one that named nothing is the one to go.
            //
            // **The plan's limits** — M12.10: a figure, and a disc only
            // when it matters; shared spacing with the adjacent controls.
            // Open In and job recovery share the outlined titlebar controls.
            // Settings remains available through the native menu and Cmd+,.
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .children(
                        guild_session::action_bar::visible(
                            &self.settings.action_bar_order,
                            &self.settings.action_bar_excluded,
                        )
                        .into_iter()
                        .map(|item| match item {
                            guild_session::action_bar::Item::Usage => self.render_usage_chip(cx),
                            guild_session::action_bar::Item::OpenIn => {
                                self.render_open_in_trigger(cx)
                            }
                            guild_session::action_bar::Item::TerminalJobs => {
                                self.render_terminal_job_trigger(cx)
                            }
                        }),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_open_in_trigger(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        let at = self.open_in_at.clone();
        let open = matches!(
            self.context_menu.as_ref().map(|menu| &menu.target),
            Some(crate::view::menu::Target::OpenIn { .. })
        );
        use guild_session::settings::OpenInTarget;
        let selected = self.selected_open_in_target();
        let icon_id = match &selected {
            OpenInTarget::Finder => Some("com.apple.finder"),
            OpenInTarget::App(id) => Some(id.as_str()),
            OpenInTarget::Terminal => None,
        };
        let enabled = self.active_project.is_some();
        // **With no project the control says it is resting**, as the
        // history arrows beside it do: faint glyphs, and the app's own
        // picture at the start screen's faint-mark opacity. Only the hover
        // and the cursor went before, and it looked live and did nothing.
        let ink = |live: u32| rgb(if enabled { live } else { theme.text.faint });
        let icon = icon_id
            .and_then(|id| self.open_in_icons.borrow().get(id).cloned())
            .map(|image| {
                gpui::img(image.clone())
                    .size(theme::space::ICON)
                    .when(!enabled, |image| image.opacity(crate::view::kind::FAINT))
                    .into_any_element()
            })
            .unwrap_or_else(|| {
                crate::icons::toolbar("ui/menu-reveal.svg")
                    .text_color(ink(theme.text.secondary))
                    .into_any_element()
            });
        let button = |id: &'static str| {
            div()
                .id(id)
                .flex()
                .flex_1()
                .h_full()
                .rounded(theme::radius::STATUS_BUTTON)
                .items_center()
                .justify_center()
                .when(enabled, |button| {
                    button
                        .cursor_pointer()
                        .hover(|s| s.bg(crate::theme::control_hover(theme, theme.surface.card)))
                })
        };
        div()
            .id("titlebar-open-in")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .debug_selector(|| "titlebar-open-in".into())
            .flex()
            .flex_none()
            .items_center()
            .w(theme::space::STATUS_BUTTON * 2.0)
            .h(theme::space::STATUS_BUTTON)
            .rounded_full()
            .overflow_hidden()
            .border_1()
            .border_color(theme::settings::outline(theme))
            .bg(rgb(theme.surface.card))
            .when(open, |button| button.bg(rgb(theme.surface.raised)))
            .child(
                button("titlebar-open-selected")
                    // External opening must keep the current body input owner.
                    .on_mouse_down(gpui::MouseButton::Left, |_, window, _| {
                        window.prevent_default()
                    })
                    .debug_selector(|| "titlebar-open-selected".into())
                    .child(icon)
                    .when(enabled && !open, |button| {
                        button.hint_delay().tooltip(move |_, app| {
                            app.new(|_| {
                                crate::view::tooltip::Tooltip::for_action(
                                    ui.text(Text::OpenSelectedApp),
                                    Named::OpenSelectedApp,
                                    theme,
                                )
                            })
                            .into()
                        })
                    })
                    .on_click(cx.listener(move |root, _, _, cx| {
                        cx.stop_propagation();
                        if enabled {
                            root.open_selected_app(cx);
                        }
                    })),
            )
            .child(
                button("titlebar-open-selector")
                    .debug_selector(|| "titlebar-open-selector".into())
                    .border_l_1()
                    .border_color(theme::settings::outline(theme))
                    .child(
                        crate::icons::toolbar("ui/chevron-down.svg")
                            .text_color(ink(theme.text.tertiary)),
                    )
                    .when(enabled && !open, |button| {
                        button.hint_delay().tooltip(move |_, app| {
                            app.new(|_| {
                                crate::view::tooltip::Tooltip::for_action(
                                    ui.text(Text::OpenInButton),
                                    Named::SelectOpenApp,
                                    theme,
                                )
                            })
                            .into()
                        })
                    })
                    .on_click(cx.listener(move |root, _, window, cx| {
                        cx.stop_propagation();
                        if enabled {
                            root.open_open_in(false, window, cx);
                        }
                    })),
            )
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| {
                        at.set(Some(bounds));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
    }

    /// The shared titlebar control: card/divider, raised on hover, with
    /// legible brass (or primary) text and the existing release-notes card.
    fn render_update_pill(
        &self,
        _build: crate::update::Build,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let measured = bounds.clone();
        let overlay = self.notes_overlay(cx);

        gpui_kit::base::Button::new("titlebar-update")
            .accessibility_label(ui.text(Text::UpdateButton))
            .track_focus(&self.notes_trigger_focus(cx))
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .flex()
            .flex_none()
            .items_center()
            .h(theme::space::STATUS_BUTTON)
            .px(theme::space::PILL_PADDING)
            .border_1()
            .border_color(theme::settings::outline(theme))
            .rounded_full()
            .cursor_pointer()
            .bg(rgb(theme.surface.card))
            .text_size(theme::size::META)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(rgb(theme::update_ink(&theme)))
            .focus(|style| style.border_color(rgb(theme.accent.patina)))
            .hover(move |style| style.bg(crate::theme::control_hover(theme, theme.surface.card)))
            .child(ui.text(Text::UpdateButton))
            .debug_selector(|| "titlebar-update".into())
            .on_prepaint(move |value, _, _| measured.set(value))
            .on_hover(cx.listener(move |root, hovered, window, cx| {
                if *hovered {
                    root.notes_trigger(bounds.get(), window, cx);
                } else {
                    overlay.update(cx, |overlay, cx| overlay.request_hide(window, cx));
                }
            }))
            // A title bar's own click is the zoom gesture.
            .on_click(cx.listener(|root, _event, window, cx| {
                cx.stop_propagation();
                root.apply_update(window, cx);
            }))
            .into_any_element()
    }

    /// Back and Forward, beside the workspace toggle — M8.10's two buttons, in the
    /// title bar since 2026-09-24.
    ///
    /// **Sized as the other titlebar controls**, not as the
    /// strip's buttons they used to be: the row is the title bar's, and
    /// two measures in one row is a row that does not sit even.
    ///
    /// **Faint where there is nowhere to go, and still pressable**: the
    /// deed says nothing when the stack is empty (M8.8.2), so a greyed
    /// arrow and a press that does nothing say the same thing, and the
    /// faint one says it before the press.
    fn render_history_buttons(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let (behind, ahead) = self.history_directions();

        let arrow = |id: &'static str,
                     icon: &'static str,
                     live: bool,
                     label: Text,
                     action: crate::actions::keymap::Named| {
            div()
                .id(id)
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .debug_selector(move || id.into())
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(theme::space::STATUS_BUTTON)
                .rounded(theme::radius::STATUS_BUTTON)
                .cursor_pointer()
                .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.card)))
                .child(crate::icons::toolbar(icon).text_color(rgb(if live {
                    theme.text.secondary
                } else {
                    theme.text.faint
                })))
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_cx| {
                        crate::view::tooltip::Tooltip::for_action(ui.text(label), action, theme)
                    })
                    .into()
                })
        };

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .child(
                arrow(
                    "go-back",
                    "ui/arrow-left.svg",
                    behind,
                    Text::MenuBack,
                    crate::actions::keymap::Named::Back,
                )
                // A title bar's own click is the zoom gesture; this press
                // is the button's alone.
                .on_click(cx.listener(|root, _event, window, cx| {
                    cx.stop_propagation();
                    root.go_back(window, cx);
                })),
            )
            .child(
                arrow(
                    "go-forward",
                    "ui/arrow-right.svg",
                    ahead,
                    Text::MenuForward,
                    crate::actions::keymap::Named::Forward,
                )
                .on_click(cx.listener(|root, _event, window, cx| {
                    cx.stop_propagation();
                    root.go_forward(window, cx);
                })),
            )
            .into_any_element()
    }
}
