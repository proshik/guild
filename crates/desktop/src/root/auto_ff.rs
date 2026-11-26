//! Auto fast-forward, the window's half — decision 21.
//!
//! **A repository is the unit**, not a root: two opened worktrees of one
//! repository share its default branch, and two runs would fetch the same
//! ref and race for its lock. [`Schedule`] says which repositories are due,
//! keeps the one run of each and the skip to show, and is pure apart from
//! the channels it holds, so the clock is a parameter and the tests need no
//! window. [`Root::poll_auto_ff`] is the glue, on `poll_git`'s clock — the
//! watch loop's, painted or not, never `render`'s (traps.md).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gpui::Context;
use guild_session::ProjectId;

use crate::Root;
use crate::git::auto_ff::{Landed, Reason};

/// How long after a run lands the next one starts.
pub(crate) const EVERY: Duration = Duration::from_secs(5 * 60);

/// Runs in flight across the window. A launch that restores twenty
/// repositories would otherwise start twenty fetches at once, and every
/// project's git already shares one descriptor limit (traps.md).
pub(crate) const AT_ONCE: usize = 2;

/// A root that may belong to a checkout has not been read by its git
/// thread yet.
#[derive(Debug)]
pub(crate) struct Undecided;

/// What the run's thread sends back once.
pub(crate) struct Finished {
    /// The default branch, for the skip line; `None` when there is none.
    pub branch: Option<String>,
    pub landed: Landed,
    /// The Console's lines for the writes it ran, in order.
    pub ran: Vec<crate::console::Line>,
}

/// What a run in flight is waiting on.
pub(crate) enum Waiting {
    /// A thread of its own: the plan, the fetch and a ref write — or, with
    /// `checkout`, the fast-forward of a checkout no root had open when it
    /// was handed back, cancelled if one opens there before the write.
    Thread {
        receiver: mpsc::Receiver<Finished>,
        checkout: Option<PathBuf>,
    },
    /// A root's git thread, with the hand-off's checkout. `clean` is the
    /// unsaved-buffer flag the window refreshes while it waits; `done` is
    /// the ask's own answer, which outlives the project and its worker, so
    /// a root closed meanwhile cannot leave the run waiting for ever.
    Worker {
        project: ProjectId,
        worktree: PathBuf,
        branch: String,
        clean: Arc<AtomicBool>,
        done: mpsc::Receiver<(Landed, Vec<crate::console::Line>)>,
    },
}

/// A run in flight. It keeps its repository's slot until it lands, even
/// when the repository is closed under it or the setting turned off —
/// it is cancelled then, and only refuses to write.
pub(crate) struct Running {
    pub cancelled: Arc<AtomicBool>,
    pub waiting: Waiting,
    /// Where its Console lines are filed.
    pub console: ProjectId,
}

#[derive(Default)]
struct Repository {
    /// When the next run may start; `None` while no root of it is open.
    due: Option<Instant>,
    running: Option<Running>,
    /// The default branch and why the last run left it alone.
    skipped: Option<(String, Reason)>,
}

/// Every repository the window has a root of, or a run still in flight for.
#[derive(Default)]
pub(crate) struct Schedule {
    repositories: HashMap<PathBuf, Repository>,
    /// Manual Update branch runs by repository, each until its ask is gone
    /// — not until its root is: closing the root does not stop the fetch.
    manual: Vec<(PathBuf, Arc<AtomicBool>)>,
}

impl Schedule {
    /// Bring the set in line with the repositories open now. A newly seen
    /// one is due at once; one no longer open stops being scheduled, and
    /// its run, if any, is cancelled and kept until it lands.
    pub fn reconcile(&mut self, open: &[PathBuf], now: Instant) {
        self.repositories.retain(|common, repository| {
            if open.contains(common) {
                return true;
            }
            repository.due = None;
            repository.skipped = None;
            match &repository.running {
                Some(running) => {
                    running.cancelled.store(true, Ordering::Release);
                    true
                }
                None => false,
            }
        });
        for common in open {
            let repository = self.repositories.entry(common.clone()).or_default();
            if repository.due.is_none() {
                repository.due = Some(now);
            }
        }
    }

    /// The repositories to start now, in `open`'s order: due, scheduled,
    /// not running, not `busy` with a manual Update branch, within the cap.
    pub fn starts(
        &self,
        open: &[PathBuf],
        busy: impl Fn(&Path) -> bool,
        now: Instant,
    ) -> Vec<PathBuf> {
        let mut room = AT_ONCE.saturating_sub(self.in_flight());
        let mut starts = Vec::new();
        for common in open {
            if room == 0 {
                break;
            }
            let Some(repository) = self.repositories.get(common) else {
                continue;
            };
            if repository.running.is_none()
                && repository.due.is_some_and(|due| due <= now)
                && !busy(common)
            {
                starts.push(common.clone());
                room -= 1;
            }
        }
        starts
    }

