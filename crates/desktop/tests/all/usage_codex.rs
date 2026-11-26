//! Codex's plan limits — M12.12, decision 47. The answer is task 0's,
//! recorded on 2026-09-25 from `codex-cli 0.157.0` with the account's
//! and the reset credit's ids taken out; the exchange is driven against
//! stand-in scripts, never the real `codex`.

use guild_desktop::usage::{WindowName, codex};

const ANSWER: &str = include_str!("../fixtures/codex-rate-limits.json");

/// `primary` is the five-hour window and `secondary` the week, named by
/// their length; `usedPercent` is the share used.
#[test]
fn the_two_windows_are_read_by_their_length() {
    let windows = codex::windows(ANSWER).expect("readable");

    let names: Vec<WindowName> = windows.iter().map(|w| w.name.clone()).collect();
    assert_eq!(names, [WindowName::Session, WindowName::Week]);
    let left: Vec<u8> = windows.iter().map(|w| w.left).collect();
    assert_eq!(left, [100, 0]);
    assert_eq!(
        windows[0].resets_at,
        Some(time::OffsetDateTime::from_unix_timestamp(1790372488).unwrap())
    );
}

/// A window of another length is named by it, rather than dropped.
#[test]
fn a_window_of_another_length_is_a_span() {
    let line = r#"{"id":1,"result":{"rateLimits":{
        "primary":{"usedPercent":12.6,"windowDurationMins":480,"resetsAt":null},
        "secondary":{"usedPercent":140,"windowDurationMins":2880}}}}"#;

    let windows = codex::windows(line).expect("readable");

    assert_eq!(windows[0].name, WindowName::Span(480));
    assert_eq!(windows[0].left, 87, "rounded");
    assert_eq!(windows[0].resets_at, None);
    assert_eq!(windows[1].name, WindowName::Span(2880));
    assert_eq!(windows[1].left, 0, "clamped at nothing left");
}

/// An account with no windows to report — both `null`, as the session
/// logs showed once — is no windows, not a failure.
#[test]
fn both_windows_null_is_no_windows() {
    let line =
        r#"{"id":1,"result":{"rateLimits":{"limitId":"premium","primary":null,"secondary":null}}}"#;

    assert_eq!(codex::windows(line), Ok(vec![]));
}

