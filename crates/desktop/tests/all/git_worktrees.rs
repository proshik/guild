//! `git worktree list --porcelain -z` parsed, and the common dir — M12.
//!
//! The lists are git 2.50.1's own, **recorded by M12's task 0** from a
//! scratch repository holding every shape a record can take, with the
//! paths rewritten; the shape is taken from disk rather than from memory.
//! The common dir is asked of a real git, because what it answers from a
//! linked worktree is the whole point of asking.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::scratch::Scratch;
use guild_desktop::git::worktrees::{Worktree, common_dir, ignored, lines, parse, read_numstat};

/// A main worktree, a detached one, a linked one, a prunable one, a
/// locked one and one under a path with a space.
const SIX: &[u8] = include_bytes!("../fixtures/worktrees-list.bin");
/// A bare repository and a worktree made from it.
const BARE: &[u8] = include_bytes!("../fixtures/worktrees-bare.bin");

const HEAD: &str = "467eaac8b04426ce7ffd85ced21a2188c38414c6";

#[test]
fn every_shape_git_prints_is_read() {
    let list = parse(SIX).expect("a list git printed");
    let paths: Vec<&Path> = list
        .iter()
        .map(|worktree| worktree.path.as_path())
        .collect();
    assert_eq!(
        paths,
        [
            "/Users/person/work/repo",
            "/Users/person/work/repo-detached",
            "/Users/person/work/repo-feat",
            "/Users/person/work/repo-gone",
            "/Users/person/work/repo-locked",
            "/Users/person/work/with space/wt",
        ]
        .map(Path::new)
    );

    let main = &list[0];
    assert_eq!(main.branch.as_deref(), Some("main"));
    assert_eq!(main.head.as_deref(), Some(HEAD));
    assert!(!main.detached && !main.bare && main.locked.is_none() && !main.prunable);

    assert!(list[1].detached);
    assert_eq!(list[1].branch, None, "a detached worktree is on no branch");
    assert_eq!(list[2].branch.as_deref(), Some("feat"));
    assert!(list[3].prunable, "its folder was deleted by hand");
    // The lock keeps git's own reason: it says who holds the checkout,
    // which is what tells a person whether to wait or to unlock. M12
    // task 18 — the fixture's is `on a stick`.
    assert_eq!(list[4].locked.as_deref(), Some("on a stick"));
    assert_eq!(list[5].branch.as_deref(), Some("spaced"));
}

#[test]
fn a_bare_repository_is_an_entry_with_no_checkout() {
    let list = parse(BARE).expect("a list git printed");
    assert!(list[0].bare);
    assert_eq!(list[0].head, None, "git prints no HEAD for a bare entry");
    assert_eq!(list[1].branch.as_deref(), Some("frombare"));
}

#[test]
fn only_what_has_a_checkout_is_shown() {
    let list = parse(SIX).unwrap();
    let shown: Vec<&str> = list
        .iter()
        .filter(|worktree| worktree.shown())
        .map(|worktree| worktree.path.file_name().unwrap().to_str().unwrap())
        .collect();
    // The prunable one has no folder; a locked one is still a checkout.
    assert_eq!(
        shown,
        ["repo", "repo-detached", "repo-feat", "repo-locked", "wt"]
    );

    let bare = parse(BARE).unwrap();
    assert!(!bare[0].shown());
    assert!(bare[1].shown());
}

#[test]
fn nothing_is_no_worktrees() {
    assert_eq!(parse(b"").unwrap(), Vec::<Worktree>::new());
}

#[test]
fn a_record_git_never_prints_is_an_error_rather_than_a_guess() {
    // A record has to begin with its path; one that does not is a
    // format this code has not seen, and guessing would draw a row for
    // a directory that may not exist.
    assert!(parse(b"HEAD abc\0\0").is_err());
}

/// A repository with one linked worktree in `base`, made by a real git.
/// The folder is the test's to hold: a helper that made it here would
/// drop it on the way out.
fn repository(base: &Path) -> (PathBuf, PathBuf) {
    let main = base.join("repo");
    let git = |dir: &Path, args: &[&str]| {
        let status = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(status.status.success(), "git {args:?}: {status:?}");
    };
    git(base, &["init", "-q", "-b", "main", "repo"]);
    std::fs::write(main.join("a.txt"), "a\n").unwrap();
    git(&main, &["add", "a.txt"]);
    git(&main, &["commit", "-qm", "init"]);
    git(
        &main,
        &["worktree", "add", "-q", "-b", "feat", "../repo-feat"],
    );
    (main, base.join("repo-feat"))
}

