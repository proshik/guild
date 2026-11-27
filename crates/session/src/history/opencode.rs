//! OpenCode's sessions of a folder, through `opencode session list --format json`
//! — its own contract, the person's choice over its SQLite store: it answers
//! top-level sessions only, with `id`, `title`, `updated` (ms) and `directory`.

use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

use super::{Past, Stores, one_line};
use crate::HarnessKind;

const DEADLINE: Duration = Duration::from_secs(5);

/// How long an already-exited leader's reader is given before its group
/// is killed: long enough for one that already has the answer (written
/// before the leader exited) to deliver it, short enough that a real
/// orphan is still caught quickly.
const GRACE: Duration = Duration::from_millis(200);

/// opencode's sessions of `folder` — none, and opencode not run, while
/// its data directory does not exist: running it there is what creates its
/// directories, and with Guild's plugin present it then installs the
/// plugin's dependencies — GLD-078.
pub(crate) fn recent(stores: &Stores, folder: &Path, limit: usize) -> Vec<Past> {
    if !crate::harness::known(&stores.opencode) {
        return Vec::new();
    }
    ask(&stores.path, folder, limit)
        .map(|out| parse(&out, folder))
        .unwrap_or_default()
}

/// Its answer, as sessions of `folder`; anything else is nothing.
pub fn parse(json: &[u8], folder: &Path) -> Vec<Past> {
    let Ok(serde_json::Value::Array(rows)) = serde_json::from_slice(json) else {
        tracing::info!("opencode session list did not answer with a JSON array");
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            if Path::new(row.get("directory")?.as_str()?) != folder {
                return None;
            }
            // An unnamed session is `New session - <ISO time>`: the time
            // is the row's age already, drawn at its right.
            let title = row
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("")
                .trim();
            let title = if title.is_empty() || title.starts_with("New session - ") {
                "New session"
            } else {
                title
            };
            Some(Past {
                harness: HarnessKind::OpenCode,
                id: row.get("id")?.as_str()?.to_owned(),
                title: one_line(title),
                touched: UNIX_EPOCH + Duration::from_millis(row.get("updated")?.as_u64()?),
            })
        })
        .collect()
}

/// `opencode session list` run in `folder` on `path` — the wait for its
/// exit **and** the read of its stdout both bounded by `DEADLINE`,
/// `HarnessKind::probe_version`'s loop for the first, a bounded
/// `recv_timeout` for the second: a `join` on the reading thread, tried
/// first, blocks past the deadline when the leader exits but a
/// descendant it started keeps our end of the pipe open — `usage/codex.rs`'s
/// hazard, met here too.
///
/// **The group is killed when something may still be running in it, and
/// only when the kill cannot reach a stranger.** A missed deadline, or
/// `try_wait` itself erroring rather than reaping the leader: the leader
/// is still alive, so it and the group id `process_group(0)` gave it are
/// both still held — kill the group, then reap. Once `try_wait` **has**
/// reaped the leader, that id may already be free for reuse
/// (`usage/codex.rs:180-186`'s "nothing reaps it before the killpg" — met
/// from the other side here, since this function's own `try_wait` is the
/// thing doing the early reaping), so a kill is not sent on trust: the
/// reader is given `GRACE` to deliver first. If it does, the pipe is
/// closed at every end, the answer is taken and **no kill is sent** — a
/// descendant that redirected its own stdout and lives on is not seen and
/// not killed. Only the pipe still held past `GRACE` — proof a member of
/// the group is alive, since POSIX will not free a process-group id that
/// still has one — makes the kill safe, and it is sent then.
fn ask(path: &OsStr, folder: &Path, limit: usize) -> Option<Vec<u8>> {
    ask_args(
        path,
        folder,
        &[
            "session",
            "list",
            "--format",
            "json",
            "-n",
            &limit.to_string(),
        ],
    )
}

fn ask_args(path: &OsStr, folder: &Path, args: &[&str]) -> Option<Vec<u8>> {
    use std::io::Read as _;
    use std::os::unix::process::CommandExt as _;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;

    use std::os::unix::fs::PermissionsExt as _;

    // The program is found among the PATH's absolute folders only, before
    // the working folder changes: a relative entry would be read against
    // the project, and a project's own `opencode` must never be what runs.
    // The first that can run, as a shell would find it; and the program is
    // given only those folders, so a wrapper looking up its interpreter
    // cannot find one in the project either.
    let absolute: Vec<_> = std::env::split_paths(path)
        .filter(|folder| folder.is_absolute())
        .collect();
    let program = absolute
        .iter()
        .map(|folder| folder.join(HarnessKind::OpenCode.program()))
        .find(|candidate| {
            std::fs::metadata(candidate)
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })?;
    let path = std::env::join_paths(&absolute).ok()?;
    let mut child = Command::new(program)
        .args(args)
        .current_dir(folder)
        .env("PATH", &path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        // Its own process group, led by this child: what it started and
        // left holding our pipe is killed with it — at the deadline, or
        // once the pipe outlasts the leader's exit by `GRACE` — rather
        // than outliving the window's patience.
        .process_group(0)
        .spawn()
        .ok()?;
    let mut pipe = child.stdout.take()?;
    let (sent, answer) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        let _ = sent.send(bytes);
    });

    let until = std::time::Instant::now() + DEADLINE;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if std::time::Instant::now() >= until => break None,
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            // Not reaped either — handled the same as a missed deadline,
            // just below: the leader may still be alive.
            Err(_) => break None,
        }
    };

    let Some(status) = status else {
        // The leader is still alive here (`try_wait` never reported it
        // exited), so it and its group id are both still held: safe to
        // kill before the reap that follows.
        tracing::info!("opencode session command did not answer in {DEADLINE:?}");
        kill_group(child.id());
        let _ = child.wait();
        return None;
    };

    // `try_wait` has already reaped the leader — see the doc comment
    // above for why that means the group is not killed yet.
    if let Ok(bytes) = answer.recv_timeout(GRACE) {
        return status.success().then_some(bytes);
    }
    kill_group(child.id());
    match answer.recv_timeout(GRACE) {
        // What was written before the leader exited is still read: the
        // reader thread had already taken it from the pipe, and was
        // only waiting on the EOF the kill of what was left in its
        // group supplies.
        Ok(bytes) => status.success().then_some(bytes),
        Err(_) => {
            tracing::info!("opencode session command did not answer in {DEADLINE:?}");
            None
        }
    }
}

