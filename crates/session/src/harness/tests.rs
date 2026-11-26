use serde_json::json;

use super::parse_version;
use super::{ClaudeHarness, CodexHarness, Harness, HarnessKind, OpenCodeHarness};
use crate::{PermissionMode, SessionId};

fn id(session: &str) -> SessionId {
    session.parse().unwrap()
}

#[test]
fn all_three_in_stable_order() {
    assert_eq!(
        HarnessKind::ALL,
        [
            HarnessKind::Claude,
            HarnessKind::OpenCode,
            HarnessKind::Codex
        ]
    );
    assert_eq!(HarnessKind::Claude.program(), "claude");
    assert_eq!(HarnessKind::OpenCode.program(), "opencode");
    assert_eq!(HarnessKind::Codex.program(), "codex");
    assert_eq!(HarnessKind::Claude.impl_().kind(), HarnessKind::Claude);
    assert_eq!(HarnessKind::OpenCode.impl_().kind(), HarnessKind::OpenCode);
    assert_eq!(HarnessKind::Codex.impl_().kind(), HarnessKind::Codex);
}

#[test]
fn detecting_claude_in_front() {
    let h = ClaudeHarness;
    assert!(h.detect_in_front("claude"));
    assert!(h.detect_in_front("/opt/homebrew/bin/claude"));
    assert!(h.detect_in_front("node /home/u/.npm/node_modules/@anthropic-ai/claude-code/cli.js"));
    assert!(h.detect_in_front("/Users/person/.super.engineering/bin/claude"));
    assert!(!h.detect_in_front("zsh -l"));
    assert!(!h.detect_in_front("opencode"));
    assert!(!h.detect_in_front("vim"));
}

#[test]
fn detecting_opencode_in_front() {
    let h = OpenCodeHarness;
    assert!(h.detect_in_front("opencode"));
    assert!(h.detect_in_front("/Users/person/.opencode/bin/opencode"));
    assert!(h.detect_in_front("/Users/person/.super.engineering/bin/opencode"));
    assert!(h.detect_in_front("node ~/.opencode/bin/opencode"));
    assert!(!h.detect_in_front("claude"));
    assert!(!h.detect_in_front("codex"));
    assert!(!h.detect_in_front("zsh -l"));
}

#[test]
fn detecting_codex_in_front() {
    let h = CodexHarness;
    assert!(h.detect_in_front("codex"));
    assert!(h.detect_in_front("/opt/homebrew/bin/codex"));
    assert!(h.detect_in_front("/Users/person/.super.engineering/bin/codex"));
    assert!(!h.detect_in_front("claude"));
    assert!(!h.detect_in_front("opencode"));
    assert!(!h.detect_in_front("zsh -l"));
}

