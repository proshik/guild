//! When a worktree's merge request is asked, and what a reply may change —
//! GLD-063. Pure, on the caller's clock: the asking is `root/forge.rs`, the
//! reading `guild_session::merge_requests::glab`.
//!
//! - **One logical refresh per repository**: everything its shown
//!   worktrees need, batched by GitLab host and project, however many glab
//!   calls that takes (spec review r2, finding 3).
//! - **Each repository keeps its own answers and failures**: two clones
//!   of one project never overwrite each other (code review r1,
//!   bugs+impl-13).
//! - **One pending refresh per repository**: a trigger while one is out
//!   does not fold into it — that request already captured its branches —
//!   but marks one more, run when it ends (finding 5).
//! - **A reply is checked before it publishes**: against the repository's
//!   current candidates, the setting's generation, and — after a push or a
//!   switch made from Guild — the worktree list after that write: until it
//!   lands, no reply publishes and nothing is asked (code review r1,
//!   bugs+impl-5).
//! - **A repository the column stops showing is hidden, not forgotten**:
//!   its last known MRs stay in memory — never on disk — while it is
//!   neither asked nor drawn, its refresh out stopped and that refresh's
//!   answer dropped; shown again, its rows draw them at once and it is
//!   asked in the background. Only the setting going off forgets them.
//! - **A resolution belongs to the candidate it was made for**: a worktree
//!   whose destination changed keeps nothing of the old one (code review
//!   r1, bugs+impl-6).
//! - **An incomplete answer is unknown, not empty — per branch**: a
//!   branch whose read failed keeps its own last known MR, with its own
//!   failure beside it, and one never read says it is unknown (code review
//!   r1, adversarial-9); another branch's failure changes nothing of it.
//! - **A frame only for a visible change**: [`Forge::land`] compares what
//!   the repository's rows and state draw before and after, and an answer
//!   that changes neither asks for no notify.
//! - **What is not a branch's is the repository's**: glab missing, a host
//!   not logged in or with a port, a batch that failed as a whole — glab's
//!   proxy, the network policy, the deadline spent before it started — is
//!   [`Forge::status`]'s, never a row's.
use guild_session::merge_requests::{
    glab::{Answers, Error},
    hosts::{Hosts, Login},
    remote::{Remote, looks_like_gitlab},
    status::MergeRequest,
};
use std::{
    collections::{BTreeMap, HashMap},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// A focus within this of a repository's last refresh asks nothing: cmd-tab
/// is not a request each time.
pub const FOCUS_DEBOUNCE: Duration = Duration::from_secs(30);
/// While the window is active, each repository is asked this often.
pub const EVERY: Duration = Duration::from_secs(120);

/// A shown worktree whose branch has a destination: what a refresh asks.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Candidate {
    /// The worktree's folder.
    pub worktree: PathBuf,
    /// The push remote's URL.
    pub url: String,
    /// The branch on that remote — the MR's source branch.
    pub branch: String,
}

/// What asks every repository again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The window became active again; debounced.
    Focus,
    /// The window's clock, while it is active; every [`EVERY`].
    Tick,
}

/// Why a worktree's row says nothing about an MR — said only where it helps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Why {
    /// glab knows the host and is not logged in to it.
    NotLoggedIn(String),
    /// glab is not on the login PATH and the remote looks like GitLab.
    NotFound,
    /// glab is signed in to a host with an explicit port, which its
    /// `--hostname` refuses: inactive, no glab started.
    PortedHost(String),
}

/// How a refresh resolved one candidate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolution {
    /// Asked of this glab host and project.
    Asked {
        /// glab's host name.
        host: String,
        /// The project's path.
        project: String,
    },
    /// Not asked, and why, for the row's hover.
    Why(Why),
    /// Not asked, and nothing to say: not GitLab as far as Guild knows.
    Silent,
}

/// One host and project's reads: each branch's MR or its own failure, or
/// why the batch as a whole has none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// glab's host name.
    pub host: String,
    /// The project's path.
    pub project: String,
    /// Each branch's answer, or the batch's failure as a whole.
    pub result: Result<Answers, Error>,
}