#[test]
fn a_linked_worktree_and_its_main_one_share_a_common_dir() {
    let base = Scratch::new("gw-common-");
    let (main, linked) = repository(&base);
    let expected = std::fs::canonicalize(main.join(".git")).unwrap();

    let from_main = common_dir(&main).expect("inside a repository");
    let from_linked = common_dir(&linked).expect("inside a repository");
    std::fs::create_dir_all(main.join("sub")).unwrap();
    let from_below = common_dir(&main.join("sub")).expect("inside a repository");

    for answer in [from_main, from_linked, from_below] {
        assert_eq!(std::fs::canonicalize(&answer).unwrap(), expected);
    }
}

#[test]
fn a_directory_outside_git_has_no_common_dir() {
    let plain = Scratch::new("gw-plain-");
    assert!(common_dir(&plain).is_err());
}

/// **One call answers every worktree** — M12 task 16. `git show -s` takes
/// any number of revisions and prints a line for each, so a repository
/// with nine worktrees costs one process rather than nine.
///
/// A line it cannot parse is passed over rather than failing the list: a
/// worktree whose head this repository no longer has is still a worktree,
/// and the column would rather draw it without a time than not at all.
#[test]
fn the_times_of_several_heads_are_read_from_one_answer() {
    use guild_desktop::git::worktrees::read_times;

    let said: &[u8] = b"467eaac8b04426ce7ffd85ced21a2188c38414c6 1757600000\n\
8f91035aa1f2c3d4e5f60718293a4b5c6d7e8f90 1757603600\n";
    assert_eq!(
        read_times(said),
        [
            (
                "467eaac8b04426ce7ffd85ced21a2188c38414c6".to_owned(),
                1_757_600_000
            ),
            (
                "8f91035aa1f2c3d4e5f60718293a4b5c6d7e8f90".to_owned(),
                1_757_603_600
            ),
        ]
    );

    // Nothing to read, and a line that is not a pair: neither is an error.
    assert!(read_times(b"").is_empty());
    assert!(read_times(b"fatal: bad object deadbeef\n").is_empty());
}

/// **What git would delete without a word, asked of a real git** — M13
/// task 8. An ignored folder is one entry and not every file in it, which
/// is what keeps `target/` from being a refusal nobody reads; an untracked
/// file is not in the list, because git refuses to remove over one rather
/// than deleting it; and `.guild/` is not in it, because it is the window's
/// own state for that root, written into every root it opens — a guard
/// that fires on every removal is a flag an agent learns to pass unread.
#[test]
fn what_git_ignores_in_a_worktree_is_named_and_the_window_s_own_state_is_not() {
    let base = Scratch::new("gw-ignored-");
    let (_, feat) = repository(&base);
    std::fs::write(feat.join(".gitignore"), "target/\n.env\n").unwrap();
    std::fs::create_dir_all(feat.join("target/debug")).unwrap();
    std::fs::write(feat.join("target/debug/guild"), "bin").unwrap();
    std::fs::write(feat.join(".env"), "TOKEN=1").unwrap();
    std::fs::write(feat.join("notes.txt"), "untracked").unwrap();
    // As `project_state` writes it: a directory that ignores itself.
    std::fs::create_dir_all(feat.join(".guild")).unwrap();
    std::fs::write(feat.join(".guild/.gitignore"), "*\n").unwrap();
    std::fs::write(feat.join(".guild/state.json"), "{}").unwrap();

    let mut listed = ignored(&feat).unwrap();
    listed.sort();
    assert_eq!(listed, vec![".env".to_owned(), "target/".to_owned()]);
}