#[test]
fn an_answer_that_is_not_the_form_is_unreadable() {
    assert!(codex::windows("not json").is_err());
    assert!(
        codex::windows(r#"{"id":1,"error":{"code":-32600,"message":"not signed in"}}"#).is_err()
    );
    assert!(codex::windows(r#"{"id":1,"result":{}}"#).is_err());
}

use std::ffi::OsStr;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::scratch::Scratch;

use crate::child_lifetime;

/// A stand-in for `codex`: checks it was asked for the app server and
/// read the three lines, then runs `body`.
fn stand_in(dir: &Scratch, body: &str) -> PathBuf {
    let script = dir.join("codex");
    let text = format!(
        "#!/bin/sh\n\
         [ \"$1\" = app-server ] || exit 3\n\
         read hello; read ready; read ask\n\
         case \"$ask\" in *account/rateLimits/read*) ;; *) exit 4 ;; esac\n\
         {body}\n"
    );
    std::fs::write(&script, text).unwrap();
    std::fs::set_permissions(&script, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    script
}

/// What the real one says before its answer — the `initialize` answer,
/// a notification — and a line that is not JSON: all passed over.
#[test]
fn the_answer_is_read_past_what_comes_before_it() {
    let dir = Scratch::new("codex-answer-");
    let mut descendant = child_lifetime::Descendant::new(&dir);
    let answer = dir.join("answer.json");
    std::fs::write(&answer, ANSWER).unwrap();
    let body = format!(
        "{}echo '{{\"id\":0,\"result\":{{\"userAgent\":\"stand-in\"}}}}'\n\
         echo '{{\"method\":\"remoteControl/status/changed\",\"params\":{{}}}}'\n\
         echo 'not json'\n\
         cat '{}'; echo\n\
         sleep 30",
        descendant.shell(),
        answer.display()
    );
    let codex = stand_in(&dir, &body);

    let windows = codex::ask(codex.as_os_str(), None, Duration::from_secs(10)).expect("answered");

    assert_eq!(windows.len(), 2);
    assert_eq!(windows[1].left, 0);
    descendant.assert_stopped();
}

/// An `error` answer is a failed request in Codex's words.
#[test]
fn an_error_answer_is_a_failed_request() {
    let dir = Scratch::new("codex-error-");
    let codex = stand_in(
        &dir,
        "echo '{\"id\":1,\"error\":{\"code\":-32600,\"message\":\"not signed in\"}}'",
    );

    let outcome = codex::ask(codex.as_os_str(), None, Duration::from_secs(10));

    assert_eq!(
        outcome,
        Err(codex::Failure::Request("not signed in".into()))
    );
}

/// Silence reaches the request deadline, even when the process has not yet
/// finished starting. Descendant ownership is checked by the ready fixtures in
/// the answer and early-exit tests, without assuming startup fits this deadline.
#[test]
fn a_silent_codex_is_given_up_on_at_the_deadline() {
    let dir = Scratch::new("codex-silent-");
    let codex = stand_in(&dir, "exec /bin/sleep 60");

    let started = Instant::now();
    let deadline = Duration::from_millis(100);
    let outcome = codex::ask(codex.as_os_str(), None, deadline);

    assert_eq!(
        outcome,
        Err(codex::Failure::Request(format!(
            "no answer in {deadline:?}"
        )))
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the deadline must stop the request rather than wait for the sleeper"
    );
}

/// One that exits before answering is a failed request, not a wait.
#[test]
fn a_codex_that_exits_before_answering_is_a_failed_request() {
    let dir = Scratch::new("codex-exit-");
    let codex = stand_in(&dir, "exit 0");

    let started = Instant::now();
    let outcome = codex::ask(codex.as_os_str(), None, Duration::from_secs(10));

    assert!(
        matches!(outcome, Err(codex::Failure::Request(_))),
        "{outcome:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// No `codex` to run is not installed — which stops the asking.
#[test]
fn no_codex_is_not_installed() {
    let dir = Scratch::new("codex-none-");
    let missing = dir.join("codex");

    let outcome = codex::ask(missing.as_os_str(), None, Duration::from_secs(1));
    assert_eq!(outcome, Err(codex::Failure::NotInstalled));

    let on_path = codex::ask(
        OsStr::new("codex"),
        Some(dir.as_os_str()),
        Duration::from_secs(1),
    );
    assert_eq!(
        on_path,
        Err(codex::Failure::NotInstalled),
        "looked up on the PATH given"
    );
}

/// An early exit must still stop its already-running descendant. EOF on the
/// lifetime pipe proves termination without depending on OS zombie reaping.
#[test]
fn what_an_early_exit_leaves_behind_is_killed() {
    let dir = Scratch::new("codex-orphan-");
    let mut descendant = child_lifetime::Descendant::new(&dir);
    let codex = stand_in(&dir, &format!("{}exit 0", descendant.shell()));

    let outcome = codex::ask(codex.as_os_str(), None, Duration::from_secs(10));

    assert_eq!(
        outcome,
        Err(codex::Failure::Request(
            "codex exited before answering".into()
        ))
    );
    descendant.assert_stopped();
}

/// The `rate_limits` of a `token_count` line in a rollout — snake_case,
/// unlike the app server's answer; the week alone on a Pro plan as
/// recorded on 2026-10-03, `secondary` null.
#[test]
fn a_rollouts_rate_limits_are_read_by_their_length() {
    let limits: serde_json::Value = serde_json::from_str(
        r#"{"limit_id":"codex","limit_name":null,
            "primary":{"used_percent":11.0,"window_minutes":10080,"resets_at":1791580401},
            "secondary":null,"credits":{"has_credits":true},"plan_type":"pro"}"#,
    )
    .unwrap();

    let windows = codex::rollout(&limits).expect("readable");

    let names: Vec<WindowName> = windows.iter().map(|w| w.name.clone()).collect();
    assert_eq!(names, [WindowName::Week]);
    assert_eq!(windows[0].left, 89);
    assert_eq!(
        windows[0].resets_at,
        Some(time::OffsetDateTime::from_unix_timestamp(1791580401).unwrap())
    );
}

#[test]
fn a_rollouts_rate_limits_without_a_window_give_nothing() {
    let limits: serde_json::Value =
        serde_json::from_str(r#"{"primary":null,"secondary":null}"#).unwrap();
    assert_eq!(codex::rollout(&limits), None);
}
