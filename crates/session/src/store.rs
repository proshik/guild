//! Keeping state between runs.
//!
//! JSON next to the user's configuration rather than a database: the
//! desktop may have no database at all — that is the whole point of its
//! autonomy.
//!
//! The store is generic over its type on purpose. There are two files —
//! the session registry and the settings — but the rule "a corrupted file
//! must not stop the application from opening" is one, and it has to live
//! in one place: two copies of that rule would drift apart sooner or
//! later, and drift silently.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::Write as _;
use std::marker::PhantomData;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, OnceLock};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// A state file.
pub struct Store<T> {
    path: PathBuf,
    _type: PhantomData<T>,
}

/// What went wrong while saving.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    /// Could not write the file or create the directory.
    #[error("could not save state to {path}: {source}")]
    Write {
        /// Where we were writing.
        path: PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },

    /// Could not build the JSON.
    #[error("could not serialise state: {0}")]
    Encode(#[source] serde_json::Error),

    /// A file that is there could not be read — too many open files, a
    /// disk error, a permission. Not a broken file: nothing was moved, and
    /// its path is not saved to for the rest of the run ([`Self::NotRead`]).
    #[error("could not read state from {path}: {source}")]
    Read {
        /// The file that could not be read: the state file or its backup.
        path: PathBuf,
        /// Why it failed.
        #[source]
        source: std::io::Error,
    },

    /// A save refused because this run could not read the file it would
    /// replace — 2026-11-07. Whatever is being saved was built without
    /// it, so it would write a default or an older copy over the person's
    /// state. The file stays as it is until Guild starts again.
    #[error("{path} was not saved over: it could not be read ({reason})")]
    NotRead {
        /// The state file that was not written.
        path: PathBuf,
        /// What reading it answered.
        reason: String,
    },
}

/// A state file that was there and could not be read at all, so it was
/// left where it is and is not saved over this run. Not [`Unreadable`]:
/// nothing is wrong with its bytes as far as anyone knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFailure {
    /// The file that could not be read: a state file or its backup.
    pub path: PathBuf,
    /// What the system answered, e.g. *Too many open files (os error 24)*.
    pub reason: String,
}

/// A state file that existed and could not be read, and where it went.
///
/// Decision 21 of M5.11: `load` used to answer a default for it, and the
/// next two saves turned the file into `.bak` and then into nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unreadable {
    /// The file that could not be read.
    pub path: PathBuf,
    /// Where its bytes are now, beside it.
    pub kept: PathBuf,
    /// Whether the previous copy was read in its place; otherwise the
    /// state started without it.
    pub from_backup: bool,
}

/// State that can put itself in order after loading.
///
/// Needed for migrating old files: the format changes, and what was saved
/// must not be lost. For state with nothing to migrate the implementation
/// is empty.
pub trait Normalise {
    /// Put what was read in order.
    fn normalise(&mut self) {}
}

/// The name the previous good copy keeps.
const BACKUP: &str = ".bak";

/// What an unreadable file's name gains, before the second it was kept.
const UNREADABLE: &str = ".unreadable-";

/// A sibling of `path` with a suffix on the end.
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

