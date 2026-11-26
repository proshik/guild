//! Auto fast-forward against real git — decision 21.
//!
//! The writes first: each one is checked against the configuration that
//! was measured to defeat it, because the argv is the guard and a pinned
//! argv only says what is spelled, not what git then does.

use std::path::{Path, PathBuf};

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::scratch::Scratch;
use guild_desktop::git::auto_ff::{
    self, Checkout, Handoff, Landed, Plan, Reason, Step, decide, preflight, run_once,
};
use guild_desktop::git::{self, Write};

fn git_at(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().into()
}

fn identify(root: &Path) {
    git_at(root, &["config", "user.name", "Test"]);
    git_at(root, &["config", "user.email", "test@example.invalid"]);
}

/// A commit that changes a file, so a squash would leave something staged.
fn commit(root: &Path, name: &str) -> String {
    std::fs::write(root.join("file"), format!("{name}\n")).unwrap();
    git_at(root, &["add", "file"]);
    git_at(root, &["commit", "-q", "-m", name]);
    git_at(root, &["rev-parse", "HEAD"])
}

fn oid(root: &Path, name: &str) -> String {
    git_at(
        root,
        &["rev-parse", "--verify", &format!("{name}^{{commit}}")],
    )
}

/// An upstream with one commit, a clone of it on `main`, and the
/// upstream one commit further on — fetched into `origin/main`, so the
/// clone's `main` is behind and a fast-forward is possible.
struct Behind {
    _folder: Scratch,
    upstream: PathBuf,
    local: PathBuf,
    /// The commit the clone's `main` is at.
    old: String,
    /// The upstream's newer commit, already in `origin/main`.
    new: String,
}

fn behind() -> Behind {
    fixture(true)
}

/// The same pair with nothing new upstream: the clone is up to date.
fn in_step() -> Behind {
    fixture(false)
}

fn fixture(advance: bool) -> Behind {
    let folder = Scratch::new("guild-auto-ff-");
    let upstream = folder.join("upstream");
    let local = folder.join("local");
    std::fs::create_dir(&upstream).unwrap();
    git_at(&upstream, &["init", "-q", "-b", "main"]);
    identify(&upstream);
    let old = commit(&upstream, "base");
    git_at(
        &folder,
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            local.to_str().unwrap(),
        ],
    );
    identify(&local);
    let new = if advance {
        commit(&upstream, "upstream")
    } else {
        old.clone()
    };
    git_at(&local, &["fetch", "-q", "origin"]);
    assert_eq!(oid(&local, "origin/main"), new);
    Behind {
        _folder: folder,
        upstream,
        local,
        old,
        new,
    }
}

fn status(root: &Path) -> String {
    git_at(root, &["status", "--porcelain=v1", "--untracked-files=all"])
}

fn parents(root: &Path, rev: &str) -> usize {
    git_at(root, &["log", "-1", "--format=%P", rev])
        .split_whitespace()
        .count()
}

fn branch_ref(branch: &str, oid: &str) -> Write {
    Write::FastForwardBranch {
        branch: branch.into(),
        oid: oid.into(),
    }
}

fn checkout(branch: &str, oid: &str) -> Write {
    Write::FastForwardCheckout {
        branch: branch.into(),
        oid: oid.into(),
    }
}

#[test]
fn a_branch_checked_out_nowhere_is_moved_by_a_fast_forward() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    git::write(&repo.local, &branch_ref("main", &repo.new)).unwrap();
    assert_eq!(oid(&repo.local, "main"), repo.new);
    assert_eq!(oid(&repo.local, "HEAD"), repo.old, "no checkout moved");
    assert_eq!(status(&repo.local), "");
}

/// The review's scenario: `main` gained a commit after the plan was read,
/// and the person's config hides forced updates — which, measured, lets a
/// plain local fetch replace it. The argv turns the refusal back on.
#[test]
fn a_diverged_branch_is_refused_even_with_forced_updates_hidden() {
    let repo = behind();
    git_at(&repo.local, &["config", "fetch.showForcedUpdates", "false"]);
    let local = commit(&repo.local, "local");
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    assert!(git::write(&repo.local, &branch_ref("main", &repo.new)).is_err());
    assert_eq!(oid(&repo.local, "main"), local, "the local commit is kept");
}

