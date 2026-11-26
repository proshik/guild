//! A source choice is a read. Updating is an identity-bound, serialized fetch/merge.
use super::{GitError, Said, Write, run, write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub(crate) const FETCH_DEADLINE: Duration = Duration::from_secs(120);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub reference: String,
    pub oid: String,
    pub upstream: String,
    pub remote: Option<(String, String)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub root: PathBuf,
    /// The checkout `root` is in, canonical — what auto fast-forward
    /// matches an opened root to a checkout by, read here off the window's
    /// thread. `None` when git would not say.
    pub checkout: Option<PathBuf>,
    pub head: Option<(String, String)>,
    pub sources: Vec<Source>,
    pub default: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub root: PathBuf,
    pub head: (String, String),
    pub selected: String,
    pub source: Source,
}
fn text(root: &Path, args: &[&str]) -> Result<String, GitError> {
    Ok(String::from_utf8_lossy(&run(root, args)?).trim().into())
}
fn refused(message: &str) -> GitError {
    GitError::Failed(message.into())
}
// Invert the configured fetch destination; a tracking name need not match
// the remote branch name. Unknown/ambiguous mappings stay unavailable.
fn mapped_source(spec: &str, reference: &str) -> Option<String> {
    if !reference.starts_with("refs/remotes/") {
        return None;
    }
    let (source, destination) = spec.trim_start_matches('+').split_once(':')?;
    if !source.starts_with("refs/heads/") {
        return None;
    }
    if let Some((prefix, suffix)) = destination.split_once('*') {
        let name = reference.strip_prefix(prefix)?.strip_suffix(suffix)?;
        let (before, after) = source.split_once('*')?;
        Some(format!("{before}{name}{after}"))
    } else {
        (destination == reference).then(|| source.to_owned())
    }
}
impl Snapshot {
    pub fn read(root: &Path) -> Result<Self, GitError> {
        let root = root.canonicalize().map_err(|e| refused(&e.to_string()))?;
        let head = text(&root, &["symbolic-ref", "--quiet", "HEAD"])
            .ok()
            .zip(text(&root, &["rev-parse", "--verify", "HEAD^{commit}"]).ok());
        let remotes = text(&root, &["remote"])?;
        let mappings: Vec<_> = remotes
            .lines()
            .flat_map(|remote| {
                text(
                    &root,
                    &["config", "--get-all", &format!("remote.{remote}.fetch")],
                )
                .unwrap_or_default()
                .lines()
                .map(|spec| (remote.to_owned(), spec.to_owned()))
                .collect::<Vec<_>>()
            })
            .collect();
        let rows = text(
            &root,
            &[
                "for-each-ref",
                "--format=%(refname)%00%(objectname)%00%(upstream)%00%(symref)",
                "refs/heads/",
                "refs/remotes/",
            ],
        )?;
        let mut sources = Vec::new();
        let mut default = None;
        for row in rows.lines() {
            let f: Vec<_> = row.split('\0').collect();
            if f.len() != 4 {
                continue;
            }
            if !f[3].is_empty() {
                if f[0] == "refs/remotes/origin/HEAD" || default.is_none() {
                    default = Some(f[3].into());
                }
                continue;
            }
            let mut matches: Vec<_> = mappings
                .iter()
                .filter_map(|(remote, spec)| {
                    mapped_source(spec, f[0]).map(|source| (remote.clone(), source))
                })
                .collect();
            matches.sort();
            matches.dedup();
            let remote = (matches.len() == 1).then(|| matches.remove(0));
            sources.push(Source {
                reference: f[0].into(),
                oid: f[1].into(),
                upstream: f[2].into(),
                remote,
            });
        }
        Ok(Self {
            checkout: super::checkout_of(&root),
            root,
            head,
            sources,
            default,
        })
    }
    pub fn request(&self, selected: &str) -> Result<Request, GitError> {
        let head = self
            .head
            .clone()
            .ok_or_else(|| refused("Update requires an attached branch with a commit."))?;
        let selected_source = self
            .sources
            .iter()
            .find(|s| s.reference == selected)
            .ok_or_else(|| refused("Selected source no longer exists. Choose a source."))?;
        let source = if head.0 == selected || !selected_source.upstream.is_empty() {
            self.sources
                .iter()
                .find(|s| {
                    !selected_source.upstream.is_empty() && s.reference == selected_source.upstream
                })
                .ok_or_else(|| {
                    refused("The current branch has no available upstream. Choose another source.")
                })?
        } else {
            selected_source
        };
        if source.reference.starts_with("refs/remotes/") && source.remote.is_none() {
            return Err(refused(
                "The selected remote is unavailable. Choose another source.",
            ));
        }
        Ok(Request {
            root: self.root.clone(),
            head,
            selected: selected.into(),
            source: source.clone(),
        })
    }
}
/// Whether a merge, rebase, cherry-pick, revert or sequence is under way in
/// the worktree at `root` — read where git keeps each, which for a linked
/// worktree is outside it. Shared with auto fast-forward's preflight.
pub(super) fn operation_in_progress(root: &Path) -> Result<bool, GitError> {
    for name in [
        "MERGE_HEAD",
        "REBASE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
        "sequencer",
    ] {
        let path = text(root, &["rev-parse", "--git-path", name])?;
        if root.join(path).exists() {
            return Ok(true);
        }
    }
    Ok(false)
}
fn preflight(root: &Path, request: &Request, after_fetch: bool) -> Result<Source, GitError> {
    let now = Snapshot::read(root)?;
    if now.root != request.root || now.head.as_ref() != Some(&request.head) {
        return Err(refused(
            "The originating branch or HEAD changed. Retry from the current branch.",
        ));
    }
    if operation_in_progress(root)? {
        return Err(refused("Finish the current Git operation before updating."));
    }
    if !run(root, &["status", "--porcelain=v1", "--untracked-files=all"])?.is_empty() {
        return Err(refused(
            "Commit or remove working tree, index and untracked changes before updating.",
        ));
    }
    let resolved = now.request(&request.selected)?.source;
    if resolved.reference != request.source.reference
        || resolved.remote != request.source.remote
        || (!after_fetch && resolved.oid != request.source.oid)
    {
        return Err(refused(
            "The selected source changed. Choose it again before updating.",
        ));
    }
    Ok(resolved)
}
/// The caller owns the project's worker; no other write may interleave these steps.
/// Every attempted command is reported, including a failure's complete output.
pub fn execute(
    root: &Path,
    request: &Request,
    report: impl FnMut(&Write, &Result<Said, GitError>),
) -> Result<(), GitError> {
    execute_guarded(root, request, || true, report)
}
/// Recheck Guild's unsaved-buffer guard before each mutation, including after fetch.
pub fn execute_guarded(
    root: &Path,
    request: &Request,
    buffers_clean: impl FnMut() -> bool,
    report: impl FnMut(&Write, &Result<Said, GitError>),
) -> Result<(), GitError> {
    execute_with_timeout(root, request, buffers_clean, report, FETCH_DEADLINE)
}

