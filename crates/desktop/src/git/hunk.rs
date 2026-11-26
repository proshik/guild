//! Stage Hunk and Unstage Hunk — decision 55: the window computes the index
//! entry's new content, and git swaps it in only over the entry it was
//! computed from.
//!
//! **The patch is binary with a full index line**, `index <old>..<new>
//! <mode>`, a `literal` forward and back. git 2.50.1 takes `index.lock`
//! before it reads the index (`apply.c` 4828–4836) and, for a binary patch,
//! hashes the entry it read and refuses unless it is `<old>` (3182–3192),
//! and refuses an entry of another type than `<mode>` (3790–3797): an outside
//! `git add`, or a symlink with the same blob, between this window's reading
//! and the write is refused under git's own lock instead of raced past. A
//! text patch would be placed by context, and git placed one at an offset
//! without a word. The mode is the one read in the checkout's turn; an
//! executable bit changed from outside in the instant between is the one
//! residue, and it is the shown entry's bit that is kept.
//!
//! **A converted file takes no hunks** — decision 55's fourth promise.
//! `hash-object` is asked only with `--no-filters`; whether git would store
//! the bytes as they are is
//! decided from the attributes and `core.autocrlf` alone ([`byte_preserving`]),
//! because `hash-object --path` would run a clean driver whose name an
//! attribute value can hide (`filter=unset` reads as unset).
//!
//! **The patch names the path from the repository's top**: git applies a
//! patch in a folder of its repository only to paths under that folder,
//! spelled from the top, and skips the rest with a success. The write reads
//! the entry back afterwards and calls a write that changed nothing a
//! refusal.
//!
//! **Nothing here reads git's words.** git's messages follow the person's
//! locale, so a refusal is classified by reading the entry again
//! ([`stale`]), never by matching what git printed.

use std::io::{Seek as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{GitError, Said, git_command};

/// What a hunk write carries — everything it was computed from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    /// Relative to the root the write runs in, as the diff names it.
    pub path: PathBuf,
    /// The index entry's content the postimage was computed from.
    pub preimage: Arc<[u8]>,
    /// The entry's new content.
    pub postimage: Arc<[u8]>,
    /// The disk's bytes when the shown working text was the disk's,
    /// checked again just before the write. `None` for a buffer, whose
    /// snapshot the window checks before it sends the write.
    pub disk: Option<Arc<[u8]>>,
    /// `HEAD`'s content of the path when the offer was made — for Stage as
    /// for Unstage: the write is refused unless `HEAD` still holds it as a
    /// regular file (review r3 `bugs+impl-4`).
    pub head: Arc<[u8]>,
}

/// The argv of both hunk writes: the patch is on stdin, the path inside it.
pub(super) const ARGS: [&str; 5] = ["apply", "--cached", "--no-3way", "--whitespace=nowarn", "-"];

/// Why a hunk write is not run — decided on the git thread just before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Something it was computed from is no longer so: the entry, the disk,
    /// or `HEAD`.
    Stale,
    /// git converts this file's content when it stores it.
    Converted,
    /// The content is past the size a hunk is computed for.
    WholeFile,
    /// git could not be asked: it did not answer within [`DEADLINE`], or
    /// did not start.
    Failed(GitError),
}

/// The size past which no hunk is computed or written: the diff's.
pub const LIMIT: usize = crate::diff::DIFF_LIMIT as usize;

/// How long one git call of a hunk may take before it is ended — review r3
/// `adversarial-6`: a FIFO where `.gitattributes` should be blocks
/// `check-attr` and `apply` on its open, and the call holds the checkout's
/// turn and a reader permit while it waits.
pub const DEADLINE: Duration = Duration::from_secs(10);

/// What a hunk's git call past [`DEADLINE`] answers — the window's words.
pub const UNANSWERED: &str = "git did not answer within 10 seconds";

/// Whether an error is a hunk call that ran past [`DEADLINE`].
pub fn unanswered(err: &GitError) -> bool {
    matches!(err, GitError::Failed(words) if words == UNANSWERED)
}

#[cfg(test)]
thread_local! {
    /// A shorter deadline for the calls of this thread, for the tests that
    /// wait one out.
    pub(crate) static DEADLINE_FOR_TEST: std::cell::Cell<Option<Duration>> =
        const { std::cell::Cell::new(None) };
}

/// A shorter deadline for every call in one root, from any thread — for the
/// tests of the git thread.
#[cfg(test)]
pub(crate) static DEADLINE_IN_ROOT: std::sync::Mutex<Option<(PathBuf, Duration)>> =
    std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    /// What runs between a successful `apply` and its read-back, for the
    /// test of a write that cannot be verified.
    pub(crate) static AFTER_APPLY: std::cell::RefCell<Option<Box<dyn Fn()>>> =
        const { std::cell::RefCell::new(None) };
}