/// Why some of a repository's MRs could not be asked at all — the
/// repository's state, never a row's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Problem {
    /// No glab was started for a host, and why: not logged in — a 401
    /// included — glab missing, or a host with a port.
    Why(Why),
    /// A host and project's batch failed as a whole: glab's own proxy over
    /// Guild's, the network policy, the deadline spent before it started.
    Batch {
        /// glab's host name.
        host: String,
        /// The project's path.
        project: String,
        /// What failed.
        error: Error,
    },
}

/// Where a repository's refresh stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// Nothing due and nothing out.
    Idle,
    /// Due, waiting for the refresh out or for a write's list to land.
    Queued,
    /// A refresh is out.
    Checking,
}

/// A repository's state, for its header: whether it is being asked, when
/// it was last answered, and what could not be asked at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStatus<'a> {
    /// Where its refresh stands.
    pub progress: Progress,
    /// When the last answer landed; `None` before the first.
    pub checked: Option<Instant>,
    /// What that answer could not ask, once each.
    pub problems: &'a [Problem],
}

/// A refresh to run: what was asked, under which generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    /// The repository, by its common git directory.
    pub repo: PathBuf,
    /// Its candidates as they stood when asked.
    pub candidates: Vec<Candidate>,
    /// The setting's generation when asked.
    pub generation: u64,
}

/// What a refresh brought back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    /// The repository asked.
    pub repo: PathBuf,
    /// The candidates asked.
    pub candidates: Vec<Candidate>,
    /// The generation asked under.
    pub generation: u64,
    /// How each candidate resolved.
    pub resolutions: Vec<(Candidate, Resolution)>,
    /// The batches read.
    pub batches: Vec<Batch>,
}

/// What a row draws for its worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown<'a> {
    /// The plain branch glyph, nothing to say.
    Nothing,
    /// The plain branch glyph: the branch was asked and never answered —
    /// unknown, not "no MR".
    Unknown {
        /// Why the branch's last read failed.
        error: &'a Error,
        /// When.
        at: Instant,
    },
    /// The MR.
    Mr {
        /// The MR as last read.
        mr: &'a MergeRequest,
        /// When it was read.
        at: Instant,
        /// The branch's last failure, when it came after that read.
        failure: Option<(&'a Error, Instant)>,
    },
}

type BranchKey = (String, String, String);

#[derive(Debug, Default)]
struct Repo {
    candidates: Vec<Candidate>,
    /// The generation and write epoch of the refresh out.
    in_flight: Option<(u64, u64)>,
    pending: bool,
    last: Option<Instant>,
    /// Bumped by a write made from Guild; a reply asked before is dropped.
    epoch: u64,
    /// A write was made, and the worktree list after it has not landed.
    awaiting: bool,
    /// The column no longer shows it: kept, with its last known MRs,
    /// neither asked nor drawn until it is shown again.
    hidden: bool,
    resolutions: HashMap<Candidate, Resolution>,
    found: HashMap<BranchKey, (Option<MergeRequest>, Instant)>,
    /// Each branch's last failure, kept until its next read holds.
    failures: HashMap<BranchKey, (Error, Instant)>,
    /// What the last landed answer could not ask at all.
    problems: Vec<Problem>,
    /// When the last answer landed.
    checked: Option<Instant>,
}

impl Repo {
    /// What `candidate`'s row draws.
    fn shown(&self, candidate: &Candidate) -> Shown<'_> {
        match self.resolutions.get(candidate) {
            Some(Resolution::Asked { host, project }) => {
                let key = (host.clone(), project.clone(), candidate.branch.clone());
                let failure = self.failures.get(&key);
                match (self.found.get(&key), failure) {
                    (Some((Some(mr), at)), failure) => Shown::Mr {
                        mr,
                        at: *at,
                        failure: failure
                            .filter(|(_, when)| when >= at)
                            .map(|(why, when)| (why, *when)),
                    },
                    (None, Some((error, at))) => Shown::Unknown { error, at: *at },
                    // Read with no MR, and the refresh after it failed:
                    // unknown now, not a confirmed absence (code review
                    // r2, bugs+impl-11).
                    (Some((None, read)), Some((error, at))) if at >= read => {
                        Shown::Unknown { error, at: *at }
                    }
                    _ => Shown::Nothing,
                }
            }
            // A reason is the repository's, in its status, not the row's.
            _ => Shown::Nothing,
        }
    }

    /// What the repository draws, owned: every row's, and its state's
    /// problems. Two answers whose pictures are equal need no frame.
    fn picture(&self) -> (Vec<Drawn>, Vec<Problem>) {
        let rows = self
            .candidates
            .iter()
            .map(|candidate| Drawn::of(&self.shown(candidate)))
            .collect();
        (rows, self.problems.clone())
    }
}