#[test]
fn a_branch_checked_out_in_a_linked_worktree_is_not_moved_by_the_ref_write() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let linked = repo.local.with_file_name("linked");
    git_at(
        &repo.local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    );
    let refused = git::write(&repo.local, &branch_ref("main", &repo.new)).unwrap_err();
    assert!(refused.to_string().contains("checked out"), "{refused}");
    assert_eq!(oid(&repo.local, "main"), repo.old);
    assert_eq!(oid(&linked, "HEAD"), repo.old);
}

/// Every configuration that was measured to turn `merge --ff-only` into a
/// squash or a merge commit, and the one that did not.
#[test]
fn a_clean_checkout_is_fast_forwarded_whatever_its_merge_options_say() {
    let hostile: &[(&str, &str)] = &[
        ("", ""),
        ("branch.main.mergeOptions", "--no-ff"),
        ("branch.main.mergeOptions", "--squash"),
        ("branch.main.mergeOptions", "--no-commit"),
        ("branch.main.mergeOptions", "-s ours"),
        (
            "branch.main.mergeOptions",
            "--squash --no-commit --no-ff -s ours",
        ),
        ("pull.twohead", "ours"),
        ("merge.ff", "false"),
    ];
    for (key, value) in hostile {
        let repo = behind();
        if !key.is_empty() {
            git_at(&repo.local, &["config", key, value]);
        }
        git::write(&repo.local, &checkout("main", &repo.new))
            .unwrap_or_else(|e| panic!("{key}={value}: {e}"));
        assert_eq!(oid(&repo.local, "HEAD"), repo.new, "{key}={value}");
        assert_eq!(
            parents(&repo.local, "HEAD"),
            1,
            "{key}={value}: no merge commit"
        );
        assert_eq!(status(&repo.local), "", "{key}={value}: nothing staged");
        assert_eq!(
            git_at(&repo.local, &["symbolic-ref", "HEAD"]),
            "refs/heads/main"
        );
    }
}

#[test]
fn a_diverged_checkout_is_refused_and_left_as_it_was() {
    let repo = behind();
    git_at(
        &repo.local,
        &["config", "branch.main.mergeOptions", "-s ours"],
    );
    let local = commit(&repo.local, "local");
    assert!(git::write(&repo.local, &checkout("main", &repo.new)).is_err());
    assert_eq!(oid(&repo.local, "HEAD"), local);
    assert_eq!(parents(&repo.local, "HEAD"), 1);
    assert_eq!(status(&repo.local), "");
}

/// A configured refspec applies as a refmap beside an explicit one —
/// measured: without `--refmap=` this fetch force-moves `feature` too.
#[test]
fn the_exact_fetch_writes_only_the_named_tracking_ref() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let feature = commit(&repo.local, "feature");
    git_at(
        &repo.local,
        &[
            "config",
            "--add",
            "remote.origin.fetch",
            "+refs/heads/main:refs/heads/feature",
        ],
    );
    let newer = commit(&repo.upstream, "newer");
    git::write(
        &repo.local,
        &Write::FetchSource {
            remote: "origin".into(),
            refspec: "refs/heads/main:refs/remotes/origin/main".into(),
            refuse_forced: true,
        },
    )
    .unwrap();
    assert_eq!(oid(&repo.local, "origin/main"), newer);
    assert_eq!(oid(&repo.local, "feature"), feature, "feature is kept");
}

// ---- The decision, the plan and the run ----

#[test]
fn the_decision_is_up_to_date_a_fast_forward_or_left_alone() {
    assert_eq!(decide("a", "a", 0), Step::UpToDate);
    assert_eq!(decide("a", "b", 0), Step::FastForward("b".into()));
    assert_eq!(decide("a", "b", 1), Step::Skip(Reason::Diverged));
    assert_eq!(decide("a", "b", 7), Step::Skip(Reason::Diverged));
}

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap()
}

