//! Guild as `claude`'s IDE: the lock file, the tools, and the words the
//! protocol answers in.
//!
//! M8.6. Everything here is what task 0 **recorded** from
//! claude-code 2.1.261 rather than what a specification says, because
//! there is no specification: the CLI reads a lock file out of
//! `~/.claude/ide/`, opens a WebSocket to the port it names, and speaks
//! MCP over it.
//!
//! [`server`] is the socket and the threads; `root::ide` is the window's
//! half — what the tools actually answer.

pub mod server;

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// What the CLI is told when a diff is **accepted** — the first of the
/// two blocks [`accepted`] sends.
///
/// **And the IDE must not write the file.** Task 0 read this the other
/// way round, from a stub session that never reached the CLI's own
/// permission question; task 5 measured both in a real window. The CLI
/// writes the edit itself, and a window that writes the file first makes
/// that edit fail — *File content has changed since it was last read* —
/// and the agent spends a re-read, a shell command and about a hundred
/// thousand tokens working out that the change it wanted is already
/// there. Measured: 337.8k tokens with the write against 241.1k without
/// it, for one line.
///
/// **Why a second block, M5.12's decision 15.** M8.6 answered with this
/// word alone and saw the CLI ask *Do you want to make this edit?* in its
/// terminal afterwards, and recorded that as the CLI's design. It was a
/// crash: `claude` 2.1.270 tests the answer with
///
/// ```js
/// e[0]?.type==="text"&&e[0].text==="FILE_SAVED"&&typeof e[1].text==="string"
/// ```
///
/// and a one-block answer throws reading `e[1].text`, which lands in the
/// caller's `.catch` as *IDE diff view failed* and leaves the terminal's
/// own question up — so the person answered twice. With the text, the
/// same function returns `{newContent: I[1].text, saved: true}`; the edit
/// is re-derived from that text and allowed, and the terminal's question
/// is torn down. `saved` reaches only the decision log, so **the CLI
/// still writes the file** (read in 2.1.270 and 2.1.272, M5.12 task 0).
pub const ACCEPTED: &str = "FILE_SAVED";

/// What it is told when the person says no. It prints *User rejected
/// update to …* with the diff it wanted, and writes nothing.
pub const REJECTED: &str = "DIFF_REJECTED";

/// The directory `claude` looks in for an IDE.
pub fn lock_directory() -> PathBuf {
    dirs_home().join(".claude/ide")
}

/// The lock file for a port.
pub fn lock_path(port: u16) -> PathBuf {
    lock_directory().join(format!("{port}.lock"))
}

/// What goes in it.
///
/// `workspaceFolders` is a list because a window is one IDE with as many
/// projects as it has open, and `claude` matches its own directory
/// against it: a session started in any of them finds us.
pub fn lock_json(port: u16, folders: &[PathBuf], token: &str) -> Value {
    json!({
        "pid": std::process::id(),
        "port": port,
        "workspaceFolders": folders
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>(),
        "ideName": "Guild",
        "transport": "ws",
        "runningInWindows": false,
        "authToken": token,
    })
}

/// Whether a lock file is one of **ours**, naming a window that is gone.
///
/// A window killed rather than quit — a crash, a `kill`, a machine turned
/// off — leaves its lock behind: `Drop` does not run for a process that
/// is not there any more. `claude` would then find a lock naming a port
/// nothing listens on, and with `--ide` meaning *if exactly one valid IDE
/// is available*, a pile of dead locks is a live window nobody can find.
///
/// Only ours, and only dead: another editor's lock is not for us to
/// remove, and a pid that answers is a window still running.
pub fn stale(lock: &Value) -> bool {
    if lock.get("ideName").and_then(Value::as_str) != Some("Guild") {
        return false;
    }
    match lock.get("pid").and_then(Value::as_u64) {
        Some(pid) => !alive(pid as i32),
        None => false,
    }
}

