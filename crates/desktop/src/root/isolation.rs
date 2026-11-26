//! Explicit diagnostic policy. Never inferred from global environment flags.
use std::path::PathBuf;

pub(crate) enum Policy {
    Production,
    #[cfg(any(test, feature = "native-preview"))]
    Diagnostic {
        state_dir: PathBuf,
    },
}
impl Policy {
    pub(crate) fn external(&self) -> bool {
        matches!(self, Self::Production)
    }
    pub(crate) fn state_dir(&self) -> PathBuf {
        match self {
            Self::Production => crate::paths::state_dir(),
            #[cfg(any(test, feature = "native-preview"))]
            Self::Diagnostic { state_dir } => state_dir.clone(),
        }
    }
    // Regression tests must fail before an unguarded call can touch real services.
    #[cfg(test)]
    pub(crate) fn assert_external_boundary(&self) {
        assert!(
            self.external(),
            "diagnostic Root reached an external service boundary"
        );
    }
}
impl crate::Root {
    #[cfg(any(test, feature = "native-preview"))]
    pub(crate) fn diagnostic(
        desk: guild_session::Desk,
        state_dir: PathBuf,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        Self::from_state_with_policy(
            Default::default(),
            desk,
            Policy::Diagnostic { state_dir },
            cx,
        )
    }
}

/// Production-policy fixtures must also own their global files. A private
/// diagnostic directory cannot protect a constructor deliberately using the
/// production policy (including panel adoption before its first frame).
#[cfg(test)]
pub(crate) fn isolated_test(name: &str) -> bool {
    isolated_test_with(name, &[])
}