    /// A manual Update branch of `common` was sent.
    pub fn manual_started(&mut self, common: PathBuf, flag: Arc<AtomicBool>) {
        self.manual.push((common, flag));
    }

    /// Whether a manual Update branch of `common` is still in flight.
    pub fn manual_running(&mut self, common: &Path) -> bool {
        self.manual.retain(|(_, flag)| flag.load(Ordering::Acquire));
        self.manual.iter().any(|(seen, _)| seen == common)
    }

    pub fn in_flight(&self) -> usize {
        self.repositories
            .values()
            .filter(|repository| repository.running.is_some())
            .count()
    }

    /// Whether a run of this repository is in flight — the manual update's
    /// gate, cancelled runs included.
    pub fn running(&self, common: &Path) -> bool {
        self.repositories
            .get(common)
            .is_some_and(|repository| repository.running.is_some())
    }

    pub fn started(&mut self, common: &Path, running: Running) {
        if let Some(repository) = self.repositories.get_mut(common) {
            repository.running = Some(running);
        }
    }

    /// Whether a run is waiting on a root's git thread — for the tests that
    /// act between the hand-off and its answer.
    #[cfg(test)]
    pub fn waiting_on_worker(&self) -> bool {
        self.repositories.values().any(|repository| {
            matches!(
                repository.running.as_ref().map(|running| &running.waiting),
                Some(Waiting::Worker { .. })
            )
        })
    }

    pub fn running_mut(&mut self, common: &Path) -> Option<&mut Running> {
        self.repositories.get_mut(common)?.running.as_mut()
    }

    /// The repositories with a run in flight.
    pub fn in_flight_repositories(&self) -> Vec<PathBuf> {
        self.repositories
            .iter()
            .filter(|(_, repository)| repository.running.is_some())
            .map(|(common, _)| common.clone())
            .collect()
    }

    /// A run landed: its slot is free, the next is due in [`EVERY`] — at
    /// once if it was cancelled under a repository open again — and the
    /// skip is kept or cleared. A repository closed meanwhile goes.
    pub fn landed(&mut self, common: &Path, branch: Option<String>, landed: &Landed, now: Instant) {
        let Some(repository) = self.repositories.get_mut(common) else {
            return;
        };
        repository.running = None;
        if repository.due.is_none() {
            self.repositories.remove(common);
            return;
        }
        repository.due = Some(match landed {
            Landed::Cancelled => now,
            _ => now + EVERY,
        });
        match (landed, branch) {
            (Landed::Skipped(reason), Some(branch)) => {
                repository.skipped = Some((branch, *reason));
            }
            (Landed::Skipped(_) | Landed::Cancelled | Landed::Handoff(_), _) => {}
            (Landed::NoPlan | Landed::UpToDate | Landed::FastForwarded, _) => {
                repository.skipped = None;
            }
        }
    }

    /// A root closed: a hand-off waiting on its git thread must not write.
    pub fn cancel_waiting_on(&self, project: ProjectId) {
        for repository in self.repositories.values() {
            if let Some(running) = &repository.running
                && matches!(&running.waiting, Waiting::Worker { project: waiting, .. } if *waiting == project)
            {
                running.cancelled.store(true, Ordering::Release);
            }
        }
    }

    /// The setting turned off: every run in flight is cancelled.
    pub fn cancel_all(&self) {
        for repository in self.repositories.values() {
            if let Some(running) = &repository.running {
                running.cancelled.store(true, Ordering::Release);
            }
        }
    }

    /// The setting turned on: every open repository is due now.
    pub fn all_due(&mut self, now: Instant) {
        for repository in self.repositories.values_mut() {
            if repository.due.is_some() {
                repository.due = Some(now);
            }
        }
    }

    /// The default branch and why it was last left alone.
    pub fn skipped(&self, common: &Path) -> Option<&(String, Reason)> {
        self.repositories.get(common)?.skipped.as_ref()
    }
}

/// Where a skip is shown — decision 21: Update branch's source list shows it
/// for every root of the repository, its hover hint only on a root that has
/// the default branch out, where it is about the branch the button updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Surface {
    Hint,
    Sources,
}

