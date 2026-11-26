//! Settings inventory: one source for grouping, search and stable row identities.
use super::Section;
use crate::i18n::{Text, Ui};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Setting {
    DevelopmentWorkflows,
    NetworkMode,
    NetworkAddress,
    NetworkBypass,
    NetworkGuild,
    NetworkAgent,
    NetworkTerminal,
    NetworkGit,
    NetworkTools,
    Language,
    SettingsFile,
    Log,
    Theme,
    CustomizeTheme,
    UiFont,
    UiSize,
    CodeFont,
    ActionBarOrder,
    OpenInApps,
    TerminalFont,
    TerminalSize,
    Copy,
    #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
    AdaptiveRefresh,
    DefaultTab,
    TabLauncherOrder,
    SettleAfter,
    Permissions,
    CodexSandbox,
    CodexCommandApprovals,
    OpenCodePermissions,
    ClaudePrefix,
    CodexPrefix,
    OpenCodePrefix,
    Reopen,
    AutoFastForward,
    GitLabMergeRequests,
    CloseProject,
    CloseAgent,
    Forget,
    Autosave,
    MarkdownDefaultView,
    Diff,
    KeymapFile,
    UpdateSource,
    UpdateChannel,
    UpdateDirectory,
    UpdateStatus,
}

impl Setting {
    pub(crate) const ALL: &[Self] = &[
        Self::NetworkMode,
        Self::NetworkAddress,
        Self::NetworkBypass,
        Self::NetworkGuild,
        Self::NetworkAgent,
        Self::NetworkTerminal,
        Self::NetworkGit,
        Self::NetworkTools,
        Self::Language,
        Self::SettingsFile,
        Self::Log,
        Self::Theme,
        Self::CustomizeTheme,
        Self::UiFont,
        Self::UiSize,
        Self::CodeFont,
        Self::ActionBarOrder,
        Self::OpenInApps,
        Self::TerminalFont,
        Self::TerminalSize,
        Self::Copy,
        #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
        Self::AdaptiveRefresh,
        Self::DefaultTab,
        Self::TabLauncherOrder,
        Self::SettleAfter,
        Self::Permissions,
        Self::ClaudePrefix,
        Self::CodexSandbox,
        Self::CodexCommandApprovals,
        Self::CodexPrefix,
        Self::OpenCodePermissions,
        Self::OpenCodePrefix,
        Self::Reopen,
        Self::AutoFastForward,
        Self::GitLabMergeRequests,
        Self::CloseProject,
        Self::CloseAgent,
        Self::Forget,
        Self::Autosave,
        Self::MarkdownDefaultView,
        Self::Diff,
        Self::KeymapFile,
        Self::DevelopmentWorkflows,
        Self::UpdateSource,
        Self::UpdateChannel,
        Self::UpdateDirectory,
        Self::UpdateStatus,
    ];
    pub(crate) fn info(self) -> (Section, Text, Text, &'static str) {
        match self {
            Self::DevelopmentWorkflows => (
                Section::Experimental,
                Text::SectionExperimental,
                Text::DevelopmentWorkflowsTitle,
                "development_workflows",
            ),
            Self::NetworkMode => (
                Section::Network,
                Text::NetworkConnection,
                Text::NetworkMode,
                "network_mode",
            ),
            Self::NetworkAddress => (
                Section::Network,
                Text::NetworkConnection,
                Text::NetworkAddress,
                "network_address",
            ),
            Self::NetworkBypass => (
                Section::Network,
                Text::NetworkConnection,
                Text::NetworkBypass,
                "network_bypass",
            ),
            Self::NetworkGuild => (
                Section::Network,
                Text::NetworkApply,
                Text::NetworkGuild,
                "network_guild",
            ),
            Self::NetworkAgent => (
                Section::Network,
                Text::NetworkApply,
                Text::NetworkAgent,
                "network_agent",
            ),
            Self::NetworkTerminal => (
                Section::Network,
                Text::NetworkApply,
                Text::NetworkTerminal,
                "network_terminal",
            ),
            Self::NetworkGit => (
                Section::Network,
                Text::NetworkApply,
                Text::NetworkGit,
                "network_git",
            ),
            Self::NetworkTools => (
                Section::Network,
                Text::NetworkApply,
                Text::NetworkTools,
                "network_tools",
            ),
            Self::UpdateSource => (
                Section::Updates,
                Text::UpdatesSection,
                Text::UpdaterSource,
                "update_source",
            ),
            Self::UpdateChannel => (
                Section::Updates,
                Text::UpdatesSection,
                Text::UpdaterChannel,
                "update_channel",
            ),
            Self::UpdateDirectory => (
                Section::Updates,
                Text::UpdatesSection,
                Text::UpdaterDirectory,
                "update_directory",
            ),
            Self::UpdateStatus => (
                Section::Updates,
                Text::UpdatesSection,
                Text::UpdaterStatus,
                "update_status",
            ),
            Self::Language => (
                Section::General,
                Text::SettingsLanguage,
                Text::LanguageTitle,
                "language",
            ),
            Self::SettingsFile => (
                Section::General,
                Text::SettingsLocalFiles,
                Text::SettingsTitle,
                "settings_file",
            ),
            Self::Log => (
                Section::General,
                Text::SettingsLocalFiles,
                Text::LogDirectoryTitle,
                "log",
            ),
            Self::Theme => (
                Section::Appearance,
                Text::SettingsColourTheme,
                Text::ThemeTitle,
                "theme",
            ),
            Self::CustomizeTheme => (
                Section::Appearance,
                Text::SettingsColourTheme,
                Text::CustomizeThemeTitle,
                "customize_theme",
            ),
            Self::UiFont => (
                Section::Appearance,
                Text::SettingsTypography,
                Text::UiFontTitle,
                "ui_font",
            ),
            Self::UiSize => (
                Section::Appearance,
                Text::SettingsTypography,
                Text::UiFontSizeTitle,
                "ui_font_size",
            ),
            Self::CodeFont => (
                Section::Appearance,
                Text::SettingsTypography,
                Text::CodeFontTitle,
                "code_font",
            ),
            Self::OpenInApps => (
                Section::General,
                Text::OpenInAppsTitle,
                Text::OpenInAppsTitle,
                "open_in_apps",
            ),
            Self::ActionBarOrder => (
                Section::Appearance,
                Text::SettingsActionBar,
                Text::ActionBarItemOrder,
                "action_bar_order",
            ),
            Self::TerminalFont => (
                Section::Terminal,
                Text::SettingsTypography,
                Text::TerminalFontTitle,
                "terminal_font",
            ),
            Self::TerminalSize => (
                Section::Terminal,
                Text::SettingsTypography,
                Text::FontSizeTitle,
                "terminal_font_size",
            ),
            Self::Copy => (
                Section::Terminal,
                Text::SettingsSelection,
                Text::CopyOnSelectTitle,
                "copy_on_select",
            ),
            #[cfg(all(target_os = "macos", feature = "adaptive-terminal-refresh"))]
            Self::AdaptiveRefresh => (
                Section::Terminal,
                Text::SettingsRendering,
                Text::AdaptiveRefreshTitle,
                "adaptive_terminal_refresh",
            ),
            Self::TabLauncherOrder => (
                Section::Sessions,
                Text::SettingsNewSessions,
                Text::TabLauncherOrderTitle,
                "tab_launcher_order",
            ),
            Self::SettleAfter => (
                Section::Sessions,
                Text::SettingsSessionList,
                Text::SettleAfterTitle,
                "settle_after_days",
            ),
            Self::DefaultTab => (
                Section::Sessions,
                Text::SettingsNewSessions,
                Text::DefaultTabTitle,
                "default_tab",
            ),
            Self::ClaudePrefix => (
                Section::Sessions,
                Text::AgentClaudeCode,
                Text::LaunchPrefixes,
                "claude_launch_prefix",
            ),
            Self::CodexPrefix => (
                Section::Sessions,
                Text::AgentCodex,
                Text::LaunchPrefixes,
                "codex_launch_prefix",
            ),
            Self::OpenCodePrefix => (
                Section::Sessions,
                Text::AgentOpenCode,
                Text::LaunchPrefixes,
                "opencode_launch_prefix",
            ),
            Self::Permissions => (
                Section::Sessions,
                Text::AgentClaudeCode,
                Text::PermissionsTitle,
                "permission_mode",
            ),
            Self::CodexSandbox => (
                Section::Sessions,
                Text::AgentCodex,
                Text::CodexSandboxTitle,
                "codex_sandbox",
            ),
            Self::CodexCommandApprovals => (
                Section::Sessions,
                Text::AgentCodex,
                Text::CodexCommandApprovalsTitle,
                "codex_command_approvals",
            ),
            Self::OpenCodePermissions => (
                Section::Sessions,
                Text::AgentOpenCode,
                Text::PermissionsTitle,
                "opencode_permissions",
            ),
            Self::Reopen => (
                Section::Projects,
                Text::SettingsStartup,
                Text::ReopenLastTitle,
                "reopen_last_project",
            ),
            Self::AutoFastForward => (
                Section::Projects,
                Text::ToolGit,
                Text::AutoFastForwardTitle,
                "auto_fast_forward_default_branch",
            ),
            Self::GitLabMergeRequests => (
                Section::Projects,
                Text::ToolGit,
                Text::GitLabMrTitle,
                "gitlab_merge_requests",
            ),
            Self::CloseProject => (
                Section::Projects,
                Text::SettingsClosing,
                Text::ConfirmCloseTitle,
                "confirm_closing_live_project",
            ),
            Self::CloseAgent => (
                Section::Projects,
                Text::SettingsClosing,
                Text::ConfirmCloseAgentTitle,
                "confirm_closing_agent_tab",
            ),
            Self::Forget => (
                Section::Projects,
                Text::SettingsClosing,
                Text::ForgetOnCloseTitle,
                "forget_sessions_on_close",
            ),
            Self::Autosave => (
                Section::Editor,
                Text::SettingsSaving,
                Text::AutosaveTitle,
                "autosave",
            ),
            Self::MarkdownDefaultView => (
                Section::Editor,
                Text::SettingsMarkdown,
                Text::MarkdownDefaultViewTitle,
                "markdown_default_view",
            ),
            Self::Diff => (
                Section::Editor,
                Text::SettingsDiffs,
                Text::DiffViewTitle,
                "diff_view",
            ),
            Self::KeymapFile => (
                Section::Keymap,
                Text::SettingsLocalFiles,
                Text::KeymapFileTitle,
                "keymap_file",
            ),
        }
    }
    pub(crate) fn matches(self, query: &str, hint: &str, ui: Ui) -> bool {
        let (section, group, title, key) = self.info();
        let launch_labels = if matches!(
            self,
            Self::ClaudePrefix | Self::CodexPrefix | Self::OpenCodePrefix
        ) {
            format!(
                "{} {}",
                ui.text(Text::LaunchPrefixLabel),
                ui.text(Text::LaunchArgumentsLabel)
            )
        } else {
            String::new()
        };
        matches_query(
            query,
            &[
                key,
                &launch_labels,
                ui.text(title),
                ui.text(group),
                ui.text(section.title()),
                hint,
            ],
        )
    }
}