impl<T: Default + Serialize + DeserializeOwned + Normalise> Store<T> {
    /// A state file at the given path.
    ///
    /// The path comes from outside rather than being computed here: that
    /// way it can be swapped in tests, and the decision "where do user
    /// settings belong" stays with the application.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            _type: PhantomData,
        }
    }

    /// Read the state.
    ///
    /// **A corrupted file is not an error.** An application that will not
    /// open because of a corrupted settings file is the worst kind of
    /// breakage: there is nothing to fix it with, because there is no
    /// interface. **A file that could not be read at all is**
    /// ([`StoreError::Read`]): too many open files or a failing disk says
    /// nothing about the bytes, so it is left where it is, said once for
    /// [`take_read_failures`], and saves to the path are refused until
    /// Guild starts again. A caller that opens with a default instead
    /// cannot then write it over the person's state.
    ///
    /// Starting clean is the last answer rather than the first. A file
    /// that cannot be read is more often a crash caught mid-write than a
    /// person's edit, and [`Self::save`] leaves the previous copy beside
    /// it for exactly this moment.
    ///
    /// **And a file that cannot be read is kept, not replaced.** It is
    /// moved aside before anything can write over it, and the move is
    /// reported once per path per run, for [`take_unreadable`] — this is
    /// [`Self::load_reporting`] with the report queued.
    pub fn load(&self) -> Result<T, StoreError> {
        let (state, report) = self.load_reporting()?;
        if let Some(report) = report {
            let mut queue = writer().queue.lock().unwrap_or_else(|e| e.into_inner());
            let Queue {
                reported,
                unreadable,
                ..
            } = &mut *queue;
            report_once(reported, unreadable, report);
        }
        Ok(state)
    }

    /// Read the state, and say whether a file that was there could not be
    /// read — and so was moved aside. Decision 21 of M5.11.
    ///
    /// The main file first; a file that exists and will not read or parse
    /// is renamed to `<name>.unreadable-<seconds>` beside it, **or removed
    /// when its bytes are those of the newest copy already kept there**,
    /// because the realistic cause — a newer build beside an older one —
    /// brings the same file back on every save of the newer. Then the
    /// previous copy, treated the same way. A file that is not there is
    /// the ordinary first run and is reported as nothing.
    ///
    /// **A file that could not be read at all is neither**: either copy
    /// failing to open or read answers [`StoreError::Read`] before
    /// anything is moved, and the path is not saved to this run — see
    /// [`Self::load`].
    pub fn load_reporting(&self) -> Result<(T, Option<Unreadable>), StoreError> {
        // What is queued for this path but not yet on the disk is the
        // newest state there is: a save is seen by the next load without
        // waiting for the write.
        if let Some(text) = writer().pending(&self.path)
            && let Some(state) = Self::parse(&self.path, &text)
        {
            return Ok((state, None));
        }
        let backup = sibling(&self.path, BACKUP);
        let main = match Self::read(&self.path) {
            Read::State(state) => return Ok((state, None)),
            Read::Failed(source) => return Err(self.not_read(&self.path, source)),
            other => other,
        };
        // The backup is read before the main file is moved: if it cannot
        // be read, whether it is good is unknown, and both stay.
        let previous = match Self::read(&backup) {
            Read::Failed(source) => return Err(self.not_read(&backup, source)),
            other => other,
        };
        let kept = match main {
            Read::Unreadable(bytes) => Some(keep_aside(&self.path, &bytes)),
            _ => None,
        };
        Ok(match (previous, kept) {
            (Read::State(state), kept) => {
                tracing::warn!(
                    path = %self.path.display(),
                    "the state file could not be read; the previous copy was used"
                );
                let report = kept.map(|kept| Unreadable {
                    path: self.path.clone(),
                    kept,
                    from_backup: true,
                });
                (state, report)
            }
            (Read::Missing, kept) => {
                let report = kept.map(|kept| Unreadable {
                    path: self.path.clone(),
                    kept,
                    from_backup: false,
                });
                (T::default(), report)
            }
            (Read::Unreadable(bytes), kept) => {
                let backup_kept = keep_aside(&backup, &bytes);
                // The main file's move is the news; the backup's is named
                // only when the main file was simply not there.
                let report = match kept {
                    Some(kept) => Some(Unreadable {
                        path: self.path.clone(),
                        kept,
                        from_backup: false,
                    }),
                    None => Some(Unreadable {
                        path: backup.clone(),
                        kept: backup_kept,
                        from_backup: false,
                    }),
                };
                (T::default(), report)
            }
            // Answered before anything was moved.
            (Read::Failed(_), _) => unreachable!("a backup that failed returned above"),
        })
    }

    /// `failed` could not be read: this store's path is not saved to for
    /// the rest of the run, and the failure is said once.
    fn not_read(&self, failed: &Path, source: std::io::Error) -> StoreError {
        tracing::warn!(
            path = %failed.display(),
            err = %source,
            "a state file could not be read; it is left as it is and not saved over this run"
        );
        let reason = source.to_string();
        let mut queue = writer().queue.lock().unwrap_or_else(|e| e.into_inner());
        if !queue.unread.contains_key(&self.path) {
            queue.unread.insert(self.path.clone(), reason.clone());
            queue.read_failures.push(ReadFailure {
                path: failed.to_path_buf(),
                reason,
            });
        }
        StoreError::Read {
            path: failed.to_path_buf(),
            source,
        }
    }

    /// Read one file: its state, nothing there, bytes that are not a
    /// state, or no bytes at all — told apart, because the third is kept,
    /// the second is the first run, and the last says nothing about the
    /// file.
    fn read(path: &Path) -> Read<T> {
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Read::Missing,
            Err(err) => return Read::Failed(err),
        };
        match std::str::from_utf8(&bytes)
            .ok()
            .and_then(|text| Self::parse(path, text))
        {
            Some(state) => Read::State(state),
            None => Read::Unreadable(bytes),
        }
    }

    /// Parse one file's text, or `None` if there is nothing usable in it.
    fn parse(path: &Path, text: &str) -> Option<T> {
        match serde_json::from_str::<T>(text) {
            Ok(mut state) => {
                // What was read is put in order here rather than by the
                // caller: forgetting the call is easy, and the consequence
                // is sessions lost from an old-format file.
                state.normalise();
                Some(state)
            }
            Err(err) => {
                tracing::warn!(path = %path.display(), %err, "the state file could not be parsed");
                None
            }
        }
    }

    /// Write the state so that a crash cannot destroy it.
    ///
    /// Written beside and renamed into place: a rename is atomic within a
    /// filesystem, so a reader sees either the old file or the new one and
    /// never half of either. `sync_all` on the file comes before the
    /// rename — otherwise the rename can reach the disk before the bytes —
    /// and `sync_all` on the directory after it, which is what makes the
    /// rename itself survive a power cut rather than merely a dead
    /// process.
    ///
    /// The previous file is kept as a backup. Between the two renames
    /// there is an instant with no main file at all; that window is safe
    /// because [`Self::load`] falls back to the backup, and only because
    /// of that.
    ///
    /// **Through the one writer thread, and waited for.** A save written
    /// here while the thread wrote the same path renamed its scratch file
    /// from under it — *No such file or directory*, seen while a dock was
    /// dragged (2026-09-16) — and a queued older snapshot could land after
    /// a newer one written here. So the bytes join the queue, replacing
    /// whatever of this path is still waiting, and this returns once they
    /// are on the disk, with the thread's own answer.
    ///
    /// **Refused for a path this run could not read** —
    /// [`StoreError::NotRead`], nothing queued.
    pub fn save(&self, state: &T) -> Result<(), StoreError> {
        let text = serde_json::to_string_pretty(state).map_err(StoreError::Encode)?;
        let writer = writer();
        let (send, receive) = std::sync::mpsc::channel();
        writer.queue(self.path.clone(), text, Some(send))?;
        receive
            .recv()
            .map_err(|_| StoreError::Write {
                path: self.path.clone(),
                source: std::io::Error::other("state writer stopped"),
            })?
            .map_err(|reason| StoreError::Write {
                path: self.path.clone(),
                source: std::io::Error::other(reason),
            })
    }

    /// Write the state without waiting for the disk.
    ///
    /// The bytes are made here and handed to one writer thread, which
    /// puts them on the disk the way [`Self::save`] does, in the order
    /// they came, the latest snapshot of a path winning over an older one
    /// still queued. Until then a [`Self::load`] of the path answers from
    /// the queue. This is what the window calls on every change: a
    /// synced write costs twenty milliseconds on the main thread, and a
    /// tab switch made two of them. A failure on the thread is kept for
    /// [`take_failure`], so the window can still say so. Refused, like
    /// [`Self::save`], for a path this run could not read.
    pub fn save_soon(&self, state: &T) -> Result<(), StoreError> {
        let text = serde_json::to_string_pretty(state).map_err(StoreError::Encode)?;
        writer().queue(self.path.clone(), text, None)
    }
}

