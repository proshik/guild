//! What a tab shows, and how it is started.
//!
//! Opening a tab's content handles agent terminals, shells, and launch failures.
//! It sat beside `Root` because it was written there, not because it belongs to it —
//! nothing here touches the window, except to name it as the one a
//! terminal tells of a changed fact.

use crate::i18n::{Text, Ui};
use crate::{Body, terminal, theme};
use guild_session::{Conversation, Runs, SessionId, Settings};

/// The UI owns Codex's two policies. Filter conflicting options only in the
/// launch copy, leaving the saved Arguments text and other harnesses untouched.
pub(super) fn launch_arguments(
    settings: &Settings,
    harness: guild_session::HarnessKind,
) -> std::borrow::Cow<'_, str> {
    let arguments = settings.launch_arguments.get(harness);
    if harness != guild_session::HarnessKind::Codex {
        return arguments.into();
    }
    let policies = format!(
        "--sandbox {} --ask-for-approval {}",
        if settings.codex_sandbox {
            "workspace-write"
        } else {
            "danger-full-access"
        },
        if settings.codex_command_approvals {
            "on-request"
        } else {
            "never"
        }
    );
    let Ok(words) = shell_words::split(arguments) else {
        // Preserve the existing launch validation error for malformed quoting.
        return format!("{policies} {arguments}").into();
    };
    let mut retained = Vec::new();
    let mut words = words.into_iter().peekable();
    while let Some(word) = words.next() {
        if word == "--" {
            retained.push(word);
            retained.extend(words);
            break;
        }
        let option = word.split('=').next().unwrap_or(&word);
        if matches!(
            option,
            "--dangerously-bypass-approvals-and-sandbox"
                | "--yolo"
                | "--full-auto"
                | "--approve-for-me"
                | "--not-so-yolo"
                | "--auto-review"
        ) {
            continue;
        }
        if matches!(
            word.as_str(),
            "--sandbox" | "--ask-for-approval" | "-s" | "-a"
        ) {
            if words.peek().is_some_and(|value| !value.starts_with('-')) {
                words.next();
            }
            continue;
        }
        if matches!(option, "--sandbox" | "--ask-for-approval")
            || (word.starts_with("-s") || word.starts_with("-a")) && !word.starts_with("--")
        {
            continue;
        }
        if matches!(word.as_str(), "-c" | "--config") {
            if words
                .peek()
                .is_some_and(|value| codex_policy_override(value))
            {
                words.next();
                continue;
            }
            retained.push(word);
            // A config value is data, even if it resembles an option.
            if let Some(value) = words.next() {
                retained.push(value);
            }
            continue;
        }
        if word
            .strip_prefix("--config=")
            .or_else(|| word.strip_prefix("-c"))
            .is_some_and(|value| codex_policy_override(value.trim_start_matches('=')))
        {
            continue;
        }
        retained.push(word);
    }
    format!("{policies} {}", shell_words::join(retained)).into()
}

fn codex_policy_override(value: &str) -> bool {
    value.split_once('=').is_some_and(|(key, _)| {
        matches!(
            key.trim().trim_matches(['\"', '\'']),
            "sandbox_mode" | "approval_policy" | "approvals_reviewer" | "default_permissions"
        )
    })
}

impl Body {
    /// What takes the keys when this tab is in front, if anything does:
    /// the terminal or editor. Settings and a broken tab have
    /// nothing to type into, and the window's own handle stands in.
    pub(crate) fn focus_handle(&self, cx: &gpui::App) -> Option<gpui::FocusHandle> {
        use gpui::Focusable;
        match self {
            Self::Terminal { view, .. } => Some(view.read(cx).focus_handle(cx)),
            Self::File {
                tab, presentation, ..
            } => Some(presentation.as_ref().map_or_else(
                || tab.read(cx).focus_handle(cx),
                |p| p.view.read(cx).focus_handle(cx),
            )),
            Self::Diff { tab, .. } => Some(tab.read(cx).focus_handle(cx)),
            Self::Annotate { tab, .. } => Some(tab.read(cx).focus_handle(cx)),
            Self::Broken { .. } | Self::Settings { .. } | Self::Process { .. } => None,
        }
    }

    /// The session being shown, if this tab shows one.
    pub(crate) fn session(&self) -> Option<SessionId> {
        match self {
            Self::Terminal { session, .. } | Self::Broken { session, .. } => Some(*session),
            Self::Settings { .. }
            | Self::Process { .. }
            | Self::File { .. }
            | Self::Diff { .. }
            | Self::Annotate { .. } => None,
        }
    }
}