fn link_worktree(repo: &Behind, branch: &str) -> PathBuf {
    let linked = repo.local.with_file_name("linked");
    git_at(
        &repo.local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), branch],
    );
    linked
}

#[test]
fn a_repository_without_a_default_branch_has_no_plan() {
    let folder = Scratch::new("guild-auto-ff-");
    git_at(&folder, &["init", "-q", "-b", "trunk"]);
    identify(&folder);
    commit(&folder, "base");
    assert_eq!(Plan::read(&folder), Ok(None));
}

#[test]
fn a_default_branch_without_a_remote_upstream_is_said_so() {
    let folder = Scratch::new("guild-auto-ff-");
    git_at(&folder, &["init", "-q", "-b", "main"]);
    identify(&folder);
    commit(&folder, "base");
    assert_eq!(Plan::read(&folder), Err(Reason::NoUpstream));
    git_at(&folder, &["branch", "-q", "x"]);
    git_at(&folder, &["config", "branch.main.remote", "."]);
    git_at(&folder, &["config", "branch.main.merge", "refs/heads/x"]);
    assert_eq!(Plan::read(&folder), Err(Reason::NoUpstream));
}

#[test]
fn the_plan_names_the_default_branch_its_upstream_and_its_checkout() {
    let repo = behind();
    let plan = Plan::read(&repo.local).unwrap().unwrap();
    assert_eq!(
        plan,
        Plan {
            branch: "main".into(),
            local: repo.old.clone(),
            remote: "origin".into(),
            source: "refs/heads/main".into(),
            tracking: "refs/remotes/origin/main".into(),
            checkout: Checkout::At(canonical(&repo.local)),
        }
    );
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    assert_eq!(
        Plan::read(&repo.local).unwrap().unwrap().checkout,
        Checkout::Nowhere
    );
    let linked = link_worktree(&repo, "main");
    assert_eq!(
        Plan::read(&repo.local).unwrap().unwrap().checkout,
        Checkout::At(canonical(&linked))
    );
    std::fs::remove_dir_all(&linked).unwrap();
    assert_eq!(Plan::read(&repo.local), Err(Reason::MissingWorktree));
}

/// The default is what new worktrees start from: `origin/HEAD`'s branch
/// beats a local `main`.
#[test]
fn the_plan_follows_origin_head_rather_than_the_name_main() {
    let folder = Scratch::new("guild-auto-ff-");
    let upstream = folder.join("upstream");
    let local = folder.join("local");
    std::fs::create_dir(&upstream).unwrap();
    git_at(&upstream, &["init", "-q", "-b", "develop"]);
    identify(&upstream);
    commit(&upstream, "base");
    git_at(
        &folder,
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            local.to_str().unwrap(),
        ],
    );
    git_at(&local, &["branch", "-q", "main"]);
    let plan = Plan::read(&local).unwrap().unwrap();
    assert_eq!(plan.branch, "develop");
    assert_eq!(plan.tracking, "refs/remotes/origin/develop");
}

#[test]
fn the_preflight_refuses_every_state_a_fast_forward_must_not_meet() {
    let check = |repo: &Behind| preflight(&repo.local, "main", &repo.old, &repo.new);
    assert_eq!(check(&behind()), Ok(()));

    let repo = behind();
    std::fs::write(repo.local.join("file"), "edited\n").unwrap();
    assert_eq!(check(&repo), Err(Reason::Dirty));
    git_at(&repo.local, &["add", "file"]);
    assert_eq!(check(&repo), Err(Reason::Dirty), "staged");

    let repo = behind();
    std::fs::write(repo.local.join("new"), "untracked\n").unwrap();
    assert_eq!(check(&repo), Err(Reason::Dirty), "untracked");

    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    assert_eq!(check(&repo), Err(Reason::BranchChanged), "another branch");

    let repo = behind();
    commit(&repo.local, "local");
    assert_eq!(check(&repo), Err(Reason::BranchChanged), "another commit");

    let repo = behind();
    std::fs::create_dir(repo.local.join(".git/rebase-merge")).unwrap();
    assert_eq!(check(&repo), Err(Reason::OperationInProgress), "rebase");

    let repo = behind();
    std::fs::write(
        repo.local.join(".git/MERGE_HEAD"),
        format!("{}\n", repo.new),
    )
    .unwrap();
    assert_eq!(check(&repo), Err(Reason::OperationInProgress), "merge");
}