/// What a row draws, as far as a frame can tell: the times a read and a
/// failure were made at are left out but for the one a drawn line ages
/// from — a failure's line says how long ago the MR beside it was read.
#[derive(Debug, PartialEq, Eq)]
enum Drawn {
    Nothing,
    Unknown(Error),
    Mr {
        mr: Box<MergeRequest>,
        failure: Option<(Error, Instant)>,
    },
}

impl Drawn {
    fn of(shown: &Shown<'_>) -> Self {
        match shown {
            Shown::Nothing => Self::Nothing,
            Shown::Unknown { error, .. } => Self::Unknown((*error).clone()),
            Shown::Mr { mr, at, failure } => Self::Mr {
                mr: Box::new((*mr).clone()),
                failure: failure.map(|(error, _)| (error.clone(), *at)),
            },
        }
    }
}

/// Every repository's MRs and their refresh state.
#[derive(Debug)]
pub struct Forge {
    enabled: bool,
    generation: u64,
    repos: BTreeMap<PathBuf, Repo>,
}

impl Forge {
    /// On or off, as the setting says.
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            generation: 0,
            repos: BTreeMap::new(),
        }
    }

    /// Whether the setting is on.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The setting's generation: a reply asked under another is dropped.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Turn it on or off. Off forgets every MR and reason and drops every
    /// reply still out — a refresh out stays counted as out until it ends,
    /// so turning on again never runs a second beside it; on asks every
    /// repository. Whether it moved.
    pub fn set_enabled(&mut self, enabled: bool, _now: Instant) -> bool {
        if self.enabled == enabled {
            return false;
        }
        self.enabled = enabled;
        self.generation += 1;
        for repo in self.repos.values_mut() {
            repo.resolutions.clear();
            repo.found.clear();
            repo.failures.clear();
            repo.problems.clear();
            repo.checked = None;
            repo.pending = enabled;
            repo.last = None;
        }
        // Off forgets: a hidden repository with nothing out goes whole.
        if !enabled {
            self.repos
                .retain(|_, repo| !repo.hidden || repo.in_flight.is_some());
        }
        true
    }

    /// A repository's candidates as its worktree list now stands. One never
    /// asked makes the repository due — the start, and a new worktree or
    /// destination.
    pub fn inputs(&mut self, repo: &Path, mut candidates: Vec<Candidate>, _now: Instant) {
        candidates.sort();
        candidates.dedup();
        let entry = self.repos.entry(repo.to_path_buf()).or_default();
        if entry.hidden {
            // Shown again: its last known MRs draw at once, and it is asked
            // in the background — after the refresh still out, if one is.
            entry.hidden = false;
            entry.pending = true;
        }
        if entry.candidates == candidates {
            return;
        }
        let new = candidates
            .iter()
            .any(|candidate| !entry.candidates.contains(candidate));
        entry.candidates = candidates;
        // A candidate that changed — another URL or branch — leaves its
        // resolution behind with the old one.
        let current = &entry.candidates;
        entry
            .resolutions
            .retain(|candidate, _| current.contains(candidate));
        if new {
            entry.pending = true;
        }
    }

    /// Hide the repositories the column no longer shows: each keeps its
    /// last known MRs in memory, is neither asked nor drawn, and draws them
    /// again the moment it is shown. Those with a refresh still out are
    /// returned, for their owner to stop.
    pub fn retain(&mut self, keep: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
        let mut out = Vec::new();
        for (path, repo) in &mut self.repos {
            if keep(path) || repo.hidden {
                continue;
            }
            repo.hidden = true;
            repo.pending = false;
            if repo.in_flight.is_some() {
                // The refresh out loses its right to publish, though it is
                // still owned until its thread ends: a return with the same
                // candidates must not take its answer (code review r2,
                // bugs+impl-7).
                repo.epoch += 1;
                out.push(path.clone());
            }
        }
        out
    }

    /// Something that asks every repository again, or `repo` alone.
    pub fn trigger(&mut self, repo: Option<&Path>, trigger: Trigger, now: Instant) {
        if !self.enabled {
            return;
        }
        for (path, state) in &mut self.repos {
            if state.hidden || repo.is_some_and(|only| only != path.as_path()) {
                continue;
            }
            let since = state.last.map(|at| now.saturating_duration_since(at));
            let due = match trigger {
                Trigger::Focus => since.is_none_or(|since| since >= FOCUS_DEBOUNCE),
                Trigger::Tick => since.is_none_or(|since| since >= EVERY),
            };
            if due {
                state.pending = true;
            }
        }
    }

    /// A push or a branch switch made from Guild in `repo` was carried
    /// out: a reply asked before it is dropped, and the repository is asked
    /// again — once the worktree list after the write has landed.
    pub fn wrote(&mut self, repo: &Path, _now: Instant) {
        if let Some(state) = self.repos.get_mut(repo) {
            state.epoch += 1;
            state.awaiting = true;
            state.pending = true;
        }
    }

    /// The worktree list after the last write has landed for `repo`.
    pub fn snapshot_landed(&mut self, repo: &Path) {
        if let Some(state) = self.repos.get_mut(repo) {
            state.awaiting = false;
        }
    }

    /// The refreshes to start now: each pending repository with none out,
    /// something to ask, and no write's list still to land.
    pub fn take_due(&mut self, now: Instant) -> Vec<Ask> {
        if !self.enabled {
            return Vec::new();
        }
        let generation = self.generation;
        let mut asks = Vec::new();
        for (repo, state) in &mut self.repos {
            if state.hidden || !state.pending || state.in_flight.is_some() || state.awaiting {
                continue;
            }
            state.pending = false;
            if state.candidates.is_empty() {
                continue;
            }
            state.in_flight = Some((generation, state.epoch));
            state.last = Some(now);
            asks.push(Ask {
                repo: repo.clone(),
                candidates: state.candidates.clone(),
                generation,
            });
        }
        asks
    }

    /// Whether a refresh is out for `repo`.
    pub fn in_flight(&self, repo: &Path) -> bool {
        self.repos
            .get(repo)
            .is_some_and(|state| state.in_flight.is_some())
    }

    /// A refresh that ended with no answer — its thread gone: the
    /// repository may be asked again, and is.
    pub fn abandon(&mut self, repo: &Path) {
        if let Some(state) = self.repos.get_mut(repo) {
            state.in_flight = None;
            state.pending = true;
        }
    }

    /// Take a refresh's answer in, if it still holds. Whether what the rows
    /// or the repository's state draw changed: an answer that brings what
    /// is already shown asks for no frame, since a notify of the root
    /// rebuilds the whole column (`docs/traps.md`, `view::region`).
    pub fn land(&mut self, answer: Answer, now: Instant) -> bool {
        let Some(state) = self.repos.get_mut(&answer.repo) else {
            return false;
        };
        let asked = state.in_flight.take();
        if !self.enabled || answer.generation != self.generation {
            return false;
        }
        // Asked before a write made since, or with branches the rows no
        // longer have: not the rows' any more, and the pending refresh
        // brings theirs.
        let epoch = asked.map(|(_, epoch)| epoch);
        if epoch != Some(state.epoch) || state.candidates != answer.candidates {
            state.pending = true;
            return false;
        }
        let before = state.picture();
        let mut problems = Vec::new();
        for (_, resolution) in &answer.resolutions {
            if let Resolution::Why(why) = resolution {
                let problem = Problem::Why(why.clone());
                if !problems.contains(&problem) {
                    problems.push(problem);
                }
            }
        }
        // A candidate that was asked and now has a reason not to be — its
        // host signed out, glab gone — keeps what it last read: the reason
        // is the repository's, and the row's MR is not a confirmed absence.
        let mut resolutions: HashMap<Candidate, Resolution> =
            answer.resolutions.into_iter().collect();
        for (candidate, resolution) in &mut resolutions {
            if let (Resolution::Why(_), Some(asked @ Resolution::Asked { .. })) =
                (&*resolution, state.resolutions.get(candidate))
            {
                *resolution = asked.clone();
            }
        }
        state.resolutions = resolutions;
        for batch in answer.batches {
            match batch.result {
                Ok(branches) => {
                    for (branch, read) in branches {
                        let key = (batch.host.clone(), batch.project.clone(), branch);
                        match read {
                            Ok(mr) => {
                                state.failures.remove(&key);
                                state.found.insert(key, (mr, now));
                            }
                            Err(why) => {
                                state.failures.insert(key, (why, now));
                            }
                        }
                    }
                }
                Err(error) => problems.push(Problem::Batch {
                    host: batch.host,
                    project: batch.project,
                    error,
                }),
            }
        }
        state.problems = problems;
        state.checked = Some(now);
        // What the repository's header would draw of its progress is not in
        // the picture while nothing draws it.
        state.picture() != before
    }

    /// What a worktree's row draws.
    pub fn shown(&self, worktree: &Path) -> Shown<'_> {
        if !self.enabled {
            return Shown::Nothing;
        }
        for state in self.repos.values().filter(|state| !state.hidden) {
            if let Some(candidate) = state.candidates.iter().find(|c| c.worktree == worktree) {
                return state.shown(candidate);
            }
        }
        Shown::Nothing
    }

    /// A repository's state, for its header. `None` while the setting is
    /// off, while the column does not show it, or before it was taken in.
    pub fn status(&self, repo: &Path) -> Option<RepoStatus<'_>> {
        let state = self.repos.get(repo).filter(|state| !state.hidden)?;
        if !self.enabled {
            return None;
        }
        let progress = if state.in_flight.is_some() {
            Progress::Checking
        } else if state.pending && !state.candidates.is_empty() {
            Progress::Queued
        } else {
            Progress::Idle
        };
        Some(RepoStatus {
            progress,
            checked: state.checked,
            problems: &state.problems,
        })
    }
}