/// A bounded fetch shares the project's serialized worker with the merge.
pub fn execute_with_timeout(
    root: &Path,
    request: &Request,
    mut buffers_clean: impl FnMut() -> bool,
    mut report: impl FnMut(&Write, &Result<Said, GitError>),
    timeout: Duration,
) -> Result<(), GitError> {
    if !buffers_clean() {
        return Err(refused("Save unsaved files before updating the branch."));
    }
    let mut source = preflight(root, request, false)?;
    if let Some((remote, branch)) = &source.remote {
        let fetch = Write::FetchSource {
            remote: remote.clone(),
            refspec: format!("{branch}:{}", source.reference),
            refuse_forced: false,
        };
        let result = write_fetch(root, &fetch, timeout);
        report(&fetch, &result);
        result?;
        source = preflight(root, request, true)?;
    }
    if !buffers_clean() {
        return Err(refused("Save unsaved files before updating the branch."));
    }
    let merge = Write::MergeSource {
        oid: source.oid,
        message: format!("Merge {} into {}", source.reference, request.head.0),
    };
    let result = write(root, &merge);
    report(&merge, &result);
    result?;
    Ok(())
}

/// Only the named update fetch uses this gateway; unrelated writes retain theirs.
pub(super) fn write_fetch(root: &Path, write: &Write, timeout: Duration) -> Result<Said, GitError> {
    use guild_session::process::ProcessGroup;
    use std::{
        fs::File,
        io::{Read, Write as _},
        os::fd::AsRawFd,
        sync::atomic::AtomicBool,
        thread,
        time::Instant,
    };
    if !matches!(write, Write::FetchSource { .. }) {
        return Err(refused("Expected update fetch."));
    }
    let started = Instant::now();
    let directory = File::open(root).map_err(|e| refused(&e.to_string()))?;
    let mut command = super::git_command();
    command.current_dir(root).args(write.args());
    let proxy = if let Write::FetchSource { remote, .. } = write {
        crate::network::git(&mut command, root, remote, false, true)?
    } else {
        crate::network::GitPolicy::default()
    };
    super::clone::noninteractive(&mut command);
    super::clone::configure_ssh(
        &mut command,
        directory.as_raw_fd(),
        &AtomicBool::new(false),
        timeout,
    )
    .map_err(|e| refused(&e))?;
    let mut group = ProcessGroup::spawn(&mut command).map_err(|e| refused(&e.to_string()))?;
    let drain = |mut stream: Box<dyn Read + Send>| {
        let mut kept = Vec::new();
        let mut chunk = [0; 4096];
        while let Ok(n) = stream.read(&mut chunk) {
            if n == 0 {
                break;
            }
            let retain = n.min((64 * 1024usize).saturating_sub(kept.len()));
            let _ = kept.write_all(&chunk[..retain]);
        }
        proxy.redact(&String::from_utf8_lossy(&kept))
    };
    thread::scope(|scope| {
        let stdout = group.take_stdout().expect("piped fetch stdout");
        let stderr = group.take_stderr().expect("piped fetch stderr");
        let out = scope.spawn(|| drain(Box::new(stdout)));
        let err = scope.spawn(|| drain(Box::new(stderr)));
        let result = loop {
            match group.try_wait() {
                Ok(Some(status)) => break Ok(status.success()),
                Ok(None) => {}
                Err(error) => break Err(error.to_string()),
            }
            if started.elapsed() >= timeout {
                break Err(
                    "Update fetch timed out; no merge was run. Retry after checking the remote."
                        .into(),
                );
            }
            thread::sleep(Duration::from_millis(20));
        };
        let stopped = group.stop();
        let said = Said {
            out: out.join().unwrap_or_default(),
            err: err.join().unwrap_or_default(),
        };
        stopped.map_err(|e| refused(&e.to_string()))?;
        match result {
            Ok(true) => Ok(said),
            Ok(false) => Err(refused(&said.text())),
            Err(error) => Err(refused(&format!("{error}\n{}", said.text()))),
        }
    })
}