/// Words may match different parts of a row; whitespace-only queries are empty.
pub(crate) fn matches_query(query: &str, fields: &[&str]) -> bool {
    let haystack = fields.join(" ").to_lowercase();
    query
        .split_whitespace()
        .all(|word| haystack.contains(&word.to_lowercase()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tabs_and_agents_group_choices_by_their_owner() {
        let ui = Ui(guild_session::Language::English);
        assert_eq!(ui.text(Section::Sessions.title()), "Tabs & agents");
        for (key, group) in [
            ("default_tab", "New tabs"),
            ("tab_launcher_order", "New tabs"),
            ("permission_mode", "Claude Code"),
            ("claude_launch_prefix", "Claude Code"),
            ("codex_sandbox", "Codex"),
            ("codex_command_approvals", "Codex"),
            ("codex_launch_prefix", "Codex"),
            ("opencode_permissions", "OpenCode"),
            ("opencode_launch_prefix", "OpenCode"),
        ] {
            let setting = Setting::ALL.iter().find(|row| row.info().3 == key).unwrap();
            assert_eq!(setting.info().0, Section::Sessions);
            assert_eq!(ui.text(setting.info().1), group);
        }
    }

    #[test]
    fn launch_prefixes_are_searchable_in_both_languages() {
        for (key, harness) in [
            ("claude_launch_prefix", "Claude Code"),
            ("codex_launch_prefix", "Codex"),
            ("opencode_launch_prefix", "OpenCode"),
        ] {
            let setting = Setting::ALL.iter().find(|row| row.info().3 == key).unwrap();
            assert_eq!(setting.info().0, Section::Sessions);
            for (language, query) in [
                (guild_session::Language::English, "Launch command"),
                (guild_session::Language::Russian, "Команда запуска"),
            ] {
                let ui = Ui(language);
                assert!(setting.matches(&format!("{query} {harness}"), "", ui));
                assert!(setting.matches(key, "", ui));
                assert!(setting.matches(ui.text(Text::LaunchArgumentsLabel), "", ui));
            }
        }
    }
    /// The switch lives in Projects under Git, and is found by its title
    /// and its key in both languages.
    #[test]
    fn auto_fast_forward_is_a_projects_row_searchable_in_both_languages() {
        let key = "auto_fast_forward_default_branch";
        let setting = Setting::ALL
            .iter()
            .find(|setting| setting.info().3 == key)
            .expect("auto fast-forward is in the settings inventory");
        assert_eq!(setting.info().0, Section::Projects);
        assert_eq!(setting.info().1, Text::ToolGit);
        for language in [
            guild_session::Language::English,
            guild_session::Language::Russian,
        ] {
            let ui = Ui(language);
            assert!(setting.matches(ui.text(Text::AutoFastForwardTitle), "", ui));
            assert!(setting.matches(key, "", ui));
        }
        assert!(setting.matches("fast-forward", "", Ui(guild_session::Language::English)));
        assert!(setting.matches("перематывать", "", Ui(guild_session::Language::Russian)));
    }
    /// GitLab merge requests sit beside auto fast-forward in Projects under
    /// Git, found by title and key in both languages — GLD-063.
    #[test]
    fn gitlab_merge_requests_are_a_projects_row_searchable_in_both_languages() {
        let key = "gitlab_merge_requests";
        let setting = Setting::ALL
            .iter()
            .find(|setting| setting.info().3 == key)
            .expect("GitLab merge requests are in the settings inventory");
        assert_eq!(setting.info().0, Section::Projects);
        assert_eq!(setting.info().1, Text::ToolGit);
        let after = Setting::ALL
            .iter()
            .position(|s| s.info().3 == "auto_fast_forward_default_branch")
            .unwrap();
        assert_eq!(
            Setting::ALL[after + 1].info().3,
            key,
            "right after auto fast-forward"
        );
        for language in [
            guild_session::Language::English,
            guild_session::Language::Russian,
        ] {
            let ui = Ui(language);
            assert!(setting.matches(ui.text(Text::GitLabMrTitle), "", ui));
            assert!(setting.matches(key, "", ui));
        }
        let english = Ui(guild_session::Language::English);
        assert!(setting.matches("glab", english.text(Text::GitLabMrHint), english));
        // What off does, as code review r1 decided and r2 asked the words
        // to say: no new request, and one already running finishes with its
        // answer dropped — never "nothing at all".
        let hint = english.text(Text::GitLabMrHint);
        assert!(
            hint.contains("no new request") && hint.contains("finishes"),
            "{hint}"
        );
        assert!(!hint.contains("asks GitLab nothing"), "{hint}");
        let hint = Ui(guild_session::Language::Russian).text(Text::GitLabMrHint);
        assert!(
            hint.contains("новых запросов") && hint.contains("доработает"),
            "{hint}"
        );
        assert!(!hint.contains("ничего не спрашивает"), "{hint}");
    }
    #[test]
    fn update_sources_and_channels_are_searchable_in_both_languages() {
        for key in [
            "update_source",
            "update_channel",
            "update_directory",
            "update_status",
        ] {
            let setting = Setting::ALL
                .iter()
                .find(|setting| setting.info().3 == key)
                .expect("updater control is present in the settings inventory");
            for language in [
                guild_session::Language::English,
                guild_session::Language::Russian,
            ] {
                let ui = Ui(language);
                assert!(setting.matches(ui.text(setting.info().2), "", ui));
                assert!(setting.matches(key, "", ui));
            }
        }
    }
    #[test]
    fn action_bar_is_searchable_in_both_languages() {
        let setting = Setting::ALL
            .iter()
            .find(|row| row.info().3 == "action_bar_order")
            .expect("Action Bar has a searchable settings home");
        assert_eq!(setting.info().0, Section::Appearance);
        for (language, query) in [
            (
                guild_session::Language::English,
                "Action Bar Items and order",
            ),
            (
                guild_session::Language::Russian,
                "Панель действий Элементы и порядок",
            ),
        ] {
            assert!(setting.matches(query, "", Ui(language)));
            let ui = Ui(language);
            assert!(setting.matches("Option", ui.text(Text::ActionBarHint), ui));
            assert!(setting.matches(
                ui.text(Text::ActionBarAdd),
                ui.text(Text::ActionBarHint),
                ui
            ));
        }
    }

    #[test]
    fn markdown_default_view_is_searchable_in_both_languages() {
        let setting = Setting::ALL
            .iter()
            .find(|s| s.info().3 == "markdown_default_view")
            .expect("Markdown default view is in the settings inventory");
        assert_eq!(setting.info().0, Section::Editor);
        for (language, title) in [
            (guild_session::Language::English, "Markdown default view"),
            (
                guild_session::Language::Russian,
                "Режим Markdown по умолчанию",
            ),
        ] {
            let ui = Ui(language);
            assert_eq!(ui.text(setting.info().2), title);
            assert!(setting.matches("MARKDOWN", "", ui));
            assert!(setting.matches(title, "", ui));
        }
    }
    #[test]
    fn searching_across_labels_and_keys_is_case_and_whitespace_insensitive() {
        assert!(matches_query(
            " TERMINAL  font ",
            &["Terminal", "terminal_font_size"]
        ));
        assert!(matches_query("ШРИФТ", &["Размер шрифта"]));
        assert!(!matches_query("terminal account", &["Terminal font"]));
        assert!(matches_query("  ", &[]));
    }
    #[test]
    fn adaptive_preference_is_searchable_only_in_supported_builds() {
        let found = Setting::ALL
            .iter()
            .find(|s| s.info().3 == "adaptive_terminal_refresh");
        assert_eq!(
            found.is_some(),
            cfg!(all(
                target_os = "macos",
                feature = "adaptive-terminal-refresh"
            ))
        );
        if let Some(setting) = found {
            assert_eq!(setting.info().0, Section::Terminal);
            for (language, query) in [
                (guild_session::Language::English, "adaptive 120"),
                (guild_session::Language::Russian, "адаптивная 120"),
            ] {
                assert!(setting.matches(query, "", Ui(language)));
            }
        }
    }
    #[test]
    fn experimental_contains_one_searchable_opt_in() {
        let rows: Vec<_> = Setting::ALL
            .iter()
            .filter(|s| s.info().0 == Section::Experimental)
            .collect();
        assert_eq!(rows, vec![&Setting::DevelopmentWorkflows]);
        for (language, query) in [
            (guild_session::Language::English, "experimental development"),
            (guild_session::Language::Russian, "эксперименты процессы"),
        ] {
            assert!(rows[0].matches(query, "", Ui(language)));
        }
    }
    #[test]
    fn every_existing_setting_has_one_home_and_a_unique_search_key() {
        let keys: std::collections::BTreeSet<_> =
            Setting::ALL.iter().map(|row| row.info().3).collect();
        assert_eq!(
            keys.len(),
            // 44 with Customize theme; 45 with the
            // Sessions mode's Settled threshold; 46 with GitLab merge
            // requests, GLD-063.
            46 + usize::from(cfg!(all(
                target_os = "macos",
                feature = "adaptive-terminal-refresh"
            )))
        );
        assert_eq!(Setting::TerminalFont.info().0, Section::Terminal);
        assert_eq!(Setting::Log.info().0, Section::General);
        assert_eq!(Setting::SettleAfter.info().0, Section::Sessions);
        assert_eq!(keys.len(), Setting::ALL.len(), "no duplicate settings keys");
        assert!(
            !keys.contains("diagram_components"),
            "diagrams ship with the app, without component management"
        );
        assert_eq!(Setting::OpenInApps.info().0, Section::General);
        for language in [
            guild_session::Language::English,
            guild_session::Language::Russian,
        ] {
            let ui = Ui(language);
            assert!(Setting::OpenInApps.matches(
                ui.text(Text::OpenInAppsTitle),
                ui.text(Text::OpenInAppsHint),
                ui
            ));
            assert!(Setting::CustomizeTheme.matches(
                ui.text(Text::CustomizeThemeTitle),
                ui.text(Text::CustomizeThemeHint),
                ui
            ));
            assert_eq!(Setting::CustomizeTheme.info().0, Section::Appearance);
        }
    }
}
