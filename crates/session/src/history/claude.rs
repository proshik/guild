//! Claude Code's sessions of a folder: `~/.claude/projects/<folder>/<id>.jsonl`.

use std::io::{BufRead as _, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::transcript::{Titles, is_meta};

/// How much of a file's head is read looking for its first `user` record.
const HEAD: u64 = 1024 * 1024;
/// How much of its end is read for the latest title.
const TAIL: u64 = 256 * 1024;

/// The directory `claude` keeps a folder's sessions in: the path with
/// every character that is not an ASCII letter or digit made a `-`
/// (measured on 2026-09-28 — `/.claude` is `--claude`, each Cyrillic
/// letter one `-`).
///
/// `docs/traps.md` warns against reproducing this scheme, because
/// `transcript::path` has to find one exact file among every project and
/// a computed path that drifts returns "no session" instead of an error.
/// The history reader has no id to search by — only a folder — so it
/// computes the directory anyway; the guard against drift is that a file
/// found here is confirmed by its own `cwd` before it is believed, never
/// trusted on the strength of the name alone.
pub fn folder_name(folder: &Path) -> String {
    folder
        .to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// A session file's own `cwd` and title, or `None` for a stub with no
/// `user` record or one unreadable. Folder-independent on purpose: which
/// folder this belongs to is for the caller to ask of the `cwd`, fresh
/// every time — including from a cached answer — never baked in here.
pub fn read<R: Read + Seek>(file: &mut R, len: u64) -> Option<(PathBuf, String)> {
    let mut titles = Titles::default();
    let mut cwd = None;

    // Lines are bytes until `serde_json` reads them: a line read as text
    // is an error when it is not UTF-8, and both ends of this read can
    // cut a character in two — the tail starts at any byte, the head's
    // bound stops at any byte.
    let mut head = std::io::BufReader::new((&mut *file).take(HEAD.min(len)));
    let mut line = Vec::new();
    while head.read_until(b'\n', &mut line).ok()? > 0 {
        if let Ok(record) = serde_json::from_slice::<serde_json::Value>(&line) {
            titles.take(&record);
            let user = record.get("type").and_then(serde_json::Value::as_str) == Some("user");
            if user && !is_meta(&record) {
                let found = record.get("cwd").and_then(serde_json::Value::as_str)?;
                cwd = Some(PathBuf::from(found));
                break;
            }
        }
        line.clear();
    }
    let cwd = cwd?;
    drop(head);

    let from = len.saturating_sub(TAIL);
    file.seek(SeekFrom::Start(from)).ok()?;
    let mut tail = std::io::BufReader::new((&mut *file).take(TAIL));
    if from > 0 {
        // The first line of the tail is cut; it is somebody's middle.
        tail.read_until(b'\n', &mut Vec::new()).ok()?;
    }
    for line in tail.split(b'\n').map_while(Result::ok) {
        if let Ok(record) = serde_json::from_slice::<serde_json::Value>(&line) {
            titles.take(&record);
        }
    }
    Some((cwd, titles.best()?))
}

/// Every session file of `folder`, as `(id, path, modified)` — the
/// directory's own `*.jsonl` only: subagents live beneath it.
pub(crate) fn files(
    projects: &Path,
    folder: &Path,
) -> Vec<(String, std::path::PathBuf, std::time::SystemTime)> {
    let Ok(entries) = std::fs::read_dir(projects.join(folder_name(folder))) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "jsonl" {
                return None;
            }
            let meta = entry.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            let id = path.file_stem()?.to_str()?.to_owned();
            Some((id, path, meta.modified().ok()?))
        })
        .collect()
}
