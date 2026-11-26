//! The worktrees of a repository, as git lists them — M12.
//!
//! **What git says is what there is.** A worktree made before Guild, by
//! hand, or by an agent in a terminal is in `git worktree list` as surely
//! as one Guild made, and the column draws what that list says — decision
//! 2 of M12's plan. The records were recorded from git 2.50.1 by that
//! plan's task 0: `worktree <path>`, then `HEAD <sha>`, then one of
//! `branch <ref>`, `detached` or `bare`, then `locked` and `prunable` when
//! they apply, each field ended by a NUL and each record by an empty one.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use super::{GitError, run};

/// One worktree of a repository.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Worktree {
    /// Its folder.
    pub path: PathBuf,
    /// The commit it has out; a bare repository's own entry has none.
    pub head: Option<String>,
    /// The branch it is on, without `refs/heads/`; none when detached.
    pub branch: Option<String>,
    /// On no branch.
    pub detached: bool,
    /// The repository's own entry when it has no working tree.
    pub bare: bool,
    /// Locked against being removed or pruned, with git's reason when it
    /// was given one — `locked` or `locked <reason>` in the porcelain.
    ///
    /// **The reason is kept because it is the whole of the message.** A
    /// lock says *somebody is using this*, and who is in the text: a tool
    /// that locked a checkout writes its own name and pid there, which is
    /// what tells a person whether to wait or to unlock. M12 task 18.
    pub locked: Option<String>,
    /// Its folder is gone, and `git worktree prune` would forget it.
    pub prunable: bool,
    /// When its `HEAD` was committed, in seconds since the epoch — M12
    /// task 16.
    ///
    /// **Not filled in by the parser**, because `git worktree list` does
    /// not say: it comes from one `git show` over every head the list
    /// named, asked on the same clock and answered into the same records.
    /// `None` for a worktree with no head, and for one whose commit git
    /// declined to describe.
    pub committed: Option<i64>,
    /// The lines its branch added and removed since it left the main
    /// worktree's branch, with what is not committed yet — M12.4, drawn as
    /// `+677 −298`.
    ///
    /// **Not filled in by the parser**, for `committed`'s reason: it comes
    /// from [`lines`], asked on the same clock. `None` when git refused,
    /// when the main worktree is on no branch, and for an entry the column
    /// does not draw.
    pub lines: Option<(u32, u32)>,
    /// Where a push of its branch would go — the remote, its URL and the
    /// remote branch's name, by `Ahead::destination`'s rule — GLD-063.
    /// The MR is looked up by it. **Not filled in by the parser**: it comes
    /// from [`destinations`]. `None` with no remote or on no branch.
    pub push: Option<Pushed>,
    /// The commits on its branch a push would send, `↑N`, counted from refs
    /// only — GLD-063. `None` where there is no remote to compare with.
    pub ahead: Option<u32>,
    /// Its branch is the repository's main branch, by
    /// [`crate::git::main_branch`]'s rule — `origin/HEAD`'s local branch,
    /// else `main` or `master` — whose merge request is never asked,
    /// GLD-063. **Not filled in by the parser**: the snapshot reads it.
    pub main_branch: bool,
}

/// Where a worktree's branch would be pushed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Pushed {
    /// One of the names `git remote` printed.
    pub remote: String,
    /// The remote's URL, as `git remote get-url` prints it.
    pub url: Option<String>,
    /// The branch on that remote: the upstream's own name when it tracks
    /// one there, the local name otherwise.
    pub branch: String,
}

impl Worktree {
    /// Whether the column draws it: it has a checkout to open. A bare
    /// entry has none and a prunable one's folder is gone; a locked one is
    /// a checkout like any other.
    pub fn shown(&self) -> bool {
        !self.bare && !self.prunable
    }
}

/// The worktrees of the repository `root` is in, as git lists them.
pub fn list(root: &Path) -> Result<Vec<Worktree>, GitError> {
    #[cfg(feature = "native-preview")]
    let _trial = diagnostic::Read::new();
    let result = parse(&run(root, &["worktree", "list", "--porcelain", "-z"])?);
    #[cfg(test)]
    read_gate::pause(root);
    result
}

