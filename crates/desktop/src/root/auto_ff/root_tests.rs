//! The glue on a real `Root`, in an isolated child with its own state
//! directory: `poll_git`'s clock starts the run, its thread fetches and
//! writes, its Console lines are filed, and a manual Update branch clicked
//! meanwhile waits for it.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui::{AppContext, Entity, TestAppContext};

/// One child for every test here, one test at a time: each drives real git
/// and a real `Root`, and five children at once beside the harness tests'
/// deadlines made those flaky under a full `just check`.
#[test]
fn isolated_auto_ff_root_regressions() {
    let state = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "root::auto_ff::root_tests::",
            "--ignored",
            "--test-threads=1",
        ])
        .env("GUILD_STATE_DIR", state.path())
        .env("GUILD_AUTO_FF_ROOT_TEST", "1")
        .env("GUILD_NO_GLOBAL_HOOKS", "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success() && stdout.contains("14 passed; 0 failed"),
        "{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git(root: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Guild",
            "-c",
            "user.email=guild@example.invalid",
        ])
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

/// An upstream one commit ahead of a clone that is on `feature`, so the
/// clone's `main` is checked out nowhere and behind once fetched.
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String) {
    let folder = tempfile::tempdir().unwrap();
    let upstream = folder.path().join("upstream");
    let local = folder.path().join("local");
    std::fs::create_dir(&upstream).unwrap();
    git(&upstream, &["init", "-q", "-b", "main"]);
    git(&upstream, &["commit", "-q", "--allow-empty", "-m", "base"]);
    git(
        folder.path(),
        &[
            "clone",
            "-q",
            upstream.to_str().unwrap(),
            local.to_str().unwrap(),
        ],
    );
    git(&local, &["switch", "-q", "-c", "feature"]);
    git(&upstream, &["commit", "-q", "--allow-empty", "-m", "newer"]);
    let newer = git(&upstream, &["rev-parse", "HEAD"]);
    (folder, upstream, local.canonicalize().unwrap(), newer)
}

fn mount(
    cx: &mut TestAppContext,
    local: &Path,
    on: bool,
) -> (Entity<crate::Root>, guild_session::ProjectId) {
    assert_eq!(
        std::env::var("GUILD_AUTO_FF_ROOT_TEST").as_deref(),
        Ok("1"),
        "only in the isolated child, which owns its state directory"
    );
    cx.executor().allow_parking();
    let mut desk = guild_session::Desk::default();
    desk.normalise();
    let project = desk.open_in(desk.active_workspace(), local);
    let settings = guild_session::Settings {
        auto_fast_forward_default_branch: on,
        ..Default::default()
    };
    let root = cx.new(|cx| {
        let mut root = crate::Root::from_state(settings, desk, cx);
        root.active_project = Some(project);
        root
    });
    (root, project)
}