/// Whether a process exists. Signal 0 asks without sending anything;
/// `EPERM` is an answer too — somebody else's process is still a process.
///
/// The hooks' socket sweep asks the same question of the same windows —
/// M5.10.
pub(crate) fn alive(pid: i32) -> bool {
    if pid <= 1 {
        // 0 is "every process in my group" and 1 is always there. Neither
        // is a window of ours, and neither is safe to ask about.
        return true;
    }
    // SAFETY: `kill` with signal 0 sends nothing; it only reports whether
    // the process could be signalled.
    let answered = unsafe { libc::kill(pid, 0) };
    answered == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// Take away the locks of Guild windows that are no longer running.
pub fn sweep() {
    let Ok(entries) = std::fs::read_dir(lock_directory()) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|kind| kind != "lock") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(lock) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if !stale(&lock) {
            continue;
        }
        tracing::info!(lock = %path.display(), "a lock left by a window that is gone");
        std::fs::remove_file(&path).ok();
    }
}

/// The tools the window offers, as `tools/list` answers.
///
/// **Only what the window can actually do.** A tool named and then
/// refused is worse than one that was never offered: the agent plans
/// around what it is told it has.
pub fn tools() -> Value {
    json!([
        {
            "name": "openDiff",
            "description": "Show a proposed change as a diff and wait for the person to accept or reject it.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "old_file_path": {"type": "string"},
                    "new_file_path": {"type": "string"},
                    "new_file_contents": {"type": "string"},
                    "tab_name": {"type": "string"}
                },
                "required": ["old_file_path", "new_file_path", "new_file_contents", "tab_name"]
            }
        },
        {
            "name": "close_tab",
            "description": "Close the tab with this name.",
            "inputSchema": {
                "type": "object",
                "properties": {"tab_name": {"type": "string"}},
                "required": ["tab_name"]
            }
        },
        {
            "name": "closeAllDiffTabs",
            "description": "Close every diff the agent opened.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "getDiagnostics",
            "description": "What the language servers say is wrong, for one file or for all of them.",
            "inputSchema": {
                "type": "object",
                "properties": {"uri": {"type": "string"}}
            }
        },
        {
            "name": "getWorkspaceFolders",
            "description": "The projects this window has open.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "getOpenEditors",
            "description": "The files open in tabs.",
            "inputSchema": {"type": "object", "properties": {}}
        },
        {
            "name": "checkDocumentDirty",
            "description": "Whether a file has unsaved changes in the window.",
            "inputSchema": {
                "type": "object",
                "properties": {"filePath": {"type": "string"}},
                "required": ["filePath"]
            }
        },
        {
            "name": "saveDocument",
            "description": "Write a file that has unsaved changes.",
            "inputSchema": {
                "type": "object",
                "properties": {"filePath": {"type": "string"}},
                "required": ["filePath"]
            }
        }
    ])
}

/// The answer to an accepted `openDiff`: [`ACCEPTED`], then the text the
/// proposal showed, as two text blocks — see [`ACCEPTED`] for why the
/// second exists. **The text is sent back byte for byte**: `claude` takes
/// the first hunk of the old file against it as the edit, so a text that
/// differs from the proposal is a different edit.
pub fn accepted(text: &str) -> Value {
    json!({"content": [
        {"type": "text", "text": ACCEPTED},
        {"type": "text", "text": text},
    ]})
}

/// The text an `openDiff` call proposes, or `None` when it carries none.
///
/// **Not `""` for a missing field.** The text is what Apply sends back,
/// and the CLI takes it as the edit — so a call without one, read as an
/// empty string, would be shown as every line deleted and, applied, would
/// empty the file. An emptied file is a proposal too, and says so with a
/// field that is there and empty.
pub fn proposed(arguments: &Value) -> Option<&str> {
    arguments.get("new_file_contents").and_then(Value::as_str)
}

/// An answer, in the shape MCP wants: content blocks, and ours are text.
pub fn text_result(text: impl Into<String>) -> Value {
    json!({"content": [{"type": "text", "text": text.into()}]})
}

/// An answer carrying JSON, which the CLI reads out of the text block —
/// that is how its own tools answer, and how `getDiagnostics` is read.
pub fn json_result(value: &Value) -> Value {
    text_result(value.to_string())
}

/// The home directory, or the current one when there is none — the same
/// fallback the log's directory takes.
fn dirs_home() -> PathBuf {
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => PathBuf::from(home),
        _ => PathBuf::from("."),
    }
}

/// Whether a path is inside one of these folders — which is how the CLI
/// decides an IDE is *its* IDE, and how the window decides a file the
/// agent names is one it may touch.
pub fn inside(folders: &[PathBuf], path: &Path) -> bool {
    folders.iter().any(|folder| path.starts_with(folder))
}