/// The skip line for a root whose `head` is its symbolic `HEAD`, if any.
pub(crate) fn skip_line(
    skipped: Option<&(String, Reason)>,
    head: Option<&str>,
    surface: Surface,
    ui: crate::i18n::Ui,
) -> Option<String> {
    let (branch, reason) = skipped?;
    if surface == Surface::Hint && head != Some(format!("refs/heads/{branch}").as_str()) {
        return None;
    }
    Some(ui.auto_ff_skipped(branch, *reason))
}

/// Run one repository's plan, fetch and ref write on a thread of its own:
/// a black-holed remote would otherwise freeze a root's status walks for
/// the fetch's whole deadline, every five minutes.
fn spawn(root: PathBuf, cancelled: Arc<AtomicBool>) -> mpsc::Receiver<Finished> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("guild-auto-ff".into())
        .spawn(move || {
            let shown = root.to_string_lossy().into_owned();
            let branch = crate::git::main_branch(&root);
            let mut ran = Vec::new();
            let landed = crate::git::auto_ff::run_once(
                &root,
                &cancelled,
                crate::git::branch_update::FETCH_DEADLINE,
                |write, result| ran.push(line(&shown, write, result)),
            );
            let _ = sender.send(Finished {
                branch,
                landed,
                ran,
            });
        });
    // A spawn that failed dropped the sender, which `poll_auto_ff` reads
    // as a run that landed refused.
    if let Err(err) = spawned {
        tracing::warn!(%err, "the auto fast-forward thread could not be started");
    }
    receiver
}

/// Fast-forward a checkout no root has open, on a thread of its own; its
/// Console lines name the checkout, which is where git ran.
fn spawn_checkout(
    handoff: crate::git::auto_ff::Handoff,
    cancelled: Arc<AtomicBool>,
) -> mpsc::Receiver<Finished> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("guild-auto-ff".into())
        .spawn(move || {
            let shown = handoff.worktree.to_string_lossy().into_owned();
            let mut ran = Vec::new();
            let landed = crate::git::auto_ff::fast_forward_checkout(
                &handoff.worktree,
                &handoff.branch,
                &handoff.expected,
                &handoff.oid,
                || true,
                &cancelled,
                |write, result| ran.push(line(&shown, write, result)),
            );
            let _ = sender.send(Finished {
                branch: Some(handoff.branch),
                landed,
                ran,
            });
        });
    if let Err(err) = spawned {
        tracing::warn!(%err, "the auto fast-forward thread could not be started");
    }
    receiver
}

fn line(
    root: &str,
    write: &crate::git::Write,
    result: &Result<crate::git::Said, crate::git::GitError>,
) -> crate::console::Line {
    crate::console::Line {
        at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_millis() as i64),
        root: root.to_owned(),
        argv: write.args(),
        said: match result {
            Ok(said) => said.text(),
            Err(error) => error.to_string(),
        },
        failed: result.is_err(),
    }
}

impl Root {
    /// The repositories open now, in the desk's order, each with its
    /// opened roots in that order.
    fn open_repositories(&self) -> Vec<(PathBuf, Vec<(ProjectId, PathBuf)>)> {
        let mut open: Vec<(PathBuf, Vec<(ProjectId, PathBuf)>)> = Vec::new();
        for project in self.desk.projects() {
            // A project group's folder: nothing of git but its branch label
            // runs for it — GLD-055 decision 7.
            if self.desk.is_project_group_root(project.id) {
                continue;
            }
            let Some(common) = &project.repository else {
                continue;
            };
            match open.iter_mut().find(|(seen, _)| seen == common) {
                Some((_, roots)) => roots.push((project.id, project.path.clone())),
                None => open.push((common.clone(), vec![(project.id, project.path.clone())])),
            }
        }
        open
    }

    /// The repository a project is a root of.
    pub(crate) fn repository_of(&self, project: ProjectId) -> Option<PathBuf> {
        self.desk.project(project)?.repository.clone()
    }

    /// The opened roots of a checkout, in the desk's order: a root may be
    /// the checkout or a folder in it, but a checkout nested in its folder
    /// is not it — so a root is matched by the checkout its own git thread
    /// read (`Snapshot::checkout`), never by its path. `worktree` is git's,
    /// canonical. `Err` while a root that could be one has not been read
    /// yet: the caller does not decide on a guess.
    fn opened_in(&self, worktree: &Path) -> Result<Vec<ProjectId>, Undecided> {
        let mut roots = Vec::new();
        for project in self.desk.projects() {
            let read = self
                .update_sources
                .get(&project.id)
                .and_then(|snapshot| snapshot.as_ref().ok())
                .and_then(|snapshot| snapshot.checkout.as_deref());
            match read {
                Some(checkout) if checkout == worktree => roots.push(project.id),
                Some(_) => {}
                None if project
                    .path
                    .canonicalize()
                    .unwrap_or_else(|_| project.path.clone())
                    .starts_with(worktree) =>
                {
                    return Err(Undecided);
                }
                None => {}
            }
        }
        Ok(roots)
    }