/// Read `git worktree list --porcelain -z`.
///
/// A path is taken as bytes, not as text: a folder whose name is not
/// UTF-8 is still a folder, and `from_utf8_lossy` would hand back a path
/// that does not exist. An attribute this code does not know is a newer
/// git, and is passed over; a record that does not begin with its path is
/// a format nobody has seen, and is an error rather than a guess — a guess
/// draws a row for a directory that may not exist.
pub fn parse(bytes: &[u8]) -> Result<Vec<Worktree>, GitError> {
    let mut list = Vec::new();
    let mut current: Option<Worktree> = None;
    for field in bytes.split(|byte| *byte == 0) {
        if field.is_empty() {
            list.extend(current.take());
            continue;
        }
        let (key, value) = match field.iter().position(|byte| *byte == b' ') {
            Some(space) => (&field[..space], &field[space + 1..]),
            None => (field, &b""[..]),
        };
        if key == b"worktree" {
            list.extend(current.take());
            current = Some(Worktree {
                path: PathBuf::from(OsStr::from_bytes(value)),
                ..Worktree::default()
            });
            continue;
        }
        let Some(worktree) = current.as_mut() else {
            return Err(GitError::Failed(format!(
                "a worktree record without its path: {}",
                String::from_utf8_lossy(field)
            )));
        };
        let text = || String::from_utf8_lossy(value).into_owned();
        match key {
            b"HEAD" => worktree.head = Some(text()),
            b"branch" => {
                let reference = text();
                worktree.branch = Some(
                    reference
                        .strip_prefix("refs/heads/")
                        .unwrap_or(&reference)
                        .to_owned(),
                );
            }
            b"detached" => worktree.detached = true,
            b"bare" => worktree.bare = true,
            b"locked" => worktree.locked = Some(text()),
            b"prunable" => worktree.prunable = true,
            _ => {}
        }
    }
    list.extend(current);
    Ok(list)
}

/// The git directory every worktree of `root`'s repository shares — the
/// main worktree's `.git`, absolute. Two roots with the same answer are
/// one repository; a directory outside git has none, and is a group of
/// one.
pub fn common_dir(root: &Path) -> Result<PathBuf, GitError> {
    let out = run(
        root,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let line = out.strip_suffix(b"\n").unwrap_or(&out);
    if line.is_empty() {
        return Err(GitError::Failed("git printed no common dir".to_owned()));
    }
    Ok(PathBuf::from(OsStr::from_bytes(line)))
}

/// When each of these commits was made, in seconds since the epoch.
///
/// **One call for every worktree, not one per row.** `git show -s` takes
/// any number of revisions and prints a line for each, so a repository
/// with nine worktrees costs one process rather than nine — the same
/// reasoning that made the status walk one call in M8.2.
///
/// A revision git cannot describe is left out rather than failing the
/// list: a worktree whose head is a commit this repository no longer has
/// is still a worktree, and the column would rather draw it without a
/// time than not at all.
pub fn times(root: &Path, heads: &[String]) -> Result<Vec<(String, i64)>, GitError> {
    if heads.is_empty() {
        return Ok(Vec::new());
    }
    let mut argv: Vec<&str> = vec!["show", "-s", "--format=%H %ct"];
    argv.extend(heads.iter().map(String::as_str));
    let out = run(root, &argv)?;
    Ok(read_times(&out))
}

/// Read what `git show -s --format=%H %ct` printed.
pub fn read_times(bytes: &[u8]) -> Vec<(String, i64)> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let (sha, at) = line.split_once(' ')?;
            Some((sha.to_owned(), at.trim().parse().ok()?))
        })
        .collect()
}

/// The lines a worktree added and removed: its branch since the merge-base
/// with `base_branch`, and what is not committed yet — M12.4.
///
/// Read in two steps —
/// `git merge-base`, then `git diff --numstat` from it, run in the worktree
/// so the diff reaches its working tree. For the main worktree the
/// merge-base with its own branch is its `HEAD`, which makes its lines what
/// it has not committed, with no second rule. **An untracked file is not
/// in a diff and is not counted**; a base git cannot find is git's
/// refusal, never zero.
pub fn lines(worktree: &Path, base_branch: &str) -> Result<(u32, u32), GitError> {
    let branch = format!("refs/heads/{base_branch}");
    let out = run(worktree, &["merge-base", "HEAD", &branch])?;
    let base = String::from_utf8_lossy(&out).trim().to_owned();
    if base.is_empty() {
        return Err(GitError::Failed("git printed no merge-base".to_owned()));
    }
    let out = run(worktree, &["diff", "--numstat", "-z", &base])?;
    Ok(read_numstat(&out))
}

