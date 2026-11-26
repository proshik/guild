use super::*;
use std::time::{Duration, Instant};

#[test]
fn isolated_real_harness_failure_and_retry() {
    let fixture = tempfile::tempdir().unwrap();
    let dir = fixture.path();
    let source = dir.join("fixture.c");
    std::fs::write(
        &source,
        include_str!("../../../tests/fixtures/harness-failure.c"),
    )
    .unwrap();
    let fake = dir.join("harness-fixture");
    assert!(
        std::process::Command::new("/usr/bin/cc")
            .arg(&source)
            .arg("-o")
            .arg(&fake)
            .status()
            .unwrap()
            .success()
    );
    let holder_source = dir.join("holder.c");
    std::fs::write(
        &holder_source,
        r#"#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
int main(void) {
    const char *path = getenv("HOLDER_ROLLOUT");
    FILE *held = path ? fopen(path, "r") : NULL;
    if (path && !held) return 3;
    FILE *ready = fopen(getenv("HOLDER_READY"), "w");
    if (!ready) return 4;
    fputs("ready", ready);
    fclose(ready);
    for (;;) pause();
}
"#,
    )
    .unwrap();
    let bin = dir.join("holders");
    std::fs::create_dir(&bin).unwrap();
    assert!(
        std::process::Command::new("/usr/bin/cc")
            .arg(&holder_source)
            .arg("-o")
            .arg(bin.join("opencode"))
            .status()
            .unwrap()
            .success()
    );
    std::fs::copy(bin.join("opencode"), bin.join("rollout-holder")).unwrap();
    std::fs::write(dir.join(".zshrc"), "").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "root::harness_retry::runtime_tests::runtime_fixture",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GUILD_FAILURE_FIXTURE", dir)
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .env("GUILD_STATE_DIR", dir.join("state"))
        .env("SHELL", "/bin/zsh")
        .env("ZDOTDIR", dir)
        .env("CODEX_HOME", dir.join("codex"))
        .env("CLAUDE_CONFIG_DIR", dir.join("claude"))
        .env("XDG_CONFIG_HOME", dir.join("xdg"))
        .env_remove("OPENCODE_CONFIG_CONTENT")
        .output()
        .unwrap();
    println!("{}", String::from_utf8_lossy(&output.stdout));
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[gpui::test]
#[ignore = "runs only in isolated_real_harness_failure_and_retry with private executable/state"]
fn runtime_fixture(cx: &mut gpui::TestAppContext) {
    use gpui::AppContext;
    use guild_session::{Desk, HarnessKind, Runs, Seat, SessionMode};
    cx.executor().allow_parking();
    let dir = std::path::PathBuf::from(std::env::var_os("GUILD_FAILURE_FIXTURE").unwrap());
    let mut entity = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        crate::editor::init(cx);
        let mut desk = Desk::default();
        let project = desk.open_in(desk.active_workspace(), &dir);
        desk.look_at(Some(project), None);
        let root = cx.new(|cx| {
            let mut root = Root::diagnostic(desk, dir.join("state"), cx);
            root.attach_test_hooks(&dir.join("hooks"));
            root
        });
        entity = Some(root.clone());
        gpui_component::Root::new(root, window, cx)
    });
    let root = entity.unwrap();
    for (kind, mode, sandbox, approvals) in [
        (HarnessKind::Claude, "new"),
        (HarnessKind::Codex, "unknown"),
        (HarnessKind::OpenCode, "unknown"),
        (HarnessKind::Codex, "resume"),
        (HarnessKind::OpenCode, "resume"),
        (HarnessKind::OpenCode, "managed"),
        (HarnessKind::Claude, "stale"),
        (HarnessKind::Claude, "broken"),
        (HarnessKind::OpenCode, "external-holder"),
        (HarnessKind::Codex, "rollout-holder"),
    ]
    .into_iter()
    .flat_map(|(kind, mode)| {
        [(true, false), (true, true), (false, true), (false, false)]
            .map(move |(sandbox, approvals)| (kind, mode, sandbox, approvals))
    }) {
        if std::env::var_os("GUILD_RETRY_ONLY_HOLDERS").is_some() && !mode.ends_with("holder") {
            continue;
        }
        if std::env::var("GUILD_RETRY_ONLY_HARNESS").is_ok_and(|program| program != kind.program())
        {
            continue;
        }
        let (tab, pane, session, old_view, old_facts, record) = cx.update(|_, cx| {
            root.update(cx, |root, cx| {
                root.settings.theme_mode = Some(guild_session::ThemeMode::Light);
                root.theme = crate::theme::Theme::light();
                root.settings.terminal_font = Some("JetBrains Mono".into());
                root.settings.terminal_font_size = 12.;
                root.settings.language = guild_session::Language::English;
                root.settings.copy_on_select = false;
                root.settings.adaptive_terminal_refresh = false;
                root.settings.permission_mode = guild_session::PermissionMode::Manual;
                root.settings
                    .launch_arguments
                    .set(kind, "--fixture-argument 'captured value'".into());
                root.settings.codex_sandbox = !sandbox;
                root.settings.codex_command_approvals = !approvals;
                root.settings
                    .launch_prefixes
                    .set(kind, dir.join("harness-fixture").display().to_string());
                let project = root.active_project.unwrap();
                let seat = Seat {
                    workspace: root.desk.active_workspace(),
                    project,
                };
                let runs = match kind {
                    HarnessKind::Claude => Runs::Claude(SessionMode::Terminal),
                    HarnessKind::Codex => Runs::Codex(SessionMode::Terminal),
                    HarnessKind::OpenCode => Runs::OpenCode(SessionMode::Terminal),
                };
                let mut record =
                    SessionRecord::new("runtime fixture".into(), seat, dir.clone(), runs);
                record.cost_usd = 1.25;
                record.usage.input_tokens = 17;
                record.usage.cost_usd = Some(1.25);
                record.transcript_offset = 42;
                record.had_turn = true;
                record.launch_selection = Some(guild_session::harness::LaunchSelection {
                    model: Some(
                        if kind == HarnessKind::OpenCode {
                            "fixture/model"
                        } else {
                            "fixture-model"
                        }
                        .into(),
                    ),
                    effort: Some("high".into()),
                });
                record.task = kind == HarnessKind::Claude || mode == "resume" || mode == "managed";
                if mode == "resume" || mode.ends_with("holder") {
                    let id = guild_session::SessionId::new().to_string();
                    let home = guild_session::harness::CodexHarness::home();
                    let sessions = home.join("sessions/2026/10/23");
                    std::fs::create_dir_all(&sessions).unwrap();
                    std::fs::write(
                        sessions.join(format!("rollout-{id}.jsonl")),
                        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n"),
                    )
                    .unwrap();
                    record.harness_session = Some(id);
                }
                let session = root.desk.add_session(record.clone());
                let body = Body::open(
                    session,
                    runs,
                    &dir,
                    Some(record.harness_session.clone().map_or_else(
                        || Conversation::New(session.to_string()),
                        Conversation::Resume,
                    )),
                    &root.settings,
                    root.theme,
                    None,
                    root.hook_launch(),
                    Some("fixture initial assignment; NEVER REPLAY".into()),
                    record.launch_selection.as_ref(),
                    &root.start_size,
                    cx,
                );
                let Body::Terminal { view, facts, .. } = &body else {
                    panic!("fixture did not launch")
                };
                let (view, facts) = (view.clone(), facts.clone());
                let tab = root.adopt(seat, project, body, cx);
                let pane = root
                    .tabs
                    .iter()
                    .find(|open| open.id == tab)
                    .unwrap()
                    .panes
                    .focused();
                root.active_tab = Some(tab);
                // The live view follows the new theme, but its launch snapshot
                // deliberately still describes the original Light launch.
                root.set_theme_mode(guild_session::ThemeMode::Dark, cx);
                (tab, pane, session, view, facts, record)
            })
        });
        let until = Instant::now() + Duration::from_secs(5);
        while !old_facts.exited() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(10));
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
        }
        let status = cx.update(|_, cx| old_view.update(cx, |view, _| view.exit_status()));
        assert_eq!(
            old_facts.failure().map(|f| (f.harness, f.code, f.network)),
            Some((kind, 7, true)),
            "exited={}, status={status:?}, attempts={:?}",
            old_facts.exited(),
            std::fs::read_to_string(dir.join(format!("{session}.attempts")))
        );
        assert_eq!(
            old_facts.launch_settings().unwrap().theme_mode,
            Some(guild_session::ThemeMode::Light)
        );
        assert_eq!(
            old_view.read_with(cx, |view, _| view.colors_for_test().0.kind),
            guild_session::ThemeKind::Dark
        );
        let seed = match std::env::var("GUILD_RETRY_OLD_STATUS").as_deref() {
            Ok("waiting") => Some(guild_session::status::Reported::Blocked(Some(
                guild_session::status::Kind::Permission,
            ))),
            Ok("failed") => Some(guild_session::status::Reported::Error),
            Ok("working") => Some(guild_session::status::Reported::Working),
            _ => match kind {
                HarnessKind::Claude => Some(guild_session::status::Reported::Working),
                HarnessKind::Codex => Some(guild_session::status::Reported::Blocked(Some(
                    guild_session::status::Kind::Permission,
                ))),
                HarnessKind::OpenCode => Some(guild_session::status::Reported::Error),
            },
        };
        let old_status = cx.update(|_, cx| {
            root.update(cx, |root, _| {
                root.board.report(session, seed);
                root.board.observed_status(session)
            })
        });
        if mode == "broken" {
            let mut settings = old_facts.launch_settings().unwrap();
            settings.launch_arguments.claude = "'unfinished".into();
            old_facts.set_launch_settings(settings);
        }
        let _holder = if mode.ends_with("holder") {
            Some(start_holder(
                &dir,
                mode,
                record.harness_session.as_deref().unwrap(),
            ))
        } else {
            None
        };
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                root.settings.codex_sandbox = sandbox;
                root.settings.codex_command_approvals = approvals;
                root.settings.permission_mode = guild_session::PermissionMode::Plan;
                root.settings
                    .launch_arguments
                    .set(kind, "--fixture-argument 'new global value'".into());
                root.settings
                    .launch_prefixes
                    .set(kind, "/missing-current-default".into());
                root.settings.terminal_font = Some("Lilex".into());
                root.settings.terminal_font_size = 15.;
                root.settings.language = guild_session::Language::Russian;
                root.settings.copy_on_select = true;
                root.settings.adaptive_terminal_refresh = true;
                root.retry_harness(tab, pane, window, cx);
                root.retry_harness(tab, pane, window, cx); // no concurrent admission
                if sandbox == approvals {
                    // Replacement cannot run until this UI update yields, even
                    // if background preparation finishes immediately.
                    assert!(old_facts.retrying());
                    root.set_theme_mode(guild_session::ThemeMode::Light, cx);
                    root.settings.terminal_font_size = 17.;
                }
                if mode == "stale" {
                    root.desk.rename_session(session, "changed while admitting");
                }
            })
        });
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
        if mode == "managed" || mode == "stale" || mode == "broken" || mode.ends_with("holder") {
            let until = Instant::now() + Duration::from_secs(5);
            while old_facts.retrying() && Instant::now() < until {
                std::thread::sleep(Duration::from_millis(10));
                cx.executor().advance_clock(Duration::from_millis(20));
                cx.run_until_parked();
            }
            cx.update(|_, cx| {
                root.update(cx, |root, cx| {
                    assert_eq!(
                        root.board.observed_status(session),
                        old_status,
                        "refusal must preserve old board"
                    );
                    let Body::Terminal { view, .. } = root
                        .tabs
                        .iter()
                        .find(|open| open.id == tab)
                        .unwrap()
                        .panes
                        .body_of(pane)
                        .unwrap()
                    else {
                        panic!()
                    };
                    assert_eq!(view.entity_id(), old_view.entity_id());
                    assert!(view.read(cx).has_text());
                })
            });
            assert!(!cx.has_pending_prompt());
            assert!(!old_facts.retrying());
            assert!(old_facts.failure().is_some());
            assert_eq!(
                std::fs::read_to_string(dir.join(format!("{session}.attempts"))).unwrap(),
                "1"
            );
            println!(
                "fixture PASS {kind:?}/{mode}: safe refusal, original runtime/output retained"
            );
            continue;
        }
        if mode == "unknown" {
            assert!(cx.has_pending_prompt());
            let ui = root.read_with(cx, |root, _| root.ui());
            cx.simulate_prompt_answer(ui.text(Text::Cancel));
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
            assert!(old_facts.failure().is_some());
            assert_eq!(
                std::fs::read_to_string(dir.join(format!("{session}.attempts"))).unwrap(),
                "1"
            );
            cx.update(|window, cx| {
                root.update(cx, |root, cx| root.retry_harness(tab, pane, window, cx))
            });
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
            cx.simulate_prompt_answer(ui.text(Text::HarnessRetry));
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
        }
        let until = Instant::now() + Duration::from_secs(5);
        let count = dir.join(format!("{session}.attempts"));
        while std::fs::read_to_string(&count).ok().as_deref() != Some("2") && Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(10));
            cx.executor().advance_clock(Duration::from_millis(20));
            cx.run_until_parked();
        }
        assert_eq!(std::fs::read_to_string(&count).unwrap(), "2");
        let capture = dir.join(format!("{session}.2.argv"));
        while std::fs::read_to_string(&capture)
            .ok()
            .is_none_or(|s| !s.contains("session="))
            && Instant::now() < until
        {
            std::thread::sleep(Duration::from_millis(10));
        }
        let argv = std::fs::read_to_string(capture).unwrap();
        assert!(!argv.contains("NEVER REPLAY"));
        assert!(argv.lines().any(|line| line == "captured value"));
        assert!(!argv.contains("new global value"));
        assert!(argv.contains("high"));
        if kind != HarnessKind::OpenCode {
            assert!(argv.lines().any(|line| line == "fixture-model"));
        } else {
            let config = argv
                .lines()
                .find_map(|line| line.strip_prefix("opencode_config="))
                .unwrap();
            let config: serde_json::Value = serde_json::from_str(config).unwrap();
            assert_eq!(config["model"], "fixture/model#high");
        }
        assert!(!argv.contains("--dangerously-bypass-approvals-and-sandbox"));
        let args: Vec<_> = argv.lines().collect();
        if kind == HarnessKind::Codex {
            let at = args.iter().position(|a| *a == "--sandbox").unwrap();
            assert_eq!(
                &args[at..at + 4],
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
        } else {
            assert!(!args.contains(&"--sandbox"));
            assert!(!args.contains(&"--ask-for-approval"));
        }
        if mode == "resume" {
            assert!(argv.lines().any(|line| line
                == if kind == HarnessKind::Codex {
                    "resume"
                } else {
                    "--session"
                }));
            assert!(
                argv.lines()
                    .any(|line| Some(line) == record.harness_session.as_deref())
            );
        }
        assert!(argv.contains(&format!("session={session}")));
        let actual_cwd = argv
            .lines()
            .find_map(|line| line.strip_prefix("cwd="))
            .unwrap();
        assert_eq!(
            std::path::Path::new(actual_cwd).canonicalize().unwrap(),
            dir.canonicalize().unwrap()
        );
        cx.update(|window, cx| {
            root.update(cx, |root, cx| {
                let current = root.desk.session(session).unwrap();
                assert_eq!(
                    root.board.observed_status(session),
                    "unknown",
                    "replacement inherited old {old_status} state"
                );
                assert!(
                    !guild_session::command::settled(
                        root.board.observed_status(session),
                        current.had_turn
                    ),
                    "agent wait must not finish on old status"
                );
                let refusal = guild_session::command::refuses_send(
                    true,
                    false,
                    root.board.observed_status(session),
                    current.had_turn,
                )
                .unwrap();
                assert!(
                    refusal.contains("status unavailable"),
                    "agent send must use new unknown status: {refusal}"
                );
                assert_eq!(
                    current, &record,
                    "Retry must preserve the whole saved session record"
                );
                assert_eq!(current.task, record.task);
                assert_eq!(current.cwd, record.cwd);
                assert_eq!(current.workspace, record.workspace);
                assert_eq!(current.project, record.project);
                let Body::Terminal { view, facts, .. } = root
                    .tabs
                    .iter()
                    .find(|open| open.id == tab)
                    .unwrap()
                    .panes
                    .body_of(pane)
                    .unwrap()
                else {
                    panic!()
                };
                assert_ne!(view.entity_id(), old_view.entity_id());
                assert!(facts.failure().is_none());
                let mut command = portable_pty::CommandBuilder::new("/usr/bin/true");
                command.env_remove("GUILD_SESSION");
                let fresh = crate::terminal::view::open_command(
                    command,
                    &dir,
                    crate::terminal::view::Look::of(&root.settings, root.theme),
                    None,
                    cx,
                ).unwrap();
                let retried_colors = view.read(cx).colors_for_test();
                let fresh_colors = fresh.read(cx).colors_for_test();
                assert_eq!(retried_colors.2, fresh_colors.2, "{kind:?}/{mode}: Retry background must match a fresh current-theme terminal");
                assert_eq!(retried_colors.1, fresh_colors.1, "{kind:?}/{mode}: Retry foreground must match a fresh current-theme terminal");
                assert_eq!(retried_colors.0, fresh_colors.0);
                fresh.update(cx, |view, _| view.let_go());
                let launched = facts.launch_settings().unwrap();
                let captured = old_facts.launch_settings().unwrap();
                let mut expected = captured;
                expected.codex_sandbox = sandbox;
                expected.codex_command_approvals = approvals;
                expected.theme_mode = root.settings.theme_mode;
                expected.theme_custom = root.settings.theme_custom;
                expected.terminal_font = root.settings.terminal_font.clone();
                expected.terminal_font_size = root.settings.terminal_font_size;
                expected.language = root.settings.language;
                expected.copy_on_select = root.settings.copy_on_select;
                expected.adaptive_terminal_refresh = root.settings.adaptive_terminal_refresh;
                assert_eq!(launched, expected, "{kind:?}/{mode}: only current presentation and existing Codex policy overrides replace captured settings");
                // A live/stale action cannot launch a third attempt.
                root.retry_harness(tab, pane, window, cx);
            })
        });
        println!(
            "fixture PASS {kind:?}/{mode}: exit7/network, same pane/settings/cwd/session/task, no prompt replay, one replacement"
        );
        if let Some(evidence) = std::env::var_os("GUILD_FAILURE_EVIDENCE_DIR") {
            let evidence = std::path::PathBuf::from(evidence);
            std::fs::create_dir_all(&evidence).unwrap();
            for attempt in [1, 2] {
                std::fs::copy(
                    dir.join(format!("{session}.{attempt}.argv")),
                    evidence.join(format!("{kind:?}-{mode}-{attempt}.argv")),
                )
                .unwrap();
            }
        }
    }
    cx.update(|_, cx| root.update(cx, |root, cx| root.stop_every_process(cx)));
    guild_session::flush();
}