/// What a run reported, as each write's verb and whether git refused it.
type Heard = Vec<(Write, bool)>;

fn run(repo: &Behind) -> (Landed, Heard) {
    run_hearing(repo, &AtomicBool::new(false), |_| {})
}

fn run_hearing(
    repo: &Behind,
    cancelled: &AtomicBool,
    mut after: impl FnMut(&Write),
) -> (Landed, Heard) {
    let mut heard = Vec::new();
    let landed = run_once(
        &repo.local,
        cancelled,
        Duration::from_secs(60),
        |write, result| {
            heard.push((write.clone(), result.is_err()));
            after(write);
        },
    );
    (landed, heard)
}

fn is_fetch(heard: &Heard) -> bool {
    matches!(
        heard.as_slice(),
        [(
            Write::FetchSource {
                refuse_forced: true,
                ..
            },
            _
        )]
    )
}

#[test]
fn a_branch_checked_out_nowhere_is_fetched_and_fast_forwarded() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let newer = commit(&repo.upstream, "newer");
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::FastForwarded);
    assert_eq!(oid(&repo.local, "main"), newer);
    assert_eq!(oid(&repo.local, "origin/main"), newer);
    assert!(matches!(
        heard.as_slice(),
        [
            (
                Write::FetchSource {
                    refuse_forced: true,
                    ..
                },
                false
            ),
            (Write::FastForwardBranch { .. }, false)
        ]
    ));
}

#[test]
fn an_up_to_date_branch_writes_nothing_after_the_fetch() {
    let repo = in_step();
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::UpToDate);
    assert!(is_fetch(&heard), "{heard:?}");
}

#[test]
fn a_diverged_branch_is_left_alone() {
    let repo = behind();
    let local = commit(&repo.local, "local");
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::Skipped(Reason::Diverged));
    assert!(is_fetch(&heard));
    assert_eq!(oid(&repo.local, "main"), local);
}

#[test]
fn a_failed_fetch_is_followed_by_no_write() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let missing = repo.upstream.with_file_name("nowhere");
    git_at(
        &repo.local,
        &["remote", "set-url", "origin", missing.to_str().unwrap()],
    );
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::Skipped(Reason::FetchFailed));
    assert!(matches!(
        heard.as_slice(),
        [(Write::FetchSource { .. }, true)]
    ));
    assert_eq!(oid(&repo.local, "main"), repo.old);
}

/// The manual update's timeout fixture: an ssh that never answers.
#[test]
fn a_fetch_past_its_deadline_is_stopped_and_followed_by_no_write() {
    use std::os::unix::fs::PermissionsExt;
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let script = repo.local.with_file_name("ssh");
    std::fs::write(&script, "#!/bin/sh\nsleep 5\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    git_at(
        &repo.local,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    git_at(
        &repo.local,
        &["remote", "set-url", "origin", "ssh://example.invalid/repo"],
    );
    let started = std::time::Instant::now();
    let mut heard = Vec::new();
    let landed = run_once(
        &repo.local,
        &AtomicBool::new(false),
        Duration::from_secs(1),
        |write, result| heard.push((write.clone(), result.is_err())),
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "the deadline held"
    );
    assert_eq!(landed, Landed::Skipped(Reason::FetchFailed));
    assert!(matches!(
        heard.as_slice(),
        [(Write::FetchSource { .. }, true)]
    ));
    assert_eq!(oid(&repo.local, "main"), repo.old);
}

#[test]
fn a_cancelled_run_fetches_nothing_and_one_cancelled_after_its_fetch_writes_nothing() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let (landed, heard) = run_hearing(&repo, &AtomicBool::new(true), |_| {});
    assert_eq!(landed, Landed::Cancelled);
    assert!(heard.is_empty());

    let cancelled = AtomicBool::new(false);
    let (landed, heard) = run_hearing(&repo, &cancelled, |_| {
        cancelled.store(true, Ordering::Release)
    });
    assert_eq!(landed, Landed::Cancelled);
    assert!(is_fetch(&heard));
    assert_eq!(oid(&repo.local, "main"), repo.old);
}

