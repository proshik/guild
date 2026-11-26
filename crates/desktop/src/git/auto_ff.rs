//! Auto fast-forward of the default branch — decision 21.
//!
//! The default branch is the one new worktrees start from
//! ([`super::main_branch`]), and nothing else is touched. A plan is a read;
//! a run is the branch update's bounded fetch refusing a forced upstream, then one
//! write that only a fast-forward survives. No window here: the window's
//! thread for a run and a root's git thread for a checkout call these, and
//! the tests call them against real repositories.

use super::{GitError, Said, Write, run, write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Why a run left the branch alone. Language-neutral: the window words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reason {
    /// No configured upstream, or one that is not a remote's branch.
    NoUpstream,
    /// The upstream's remote-tracking ref is a symbolic ref: a fetch into it
    /// would write the ref it points at instead.
    SymbolicTracking,
    /// The default branch itself is a symbolic ref: a write to it would
    /// move the branch it points at.
    SymbolicBranch,
    /// The fetch failed, timed out, or left no tracking ref to read.
    FetchFailed,
    /// The branch has commits its upstream lacks — ahead or diverged.
    Diverged,
    /// Index, worktree or untracked changes in the checkout.
    Dirty,
    /// A merge, rebase, cherry-pick, revert or sequence is under way.
    OperationInProgress,
    /// A Guild tab of the checkout holds unsaved changes.
    Unsaved,
    /// The branch or its commit moved after the plan was read.
    BranchChanged,
    /// git lists the branch in a worktree whose folder is gone.
    MissingWorktree,
    /// git refused the fast-forward, or did something else than one.
    Refused,
    /// The checkout is, or holds, a project group's folder, where nothing
    /// of git is written — GLD-055.
    GroupFolder,
}

/// Where the default branch is checked out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Checkout {
    Nowhere,
    At(PathBuf),
}

/// What a run means to do, read before it fetches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The default branch, without `refs/heads/`.
    pub branch: String,
    /// Its commit when the plan was read.
    pub local: String,
    /// The remote its upstream is on.
    pub remote: String,
    /// The upstream's name on the remote, `refs/heads/…`.
    pub source: String,
    /// The upstream's remote-tracking ref, `refs/remotes/…`.
    pub tracking: String,
    pub checkout: Checkout,
}

fn text(root: &Path, args: &[&str]) -> Result<String, GitError> {
    Ok(String::from_utf8_lossy(&run(root, args)?).trim().into())
}

fn commit_of(root: &Path, reference: &str) -> Result<String, GitError> {
    text(
        root,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("{reference}^{{commit}}"),
        ],
    )
}

impl Plan {
    /// `Ok(None)` when there is nothing to do and nothing to say: no
    /// default branch, or git would not answer at all.
    pub fn read(root: &Path) -> Result<Option<Self>, Reason> {
        let Some(branch) = super::main_branch(root) else {
            return Ok(None);
        };
        if branch.starts_with('-') {
            return Ok(None);
        }
        let reference = format!("refs/heads/{branch}");
        // A write to a symbolic branch moves the branch it points at, and
        // every read after it follows the same link.
        if run(root, &["symbolic-ref", "--quiet", &reference]).is_ok() {
            return Err(Reason::SymbolicBranch);
        }
        let Ok(row) = text(
            root,
            &[
                "for-each-ref",
                "--format=%(objectname)%00%(upstream:remotename)%00%(upstream:remoteref)%00%(upstream)",
                &reference,
            ],
        ) else {
            return Ok(None);
        };
        let fields: Vec<&str> = row.split('\0').collect();
        let [local, remote, source, tracking] = fields[..] else {
            return Ok(None);
        };
        if remote.is_empty()
            || remote == "."
            || !source.starts_with("refs/heads/")
            || !tracking.starts_with("refs/remotes/")
        {
            return Err(Reason::NoUpstream);
        }
        // `--refmap=` stops other mappings, not dereferencing: a fetch into a
        // symbolic tracking ref writes the ref it points at.
        if run(root, &["symbolic-ref", "--quiet", tracking]).is_ok() {
            return Err(Reason::SymbolicTracking);
        }
        let Ok(worktrees) = super::worktrees::list(root) else {
            return Ok(None);
        };
        let checkout = match worktrees
            .iter()
            .find(|tree| tree.branch.as_deref() == Some(branch.as_str()))
        {
            Some(tree) if tree.prunable => return Err(Reason::MissingWorktree),
            Some(tree) => Checkout::At(tree.path.clone()),
            None => Checkout::Nowhere,
        };
        Ok(Some(Self {
            branch,
            local: local.into(),
            remote: remote.into(),
            source: source.into(),
            tracking: tracking.into(),
            checkout,
        }))
    }
}

