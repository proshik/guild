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
//! - **An incomplete answer is unknown, not empty**: a failed batch keeps
//!   its branches' last known MRs, with the failure beside them, and a
//!   branch never read says it is unknown (code review r1, adversarial-9).
use guild_session::merge_requests::{
    glab::Error,
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

/// One host and project's reads: each branch's MR, or why there are none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// glab's host name.
    pub host: String,
    /// The project's path.
    pub project: String,
    /// Each branch's MR, or why the batch has none.
    pub result: Result<Vec<(String, Option<MergeRequest>)>, Error>,
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
    /// The plain branch glyph, with a reason in its hover.
    Why(Why),
    /// The plain branch glyph: the branch was asked and never answered —
    /// unknown, not "no MR".
    Unknown {
        /// Why the last refresh failed.
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
        /// The last refresh's failure, when it came after that read.
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
    failures: HashMap<(String, String), (Error, Instant)>,
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
    /// draw may have changed.
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
        state.resolutions = answer.resolutions.into_iter().collect();
        for batch in answer.batches {
            match batch.result {
                Ok(branches) => {
                    state
                        .failures
                        .remove(&(batch.host.clone(), batch.project.clone()));
                    for (branch, mr) in branches {
                        state.found.insert(
                            (batch.host.clone(), batch.project.clone(), branch),
                            (mr, now),
                        );
                    }
                }
                Err(why) => {
                    state
                        .failures
                        .insert((batch.host, batch.project), (why, now));
                }
            }
        }
        true
    }

    /// What a worktree's row draws.
    pub fn shown(&self, worktree: &Path) -> Shown<'_> {
        if !self.enabled {
            return Shown::Nothing;
        }
        for state in self.repos.values().filter(|state| !state.hidden) {
            let Some(candidate) = state.candidates.iter().find(|c| c.worktree == worktree) else {
                continue;
            };
            return match state.resolutions.get(candidate) {
                Some(Resolution::Asked { host, project }) => {
                    let key = (host.clone(), project.clone(), candidate.branch.clone());
                    let failure = state.failures.get(&(host.clone(), project.clone()));
                    match (state.found.get(&key), failure) {
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
                Some(Resolution::Why(why)) => Shown::Why(why.clone()),
                _ => Shown::Nothing,
            };
        }
        Shown::Nothing
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