/// Poll the window's git clock until `done` says so.
fn poll_until(
    cx: &mut TestAppContext,
    root: &Entity<crate::Root>,
    within: Duration,
    mut done: impl FnMut(&crate::Root) -> bool,
) -> bool {
    let until = Instant::now() + within;
    while Instant::now() < until {
        let finished = root.update(cx, |root, cx| {
            root.poll_git(cx);
            done(root)
        });
        if finished {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn console(root: &crate::Root, project: guild_session::ProjectId) -> Vec<Vec<String>> {
    root.consoles
        .get(&project)
        .map(|lines| lines.lines().map(|line| line.argv.clone()).collect())
        .unwrap_or_default()
}

#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn opening_a_project_fast_forwards_its_default_branch_silently(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, newer) = fixture();
    let (root, project) = mount(cx, &local, true);
    let landed = poll_until(cx, &root, Duration::from_secs(30), |_| {
        git(&local, &["rev-parse", "main"]) == newer
    });
    assert!(landed, "main was not fast-forwarded");
    poll_until(cx, &root, Duration::from_secs(5), |root| {
        console(root, project).len() >= 2
    });
    root.update(cx, |root, _| {
        let lines = console(root, project);
        assert!(
            lines[0].contains(&"fetch.showForcedUpdates=true".to_owned()),
            "{lines:?}"
        );
        assert!(
            lines[1]
                .iter()
                .any(|word| word.ends_with(":refs/heads/main"))
        );
        assert!(root.notices.is_empty(), "silent: no notice");
    });
    assert_eq!(git(&local, &["branch", "--show-current"]), "feature");
}

#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn with_the_setting_off_nothing_is_fetched(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, _newer) = fixture();
    let before = git(&local, &["rev-parse", "origin/main"]);
    let (root, project) = mount(cx, &local, false);
    // Long enough for the repository to be discovered and a run to start.
    poll_until(cx, &root, Duration::from_secs(3), |_| false);
    assert_eq!(git(&local, &["rev-parse", "origin/main"]), before);
    root.update(cx, |root, _| {
        assert!(console(root, project).is_empty());
        assert_eq!(root.auto_ff.in_flight(), 0);
    });
}

/// A click during the run is held and runs after it — never beside it.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_manual_update_clicked_during_a_run_waits_for_it(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (folder, _upstream, local, _newer) = fixture();
    // A remote that answers nothing for two seconds keeps the run in flight.
    let script = folder.path().join("ssh");
    std::fs::write(&script, "#!/bin/sh\nsleep 2\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    git(
        &local,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    git(
        &local,
        &["remote", "set-url", "origin", "ssh://example.invalid/repo"],
    );
    let (root, project) = mount(cx, &local, true);
    let started = poll_until(cx, &root, Duration::from_secs(20), |root| {
        root.auto_ff.in_flight() == 1 && root.update_snapshot().is_some()
    });
    assert!(started, "the run did not start");
    root.update(cx, |root, cx| root.update_current_branch(cx));
    root.update(cx, |root, _| {
        assert!(
            root.updating_branches.contains_key(&project),
            "shown running"
        );
        assert!(root.held_updates.contains_key(&project), "held");
    });
    let finished = poll_until(cx, &root, Duration::from_secs(30), |root| {
        !root.updating_branches.contains_key(&project) && console(root, project).len() >= 2
    });
    assert!(finished, "the held update never ran");
    root.update(cx, |root, _| {
        let lines = console(root, project);
        assert!(
            lines[0].contains(&"fetch.showForcedUpdates=true".to_owned()),
            "the automatic fetch first: {lines:?}"
        );
        assert!(
            lines[1].contains(&"fetch".to_owned())
                && !lines[1].contains(&"fetch.showForcedUpdates=true".to_owned()),
            "then the manual one, after it: {lines:?}"
        );
    });
}

/// The default branch checked out in the opened root: the run hands the
/// write to that root's git thread, which fast-forwards the checkout.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_checked_out_default_branch_is_fast_forwarded_by_its_roots_thread(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    let (root, project) = mount(cx, &local, true);
    let landed = poll_until(cx, &root, Duration::from_secs(30), |root| {
        root.auto_ff.in_flight() == 0 && git(&local, &["rev-parse", "HEAD"]) == newer
    });
    assert!(landed, "the checkout was not fast-forwarded");
    assert_eq!(git(&local, &["status", "--porcelain"]), "");
    poll_until(cx, &root, Duration::from_secs(5), |root| {
        console(root, project).len() >= 2
    });
    root.update(cx, |root, _| {
        let lines = console(root, project);
        assert!(lines[1].contains(&"--ff-only".to_owned()), "{lines:?}");
        assert!(root.notices.is_empty(), "silent: no notice");
        let common = root.repository_of(project).unwrap();
        assert_eq!(root.auto_ff.skipped(&common), None);
    });
}