fn deadline(root: &Path) -> Duration {
    #[cfg(test)]
    {
        if let Some(deadline) = DEADLINE_FOR_TEST.with(std::cell::Cell::get) {
            return deadline;
        }
        if let Some((at, deadline)) = &*DEADLINE_IN_ROOT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            && at == root
        {
            return *deadline;
        }
    }
    let _ = root;
    DEADLINE
}

/// What a write answers when `apply` reported success and the entry could
/// not be read back — the index has most likely changed. Review r4
/// `bugs+impl-3`.
pub const UNVERIFIED: &str = "git applied the change, but the index could not be read back";

/// What a write answers when `apply` ran past the deadline and the entry
/// could not be read back either: whether it was written is not known.
/// Review r5 `docs+tests-8`.
pub const UNCONFIRMED: &str = "git did not answer in time and the index could not be read back; the change may or may not have been written";

/// Whether an error is a write that reported success and could not be
/// verified.
pub fn unverified(err: &GitError) -> bool {
    matches!(err, GitError::Failed(words) if words == UNVERIFIED)
}

/// Whether an error is a write that ran past its deadline and could not be
/// verified.
pub fn unconfirmed(err: &GitError) -> bool {
    matches!(err, GitError::Failed(words) if words == UNCONFIRMED)
}

/// What runs in one root between a successful `apply` and its read-back,
/// on any thread — for the test of the git thread's outcome.
#[cfg(test)]
#[allow(clippy::type_complexity)]
pub(crate) static AFTER_APPLY_IN_ROOT: std::sync::Mutex<Option<(PathBuf, Box<dyn Fn() + Send>)>> =
    std::sync::Mutex::new(None);

/// The checks before the write, in the checkout's turn.
pub(super) fn settle(root: &Path, hunk: &Hunk) -> Result<(), Refusal> {
    let Some(path) = hunk.path.to_str() else {
        return Err(Refusal::Stale);
    };
    if path.contains('\0') {
        return Err(Refusal::Stale);
    }
    if [&hunk.postimage, &hunk.preimage, &hunk.head]
        .iter()
        .any(|bytes| bytes.len() > LIMIT)
    {
        return Err(Refusal::WholeFile);
    }
    // A partial clone with a git that cannot be told not to fetch: no read
    // of an object is made at all.
    if fetches_lazily(root).map_err(Refusal::Failed)? {
        return Err(Refusal::WholeFile);
    }
    // The entry: one, at stage 0, a regular file, holding the preimage.
    if stale(root, hunk).map_err(Refusal::Failed)? {
        return Err(Refusal::Stale);
    }
    // The working file: a regular file, never followed through a link —
    // for every view, a buffer's and the Staged diff's included.
    let working = match read_regular(root, &hunk.path, LIMIT) {
        Some(bytes) => {
            if hunk.disk.as_deref().is_some_and(|disk| disk != &bytes[..]) {
                return Err(Refusal::Stale);
            }
            bytes
        }
        None => return Err(Refusal::Stale),
    };
    // A binary working file takes no hunk in any view, the Staged diff's
    // included, whatever its blobs are.
    if !text(&working) || !text(&hunk.postimage) {
        return Err(Refusal::WholeFile);
    }
    // `HEAD` still a regular file holding what the offer was made over.
    if head_moved(root, hunk).map_err(Refusal::Failed)? {
        return Err(Refusal::Stale);
    }
    let conversion = conversion(root, path).map_err(Refusal::Failed)?;
    if !conversion.keeps(&[&hunk.postimage, &hunk.preimage, &working]) {
        return Err(Refusal::Converted);
    }
    Ok(())
}

/// One entry of the index or a tree: its mode and object id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub mode: String,
    pub oid: String,
}

impl Entry {
    fn regular(&self) -> bool {
        matches!(self.mode.as_str(), "100644" | "100755")
    }
}

/// The path's one index entry at stage 0, when it is a regular file.
/// `Ok(None)` for no entry, a conflict, a symlink or a submodule.
pub fn entry(root: &Path, path: &str) -> Result<Option<Entry>, GitError> {
    let Some(listed) = listed(
        root,
        &[
            "--literal-pathspecs",
            "ls-files",
            "--stage",
            "-z",
            "--",
            path,
        ],
    )?
    else {
        return Ok(None);
    };
    Ok(only(&listed).and_then(|entry| {
        // `<mode> <oid> <stage>\t<path>`
        let mut fields = entry.split(['\t', ' ']);
        let (mode, oid, stage) = (fields.next()?, fields.next()?, fields.next()?);
        let entry = Entry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
        };
        (stage == "0" && entry.regular()).then_some(entry)
    }))
}