/// **What `git diff --numstat -z` prints, summed** — M12.4, recorded from
/// git 2.50.1: a row is `added\tremoved\tpath\0`, a binary file prints
/// `-` for both and counts nothing, and a rename leaves its path empty and
/// puts the two names in the next two fields — which a reader that took
/// every field for a row would count as two rows of nothing, or fail on.
#[test]
fn the_lines_a_diff_added_and_removed_are_summed() {
    let recorded = [
        &b"5\t0\ta.txt\0"[..],
        b"-\t-\tb.bin\0",
        b"1\t0\t\0old.txt\0new.txt\0",
        b"2\t7\tsrc/lib.rs\0",
    ]
    .concat();
    assert_eq!(read_numstat(&recorded), (8, 7));
    assert_eq!(read_numstat(b""), (0, 0), "an empty diff is nothing");
    assert_eq!(
        read_numstat(b"-\t-\tlogo.png\0"),
        (0, 0),
        "a binary file has no lines"
    );
}

/// **A worktree's lines are its branch since it left the main one, and what
/// is not committed yet** — M12.4, read in two steps: the merge-base with the
/// main worktree's branch, then `git diff --numstat` from it. The main
/// worktree's own merge-base is its `HEAD`, so its lines are only what it has
/// not committed.
#[test]
fn a_worktree_counts_its_branch_and_its_changes_and_the_main_one_its_changes() {
    let base = Scratch::new("gw-lines-");
    let (main, linked) = repository(&base);
    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    // Two commits on the branch: ten lines, then four more and one taken.
    std::fs::write(linked.join("b.txt"), "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n").unwrap();
    git(&linked, &["add", "b.txt"]);
    git(&linked, &["commit", "-qm", "ten"]);
    std::fs::write(linked.join("a.txt"), "b\nc\nd\ne\n").unwrap();
    git(&linked, &["commit", "-qam", "four for one"]);
    // And a change on the disk, not committed.
    std::fs::write(linked.join("b.txt"), "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n11\n").unwrap();
    // The main worktree changes a line of its own, not committed.
    std::fs::write(main.join("a.txt"), "z\n").unwrap();

    assert_eq!(
        lines(&linked, "main").expect("git counts"),
        (15, 1),
        "eleven in b.txt, four in a.txt, and the one a.txt lost"
    );
    assert_eq!(
        lines(&main, "main").expect("git counts"),
        (1, 1),
        "the main worktree's line changed, and nothing of the branch"
    );
    assert!(
        lines(&linked, "no-such-branch").is_err(),
        "a base git cannot find is git's refusal, not zero"
    );
}

/// **The main branch is read from a real repository** — M13.1: a clone
/// whose origin names `develop` starts a task there, and a repository with
/// no remote starts it on `main` even while its checkout is on a feature
/// branch — the report this came from.
#[test]
fn the_main_branch_is_origin_s_then_main_whatever_the_checkout_is_on() {
    let base = Scratch::new("gw-main-");
    let (main, linked) = repository(&base);
    assert_eq!(
        guild_desktop::git::main_branch(&linked),
        Some("main".to_owned()),
        "a worktree on `feat` still names `main`"
    );

    let git = |dir: &Path, args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&main, &["branch", "develop"]);
    git(&base, &["clone", "-q", "repo", "clone"]);
    let clone = base.join("clone");
    git(&clone, &["remote", "set-head", "origin", "develop"]);
    git(&clone, &["branch", "develop", "origin/develop"]);
    assert_eq!(
        guild_desktop::git::main_branch(&clone),
        Some("develop".to_owned())
    );
}