/// What to do once the upstream is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    UpToDate,
    /// Move the branch to this commit.
    FastForward(String),
    Skip(Reason),
}

/// The decision, pure: `ahead` is how many commits the branch has that
/// its upstream lacks, and any is a reason to leave it alone.
pub fn decide(local: &str, upstream: &str, ahead: u64) -> Step {
    if local == upstream {
        Step::UpToDate
    } else if ahead == 0 {
        Step::FastForward(upstream.into())
    } else {
        Step::Skip(Reason::Diverged)
    }
}

/// How many commits `local` has that `upstream` lacks. A count rather than
/// `merge-base --is-ancestor`, whose "no" and whose failure are both a
/// non-zero exit.
fn ahead(root: &Path, local: &str, upstream: &str) -> Result<u64, GitError> {
    text(
        root,
        &["rev-list", "--count", &format!("{upstream}..{local}")],
    )?
    .parse()
    .map_err(|_| GitError::Failed("rev-list answered no count".into()))
}

/// A checkout with the default branch to fast-forward, handed back to the
/// window: only it knows which roots are open in that checkout *now* —
/// one opened during the fetch has buffers and a git thread of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handoff {
    /// The checkout, as git lists it.
    pub worktree: PathBuf,
    pub branch: String,
    /// The commit the branch must still be at.
    pub expected: String,
    /// The commit to fast-forward to.
    pub oid: String,
}

/// How a run, or a checkout's fast-forward, ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landed {
    /// No default branch: nothing was run.
    NoPlan,
    UpToDate,
    FastForwarded,
    Skipped(Reason),
    /// Cancelled before a write; not a reason, and nothing to show.
    Cancelled,
    Handoff(Handoff),
}

/// One run, on the caller's thread, up to the write: the plan, the fetch,
/// the decision, and the ref write when the branch is checked out nowhere.
/// A checked-out branch is handed back. Every write happens in `root`;
/// `cancelled` is read before each; `report` hears each with what git said.
pub fn run_once(
    root: &Path,
    cancelled: &AtomicBool,
    timeout: Duration,
    mut report: impl FnMut(&Write, &Result<Said, GitError>),
) -> Landed {
    let stop = || cancelled.load(Ordering::Acquire);
    if stop() {
        return Landed::Cancelled;
    }
    let plan = match Plan::read(root) {
        Ok(Some(plan)) => plan,
        Ok(None) => return Landed::NoPlan,
        Err(reason) => return Landed::Skipped(reason),
    };
    if stop() {
        return Landed::Cancelled;
    }
    let fetch = Write::FetchSource {
        remote: plan.remote.clone(),
        refspec: format!("{}:{}", plan.source, plan.tracking),
        refuse_forced: true,
    };
    let fetched = super::branch_update::write_fetch(root, &fetch, timeout);
    report(&fetch, &fetched);
    if fetched.is_err() {
        return Landed::Skipped(Reason::FetchFailed);
    }
    let Ok(upstream) = commit_of(root, &plan.tracking) else {
        return Landed::Skipped(Reason::FetchFailed);
    };
    let Ok(ahead) = ahead(root, &plan.local, &upstream) else {
        return Landed::Skipped(Reason::Refused);
    };
    let oid = match decide(&plan.local, &upstream, ahead) {
        Step::UpToDate => return Landed::UpToDate,
        Step::Skip(reason) => return Landed::Skipped(reason),
        Step::FastForward(oid) => oid,
    };
    match &plan.checkout {
        Checkout::Nowhere => {
            // git refuses a non-fast-forward itself; this catches a commit
            // made during the fetch before git has to.
            if commit_of(root, &format!("refs/heads/{}", plan.branch)).ok() != Some(plan.local) {
                return Landed::Skipped(Reason::BranchChanged);
            }
            // Read last, immediately before the write: the read above can
            // wait on the read budget while the setting is turned off.
            if stop() {
                return Landed::Cancelled;
            }
            let moved = Write::FastForwardBranch {
                branch: plan.branch.clone(),
                oid: oid.clone(),
            };
            let result = write(root, &moved);
            report(&moved, &result);
            match result {
                Ok(_)
                    if commit_of(root, &format!("refs/heads/{}", plan.branch)).ok()
                        == Some(oid) =>
                {
                    Landed::FastForwarded
                }
                _ => Landed::Skipped(Reason::Refused),
            }
        }
        Checkout::At(worktree) => Landed::Handoff(Handoff {
            worktree: worktree.clone(),
            branch: plan.branch,
            expected: plan.local,
            oid,
        }),
    }
}

