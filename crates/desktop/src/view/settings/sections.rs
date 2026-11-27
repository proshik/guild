//! Grouped preference rows, retaining the existing setters and timing semantics.
use super::{Section, catalog::Setting, choices::Value};
use crate::i18n::Text;
use crate::{Root, paths, short_path, theme};
use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

impl Root {
    fn render_tab_launcher_order(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_GAP)
            .children(
                self.settings
                    .tab_launcher_order()
                    .into_iter()
                    .enumerate()
                    .map(|(index, tab)| {
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::space::CARD_GAP)
                            .child(format!(
                                "{}  {}",
                                index + 1,
                                crate::view::kind::default_tab_label(self.ui(), tab)
                            ))
                            .children([false, true].into_iter().map(|down| {
                                let enabled = if down { index < 3 } else { index > 0 };
                                gpui_kit::base::Button::new(gpui::SharedString::from(format!(
                                    "tab-order-{index}-{down}"
                                )))
                                .debug_selector(move || format!("tab-order-{index}-{down}"))
                                .size(theme::space::STATUS_BUTTON)
                                .p_0()
                                .rounded(theme::radius::ROW)
                                .text_color(rgb(if enabled {
                                    theme.text.secondary
                                } else {
                                    theme.text.tertiary
                                }))
                                .accessibility_label(format!(
                                    "{} {}",
                                    crate::view::kind::default_tab_label(self.ui(), tab),
                                    if down { "↓" } else { "↑" }
                                ))
                                .child(if down { "↓" } else { "↑" })
                                .on_click(cx.listener(
                                    move |root, _, _, cx| {
                                        if enabled {
                                            let mut order = root.settings.tab_launcher_order();
                                            order.swap(
                                                index,
                                                if down { index + 1 } else { index - 1 },
                                            );
                                            root.settings.tab_launcher_order = order;
                                            root.store_settings();
                                            cx.notify();
                                        }
                                    },
                                ))
                            }))
                    }),
            )
            .into_any_element()
    }

    pub(crate) fn setting_hint(&self, setting: Setting) -> String {
        let text = match setting {
            Setting::NetworkMode => Text::NetworkModeHint,
            Setting::NetworkAddress => Text::NetworkAddressHint,
            Setting::NetworkBypass => Text::NetworkBypassHint,
            Setting::NetworkGuild => Text::NetworkGuildHint,
            Setting::NetworkAgent => Text::NetworkAgentHint,
            Setting::NetworkTerminal => Text::NetworkTerminalHint,
            Setting::NetworkGit => Text::NetworkGitHint,
            Setting::NetworkTools => Text::NetworkToolsHint,
            Setting::UpdateSource => Text::UpdaterSourceHint,
            Setting::UpdateChannel => Text::UpdaterChannelHint,
            Setting::UpdateDirectory => Text::UpdaterDirectoryHint,
            Setting::UpdateStatus => Text::UpdaterStatusHint,
            Setting::ActionBarOrder => Text::ActionBarHint,
            Setting::OpenInApps => Text::OpenInAppsHint,
            Setting::Language => Text::LanguageHint,
            Setting::Theme => Text::ThemeHint,
            Setting::CustomizeTheme => Text::CustomizeThemeHint,
            Setting::UiFont => Text::UiFontTitle,
            Setting::UiSize => Text::UiFontSizeHint,
            Setting::CodeFont => Text::CodeFontHint,
            Setting::TerminalFont => Text::TerminalFontHint,
            Setting::TerminalSize => Text::FontSizeHint,
            Setting::DevelopmentWorkflows => Text::DevelopmentWorkflowsHint,
            Setting::Copy => Text::CopyOnSelectHint,
            #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
            Setting::AdaptiveRefresh => Text::AdaptiveRefreshHint,
            Setting::DefaultTab => Text::DefaultTabHint,
            Setting::TabLauncherOrder => Text::TabLauncherOrderHint,
            Setting::SettleAfter => Text::SettleAfterHint,
            Setting::Permissions => Text::PermissionsHint,
            Setting::CodexSandbox => Text::CodexSandboxHint,
            Setting::CodexCommandApprovals => Text::CodexCommandApprovalsHint,
            Setting::OpenCodePermissions => Text::OpenCodePermissionsHint,
            Setting::ClaudePrefix | Setting::CodexPrefix | Setting::OpenCodePrefix => {
                return self.ui().text(Text::LaunchPrefixHint).to_owned();
            }
            Setting::Reopen => Text::ReopenLastHint,
            Setting::AutoFastForward => Text::AutoFastForwardHint,
            Setting::GitLabMergeRequests => Text::GitLabMrHint,
            Setting::CloseProject => Text::ConfirmCloseHint,
            Setting::CloseAgent => Text::ConfirmCloseAgentHint,
            Setting::Forget => Text::ForgetOnCloseHint,
            Setting::Autosave => Text::AutosaveHint,
            Setting::MarkdownDefaultView => Text::MarkdownDefaultViewHint,
            Setting::Diff => Text::DiffViewHint,
            Setting::SettingsFile => return short_path(&paths::settings_file()),
            Setting::Log => return short_path(&crate::logging::file()),
            Setting::KeymapFile => return short_path(&paths::keymap_file()),
        };
        self.ui().text(text).to_owned()
    }
    pub(crate) fn visible_settings(&self, chosen: Section) -> Vec<Setting> {
        let query = self.settings_query.trim();
        Setting::ALL
            .iter()
            .copied()
            .filter(|row| {
                (query.is_empty() && row.info().0 == chosen)
                    || (!query.is_empty()
                        && row.matches(query, &self.setting_hint(*row), self.ui()))
            })
            .collect()
    }
    pub(crate) fn section_rows(
        &self,
        chosen: Section,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let theme = self.theme;
        let mut groups: Vec<(Section, Text, Vec<gpui::AnyElement>)> = Vec::new();
        for setting in self.visible_settings(chosen) {
            let (section, group, title, key) = setting.info();
            if groups
                .last()
                .is_none_or(|(s, g, _)| *s != section || *g != group)
            {
                groups.push((section, group, Vec::new()));
            }
            if section == Section::Updates {
                // Search keeps every update key, but all matching rows share one card.
                let rows = &mut groups.last_mut().unwrap().2;
                if rows.is_empty() {
                    rows.push(self.update_status_control(cx));
                }
                continue;
            }
            let control = self.setting_control(setting, cx);
            let rows = &mut groups.last_mut().unwrap().2;
            let theme_row = matches!(
                setting,
                Setting::UpdateStatus
                    | Setting::UpdateDirectory
                    | Setting::ActionBarOrder
                    | Setting::OpenInApps
            );
            rows.push(
                div()
                    .id(key)
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::CARD_INNER_GAP)
                    .p(theme::space::CARD_PADDING)
                    .when(!rows.is_empty(), |row| {
                        row.border_t_1().border_color(theme::settings::rule(theme))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w(theme::settings::LABEL_MIN)
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .text_color(rgb(theme.text.primary))
                                    .child(self.ui().text(title)),
                            )
                            .child(
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.tertiary))
                                    .child(self.setting_hint(setting)),
                            ),
                    )
                    .when(setting == Setting::OpenInApps, |row| {
                        row.justify_end().child(self.open_in_add_button(cx))
                    })
                    .child(div().when(theme_row, |slot| slot.w_full()).child(control))
                    .into_any_element(),
            );
            if setting == Setting::CustomizeTheme
                && self.customizer_drawn()
                && let Some(card) = self.render_customizer(cx)
            {
                rows.push(card);
            }
        }
        let mut result: Vec<_> = groups
            .into_iter()
            .map(|(section, title, rows)| {
                let heading = if self.settings_query.trim().is_empty() {
                    self.ui().text(title).to_owned()
                } else {
                    format!(
                        "{} / {}",
                        self.ui().text(section.title()),
                        self.ui().text(title)
                    )
                };
                let positions = self.settings_group_positions.clone();
                let scroll = self.settings_scroll.clone();
                div()
                    .relative()
                    .child(
                        gpui::canvas(
                            move |bounds, _, _| {
                                let y =
                                    bounds.origin.y - scroll.bounds().origin.y - scroll.offset().y;
                                let mut positions = positions.borrow_mut();
                                if let Some((_, at)) =
                                    positions.iter_mut().find(|(group, _)| *group == title)
                                {
                                    *at = y;
                                } else {
                                    positions.push((title, y));
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    )
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .flex_none()
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.tertiary))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(heading.to_uppercase()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .rounded(theme::radius::CARD)
                            .bg(theme::settings::card(theme))
                            .border_1()
                            .border_color(theme::settings::outline(theme))
                            .children(rows)
                            .when(title == Text::SettingsTypography, |card| {
                                let terminal = section == Section::Terminal;
                                let family = if terminal { self.settings.terminal_font.clone().unwrap_or_else(|| crate::fonts::Which::Terminal.default_face().to_owned()) }
                                    else { self.settings.code_font.clone().unwrap_or_else(|| crate::fonts::Which::Code.default_face().to_owned()) };
                                card.child(div().p(theme::space::CARD_PADDING).border_t_1().border_color(theme::settings::rule(theme))
                                    .child(div().text_size(theme::size::META).text_color(rgb(theme.text.tertiary)).child(self.ui().text(Text::SettingsPreview)))
                                    .child(div().id(if terminal { "terminal-font-preview" } else { "code-font-preview" })
                                        .debug_selector(move || if terminal { "terminal-font-preview".into() } else { "code-font-preview".into() })
                                        .mt(theme::space::CARD_GAP).p(theme::space::CARD_PADDING).overflow_x_scroll()
                                        .rounded(theme::radius::ROW).bg(rgb(theme.surface.sunken)).font_family(family)
                                        .text_size(theme::size::BODY)
                                        .when(terminal, |preview| preview
                                            .text_size(px(self.settings.terminal_font_size))
                                            .line_height(px(self.settings.terminal_font_size * theme::leading::TERMINAL)))
                                        .text_color(rgb(theme.text.primary))
                                        .child(if terminal { "$ git status\nOn branch main\nWorking tree clean" } else { "fn main() {\n    println!(\"Hello, Guild\");\n}" })))
                            }),
                    )
                    .into_any_element()
            })
            .collect();
        if (chosen == Section::Network || !self.settings_query.trim().is_empty())
            && !self.network_harnesses().is_empty()
        {
            result.push(self.network_harness_surface(cx));
        }
        if chosen == Section::Network {
            result.push(self.network_check_surface(cx));
        }
        if chosen == Section::Keymap || !self.settings_query.trim().is_empty() {
            let rows = self.keymap_rows(cx);
            if !rows.is_empty() || chosen == Section::Keymap {
                result.insert(
                    0,
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::space::CARD_GAP)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .gap(theme::space::CARD_GAP)
                                .child(self.ui().text(Text::SettingsShortcuts))
                                .child(
                                    setting_button(
                                        theme,
                                        "settings-modified",
                                        self.ui().text(Text::SettingsModified).to_owned(),
                                        self.settings_modified,
                                    )
                                    .on_click(cx.listener(
                                        |root, _, _, cx| {
                                            root.settings_modified = !root.settings_modified;
                                            cx.notify();
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .bg(theme::settings::card(theme))
                                .rounded(theme::radius::CARD)
                                .children(rows),
                        )
                        .into_any_element(),
                );
            }
        }
        result
    }
    fn setting_control(&self, setting: Setting, cx: &mut Context<Self>) -> gpui::AnyElement {
        let settings = &self.settings;
        let theme = self.theme;
        match setting {
            Setting::NetworkMode
            | Setting::NetworkAddress
            | Setting::NetworkBypass
            | Setting::NetworkGuild
            | Setting::NetworkAgent
            | Setting::NetworkTerminal
            | Setting::NetworkGit
            | Setting::NetworkTools => self.network_control(setting, cx),
            Setting::UpdateSource => self.settings_choice(
                "setting-update-source",
                Value::UpdateSource(self.update_preferences().source),
                cx,
            ),
            Setting::UpdateChannel => self.settings_choice(
                "setting-update-channel",
                Value::UpdateChannel(self.update_preferences().channel),
                cx,
            ),
            Setting::UpdateDirectory => self.update_directory_control(cx),
            Setting::UpdateStatus => self.update_status_control(cx),
            Setting::ActionBarOrder => self.render_action_bar_order(cx),
            Setting::OpenInApps => self.render_open_in_apps_settings(cx),
            Setting::Language => {
                self.settings_choice("setting-language", Value::Language(settings.language), cx)
            }
            Setting::TabLauncherOrder => self.render_tab_launcher_order(cx),
            Setting::DefaultTab => {
                self.settings_choice("setting-defaulttab", Value::Tab(self.default_tab()), cx)
            }
            Setting::ClaudePrefix | Setting::CodexPrefix | Setting::OpenCodePrefix => {
                let (harness, id) = match setting {
                    Setting::ClaudePrefix => (
                        guild_session::HarnessKind::Claude,
                        "claude-launch-prefix-input",
                    ),
                    Setting::CodexPrefix => (
                        guild_session::HarnessKind::Codex,
                        "codex-launch-prefix-input",
                    ),
                    _ => (
                        guild_session::HarnessKind::OpenCode,
                        "opencode-launch-prefix-input",
                    ),
                };
                div()
                    .w(theme::settings::LABEL_MIN)
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .children([false, true].map(|arguments| {
                        let field_id = format!("{id}{}", if arguments { "-arguments" } else { "" });
                        div()
                            .flex()
                            .flex_col()
                            .gap(theme::space::CARD_GAP)
                            .child(
                                div()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.tertiary))
                                    .child(self.ui().text(if arguments {
                                        Text::LaunchArgumentsLabel
                                    } else {
                                        Text::LaunchPrefixLabel
                                    })),
                            )
                            .child(
                                div()
                                    .debug_selector(move || field_id.clone())
                                    .min_w_0()
                                    .h(theme::settings::CONTROL)
                                    .rounded(theme::radius::ROW)
                                    .border_1()
                                    .border_color(theme::settings::outline(theme))
                                    .bg(rgb(theme.surface.inset))
                                    .child(
                                        crate::text_field::TextField::new(
                                            format!(
                                                "{id}{}",
                                                if arguments { "-arguments" } else { "" }
                                            ),
                                            if arguments {
                                                settings.launch_arguments.get(harness)
                                            } else {
                                                settings.launch_prefixes.get(harness)
                                            },
                                            cx.listener(move |root, value: &String, _, cx| {
                                                if arguments {
                                                    root.settings
                                                        .launch_arguments
                                                        .set(harness, value.clone());
                                                } else {
                                                    root.settings
                                                        .launch_prefixes
                                                        .set(harness, value.clone());
                                                }
                                                root.store_settings();
                                                cx.notify();
                                            }),
                                        )
                                        .placeholder(if arguments { "--model glm" } else { "pai" })
                                        .padded()
                                        .vertical_scroll()
                                        .on_key(
                                            cx.listener(
                                                |root, event: &gpui::KeyDownEvent, window, cx| {
                                                    match event.keystroke.key.as_str() {
                                                        "escape" | "enter" => {
                                                            window.focus(&root.focus, cx)
                                                        }
                                                        "tab" => {
                                                            if event.keystroke.modifiers.shift {
                                                                window.focus_prev(cx);
                                                            } else {
                                                                window.focus_next(cx);
                                                            }
                                                        }
                                                        _ => return,
                                                    }
                                                    cx.stop_propagation();
                                                },
                                            ),
                                        ),
                                    ),
                            )
                    }))
                    .into_any_element()
            }
            Setting::CodexSandbox => toggle(theme, "setting-codex-sandbox", settings.codex_sandbox)
                .debug_selector(|| "codex-sandbox-toggle".into())
                .on_click(cx.listener(|root, _, _, cx| {
                    root.settings.codex_sandbox = !root.settings.codex_sandbox;
                    root.store_settings();
                    cx.notify();
                }))
                .into_any_element(),
            Setting::CodexCommandApprovals => toggle(
                theme,
                "setting-codex-approvals",
                settings.codex_command_approvals,
            )
            .debug_selector(|| "codex-approvals-toggle".into())
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.codex_command_approvals = !root.settings.codex_command_approvals;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::OpenCodePermissions => div()
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.tertiary))
                .child(self.ui().text(Text::NativePermissionsValue))
                .into_any_element(),
            Setting::Permissions => self.settings_choice(
                "setting-permissions",
                Value::Permission(settings.permission_mode),
                cx,
            ),
            Setting::MarkdownDefaultView => self.settings_choice(
                "setting-markdown-default-view",
                Value::Markdown(settings.markdown_default_view),
                cx,
            ),
            Setting::Diff => {
                self.settings_choice("setting-diff", Value::Diff(settings.diff_view), cx)
            }
            Setting::UiFont => self.render_font_button(crate::fonts::Which::Ui, cx),
            Setting::CodeFont => self.render_font_choice(crate::fonts::Which::Code, cx),
            Setting::TerminalFont => self.render_font_choice(crate::fonts::Which::Terminal, cx),
            #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
            Setting::AdaptiveRefresh => toggle(
                theme,
                "setting-adaptive_terminal_refresh",
                settings.adaptive_terminal_refresh,
            )
            .debug_selector(|| "adaptive-refresh-toggle".into())
            .on_click(cx.listener(|root, _, window, cx| {
                root.apply_terminal_refresh(!root.settings.adaptive_terminal_refresh, window, cx);
            }))
            .into_any_element(),
            Setting::DevelopmentWorkflows => toggle(
                theme,
                "setting-development_workflows",
                settings.development_workflows,
            )
            .debug_selector(|| "development-workflows-toggle".into())
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.development_workflows = !root.settings.development_workflows;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::Copy => toggle(theme, "setting-copy_on_select", settings.copy_on_select)
                .on_click(cx.listener(|root, _, _, cx| {
                    root.settings.copy_on_select = !root.settings.copy_on_select;
                    root.store_settings();
                    cx.notify();
                }))
                .into_any_element(),
            Setting::Reopen => toggle(
                theme,
                "setting-reopen_last_project",
                settings.reopen_last_project,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.reopen_last_project = !root.settings.reopen_last_project;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::AutoFastForward => toggle(
                theme,
                "setting-auto_fast_forward_default_branch",
                settings.auto_fast_forward_default_branch,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.auto_fast_forward_default_branch =
                    !root.settings.auto_fast_forward_default_branch;
                root.auto_fast_forward_toggled();
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            // GLD-063: off stops every glab child at the next poll, which
            // is this click's own notify away.
            Setting::GitLabMergeRequests => toggle(
                theme,
                "setting-gitlab_merge_requests",
                settings.gitlab_merge_requests,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.gitlab_merge_requests = !root.settings.gitlab_merge_requests;
                root.store_settings();
                root.poll_forge(cx);
                cx.notify();
            }))
            .into_any_element(),
            Setting::CloseProject => toggle(
                theme,
                "setting-confirm_closing_live_project",
                settings.confirm_closing_live_project,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.confirm_closing_live_project =
                    !root.settings.confirm_closing_live_project;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::CloseAgent => toggle(
                theme,
                "setting-confirm_closing_agent_tab",
                settings.confirm_closing_agent_tab,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.confirm_closing_agent_tab = !root.settings.confirm_closing_agent_tab;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::Forget => toggle(
                theme,
                "setting-forget_sessions_on_close",
                settings.forget_sessions_on_close,
            )
            .on_click(cx.listener(|root, _, _, cx| {
                root.settings.forget_sessions_on_close = !root.settings.forget_sessions_on_close;
                root.store_settings();
                cx.notify();
            }))
            .into_any_element(),
            Setting::Autosave => toggle(theme, "setting-autosave", settings.autosave)
                .on_click(cx.listener(|root, _, _, cx| {
                    root.settings.autosave = !root.settings.autosave;
                    root.store_settings();
                    root.restyle_file_tabs(cx);
                    cx.notify();
                }))
                .into_any_element(),
            Setting::SettingsFile => setting_button(
                theme,
                "reveal-settingsfile",
                self.ui().text(Text::RevealFile).to_owned(),
                false,
            )
            .on_click(cx.listener(|_root, _, _, cx| cx.reveal_path(&paths::settings_file())))
            .into_any_element(),
            Setting::KeymapFile => setting_button(
                theme,
                "reveal-keymapfile",
                self.ui().text(Text::RevealFile).to_owned(),
                false,
            )
            .on_click(cx.listener(|_root, _, _, cx| cx.reveal_path(&paths::keymap_file())))
            .into_any_element(),
            Setting::Log => setting_button(
                theme,
                "reveal-log",
                self.ui().text(Text::RevealFile).to_owned(),
                false,
            )
            .on_click(cx.listener(|_root, _, _, cx| cx.reveal_path(&crate::logging::file())))
            .into_any_element(),
            Setting::UiSize => self.settings_stepper(true, cx),
            Setting::TerminalSize => self.settings_stepper(false, cx),
            Setting::SettleAfter => self.settle_stepper(cx),
            Setting::CustomizeTheme => self.customize_theme_button(cx),
            Setting::Theme => {
                // System / Light / Dark as three buttons, each a settings
                // button, so Tab reaches it and
                // Enter or Space presses it.
                let chosen = settings.theme_mode();
                div()
                    .flex()
                    .gap(px(2.0))
                    .children(theme::Mode::ALL.iter().copied().map(|mode| {
                        let (id, text) = match mode {
                            theme::Mode::System => ("theme-mode-system", Text::ThemeSystem),
                            theme::Mode::Light => ("theme-mode-light", Text::ThemeLight),
                            theme::Mode::Dark => ("theme-mode-dark", Text::ThemeDark),
                        };
                        setting_button(theme, id, self.ui().text(text).to_owned(), mode == chosen)
                            .debug_selector(move || id.into())
                            .on_click(
                                cx.listener(move |root, _, _, cx| root.set_theme_mode(mode, cx)),
                            )
                    }))
                    .into_any_element()
            }
        }
    }
    /// After how many days the Sessions mode settles: the size steppers'
    /// look, a day a step, between one day and a month.
    fn settle_stepper(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        const MIN: u32 = 1;
        const MAX: u32 = 30;
        let value = self.settings.settle_after_days.clamp(MIN, MAX);
        let theme = self.theme;
        let step = |root: &mut Root, by: i32, cx: &mut Context<Root>| {
            let next = (root.settings.settle_after_days as i32 + by).clamp(MIN as i32, MAX as i32);
            root.settings.settle_after_days = next as u32;
            root.store_settings();
            cx.notify();
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                setting_button(theme, ("settle-after", 0usize), String::new(), false)
                    .child(
                        crate::icons::glyph("ui/head-hide.svg", theme::size::BODY)
                            .text_color(rgb(theme.text.primary)),
                    )
                    .w(theme::settings::CONTROL)
                    .when(value <= MIN, |button| button.opacity(0.4).tab_stop(false))
                    .on_click(cx.listener(move |root, _, _, cx| step(root, -1, cx))),
            )
            .child(
                div()
                    .debug_selector(|| "settle-after-value".into())
                    .min_w(theme::settings::VALUE_WIDTH)
                    .text_center()
                    .child(format!("{value}")),
            )
            .child(
                setting_button(theme, ("settle-after", 1usize), String::new(), false)
                    .child(
                        crate::icons::glyph("ui/head-plus.svg", theme::size::BODY)
                            .text_color(rgb(theme.text.primary)),
                    )
                    .w(theme::settings::CONTROL)
                    .when(value >= MAX, |button| button.opacity(0.4).tab_stop(false))
                    .on_click(cx.listener(move |root, _, _, cx| step(root, 1, cx))),
            )
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.tertiary))
                    .child(self.ui().text(Text::SettleAfterUnit).to_owned()),
            )
            .into_any_element()
    }

    fn settings_stepper(&self, ui: bool, cx: &mut Context<Self>) -> gpui::AnyElement {
        let (value, min, max, step, id) = if ui {
            (self.settings.ui_font_size, 10.0, 20.0, 0.5, "ui-size")
        } else {
            (
                self.settings.terminal_font_size,
                7.0,
                40.0,
                0.5,
                "terminal-size",
            )
        };
        let theme = self.theme;
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .child(
                setting_button(theme, (id, 0usize), String::new(), false)
                    .child(
                        crate::icons::glyph("ui/head-hide.svg", theme::size::BODY)
                            .text_color(rgb(theme.text.primary)),
                    )
                    .w(theme::settings::CONTROL)
                    .when(value <= min, |button| button.opacity(0.4).tab_stop(false))
                    .on_click(cx.listener(move |root, _, _, cx| {
                        if ui {
                            root.change_ui_font_size(-step, cx);
                        } else {
                            root.change_font_size(-step, cx);
                        }
                    })),
            )
            .child(
                div()
                    .min_w(theme::settings::VALUE_WIDTH)
                    .text_center()
                    .child(format!("{value}")),
            )
            .child(
                setting_button(theme, (id, 1usize), String::new(), false)
                    .child(
                        crate::icons::glyph("ui/head-plus.svg", theme::size::BODY)
                            .text_color(rgb(theme.text.primary)),
                    )
                    .w(theme::settings::CONTROL)
                    .when(value >= max, |button| button.opacity(0.4).tab_stop(false))
                    .on_click(cx.listener(move |root, _, _, cx| {
                        if ui {
                            root.change_ui_font_size(step, cx);
                        } else {
                            root.change_font_size(step, cx);
                        }
                    })),
            )
            .into_any_element()
    }
}
use crate::view::controls::toggle;