/// The path's entry in `HEAD`, when it is a regular file.
pub fn head_entry(root: &Path, path: &str) -> Result<Option<Entry>, GitError> {
    let Some(listed) = listed(
        root,
        &["--literal-pathspecs", "ls-tree", "-z", "HEAD", "--", path],
    )?
    else {
        return Ok(None);
    };
    Ok(only(&listed).and_then(|entry| {
        // `<mode> <type> <oid>\t<path>`
        let mut fields = entry.split(['\t', ' ']);
        let (mode, kind, oid) = (fields.next()?, fields.next()?, fields.next()?);
        let entry = Entry {
            mode: mode.to_owned(),
            oid: oid.to_owned(),
        };
        (kind == "blob" && entry.regular()).then_some(entry)
    }))
}

/// The one NUL-terminated record of a listing, when there is exactly one.
fn only(listed: &[u8]) -> Option<&str> {
    let records: Vec<&[u8]> = listed
        .split(|b| *b == 0)
        .filter(|e| !e.is_empty())
        .collect();
    let [record] = records[..] else {
        return None;
    };
    std::str::from_utf8(record).ok()
}

/// The size of an object, without reading it.
fn object_size(root: &Path, oid: &str) -> Result<Option<usize>, GitError> {
    Ok(listed(root, &["cat-file", "-s", oid])?
        .and_then(|size| String::from_utf8_lossy(&size).trim().parse().ok()))
}

/// A working file's bytes when it is a regular file within `limit`: opened
/// beneath `root` without following a link at any component and without
/// blocking on a FIFO (`file_operations::open_beneath`), then checked to be
/// a regular file before anything is read.
pub(crate) fn read_regular(root: &Path, path: &Path, limit: usize) -> Option<Vec<u8>> {
    use std::io::Read as _;
    let mut file = crate::file_operations::open_beneath(root, path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return None;
    }
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    std::io::Read::by_ref(&mut file)
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= limit).then_some(bytes)
}

/// Whether bytes are text as the diff reads them: no NUL near the start,
/// UTF-8 throughout.
fn text(bytes: &[u8]) -> bool {
    matches!(
        crate::highlight::Text::of(bytes),
        crate::highlight::Text::Source { .. }
    )
}

/// Whether the index entry is no longer the preimage: gone, conflicted, not
/// a regular file, or other content.
pub fn stale(root: &Path, hunk: &Hunk) -> Result<bool, GitError> {
    let Some(path) = hunk.path.to_str() else {
        return Ok(true);
    };
    let Some(entry) = entry(root, path)? else {
        return Ok(true);
    };
    Ok(object_id(root, &hunk.preimage)? != entry.oid)
}

/// Whether `HEAD`'s entry for the path is no longer the regular file the
/// offer was made over — checked before the write and, for an Unstage,
/// since git has no command that holds `HEAD` and the index at once, again
/// after it.
pub fn head_moved(root: &Path, hunk: &Hunk) -> Result<bool, GitError> {
    let Some(path) = hunk.path.to_str() else {
        return Ok(true);
    };
    match head_entry(root, path)? {
        Some(entry) => Ok(object_id(root, &hunk.head)? != entry.oid),
        None => Ok(true),
    }
}

/// What git may do to the bytes of a path when it stores them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Conversion {
    /// Nothing: the bytes are stored as they are.
    None,
    /// Line endings: only content with no carriage return is left alone.
    Endings,
    /// A program, an `$Id$` or an encoding — or git could not say.
    Other,
}

impl Conversion {
    /// Whether git would store every one of `contents` byte for byte.
    pub fn keeps(self, contents: &[&[u8]]) -> bool {
        match self {
            Conversion::None => true,
            Conversion::Endings => contents.iter().all(|c| !c.contains(&b'\r')),
            Conversion::Other => false,
        }
    }
}