/// Exercise the production write door, including git's own refusals. State
/// survives a dirty/locked refusal and never brings a removed tree back.
#[test]
fn worktree_removal_preserves_state_on_refusal_and_drains_it_on_success() {
    use guild_desktop::git::{Write, write};
    use guild_session::{OpenTab, ProjectState, Shows, flush, project_state};

    let base = Scratch::new("gw-state-removal-");
    let (main, linked) = repository(&base);
    let state = ProjectState {
        tabs: vec![OpenTab::showing(Shows::Settings)],
        ..Default::default()
    };
    project_state::save(&linked, &state).unwrap();
    std::fs::write(linked.join("untracked.txt"), "keep me").unwrap();
    let error = write(&main, &Write::WorktreeRemove(linked.clone())).unwrap_err();
    assert!(
        error.to_string().contains("modified or untracked"),
        "{error}"
    );
    assert_eq!(project_state::load(&linked).tabs, state.tabs);
    assert_eq!(
        std::fs::read_to_string(linked.join("untracked.txt")).unwrap(),
        "keep me"
    );
    std::fs::remove_file(linked.join("untracked.txt")).unwrap();

    let locked = Command::new("git")
        .args(["worktree", "lock"])
        .arg(&linked)
        .current_dir(&main)
        .output()
        .unwrap();
    assert!(locked.status.success());
    assert!(write(&main, &Write::WorktreeRemove(linked.clone())).is_err());
    project_state::save(&linked, &state).unwrap();
    write(&main, &Write::WorktreeUnlock(linked.clone())).unwrap();
    write(&main, &Write::WorktreeRemove(linked.clone())).unwrap();
    flush();
    assert!(!linked.exists());
    project_state::save(&linked, &state).unwrap();
    flush();
    assert!(!linked.exists(), "even a late caller cannot resurrect it");
    assert_eq!(std::fs::read_to_string(main.join("a.txt")).unwrap(), "a\n");
}

fn removal_fixture_git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_AUTHOR_NAME", "Fixture")
        .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
        .env("GIT_COMMITTER_NAME", "Fixture")
        .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
        .output()
        .unwrap();
    assert!(result.status.success(), "{args:?}: {result:?}");
    String::from_utf8(result.stdout).unwrap().trim().into()
}

#[test]
fn confirmed_worktree_removal_deletes_only_the_confirmed_branch_and_can_keep_it() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    for delete_branch in [false, true] {
        let base = Scratch::new("gw-confirmed-");
        let (main, linked) = repository(&base);
        let target = Target::read(&main, &linked).unwrap();
        let result = write(
            &main,
            &Write::WorktreeRemoveConfirmed {
                target,
                delete_branch,
            },
        )
        .unwrap();
        assert!(!linked.exists());
        let branches = removal_fixture_git(&main, &["branch", "--format=%(refname:short)"]);
        assert_eq!(
            branches.lines().any(|name| name == "feat"),
            !delete_branch,
            "{}",
            result.text()
        );
        assert!(branches.lines().any(|name| name == "main"));
    }
}

#[test]
fn confirmed_worktree_removal_refuses_dirty_locked_changed_and_unmerged_targets() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    for scenario in ["dirty", "locked", "changed", "unmerged"] {
        let base = Scratch::new("gw-refused-");
        let (main, linked) = repository(&base);
        let target = Target::read(&main, &linked).unwrap();
        match scenario {
            "dirty" => std::fs::write(linked.join("untracked"), "keep").unwrap(),
            "locked" => {
                removal_fixture_git(&main, &["worktree", "lock", linked.to_str().unwrap()]);
            }
            _ => {
                removal_fixture_git(&linked, &["commit", "--allow-empty", "-m", "later"]);
            }
        }
        let target = if scenario == "unmerged" {
            Target::read(&main, &linked).unwrap()
        } else {
            target
        };
        let error = write(
            &main,
            &Write::WorktreeRemoveConfirmed {
                target,
                delete_branch: true,
            },
        )
        .unwrap_err();
        assert_eq!(
            linked.exists(),
            scenario != "unmerged",
            "{scenario}: {error}"
        );
        assert!(
            removal_fixture_git(&main, &["branch", "--format=%(refname:short)"])
                .lines()
                .any(|b| b == "feat")
        );
        if scenario == "unmerged" {
            assert!(
                error
                    .to_string()
                    .contains("Worktree removed; branch deletion was not confirmed"),
                "{error}"
            );
        }
    }
}

