//! Global Network preferences, in the existing Settings row/control system.
use super::{catalog::Setting, choices::Value, sections::setting_button};
use crate::{Root, i18n::Text, network::Outcome, theme};
use gpui::{Context, div, prelude::*, rgb};
use guild_session::network::{Mode, Scope};
impl Root {
    pub(super) fn network_control(
        &self,
        setting: Setting,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let n = &self.settings.network;
        match setting {
            Setting::NetworkMode => {
                self.settings_choice("network-mode", Value::Network(n.mode), cx)
            }
            Setting::NetworkAddress | Setting::NetworkBypass => {
                let address = setting == Setting::NetworkAddress;
                let disabled = address && n.mode == Mode::System;
                let value = if address {
                    if disabled {
                        String::new()
                    } else {
                        n.address.clone()
                    }
                } else {
                    n.bypass.clone()
                };
                let id = if address {
                    "network-address"
                } else {
                    "network-bypass"
                };
                div()
                    .debug_selector(move || id.into())
                    .w(theme::settings::NETWORK_FIELD)
                    .max_w_full()
                    .min_w_0()
                    .h(theme::settings::CONTROL)
                    .rounded(theme::radius::ROW)
                    .border_1()
                    .border_color(theme::settings::outline(self.theme))
                    .bg(rgb(self.theme.surface.inset))
                    .child(
                        crate::text_field::TextField::new(
                            id,
                            value,
                            cx.listener(move |root, value: &String, _, cx| {
                                if address {
                                    root.settings.network.address = value.clone();
                                } else {
                                    root.settings.network.bypass = value.clone();
                                }
                                root.network_check.invalidate();
                                root.store_settings();
                                cx.notify();
                            }),
                        )
                        .placeholder(if disabled {
                            ""
                        } else if address {
                            "http://127.0.0.1:8080"
                        } else {
                            "localhost, 127.0.0.1, .internal.example"
                        })
                        .disabled(disabled)
                        .padded()
                        .vertical_scroll(),
                    )
                    .into_any_element()
            }
            _ => {
                let scope = match setting {
                    Setting::NetworkGuild => Scope::Guild,
                    Setting::NetworkAgent => Scope::Agent,
                    Setting::NetworkTerminal => Scope::Terminal,
                    Setting::NetworkGit => Scope::Git,
                    Setting::NetworkTools => Scope::Tools,
                    _ => unreachable!(),
                };
                crate::view::controls::toggle(
                    self.theme,
                    setting.info().3,
                    n.scopes[scope as usize],
                )
                .on_click(cx.listener(move |root, _, _, cx| {
                    root.settings.network.scopes[scope as usize] =
                        !root.settings.network.scopes[scope as usize];
                    root.store_settings();
                    cx.notify();
                }))
                .into_any_element()
            }
        }
    }
    pub(super) fn network_harnesses(&self) -> Vec<guild_session::HarnessKind> {
        guild_session::HarnessKind::ALL
            .into_iter()
            .filter(|h| {
                super::catalog::matches_query(
                    &self.settings_query,
                    &[
                        h.program(),
                        self.ui().text(Text::NetworkHarnesses),
                        self.ui().text(Text::NetworkHarnessHint),
                        self.ui().text(Text::NetworkUseProxy),
                        self.ui().text(Text::NetworkDirect),
                    ],
                )
            })
            .collect()
    }
    pub(super) fn network_harness_surface(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let ui = self.ui();
        div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .flex_none()
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.tertiary))
                    .child(ui.text(Text::NetworkHarnesses).to_uppercase()),
            )
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.tertiary))
                    .child(ui.text(Text::NetworkHarnessHint)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .rounded(theme::radius::CARD)
                    .bg(theme::settings::card(self.theme))
                    .border_1()
                    .border_color(theme::settings::outline(self.theme))
                    .children(self.network_harnesses().into_iter().enumerate().map(
                        |(index, harness)| {
                            div()
                                .flex()
                                .flex_wrap()
                                .items_center()
                                .gap(theme::space::CARD_INNER_GAP)
                                .p(theme::space::CARD_PADDING)
                                .when(index > 0, |row| {
                                    row.border_t_1()
                                        .border_color(theme::settings::rule(self.theme))
                                })
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(theme::settings::LABEL_MIN)
                                        .text_color(rgb(self.theme.text.primary))
                                        .child(harness.program()),
                                )
                                .child(self.settings_choice(
                                    harness.program(),
                                    Value::HarnessNetwork(
                                        harness,
                                        self.settings.network.agent_route(harness),
                                    ),
                                    cx,
                                ))
                        },
                    )),
            )
            .into_any_element()
    }
    pub(super) fn network_check_surface(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let ui = self.ui();
        let active = self.network_check.active();
        let n = &self.settings.network;
        let invalid = (n.mode != Mode::System).then(|| n.custom().err()).flatten();
        let result = self
            .network_check
            .result
            .as_ref()
            .filter(|(tested, _)| tested == n)
            .map(|(_, result)| match result {
                Outcome::Status(code) => format!(
                    "{} (HTTP {code}). {}",
                    ui.text(if (200..300).contains(code) {
                        Text::NetworkConnected
                    } else if [401, 403, 404, 429].contains(code) {
                        Text::NetworkReachable
                    } else {
                        Text::NetworkEndpointError
                    }),
                    ui.text(Text::NetworkNotUsage)
                ),
                result => ui
                    .text(match result {
                        Outcome::Authentication => Text::NetworkAuthError,
                        Outcome::SystemCredentials => Text::NetworkSystemCredentials,
                        Outcome::SystemTransport => Text::NetworkSystemTransport,
                        Outcome::Timeout => Text::NetworkTimeout,
                        Outcome::Dns => Text::NetworkDnsError,
                        Outcome::Refused => Text::NetworkRefused,
                        Outcome::Tls => Text::NetworkTlsError,
                        Outcome::Unavailable => Text::NetworkUnavailable,
                        Outcome::Invalid(_) => Text::NetworkInvalid,
                        _ => Text::NetworkFailed,
                    })
                    .to_owned(),
            });
        let color = match self
            .network_check
            .result
            .as_ref()
            .filter(|(tested, _)| tested == n)
            .map(|(_, r)| r)
        {
            Some(Outcome::Status(200..=299)) if invalid.is_none() => self.theme.accent.green,
            Some(Outcome::Status(401 | 403 | 404 | 429)) => self.theme.text.secondary,
            Some(_) => self.theme.accent.rust,
            None if invalid.is_some() => self.theme.accent.rust,
            None => self.theme.text.secondary,
        };
        let text = if invalid.is_some() {
            ui.text(Text::NetworkInvalid).to_owned()
        } else {
            result.unwrap_or_else(|| ui.text(Text::NetworkCheckHint).to_owned())
        };
        div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.secondary))
                    .child(ui.text(Text::NetworkTiming)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .gap(theme::space::CARD_INNER_GAP)
                    .child(
                        setting_button(
                            self.theme,
                            "network-check",
                            ui.text(if active {
                                Text::NetworkChecking
                            } else {
                                Text::NetworkCheck
                            })
                            .to_owned(),
                            false,
                        )
                        .debug_selector(|| "network-check".into())
                        .when(active, |b| b.opacity(0.5).cursor_default())
                        .on_click(cx.listener(|root, _, _, cx| {
                            if !root.network_check.active() {
                                root.network_check.start(root.settings.network.clone());
                                cx.notify();
                            }
                        })),
                    )
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(color))
                            .child(text),
                    ),
            )
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.tertiary))
                    .child(format!(
                        "{} → api.anthropic.com",
                        ui.text(
                            match self
                                .network_check
                                .route
                                .filter(|_| self.network_check.matches(n))
                            {
                                Some(true) => Text::NetworkProxyRoute,
                                Some(false) => Text::NetworkDirectRoute,
                                None if n.mode == Mode::System => Text::NetworkSystem,
                                None => Text::NetworkCustom,
                            }
                        )
                    )),
            )
            .into_any_element()
    }
}
