//! The list the server's state in the status bar opens, upwards. M8.5.1.
//!
//! The background tasks, for the project in front: a head per language
//! server with its command and how it is, and under it each task the
//! server is running, with its message and a bar when it says how far.

use gpui::prelude::*;
use gpui::{Context, MouseButton, anchored, deferred, div, px, rgb};

use crate::i18n::Text;
use crate::root::lsp::ServerState;
use crate::{Root, theme};

impl Root {
    /// Open the list, or put it away.
    pub(crate) fn toggle_servers(&mut self, cx: &mut Context<Root>) {
        let opening = !self.showing_servers;
        self.put_fields_away();
        self.showing_servers = opening;
        cx.notify();
    }

    /// The list, when it is up: a layer that closes it, and the card
    /// hanging upwards from the state's right edge.
    pub(crate) fn render_servers(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        if !self.showing_servers {
            return None;
        }
        let theme = self.theme;
        let ui = self.ui();
        let rows = self
            .active_project
            .map(|project| self.servers_of(project))
            .unwrap_or_default();
        let viewport = self.viewport();
        let at = self.server_state_at.get();

        let backdrop = deferred(
            anchored().position(gpui::Point::default()).child(
                div()
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _event, _window, cx| {
                            cx.stop_propagation();
                            root.showing_servers = false;
                            cx.notify();
                        }),
                    ),
            ),
        );

        let line = |text: String| {
            div()
                .flex()
                .items_center()
                .h(theme::space::MENU_ROW)
                .px(theme::space::MENU_INSET)
                .min_w_0()
                .child(div().min_w_0().truncate().child(text))
        };

        let mut card = crate::view::popup::card(theme).w(theme::space::SERVER_LIST);
        if rows.is_empty() {
            card = card.child(
                line(ui.text(Text::ServersNone).to_owned()).text_color(rgb(theme.text.faint)),
            );
        }
        for server in rows {
            let (said, colour) = match &server.state {
                ServerState::IdentityPending => (
                    ui.text(Text::ScopedIdentityPending).to_owned(),
                    theme.text.faint,
                ),
                ServerState::IdentityFailed(error) => (
                    format!("{}: {error}", ui.text(Text::ScopedUnavailable)),
                    theme.accent.rust,
                ),
                ServerState::Disabled => (ui.text(Text::LspDisabled).to_owned(), theme.text.faint),
                ServerState::None => (ui.text(Text::LspNotStarted).to_owned(), theme.text.faint),
                ServerState::Ready => (ui.text(Text::ServerReady).to_owned(), theme.text.faint),
                ServerState::Starting => {
                    (ui.text(Text::ServerStarting).to_owned(), theme.text.faint)
                }
                ServerState::Doing(_) => (String::new(), theme.text.faint),
                ServerState::Failed(fault) => {
                    (ui.server_fault(server.command, fault), theme.accent.rust)
                }
            };
            card = card.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(theme::space::MENU_INSET)
                    .h(px(24.0))
                    .px(theme::space::MENU_INSET)
                    .text_size(theme::size::EYEBROW)
                    .text_color(rgb(theme.text.faint))
                    .child(format!("{} · {}", server.language, server.command))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(colour))
                            .child(said),
                    ),
            );
            // A Swift server not started in an Xcode project: the
            // command that writes the file it needs, on a click.
            if server.language == "swift"
                && let Some(project) = self.active_project
                && let Some(offer) = self.xcode_offer(project)
            {
                card = card.children(self.xcode_offer_rows(project, offer, cx));
            }
            for task in server.tasks {
                let detail = match (&task.message, task.percentage) {
                    (Some(message), _) => Some(message.clone()),
                    (None, Some(percentage)) => Some(format!("{percentage}%")),
                    (None, None) => None,
                };
                card = card.child(
                    // `w_full` on the row and on each line: without it a
                    // truncated line in this column was measured at nothing
                    // and drew as a bare ellipsis in the window.
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .gap(px(4.0))
                        .min_h(theme::space::MENU_ROW)
                        .py(px(4.0))
                        .px(theme::space::MENU_INSET)
                        .child(div().w_full().truncate().child(task.title.clone()))
                        .children(detail.map(|detail| {
                            div()
                                .w_full()
                                .truncate()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.text.faint))
                                .child(detail)
                        }))
                        .children(task.percentage.map(|percentage| {
                            gpui_component::progress::Progress::new("server-progress")
                                .value(percentage as f32)
                                .bg(rgb(theme.accent.patina))
                                .h(theme::space::PROGRESS_HEIGHT)
                        })),
                );
            }
        }

        let list = deferred(
            anchored()
                .anchor(gpui::Anchor::BottomRight)
                .position(gpui::point(at.right(), at.top() - px(4.0)))
                .snap_to_window_with_margin(px(8.0))
                .child(card.occlude()),
        )
        .priority(1);
        Some(div().child(backdrop).child(list).into_any_element())
    }

    /// *Set up for Xcode* under the Swift server's row: the button, the
    /// same row faint while the command runs, and after a failure the
    /// tool's words above the button offered again.
    ///
    /// **A pointer tint and no lit row**: this list has no keyboard, so
    /// the tint is the only highlight there is — traps.md's second
    /// highlight needs a first.
    fn xcode_offer_rows(
        &self,
        project: guild_session::ProjectId,
        offer: crate::root::xcode::Offer,
        cx: &mut Context<Root>,
    ) -> Vec<gpui::AnyElement> {
        use crate::root::xcode::Offer;
        let theme = self.theme;
        let ui = self.ui();
        let running = matches!(offer, Offer::Running);
        let mut rows = Vec::new();
        if let Offer::Failed(why) = &offer {
            rows.push(
                div()
                    .w_full()
                    .py(px(4.0))
                    .px(theme::space::MENU_INSET)
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.accent.rust))
                    .child(ui.xcode_setup_failed(why))
                    .into_any_element(),
            );
        }
        let label = ui.text(if running {
            Text::XcodeSettingUp
        } else {
            Text::XcodeSetUp
        });
        let button = crate::view::popup::row(
            "servers-xcode-setup",
            crate::view::popup::Row {
                label: label.to_owned().into(),
                mark: None,
                icons: false,
                check: None,
                stroke: None,
                loses: false,
                submenu: false,
                lit: false,
                disabled: running,
                trailing: None,
            },
            theme,
        );
        rows.push(if running {
            button.into_any_element()
        } else {
            button
                .hover(|line| line.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.set_up_xcode(project, cx);
                }))
                .into_any_element()
        });
        rows
    }
}