/// A dirty checkout is left alone, silently, and the reason is what Update
/// branch's hint and source list say.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_dirty_checkout_is_skipped_and_the_reason_is_shown_where_update_branch_explains(
    cx: &mut TestAppContext,
) {
    let (_folder, _upstream, local, newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    let before = git(&local, &["rev-parse", "HEAD"]);
    std::fs::write(local.join("untracked"), "work\n").unwrap();
    let (root, _project) = mount(cx, &local, true);
    let skipped = poll_until(cx, &root, Duration::from_secs(30), |root| {
        root.auto_ff_skip_line(crate::root::auto_ff::Surface::Hint)
            .is_some()
    });
    assert!(skipped, "no skip was recorded");
    assert_eq!(git(&local, &["rev-parse", "HEAD"]), before);
    assert_eq!(git(&local, &["rev-parse", "origin/main"]), newer, "fetched");
    root.update(cx, |root, _| {
        let line = root
            .auto_ff_skip_line(crate::root::auto_ff::Surface::Sources)
            .unwrap();
        assert!(
            line.contains("main") && line.contains("uncommitted"),
            "{line}"
        );
        assert!(root.notices.is_empty(), "silent: no notice");
    });
}

/// A remote that answers after two seconds, through git's own upload-pack:
/// a run stays in flight long enough to act during its fetch, and then
/// succeeds.
fn slow_remote(folder: &Path, local: &Path, upstream: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let script = folder.join("slow-ssh");
    std::fs::write(&script, "#!/bin/sh\nsleep 2\nexec sh -c \"$2\"\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    git(
        local,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    git(local, &["config", "ssh.variant", "simple"]);
    git(
        local,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("ssh://example.invalid{}", upstream.display()),
        ],
    );
}

fn open(
    cx: &mut TestAppContext,
    root: &Entity<crate::Root>,
    path: &Path,
) -> guild_session::ProjectId {
    root.update(cx, |root, _| {
        let workspace = root.desk.active_workspace();
        root.desk.open_in(workspace, path)
    })
}

/// Review r1: a checkout opened while the fetch runs is written by its own
/// root's git thread, where its buffers are asked — not by the run's thread
/// from the list of roots it started with.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_checkout_opened_during_the_fetch_is_written_by_its_own_root(cx: &mut TestAppContext) {
    let (folder, upstream, local, newer) = fixture();
    let linked = folder.path().join("linked");
    git(
        &local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    );
    let linked = linked.canonicalize().unwrap();
    slow_remote(folder.path(), &local, &upstream);
    let (root, primary) = mount(cx, &local, true);
    assert!(poll_until(cx, &root, Duration::from_secs(20), |root| {
        root.auto_ff.in_flight() == 1
    }));
    let opened = open(cx, &root, &linked);
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            root.auto_ff.in_flight() == 0 && git(&linked, &["rev-parse", "HEAD"]) == newer
        }),
        "the opened checkout was not fast-forwarded"
    );
    poll_until(cx, &root, Duration::from_secs(5), |root| {
        !console(root, opened).is_empty()
    });
    root.update(cx, |root, _| {
        let merged = |project| {
            console(root, project)
                .iter()
                .any(|argv| argv.contains(&"--ff-only".to_owned()))
        };
        assert!(merged(opened), "written on the opened root's thread");
        assert!(!merged(primary), "not by the run's thread");
    });
}

/// Review r1: closing the checkout's root after the hand-off must not keep
/// the run in flight for good; reopened, the repository runs again and a
/// manual update is not held for ever.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn closing_the_checkout_after_the_handoff_frees_the_run(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, _newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    let (root, project) = mount(cx, &local, true);
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            root.auto_ff.waiting_on_worker()
        }),
        "the run never handed off"
    );
    root.update(cx, |root, cx| root.close_worktree_now(project, cx));
    assert!(
        poll_until(cx, &root, Duration::from_secs(10), |root| {
            root.auto_ff.in_flight() == 0
        }),
        "the run kept its slot after its root closed"
    );
    let reopened = open(cx, &root, &local);
    root.update(cx, |root, _| root.active_project = Some(reopened));
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            root.auto_ff.in_flight() == 0
                && !console(root, reopened).is_empty()
                && root.update_snapshot().is_some()
        }),
        "the reopened repository did not run again"
    );
    root.update(cx, |root, cx| root.update_current_branch(cx));
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            !root.updating_branches.contains_key(&reopened)
        }),
        "the manual update was held for ever"
    );
}

