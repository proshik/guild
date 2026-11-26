//! Codex's plan limits — M12.12, decision 47 of the spec.
//!
//! Read through `codex app-server`, the person's choice: Codex's own
//! JSON-RPC over stdio, so Guild never
//! holds Codex's token — Codex refreshes it and makes the request. Task 0
//! checked it on `codex-cli 0.157.0`: `initialize`, `initialized`, then
//! `account/rateLimits/read`, answered in about a second.
//!
//! Only `rateLimits.primary` and `.secondary` are read. The answer also
//! carries credits, the plan, free resets and a banner; a second limit id
//! under `rateLimitsByLimitId` is a pass of its own when one appears.

use std::ffi::OsStr;
use std::io::{BufRead as _, BufReader, Write as _};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::Value;
use time::OffsetDateTime;

use super::{Unreadable, Window, WindowName, left_of};

/// The windows of the answer to `account/rateLimits/read` — the whole
/// line, `{"id":1,"result":{"rateLimits":{…}}}`. A window that is `null`
/// is left out; an answer without `rateLimits` is [`Unreadable`].
pub fn windows(line: &str) -> Result<Vec<Window>, Unreadable> {
    let line: Value = serde_json::from_str(line).map_err(|_| Unreadable)?;
    let limits = line
        .get("result")
        .and_then(|result| result.get("rateLimits"))
        .filter(|limits| limits.is_object())
        .ok_or(Unreadable)?;
    Ok(["primary", "secondary"]
        .into_iter()
        .filter_map(|key| window(limits.get(key)?))
        .collect())
}

/// The windows of a rollout's `rate_limits` — what Codex writes beside
/// every `token_count` it records in `~/.codex/sessions`, the same two
/// windows as the app server's answer in snake_case:
/// `{used_percent, window_minutes, resets_at}`. `None` when neither is
/// there.
pub fn rollout(limits: &Value) -> Option<Vec<Window>> {
    let windows: Vec<Window> = ["primary", "secondary"]
        .into_iter()
        .filter_map(|key| {
            let entry = limits.get(key)?;
            read(
                entry.get("used_percent")?,
                entry.get("window_minutes")?,
                entry.get("resets_at"),
            )
        })
        .collect();
    (!windows.is_empty()).then_some(windows)
}

/// One window: `{usedPercent, windowDurationMins, resetsAt}`, named by
/// its length.
fn window(entry: &Value) -> Option<Window> {
    read(
        entry.get("usedPercent")?,
        entry.get("windowDurationMins")?,
        entry.get("resetsAt"),
    )
}

/// A window from its three fields, whichever case they were spelled in.
fn read(used: &Value, minutes: &Value, resets_at: Option<&Value>) -> Option<Window> {
    let used = used.as_f64()?;
    let name = match minutes.as_u64()? {
        300 => WindowName::Session,
        10080 => WindowName::Week,
        other => WindowName::Span(u32::try_from(other).ok()?),
    };
    let resets_at = resets_at
        .and_then(Value::as_i64)
        .and_then(|at| OffsetDateTime::from_unix_timestamp(at).ok());
    Some(Window {
        name,
        left: left_of(used),
        resets_at,
    })
}

/// Why Codex's figures could not be had.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// No `codex` to run: the asking stops until Refresh.
    NotInstalled,
    /// It ran and gave no windows: an `error` answer in its words, the
    /// deadline, an exit before the answer.
    Request(String),
    /// The answer was not the form.
    Unreadable,
}

/// The id the question is asked under.
const ASK: u64 = 1;

/// What Guild says, a line each: the handshake, then the question.
fn said() -> [String; 3] {
    [
        format!(
            r#"{{"id":0,"method":"initialize","params":{{"clientInfo":{{"name":"guild","title":"Guild","version":"{}"}}}}}}"#,
            env!("CARGO_PKG_VERSION")
        ),
        r#"{"method":"initialized"}"#.to_owned(),
        format!(r#"{{"id":{ASK},"method":"account/rateLimits/read"}}"#),
    ]
}

/// Run `program app-server`, ask for the limits, and read lines until the
/// answer — `deadline` at most — then kill what it started. `path` is the
/// `PATH` it is run with, and looked up on; `None` leaves the window's.
pub fn ask(
    program: &OsStr,
    path: Option<&OsStr>,
    deadline: Duration,
) -> Result<Vec<Window>, Failure> {
    let mut command = Command::new(program);
    command
        .arg("app-server")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    crate::network::apply(&mut command, guild_session::network::Scope::Guild)
        .map_err(|e| Failure::Request(e.into()))?;
    let mut child = guild_session::process::ProcessGroup::spawn(&mut command).map_err(|err| {
        match err.kind() {
            std::io::ErrorKind::NotFound => Failure::NotInstalled,
            _ => Failure::Request(err.to_string()),
        }
    })?;
    let outcome = exchange(&mut child, deadline);
    child
        .stop()
        .map_err(|error| Failure::Request(error.to_string()))?;
    outcome
}

/// The three lines out, the answer in. Its stdin is held until the answer
/// is read: a closed stdin is the app server's cue to leave.
fn exchange(
    child: &mut guild_session::process::ProcessGroup,
    deadline: Duration,
) -> Result<Vec<Window>, Failure> {
    let mut stdin = child.take_stdin().expect("piped");
    let stdout = child.take_stdout().expect("piped");
    for line in said() {
        writeln!(stdin, "{line}").map_err(|err| Failure::Request(err.to_string()))?;
    }
    stdin
        .flush()
        .map_err(|err| Failure::Request(err.to_string()))?;

    let (send, receive) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if send.send(line).is_err() {
                break;
            }
        }
    });

    let until = Instant::now() + deadline;
    loop {
        let left = until.saturating_duration_since(Instant::now());
        match receive.recv_timeout(left) {
            Ok(line) => {
                let Some(answer) = answer_to_ask(&line) else {
                    continue;
                };
                if let Some(error) = answer.get("error") {
                    let said = error.get("message").and_then(Value::as_str);
                    return Err(Failure::Request(said.unwrap_or("an error").to_owned()));
                }
                return windows(&line).map_err(|_| Failure::Unreadable);
            }
            Err(RecvTimeoutError::Timeout) => {
                return Err(Failure::Request(format!("no answer in {deadline:?}")));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(Failure::Request("codex exited before answering".to_owned()));
            }
        }
    }
}

/// The line, when it is the answer to the question; `None` for the
/// `initialize` answer, a notification, or a line that is not JSON.
fn answer_to_ask(line: &str) -> Option<Value> {
    let value: Value = serde_json::from_str(line).ok()?;
    (value.get("id")?.as_u64()? == ASK).then_some(value)
}