/// What git would do to the bytes at `path` — decided from what git
/// reports, running nothing, and refusing what it cannot tell.
/// `check-attr --all` lists every attribute that is set, unset or valued for
/// the path (unspecified ones are left out), so a filter literally named
/// `unspecified` or `unset` is listed as well:
/// - `filter`, `ident` and `working-tree-encoding` listed in any state —
///   [`Conversion::Other`];
/// - `text`, `eol` and the legacy `crlf` listed, or `core.autocrlf` present
///   and not false (a valueless key is true) — [`Conversion::Endings`].
pub fn conversion(root: &Path, path: &str) -> Result<Conversion, GitError> {
    let Some(listed) = listed(root, &["check-attr", "--all", "-z", "--", path])? else {
        return Ok(Conversion::Other);
    };
    // `<path>\0<attribute>\0<value>\0`, three fields each.
    let fields: Vec<&[u8]> = listed.split(|b| *b == 0).collect();
    let mut endings = false;
    for record in fields.chunks(3).filter(|r| r.len() == 3) {
        match record[1] {
            b"filter" | b"ident" | b"working-tree-encoding" => return Ok(Conversion::Other),
            b"text" | b"eol" | b"crlf" => endings = true,
            _ => {}
        }
    }
    let asked = read(root, &["config", "--get", "core.autocrlf"])?;
    if asked.status.success() {
        let value = String::from_utf8_lossy(&asked.stdout)
            .trim()
            .to_ascii_lowercase();
        if !matches!(value.as_str(), "false" | "no" | "off" | "0") {
            endings = true;
        }
    } else if asked.status.code() != Some(1) || !asked.stderr.is_empty() {
        // Exit 1 with nothing said is a key not set; anything else is not
        // known.
        return Ok(Conversion::Other);
    }
    Ok(if endings {
        Conversion::Endings
    } else {
        Conversion::None
    })
}

/// Whether git would store every one of `contents` at `path` byte for byte.
pub fn byte_preserving(root: &Path, path: &str, contents: &[&[u8]]) -> bool {
    conversion(root, path).is_ok_and(|conversion| conversion.keeps(contents))
}

/// The write: the patch built from git's own ids, on an owned stdin file.
pub(super) fn write(root: &Path, hunk: &Hunk) -> Result<Said, GitError> {
    let Some(path) = hunk.path.to_str() else {
        return Err(GitError::Failed("Unsupported non-UTF-8 path".into()));
    };
    let changed = || GitError::Failed("The index entry changed".into());
    let entry = entry(root, path)?.ok_or_else(changed)?;
    let prefix = listed(root, &["rev-parse", "--show-prefix"])?
        .ok_or_else(|| GitError::Failed("git could not say where the folder is".into()))?;
    let prefix = String::from_utf8(prefix)
        .map_err(|_| GitError::Failed("Unsupported non-UTF-8 path".into()))?;
    let from_top = format!("{}{path}", prefix.trim_end_matches('\n'));
    let old = object_id(root, &hunk.preimage)?;
    let new = object_id(root, &hunk.postimage)?;
    let patch = patch(
        &from_top,
        &old,
        &new,
        &entry.mode,
        &hunk.preimage,
        &hunk.postimage,
    );
    let unverified = || GitError::Failed(UNVERIFIED.into());
    let applied = call(root, &ARGS, &patch, false);
    #[cfg(test)]
    {
        AFTER_APPLY.with(|hook| {
            if let Some(hook) = &*hook.borrow() {
                hook();
            }
        });
        if let Some((at, hook)) = &*AFTER_APPLY_IN_ROOT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            && at == root
        {
            hook();
        }
    }
    let output = match applied {
        Ok(output) => output,
        // Ended past the deadline: the entry says whether it was swapped
        // in — still the preimage is a write that did not happen.
        Err(err) if self::unanswered(&err) => {
            return match self::entry(root, path) {
                Ok(Some(now)) if now.oid == old => Err(err),
                Ok(Some(now)) if now.oid == new => Ok(Said::default()),
                _ => Err(GitError::Failed(UNCONFIRMED.into())),
            };
        }
        Err(err) => return Err(err),
    };
    let said = Said {
        out: String::from_utf8_lossy(&output.stdout).into_owned(),
        err: String::from_utf8_lossy(&output.stderr).into_owned(),
    };
    // git refused: `apply` writes the index whole or not at all.
    if !output.status.success() {
        return Err(GitError::Failed(said.text()));
    }
    // A success that wrote nothing is a refusal: git skips a patch outside
    // the folder it runs in and still answers 0. A read-back that fails, or
    // finds a third content, is a write that may have happened.
    match self::entry(root, path) {
        Ok(Some(now)) if now.oid == new => Ok(said),
        Ok(Some(now)) if now.oid == old => Err(changed()),
        _ => Err(unverified()),
    }
}