/// What one file held.
enum Read<T> {
    State(T),
    Missing,
    /// There, and not a state: its bytes, to compare with a copy kept.
    Unreadable(Vec<u8>),
    /// There, or perhaps there, and not read at all: kept nowhere, since
    /// nothing is known to be wrong with it.
    Failed(std::io::Error),
}

/// Move an unreadable file out of the way of the next save, and answer
/// where its bytes are now.
///
/// **Removed rather than kept a second time** when its bytes equal the
/// newest copy already beside it — that copy is then the answer, and
/// still is when the removal fails, since those bytes are kept either way.
///
/// **A file that cannot be moved is answered where it is**, logged, and
/// still reported: the load must start, and a load that said nothing
/// would leave the file for the next save that can rename anything to push
/// over a good `.bak` with nobody told.
fn keep_aside(path: &Path, bytes: &[u8]) -> PathBuf {
    if let Some(newest) = newest_kept(path)
        && std::fs::read(&newest).is_ok_and(|kept| kept == bytes)
    {
        if let Err(err) = std::fs::remove_file(path) {
            tracing::warn!(path = %path.display(), %err, "an unreadable state file could not be removed");
        }
        return newest;
    }
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let kept = kept_name(path, seconds, |candidate| candidate.exists());
    match std::fs::rename(path, &kept) {
        Ok(()) => {
            tracing::warn!(path = %path.display(), kept = %kept.display(), "an unreadable state file was kept aside");
            kept
        }
        Err(err) => {
            tracing::warn!(path = %path.display(), %err, "an unreadable state file could not be kept aside");
            path.to_path_buf()
        }
    }
}