/// Review r1, task 3.4's promise: a click on the default branch while the
/// run fetches waits, the run fast-forwards the checkout, and the manual
/// update then runs from the new HEAD — no "HEAD changed" refusal.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_click_on_main_during_a_successful_run_updates_from_the_new_head(cx: &mut TestAppContext) {
    let (folder, upstream, local, newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    slow_remote(folder.path(), &local, &upstream);
    let (root, project) = mount(cx, &local, true);
    assert!(poll_until(cx, &root, Duration::from_secs(20), |root| {
        root.auto_ff.in_flight() == 1 && root.update_snapshot().is_some()
    }));
    root.update(cx, |root, cx| root.update_current_branch(cx));
    root.update(cx, |root, _| {
        assert!(root.held_updates.contains_key(&project), "held");
    });
    assert!(
        poll_until(cx, &root, Duration::from_secs(40), |root| {
            !root.updating_branches.contains_key(&project)
        }),
        "the held update never ran"
    );
    assert_eq!(git(&local, &["rev-parse", "HEAD"]), newer);
    root.update(cx, |root, _| {
        let receipt = root
            .settings
            .branch_update_receipts
            .get(&local)
            .expect("the manual update left its receipt");
        let guild_session::settings::BranchUpdateRecord::Current(receipt) = receipt else {
            panic!("a current receipt: {receipt:?}");
        };
        assert_eq!(
            receipt.kind,
            guild_session::settings::BranchUpdateKind::Unchanged,
            "{receipt:?}"
        );
        assert!(!receipt.diagnostic.contains("changed"), "{receipt:?}");
        assert_eq!(receipt.before, newer, "built from the new HEAD");
    });
}

/// Review r1: a write in a checkout no root has open is filed under the run's
/// project, but names the directory git ran in — the checkout's.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_write_in_an_unopened_checkout_names_that_checkout_in_the_console(cx: &mut TestAppContext) {
    let (folder, _upstream, local, newer) = fixture();
    let linked = folder.path().join("linked");
    git(
        &local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    );
    let linked = linked.canonicalize().unwrap();
    let (root, primary) = mount(cx, &local, true);
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            root.auto_ff.in_flight() == 0 && git(&linked, &["rev-parse", "HEAD"]) == newer
        }),
        "the unopened checkout was not fast-forwarded"
    );
    root.update(cx, |root, _| {
        let lines: Vec<_> = root
            .consoles
            .get(&primary)
            .unwrap()
            .lines()
            .cloned()
            .collect();
        let fetch = lines
            .iter()
            .find(|line| line.argv.contains(&"fetch".to_owned()))
            .unwrap();
        let merge = lines
            .iter()
            .find(|line| line.argv.contains(&"--ff-only".to_owned()))
            .expect("the merge is in the run's Console");
        assert_eq!(
            Path::new(&fetch.root),
            local.as_path(),
            "the fetch ran in the root"
        );
        assert_eq!(
            Path::new(&merge.root),
            linked.as_path(),
            "the merge ran in the checkout"
        );
    });
}

/// Review r2: a checkout nested inside the default checkout's folder — a
/// linked worktree here — is not a root of that checkout. With only the
/// nested one open, the default checkout is written as an unopened one.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_nested_checkout_is_not_taken_for_the_default_checkout(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    std::fs::write(local.join(".git/info/exclude"), "nested/\n").unwrap();
    let nested = local.join("nested");
    git(
        &local,
        &["worktree", "add", "-q", nested.to_str().unwrap(), "feature"],
    );
    let (root, _project) = mount(cx, &nested, true);
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |_| {
            git(&local, &["rev-parse", "HEAD"]) == newer
        }),
        "the default checkout was not fast-forwarded"
    );
    assert_eq!(git(&nested, &["branch", "--show-current"]), "feature");
}