#[test]
fn confirmed_worktree_removal_protects_primary_main_master_and_detached_contexts() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    for scenario in ["primary", "main", "master", "detached"] {
        let base = Scratch::new("gw-protected-");
        let (main, linked) = repository(&base);
        if scenario == "main" {
            removal_fixture_git(&main, &["branch", "-m", "trunk"]);
            removal_fixture_git(&linked, &["branch", "-m", "main"]);
        } else if scenario == "master" {
            removal_fixture_git(&linked, &["branch", "-m", "master"]);
        } else if scenario == "detached" {
            removal_fixture_git(&linked, &["switch", "--detach"]);
        }
        let path = if scenario == "primary" {
            &main
        } else {
            &linked
        };
        let target = Target::read(&main, path).unwrap();
        assert!(!target.can_delete_branch());
        assert!(
            write(
                &main,
                &Write::WorktreeRemoveConfirmed {
                    target: target.clone(),
                    delete_branch: true
                }
            )
            .is_err()
        );
        assert!(path.exists());
        let result = write(
            &main,
            &Write::WorktreeRemoveConfirmed {
                target,
                delete_branch: false,
            },
        );
        assert_eq!(
            result.is_ok(),
            scenario != "primary",
            "{scenario}: {result:?}"
        );
    }
}

#[test]
fn confirmed_worktree_removal_rejects_a_switch_even_when_the_commit_is_unchanged() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    let base = Scratch::new("gw-switched-");
    let (main, linked) = repository(&base);
    let target = Target::read(&main, &linked).unwrap();
    removal_fixture_git(&linked, &["switch", "-c", "replacement"]);
    assert!(
        write(
            &main,
            &Write::WorktreeRemoveConfirmed {
                target,
                delete_branch: true
            }
        )
        .is_err()
    );
    assert!(linked.exists());
    let branches = removal_fixture_git(&main, &["branch", "--format=%(refname:short)"]);
    assert!(branches.lines().any(|b| b == "feat"));
    assert!(branches.lines().any(|b| b == "replacement"));
}

#[test]
fn confirmed_worktree_removal_preserves_hooks_and_blocks_concurrent_ref_writes() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    use std::os::unix::fs::PermissionsExt;
    for packed in [false, true] {
        for change in [false, true] {
            let base = Scratch::new("gw-delete-hook-");
            let (main, linked) = repository(&base);
            removal_fixture_git(
                &main,
                &["commit", "--allow-empty", "-m", "later merged commit"],
            );
            let later = removal_fixture_git(&main, &["rev-parse", "HEAD"]);
            if packed {
                removal_fixture_git(&main, &["pack-refs", "--all"]);
            }
            let hooks = base.join("hooks");
            std::fs::create_dir(&hooks).unwrap();
            // Paths in this fixture contain no apostrophes. The real gateway
            // passes identities through env/argv, never interpolated shell code.
            let marker = base.join("changed");
            let log = base.join("hook.log");
            let body = if change {
                format!(
                    "if [ \"$1\" = prepared ] && [ ! -e '{}' ]; then\n : > '{}'\n git -c core.hooksPath=/dev/null update-ref refs/heads/feat '{}'\nfi\n",
                    marker.display(),
                    marker.display(),
                    later
                )
            } else {
                "if [ \"$1\" = prepared ]; then echo 'original hook refuses' >&2; exit 1; fi\n"
                    .into()
            };
            let script = format!(
                "#!/bin/sh\nset -eu\necho \"$1\" >> '{}'\ncat >/dev/null\n{body}",
                log.display()
            );
            let hook = hooks.join("reference-transaction");
            std::fs::write(&hook, script).unwrap();
            std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o700)).unwrap();
            removal_fixture_git(
                &main,
                &["config", "core.hooksPath", hooks.to_str().unwrap()],
            );
            let target = Target::read(&main, &linked).unwrap();
            let result = write(
                &main,
                &Write::WorktreeRemoveConfirmed {
                    target,
                    delete_branch: true,
                },
            );
            assert!(!linked.exists());
            assert!(log.is_file(), "the original configured hook still runs");
            if change {
                let error = result.unwrap_err();
                assert!(marker.is_file());
                assert!(error.to_string().contains("cannot lock ref"), "{error}");
                assert_ne!(
                    removal_fixture_git(&main, &["rev-parse", "refs/heads/feat"]),
                    later,
                    "the concurrent replacement was blocked"
                );
            } else {
                let error = result.unwrap_err();
                assert!(
                    error
                        .to_string()
                        .contains("Worktree removed; branch deletion was not confirmed"),
                    "{error}"
                );
                assert!(
                    error.to_string().contains("original hook refuses"),
                    "{error}"
                );
                assert!(!removal_fixture_git(&main, &["rev-parse", "refs/heads/feat"]).is_empty());
            }
        }
    }
}