/// The name an unreadable file is kept under: `<name>.unreadable-<seconds>`,
/// and `-2`, `-3` after it while that is taken.
fn kept_name(path: &Path, seconds: u64, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let plain = sibling(path, &format!("{UNREADABLE}{seconds}"));
    if !taken(&plain) {
        return plain;
    }
    (2u64..)
        .map(|n| sibling(path, &format!("{UNREADABLE}{seconds}-{n}")))
        .find(|candidate| !taken(candidate))
        .expect("some number is free")
}

/// The copy most recently kept beside `path`, by the second and the
/// number in its name — not by its time on the disk, which a rename keeps
/// from the file's last write.
fn newest_kept(path: &Path) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    let prefix = format!("{name}{UNREADABLE}");
    let dir = path.parent().filter(|dir| !dir.as_os_str().is_empty())?;
    std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file = entry.file_name();
            let rest = file.to_str()?.strip_prefix(&prefix)?.to_owned();
            let (seconds, number) = match rest.split_once('-') {
                Some((seconds, number)) => {
                    (seconds.parse::<u64>().ok()?, number.parse::<u64>().ok()?)
                }
                None => (rest.parse::<u64>().ok()?, 1),
            };
            Some(((seconds, number), entry.path()))
        })
        .max_by_key(|(order, _)| *order)
        .map(|(_, path)| path)
}

/// Queue a report unless its path has been reported already this run.
fn report_once(reported: &mut HashSet<PathBuf>, pending: &mut Vec<Unreadable>, report: Unreadable) {
    if reported.insert(report.path.clone()) {
        pending.push(report);
    }
}

/// The atomic write: beside the file, synced, renamed over it, the
/// directory synced after, and the previous copy kept.
///
/// **Bytes the file already holds are not written again** — 2026-09-24.
/// The window saves every open project on any change, and one agent's
/// cost ticking over had eighteen `state.json` renamed, synced and backed
/// up every few seconds with the bytes they held. The disk is what is
/// compared, not what was last written here, so a file somebody else
/// changed is still written over. **Only once a backup is beside it**:
/// the second save of a new file is what puts the good copy in `.bak`,
/// and a crash that later truncates the file falls back to it — so a
/// backup is the previous *different* copy, never the same bytes twice.
fn write_atomically(path: &Path, text: &str) -> Result<(), StoreError> {
    write_atomically_with_directory(path, text, |dir| std::fs::File::open(dir))
}

fn write_error(path: &Path, operation: &str, source: std::io::Error) -> StoreError {
    StoreError::Write {
        path: path.to_path_buf(),
        // The queue keeps only the reason, so it must retain the actual IO
        // path even when that differs from the requested state file.
        source: std::io::Error::new(
            source.kind(),
            format!("{operation} ({}): {source}", path.display()),
        ),
    }
}

fn write_atomically_with_directory(
    path: &Path,
    text: &str,
    open_directory: impl FnOnce(&Path) -> std::io::Result<std::fs::File>,
) -> Result<(), StoreError> {
    if sibling(path, BACKUP).exists()
        && std::fs::read(path).is_ok_and(|held| held == text.as_bytes())
    {
        return Ok(());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)
        .map_err(|source| write_error(parent, "create state directory", source))?;
    // Opening the durability handle can fail independently of file writes.
    // Discover that before changing either the current file or its backup.
    let directory = open_directory(parent)
        .map_err(|source| write_error(parent, "open state directory for sync", source))?;

    // The queue serializes this process only. Other windows and test processes
    // must never truncate or rename our unpublished bytes. Keep a unique owned
    // scratch beside the destination (same filesystem), removed on any failure.
    let mut scratch = tempfile::Builder::new()
        .prefix(".guild-state-")
        // Retain File::create's mode, restricted by the process's umask.
        .permissions(std::fs::Permissions::from_mode(0o666))
        .tempfile_in(parent)
        .map_err(|source| write_error(parent, "create state scratch", source))?;
    scratch
        .write_all(text.as_bytes())
        .map_err(|source| write_error(scratch.path(), "write state scratch", source))?;
    scratch
        .as_file()
        .sync_all()
        .map_err(|source| write_error(scratch.path(), "sync state scratch", source))?;

    // A failure here costs only the backup. Stopping the save because
    // the previous copy could not be moved would trade a real write
    // for a spare one.
    if path.exists() {
        let _ = std::fs::rename(path, sibling(path, BACKUP));
    }

    let publish = format!(
        "rename state scratch {} into place",
        scratch.path().display()
    );
    scratch
        .persist(path)
        .map_err(|error| write_error(path, &publish, error.error))?;

    // The rename is a change to the directory, and it is durable only
    // once the directory itself is flushed.
    directory
        .sync_all()
        .map_err(|source| write_error(parent, "sync state directory", source))?;

    Ok(())
}