/// Review r2: a manual Update branch still fetching when its last root
/// closes is still in flight; reopened, the repository's automatic run
/// waits for it.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_manual_update_outliving_its_root_still_holds_the_run(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (folder, upstream, local, newer) = fixture();
    let (root, project) = mount(cx, &local, true);
    assert!(poll_until(cx, &root, Duration::from_secs(30), |root| {
        root.auto_ff.in_flight() == 0 && !console(root, project).is_empty()
    }));
    // The window has no file watcher here, which is what re-reads the
    // update's source after the run's fetch moved `origin/main`.
    root.update(cx, |root, _| root.refresh_branches(project));
    assert!(poll_until(cx, &root, Duration::from_secs(10), |root| {
        root.update_snapshot().is_some_and(|snapshot| {
            snapshot
                .sources
                .iter()
                .any(|source| source.reference == "refs/remotes/origin/main" && source.oid == newer)
        })
    }));
    // From now on every fetch answers after four seconds, and says when it
    // has started.
    let started_fetch = folder.path().join("fetch-started");
    let script = folder.path().join("slower-ssh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\ntouch '{}'\nsleep 4\nexec sh -c \"$2\"\n",
            started_fetch.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
    git(
        &local,
        &["config", "core.sshCommand", script.to_str().unwrap()],
    );
    git(&local, &["config", "ssh.variant", "simple"]);
    git(
        &local,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("ssh://example.invalid{}", upstream.display()),
        ],
    );
    root.update(cx, |root, cx| {
        root.update_current_branch(cx);
        assert!(root.updating_branches.contains_key(&project));
    });
    // Closed only once the manual fetch is really under way: an ask its
    // thread drops unrun runs nothing, and holds nothing.
    let until = Instant::now() + Duration::from_secs(10);
    while !started_fetch.exists() {
        assert!(Instant::now() < until, "the manual fetch never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    root.update(cx, |root, cx| root.close_worktree_now(project, cx));
    let reopened = open(cx, &root, &local);
    let started = Instant::now();
    while started.elapsed() < Duration::from_millis(2500) {
        root.update(cx, |root, cx| {
            root.poll_git(cx);
            assert_eq!(
                root.auto_ff.in_flight(),
                0,
                "an automatic run started beside the manual update"
            );
        });
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            !console(root, reopened).is_empty() && root.auto_ff.in_flight() == 0
        }),
        "the automatic run never followed"
    );
}

/// Review r2: the last root closes after the hand-off but before its git
/// thread reaches the write — held off here by taking the checkout's turn.
/// The close cancels at once, not on the next tick: nothing is merged.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn closing_the_last_root_cancels_a_handed_off_write_at_once(cx: &mut TestAppContext) {
    let (_folder, _upstream, local, _newer) = fixture();
    git(&local, &["switch", "-q", "main"]);
    let before = git(&local, &["rev-parse", "HEAD"]);
    let turn = crate::git::turn(&local).expect("the checkout's turn");
    let (root, project) = mount(cx, &local, true);
    assert!(poll_until(cx, &root, Duration::from_secs(30), |root| {
        root.auto_ff.waiting_on_worker()
    }));
    root.update(cx, |root, cx| root.close_worktree_now(project, cx));
    drop(turn);
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(
        git(&local, &["rev-parse", "HEAD"]),
        before,
        "nothing merged"
    );
    assert!(poll_until(cx, &root, Duration::from_secs(10), |root| {
        root.auto_ff.in_flight() == 0
    }));
}