#[test]
fn confirmed_worktree_removal_never_substitutes_lossy_paths_or_branch_names() {
    use guild_desktop::git::{Write, worktree_removal::Target, write};
    use std::os::unix::ffi::OsStringExt;
    let base = Scratch::new("gw-byte-identity-");
    let (main, linked) = repository(&base);
    let raw_path = base.join(std::ffi::OsString::from_vec(b"tree-\xff".to_vec()));
    let replacement_path = base.join("tree-�");
    removal_fixture_git(
        &main,
        &[
            "worktree",
            "add",
            "-b",
            "replacement",
            replacement_path.to_str().unwrap(),
        ],
    );
    let mut target = Target::read(&main, &linked).unwrap();
    target.path = raw_path;
    // macOS refuses creating a non-UTF-8 filename (EILSEQ). The write gate
    // must reject such a request before filesystem/git reads or lossy argv.
    let error = write(
        &main,
        &Write::WorktreeRemoveConfirmed {
            target,
            delete_branch: false,
        },
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("cannot be represented safely"),
        "{error}"
    );
    assert!(linked.exists() && replacement_path.exists());

    // A replacement-character label cannot establish whether the original
    // cached ref was UTF-8. Conservatively permit folder-only deletion.
    removal_fixture_git(&linked, &["branch", "-m", "feature-�"]);
    let target = Target::read(&main, &linked).unwrap();
    assert!(!target.can_delete_branch());
    assert!(
        write(
            &main,
            &Write::WorktreeRemoveConfirmed {
                target: target.clone(),
                delete_branch: true
            }
        )
        .is_err()
    );
    write(
        &main,
        &Write::WorktreeRemoveConfirmed {
            target,
            delete_branch: false,
        },
    )
    .unwrap();
    assert!(!linked.exists() && replacement_path.exists());
    assert!(
        !removal_fixture_git(&main, &["rev-parse", "--verify", "refs/heads/feature-�"]).is_empty()
    );
}

// --- GLD-063: where a worktree's branch would be pushed, and `↑N`

/// `git` in a fixture, its output when it must succeed.
fn fixture_git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A clone of a bare `origin` with `main` pushed, and a linked worktree on
/// `feat`; the folder is the test's to hold.
fn cloned(base: &Path) -> (PathBuf, PathBuf, String) {
    fixture_git(base, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    let url = base.join("origin.git").display().to_string();
    fixture_git(base, &["clone", "-q", &url, "repo"]);
    let main = base.join("repo");
    std::fs::write(main.join("a.txt"), "a\n").unwrap();
    fixture_git(&main, &["add", "a.txt"]);
    fixture_git(&main, &["commit", "-qm", "init"]);
    fixture_git(&main, &["push", "-q", "origin", "main"]);
    fixture_git(
        &main,
        &["worktree", "add", "-q", "-b", "feat", "../repo-feat"],
    );
    (main, base.join("repo-feat"), url)
}

fn commit(dir: &Path, name: &str) {
    std::fs::write(dir.join(name), name).unwrap();
    fixture_git(dir, &["add", name]);
    fixture_git(dir, &["commit", "-qm", name]);
}

fn listed_with_destinations(main: &Path) -> Vec<Worktree> {
    let mut list = guild_desktop::git::worktrees::list(main).expect("git lists");
    guild_desktop::git::worktrees::destinations(main, &mut list).expect("git answers");
    list
}

fn on<'a>(list: &'a [Worktree], branch: &str) -> &'a Worktree {
    list.iter()
        .find(|tree| tree.branch.as_deref() == Some(branch))
        .unwrap_or_else(|| panic!("{branch} in {list:#?}"))
}

#[test]
fn an_upstream_names_the_destination_and_counts_what_it_lacks() {
    let base = Scratch::new("gw-ahead-");
    let (main, linked, url) = cloned(&base);
    commit(&linked, "one");
    fixture_git(&linked, &["push", "-q", "-u", "origin", "feat"]);
    commit(&linked, "two");
    commit(&linked, "three");
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    assert_eq!(feat.ahead, Some(2));
    let push = feat.push.as_ref().expect("a destination");
    assert_eq!(push.remote, "origin");
    assert_eq!(push.branch, "feat");
    assert_eq!(push.url.as_deref(), Some(url.as_str()));
    assert_eq!(on(&list, "main").ahead, Some(0));
}