impl Body {
    /// Open a tab's content.
    ///
    /// `conversation` is required by every agent harness. A plain shell
    /// has no conversation.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn open(
        session: SessionId,
        runs: Runs,
        cwd: &std::path::Path,
        conversation: Option<Conversation>,
        settings: &Settings,
        // The palette in effect — the window's, which the settings alone
        // cannot say: the mode may follow the system, and a customization
        // is computed from the built-in palette, not stored as one.
        theme: theme::Theme,
        // The port this window is announcing itself on, when it is: a
        // terminal session is pointed at it so that the agent's edits
        // come back to the window they were asked for in. M8.6.
        ide: Option<u16>,
        // The hooks a terminal session reports through, when the window
        // is listening for them. M5.10.
        hooks: Option<guild_session::Hooks>,
        // What the session is asked first, when the command line started
        // it with a task — M13. A terminal session takes it as `claude`'s
        // positional prompt.
        first: Option<String>,
        // The window's start size — what a terminal tab is spawned at, so
        // that a tab restored behind another is not born at 80 × 24.
        selection: Option<&guild_session::harness::LaunchSelection>,
        start: &terminal::view::StartSize,
        // The window's root: a terminal tells it by id when a fact it
        // reads changes — `terminal::facts`.
        cx: &mut gpui::Context<crate::Root>,
    ) -> Self {
        let root = cx.entity_id();
        let permission_mode = settings.permission_mode;
        let look = terminal::view::Look {
            copy_on_select: settings.copy_on_select,
            adaptive_terminal_refresh: settings.adaptive_terminal_refresh,
            font_size: settings.terminal_font_size,
            font: settings.terminal_font.clone(),
            theme,
            ui: Ui(settings.language),
        };
        let opened = match (runs, conversation) {
            (Runs::Claude(_) | Runs::OpenCode(_) | Runs::Codex(_), Some(conversation)) => {
                let harness = runs.harness().expect("an agent tab has a harness");
                terminal::view::open_with_selection(
                    harness,
                    &session.to_string(),
                    cwd,
                    &conversation,
                    permission_mode,
                    settings.launch_prefixes.get(harness),
                    &launch_arguments(settings, harness),
                    ide,
                    hooks.as_ref(),
                    first.as_deref(),
                    selection,
                    look,
                    start,
                    cx,
                )
            }
            (Runs::Shell, _) => terminal::view::open_shell(
                cwd,
                &session.to_string(),
                hooks.as_ref(),
                look,
                start,
                cx,
            ),
            // An agent session without a conversation is impossible: the
            // caller assigns it, and forgetting that means opening a tab
            // with nothing to work with.
            (Runs::Claude(_) | Runs::OpenCode(_) | Runs::Codex(_), None) => {
                return Self::Broken {
                    session,
                    reason: Ui(settings.language)
                        .text(Text::ClaudeWithoutConversation)
                        .to_owned(),
                };
            }
        };

        match opened {
            Ok(view) => {
                // Counted while its process goes after the tab closes, so
                // a continuation waits for it: `terminal::retiring`.
                view.update(cx, |view, _| view.set_retire_tag(session.to_string()));
                crate::Root::hear_terminal_input(&view, cx);
                let diagnostics = view.read(cx).discovery_diagnostics.clone();
                if !diagnostics.is_empty() {
                    let root = cx.entity().downgrade();
                    cx.defer(move |cx| {
                        let _ = root.update(cx, |root, cx| {
                            root.warn(diagnostics.join("\n"));
                            cx.notify();
                        });
                    });
                }
                let facts = view.read(cx).facts().clone();
                facts.listen(root);
                facts.set_launch_settings(settings.clone());
                Self::Terminal {
                    session,
                    view,
                    facts,
                }
            }
            Err(err) => {
                // To the log as well: the tab shows the reason, but the
                // window may not be looked at, and without a record there
                // would be nothing to work from later.
                tracing::error!(%err, cwd = %cwd.display(), "tab failed to open");
                Self::Broken {
                    session,
                    reason: err.to_string(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::AppContext;

    #[test]
    fn independent_codex_policies_override_manual_flags_without_changing_saved_arguments() {
        use guild_session::HarnessKind;
        let mut settings = Settings::default();
        for manual in [
            "  --model 'multi word'  ",
            "--sandbox danger-full-access --ask-for-approval never --yolo --model 'multi word'",
            "-sdanger-full-access -anever --dangerously-bypass-approvals-and-sandbox --full-auto --approve-for-me --not-so-yolo --model 'multi word'",
            "--sandbox=read-only --ask-for-approval=never --config 'sandbox_mode=\"danger-full-access\"' -c 'approval_policy=\"never\"' --config=approvals_reviewer=\"auto_review\" --model 'multi word'",
        ] {
            settings.launch_arguments.codex = manual.into();
            for sandbox in [true, false] {
                for approvals in [true, false] {
                    settings.codex_sandbox = sandbox;
                    settings.codex_command_approvals = approvals;
                    let actual =
                        shell_words::split(&launch_arguments(&settings, HarnessKind::Codex))
                            .unwrap();
                    assert_eq!(
                        actual,
                        [
                            "--sandbox",
                            if sandbox {
                                "workspace-write"
                            } else {
                                "danger-full-access"
                            },
                            "--ask-for-approval",
                            if approvals { "on-request" } else { "never" },
                            "--model",
                            "multi word"
                        ]
                    );
                    assert_eq!(settings.launch_arguments.codex, manual);
                    for kind in [HarnessKind::Claude, HarnessKind::OpenCode] {
                        settings.launch_arguments.set(kind, manual.into());
                        assert_eq!(launch_arguments(&settings, kind), manual);
                    }
                }
            }
        }
        settings.launch_arguments.codex = "-- --yolo literal".into();
        assert!(launch_arguments(&settings, HarnessKind::Codex).ends_with("-- --yolo literal"));
        settings.launch_arguments.codex = "--model 'unfinished".into();
        assert!(shell_words::split(&launch_arguments(&settings, HarnessKind::Codex)).is_err());
    }

    #[test]
    fn isolated_configured_prefix_body_launches() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let bin = dir.join("bin with spaces");
        std::fs::create_dir(&bin).unwrap();
        let source = dir.join("harness.c");
        std::fs::write(
            &source,
            include_str!("../../tests/fixtures/native-harness.c"),
        )
        .unwrap();
        assert!(
            std::process::Command::new("/usr/bin/cc")
                .arg(&source)
                .arg("-o")
                .arg(bin.join("codex"))
                .status()
                .unwrap()
                .success()
        );
        for name in ["claude", "opencode"] {
            std::fs::copy(bin.join("codex"), bin.join(name)).unwrap();
        }
        std::fs::write(dir.join(".zshrc"), "dp() { export GUILD_LAUNCH_FIXTURE_WRAPPED=codex; \"$@\"; }\ndp_claude() { export GUILD_LAUNCH_FIXTURE_WRAPPED=claude; \"$@\"; }\ndp_opencode() { export GUILD_LAUNCH_FIXTURE_WRAPPED=opencode; \"$@\"; }\ncodex() { return 77; }\nalias codex='dp codex'\n").unwrap();
        std::fs::write(dir.join("fixture.sb"), "(version 1)\n(allow default)\n(deny network*)\n(allow network-outbound (remote unix-socket))\n(allow network-inbound (local unix-socket))\n(allow network-bind (local unix-socket))\n(deny process-exec (literal \"/usr/bin/security\"))\n").unwrap();
        let output = std::process::Command::new("/usr/bin/sandbox-exec")
            .arg("-f")
            .arg(dir.join("fixture.sb"))
            .arg(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "root::body::tests::configured_prefix_body_launch_fixture",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display()),
            )
            .env("SHELL", "/bin/zsh")
            .env("ZDOTDIR", dir)
            .env("GUILD_STATE_DIR", dir.join("state"))
            .env("GUILD_NO_GLOBAL_HOOKS", "1")
            .env("CODEX_HOME", dir.join("codex-home"))
            .env("CLAUDE_CONFIG_DIR", dir.join("claude-home"))
            .env("XDG_CONFIG_HOME", dir.join("xdg-home"))
            .env("GUILD_LAUNCH_FIXTURE_DIR", dir)
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
    #[ignore = "runs with sandboxed native fake harnesses in isolated_configured_prefix_body_launches"]
    fn configured_prefix_body_launch_fixture(cx: &mut gpui::TestAppContext) {
        use guild_session::{HarnessKind, SessionMode};
        let dir = std::path::PathBuf::from(std::env::var_os("GUILD_LAUNCH_FIXTURE_DIR").unwrap());
        let settings = Settings {
            launch_prefixes: guild_session::settings::LaunchPrefixes {
                claude: "dp_claude".into(),
                codex: "dp".into(),
                opencode: "dp_opencode".into(),
            },
            ..Default::default()
        };
        let mut value = serde_json::to_value(&settings).unwrap();
        value["launch_arguments"] = serde_json::json!({
            "claude": "--model 'glm multi word'",
            "codex": "--model 'glm multi word'",
            "opencode": "--model 'glm multi word'"
        });
        let settings: Settings = serde_json::from_value(value).unwrap();
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            cx.new(|cx| crate::Root::from_state(settings.clone(), Default::default(), cx))
        });
        let hooks = guild_session::Hooks {
            settings: dir.join("hooks.json"),
            statusline_settings: dir.join("statusline.json"),
            socket: dir.join("fixture.sock"),
            user_statusline: None,
            doors: None,
        };
        let prompt = "literal first task ' \" $(touch never) ; Привет";
        let mut invalid = settings.clone();
        invalid.launch_arguments.codex = "--model 'unfinished".into();
        let invalid_session = SessionId::new();
        let broken = cx.update(|cx| {
            root.update(cx, |_, cx| {
                Body::open(
                    invalid_session,
                    Runs::Codex(SessionMode::Terminal),
                    &dir,
                    Some(Conversation::New("invalid-arguments-fixture".into())),
                    &invalid,
                    Default::default(),
                    None,
                    None,
                    None,
                    None,
                    &Default::default(),
                    cx,
                )
            })
        });
        assert!(
            matches!(broken, Body::Broken { reason, .. } if reason.contains("Invalid launch arguments"))
        );
        assert!(!dir.join(format!("{invalid_session}.ready")).exists());
        for (wrapped, sandbox, approvals) in [
            (true, true, true),
            (true, true, false),
            (true, false, true),
            (true, false, false),
            (false, true, true),
            (false, true, false),
            (false, false, true),
            (false, false, false),
        ] {
            let mut settings = settings.clone();
            settings.codex_sandbox = sandbox;
            settings.codex_command_approvals = approvals;
            if !wrapped {
                settings.launch_prefixes = Default::default();
            }
            for kind in [
                HarnessKind::Codex,
                HarnessKind::Claude,
                HarnessKind::OpenCode,
            ] {
                for resume in [false, true] {
                    let mut settings = settings.clone();
                    if resume {
                        settings.launch_arguments.set(kind, "--model glm".into());
                    }
                    if kind == HarnessKind::Codex {
                        settings
                            .launch_arguments
                            .codex
                            .push_str(" --sandbox read-only -a never --yolo");
                    }
                    for first in [None, Some(prompt)] {
                        let session = SessionId::new();
                        let conversation = if resume {
                            Conversation::Resume("body-resume-fixture".into())
                        } else {
                            Conversation::New("body-new-fixture".into())
                        };
                        let runs = match kind {
                            HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
                            HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
                            HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
                        };
                        let body = cx.update(|cx| {
                            root.update(cx, |_, cx| {
                                Body::open(
                                    session,
                                    runs,
                                    &dir,
                                    Some(conversation),
                                    &settings,
                                    Default::default(),
                                    None,
                                    Some(hooks.clone()),
                                    first.map(str::to_owned),
                                    None,
                                    &Default::default(),
                                    cx,
                                )
                            })
                        });
                        let Body::Terminal { view, .. } = body else {
                            panic!("production Body::open did not launch a terminal");
                        };
                        let ready = dir.join(format!("{session}.ready"));
                        let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
                        while !ready.exists() && std::time::Instant::now() < until {
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        let bytes =
                            std::fs::read(dir.join(format!("{session}.argv"))).unwrap_or_default();
                        let argv: Vec<_> = bytes
                            .split(|b| *b == 0)
                            .filter(|b| !b.is_empty())
                            .map(|b| std::str::from_utf8(b).unwrap().to_owned())
                            .collect();
                        let meta = std::fs::read_to_string(dir.join(format!("{session}.meta")))
                            .unwrap_or_default();
                        let fields: Vec<_> = meta.lines().collect();
                        // argv comes from the native child, not a hand-built command.
                        // macOS forbids exec of set-id ps inside this network sandbox;
                        // native replay inspects the real process table externally.
                        let ps = format!("{} 1 {}", fields.first().unwrap_or(&"0"), argv.join(" "));
                        let over = cx.update(|cx| view.update(cx, |view, _| view.let_go()));
                        if let Some(over) = over {
                            over.recv_timeout(std::time::Duration::from_secs(3))
                                .unwrap();
                        }
                        assert!(
                            ready.exists(),
                            "fake harness was not invoked; {kind:?}, resume={resume}, first={first:?}"
                        );
                        assert_eq!(
                            fields[1],
                            if wrapped { kind.program() } else { "" },
                            "configured harness-specific prefix was lost"
                        );
                        assert_eq!(
                            std::path::Path::new(fields[2]).canonicalize().unwrap(),
                            dir.canonicalize().unwrap()
                        );
                        assert_eq!(
                            argv.first().map(String::as_str),
                            Some(kind.program()),
                            "native argv[0] must remain recognizable even when PATH contains spaces"
                        );
                        assert!(
                            !argv
                                .iter()
                                .any(|a| a == "--dangerously-bypass-approvals-and-sandbox")
                        );
                        let offset = if kind == HarnessKind::Codex {
                            assert_eq!(
                                &argv[1..5],
                                &[
                                    "--sandbox",
                                    if sandbox {
                                        "workspace-write"
                                    } else {
                                        "danger-full-access"
                                    },
                                    "--ask-for-approval",
                                    if approvals { "on-request" } else { "never" }
                                ]
                            );
                            4
                        } else {
                            0
                        };
                        assert_eq!(
                            &argv[1 + offset..3 + offset],
                            &["--model", if resume { "glm" } else { "glm multi word" }],
                            "configured arguments must follow the program, before Guild flags"
                        );
                        if let Some(first) = first {
                            assert_eq!(
                                argv.last().map(String::as_str),
                                Some(first),
                                "production open lost the first task prompt"
                            );
                        }
                        if kind == HarnessKind::Codex {
                            assert!(argv.iter().any(|a| a == "--no-daemon"));
                        }
                        if kind == HarnessKind::OpenCode {
                            assert!(argv.iter().any(|a| a == "--standalone"));
                        }
                        if resume {
                            assert!(argv.iter().any(|a| a == "body-resume-fixture"));
                            let held = guild_session::harness::holders_with_parents(
                                &ps,
                                "body-resume-fixture",
                            );
                            assert_eq!(
                                held.len(),
                                1,
                                "live wrapped native resume holder was hidden: {ps}"
                            );
                        }
                        let args = ps.split_whitespace().skip(2).collect::<Vec<_>>().join(" ");
                        assert!(
                            kind.impl_().detect_in_front(&args),
                            "foreground harness was hidden: {ps}"
                        );
                        assert!(!dir.join("never").exists());
                    }
                }
            }
        }
    }

    /// GLD-078's contract, through the production launch: a Codex or
    /// OpenCode tab opened by `Body::open` finds its status door — the hook
    /// block, the plugin — on disk before its process began, in agent
    /// directories that did not exist; and a saved tab whose agent is not on
    /// the launch's `PATH` creates neither directory. The stand-in compares
    /// the door's mtime with its terminal's start, so a write moved after
    /// the spawn fails as surely as one removed.
    #[test]
    fn isolated_launch_writes_the_door_before_the_agent_starts() {
        let fixture = tempfile::tempdir().unwrap();
        let dir = fixture.path();
        let bin = dir.join("bin");
        std::fs::create_dir(&bin).unwrap();
        let source = dir.join("door-harness.c");
        std::fs::write(&source, include_str!("../../tests/fixtures/door-harness.c")).unwrap();
        assert!(
            std::process::Command::new("/usr/bin/cc")
                .arg(&source)
                .arg("-o")
                .arg(bin.join("codex"))
                .status()
                .unwrap()
                .success()
        );
        std::fs::copy(bin.join("codex"), bin.join("opencode")).unwrap();
        std::fs::write(dir.join("fixture.sb"), "(version 1)\n(allow default)\n(deny network*)\n(allow network-outbound (remote unix-socket))\n(allow network-inbound (local unix-socket))\n(allow network-bind (local unix-socket))\n(deny process-exec (literal \"/usr/bin/security\"))\n").unwrap();
        for present in [true, false] {
            // A directory of its own per run: the first run's doors are
            // not the second's absence.
            let case = dir.join(if present { "present" } else { "absent" });
            std::fs::create_dir(&case).unwrap();
            let path = if present {
                format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display())
            } else {
                "/usr/bin:/bin:/usr/sbin:/sbin".to_owned()
            };
            let output = std::process::Command::new("/usr/bin/sandbox-exec")
                .arg("-f")
                .arg(dir.join("fixture.sb"))
                .arg(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "root::body::tests::launch_door_fixture",
                    "--ignored",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env("PATH", path)
                .env("HOME", case.join("home"))
                .env("SHELL", "/bin/zsh")
                .env("ZDOTDIR", &case)
                .env("GUILD_STATE_DIR", case.join("state"))
                .env("GUILD_NO_GLOBAL_HOOKS", "1")
                .env("CODEX_HOME", case.join("codex-home"))
                .env("CLAUDE_CONFIG_DIR", case.join("claude-home"))
                .env("XDG_CONFIG_HOME", case.join("xdg-home"))
                .env("GUILD_LAUNCH_FIXTURE_DIR", &case)
                .env("GUILD_DOOR_FIXTURE_PRESENT", if present { "1" } else { "" })
                .env_remove("GUILD_SOCKET")
                .env_remove("GUILD_SESSION")
                .env_remove("GUILD_HOOK_SOCKET")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "present={present}\n{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }

    #[gpui::test]
    #[ignore = "runs with sandboxed native stand-ins in isolated_launch_writes_the_door_before_the_agent_starts"]
    fn launch_door_fixture(cx: &mut gpui::TestAppContext) {
        use guild_session::{HarnessKind, SessionMode};
        let dir = std::path::PathBuf::from(std::env::var_os("GUILD_LAUNCH_FIXTURE_DIR").unwrap());
        let present = std::env::var_os("GUILD_DOOR_FIXTURE_PRESENT").is_some_and(|v| !v.is_empty());
        let dirs = guild_session::harness::AgentDirs {
            codex: dir.join("codex-home"),
            opencode: dir.join("xdg-home").join("opencode"),
        };
        assert!(!dirs.codex.exists() && !dirs.opencode.exists());
        let settings = Settings::default();
        let root = cx.update(|cx| {
            crate::editor::init(cx);
            cx.new(|cx| crate::Root::from_state(settings.clone(), Default::default(), cx))
        });
        // What a production window hands a launch: the doors named — the
        // flag that keeps them out of this process's start is not read here.
        let hooks = guild_session::Hooks {
            settings: dir.join("hooks.json"),
            statusline_settings: dir.join("statusline.json"),
            socket: dir.join("fixture.sock"),
            user_statusline: None,
            doors: Some(dirs.clone()),
        };
        for kind in [HarnessKind::Codex, HarnessKind::OpenCode] {
            let session = SessionId::new();
            let runs = match kind {
                HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
                _ => Runs::OpenCode(SessionMode::Terminal),
            };
            // A saved tab restored resumes its conversation; a new one starts.
            let conversation = if present {
                Conversation::New("door-fixture".into())
            } else {
                Conversation::Resume("door-fixture-saved".into())
            };
            let body = cx.update(|cx| {
                root.update(cx, |_, cx| {
                    Body::open(
                        session,
                        runs,
                        &dir,
                        Some(conversation),
                        &settings,
                        Default::default(),
                        None,
                        Some(hooks.clone()),
                        None,
                        None,
                        &Default::default(),
                        cx,
                    )
                })
            });
            let door = dir.join(format!("{session}.door"));
            if let Body::Terminal { view, .. } = &body {
                let until = std::time::Instant::now() + std::time::Duration::from_secs(3);
                while present && !door.exists() && std::time::Instant::now() < until {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                let over = cx.update(|cx| view.update(cx, |view, _| view.let_go()));
                if let Some(over) = over {
                    over.recv_timeout(std::time::Duration::from_secs(3))
                        .unwrap();
                }
            }
            if present {
                assert!(
                    matches!(body, Body::Terminal { .. }),
                    "{kind:?} did not launch"
                );
                assert_eq!(
                    std::fs::read_to_string(&door).unwrap_or_default().trim(),
                    "before",
                    "{kind:?}'s door was not on disk before its process began"
                );
            } else {
                assert!(!door.exists(), "{kind:?} ran without being on the PATH");
            }
        }
        if !present {
            assert!(
                !dirs.codex.exists(),
                "a saved Codex tab not on the PATH created Codex's home"
            );
            assert!(
                !dirs.opencode.exists(),
                "a saved OpenCode tab not on the PATH created opencode's config"
            );
        }
    }
}