struct Holder(std::process::Child);
impl Drop for Holder {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn start_holder(dir: &std::path::Path, mode: &str, id: &str) -> Holder {
    let ready = dir.join(format!("{id}.holder-ready"));
    let mut command =
        std::process::Command::new(dir.join("holders").join(if mode == "external-holder" {
            "opencode"
        } else {
            "rollout-holder"
        }));
    command.env("HOLDER_READY", &ready);
    if mode == "external-holder" {
        command.args(["--session", id]);
    } else {
        let rollout = guild_session::codex_transcript::path(
            &guild_session::harness::CodexHarness::home(),
            id,
        )
        .unwrap();
        command.env("HOLDER_ROLLOUT", rollout);
    }
    let holder = Holder(command.spawn().unwrap());
    let until = Instant::now() + Duration::from_secs(3);
    while std::fs::read_to_string(&ready).ok().as_deref() != Some("ready") {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let ps = std::process::Command::new("/bin/ps")
        .args(["-p", &holder.0.id().to_string(), "-o", "args="])
        .output()
        .unwrap();
    let argv = String::from_utf8(ps.stdout).unwrap();
    assert!(ps.status.success());
    assert_eq!(
        argv.contains(id),
        mode == "external-holder",
        "rollout holder must not advertise ID in argv: {argv}"
    );
    println!(
        "holder input mode={mode} pid={} argv={} conversation={id}",
        holder.0.id(),
        argv.trim()
    );
    holder
}
