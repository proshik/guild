//! The disk, watched rather than polled.
//!
//! A poll over a tree is a scan, and a file the agent wrote a moment ago
//! has to appear when it lands. `notify` watches the project root
//! recursively — FSEvents underneath — on a thread of its own, and hands
//! the window batches: events collected for a hundred milliseconds,
//! coalesced to the directories they touch, with everything under an
//! ignored directory dropped before it reaches the window.
//!
//! The batch is pure and tested on its own; the thread is what owns the
//! watcher, and the watcher goes with the project, like the pty.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use notify::event::{CreateKind, RemoveKind};
use notify::{EventKind, RecursiveMode, Watcher as _};

/// How long events are collected before the window hears of them. A
/// `cargo build` writes hundreds of files a second; the window wants to
/// hear "target changed" ten times a second at most, and not at all when
/// `target` is ignored.
pub const LATENCY: Duration = Duration::from_millis(100);

/// What changed on disk since the last batch, coalesced.
#[derive(Debug, Clone)]
pub struct Batch {
    ignore: Gitignore,
    directories: Vec<PathBuf>,
    files: Vec<PathBuf>,
    /// Something under `.git` that changes what `git status` says moved:
    /// `HEAD`, the index, a ref. Not named as a file — the window has no
    /// tab for it — but said, so that the marks are asked for again.
    git: bool,
    dependencies: bool,
}

impl Batch {
    /// An empty batch, dropping whatever the rules ignore.
    pub fn new(ignore: &Gitignore) -> Self {
        Self {
            ignore: ignore.clone(),
            directories: Vec::new(),
            files: Vec::new(),
            git: false,
            dependencies: false,
        }
    }

    /// Whether a path under `.git` is one of the few that change what
    /// `git status` says: `HEAD` and the index — a commit, a `git add` —
    /// the refs, and the two heads a merge leaves. Everything else under
    /// `.git` — objects, logs, the index's lock — is noise. `None` for a
    /// path not under `.git` at all.
    fn git_change(path: &Path) -> Option<bool> {
        let mut parts = path.components();
        if parts.next()?.as_os_str() != ".git" {
            return None;
        }
        let inside: PathBuf = parts.collect();
        let name = inside.as_os_str().to_string_lossy();
        Some(
            matches!(
                name.as_ref(),
                "HEAD" | "index" | "ORIG_HEAD" | "MERGE_HEAD" | "packed-refs"
            ) || inside.starts_with("refs"),
        )
    }

    fn note_git(&mut self, path: &Path) -> bool {
        match Self::git_change(path) {
            Some(matters) => {
                self.git |= matters;
                true
            }
            None => false,
        }
    }

    /// Whether something under `.git` moved that changes the marks.
    pub fn git(&self) -> bool {
        self.git
    }

    /// A file changed, appeared or went. Its directory is what the tree
    /// re-reads; the file itself is what an open tab compares itself to.
    pub fn push(&mut self, path: &Path) {
        if self.note_git(path) || self.under_ignored(path) {
            return;
        }
        if let Some(parent) = path.parent() {
            self.note_directory(parent);
        }
        if !self.files.iter().any(|known| known == path) {
            self.files.push(path.to_path_buf());
        }
    }

    /// A directory appeared or went: an event on its parent, whose
    /// listing is what changed. A directory is nothing a tab shows.
    pub fn push_directory(&mut self, path: &Path) {
        if self.note_git(path) || self.under_ignored(path) {
            return;
        }
        if let Some(parent) = path.parent() {
            self.note_directory(parent);
        }
    }

    fn note_directory(&mut self, dir: &Path) {
        if !self.directories.iter().any(|known| known == dir) {
            self.directories.push(dir.to_path_buf());
        }
    }

    /// Under an ignored directory — not ignored itself. `target`
    /// appearing is a change to the root's listing, where the tree shows
    /// it dimmed; what happens inside it afterwards is nobody's news.
    /// And an ignored *file* open in a tab still has to reload.
    fn under_ignored(&self, path: &Path) -> bool {
        path.parent().is_some_and(|parent| {
            !parent.as_os_str().is_empty()
                && self
                    .ignore
                    .matched_path_or_any_parents(parent, true)
                    .is_ignore()
        })
    }

    /// The directories touched, in order of first appearance, relative
    /// to the root. The root itself is the empty path.
    pub fn directories(&self) -> &[PathBuf] {
        &self.directories
    }

    /// The files touched, in the same order and the same terms.
    pub fn files(&self) -> &[PathBuf] {
        &self.files
    }

    /// Nothing the window needs to hear.
    pub fn is_empty(&self) -> bool {
        self.directories.is_empty() && self.files.is_empty() && !self.git && !self.dependencies
    }

    /// Whether this batch invalidates dependency discovery, including exact
    /// Go inputs outside the project or below an ignored vendor directory.
    pub fn dependencies(&self) -> bool {
        self.dependencies
            || self.files.iter().any(|file| {
                crate::deps::Stack::ALL
                    .iter()
                    .any(|stack| stack.watched().iter().any(|name| file.ends_with(name)))
            })
    }
}