/// The review's race, from the run's side: a commit lands on `main`
/// between the plan and the write.
#[test]
fn a_commit_made_during_the_fetch_stops_the_ref_write() {
    let repo = behind();
    git_at(&repo.local, &["config", "fetch.showForcedUpdates", "false"]);
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let mut made = None;
    let (landed, heard) = run_hearing(&repo, &AtomicBool::new(false), |_| {
        git_at(&repo.local, &["switch", "-q", "main"]);
        made = Some(commit(&repo.local, "during"));
        git_at(&repo.local, &["switch", "-q", "feature"]);
    });
    assert_eq!(landed, Landed::Skipped(Reason::BranchChanged));
    assert!(is_fetch(&heard));
    assert_eq!(Some(oid(&repo.local, "main")), made);
}

/// A checked-out default branch is never written by the run: only the
/// window knows which roots are open in that checkout when it decides.
#[test]
fn a_checked_out_branch_is_handed_back_unwritten() {
    let repo = behind();
    let (landed, heard) = run(&repo);
    assert_eq!(
        landed,
        Landed::Handoff(Handoff {
            worktree: canonical(&repo.local),
            branch: "main".into(),
            expected: repo.old.clone(),
            oid: repo.new.clone(),
        })
    );
    assert!(is_fetch(&heard));
    assert_eq!(oid(&repo.local, "HEAD"), repo.old);

    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
    let linked = link_worktree(&repo, "main");
    let (landed, heard) = run(&repo);
    assert!(
        matches!(&landed, Landed::Handoff(handoff) if handoff.worktree == canonical(&linked)),
        "{landed:?}"
    );
    assert!(is_fetch(&heard));
    assert_eq!(oid(&linked, "HEAD"), repo.old);
}

/// What the window does with a hand-off no root has open: the checkout's
/// own fast-forward, clean or refused.
#[test]
fn a_handed_back_checkout_is_fast_forwarded_when_clean_and_left_when_dirty() {
    let never = AtomicBool::new(false);
    for dirty in [false, true] {
        let repo = behind();
        git_at(&repo.local, &["switch", "-q", "-c", "feature"]);
        let linked = link_worktree(&repo, "main");
        if dirty {
            std::fs::write(linked.join("file"), "edited\n").unwrap();
        }
        let mut heard = Vec::new();
        let landed = auto_ff::fast_forward_checkout(
            &linked,
            "main",
            &repo.old,
            &repo.new,
            || true,
            &never,
            |write, result| heard.push((write.clone(), result.is_err())),
        );
        if dirty {
            assert_eq!(landed, Landed::Skipped(Reason::Dirty));
            assert!(heard.is_empty());
            assert_eq!(oid(&linked, "HEAD"), repo.old);
        } else {
            assert_eq!(landed, Landed::FastForwarded);
            assert_eq!(oid(&linked, "HEAD"), repo.new);
            assert_eq!(status(&linked), "");
            assert!(matches!(
                heard.as_slice(),
                [(Write::FastForwardCheckout { .. }, false)]
            ));
        }
    }
}

#[test]
fn the_checkout_fast_forward_asks_the_buffers_and_the_cancellation_last() {
    let repo = behind();
    let never = AtomicBool::new(false);
    let landed = auto_ff::fast_forward_checkout(
        &repo.local,
        "main",
        &repo.old,
        &repo.new,
        || false,
        &never,
        |_, _| {},
    );
    assert_eq!(landed, Landed::Skipped(Reason::Unsaved));
    let landed = auto_ff::fast_forward_checkout(
        &repo.local,
        "main",
        &repo.old,
        &repo.new,
        || true,
        &AtomicBool::new(true),
        |_, _| panic!("nothing is written once cancelled"),
    );
    assert_eq!(landed, Landed::Cancelled);
    assert_eq!(oid(&repo.local, "HEAD"), repo.old);
}