/// How each candidate resolves, and the batches to read: `(host, project,
/// branches)` for every signed-in glab host — no request for a signed-out
/// host, or at all without glab. A reason is kept only where it helps:
/// glab knows the host and is not logged in, or glab is missing and the
/// remote looks like GitLab.
#[allow(clippy::type_complexity)]
pub fn resolve(
    candidates: &[Candidate],
    glab_located: bool,
    hosts: &Hosts,
) -> (
    Vec<(Candidate, Resolution)>,
    Vec<(String, String, Vec<String>)>,
) {
    let mut resolutions = Vec::new();
    let mut batches: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for candidate in candidates {
        let resolution = match Remote::parse(&candidate.url) {
            None => Resolution::Silent,
            Some(remote) if !glab_located => {
                if looks_like_gitlab(&remote.host) {
                    Resolution::Why(Why::NotFound)
                } else {
                    Resolution::Silent
                }
            }
            Some(remote) => match hosts.login(&remote.host, remote.ssh) {
                // glab 1.118 refuses `--hostname host:port` as an invalid
                // hostname: such a host is inactive, and no glab is started
                // for it. The port stays in the host's identity.
                Login::SignedIn(host) if host.contains(':') => {
                    Resolution::Why(Why::PortedHost(host))
                }
                Login::SignedIn(host) => {
                    // The path after the host's subfolder, which glab puts
                    // in its API base itself (code review r2, adversarial-4).
                    let project = hosts.project(&host, &remote.project);
                    let branches = batches.entry((host.clone(), project.clone())).or_default();
                    if !branches.contains(&candidate.branch) {
                        branches.push(candidate.branch.clone());
                    }
                    Resolution::Asked { host, project }
                }
                Login::SignedOut => Resolution::Why(Why::NotLoggedIn(remote.host)),
                Login::Unknown => Resolution::Silent,
            },
        };
        resolutions.push((candidate.clone(), resolution));
    }
    let batches = batches
        .into_iter()
        .map(|((host, project), branches)| (host, project, branches))
        .collect();
    (resolutions, batches)
}