/// The rules the watcher drops events by: the root's own `.gitignore`,
/// `.git`, and `.guild` — what Guild writes there is never news.
fn rules(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let file = root.join(".gitignore");
    if file.is_file() {
        builder.add(&file);
    }
    for own in [".git/", ".guild/"] {
        let _ = builder.add_line(None, own);
    }
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// A small snapshot of build metadata, read on background threads only.
/// Content comparisons also catch atomic replacements and equal-size writes;
/// no recursive walk of vendor or dependency caches is needed.
#[derive(Debug)]
pub struct DependencyInputs {
    files: Vec<(PathBuf, Result<Vec<u8>, std::io::ErrorKind>)>,
}

impl DependencyInputs {
    pub fn read(paths: Vec<PathBuf>) -> Self {
        Self {
            files: paths
                .into_iter()
                .map(|path| {
                    let bytes = std::fs::read(&path).map_err(|err| err.kind());
                    (path, bytes)
                })
                .collect(),
        }
    }

    fn changed(&mut self) -> bool {
        let mut changed = false;
        for (path, before) in &mut self.files {
            let now = std::fs::read(path).map_err(|err| err.kind());
            changed |= *before != now;
            *before = now;
        }
        changed
    }
}

/// A project's watcher: the platform's, on a thread that batches what it
/// says. Dropping this stops both.
///
/// The platform's watcher is made **on the thread**, not here: on macOS
/// starting an FSEvents stream took two and a half seconds, and this is
/// called from the window's own loop. What the window gets back at once
/// is the channel; `is_ready` says when the disk is actually being
/// watched, and a write before that is not seen.
pub struct Watcher {
    batches: mpsc::Receiver<Batch>,
    /// Dropped with the watcher; the thread notices and ends, taking
    /// the platform's watcher with it.
    _stop: mpsc::Sender<()>,
    ready: Arc<AtomicBool>,
    dependency_inputs: mpsc::Sender<DependencyInputs>,
}

impl Watcher {
    /// Watch a project root, recursively.
    pub fn start(root: &Path) -> Self {
        Self::start_asking_git(root, true)
    }

    /// Watch a project group's folder, recursively, without asking git
    /// where its directories are — GLD-055: nothing of git runs for a group
    /// root but its branch label, and its tree has no git marks to keep
    /// fresh.
    pub fn start_without_git(root: &Path) -> Self {
        Self::start_asking_git(root, false)
    }

    fn start_asking_git(root: &Path, ask_git: bool) -> Self {
        let (batches_tx, batches_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let ready = Arc::new(AtomicBool::new(false));
        let armed = ready.clone();
        let (inputs_tx, inputs_rx) = mpsc::channel();
        let root = root.to_path_buf();
        let spawned = std::thread::Builder::new()
            .name("guild-watch".into())
            .spawn(move || run(&root, ask_git, &armed, &stop_rx, &batches_tx, &inputs_rx));
        if let Err(err) = spawned {
            tracing::warn!(%err, "the watcher's thread could not be started");
        }
        Self {
            batches: batches_rx,
            _stop: stop_tx,
            ready,
            dependency_inputs: inputs_tx,
        }
    }

    /// Install a snapshot taken before discovery, so a change during that
    /// discovery still causes a second asking through the ordinary batch route.
    pub fn track_dependencies(&self, inputs: DependencyInputs) {
        let _ = self.dependency_inputs.send(inputs);
    }

    /// Whether the disk is being watched yet.
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// The next batch, if one is waiting. Never blocks: the window asks
    /// on its own clock.
    pub fn try_recv(&self) -> Option<Batch> {
        self.batches.try_recv().ok()
    }
}

/// The thread: start the platform's watcher, then wait for an event,
/// gather what follows within the latency, and send the batch if there
/// is anything in it. Ends when the `Watcher` is dropped — the stop
/// channel closes — or when nobody is left to receive.
///
/// The root is canonicalised first: FSEvents reports real paths, and a
/// root reached through a symlink — every `/var` on macOS — would match
/// none of them.
fn run(
    root: &Path,
    ask_git: bool,
    ready: &AtomicBool,
    stop: &mpsc::Receiver<()>,
    batches: &mpsc::Sender<Batch>,
    inputs: &mpsc::Receiver<DependencyInputs>,
) {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let (events_tx, events_rx) = mpsc::channel::<notify::Result<notify::Event>>();
    let mut platform = match notify::recommended_watcher(move |event| {
        let _ = events_tx.send(event);
    }) {
        Ok(platform) => platform,
        Err(err) => {
            tracing::warn!(project = %root.display(), %err, "the project is not watched");
            return;
        }
    };
    if let Err(err) = platform.watch(&root, RecursiveMode::Recursive) {
        tracing::warn!(project = %root.display(), %err, "the project is not watched");
        return;
    }
    // A linked worktree's `.git` is a file, and what a commit or a push
    // moves is outside the root: watched too, or a shell's commit is
    // news to nobody. A failure costs the marks their freshness, not the
    // watcher.
    let outside = ask_git.then(|| GitDirs::outside(&root)).flatten();
    if let Some(dirs) = &outside {
        for (path, mode) in dirs.watched() {
            if let Err(err) = platform.watch(&path, mode) {
                tracing::warn!(path = %path.display(), %err, "git's directory is not watched");
            }
        }
    }
    ready.store(true, Ordering::Release);
    let ignore = rules(&root);
    let mut dependencies: Option<DependencyInputs> = None;
    let mut next_check = Instant::now();

    loop {
        if stop.try_recv() == Err(mpsc::TryRecvError::Disconnected) {
            return;
        }
        let mut changed = false;
        while let Ok(mut new) = inputs.try_recv() {
            changed |= new.changed();
            dependencies = Some(new);
        }
        if Instant::now() >= next_check {
            changed |= dependencies.as_mut().is_some_and(DependencyInputs::changed);
            next_check = Instant::now() + Duration::from_secs(1);
        }
        if changed {
            let mut batch = Batch::new(&ignore);
            batch.dependencies = true;
            if batches.send(batch).is_err() {
                return;
            }
        }
        let first = match events_rx.recv_timeout(LATENCY) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stop.try_recv() == Err(mpsc::TryRecvError::Disconnected) {
                    return;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        let mut batch = Batch::new(&ignore);
        absorb(&mut batch, first, &root, outside.as_ref());
        let deadline = Instant::now() + LATENCY;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match events_rx.recv_timeout(left) {
                Ok(event) => absorb(&mut batch, event, &root, outside.as_ref()),
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => return,
            }
        }
        if !batch.is_empty() && batches.send(batch).is_err() {
            return;
        }
    }
}

/// One event into the batch, its paths made relative to the root. The
/// platform says when a directory appeared or went; for the rest, the
/// disk is asked, and a path no longer there is taken for a file — its
/// parent is re-read either way, and a tab that showed it learns it is
/// gone.
fn absorb(
    batch: &mut Batch,
    event: notify::Result<notify::Event>,
    root: &Path,
    outside: Option<&GitDirs>,
) {
    let event = match event {
        Ok(event) => event,
        Err(err) => {
            tracing::warn!(%err, "the watcher reported an error");
            return;
        }
    };
    for path in &event.paths {
        let Ok(relative) = path.strip_prefix(root) else {
            // Outside the root is git's, and only what `.git` would say.
            if let Some(inside) = outside.and_then(|dirs| dirs.as_dot_git(path)) {
                batch.push(&inside);
            }
            continue;
        };
        let is_dir = match event.kind {
            EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => true,
            _ => path.is_dir(),
        };
        if is_dir {
            batch.push_directory(relative);
        } else {
            batch.push(relative);
        }
    }
}

/// Where git keeps a root's repository when that is not `<root>/.git` — a
/// linked worktree, whose `.git` is a file naming
/// `<common>/worktrees/<name>`, or a root opened below its repository's.
#[derive(Debug, Clone)]
struct GitDirs {
    /// The worktree's own: its `HEAD`, its index, the heads a merge leaves.
    git_dir: PathBuf,
    /// Every worktree's: the refs and `packed-refs`.
    common: PathBuf,
}

impl GitDirs {
    /// Asked of git, on the watcher's thread; `None` for a root whose git
    /// directory is its own `.git` — the root's watch already hears it —
    /// and for a root git knows nothing of.
    fn outside(root: &Path) -> Option<Self> {
        let out = crate::git::run(
            root,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--git-dir",
                "--git-common-dir",
            ],
        )
        .ok()?;
        let text = String::from_utf8(out).ok()?;
        let mut lines = text.lines();
        // Canonical, because FSEvents reports real paths.
        let real = |line: &str| {
            let path = PathBuf::from(line);
            path.canonicalize().unwrap_or(path)
        };
        let git_dir = real(lines.next()?);
        let common = real(lines.next()?);
        if git_dir.starts_with(root) {
            return None;
        }
        Some(Self { git_dir, common })
    }

    /// What to watch: the worktree's own directory and the common one
    /// **without** what is below them — the common dir holds every
    /// worktree's index and all the objects, and a `git add` in the next
    /// worktree is not this one's news — and the refs with everything
    /// below.
    fn watched(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut paths = vec![
            (self.git_dir.clone(), RecursiveMode::NonRecursive),
            (self.common.join("refs"), RecursiveMode::Recursive),
        ];
        if self.common != self.git_dir {
            paths.push((self.common.clone(), RecursiveMode::NonRecursive));
        }
        paths
    }

    /// A path git wrote, as the same file under a `.git` of the root's —
    /// the terms `Batch` already judges — or `None` for what is not this
    /// worktree's to hear.
    fn as_dot_git(&self, path: &Path) -> Option<PathBuf> {
        let dot_git = Path::new(".git");
        if let Ok(rest) = path.strip_prefix(&self.git_dir) {
            return Some(dot_git.join(rest));
        }
        if let Ok(rest) = path.strip_prefix(self.common.join("refs")) {
            return Some(dot_git.join("refs").join(rest));
        }
        (path == self.common.join("packed-refs")).then(|| dot_git.join("packed-refs"))
    }
}