/// [`isolated_test`] with more of the child's environment owned by the
/// fixture — a shell, its startup directory, a `PATH` — for a test whose
/// Root launches terminal programs.
#[cfg(test)]
pub(crate) fn isolated_test_with(name: &str, env: &[(&str, &std::ffi::OsStr)]) -> bool {
    let name = name.strip_prefix("guild_desktop::").unwrap_or(name);
    if std::env::var("GUILD_ISOLATED_ROOT_TEST").as_deref() == Ok(name) {
        let state = std::path::PathBuf::from(
            std::env::var_os("GUILD_ISOLATED_ROOT_STATE").expect("owned child state"),
        );
        assert_eq!(crate::paths::settings_file(), state.join("settings.json"));
        assert_eq!(crate::paths::workspace_file(), state.join("workspace.json"));
        return true;
    }
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([name, "--exact", "--nocapture", "--test-threads=1"])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_ISOLATED_ROOT_STATE", state.path())
        .env("GUILD_ISOLATED_ROOT_TEST", name)
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .envs(env.iter().copied())
        .output()
        .unwrap();
    assert!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[cfg(test)]
mod tests {
    use crate::Root;
    use gpui::{AppContext, TestAppContext};

    #[test]
    fn production_fixture_does_not_reset_preferences_on_relaunch() {
        use guild_session::{Settings, Store, ThemeMode};
        let state = tempfile::tempdir().unwrap();
        let path = state.path().join("settings.json");
        Store::new(path.clone())
            .save(&Settings {
                theme_mode: Some(ThemeMode::Dark),
                ..Default::default()
            })
            .unwrap();
        let before = std::fs::read(&path).unwrap();
        for fixture in [
            "root::run::tests::elapsed_time_does_not_notify_root_or_hidden_output",
            "root::command::listing_tests::the_listing_carries_the_reported_waiting_reason",
            "root::status::tests::reports_between_two_polls_reach_the_board_in_order",
            "root::status::tests::only_an_unseen_change_into_done_chimes",
            "root::status::tests::a_reported_turn_chimes_once_and_is_unread_until_seen",
            "root::status::tests::a_rebound_session_starts_from_the_terminal_s_record",
        ] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([fixture, "--exact", "--nocapture", "--test-threads=1"])
                .env("GUILD_STATE_DIR", state.path())
                .env("GUILD_NO_GLOBAL_HOOKS", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("1 passed; 0 failed"),
                "{fixture}:\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                std::fs::read(&path).unwrap(),
                before,
                "passing fixture {fixture} must not reset the enclosing application's preferences"
            );
        }
    }

    #[test]
    fn isolated_diagnostic_settings_preserve_saved_preferences() {
        let state = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "root::isolation::tests::diagnostic_saves_do_not_reset_preferences_on_relaunch",
                "--exact",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env("GUILD_STATE_DIR", state.path())
            .env("GUILD_SETTINGS_ISOLATION_TEST", state.path())
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[gpui::test]
    #[ignore = "run through isolated_diagnostic_settings_preserve_saved_preferences"]
    fn diagnostic_saves_do_not_reset_preferences_on_relaunch(cx: &mut TestAppContext) {
        use guild_session::{Language, Settings, Store, ThemeMode};
        cx.executor().allow_parking();
        let state = std::path::PathBuf::from(
            std::env::var_os("GUILD_SETTINGS_ISOLATION_TEST").expect("isolated child only"),
        );
        let settings_path = state.join("settings.json");
        assert_eq!(crate::paths::settings_file(), settings_path);
        assert_eq!(std::env::var("GUILD_NO_GLOBAL_HOOKS").as_deref(), Ok("1"));
        let saved = Settings {
            theme_mode: Some(ThemeMode::Dark),
            language: Language::Russian,
            terminal_font_size: 19.,
            ..Default::default()
        };
        Store::new(settings_path.clone()).save(&saved).unwrap();
        Store::new(settings_path.clone()).save(&saved).unwrap();
        let before = std::fs::read(&settings_path).unwrap();
        let diagnostic = state.join("diagnostic");
        let root = cx.new(|cx| Root::diagnostic(Default::default(), diagnostic.clone(), cx));
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                root.update(cx, |root, _| root.remember_window(window));
            })
            .unwrap();
        guild_session::flush();
        // The application's real loader on the next launch must see the same
        // saved preferences, even after a default-valued diagnostic was drawn.
        let relaunched = Store::<Settings>::new(settings_path.clone())
            .load()
            .unwrap();
        assert_eq!(
            relaunched.theme_mode, saved.theme_mode,
            "diagnostic paint reset the saved theme"
        );
        assert_eq!(relaunched.language, saved.language);
        assert_eq!(relaunched.terminal_font_size, saved.terminal_font_size);
        assert_eq!(std::fs::read(&settings_path).unwrap(), before);
        assert_eq!(
            std::fs::read(state.join("settings.json.bak")).unwrap(),
            before
        );
        assert!(diagnostic.join("settings.json").is_file());

        // The scoped worker and its quit drain must select the same owned file
        // as ordinary settings edits; neither may overwrite the enclosing app.
        root.update(cx, |root, cx| {
            root.commit_scoped_preference(
                super::super::repository_settings::Edit::Default("rust".into(), false),
                cx,
            );
            root.commit_scoped_preference(
                super::super::repository_settings::Edit::Default("go".into(), false),
                cx,
            );
            root.settings.copy_on_select = false;
            root.store_settings();
            root.write();
        });
        let finish = root.update(cx, |root, cx| root.finish_preferences(cx));
        cx.foreground_executor().block_test(finish);
        let own: Settings =
            serde_json::from_slice(&std::fs::read(diagnostic.join("settings.json")).unwrap())
                .unwrap();
        assert_eq!(own.lsp_defaults.get("rust"), Some(&false));
        assert_eq!(own.lsp_defaults.get("go"), Some(&false));
        assert!(!own.copy_on_select);
        assert!(diagnostic.join("workspace.json").is_file());
        assert!(!state.join("workspace.json").exists());
        assert_eq!(std::fs::read(settings_path).unwrap(), before);
        assert_eq!(
            std::fs::read(state.join("settings.json.bak")).unwrap(),
            before
        );
    }

    #[gpui::test]
    fn diagnostic_root_never_enters_external_startup_services(cx: &mut TestAppContext) {
        let state = tempfile::tempdir().unwrap();
        let root = cx.new(|cx| Root::diagnostic(Default::default(), state.path().into(), cx));
        root.update(cx, |root, cx| {
            root.start_ide();
            root.start_hooks();
            root.poll_usage(cx);
            assert!(root.ide.is_none());
            assert!(root.hooks.is_none());
        });
    }

    #[gpui::test]
    fn diagnostic_root_retains_local_tree_and_file_watch(cx: &mut TestAppContext) {
        let state = tempfile::tempdir().unwrap();
        let project_path = state.path().join("project");
        std::fs::create_dir(&project_path).unwrap();
        std::fs::write(project_path.join("local.txt"), "fixture").unwrap();
        let mut desk = guild_session::Desk::default();
        let project = desk.open_in(desk.active_workspace(), &project_path);
        desk.look_at(Some(project), None);
        let root = cx.new(|cx| Root::diagnostic(desk, state.path().into(), cx));
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                root.update(cx, |root, cx| {
                    root.ensure_active_tree(cx);
                    root.apply_batches(window, cx);
                    assert!(root.tree().is_some());
                    assert!(root.watchers.contains_key(&project));
                    assert!(root.ide.is_none());
                    assert!(root.hooks.is_none());
                    assert!(root.servers.is_empty());
                });
            })
            .unwrap();
    }

    #[gpui::test]
    fn diagnostic_root_watch_does_not_dispatch_external_integrations(cx: &mut TestAppContext) {
        let state = tempfile::tempdir().unwrap();
        let root = cx.new(|cx| Root::diagnostic(Default::default(), state.path().into(), cx));
        let window = cx.add_window(|_, _| gpui::Empty);
        window
            .update(cx, |_, window, cx| {
                root.update(cx, |root, cx| root.poll_integrations(window, cx));
            })
            .unwrap();
    }
}