    /// Whether any of these roots holds an unsaved tab.
    fn unsaved(&self, projects: &[ProjectId], cx: &Context<Self>) -> bool {
        projects
            .iter()
            .any(|project| !self.dirty_file_tabs(Some(*project), cx).is_empty())
    }

    /// One tick of auto fast-forward, from `poll_git`: what landed, the
    /// buffer flags of hand-offs still waiting, and what starts now.
    pub(crate) fn poll_auto_ff(&mut self, now: Instant, cx: &mut Context<Self>) {
        let open = self.open_repositories();
        let commons: Vec<PathBuf> = open.iter().map(|(common, _)| common.clone()).collect();
        self.auto_ff.reconcile(&commons, now);

        for common in self.auto_ff.in_flight_repositories() {
            let Some(running) = self.auto_ff.running_mut(&common) else {
                continue;
            };
            let console = running.console;
            let mut filed = console;
            let cancelled = running.cancelled.clone();
            let finished = match &running.waiting {
                Waiting::Thread { receiver, checkout } => {
                    let answer = receiver.try_recv();
                    // A root opened in the checkout since the hand-off has
                    // buffers this write would not ask: let it go to that
                    // root's thread on the next run instead.
                    if let Some(checkout) = checkout.clone()
                        && self
                            .opened_in(&checkout)
                            .map_or(true, |roots| !roots.is_empty())
                    {
                        cancelled.store(true, Ordering::Release);
                    }
                    match answer {
                        Ok(finished) => finished,
                        Err(mpsc::TryRecvError::Empty) => continue,
                        Err(mpsc::TryRecvError::Disconnected) => Finished {
                            branch: None,
                            landed: Landed::Skipped(Reason::Refused),
                            ran: Vec::new(),
                        },
                    }
                }
                Waiting::Worker {
                    project,
                    worktree,
                    branch,
                    clean,
                    done,
                } => {
                    let answer = done.try_recv();
                    let (project, worktree, branch, clean) =
                        (*project, worktree.clone(), branch.clone(), clean.clone());
                    if self.status_workers.contains_key(&project) {
                        let inside = self.opened_in(&worktree).unwrap_or_default();
                        let unsaved = self.unsaved(&inside, cx)
                            || self.unsaved(std::slice::from_ref(&project), cx);
                        clean.store(!unsaved, Ordering::Release);
                    } else {
                        // Its root closed: an ask not yet started must not
                        // write. One already writing still answers on `done`.
                        cancelled.store(true, Ordering::Release);
                    }
                    // Its lines are filed under its own root while that is
                    // open, and under the run's once it has closed.
                    if self.desk.project(project).is_some() {
                        filed = project;
                    }
                    match answer {
                        Ok((landed, ran)) => Finished {
                            branch: Some(branch),
                            landed,
                            ran,
                        },
                        Err(mpsc::TryRecvError::Empty) => continue,
                        // The thread dropped the ask unrun: nothing written.
                        Err(mpsc::TryRecvError::Disconnected) => Finished {
                            branch: Some(branch),
                            landed: Landed::Cancelled,
                            ran: Vec::new(),
                        },
                    }
                }
            };
            for line in finished.ran {
                self.consoles.entry(filed).or_default().push(line);
                if self.active_project == Some(filed) {
                    self.show_console_end();
                }
            }
            match finished.landed {
                Landed::Handoff(handoff) => {
                    self.hand_off(&common, finished.branch, handoff, now, cx)
                }
                landed => {
                    self.auto_ff.landed(&common, finished.branch, &landed, now);
                    self.release_held_updates(&common);
                }
            }
            cx.notify();
        }

        if !self.settings.auto_fast_forward_default_branch {
            return;
        }
        let manual: Vec<PathBuf> = commons
            .iter()
            .filter(|common| self.auto_ff.manual_running(common))
            .cloned()
            .collect();
        let busy = |common: &Path| {
            manual.iter().any(|seen| seen == common)
                || open
                    .iter()
                    .find(|(seen, _)| seen == common)
                    .is_some_and(|(_, roots)| {
                        roots
                            .iter()
                            .any(|(project, _)| self.updating_branches.contains_key(project))
                    })
        };
        for common in self.auto_ff.starts(&commons, busy, now) {
            let Some((_, roots)) = open.iter().find(|(seen, _)| *seen == common) else {
                continue;
            };
            let (console, root) = roots[0].clone();
            let cancelled = Arc::new(AtomicBool::new(false));
            let receiver = spawn(root, cancelled.clone());
            self.auto_ff.started(
                &common,
                Running {
                    cancelled,
                    waiting: Waiting::Thread {
                        receiver,
                        checkout: None,
                    },
                    console,
                },
            );
        }
    }