/// The writer behind [`Store::save_soon`]: one thread, one queue.
///
/// An entry stays in the queue while it is being written, so that a load
/// meanwhile still answers from it; it leaves once the disk has it — or
/// stays, if a newer snapshot of the same path arrived in between.
struct Writer {
    queue: Mutex<Queue>,
    changed: Condvar,
}

type Completion = std::sync::mpsc::Sender<Result<(), String>>;
struct Pending {
    path: PathBuf,
    text: String,
    completions: Vec<Completion>,
}
#[derive(Default)]
struct Queue {
    pending: VecDeque<Pending>,
    // Path-drain outcomes survive UI consumption. Each synchronous caller has
    // its own completion channel, independent of both of these collections.
    failures: HashMap<PathBuf, String>,
    notices: VecDeque<(PathBuf, String)>,
    /// Files kept aside as unreadable, not yet taken by the window.
    unreadable: Vec<Unreadable>,
    /// Every path reported this run, so that one is said once however
    /// many loads meet it.
    reported: HashSet<PathBuf>,
    /// State files this run could not read, with what reading answered:
    /// never saved to until the process ends.
    unread: HashMap<PathBuf, String>,
    /// Those failures, not yet taken by the window.
    read_failures: Vec<ReadFailure>,
}

static WRITER: OnceLock<Writer> = OnceLock::new();

fn writer() -> &'static Writer {
    WRITER.get_or_init(|| {
        let writer = Writer {
            queue: Mutex::new(Queue::default()),
            changed: Condvar::new(),
        };
        std::thread::Builder::new()
            .name("guild-state".into())
            .spawn(run)
            .expect("the state writer's thread starts");
        writer
    })
}

impl Writer {
    fn queue(
        &self,
        path: PathBuf,
        text: String,
        completion: Option<Completion>,
    ) -> Result<(), StoreError> {
        let mut queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        // Under the same lock a failed read takes, so no save slips in
        // between the failure and the refusal.
        if let Some(reason) = queue.unread.get(&path) {
            tracing::debug!(path = %path.display(), "a save over an unread state file was refused");
            return Err(StoreError::NotRead {
                path,
                reason: reason.clone(),
            });
        }
        if let Some(entry) = queue.pending.iter_mut().find(|p| p.path == path) {
            entry.text = text;
            entry.completions.extend(completion);
        } else {
            queue.pending.push_back(Pending {
                path,
                text,
                completions: completion.into_iter().collect(),
            });
        }
        self.changed.notify_all();
        Ok(())
    }

    fn pending(&self, path: &Path) -> Option<String> {
        let queue = self.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue
            .pending
            .iter()
            .find(|entry| entry.path == path)
            .map(|entry| entry.text.clone())
    }
}

fn run() {
    let writer = writer();
    loop {
        let (path, text, mut completions) = {
            let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
            while queue.pending.is_empty() {
                queue = writer
                    .changed
                    .wait(queue)
                    .unwrap_or_else(|e| e.into_inner());
            }
            let entry = &mut queue.pending[0];
            (
                entry.path.clone(),
                entry.text.clone(),
                std::mem::take(&mut entry.completions),
            )
        };
        let result = write_atomically(&path, &text);
        let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.notices.retain(|(pending, _)| *pending != path);
        let outcome = if let Err(err) = result {
            tracing::warn!(path = %path.display(), %err, "state was not saved");
            // The reason alone: whoever shows it names the path already.
            let reason = match &err {
                StoreError::Write { source, .. } => source.to_string(),
                other => other.to_string(),
            };
            queue.failures.insert(path.clone(), reason.clone());
            queue.notices.push_back((path.clone(), reason.clone()));
            Err(reason)
        } else {
            queue.failures.remove(&path);
            Ok(())
        };
        // Gone only if nothing newer arrived for it meanwhile.
        if queue
            .pending
            .front()
            .is_some_and(|entry| entry.path == path && entry.text == text)
        {
            completions.extend(
                queue
                    .pending
                    .pop_front()
                    .expect("the current write")
                    .completions,
            );
        }
        for completion in completions {
            let _ = completion.send(outcome.clone());
        }
        writer.changed.notify_all();
    }
}