// std's `CommandExt::process_group` is safe and stable, but killing the
// group it made needs `kill(2)` with a negative pid, which std does not
// expose; this crate links no `libc`, so the one call is declared here —
// `login_path.rs` declares `setsid` the same way.
unsafe extern "C" {
    fn kill(pid: std::ffi::c_int, sig: std::ffi::c_int) -> std::ffi::c_int;
}

const SIGKILL: std::ffi::c_int = 9;

/// Kill the process group `pid` leads — `pid` being both a child of
/// ours' own id and, from `process_group(0)`, its group's. Called either
/// while that child is still alive and unreaped, or once `ask`'s own
/// grace period has shown some member of the group still is: either way
/// `pid` cannot yet have been handed to a stranger.
fn kill_group(pid: u32) {
    // SAFETY: `kill` touches no memory of ours; `-pid` addresses the
    // group `process_group(0)` made `pid` the leader of.
    unsafe {
        kill(-(pid as std::ffi::c_int), SIGKILL);
    }
}

/// Read an exact session through OpenCode's own bounded export command.
pub fn replies(path: &OsStr, folder: &Path, id: &str, count: usize) -> Result<Vec<String>, String> {
    if !id.starts_with("ses_") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("invalid OpenCode conversation id".into());
    }
    // The installed V2 contract offers a private server; never auto-start or
    // borrow the user's background service merely to inspect saved identity.
    let bytes = ask_args(path, folder, &["session", "export", id, "--standalone"])
        .ok_or_else(|| "OpenCode session export failed or timed out".to_owned())?;
    replies_in(&bytes, id, count)
}

/// Assistant replies in an exact OpenCode v2 session export. Tool content
/// and reasoning are deliberately not included in the assistant's words.
pub fn replies_in(json: &[u8], id: &str, count: usize) -> Result<Vec<String>, String> {
    let value: serde_json::Value = serde_json::from_slice(json)
        .map_err(|_| "OpenCode did not export valid JSON".to_owned())?;
    if value["info"]["id"].as_str() != Some(id) {
        return Err("OpenCode exported a different conversation — refusing to read it".into());
    }
    let messages = value["messages"]
        .as_array()
        .ok_or_else(|| "OpenCode export has no message list".to_owned())?;
    let mut replies = std::collections::VecDeque::new();
    for message in messages {
        if message["type"] != "assistant" {
            continue;
        }
        let Some(parts) = message["content"].as_array() else {
            continue;
        };
        let text = parts
            .iter()
            .filter(|p| p["type"] == "text")
            .filter_map(|p| p["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() {
            replies.push_back(text);
            while replies.len() > count {
                replies.pop_front();
            }
        }
    }
    Ok(replies.into_iter().collect())
}

/// Qualify an existing exact conversation before any same-session native resume.
/// Export is read-only; --session alone can otherwise create a missing session.
pub fn qualify_conversation(path: &OsStr, folder: &Path, id: &str) -> Result<(), String> {
    if !id.starts_with("ses_") || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("workflow.recovery: invalid OpenCode conversation identity".into());
    }
    // The installed V2 contract offers a private server; never auto-start or
    // borrow the user's background service merely to inspect saved identity.
    let bytes = ask_args(path, folder, &["session", "export", id, "--standalone"])
        .ok_or("workflow.recovery: exact OpenCode export unavailable")?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| "workflow.recovery: corrupt OpenCode conversation export")?;
    let info = &value["info"];
    // V2 exports location.directory. Retain the previously supported legacy
    // shape only when location is absent; present metadata must agree.
    let directory = match info.get("location") {
        Some(location) => location
            .get("directory")
            .and_then(serde_json::Value::as_str),
        None => info.get("directory").and_then(serde_json::Value::as_str),
    };
    if info["id"].as_str() != Some(id)
        || directory.is_none()
        || directory != folder.to_str()
        || info
            .get("directory")
            .is_some_and(|legacy| legacy.as_str() != directory)
        || !value["messages"].is_array()
    {
        return Err("workflow.recovery: OpenCode conversation/cwd identity differs".into());
    }
    Ok(())
}