    /// The run found the default branch checked out. Which roots are open
    /// in that checkout is decided now, not when the run started: an open
    /// one's git thread does the write after its buffers are asked; with
    /// none, a thread of the run's writes it.
    fn hand_off(
        &mut self,
        common: &Path,
        branch: Option<String>,
        handoff: crate::git::auto_ff::Handoff,
        now: Instant,
        cx: &mut Context<Self>,
    ) {
        let Some(cancelled) = self
            .auto_ff
            .running_mut(common)
            .map(|running| running.cancelled.clone())
        else {
            return;
        };
        if cancelled.load(Ordering::Acquire) || !self.settings.auto_fast_forward_default_branch {
            self.auto_ff.landed(common, branch, &Landed::Cancelled, now);
            self.release_held_updates(common);
            return;
        }
        // A checkout that is, or holds, a project group's folder — open or
        // closed — is never written: nothing of git runs there (GLD-055).
        let holds_folder = self.desk.workspaces().iter().any(|holder| {
            holder
                .project_groups
                .iter()
                .filter_map(|group| group.folder.as_deref())
                .any(|folder| folder.starts_with(&handoff.worktree))
        });
        if holds_folder {
            self.auto_ff
                .landed(common, branch, &Landed::Skipped(Reason::GroupFolder), now);
            self.release_held_updates(common);
            return;
        }
        // A root that may be this checkout's has not been read yet: run
        // again rather than guess. A cancelled run is due again at once.
        let Ok(inside) = self.opened_in(&handoff.worktree) else {
            self.auto_ff.landed(common, branch, &Landed::Cancelled, now);
            self.release_held_updates(common);
            return;
        };
        let Some(&project) = inside.first() else {
            let checkout = handoff.worktree.clone();
            let receiver = spawn_checkout(handoff, cancelled);
            if let Some(running) = self.auto_ff.running_mut(common) {
                running.waiting = Waiting::Thread {
                    receiver,
                    checkout: Some(checkout),
                };
            }
            return;
        };
        if self.unsaved(&inside, cx) {
            self.auto_ff
                .landed(common, branch, &Landed::Skipped(Reason::Unsaved), now);
            self.release_held_updates(common);
            return;
        }
        let clean = Arc::new(AtomicBool::new(true));
        let worktree = handoff.worktree.clone();
        let branch_name = handoff.branch.clone();
        let (answer, done) = mpsc::channel();
        let sent = self
            .status_workers
            .get(&project)
            .is_some_and(|worker| worker.fast_forward(handoff, clean.clone(), cancelled, answer));
        if !sent {
            self.auto_ff.landed(common, branch, &Landed::Cancelled, now);
            self.release_held_updates(common);
            return;
        }
        if let Some(running) = self.auto_ff.running_mut(common) {
            running.waiting = Waiting::Worker {
                project,
                worktree,
                branch: branch_name,
                clean,
                done,
            };
        }
    }

    /// The active root's skip line for one of Update branch's surfaces.
    pub(crate) fn auto_ff_skip_line(&self, surface: Surface) -> Option<String> {
        let project = self.active_project?;
        let common = self.repository_of(project)?;
        let head = self
            .update_snapshot()
            .and_then(|snapshot| snapshot.head.as_ref())
            .map(|(head, _)| head.as_str());
        skip_line(self.auto_ff.skipped(&common), head, surface, self.ui())
    }

    /// A root left the window. Cancelled now, not on the next tick: a
    /// handed-off write could otherwise start in between. The run keeps its
    /// slot until it lands.
    pub(crate) fn auto_ff_released(&mut self, project: ProjectId) {
        let open: Vec<PathBuf> = self
            .open_repositories()
            .into_iter()
            .map(|(common, _)| common)
            .collect();
        self.auto_ff.reconcile(&open, Instant::now());
        self.auto_ff.cancel_waiting_on(project);
    }

    /// The setting was flipped in Settings → Projects.
    pub(crate) fn auto_fast_forward_toggled(&mut self) {
        if self.settings.auto_fast_forward_default_branch {
            self.auto_ff.all_due(Instant::now());
        } else {
            self.auto_ff.cancel_all();
        }
    }
}

#[cfg(test)]
mod root_tests;
#[cfg(test)]
mod tests;
