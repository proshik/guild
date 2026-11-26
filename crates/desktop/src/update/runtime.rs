//! One owned updater worker. Heavy payloads and cleanup never cross onto the UI.
use super::{
    Build,
    control::{Identity, Permit, Ticket},
    preparation, remote,
};
use guild_session::update::{BuildInfo, Source, transaction};
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};
/// Private state directories; Local producer output is distinct from updater work/cache.
pub struct Config {
    pub default_stage: PathBuf,
    pub work: PathBuf,
    pub cache: PathBuf,
}
/// A small UI snapshot; owns no temporary directory or native child.
#[derive(Debug, Clone)]
pub struct View {
    pub identity: Identity,
    pub build: Build,
    pub info: Option<BuildInfo>,
    pub notes: String,
}
/// Every asynchronous mutation retains its opaque selection/operation ticket.
pub enum Request {
    Inspect {
        ticket: Ticket,
        state: PathBuf,
        destination: Option<PathBuf>,
    },
    Check(Ticket),
    PollLocal(Ticket),
    Download {
        ticket: Ticket,
        identity: Identity,
    },
    Cache {
        ticket: Ticket,
        permit: Permit,
    },
    Handoff {
        ticket: Ticket,
        identity: Identity,
    },
    Journal {
        ticket: Ticket,
        identity: Identity,
        destination: PathBuf,
        state: PathBuf,
    },
    Prepare {
        ticket: Ticket,
        identity: Identity,
        root: PathBuf,
    },
}
impl Request {
    fn ticket(&self) -> &Ticket {
        match self {
            Self::Inspect { ticket, .. }
            | Self::Check(ticket)
            | Self::PollLocal(ticket)
            | Self::Download { ticket, .. }
            | Self::Cache { ticket, .. }
            | Self::Prepare { ticket, .. }
            | Self::Journal { ticket, .. }
            | Self::Handoff { ticket, .. } => ticket,
        }
    }
}
/// Sanitized typed failures for localized UI, without raw tool or request data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Remote(remote::Error),
    Preparation(preparation::Failure),
    Transaction(transaction::Failure),
    Cancelled,
    Unavailable,
    Internal,
}
/// Final operation replies; progress is coalesced separately.
#[derive(Debug)]
pub enum Event {
    Inspected {
        ticket: Ticket,
        receipt: Option<Box<transaction::Receipt>>,
    },
    Unchanged {
        ticket: Ticket,
    },
    Checked {
        ticket: Ticket,
        candidates: Vec<View>,
        unverified: usize,
        ready: Option<Identity>,
        cache_warning: Option<remote::Error>,
    },
    Downloaded {
        ticket: Ticket,
        candidate: Box<View>,
    },
    Cached {
        ticket: Ticket,
        identity: Identity,
    },
    HandoffReady {
        ticket: Ticket,
        identity: Identity,
        token: transaction::ArmToken,
    },
    Journaled {
        ticket: Ticket,
        identity: Identity,
        receipt: Box<guild_session::update::transaction::Receipt>,
    },
    Prepared {
        ticket: Ticket,
        identity: Identity,
        bundle: PathBuf,
    },
    Progress {
        ticket: Ticket,
        progress: remote::Progress,
    },
    Failed {
        ticket: Ticket,
        error: Failure,
    },
}
impl Event {
    /// The host must reject every stale event, including errors and progress.
    pub fn ticket(&self) -> &Ticket {
        match self {
            Self::Inspected { ticket, .. }
            | Self::Unchanged { ticket }
            | Self::Checked { ticket, .. }
            | Self::Downloaded { ticket, .. }
            | Self::Cached { ticket, .. }
            | Self::Prepared { ticket, .. }
            | Self::Journaled { ticket, .. }
            | Self::HandoffReady { ticket, .. }
            | Self::Progress { ticket, .. }
            | Self::Failed { ticket, .. } => ticket,
        }
    }
}
struct State {
    pending: Option<Job>,
    active: Option<Arc<AtomicBool>>,
    stopping: bool,
    completed: Option<Event>,
    progress: Option<Event>,
}
enum Job {
    Request(Request),
    Clear,
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
/// One thread, one current job and one replaceable pending job. The UI invalidates
/// Gate first, then calls clear/submit to cancel and retire the old worker result.
pub struct Runtime {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Runtime {
    /// Start an owned worker; provider absence reports configuration unavailable.
    pub fn spawn(config: Config, provider: Option<remote::Provider>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                pending: None,
                active: None,
                stopping: false,
                completed: None,
                progress: None,
            }),
            wake: Condvar::new(),
        });
        let actor_shared = shared.clone();
        let worker = std::thread::Builder::new()
            .name("guild-updater".into())
            .spawn(move || run(actor_shared, config, provider))?;
        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }
    /// Submit the latest desired operation and cancel the previous operation.
    pub fn submit(&self, request: Request) -> bool {
        self.enqueue(Job::Request(request))
    }
    /// Cancel work immediately and dispose selected/prepared payloads on the worker.
    pub fn clear(&self) -> bool {
        self.enqueue(Job::Clear)
    }
    fn enqueue(&self, job: Job) -> bool {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.stopping {
            return false;
        }
        if let Some(cancel) = &state.active {
            cancel.store(true, Ordering::Release);
        }
        state.pending = Some(job);
        state.progress = None;
        state.completed = None;
        self.shared.wake.notify_one();
        true
    }
    /// Read only small event snapshots; progress precedes the final result.
    pub fn drain(&self) -> Vec<Event> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        [state.progress.take(), state.completed.take()]
            .into_iter()
            .flatten()
            .collect()
    }
    /// No operation or deferred cleanup remains on the worker.
    pub fn idle(&self) -> bool {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active.is_none() && state.pending.is_none()
    }
    /// Request stop without blocking the window; the host joins this handle on
    /// its background executor as part of the existing quit admission sequence.
    pub fn stop(&mut self) -> Option<JoinHandle<()>> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopping = true;
        state.pending = None;
        if let Some(cancel) = &state.active {
            cancel.store(true, Ordering::Release);
        }
        self.shared.wake.notify_one();
        self.worker.take()
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        // Fallback only: normal UI quit takes/joins via stop off-thread. Never
        // abandon a native child or an owned preparation if construction unwinds.
        if let Some(worker) = self.stop() {
            let _ = worker.join();
        }
    }
}
fn run(shared: Arc<Shared>, config: Config, provider: Option<remote::Provider>) {
    let mut actor = Actor::new(config, provider);
    loop {
        let (job, cancel) = {
            let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            while state.pending.is_none() && !state.stopping {
                state = shared.wake.wait(state).unwrap_or_else(|e| e.into_inner());
            }
            if state.stopping {
                break;
            }
            let job = state.pending.take().unwrap();
            let cancel = Arc::new(AtomicBool::new(false));
            state.active = Some(cancel.clone());
            (job, cancel)
        };
        let event = match job {
            Job::Clear => {
                actor.clear();
                None
            }
            Job::Request(request) => {
                let ticket = request.ticket().clone();
                Some(
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        actor.execute(request, &cancel, &shared)
                    }))
                    .unwrap_or_else(|_| {
                        actor.clear();
                        Event::Failed {
                            ticket,
                            error: Failure::Internal,
                        }
                    }),
                )
            }
        };
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active = None;
        state.progress = None;
        state.completed = event;
    }
    actor.clear();
}
struct Selected {
    check: Ticket,
    input: preparation::Candidate,
    view: View,
}
struct PendingJournal {
    store: transaction::Store,
    receipt: transaction::Receipt,
    helper: Option<transaction::Handoff>,
}
struct Actor {
    config: Config,
    provider: Option<remote::Provider>,
    discovery: Option<(Ticket, Vec<remote::Candidate>)>,
    selected: Option<Selected>,
    prepared: Option<preparation::Prepared>,
    journal: Option<PendingJournal>,
    committed: bool,
}
impl Actor {
    fn new(config: Config, provider: Option<remote::Provider>) -> Self {
        Self {
            config,
            provider,
            discovery: None,
            selected: None,
            prepared: None,
            journal: None,
            committed: false,
        }
    }
    fn clear(&mut self) {
        self.retire_journal();
        self.prepared = None;
        self.selected = None;
        self.discovery = None;
    }
    fn retire_journal(&mut self) {
        let Some(mut journal) = self.journal.take() else {
            return;
        };
        if let Some(helper) = &mut journal.helper {
            match helper.cancel() {
                Ok(()) => (),
                Err(transaction::Failure::Conflict) => {
                    self.committed = true;
                    // Arming won atomically. Its detached helper owns this receipt;
                    // even a racing stale Clear must never abort that transaction.
                    return;
                }
                Err(failure) => {
                    tracing::warn!(?failure, "update helper still needs cleanup");
                    self.journal = Some(journal);
                    return;
                }
            }
        }
        journal.helper = None;
        if let Err(failure) = journal
            .store
            .abort_prepared(journal.receipt.id(), &AtomicBool::new(false))
        {
            tracing::warn!(?failure, transaction = %journal.receipt.id(), "update preparation needs explicit recovery");
            self.journal = Some(journal);
        }
    }
    fn execute(&mut self, request: Request, cancel: &AtomicBool, shared: &Shared) -> Event {
        let ticket = request.ticket().clone();
        let result = self.execute_inner(request, cancel, shared);
        result.unwrap_or_else(|error| Event::Failed { ticket, error })
    }
    fn execute_inner(
        &mut self,
        request: Request,
        cancel: &AtomicBool,
        shared: &Shared,
    ) -> Result<Event, Failure> {
        check_cancel(cancel)?;
        if self.committed {
            return Err(Failure::Transaction(transaction::Failure::Conflict));
        }
        match request {
            Request::Inspect {
                ticket,
                state,
                destination,
            } => {
                let receipt = destination
                    .map(|destination| transaction::Store::new(state).pending(&destination))
                    .transpose()
                    .map_err(Failure::Transaction)?
                    .flatten()
                    .map(Box::new);
                check_cancel(cancel)?;
                Ok(Event::Inspected { ticket, receipt })
            }
            Request::Handoff { ticket, identity } => {
                self.current(&ticket, Some(&identity))?;
                let journal = self.journal.as_mut().ok_or(Failure::Unavailable)?;
                if journal.receipt.selection() != &identity || journal.helper.is_some() {
                    return Err(Failure::Transaction(transaction::Failure::Conflict));
                }
                let (helper, token) = journal
                    .store
                    .spawn_helper(journal.receipt.id(), cancel)
                    .map_err(Failure::Transaction)?;
                journal.helper = Some(helper);
                check_cancel(cancel)?;
                Ok(Event::HandoffReady {
                    ticket,
                    identity,
                    token,
                })
            }
            Request::Journal {
                ticket,
                identity,
                destination,
                state,
            } => {
                self.current(&ticket, Some(&identity))?;
                if self.journal.is_some() {
                    return Err(Failure::Transaction(transaction::Failure::Conflict));
                }
                let prepared = self.prepared.take().ok_or(Failure::Unavailable)?;
                if prepared.identity() != &identity {
                    return Err(Failure::Unavailable);
                }
                let store = transaction::Store::new(state);
                match store.begin_owned(
                    &destination,
                    prepared.bundle(),
                    &identity,
                    std::process::id(),
                    cancel,
                ) {
                    Ok(receipt) => {
                        self.journal = Some(PendingJournal {
                            store,
                            receipt: receipt.clone(),
                            helper: None,
                        });
                        check_cancel(cancel)?;
                        Ok(Event::Journaled {
                            ticket,
                            identity,
                            receipt: Box::new(receipt),
                        })
                    }
                    Err(error) => {
                        if let Some(receipt) = error.receipt {
                            self.journal = Some(PendingJournal {
                                store,
                                receipt: *receipt,
                                helper: None,
                            });
                            self.retire_journal();
                        }
                        Err(Failure::Transaction(error.failure))
                    }
                }
            }
            Request::Check(ticket) => self.check(ticket, cancel, shared),
            Request::PollLocal(ticket) => {
                if ticket.preferences().source != Source::Local {
                    return Err(Failure::Unavailable);
                }
                if self.journal.is_some() || self.prepared.is_some() {
                    return Err(Failure::Transaction(transaction::Failure::Conflict));
                }
                if let Ok(selected) = self.current(&ticket, None)
                    && selected
                        .input
                        .local_unchanged(ticket.preferences(), &self.config.default_stage)
                        .map_err(preparation_failure)?
                {
                    return Ok(Event::Unchanged { ticket });
                }
                self.check(ticket, cancel, shared)
            }
            Request::Download { ticket, identity } => {
                self.download(ticket, identity, cancel, shared)
            }
            Request::Cache { ticket, permit } => {
                let selected = self.current(&ticket, None)?;
                let identity = selected.view.identity.clone();
                if !permit.pins(&identity) {
                    return Err(Failure::Cancelled);
                }
                let acquired = selected.input.download().ok_or(Failure::Unavailable)?;
                let provider = self.provider.as_ref().ok_or(Failure::Unavailable)?;
                provider
                    .prepare_cache(acquired, &self.config.cache, cancel)
                    .map_err(remote_failure)?
                    .publish(&permit)
                    .map_err(remote_failure)?;
                check_cancel(cancel)?;
                Ok(Event::Cached { ticket, identity })
            }
            Request::Prepare {
                ticket,
                identity,
                root,
            } => {
                if self.journal.is_some() {
                    return Err(Failure::Transaction(transaction::Failure::Conflict));
                }
                self.prepared = None;
                let selected = self.current(&ticket, Some(&identity))?;
                let prepared = selected
                    .input
                    .prepare(&root, cancel)
                    .map_err(preparation_failure)?;
                check_cancel(cancel)?;
                let bundle = prepared.bundle().to_path_buf();
                self.prepared = Some(prepared);
                Ok(Event::Prepared {
                    ticket,
                    identity,
                    bundle,
                })
            }
        }
    }
    fn current(&self, ticket: &Ticket, identity: Option<&Identity>) -> Result<&Selected, Failure> {
        let selected = self.selected.as_ref().ok_or(Failure::Unavailable)?;
        if !selected.check.same_selection(ticket)
            || identity.is_some_and(|id| id != &selected.view.identity)
        {
            return Err(Failure::Unavailable);
        }
        Ok(selected)
    }
    fn check(
        &mut self,
        ticket: Ticket,
        cancel: &AtomicBool,
        shared: &Shared,
    ) -> Result<Event, Failure> {
        self.clear();
        if self.committed || self.journal.is_some() {
            return Err(Failure::Transaction(transaction::Failure::Conflict));
        }
        if ticket.preferences().source == Source::Local {
            let input = preparation::Candidate::local(
                ticket.preferences(),
                &self.config.default_stage,
                cancel,
            )
            .map_err(preparation_failure)?;
            let view = local_view(&input);
            check_cancel(cancel)?;
            let identity = view.identity.clone();
            self.selected = Some(Selected {
                check: ticket.clone(),
                input,
                view: view.clone(),
            });
            return Ok(Event::Checked {
                ticket,
                candidates: vec![view],
                unverified: 0,
                ready: Some(identity),
                cache_warning: None,
            });
        }
        let provider = self.provider.as_ref().ok_or(Failure::Unavailable)?;
        let discovery = provider
            .discover(ticket.preferences().channel, cancel)
            .map_err(remote_failure)?;
        let views = discovery
            .candidates
            .iter()
            .map(remote_view)
            .collect::<Result<Vec<_>, _>>()?;
        let mut cache_warning = None;
        let mut ready = None;
        // A previously chosen historical candidate may remain available even
        // when a newer release appeared. Only fresh signed discovery authorizes
        // revalidation of that exact ready-cache identity.
        for candidate in &discovery.candidates {
            let restored = provider.restore(candidate, &self.config.cache, cancel, &mut |event| {
                progress(shared, &ticket, event)
            });
            match restored {
                Ok(Some(acquired)) => {
                    let input =
                        preparation::Candidate::remote(acquired).map_err(preparation_failure)?;
                    let view = remote_view(candidate)?;
                    ready = Some(view.identity.clone());
                    self.selected = Some(Selected {
                        check: ticket.clone(),
                        input,
                        view,
                    });
                    break;
                }
                Ok(None) => (),
                Err(remote::Error::Cancelled) => return Err(Failure::Cancelled),
                Err(error) => {
                    cache_warning = Some(error);
                    break;
                }
            }
        }
        check_cancel(cancel)?;
        self.discovery = Some((ticket.clone(), discovery.candidates));
        Ok(Event::Checked {
            ticket,
            candidates: views,
            unverified: discovery.unverified,
            ready,
            cache_warning,
        })
    }
    fn download(
        &mut self,
        ticket: Ticket,
        identity: Identity,
        cancel: &AtomicBool,
        shared: &Shared,
    ) -> Result<Event, Failure> {
        if let Ok(selected) = self.current(&ticket, Some(&identity)) {
            return Ok(Event::Downloaded {
                ticket,
                candidate: Box::new(selected.view.clone()),
            });
        }
        let (check, candidates) = self.discovery.as_ref().ok_or(Failure::Unavailable)?;
        if !check.same_selection(&ticket) {
            return Err(Failure::Unavailable);
        }
        let candidate = candidates
            .iter()
            .find(|c| c.identity().as_ref() == Some(&identity))
            .ok_or(Failure::Unavailable)?
            .clone();
        let provider = self.provider.as_ref().ok_or(Failure::Unavailable)?;
        self.prepared = None;
        self.selected = None;
        // This is an explicit acquisition action. A damaged cache can be repaired
        // by a new download; an inaccessible release check cannot reach this path.
        let restored = provider.restore(&candidate, &self.config.cache, cancel, &mut |event| {
            progress(shared, &ticket, event)
        });
        let acquired = match restored {
            Ok(Some(acquired)) => acquired,
            Err(remote::Error::Cancelled) => return Err(Failure::Cancelled),
            _ => provider
                .download(&candidate, &self.config.work, cancel, &mut |event| {
                    progress(shared, &ticket, event)
                })
                .map_err(remote_failure)?,
        };
        check_cancel(cancel)?;
        let input = preparation::Candidate::remote(acquired).map_err(preparation_failure)?;
        let view = remote_view(&candidate)?;
        self.selected = Some(Selected {
            check: ticket.clone(),
            input,
            view: view.clone(),
        });
        Ok(Event::Downloaded {
            ticket,
            candidate: Box::new(view),
        })
    }
}
fn progress(shared: &Shared, ticket: &Ticket, event: remote::Progress) {
    let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
    if !state.stopping {
        state.progress = Some(Event::Progress {
            ticket: ticket.clone(),
            progress: event,
        });
    }
}
fn remote_view(candidate: &remote::Candidate) -> Result<View, Failure> {
    let identity = candidate.identity().ok_or(Failure::Unavailable)?;
    Ok(View {
        identity,
        build: Build::from(&candidate.manifest.build),
        info: Some(candidate.manifest.build.clone()),
        notes: candidate.manifest.notes.clone(),
    })
}
fn local_view(input: &preparation::Candidate) -> View {
    let changelog = super::changelog_of(input.source());
    let version = input.build().tag.as_deref().unwrap_or("Unreleased");
    let notes = changelog
        .iter()
        .find(|release| release.version == version)
        .map(guild_session::changelog::Release::markdown_notes)
        .unwrap_or_default();
    View {
        identity: input.identity().clone(),
        build: input.build().clone(),
        info: input.info().cloned(),
        notes,
    }
}
fn check_cancel(cancel: &AtomicBool) -> Result<(), Failure> {
    if cancel.load(Ordering::Acquire) {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}
fn remote_failure(error: remote::Error) -> Failure {
    if error == remote::Error::Cancelled {
        Failure::Cancelled
    } else {
        Failure::Remote(error)
    }
}
fn preparation_failure(error: preparation::Failure) -> Failure {
    if error == preparation::Failure::Cancelled {
        Failure::Cancelled
    } else {
        Failure::Preparation(error)
    }
}