/// A symbolic tracking ref would redirect the fetch's write: with
/// `refs/remotes/origin/main` pointing at `refs/heads/feature`, the fetch
/// moved `feature` — measured on git 2.50.1, `--refmap=` notwithstanding.
/// The run fetches nothing and says why.
#[test]
fn a_symbolic_tracking_ref_is_refused_before_the_fetch() {
    let repo = behind();
    git_at(&repo.local, &["branch", "feature"]);
    git_at(
        &repo.local,
        &[
            "symbolic-ref",
            "refs/remotes/origin/main",
            "refs/heads/feature",
        ],
    );
    let newer = commit(&repo.upstream, "newer");
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::Skipped(Reason::SymbolicTracking));
    assert!(heard.is_empty(), "nothing fetched: {heard:?}");
    assert_eq!(oid(&repo.local, "feature"), repo.old, "feature kept");
    assert_ne!(oid(&repo.local, "feature"), newer);
}

/// Review r2: a file git ignores here and upstream starts to track. A
/// merge overwrites an ignored file by default, and nothing of it is in
/// git to get back; `--no-overwrite-ignore` makes git refuse instead.
#[test]
fn an_ignored_file_upstream_adds_is_not_overwritten() {
    let repo = behind();
    std::fs::write(repo.upstream.join("local.conf"), "theirs\n").unwrap();
    git_at(&repo.upstream, &["add", "local.conf"]);
    git_at(&repo.upstream, &["commit", "-q", "-m", "track local.conf"]);
    let newer = oid(&repo.upstream, "HEAD");
    git_at(&repo.local, &["fetch", "-q", "origin"]);
    std::fs::write(repo.local.join(".git/info/exclude"), "local.conf\n").unwrap();
    std::fs::write(repo.local.join("local.conf"), "mine\n").unwrap();
    let mut heard = Vec::new();
    let landed = auto_ff::fast_forward_checkout(
        &repo.local,
        "main",
        &repo.old,
        &newer,
        || true,
        &AtomicBool::new(false),
        |write, result| heard.push((write.clone(), result.is_err())),
    );
    assert_eq!(landed, Landed::Skipped(Reason::Refused));
    assert!(
        matches!(
            heard.as_slice(),
            [(Write::FastForwardCheckout { .. }, true)]
        ),
        "git refused, in the Console: {heard:?}"
    );
    assert_eq!(
        std::fs::read_to_string(repo.local.join("local.conf")).unwrap(),
        "mine\n"
    );
    assert_eq!(oid(&repo.local, "HEAD"), repo.old);
}

/// Review r2: a default branch that is itself a symbolic ref. With
/// `refs/heads/main` pointing at `refs/heads/feature`, the ref write would
/// move `feature` and the post-check, reading through the same link, would
/// call it a fast-forward. The plan refuses before anything is written.
#[test]
fn a_symbolic_default_branch_is_refused_before_any_write() {
    let repo = behind();
    git_at(&repo.local, &["switch", "-q", "-c", "other"]);
    git_at(&repo.local, &["branch", "feature", &repo.old]);
    git_at(
        &repo.local,
        &["symbolic-ref", "refs/heads/main", "refs/heads/feature"],
    );
    git_at(&repo.local, &["config", "branch.main.remote", "origin"]);
    git_at(
        &repo.local,
        &["config", "branch.main.merge", "refs/heads/main"],
    );
    let head = oid(&repo.local, "HEAD");
    let (landed, heard) = run(&repo);
    assert_eq!(landed, Landed::Skipped(Reason::SymbolicBranch));
    assert!(heard.is_empty(), "nothing fetched or written: {heard:?}");
    assert_eq!(oid(&repo.local, "feature"), repo.old, "feature kept");
    assert_eq!(oid(&repo.local, "HEAD"), head);
    assert_eq!(git_at(&repo.local, &["branch", "--show-current"]), "other");
    assert_eq!(status(&repo.local), "", "index and files untouched");
}