#[test]
fn resume_arguments_per_harness() {
    let to_strings = |v: Vec<std::ffi::OsString>| {
        v.into_iter()
            .map(|s| s.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        to_strings(ClaudeHarness.resume_args("abc")),
        ["--resume", "abc"]
    );
    assert_eq!(
        to_strings(OpenCodeHarness.resume_args("abc")),
        ["--session", "abc"]
    );
    assert_eq!(
        to_strings(CodexHarness.resume_args("abc")),
        ["resume", "abc"]
    );
}

#[test]
fn approval_arguments_per_harness() {
    let to_strings = |v: Vec<std::ffi::OsString>| {
        v.into_iter()
            .map(|s| s.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(
        to_strings(ClaudeHarness.approval_args(PermissionMode::Auto)),
        ["--permission-mode", "auto"]
    );
    assert_eq!(
        to_strings(OpenCodeHarness.approval_args(PermissionMode::Auto)),
        Vec::<String>::new()
    );
    assert_eq!(
        to_strings(OpenCodeHarness.approval_args(PermissionMode::Manual)),
        Vec::<String>::new()
    );
    for permission in [
        PermissionMode::Auto,
        PermissionMode::Manual,
        PermissionMode::AcceptEdits,
        PermissionMode::Plan,
        PermissionMode::DontAsk,
        PermissionMode::BypassPermissions,
    ] {
        assert!(CodexHarness.approval_args(permission).is_empty());
        assert!(OpenCodeHarness.approval_args(permission).is_empty());
    }
}

#[test]
fn a_codex_hook_is_the_latch_s_signal_and_its_stop_a_completed_turn() {
    // Only Codex's hooks still mean anything to status: managed live
    // input's latch. Claude and OpenCode report through OSC 7501.
    let session = id("00000000-0000-0000-0000-000000000001");
    let tool = json!({
        "session_id": "00000000-0000-0000-0000-000000000001",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash"
    });
    assert_eq!(
        CodexHarness.status_from_payload(&tool),
        Some((session, crate::status::Signal::ToolStarted))
    );
    let stop =
        json!({"session_id": "00000000-0000-0000-0000-000000000001", "hook_event_name": "Stop"});
    assert_eq!(
        CodexHarness.status_from_payload(&stop),
        Some((session, crate::status::Signal::TurnCompleted))
    );
    assert_eq!(
        CodexHarness.status_from_payload(&json!({"hook_event_name": "something_new"})),
        None
    );
}

#[test]
fn available_on_tells_which_harnesses_a_path_offers() {
    use std::os::unix::fs::PermissionsExt;
    let folder = tempfile::Builder::new()
        .prefix("guild-harness-test-")
        .tempdir()
        .expect("a scratch folder");
    let dir = folder.path();
    for name in ["claude", "opencode"] {
        let file = dir.join(name);
        std::fs::write(&file, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut path = dir.as_os_str().to_owned();
    path.push(std::ffi::OsStr::new(":"));
    let found = HarnessKind::available_on(path.to_str().unwrap());
    assert!(found.contains(&HarnessKind::Claude));
    assert!(found.contains(&HarnessKind::OpenCode));
    assert!(
        !found.contains(&HarnessKind::Codex),
        "codex is not on the made-up PATH"
    );
}

#[test]
fn runs_wire_format_is_additive() {
    use crate::Runs;
    use crate::session_id::SessionMode;
    // A file written before M14 still reads: the `"claude"` name is data.
    let old: Runs = serde_json::from_str(r#"{"claude":"terminal"}"#).unwrap();
    assert_eq!(old, Runs::Claude(SessionMode::Terminal));
    assert!(old.is_agent());
    assert!(old.is_claude());
    // The new harnesses are new paths; their names on disk are snake_case.
    let open: Runs = serde_json::from_str(r#"{"open_code":"terminal"}"#).unwrap();
    assert_eq!(open, Runs::OpenCode(SessionMode::Terminal));
    assert!(open.is_agent());
    assert!(!open.is_claude());
    assert_eq!(open.harness(), Some(HarnessKind::OpenCode));
    let codex: Runs = serde_json::from_str(r#"{"codex":"terminal"}"#).unwrap();
    assert_eq!(codex, Runs::Codex(SessionMode::Terminal));
    assert_eq!(
        serde_json::to_string(&codex).unwrap(),
        r#"{"codex":"terminal"}"#
    );
    // A shell is none of them.
    let shell: Runs = serde_json::from_str(r#""shell""#).unwrap();
    assert!(!shell.is_agent());
    assert_eq!(shell.harness(), None);
}

/// Each CLI prints its version in its own prose; the one thing they share
/// is a number. These are the shapes measured on this machine and the
/// READMEs of the three harnesses.
#[test]
fn the_version_is_read_from_whatever_the_cli_prints() {
    for (prose, expected) in [
        ("opencode v2.0.10", Some((2, 0))),
        ("codex-cli 0.155.1", Some((0, 155))),
        ("2.1.278 (Claude Code)", Some((2, 1))),
        ("v1.9.4", Some((1, 9))),
        ("51", Some((51, 0))),
        ("no version here", None),
        ("", None),
    ] {
        assert_eq!(parse_version(prose.as_bytes()), expected, "{prose:?}");
    }
}

/// OpenCode is the one harness with a floor, and it is V2: the status
/// plugin and the launch flags are the V2 shape, and an opencode before V2
/// would launch but never report status — offered as working when it is not.
#[test]
fn opencode_requires_v2_and_only_opencode_does() {
    assert_eq!(OpenCodeHarness.version_floor(), Some((2, 0)));
    assert_eq!(ClaudeHarness.version_floor(), None);
    assert_eq!(CodexHarness.version_floor(), None);
}

#[test]
fn what_a_claude_command_line_resumes() {
    let h = ClaudeHarness;
    let id = "493409f7-1111-2222-3333-444444444444";
    for args in [
        format!("claude --resume {id}"),
        format!("claude -r {id}"),
        format!("claude --resume={id}"),
        format!("/Users/p/.local/bin/claude --model opus --resume {id}"),
        format!("node /n/@anthropic-ai/claude-code/cli.js --resume {id}"),
        format!("/bin/bash /Users/p/.super.engineering/bin/claude --resume {id}"),
    ] {
        assert_eq!(h.resumed_in(&args).as_deref(), Some(id), "{args}");
    }
    for args in [
        "claude",
        "claude --resume",
        "claude --resume --model opus",
        "claude --continue",
        "claude --session-id 493409f7-1111-2222-3333-444444444444",
        "grep --resume 493409f7",
    ] {
        assert_eq!(h.resumed_in(args), None, "{args}");
    }
}

#[test]
fn what_an_opencode_command_line_resumes() {
    let h = OpenCodeHarness;
    assert_eq!(
        h.resumed_in("opencode --session ses_1").as_deref(),
        Some("ses_1")
    );
    assert_eq!(h.resumed_in("opencode -s ses_1").as_deref(), Some("ses_1"));
    assert_eq!(
        h.resumed_in("opencode --session=ses_1").as_deref(),
        Some("ses_1")
    );
    assert_eq!(h.resumed_in("opencode --continue"), None);
    assert_eq!(h.resumed_in("claude --session ses_1"), None);
}

#[test]
fn what_a_codex_command_line_resumes() {
    let h = CodexHarness;
    let id = "01a0c0cb-f105-73d0-a0d3-bb9faf592187";
    assert_eq!(
        h.resumed_in(&format!("codex resume {id}")).as_deref(),
        Some(id)
    );
    assert_eq!(
        h.resumed_in(&format!("/opt/homebrew/bin/codex resume {id}"))
            .as_deref(),
        Some(id)
    );
    assert_eq!(h.resumed_in("codex resume --last"), None);
    assert_eq!(h.resumed_in("codex resume"), None);
    assert_eq!(
        h.resumed_in(&format!("codex {id}")),
        None,
        "a prompt, not a resume"
    );
    assert_eq!(h.resumed_in(&format!("claude resume {id}")), None);
}

#[test]
fn a_claude_started_with_an_id_holds_it_too() {
    let h = ClaudeHarness;
    let id = "493409f7-1111-2222-3333-444444444444";
    assert!(h.holds(&format!("claude --session-id {id}"), id));
    assert!(h.holds(&format!("claude --resume {id}"), id));
    assert!(!h.holds(&format!("claude --resume {id}"), "another"));
    assert!(!h.holds(&format!("grep --session-id {id}"), id));
}

/// A holder's parent is read too — M13.3: a tab just closed leaves its
/// `claude` alive for up to a second of hangup grace, a child of the window
/// no tab holds, and `session resume` waits for it rather than refusing.
#[test]
fn holders_are_read_with_their_parents() {
    let id = "493409f7-1111-2222-3333-444444444444";
    let ps = format!(
        "    1     0 /sbin/launchd\n\
         4242   900 claude --resume {id} --permission-mode auto\n\
         4243   900 zsh -l\n\
         4300   900 codex resume 01a0c0cb\n\
         4301  4300 claude --session-id {id}\n\
         bad line\n"
    );
    assert_eq!(
        super::holders_with_parents(&ps, id),
        vec![
            (
                4242,
                900,
                format!("claude --resume {id} --permission-mode auto")
            ),
            (4301, 4300, format!("claude --session-id {id}")),
        ]
    );
    assert!(super::holders_with_parents(&ps, "nobody").is_empty());
}
