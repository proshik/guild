//! What each harness remembers of a folder — M14.5, the start screen.
//!
//! Read from the harness's **own** store, the person's choice of
//! 2026-09-28 over Guild's records: a session begun in a plain terminal
//! in this folder is one to resume too. One reader a harness, one shape.

pub mod claude;
pub mod codex;
pub mod opencode;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::HarnessKind;

/// How many rows the start screen shows.
pub const LIMIT: usize = 50;

/// The most a [`History::since`] read takes of one folder, newest first:
/// far beyond what a few days of work leaves, a bound on a runaway store.
pub const SINCE_LIMIT: usize = 500;

/// One session a harness remembers of a folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Past {
    /// Which harness this session belongs to.
    pub harness: HarnessKind,
    /// What `resume_args` takes.
    pub id: String,
    /// One line, never empty.
    pub title: String,
    /// When the session was last written to.
    pub touched: SystemTime,
}

/// Where each store is — the real ones from `Stores::home`, a test's own
/// otherwise.
#[derive(Debug, Clone)]
pub struct Stores {
    /// `~/.claude/projects`.
    pub claude: PathBuf,
    /// `$CODEX_HOME`, else `~/.codex`.
    pub codex: PathBuf,
    /// The `PATH` `opencode` is looked up on.
    pub path: std::ffi::OsString,
    /// opencode's data directory: while it does not exist opencode has no
    /// sessions and is not run to say so — GLD-078.
    pub opencode: PathBuf,
}

impl Stores {
    /// The person's own stores, and the `PATH` the window runs things on.
    pub fn home(path: std::ffi::OsString) -> Self {
        Self {
            claude: crate::transcript::projects_dir(),
            codex: crate::harness::CodexHarness::home(),
            path,
            opencode: crate::harness::OpenCodeHarness::data_dir(),
        }
    }
}

/// A file's answer, kept until the file changes: its own `cwd` beside
/// the `Past` it would give, both folder-independent, so one `History`
/// can be asked about different folders (a worktree switch, or two
/// folders sharing Claude's lossy directory name) without one's cached
/// session leaking into the other's answer — the `cwd == folder`
/// comparison is made fresh on every call, from the cache.
pub(crate) type Cache = HashMap<PathBuf, (SystemTime, Option<(PathBuf, Past)>)>;

/// The three harnesses' readers, each keeping its own cache.
#[derive(Debug, Default)]
pub struct History {
    claude: Cache,
    codex: Cache,
}

impl History {
    /// Every harness's sessions of `folder`, newest first, at most `limit`.
    /// A store that cannot be read gives nothing and the others still answer.
    pub fn recent(&mut self, stores: &Stores, folder: &Path, limit: usize) -> Vec<Past> {
        let mut all = self.claude(stores, folder, limit, None);
        all.extend(codex::recent(&mut self.codex, &stores.codex, folder, limit));
        all.extend(opencode::recent(stores, folder, limit));
        all.sort_by_key(|p| std::cmp::Reverse(p.touched));
        all.truncate(limit);
        all
    }

    /// Every harness's sessions of `folder` touched at `since` or after,
    /// newest first — the column's Settled section. The age is applied
    /// before any count, so newer conversations cannot push an eligible
    /// one out; [`SINCE_LIMIT`] only bounds a runaway store.
    pub fn since(&mut self, stores: &Stores, folder: &Path, since: SystemTime) -> Vec<Past> {
        let mut all = self.claude(stores, folder, SINCE_LIMIT, Some(since));
        all.extend(codex::since(
            &mut self.codex,
            &stores.codex,
            folder,
            since,
            SINCE_LIMIT,
        ));
        all.extend(opencode::recent(stores, folder, SINCE_LIMIT));
        all.retain(|p| p.touched >= since);
        all.sort_by_key(|p| std::cmp::Reverse(p.touched));
        all.truncate(SINCE_LIMIT);
        all
    }

    fn claude(
        &mut self,
        stores: &Stores,
        folder: &Path,
        limit: usize,
        since: Option<SystemTime>,
    ) -> Vec<Past> {
        let mut files = claude::files(&stores.claude, folder);
        // Only this folder's directory is listed, so only its entries can
        // be known to be gone: another folder's are kept for the next
        // time it is asked about — a worktree switch and back reads
        // nothing again.
        let directory = stores.claude.join(claude::folder_name(folder));
        self.claude.retain(|path, _| {
            path.parent() != Some(directory.as_path()) || files.iter().any(|f| &f.1 == path)
        });
        files.sort_by_key(|f| std::cmp::Reverse(f.2));
        if let Some(since) = since {
            files.retain(|f| f.2 >= since);
        }
        files.truncate(limit);
        files
            .into_iter()
            .filter_map(|(id, path, touched)| {
                let found = match self.claude.get(&path) {
                    Some((seen, found)) if *seen == touched => found.clone(),
                    _ => {
                        let found = std::fs::File::open(&path).ok().and_then(|mut file| {
                            let len = file.metadata().ok()?.len();
                            let (cwd, title) = claude::read(&mut file, len)?;
                            Some((
                                cwd,
                                Past {
                                    harness: HarnessKind::Claude,
                                    id,
                                    title: one_line(&title),
                                    touched,
                                },
                            ))
                        });
                        self.claude.insert(path, (touched, found.clone()));
                        found
                    }
                };
                found.and_then(|(cwd, past)| (cwd == folder).then_some(past))
            })
            .collect()
    }
}

/// A title as one line, never empty.
pub(crate) fn one_line(title: &str) -> String {
    let line = title
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    if line.is_empty() {
        "…".to_owned()
    } else {
        line.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the integration tests cannot see: an entry whose file is gone
    /// leaves the cache, and only the folder being asked about is swept.
    #[test]
    fn a_gone_transcript_leaves_the_cache_and_another_folder_s_stays() {
        let scratch = tempfile::TempDir::new().unwrap();
        let root = scratch.path();
        let stores = Stores {
            claude: root.join("projects"),
            codex: root.join("codex"),
            path: root.join("bin").into_os_string(),
            opencode: root.join("opencode"),
        };
        let write = |folder: &Path, id: &str| {
            let dir = stores.claude.join(claude::folder_name(folder));
            std::fs::create_dir_all(&dir).unwrap();
            let file = dir.join(format!("{id}.jsonl"));
            let user = serde_json::json!({"type":"user","cwd":folder,"message":{"content":"hi"}});
            std::fs::write(&file, user.to_string()).unwrap();
            file
        };
        let (a, b) = (root.join("a"), root.join("b"));
        let gone = write(&a, "a1");
        let kept = write(&a, "a2");
        let other = write(&b, "b1");

        let mut history = History::default();
        history.recent(&stores, &a, LIMIT);
        history.recent(&stores, &b, LIMIT);
        std::fs::remove_file(&gone).unwrap();
        std::fs::remove_file(&other).unwrap();
        history.recent(&stores, &a, LIMIT);

        let mut cached: Vec<_> = history.claude.keys().cloned().collect();
        cached.sort();
        assert_eq!(cached, [kept, other]);
    }
}