/// Wait until every queued save is on the disk. Called before the
/// application quits: the queue lives in memory, and a quit a moment
/// after a change must not lose the change.
pub fn flush() {
    let writer = writer();
    let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
    while !queue.pending.is_empty() {
        queue = writer
            .changed
            .wait(queue)
            .unwrap_or_else(|e| e.into_inner());
    }
}

/// Drain one path, including a write already in progress, and report its
/// failure. Other projects may keep saving while this path is quiesced.
pub(crate) fn flush_path(path: &Path) -> Result<(), StoreError> {
    let writer = writer();
    let mut queue = writer.queue.lock().unwrap_or_else(|e| e.into_inner());
    while queue.pending.iter().any(|pending| pending.path == path) {
        queue = writer
            .changed
            .wait(queue)
            .unwrap_or_else(|e| e.into_inner());
    }
    match queue.failures.get(path) {
        Some(reason) => Err(StoreError::Write {
            path: path.to_path_buf(),
            source: std::io::Error::other(reason.clone()),
        }),
        None => Ok(()),
    }
}

/// A queued save that failed: at most one pending notice per path. The
/// window asks on its own clock and shows it, so a failing disk is still
/// seen rather than only logged.
pub fn take_failure() -> Option<(PathBuf, String)> {
    let mut queue = writer().queue.lock().unwrap_or_else(|e| e.into_inner());
    queue.notices.pop_front()
}

/// Every state file kept aside as unreadable since the last ask, each
/// path once per run. The window asks on its own clock and names them.
/// Decision 21 of M5.11.
pub fn take_unreadable() -> Vec<Unreadable> {
    let mut queue = writer().queue.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut queue.unreadable)
}

/// Every state file that could not be read at all since the last ask,
/// each once per run: left where it is and not saved over until Guild
/// starts again. The window asks on its own clock and names them.
pub fn take_read_failures() -> Vec<ReadFailure> {
    let mut queue = writer().queue.lock().unwrap_or_else(|e| e.into_inner());
    std::mem::take(&mut queue.read_failures)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two files kept unreadable in the same second take two names, and
    /// the second never replaces the first.
    #[test]
    fn a_name_taken_in_the_same_second_gets_a_number() {
        let path = Path::new("/state/state.json");
        assert_eq!(
            kept_name(path, 1_757_900_000, |_| false),
            PathBuf::from("/state/state.json.unreadable-1757900000")
        );
        let taken = |candidate: &Path| {
            candidate == Path::new("/state/state.json.unreadable-1757900000")
                || candidate == Path::new("/state/state.json.unreadable-1757900000-2")
        };
        assert_eq!(
            kept_name(path, 1_757_900_000, taken),
            PathBuf::from("/state/state.json.unreadable-1757900000-3")
        );
    }

    /// **Said once.** A project's state is loaded on many gestures, and a
    /// file that keeps coming back unreadable would otherwise be a notice
    /// per gesture.
    #[test]
    fn a_path_is_reported_once_per_run() {
        let mut reported = HashSet::new();
        let mut pending = Vec::new();
        let report = |path: &str, kept: &str| Unreadable {
            path: PathBuf::from(path),
            kept: PathBuf::from(kept),
            from_backup: false,
        };
        report_once(
            &mut reported,
            &mut pending,
            report("/a/state.json", "/a/k1"),
        );
        report_once(
            &mut reported,
            &mut pending,
            report("/a/state.json", "/a/k2"),
        );
        report_once(
            &mut reported,
            &mut pending,
            report("/b/state.json", "/b/k1"),
        );
        assert_eq!(pending.len(), 2, "{pending:?}");
        assert_eq!(pending[0].kept, PathBuf::from("/a/k1"));
        assert_eq!(pending[1].path, PathBuf::from("/b/state.json"));
    }
    // Fault injection avoids platform/root-dependent permission tests.
    #[test]
    fn directory_open_failure_preserves_state_and_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let backup = sibling(&path, BACKUP);
        std::fs::write(&path, "original").unwrap();
        std::fs::write(&backup, "previous").unwrap();
        let result = write_atomically_with_directory(&path, "replacement", |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "fixture directory refusal",
            ))
        });
        assert!(result.is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "original");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "previous");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}
