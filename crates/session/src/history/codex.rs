//! Codex's sessions of a folder: `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<uuid>.jsonl`,
//! a `session_meta` line first; `session_index.jsonl` names a thread.

use std::collections::HashMap;
use std::io::{BufRead as _, Read as _};
use std::path::{Path, PathBuf};

use super::{Cache, Past, one_line};
use crate::HarnessKind;

/// How many of the newest rollouts are looked at: the store is every
/// folder's, and a folder's own are among the newest or are old.
const LOOKED_AT: usize = 400;
/// How much of a rollout is read for its meta line and first words.
const HEAD: u64 = 2 * 1024 * 1024;

pub(crate) fn recent(cache: &mut Cache, codex: &Path, folder: &Path, limit: usize) -> Vec<Past> {
    let mut files = rollouts(&codex.join("sessions"));
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name())); // the name leads with its time
    files.truncate(LOOKED_AT);
    cache.retain(|path, _| files.contains(path));
    let files = files.into_iter().filter_map(|path| {
        let touched = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
        Some((path, touched))
    });
    found_in(cache, codex, folder, limit, files)
}

/// The rollouts of `folder` written to at `since` or after, newest first —
/// by when each was last written, not by the name's start time: a thread
/// begun long ago and resumed today is today's. Every rollout's time is
/// read, which is a walk of the store, so this runs off the window.
pub(crate) fn since(
    cache: &mut Cache,
    codex: &Path,
    folder: &Path,
    since: std::time::SystemTime,
    limit: usize,
) -> Vec<Past> {
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = rollouts(&codex.join("sessions"))
        .into_iter()
        .filter_map(|path| {
            let touched = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
            (touched >= since).then_some((path, touched))
        })
        .collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.1));
    found_in(cache, codex, folder, limit, files.into_iter())
}

/// Read `files` in order until `limit` of them are `folder`'s own threads.
fn found_in(
    cache: &mut Cache,
    codex: &Path,
    folder: &Path,
    limit: usize,
    files: impl Iterator<Item = (PathBuf, std::time::SystemTime)>,
) -> Vec<Past> {
    let names = names(codex);
    let mut found = Vec::new();
    for (path, touched) in files {
        if found.len() >= limit {
            break;
        }
        let cached = match cache.get(&path) {
            Some((seen, cached)) if *seen == touched => cached.clone(),
            _ => {
                let cached = read(&path, touched);
                cache.insert(path.clone(), (touched, cached.clone()));
                cached
            }
        };
        let Some((cwd, mut past)) = cached else {
            continue;
        };
        if cwd != folder {
            continue;
        }
        if let Some(name) = names.get(&past.id) {
            past.title = one_line(name);
        }
        found.push(past);
    }
    found
}

/// A rollout's own `cwd` and session, when it is a person's own thread —
/// folder-independent on purpose, so a cached answer serves any folder
/// that asks (see `Cache`'s doc in `history.rs`).
fn read(path: &Path, touched: std::time::SystemTime) -> Option<(PathBuf, Past)> {
    let file = std::fs::File::open(path).ok()?;
    // Bytes until `serde_json` reads them, as in `claude::read`: `HEAD`
    // can cut a character in two, and a line read as text that is not
    // UTF-8 ends the reading rather than being skipped.
    let mut lines = std::io::BufReader::new(file.take(HEAD))
        .split(b'\n')
        .map_while(Result::ok);
    let meta: serde_json::Value = serde_json::from_slice(&lines.next()?).ok()?;
    let payload = meta.get("payload")?;
    if meta.get("type")?.as_str()? != "session_meta" {
        return None;
    }
    let cwd = PathBuf::from(payload.get("cwd")?.as_str()?);
    if !a_person_s(payload) {
        return None;
    }
    let id = payload.get("id")?.as_str()?.to_owned();
    let records: Vec<serde_json::Value> = lines
        .filter_map(|l| serde_json::from_slice::<serde_json::Value>(&l).ok())
        .collect();
    let said = first_words(&records);
    Some((
        cwd,
        Past {
            harness: HarnessKind::Codex,
            id,
            title: one_line(said.as_deref().unwrap_or("")),
            touched,
        },
    ))
}

/// The person's first words, in whichever shape this rollout carries
/// them — measured on this machine 2026-09-28, across CLI versions
/// 0.153.4 to 0.158.0: only 0.155.x (7 of 63 rollouts here) writes an
/// `event_msg`/`user_message`; every other version carries them as a
/// `response_item` message of role `user`, and Codex injects its own
/// role-`user` context before it — `<environment_context>`, AGENTS.md
/// instructions and the like, always starting with `<` or `#`, which is
/// what tells them apart from what a person actually typed.
fn first_words(records: &[serde_json::Value]) -> Option<String> {
    records
        .iter()
        .find_map(|r| {
            let p = r.get("payload")?;
            (r.get("type")?.as_str()? == "event_msg" && p.get("type")?.as_str()? == "user_message")
                .then(|| p.get("message")?.as_str().map(str::to_owned))?
        })
        .or_else(|| {
            records.iter().find_map(|r| {
                if r.get("type")?.as_str()? != "response_item" {
                    return None;
                }
                let p = r.get("payload")?;
                if p.get("type")?.as_str()? != "message" || p.get("role")?.as_str()? != "user" {
                    return None;
                }
                let text = p
                    .get("content")?
                    .as_array()?
                    .first()?
                    .get("text")?
                    .as_str()?;
                (!text.starts_with('<') && !text.starts_with('#')).then(|| text.to_owned())
            })
        })
}

/// Half of a folder's rollouts are Codex's own subagents and guardian
/// reviews (measured 2026-09-28): `thread_source` says `user` for a
/// person's; a rollout older than that field says so by a plain `source`.
fn a_person_s(payload: &serde_json::Value) -> bool {
    match payload
        .get("thread_source")
        .and_then(serde_json::Value::as_str)
    {
        Some(source) => source == "user",
        None => payload
            .get("source")
            .is_some_and(serde_json::Value::is_string),
    }
}

/// The names people gave threads, by id; the last line for an id wins.
fn names(codex: &Path) -> HashMap<String, String> {
    let Ok(file) = std::fs::File::open(codex.join("session_index.jsonl")) else {
        return HashMap::new();
    };
    std::io::BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(&l).ok())
        .filter_map(|r| {
            Some((
                r.get("id")?.as_str()?.to_owned(),
                r.get("thread_name")?.as_str()?.to_owned(),
            ))
        })
        .filter(|(_, name)| !name.trim().is_empty())
        .collect()
}

/// Every `rollout-*.jsonl` three levels under `sessions`.
pub(crate) fn rollouts(sessions: &Path) -> Vec<PathBuf> {
    let children = |dir: &Path| -> Vec<PathBuf> {
        std::fs::read_dir(dir)
            .map(|e| e.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    };
    children(sessions)
        .iter()
        .flat_map(|year| children(year))
        .flat_map(|month| children(&month))
        .flat_map(|day| children(&day))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("rollout-") && n.ends_with(".jsonl"))
        })
        .collect()
}
