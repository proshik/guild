//! Root-owned refresh epochs; existing Git workers execute the shared reads.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

type List = Result<(PathBuf, Vec<git::worktrees::Worktree>), GitError>;
pub(super) type ReadReply = (Counted, Option<List>);
struct Subscriber {
    worker: u64,
    common: Option<PathBuf>,
    identity: Option<mpsc::Receiver<Result<PathBuf, GitError>>>,
    requested: Option<bool>,
}
struct Active {
    representative: ProjectId,
    epoch: u64,
    reply: mpsc::Receiver<ReadReply>,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct Repository {
    failed: std::collections::HashSet<ProjectId>,
    preferred: Option<ProjectId>,
    desired: u64,
    pending: bool,
    full: bool,
    counted: Counted,
    active: Option<Active>,
}
impl Drop for Repository {
    fn drop(&mut self) {
        if let Some(active) = &self.active {
            active.cancelled.store(true, Ordering::Release);
        }
    }
}
#[derive(Default)]
pub(crate) struct Coordinator {
    subscribers: HashMap<ProjectId, Subscriber>,
    repositories: HashMap<PathBuf, Repository>,
    epoch: u64,
}
impl Coordinator {
    pub fn request(&mut self, workers: &StatusWorkers, full: impl Fn(ProjectId) -> bool) {
        self.sync(workers);
        for (id, subscriber) in &mut self.subscribers {
            subscriber.requested = Some(subscriber.requested.unwrap_or(false) || full(*id));
        }
    }
    pub fn request_project(&mut self, workers: &StatusWorkers, project: ProjectId) {
        self.sync(workers);
        if let Some(subscriber) = self.subscribers.get_mut(&project) {
            subscriber.requested = Some(true);
        }
    }
    fn sync(&mut self, workers: &StatusWorkers) {
        self.subscribers.retain(|id, _| workers.contains_key(id));
        for (id, worker) in workers {
            if self
                .subscribers
                .get(id)
                .is_none_or(|s| s.worker != worker.serial)
            {
                self.subscribers.insert(
                    *id,
                    Subscriber {
                        worker: worker.serial,
                        common: None,
                        identity: None,
                        requested: Some(true),
                    },
                );
            }
        }
        self.repositories.retain(|common, _| {
            self.subscribers
                .values()
                .any(|s| s.common.as_ref() == Some(common))
        });
    }
    pub fn poll(&mut self, workers: &StatusWorkers) -> Vec<(ProjectId, List)> {
        self.sync(workers);
        let mut answers = Vec::new();
        // Identity IO and canonicalization happen on the existing project thread.
        for (id, s) in &mut self.subscribers {
            if s.common.is_none() && s.identity.is_none() && s.requested.is_some() {
                let (tx, rx) = mpsc::channel();
                let _ = workers[id].asks.send(Ask::Common(tx));
                s.identity = Some(rx);
            }
            if let Some(rx) = &s.identity {
                match rx.try_recv() {
                    Ok(Ok(common)) => {
                        s.common = Some(common);
                        s.identity = None;
                    }
                    Ok(Err(error)) => {
                        answers.push((*id, Err(error)));
                        s.identity = None;
                        s.requested = None;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        s.identity = None;
                        s.requested = None;
                        answers.push((
                            *id,
                            Err(GitError::Failed("Git identity worker stopped".into())),
                        ));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
        }
        self.epoch = self.epoch.wrapping_add(1);
        for s in self.subscribers.values_mut() {
            if let Some(common) = &s.common
                && let Some(full) = s.requested.take()
            {
                let repo = self.repositories.entry(common.clone()).or_default();
                // A cadence tick requests another observation, not revocation of
                // stationary work. Forced freshness alone supersedes an active read.
                if full || repo.active.is_none() {
                    repo.failed.clear();
                    repo.desired = self.epoch;
                    if let Some(active) = &repo.active {
                        active.cancelled.store(true, Ordering::Release);
                    }
                }
                repo.pending = true;
                repo.full |= full;
            }
        }
        for (common, repo) in &mut self.repositories {
            if let Some(active) = &repo.active {
                let reply = match active.reply.try_recv() {
                    Ok(reply) => Some(reply),
                    Err(mpsc::TryRecvError::Disconnected) => Some((
                        Counted::default(),
                        Some(Err(GitError::Failed("Git list worker stopped".into()))),
                    )),
                    Err(mpsc::TryRecvError::Empty) => None,
                };
                if let Some((counted, result)) = reply {
                    if active.epoch == repo.desired {
                        let representative = active.representative;
                        if result.as_ref().is_some_and(Result::is_err) {
                            repo.failed.insert(representative);
                            repo.pending = true;
                        } else {
                            repo.preferred = Some(representative);
                            repo.full = false;
                        }
                        repo.counted = counted;
                        if let Some(result) = result {
                            for (id, s) in &self.subscribers {
                                if s.common.as_ref() == Some(common) {
                                    answers.push((*id, result.clone()));
                                }
                            }
                        }
                    }
                    repo.active = None;
                }
            }
            if repo.active.is_none() && repo.pending {
                let eligible = |id: &ProjectId, s: &Subscriber| {
                    s.common.as_ref() == Some(common) && !repo.failed.contains(id)
                };
                let representative = repo
                    .preferred
                    .filter(|id| self.subscribers.get(id).is_some_and(|s| eligible(id, s)))
                    .or_else(|| {
                        self.subscribers
                            .iter()
                            .find(|(id, s)| eligible(id, s))
                            .map(|(id, _)| *id)
                    });
                let Some(id) = representative else {
                    repo.pending = false;
                    continue;
                };
                let (tx, rx) = mpsc::channel();
                let cancelled = Arc::new(AtomicBool::new(false));
                let mut counted = std::mem::take(&mut repo.counted);
                counted.common = Some(common.clone());
                let _ = workers[&id].asks.send(Ask::SharedWorktrees {
                    counted,
                    full: repo.full,
                    cancelled: cancelled.clone(),
                    reply: tx,
                });
                repo.active = Some(Active {
                    representative: id,
                    epoch: repo.desired,
                    reply: rx,
                    cancelled,
                });
                repo.pending = false;
            }
        }
        answers
    }
    #[cfg(test)]
    pub fn retained(&self) -> (usize, usize) {
        (self.subscribers.len(), self.repositories.len())
    }
    #[cfg(test)]
    pub fn idle(&self) -> bool {
        self.subscribers
            .values()
            .all(|s| s.identity.is_none() && s.requested.is_none())
            && self
                .repositories
                .values()
                .all(|r| r.active.is_none() && !r.pending)
    }
}