#[test]
fn a_branch_tracking_another_name_is_looked_up_by_that_name() {
    let base = Scratch::new("gw-onto-");
    let (main, linked, _) = cloned(&base);
    commit(&linked, "one");
    fixture_git(&linked, &["push", "-q", "origin", "feat:foo"]);
    fixture_git(&linked, &["branch", "-q", "--set-upstream-to=origin/foo"]);
    commit(&linked, "two");
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    assert_eq!(feat.push.as_ref().unwrap().branch, "foo");
    assert_eq!(feat.ahead, Some(1));
}

/// bugs+impl-8 of code review r1: a tag named like the branch makes git's
/// short refname `heads/bar`; the upstream is still found.
#[test]
fn a_tag_named_like_the_branch_does_not_hide_its_upstream() {
    let base = Scratch::new("gw-tagged-");
    let (main, linked, _) = cloned(&base);
    commit(&linked, "one");
    fixture_git(&linked, &["push", "-q", "origin", "feat:foo"]);
    fixture_git(&linked, &["branch", "-q", "--set-upstream-to=origin/foo"]);
    fixture_git(&linked, &["tag", "feat"]);
    commit(&linked, "two");
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    assert_eq!(feat.push.as_ref().unwrap().branch, "foo");
    assert_eq!(feat.ahead, Some(1));
}

/// bugs+impl-4 of code review r2: a remote that fetches from a mirror and
/// pushes to GitLab is looked up where it pushes; `↑N` still counts against
/// the local tracking ref.
#[test]
fn the_merge_request_is_looked_up_where_the_branch_pushes() {
    let base = Scratch::new("gw-pushurl-");
    let (main, linked, _) = cloned(&base);
    commit(&linked, "one");
    fixture_git(&linked, &["push", "-q", "-u", "origin", "feat"]);
    commit(&linked, "two");
    fixture_git(
        &main,
        &[
            "remote",
            "set-url",
            "--push",
            "origin",
            "git@gitlab.example.com:team/app.git",
        ],
    );
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    let pushed = feat.push.as_ref().unwrap();
    assert_eq!(
        pushed.url.as_deref(),
        Some("git@gitlab.example.com:team/app.git")
    );
    assert_eq!(pushed.branch, "feat");
    assert_eq!(feat.ahead, Some(1));
}

#[test]
fn no_upstream_counts_what_no_ref_of_the_remote_holds() {
    let base = Scratch::new("gw-noup-");
    let (main, linked, _) = cloned(&base);
    commit(&linked, "one");
    // Pushed under another name, without `-u`: nothing is unpushed.
    fixture_git(&linked, &["push", "-q", "origin", "feat:review"]);
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    assert_eq!(feat.ahead, Some(0), "origin/review holds it");
    let push = feat.push.as_ref().unwrap();
    assert_eq!(
        (push.remote.as_str(), push.branch.as_str()),
        ("origin", "feat")
    );
    // Two more, never pushed.
    commit(&linked, "two");
    commit(&linked, "three");
    let list = listed_with_destinations(&main);
    assert_eq!(on(&list, "feat").ahead, Some(2));
}

#[test]
fn no_remote_or_a_detached_head_has_no_destination_and_no_count() {
    let base = Scratch::new("gw-noremote-");
    let (main, linked) = repository(&base);
    commit(&linked, "one");
    let list = listed_with_destinations(&main);
    let feat = on(&list, "feat");
    assert_eq!(feat.push, None);
    assert_eq!(feat.ahead, None);

    let base = Scratch::new("gw-detached-");
    let (main, linked, _) = cloned(&base);
    fixture_git(&linked, &["checkout", "-q", "--detach"]);
    let list = listed_with_destinations(&main);
    let detached = list
        .iter()
        .find(|tree| tree.path.ends_with("repo-feat"))
        .unwrap();
    assert!(detached.detached);
    assert_eq!(detached.push, None);
    assert_eq!(detached.ahead, None);
}