/// Review r2: the hand-off's root closes after its merge ran but before the
/// window heard it. The merge's Console line comes with the run's own
/// answer and is filed where the run's are.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn a_merge_whose_root_closed_is_still_in_the_console(cx: &mut TestAppContext) {
    let (folder, _upstream, local, newer) = fixture();
    let linked = folder.path().join("linked");
    git(
        &local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    );
    let linked = linked.canonicalize().unwrap();
    let turn = crate::git::turn(&linked).expect("the checkout's turn");
    let (root, primary) = mount(cx, &local, true);
    let handed = open(cx, &root, &linked);
    assert!(poll_until(cx, &root, Duration::from_secs(30), |root| {
        root.auto_ff.waiting_on_worker()
    }));
    drop(turn);
    let until = Instant::now() + Duration::from_secs(20);
    while git(&linked, &["rev-parse", "HEAD"]) != newer {
        assert!(Instant::now() < until, "the merge did not run");
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(200));
    root.update(cx, |root, cx| root.close_worktree_now(handed, cx));
    assert!(poll_until(cx, &root, Duration::from_secs(10), |root| {
        root.auto_ff.in_flight() == 0
    }));
    root.update(cx, |root, _| {
        assert!(
            console(root, primary)
                .iter()
                .any(|argv| argv.contains(&"--ff-only".to_owned())),
            "the merge is in the Console: {:?}",
            console(root, primary)
        );
    });
}

/// Review r2: the checkout opened during the fetch holds a real unsaved
/// tab (autosave off). The window asks the buffers before handing off: the
/// run skips as Unsaved, HEAD stays, and nothing is merged.
#[gpui::test]
#[ignore = "runs in isolated_auto_ff_root_regressions' child"]
fn an_unsaved_tab_in_a_checkout_opened_during_the_fetch_stops_the_write(cx: &mut TestAppContext) {
    let (folder, upstream, local, _newer) = fixture();
    let linked = folder.path().join("linked");
    git(
        &local,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    );
    let linked = linked.canonicalize().unwrap();
    let before = git(&linked, &["rev-parse", "HEAD"]);
    // Ignored, so the disk is clean while the buffer is not.
    std::fs::write(local.join(".git/info/exclude"), "notes.txt\n").unwrap();
    std::fs::write(linked.join("notes.txt"), "on disk\n").unwrap();
    slow_remote(folder.path(), &local, &upstream);
    cx.update(crate::editor::init);
    let (root, primary) = mount(cx, &local, true);
    root.update(cx, |root, _| root.settings.autosave = false);
    assert!(poll_until(cx, &root, Duration::from_secs(20), |root| {
        root.auto_ff.in_flight() == 1
    }));
    let opened = open(cx, &root, &linked);
    root.update(cx, |root, _| root.active_project = Some(opened));
    let mut file = None;
    let (_, visual) = cx.add_window_view(|window, cx| {
        root.update(cx, |root, cx| {
            root.open_file_from(crate::Origin::Project, Path::new("notes.txt"), window, cx)
        });
        let view = root
            .read(cx)
            .tabs
            .iter()
            .flat_map(crate::Tab::bodies)
            .find_map(|body| match body {
                crate::Body::File { tab, .. } => Some(tab.clone()),
                _ => None,
            })
            .expect("the file's tab");
        file = Some(view.clone());
        gpui_component::Root::new(view, window, cx)
    });
    let file = file.unwrap();
    visual.run_until_parked();
    let editor = visual.update(|_, cx| file.read(cx).editor().unwrap().clone());
    visual.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.set_selected_range(0..0, cx);
            editor.replace_all("in the buffer\n".to_owned(), window, cx);
        })
    });
    visual.run_until_parked();
    assert!(visual.update(|_, cx| file.read(cx).is_dirty()), "unsaved");
    assert!(
        poll_until(cx, &root, Duration::from_secs(30), |root| {
            root.auto_ff.in_flight() == 0
        }),
        "the run never landed"
    );
    root.update(cx, |root, _| {
        let common = root.repository_of(primary).unwrap();
        assert_eq!(
            root.auto_ff.skipped(&common),
            Some(&("main".to_owned(), crate::git::auto_ff::Reason::Unsaved))
        );
        for project in [primary, opened] {
            assert!(
                !console(root, project)
                    .iter()
                    .any(|argv| argv.contains(&"--ff-only".to_owned())),
                "nothing merged"
            );
        }
    });
    assert_eq!(git(&linked, &["rev-parse", "HEAD"]), before);
    assert_eq!(
        std::fs::read_to_string(linked.join("notes.txt")).unwrap(),
        "on disk\n"
    );
}