/// `git hash-object --stdin --no-filters`: the id of raw bytes. A read —
/// nothing is written without `-w`, no attribute is consulted — and the
/// repository's hash function is git's to choose.
fn object_id(root: &Path, content: &[u8]) -> Result<String, GitError> {
    let output = call(
        root,
        &["hash-object", "--stdin", "--no-filters"],
        content,
        true,
    )?;
    if !output.status.success() {
        return Err(GitError::Failed(
            String::from_utf8_lossy(&output.stderr).into_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The status walk that settles a hunk write, through the same door as the
/// write: bounded, no monitor, nothing fetched. Review r4 `bugs+impl-2`: the
/// barrier waits for this map, and an unbounded walk would hold it and
/// every later ask of the git thread.
pub fn status(root: &Path) -> Result<Vec<u8>, GitError> {
    // Renames as the ordinary walk finds them — the barrier ticks and plans
    // from this map, and a rename split into its two paths would reach
    // `to_tick` as a deletion never seen (the person's diff review of r5).
    // Except in a partial clone: an inexact rename needs its source's bytes,
    // an unrelated source may be a promisor object, and git then fails the
    // whole walk rather than fetch it — and 2.45.0's rename prefetch fetches
    // past `GIT_NO_LAZY_FETCH` (review r5 `bugs+impl-2`). There no rename
    // is detected, and `changes::to_tick` keeps the split harmless.
    let split = partial_clone(root).unwrap_or(true);
    let mut args = vec!["status", "--porcelain=v2", "-z", "--untracked-files=normal"];
    if split {
        args.push("--no-renames");
    }
    let output = read(root, &args)?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        let words = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if words.contains("not a git repository") {
            Err(GitError::NoRepository)
        } else {
            Err(GitError::Failed(words))
        }
    }
}

/// Whether git could fetch a missing object here: a partial clone — a
/// promisor remote, or `extensions.partialClone` — read by a git older than
/// 2.45.0, which ignores `GIT_NO_LAZY_FETCH`. Such a file takes no hunks.
fn fetches_lazily(root: &Path) -> Result<bool, GitError> {
    if no_lazy_fetch_understood() {
        return Ok(false);
    }
    partial_clone(root)
}

/// Whether the repository is a partial clone as git reads one.
pub fn partial_clone(root: &Path) -> Result<bool, GitError> {
    let Some(listed) = listed(
        root,
        &[
            "config",
            "--get-regexp",
            r"^(extensions\.partialclone|remote\..*\.(promisor|partialclonefilter))$",
        ],
    )?
    else {
        return Ok(false);
    };
    // A filter alone makes a remote a promisor (review r5 `docs+tests-3`).
    Ok(String::from_utf8_lossy(&listed).lines().any(|line| {
        let (key, value) = line.split_once(' ').unwrap_or((line, "true"));
        let key = key.to_ascii_lowercase();
        key == "extensions.partialclone"
            || key.ends_with(".partialclonefilter")
            || !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "false" | "no" | "off" | "0"
            )
    }))
}

/// Whether the git on the login `PATH` honours `GIT_NO_LAZY_FETCH` — asked
/// once a run.
fn no_lazy_fetch_understood() -> bool {
    static UNDERSTOOD: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *UNDERSTOOD.get_or_init(|| {
        call(Path::new("/"), &["version"], b"", true)
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| {
                understands_no_lazy_fetch(&String::from_utf8_lossy(&output.stdout))
            })
    })
}

/// `git version 2.45.0`, Apple's suffix and all: 2.45 or later.
pub fn understands_no_lazy_fetch(version: &str) -> bool {
    let mut numbers = version
        .trim()
        .trim_start_matches("git version ")
        .split(['.', ' '])
        .map(|part| part.parse::<u32>().ok());
    match (numbers.next().flatten(), numbers.next().flatten()) {
        (Some(major), Some(minor)) => (major, minor) >= (2, 45),
        _ => false,
    }
}

/// A read's standard output when git answered 0, `None` when it refused.
fn listed(root: &Path, args: &[&str]) -> Result<Option<Vec<u8>>, GitError> {
    let output = read(root, args)?;
    Ok(output.status.success().then_some(output.stdout))
}

/// A read with nothing on stdin.
fn read(root: &Path, args: &[&str]) -> Result<std::process::Output, GitError> {
    call(root, args, b"", true)
}

/// Every git call of a hunk, the write's included, and the status read that
/// settles one — reviews r3 and r4:
/// - **`core.fsmonitor=`, empty**: a configured monitor can hold a call, and
///   git 2.50.1 runs it from `apply --cached` (measured) and from anything
///   that refreshes the index. This bounds time; it is not a promise that no
///   configured program runs — hooks run as everywhere in Guild (decision
///   55). Empty, not `false`: git
///   through 2.35.1 reads the key as a hook's path, so `false` would be run,
///   and treats an empty path as unset (`git_config_get_fsmonitor`); from
///   2.36 the key is a bool first, and an empty value is false. No other
///   setting these commands read starts one: `apply` runs no hook,
///   `cat-file blob` no textconv, `hash-object --no-filters` no filter, and
///   nothing here pages or edits.
/// - **`GIT_NO_LAZY_FETCH=1`**: in a partial clone a missing object would
///   be fetched from the promisor remote. git reads the variable from
///   2.45.0; an older git in a partial clone takes no hunks at all
///   ([`fetches_lazily`]).
/// - **a deadline**, [`DEADLINE`]: past it the call's process group is asked
///   to end (`SIGTERM`, on which git removes its lock files), then killed,
///   and the call answers [`UNANSWERED`] — the turn and the permit go with
///   it.
/// - `input` on stdin, and both outputs, through owned temporary files —
///   never a pipe a slow reader could block this thread on.
///
/// A read waits for the shared reader permit before it starts anything, as
/// every read of `git::run` does.
fn call(
    root: &Path,
    args: &[&str],
    input: &[u8],
    read: bool,
) -> Result<std::process::Output, GitError> {
    use std::io::Read as _;
    let _permit = read.then(super::read_budget::Permit::acquire);
    let failed = |err: std::io::Error| GitError::Failed(err.to_string());
    let mut stdin = tempfile::tempfile().map_err(failed)?;
    stdin.write_all(input).map_err(failed)?;
    stdin.rewind().map_err(failed)?;
    let mut out = tempfile::tempfile().map_err(failed)?;
    let mut err = tempfile::tempfile().map_err(failed)?;
    let mut command = git_command();
    command
        .args(["-c", "core.quotePath=false", "-c", "core.fsmonitor="])
        .args(args)
        .env("GIT_NO_LAZY_FETCH", "1")
        .current_dir(root)
        .stdin(stdin)
        .stdout(out.try_clone().map_err(failed)?)
        .stderr(err.try_clone().map_err(failed)?);
    let mut group = guild_session::process::ProcessGroup::spawn(&mut command).map_err(|err| {
        if err.kind() == std::io::ErrorKind::NotFound {
            GitError::NoGit
        } else {
            GitError::Failed(err.to_string())
        }
    })?;
    let until = Instant::now() + deadline(root);
    let status = loop {
        if let Some(status) = group.try_wait().map_err(failed)? {
            break status;
        }
        if Instant::now() >= until {
            if let Err(err) = group.terminate(Duration::from_secs(1)) {
                tracing::warn!(%err, "a hunk's git call could not be ended");
            }
            return Err(GitError::Failed(UNANSWERED.into()));
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    out.rewind().map_err(failed)?;
    out.read_to_end(&mut stdout).map_err(failed)?;
    err.rewind().map_err(failed)?;
    err.read_to_end(&mut stderr).map_err(failed)?;
    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

/// A binary patch from `old` to `new`, both ids full: git refuses it unless
/// the entry it reads under its lock hashes to `old`.
pub fn patch(path: &str, old: &str, new: &str, mode: &str, before: &[u8], after: &[u8]) -> Vec<u8> {
    let mut out = b"diff --git ".to_vec();
    out.extend(quote(&format!("a/{path}")));
    out.push(b' ');
    out.extend(quote(&format!("b/{path}")));
    out.extend(format!("\nindex {old}..{new} {mode}\nGIT binary patch\n").into_bytes());
    out.extend(literal(after));
    out.extend(literal(before));
    out
}

/// A path in git's C quoting, always quoted so that a space or a quote in it
/// cannot be read as the boundary between the two names. Bytes git does not
/// escape — UTF-8 among them — go through as they are.
fn quote(path: &str) -> Vec<u8> {
    let mut out = vec![b'"'];
    for byte in path.bytes() {
        match byte {
            b'"' => out.extend(b"\\\""),
            b'\\' => out.extend(b"\\\\"),
            b'\t' => out.extend(b"\\t"),
            b'\n' => out.extend(b"\\n"),
            0x00..=0x1f | 0x7f => out.extend(format!("\\{byte:03o}").into_bytes()),
            _ => out.push(byte),
        }
    }
    out.push(b'"');
    out
}

/// One `literal` block: the content zlib-compressed, in git's base85 lines
/// of up to 52 bytes, then a blank line.
fn literal(content: &[u8]) -> Vec<u8> {
    let mut deflated = Vec::new();
    {
        let mut encoder =
            flate2::write::ZlibEncoder::new(&mut deflated, flate2::Compression::best());
        let _ = encoder.write_all(content);
        let _ = encoder.finish();
    }
    let mut out = format!("literal {}\n", content.len()).into_bytes();
    for chunk in deflated.chunks(52) {
        let n = chunk.len();
        out.push(if n <= 26 {
            b'A' + (n as u8 - 1)
        } else {
            b'a' + (n as u8 - 27)
        });
        out.extend(base85(chunk));
        out.push(b'\n');
    }
    out.push(b'\n');
    out
}

/// git's base85 (`base85.c`): big-endian groups of four, zero-padded.
fn base85(data: &[u8]) -> Vec<u8> {
    const ALPHABET: &[u8; 85] =
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
    let mut out = Vec::with_capacity(data.len().div_ceil(4) * 5);
    for group in data.chunks(4) {
        let mut word = [0u8; 4];
        word[..group.len()].copy_from_slice(group);
        let mut acc = u32::from_be_bytes(word);
        let mut encoded = [0u8; 5];
        for slot in encoded.iter_mut().rev() {
            *slot = ALPHABET[(acc % 85) as usize];
            acc /= 85;
        }
        out.extend_from_slice(&encoded);
    }
    out
}

/// Whether a file takes hunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eligibility {
    Yes,
    /// New, deleted, renamed, conflicted, a symlink, binary or not UTF-8:
    /// its changes go into the index whole.
    WholeFile,
    /// git converts it when it stores it.
    Converted,
}

/// What git says about a path, for its hunks: the index entry's text,
/// `HEAD`'s bytes, whether the file takes hunks at all, and whether git
/// converts line endings there — so that each offer's own bytes are checked
/// (review r3 `bugs+impl-5`). Read once per baseline by the editor and per
/// fetch by a diff, never per keystroke.
#[derive(Debug, Clone)]
pub struct Facts {
    pub index: Option<Arc<str>>,
    pub head: Option<Arc<[u8]>>,
    pub eligibility: Eligibility,
    /// Line-ending conversion may run: an offer whose written bytes hold a
    /// carriage return is disabled as converted.
    pub endings: bool,
}

impl Facts {
    /// Whether git would store an offer's written bytes as they are.
    pub fn keeps(&self, postimage: &[u8]) -> bool {
        !self.endings || !postimage.contains(&b'\r')
    }
}

/// The facts of one path, relative to `root`, for content within `limit`.
/// Sizes are asked before anything is read; nothing runs a program of the
/// person's.
pub fn facts(root: &Path, path: &Path, limit: usize, lines: usize) -> Facts {
    let whole = Facts {
        index: None,
        head: None,
        eligibility: Eligibility::WholeFile,
        endings: false,
    };
    let Some(name) = path.to_str() else {
        return whole;
    };
    read_facts(root, path, name, limit, lines)
        .ok()
        .flatten()
        .unwrap_or(whole)
}

fn read_facts(
    root: &Path,
    path: &Path,
    name: &str,
    limit: usize,
    lines: usize,
) -> Result<Option<Facts>, GitError> {
    if fetches_lazily(root)? {
        return Ok(None);
    }
    // A modified, tracked, regular file: one stage-0 regular entry, a
    // regular file in HEAD, and a regular working file — never a link.
    let (Some(index), Some(head)) = (entry(root, name)?, head_entry(root, name)?) else {
        return Ok(None);
    };
    for oid in [&index.oid, &head.oid] {
        if object_size(root, oid)?.is_none_or(|size| size > limit) {
            return Ok(None);
        }
    }
    let Some(working) = read_regular(root, path, limit).filter(|w| text(w)) else {
        return Ok(None);
    };
    // `cat-file blob` is the raw bytes, never a textconv program's.
    let Some(text) = listed(root, &["cat-file", "blob", &index.oid])?
        .filter(|bytes| !bytes.contains(&0))
        .and_then(|bytes| String::from_utf8(bytes).ok())
    else {
        return Ok(None);
    };
    // The caller's line budget holds for the index text as for its own.
    if text.bytes().filter(|b| *b == b'\n').count() >= lines {
        return Ok(None);
    }
    let Some(head) = listed(root, &["cat-file", "blob", &head.oid])? else {
        return Ok(None);
    };
    let conversion = conversion(root, name)?;
    let eligibility = if conversion.keeps(&[&working, text.as_bytes()]) {
        Eligibility::Yes
    } else {
        Eligibility::Converted
    };
    Ok(Some(Facts {
        index: Some(Arc::from(text)),
        head: Some(Arc::from(head)),
        eligibility,
        endings: conversion == Conversion::Endings,
    }))
}

#[cfg(test)]
mod tests {
    //! The deadline, which only this module can shorten.
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .status()
            .unwrap()
            .success();
        assert!(ok, "git {args:?}");
    }

    /// Review r3 `adversarial-6`: a FIFO where `.gitattributes` should be
    /// blocks `check-attr` and `apply` on its open. Each ends in a refusal
    /// that says so, within the deadline, instead of a wait that holds the
    /// checkout's turn.
    #[test]
    fn an_attributes_fifo_ends_in_a_refusal_not_a_wait() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        std::fs::write(root.join("f"), "a\nb\n").unwrap();
        git(root, &["add", "f"]);
        git(
            root,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-qm",
                "x",
            ],
        );
        std::fs::write(root.join("f"), "a\nB\n").unwrap();
        let fifo = std::ffi::CString::new(root.join(".gitattributes").to_str().unwrap()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        let hunk = Hunk {
            path: "f".into(),
            preimage: Arc::from(&b"a\nb\n"[..]),
            postimage: Arc::from(&b"a\nB\n"[..]),
            disk: None,
            head: Arc::from(&b"a\nb\n"[..]),
        };
        DEADLINE_FOR_TEST.set(Some(Duration::from_millis(300)));
        let started = Instant::now();
        let settled = settle(root, &hunk);
        assert!(
            matches!(&settled, Err(Refusal::Failed(err)) if unanswered(err)),
            "{settled:?}"
        );
        let written = write(root, &hunk);
        assert!(written.as_ref().is_err_and(unanswered), "{written:?}");
        assert!(started.elapsed() < Duration::from_secs(5));
        DEADLINE_FOR_TEST.set(None);
        assert!(
            !root.join(".git/index.lock").exists(),
            "an ended write leaves no lock"
        );
    }

    /// Review r4 `bugs+impl-3`: `apply` swapped the entry in and the read-back
    /// cannot answer. The write says so as a write that may have happened —
    /// never as a refusal before any change — and the index did change.
    #[test]
    fn an_applied_write_that_cannot_be_read_back_is_unverified() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        std::fs::write(root.join("f"), "a\nb\n").unwrap();
        git(root, &["add", "f"]);
        git(
            root,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-qm",
                "x",
            ],
        );
        std::fs::write(root.join("f"), "a\nB\n").unwrap();
        let hunk = Hunk {
            path: "f".into(),
            preimage: Arc::from(&b"a\nb\n"[..]),
            postimage: Arc::from(&b"a\nB\n"[..]),
            disk: None,
            head: Arc::from(&b"a\nb\n"[..]),
        };
        AFTER_APPLY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|| {
                DEADLINE_FOR_TEST.set(Some(Duration::ZERO));
            }))
        });
        let written = write(root, &hunk);
        AFTER_APPLY.with(|hook| *hook.borrow_mut() = None);
        DEADLINE_FOR_TEST.set(None);
        assert!(written.as_ref().is_err_and(unverified), "{written:?}");
        let index = std::process::Command::new("git")
            .args(["show", ":f"])
            .current_dir(root)
            .output()
            .unwrap()
            .stdout;
        assert_eq!(index, b"a\nB\n", "the index did change");
    }

    /// Review r4 `bugs+impl-5`: the variable is git's from 2.45.0.
    #[test]
    fn no_lazy_fetch_is_understood_from_two_forty_five() {
        assert!(understands_no_lazy_fetch(
            "git version 2.50.1 (Apple Git-155)\n"
        ));
        assert!(understands_no_lazy_fetch("git version 2.45.0"));
        assert!(!understands_no_lazy_fetch("git version 2.44.2"));
        assert!(!understands_no_lazy_fetch(
            "git version 2.39.5 (Apple Git-154)"
        ));
        assert!(!understands_no_lazy_fetch("not git"));
    }

    /// Review r5 `docs+tests-8`: `apply` ran past the deadline — blocked on
    /// a FIFO where `.gitattributes` should be — and the read-back fails too.
    /// Whether it was written is not known, and the write says exactly that,
    /// not that git wrote it.
    #[test]
    fn a_timed_out_write_that_cannot_be_read_back_is_unconfirmed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        git(root, &["init", "-q"]);
        std::fs::write(root.join("f"), "a\nb\n").unwrap();
        git(root, &["add", "f"]);
        git(
            root,
            &[
                "-c",
                "user.name=T",
                "-c",
                "user.email=t@e.invalid",
                "commit",
                "-qm",
                "x",
            ],
        );
        let fifo = std::ffi::CString::new(root.join(".gitattributes").to_str().unwrap()).unwrap();
        // SAFETY: a valid NUL-terminated path.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o644) }, 0);
        let hunk = Hunk {
            path: "f".into(),
            preimage: Arc::from(&b"a\nb\n"[..]),
            postimage: Arc::from(&b"a\nB\n"[..]),
            disk: None,
            head: Arc::from(&b"a\nb\n"[..]),
        };
        DEADLINE_FOR_TEST.set(Some(Duration::from_millis(500)));
        AFTER_APPLY.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(|| {
                DEADLINE_FOR_TEST.set(Some(Duration::ZERO));
            }))
        });
        let written = write(root, &hunk);
        AFTER_APPLY.with(|hook| *hook.borrow_mut() = None);
        DEADLINE_FOR_TEST.set(None);
        assert!(written.as_ref().is_err_and(unconfirmed), "{written:?}");
    }
}