pub use guild_session::settings::{
    BranchUpdateKind as ReceiptKind, BranchUpdateReceipt as Receipt,
};

/// Keep recovery identity separate from the currently selected project/branch.
pub fn execute_recorded(
    root: &Path,
    request: &Request,
    buffers_clean: impl FnMut() -> bool,
    mut report: impl FnMut(&Write, &Result<Said, GitError>),
) -> Receipt {
    let mut merged = None;
    let mut fetch_failed = false;
    let mut diagnostic = String::new();
    let result = execute_guarded(root, request, buffers_clean, |write, result| {
        if matches!(write, Write::FetchSource { .. }) && result.is_err() {
            fetch_failed = true;
        }
        if let Write::MergeSource { oid, .. } = write {
            merged = Some(oid.clone());
        }
        if let Ok(said) = result {
            diagnostic.push_str(&said.text());
        }
        report(write, result);
    });
    let after = text(root, &["rev-parse", "--verify", "HEAD^{commit}"]).ok();
    let kind = match &result {
        Err(_)
            if merged.is_some() && text(root, &["rev-parse", "--verify", "MERGE_HEAD"]).is_ok() =>
        {
            ReceiptKind::Conflict
        }
        Err(_) if fetch_failed => ReceiptKind::FetchRefused,
        Err(_) => ReceiptKind::Refused,
        Ok(_) if after.as_ref() == Some(&request.head.1) => ReceiptKind::Unchanged,
        Ok(_) if after == merged => ReceiptKind::FastForward,
        Ok(_) => ReceiptKind::MergeCommit,
    };
    if let Err(error) = result {
        diagnostic.push_str(&error.to_string());
    }
    Receipt {
        root: request.root.clone(),
        before: request.head.1.clone(),
        after,
        kind,
        diagnostic: bounded_diagnostic(&diagnostic),
    }
}

// The complete command output belongs to Console. A persisted recovery card
// keeps a UTF-8-safe excerpt, even for huge diffstats or conflict lists.
fn bounded_diagnostic(text: &str) -> String {
    let mut out = String::new();
    let mut lines = 1;
    for (count, ch) in text.chars().enumerate() {
        if count == 480 || (ch == '\n' && lines == 5) {
            out.push_str("\n…");
            break;
        }
        out.push(ch);
        if ch == '\n' {
            lines += 1;
        }
    }
    out
}