/// What `git diff --numstat -z` printed, summed: lines added, lines
/// removed.
///
/// A row is `added\tremoved\tpath\0`. A binary file prints `-` for both
/// and counts nothing. **A rename leaves the path empty** and puts the old
/// and new names in the next two fields, which are skipped rather than
/// read as rows of their own.
pub fn read_numstat(bytes: &[u8]) -> (u32, u32) {
    let mut fields = bytes.split(|byte| *byte == 0);
    let (mut added, mut removed) = (0u32, 0u32);
    while let Some(field) = fields.next() {
        if field.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(field);
        let mut parts = text.splitn(3, '\t');
        let (Some(plus), Some(minus), Some(path)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        added = added.saturating_add(plus.parse().unwrap_or(0));
        removed = removed.saturating_add(minus.parse().unwrap_or(0));
        if path.is_empty() {
            fields.next();
            fields.next();
        }
    }
    (added, removed)
}

/// What git ignores in a worktree — what `git worktree remove` deletes with
/// the folder and does not mention. M13 task 8.
///
/// **A folder git ignores is one entry**, `target/`, and not every file in
/// it: `--directory` does not walk into it, which is what keeps a Rust
/// worktree a ten-millisecond question and a refusal a person can read.
/// **The window's own `.guild/` is left out**: it is the state of the root
/// the removal closes, it is in every root the window has opened, and
/// naming it would make every removal a refusal.
pub fn ignored(worktree: &Path) -> Result<Vec<String>, GitError> {
    let out = run(
        worktree,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
            "-z",
        ],
    )?;
    Ok(String::from_utf8_lossy(&out)
        .split('\0')
        .filter(|name| !name.is_empty())
        .filter(|name| !name.starts_with(".guild/"))
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
pub(crate) mod read_gate {
    use super::*;
    use std::sync::{Mutex, mpsc};
    type Gate = (PathBuf, mpsc::SyncSender<PathBuf>, mpsc::Receiver<()>);
    static GATES: Mutex<Vec<Gate>> = Mutex::new(Vec::new());
    pub(crate) fn hold(folder: &Path) -> (mpsc::Receiver<PathBuf>, mpsc::Sender<()>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::channel();
        GATES.lock().unwrap().push((folder.to_path_buf(), tx, wait));
        (rx, release)
    }
    pub(super) fn pause(root: &Path) {
        let gate = {
            let mut gates = GATES.lock().unwrap();
            gates
                .iter()
                .position(|(folder, _, _)| root.starts_with(folder))
                .map(|i| gates.remove(i))
        };
        if let Some((_, ready, wait)) = gate {
            let _ = ready.send(root.to_path_buf());
            let _ = wait.recv_timeout(std::time::Duration::from_secs(20));
        }
    }
}

/// Invocation counters for task-owned production scheduling trials only.
#[cfg(feature = "native-preview")]
pub mod diagnostic {
    use std::sync::atomic::{AtomicU64, Ordering};
    static STARTED: AtomicU64 = AtomicU64::new(0);
    static FINISHED: AtomicU64 = AtomicU64::new(0);
    pub struct Read;
    impl Default for Read {
        fn default() -> Self {
            Self::new()
        }
    }
    impl Read {
        pub fn new() -> Self {
            STARTED.fetch_add(1, Ordering::Relaxed);
            Self
        }
    }
    impl Drop for Read {
        fn drop(&mut self) {
            FINISHED.fetch_add(1, Ordering::Release);
        }
    }
    pub fn counts() -> (u64, u64) {
        (
            STARTED.load(Ordering::Acquire),
            FINISHED.load(Ordering::Acquire),
        )
    }
}

/// Fill in each branch's [`Pushed`] destination and its `↑N` — GLD-063.
///
/// **Refs only** (spec review r2, finding 10): one `for-each-ref` for every
/// branch's upstream and its count, `git remote` and one `remote get-url`
/// per remote used, and for a branch with no upstream on its remote one
/// `rev-list --count <branch> --not --remotes=<remote>` — Push Commits'
/// own reachability rule (`git::ahead_args`). No `git status` runs, so
/// nothing here can refresh or write the index.
pub fn destinations(root: &Path, list: &mut [Worktree]) -> Result<(), GitError> {
    let remotes = crate::git::remotes(root)?;
    if remotes.is_empty() {
        return Ok(());
    }
    let tracked = upstreams(root)?;
    let mut urls: std::collections::HashMap<String, Option<String>> =
        std::collections::HashMap::new();
    for tree in list.iter_mut().filter(|tree| tree.shown()) {
        let Some(branch) = tree.branch.clone() else {
            continue;
        };
        let upstream = tracked.get(&branch);
        let ahead = crate::git::Ahead {
            branch: Some(branch.clone()),
            remotes: remotes.clone(),
            upstream: upstream
                .filter(|upstream| !upstream.remote.is_empty() && upstream.remote != ".")
                .map(|upstream| format!("{}/{}", upstream.remote, upstream.branch)),
            commits: Vec::new(),
        };
        let Some(destination) = ahead.destination(None) else {
            continue;
        };
        let url = urls
            .entry(destination.remote.clone())
            .or_insert_with(|| remote_url(root, &destination.remote))
            .clone();
        tree.ahead = match upstream {
            Some(upstream) if !destination.set_upstream && !upstream.gone => Some(upstream.ahead),
            _ => unpushed(root, &branch, &destination.remote).ok(),
        };
        tree.push = Some(Pushed {
            remote: destination.remote,
            url,
            branch: destination.onto,
        });
    }
    Ok(())
}

/// A branch's upstream as `for-each-ref` prints it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Upstream {
    remote: String,
    branch: String,
    ahead: u32,
    gone: bool,
}

/// Every local branch's upstream and how far ahead of it it is, in one
/// `for-each-ref`.
fn upstreams(root: &Path) -> Result<std::collections::HashMap<String, Upstream>, GitError> {
    let out = run(
        root,
        &[
            "for-each-ref",
            // `lstrip=2`, not `short`: a tag named like the branch makes
            // `short` say `heads/bar` (code review r1, bugs+impl-8).
            "--format=%(refname:lstrip=2)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(upstream:track,nobracket)",
            "refs/heads",
        ],
    )?;
    Ok(read_upstreams(&out))
}

/// What [`upstreams`]' format printed: `branch\0remote\0refs/heads/x\0track`.
fn read_upstreams(bytes: &[u8]) -> std::collections::HashMap<String, Upstream> {
    String::from_utf8_lossy(bytes)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\0');
            let branch = fields.next()?.to_owned();
            let remote = fields.next()?.to_owned();
            let remote_ref = fields.next()?;
            let track = fields.next().unwrap_or_default();
            let remote_branch = remote_ref.strip_prefix("refs/heads/")?.to_owned();
            if remote.is_empty() || remote_branch.is_empty() {
                return None;
            }
            let ahead = track
                .split(", ")
                .find_map(|part| part.strip_prefix("ahead "))
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0);
            Some((
                branch,
                Upstream {
                    remote,
                    branch: remote_branch,
                    ahead,
                    gone: track.trim() == "gone",
                },
            ))
        })
        .collect()
}

/// Where `remote` pushes — its `pushurl` when it has one — which is where
/// the merge request lives (code review r2, bugs+impl-4).
fn remote_url(root: &Path, remote: &str) -> Option<String> {
    let out = run(root, &["remote", "get-url", "--push", "--", remote]).ok()?;
    let url = String::from_utf8_lossy(&out).trim().to_owned();
    (!url.is_empty()).then_some(url)
}

/// The commits on `branch` no ref of `remote` holds.
fn unpushed(root: &Path, branch: &str, remote: &str) -> Result<u32, GitError> {
    let out = run(
        root,
        &[
            "rev-list",
            "--count",
            &format!("refs/heads/{branch}"),
            "--not",
            &format!("--remotes={remote}"),
            "--",
        ],
    )?;
    String::from_utf8_lossy(&out)
        .trim()
        .parse()
        .map_err(|_| GitError::Failed("rev-list answered no count".into()))
}