/// Whether the checkout may be fast-forwarded now: still on `branch` at
/// `expected`, nothing under way, nothing changed, and `oid` still ahead.
pub fn preflight(worktree: &Path, branch: &str, expected: &str, oid: &str) -> Result<(), Reason> {
    let on = text(worktree, &["symbolic-ref", "--quiet", "HEAD"]).ok();
    let at = commit_of(worktree, "HEAD").ok();
    if on.as_deref() != Some(format!("refs/heads/{branch}").as_str())
        || at.as_deref() != Some(expected)
    {
        return Err(Reason::BranchChanged);
    }
    match super::branch_update::operation_in_progress(worktree) {
        Ok(false) => {}
        Ok(true) => return Err(Reason::OperationInProgress),
        Err(_) => return Err(Reason::Refused),
    }
    match run(
        worktree,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    ) {
        Ok(changes) if changes.is_empty() => {}
        Ok(_) => return Err(Reason::Dirty),
        Err(_) => return Err(Reason::Refused),
    }
    match ahead(worktree, expected, oid) {
        Ok(0) => Ok(()),
        Ok(_) => Err(Reason::Diverged),
        Err(_) => Err(Reason::Refused),
    }
}

/// Fast-forward a checkout: preflight, then the buffers, then the
/// cancellation, each immediately before the write; afterwards HEAD must
/// be at `oid` on a clean checkout, or it is reported as refused — what a
/// config nobody measured did cannot be walked back here, only said.
pub fn fast_forward_checkout(
    worktree: &Path,
    branch: &str,
    expected: &str,
    oid: &str,
    mut buffers_clean: impl FnMut() -> bool,
    cancelled: &AtomicBool,
    mut report: impl FnMut(&Write, &Result<Said, GitError>),
) -> Landed {
    // The checkout's turn from the preflight to the post-check: no other
    // write of Guild's in it can land between them. Without it, no write.
    let Some(_turn) = super::turn(worktree) else {
        return Landed::Skipped(Reason::Refused);
    };
    if let Err(reason) = preflight(worktree, branch, expected, oid) {
        return Landed::Skipped(reason);
    }
    if !buffers_clean() {
        return Landed::Skipped(Reason::Unsaved);
    }
    if cancelled.load(Ordering::Acquire) {
        return Landed::Cancelled;
    }
    let merge = Write::FastForwardCheckout {
        branch: branch.into(),
        oid: oid.into(),
    };
    let result = super::write_in_turn(worktree, &merge);
    report(&merge, &result);
    if result.is_err() {
        return Landed::Skipped(Reason::Refused);
    }
    let landed = commit_of(worktree, "HEAD").ok().as_deref() == Some(oid)
        && text(worktree, &["symbolic-ref", "--quiet", "HEAD"]).ok()
            == Some(format!("refs/heads/{branch}"))
        && run(
            worktree,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )
        .is_ok_and(|changes| changes.is_empty());
    if landed {
        Landed::FastForwarded
    } else {
        Landed::Skipped(Reason::Refused)
    }
}
