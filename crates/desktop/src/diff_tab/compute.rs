//! One CPU owner per diff, with replaceable inputs and ticketed atomic results.
use super::*;
use std::sync::{Condvar, Mutex};
use std::thread::JoinHandle;
use tokio::sync::Notify;

type Side = Option<Result<Option<Arc<[u8]>>, GitError>>;
#[derive(Clone, PartialEq, Eq)]
pub(super) struct Input {
    pub identity: (PathBuf, Base, Option<String>),
    pub old: Side,
    pub new: Side,
    oversized: [bool; 2],
}
impl Input {
    pub fn matches(
        &self,
        identity: (&Path, &Base, &Option<String>),
        old: Option<&Result<Option<Vec<u8>>, GitError>>,
        new: Option<Result<Option<&[u8]>, &GitError>>,
    ) -> bool {
        let side =
            |owned: &Side, oversized: bool, borrowed: Option<Result<Option<&[u8]>, &GitError>>| {
                match (owned, borrowed) {
                    (None, None) => true,
                    (Some(Ok(a)), Some(Ok(b))) => {
                        if b.is_some_and(|b| b.len() as u64 > DIFF_LIMIT) {
                            oversized
                        } else {
                            !oversized && a.as_deref() == b
                        }
                    }
                    (Some(Err(a)), Some(Err(b))) => a == b,
                    _ => false,
                }
            };
        self.identity.0 == identity.0
            && &self.identity.1 == identity.1
            && &self.identity.2 == identity.2
            && side(
                &self.old,
                self.oversized[0],
                old.map(|v| v.as_ref().map(|v| v.as_deref())),
            )
            && side(&self.new, self.oversized[1], new)
    }
    pub fn capture(
        identity: (PathBuf, Base, Option<String>),
        old: Option<&Result<Option<Vec<u8>>, GitError>>,
        new: Option<Result<Option<&[u8]>, &GitError>>,
    ) -> Self {
        let too_large = |v: Option<Result<Option<&[u8]>, &GitError>>| {
            v.and_then(Result::ok)
                .flatten()
                .is_some_and(|v| v.len() as u64 > DIFF_LIMIT)
        };
        let oversized = [
            too_large(old.map(|v| v.as_ref().map(|v| v.as_deref()))),
            too_large(new),
        ];
        let copy = |v: Result<Option<&[u8]>, &GitError>| {
            v.map(|v| {
                v.map(|b| {
                    Arc::from(if b.len() as u64 > DIFF_LIMIT {
                        &[][..]
                    } else {
                        b
                    })
                })
            })
            .map_err(Clone::clone)
        };
        Self {
            identity,
            old: old.map(|v| copy(v.as_ref().map(|v| v.as_deref()))),
            new: new.map(copy),
            oversized,
        }
    }
}
pub(super) struct ResultPair {
    pub ticket: u64,
    pub file: FileDiff,
    pub document: Option<Arc<Document>>,
    pub paint: Option<Arc<Painted>>,
}
struct Job {
    path: PathBuf,
    ticket: u64,
    input: Input,
}
#[derive(Default)]
struct Mailbox {
    stop: bool,
    tickets: HashMap<PathBuf, u64>,
    pending: HashMap<PathBuf, Job>,
    order: std::collections::VecDeque<PathBuf>,
    finished: HashMap<PathBuf, ResultPair>,
    active: Option<(PathBuf, u64)>,
}
pub(super) struct Shared {
    state: Mutex<Mailbox>,
    wake: Condvar,
    changed: Arc<Notify>,
    #[cfg(test)]
    gate: Mutex<
        Option<(
            std::sync::mpsc::SyncSender<()>,
            std::sync::mpsc::Receiver<()>,
        )>,
    >,
}
pub(super) struct Owner {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    executor: gpui::BackgroundExecutor,
}
impl Owner {
    pub fn new(executor: gpui::BackgroundExecutor) -> Self {
        let shared = Arc::new(Shared {
            state: Mutex::new(Mailbox::default()),
            wake: Condvar::new(),
            changed: Arc::new(Notify::new()),
            #[cfg(test)]
            gate: Mutex::new(None),
        });
        let work = shared.clone();
        let worker = std::thread::Builder::new()
            .name("guild-diff".into())
            .spawn(move || run(work));
        match worker {
            Ok(worker) => Self {
                shared,
                worker: Some(worker),
                executor,
            },
            Err(error) => {
                tracing::error!(%error, "diff computation thread could not start");
                shared.state.lock().unwrap().stop = true;
                Self {
                    shared,
                    worker: None,
                    executor,
                }
            }
        }
    }
    pub fn signal(&self) -> Arc<Notify> {
        self.shared.changed.clone()
    }
    pub fn submit(&self, path: PathBuf, ticket: u64, input: Input) {
        let mut state = self.shared.state.lock().unwrap();
        state.tickets.insert(path.clone(), ticket);
        state.finished.remove(&path);
        if state.stop {
            state.finished.insert(
                path.clone(),
                ResultPair {
                    ticket,
                    file: FileDiff::failed(
                        path,
                        &GitError::Failed("diff computation worker is unavailable".into()),
                    ),
                    document: None,
                    paint: None,
                },
            );
            self.shared.changed.notify_one();
        } else {
            if !state.pending.contains_key(&path) {
                state.order.push_back(path.clone());
            }
            state.pending.insert(
                path.clone(),
                Job {
                    path,
                    ticket,
                    input,
                },
            );
            self.shared.wake.notify_one();
        }
    }
    pub fn revoke(&self, path: &Path) {
        let mut state = self.shared.state.lock().unwrap();
        state.tickets.remove(path);
        state.pending.remove(path);
        state.finished.remove(path);
        state.order.retain(|p| p != path);
    }
    pub fn retain(&self, paths: &HashSet<PathBuf>) {
        let mut state = self.shared.state.lock().unwrap();
        state.tickets.retain(|p, _| paths.contains(p));
        state.pending.retain(|p, _| paths.contains(p));
        state.order.retain(|p| paths.contains(p));
        state.finished.retain(|p, _| paths.contains(p));
    }
    pub fn drain(&self) -> Vec<(PathBuf, ResultPair)> {
        self.shared.state.lock().unwrap().finished.drain().collect()
    }
    pub fn stop(&mut self) -> Option<JoinHandle<()>> {
        let mut state = self.shared.state.lock().unwrap();
        state.stop = true;
        state.tickets.clear();
        state.pending.clear();
        state.order.clear();
        state.finished.clear();
        self.shared.wake.notify_one();
        self.worker.take()
    }
    #[cfg(test)]
    pub fn gate(&self) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        let (entered, observed) = std::sync::mpsc::sync_channel(1);
        let (release, waiting) = std::sync::mpsc::channel();
        *self.shared.gate.lock().unwrap() = Some((entered, waiting));
        (observed, release)
    }
    #[cfg(test)]
    pub fn weak(&self) -> std::sync::Weak<Shared> {
        Arc::downgrade(&self.shared)
    }
    #[cfg(test)]
    pub fn counts(&self) -> (usize, usize, usize) {
        let s = self.shared.state.lock().unwrap();
        (
            usize::from(s.active.is_some()),
            s.pending.len(),
            s.finished.len(),
        )
    }
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        let state = self.shared.state.lock().unwrap();
        state.pending.is_empty() && state.active.is_none() && state.finished.is_empty()
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        if let Some(worker) = self.stop() {
            self.executor
                .spawn(async move {
                    let _ = worker.join();
                })
                .detach();
        }
    }
}
fn fresh(shared: &Shared, job: &Job) -> bool {
    let state = shared.state.lock().unwrap();
    !state.stop && state.tickets.get(&job.path) == Some(&job.ticket)
}
fn run(shared: Arc<Shared>) {
    loop {
        let job = {
            let mut state = shared.state.lock().unwrap();
            while !state.stop && state.pending.is_empty() {
                state = shared.wake.wait(state).unwrap();
            }
            if state.stop {
                return;
            }
            let path = state.order.pop_front().unwrap();
            let job = state.pending.remove(&path).unwrap();
            state.active = Some((path, job.ticket));
            job
        };
        let result = calculate(&shared, &job);
        #[cfg(test)]
        {
            let gate = shared.gate.lock().unwrap().take();
            if let Some((entered, release)) = gate {
                let _ = entered.send(());
                let _ = release.recv();
            }
        }
        let mut state = shared.state.lock().unwrap();
        state.active = None;
        if !state.stop
            && state.tickets.get(&job.path) == Some(&job.ticket)
            && let Some(result) = result
        {
            state.finished.insert(job.path, result);
            shared.changed.notify_one();
        }
    }
}
fn calculate(shared: &Shared, job: &Job) -> Option<ResultPair> {
    if !fresh(shared, job) {
        return None;
    }
    let sides = (&job.input.old, &job.input.new);
    let mut file = match sides {
        (None, _) | (_, None) => FileDiff::loading(job.path.clone()),
        (Some(Err(e)), _) | (_, Some(Err(e))) => FileDiff::failed(job.path.clone(), e),
        (Some(Ok(_)), Some(Ok(_))) if job.input.oversized.iter().any(|v| *v) => FileDiff {
            path: job.path.clone(),
            state: State::TooLarge,
            hunks: Vec::new(),
            counts: (0, 0),
        },
        (Some(Ok(old)), Some(Ok(new))) => {
            FileDiff::compute(job.path.clone(), old.as_deref(), new.as_deref())
        }
    };
    if !fresh(shared, job) {
        return None;
    }
    let mut document = None;
    let mut paint = None;
    if matches!(
        file.state,
        State::Hunks | State::Added | State::Deleted | State::NoChanges
    ) {
        let text = |side: &Side| {
            side.as_ref()
                .and_then(|v| v.as_ref().ok())
                .and_then(|v| v.as_deref())
                .and_then(|b| std::str::from_utf8(b).ok())
                .unwrap_or("")
                .to_owned()
        };
        let old = text(&job.input.old);
        let new = text(&job.input.new);
        let doc = Document::new(&old, &new);
        if !fresh(shared, job) {
            return None;
        }
        file.counts = doc
            .blocks
            .iter()
            .fold((0, 0), |(a, r), b| (a + b.new.len(), r + b.old.len()));
        if matches!(file.state, State::Hunks | State::NoChanges) {
            file.state = if doc.blocks.is_empty() {
                State::NoChanges
            } else {
                State::Hunks
            };
        }
        if matches!(file.state, State::Hunks | State::Added | State::Deleted) {
            paint = Painted::of(
                &job.path,
                &old.replace("\r\n", "\n"),
                &new.replace("\r\n", "\n"),
            )
            .map(Arc::new);
        }
        document = Some(Arc::new(doc));
    }
    fresh(shared, job).then_some(ResultPair {
        ticket: job.ticket,
        file,
        document,
        paint,
    })
}