impl Root {
    /// A face setting: a button per face
    /// that ships for it, then *Custom*, which opens the list — M12.11.
    ///
    /// **Custom names the face it is on**, drawn in it, once one is
    /// chosen: a lit button that only says
    /// *Custom* would hide which face the setting is.
    fn render_font_choice(
        &self,
        which: crate::fonts::Which,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        use crate::fonts::{Pick, picked, presets};

        let theme = self.theme;
        let ui = self.ui();
        let chosen = match which {
            crate::fonts::Which::Ui => self.settings.ui_font.clone(),
            crate::fonts::Which::Code => self.settings.code_font.clone(),
            crate::fonts::Which::Terminal => self.settings.terminal_font.clone(),
        };
        let lit = picked(which, chosen.as_deref());

        let buttons = presets(which).iter().enumerate().map(|(index, preset)| {
            let label = if preset.beta {
                format!("{} ({})", preset.family, ui.text(Text::FontBeta))
            } else {
                preset.family.to_owned()
            };
            let family = preset.family;
            setting_button(
                theme,
                gpui::ElementId::NamedInteger(font_choice_id(which).into(), index as u64),
                label,
                lit == Pick::Preset(index),
            )
            // Each button is a sample of what it will do.
            .font_family(family)
            .on_click(cx.listener(move |root, _event, _window, cx| {
                // The first button is what no choice draws, and no choice
                // is what it writes: a settings file that names the default
                // keeps naming it after the default moves.
                let family = (index > 0).then(|| family.to_owned());
                root.set_font(which, family, cx);
            }))
            .into_any_element()
        });

        let custom = {
            let label = match (&chosen, lit) {
                (Some(family), Pick::Custom) => family.clone(),
                _ => ui.text(Text::FontCustom).to_owned(),
            };
            let at = self.font_at[which as usize].clone();
            setting_button(
                theme,
                gpui::ElementId::NamedInteger(font_choice_id(which).into(), u64::MAX),
                label,
                lit == Pick::Custom,
            )
            .relative()
            .when_some(
                chosen.clone().filter(|_| lit == Pick::Custom),
                |button, family| button.font_family(family),
            )
            .on_click(cx.listener(move |root, _event, window, cx| {
                cx.stop_propagation();
                root.toggle_font_picker(which, window, cx);
            }))
            // Where the button ended up, for the list that hangs from it.
            .child(
                gpui::canvas(
                    move |bounds, _window, _cx| at.set(bounds.bottom_left()),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .into_any_element()
        };

        div()
            .flex()
            .flex_wrap()
            .min_w_0()
            .items_center()
            .gap(px(6.0))
            .children(buttons)
            .child(custom)
            .into_any_element()
    }
}

/// The name a setting's buttons are told apart by.
fn font_choice_id(which: crate::fonts::Which) -> &'static str {
    match which {
        crate::fonts::Which::Ui => "setting-ui-font",
        crate::fonts::Which::Code => "setting-code-font",
        crate::fonts::Which::Terminal => "setting-terminal-font",
    }
}

pub(super) fn setting_button(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    label: String,
    on: bool,
) -> gpui::Stateful<gpui::Div> {
    crate::view::controls::value(palette, id, label, on)
        .bg(if on {
            rgb(palette.surface.raised).into()
        } else {
            gpui::transparent_black()
        })
        .border_color(theme::settings::outline(palette))
}

pub(super) fn navigation_button(
    palette: theme::Theme,
    id: impl Into<gpui::ElementId>,
    content: impl IntoElement,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    setting_button(palette, id, String::new(), active)
        .text_color(rgb(if active {
            palette.accent.patina
        } else {
            palette.text.tertiary
        }))
        .justify_start()
        .bg(if active {
            rgb(palette.surface.raised).into()
        } else {
            gpui::transparent_black()
        })
        .border_color(if active {
            rgb(palette.accent.patina).into()
        } else {
            gpui::transparent_black()
        })
        .child(content)
}
