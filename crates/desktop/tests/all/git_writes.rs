//! The commands and path input of the writes the window makes. Argument
//! assertions pin their spelling; real Git checks transport and identity
//! boundaries. The finishing checklist also guards the named verbs.
//!
//! Decision 9 of M8.3 said there were three and no more. **Decision 21
//! of the spec lifted the count on 2026-09-04** — the window does git
//! as an environment does it — and put a rule about guards in its
//! place. What is pinned here is each verb, and that the destructive
//! spellings are not among them.
//!
//! **`changes::rollback_of` is tested here too**, though its subject is
//! a row of the Commit panel: the question it answers is *which write*,
//! and the whole point of the answer is that two of the three are not
//! the rollback at all.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::changes::{Listed, Row, Section, rollback_of};
use guild_desktop::git::{Destination, PushTags, Write};
use guild_desktop::theme::Mark;

fn paths(list: &[&str]) -> Vec<PathBuf> {
    list.iter().map(PathBuf::from).collect()
}

/// A row of the Commit panel, as `changes::list` would have built it.
fn listed(section: Section, path: &str, mark: Mark, from: Option<&str>) -> Listed {
    Listed {
        section,
        row: Row {
            path: PathBuf::from(path),
            mark,
            from: from.map(PathBuf::from),
            is_dir: false,
            unmerged: false,
            staged: false,
        },
    }
}

fn hunk_sample() -> guild_desktop::git::Hunk {
    guild_desktop::git::Hunk {
        path: PathBuf::from("a"),
        preimage: std::sync::Arc::from(&b"a\n"[..]),
        postimage: std::sync::Arc::from(&b"b\n"[..]),
        disk: None,
        head: std::sync::Arc::from(&b"a\n"[..]),
    }
}

/// One of every write there is.
///
/// **The `match` is not for its answer; it is for the exhaustiveness.**
/// A hand-written array of writes is a list that goes stale silently,
/// and it did: `no_write_carries_a_destructive_spelling` was checking
/// seven of the ten, so `Stage`, `Unstage` and `Commit` were never
/// looked at. A new variant of `git::Write` now fails this file to
/// compile, which is the only kind of reminder that cannot be missed.
fn one_of_each() -> Vec<Write> {
    let sample = Write::Stage(paths(&["a"]));
    match &sample {
        Write::Stage(_)
        | Write::Clone { .. }
        | Write::Unstage(_)
        | Write::Rollback(_)
        | Write::Commit { .. }
        | Write::Switch(_)
        | Write::Create(_)
        | Write::DeleteBranch(_)
        | Write::ApplyStash(_)
        | Write::DropStash { .. }
        | Write::Push { .. }
        | Write::WorktreeAdd { .. }
        | Write::WorktreeRemoveConfirmed { .. }
        | Write::WorktreeRemove(_)
        | Write::WorktreeUnlock(_)
        | Write::FetchSource { .. }
        | Write::MergeSource { .. }
        | Write::FastForwardBranch { .. }
        | Write::FastForwardCheckout { .. }
        | Write::StageHunk(_)
        | Write::UnstageHunk(_) => {}
    }
    vec![
        Write::FetchSource {
            remote: "origin".into(),
            refspec: "refs/heads/main:refs/remotes/origin/main".into(),
            refuse_forced: false,
        },
        Write::MergeSource {
            oid: "abcdef".into(),
            message: "Merge source".into(),
        },
        Write::Clone {
            url: "https://example.test/repo.git".into(),
            destination: "repo".into(),
        },
        Write::Stage(paths(&["a"])),
        Write::Unstage(paths(&["a"])),
        Write::Rollback(paths(&["a"])),
        Write::Commit {
            message_file: PathBuf::from("m"),
        },
        Write::Switch("main".to_owned()),
        Write::Create("feature".to_owned()),
        Write::DeleteBranch("spike".to_owned()),
        Write::ApplyStash("stash@{0}".to_owned()),
        Write::DropStash {
            reference: "stash@{0}".to_owned(),
            sha: "708c4c945349fbd9c22c91456a50d96a0bfafa20".to_owned(),
        },
        Write::WorktreeAdd {
            path: PathBuf::from("/w/guild-lazy-lsp"),
            branch: "lazy-lsp".to_owned(),
            base: Some("main".to_owned()),
        },
        Write::WorktreeRemoveConfirmed {
            target: guild_desktop::git::worktree_removal::Target {
                repository: "/w/.git".into(),
                path: "/w/tree".into(),
                branch: Some("feature".into()),
                head: Some("0123456789012345678901234567890123456789".into()),
            },
            delete_branch: true,
        },
        Write::WorktreeRemove(PathBuf::from("/w/guild-lazy-lsp")),
        Write::WorktreeUnlock(PathBuf::from("/w/guild-lazy-lsp")),
        Write::push(&Destination {
            remote: "origin".to_owned(),
            branch: "main".to_owned(),
            onto: "main".to_owned(),
            set_upstream: false,
        }),
        Write::FetchSource {
            remote: "origin".into(),
            refspec: "refs/heads/main:refs/remotes/origin/main".into(),
            refuse_forced: true,
        },
        Write::FastForwardBranch {
            branch: "main".into(),
            oid: "0123456789012345678901234567890123456789".into(),
        },
        Write::FastForwardCheckout {
            branch: "main".into(),
            oid: "0123456789012345678901234567890123456789".into(),
        },
        Write::StageHunk(hunk_sample()),
        Write::UnstageHunk(hunk_sample()),
    ]
}

/// The actual Stage command reads literal paths from NUL-delimited stdin.
#[test]
fn staging_is_literal_git_add_with_nul_pathspec_input() {
    let write = Write::Stage(paths(&["src/lib.rs", "-n", "with space/file name.md"]));
    assert_eq!(
        write.args(),
        [
            "--literal-pathspecs",
            "add",
            "--pathspec-from-file=-",
            "--pathspec-file-nul"
        ]
    );
}

/// `git restore --staged -- <paths>`: the index entries back to `HEAD`,
/// the working tree untouched.
#[test]
fn unstaging_is_git_restore_staged() {
    let write = Write::Unstage(paths(&["src/lib.rs"]));
    assert_eq!(
        write.args(),
        [
            "--literal-pathspecs",
            "restore",
            "--staged",
            "--pathspec-from-file=-",
            "--pathspec-file-nul"
        ]
    );
}

/// `git restore --source=HEAD --staged --worktree -- <paths>`: the
/// paths back to what `HEAD` holds, the index and the working tree
/// together — Rollback.
///
/// **One verb, not `checkout`.** `git checkout HEAD -- <paths>` would do
/// the same, and it is the same gesture `switch` was chosen over in
/// M8.3.3: `checkout` does five other things, and restoring a path is
/// one of them. `restore` does nothing else, so the verb is the guard.
///
/// **`--source=HEAD` is not decoration.** `restore --staged` alone puts
/// the index back to `HEAD` and `restore --worktree` alone puts the
/// disk back to the *index*; naming the source is what makes both
/// halves say the same thing.
#[test]
fn rolling_back_is_restore_from_head_into_both_the_index_and_the_disk() {
    let write = Write::Rollback(paths(&["src/lib.rs", "docs/plan.md"]));
    assert_eq!(
        write.args(),
        [
            "--literal-pathspecs",
            "restore",
            "--source=HEAD",
            "--staged",
            "--worktree",
            "--pathspec-from-file=-",
            "--pathspec-file-nul"
        ]
    );
    assert!(
        !write.args().iter().any(|word| word == "checkout"),
        "checkout does more than this gesture means"
    );
}

/// **A modified row is the rollback, and a renamed one carries both of
/// its paths.**
///
/// `restore` given only the new name puts that name back to what `HEAD`
/// has — which is nothing — and leaves the old one deleted in the
/// index: a half-undone rename, worse than either end of it. Measured
/// against a real git, not reasoned about.
#[test]
fn rolling_back_a_rename_names_the_path_it_came_from_as_well() {
    let list = [
        listed(Section::Changes, "src/lib.rs", Mark::Modified, None),
        listed(
            Section::Changes,
            "src/new-name.rs",
            Mark::Modified,
            Some("src/old-name.rs"),
        ),
    ];
    assert_eq!(
        rollback_of(&list, Path::new("src/lib.rs")),
        Some(Write::Rollback(paths(&["src/lib.rs"])))
    );
    assert_eq!(
        rollback_of(&list, Path::new("src/new-name.rs")),
        Some(Write::Rollback(paths(&[
            "src/new-name.rs",
            "src/old-name.rs"
        ])))
    );
}

/// **An added row is unstaged and the file stays**, which is the expected
/// default and not a softening of it.
///
/// `restore --source=HEAD --worktree` on a path `HEAD` does not have
/// reads as *there should be nothing here* and deletes the file from
/// the disk. A person who rolls back a file they wrote this morning
/// means *take it out of the commit*, not *throw the morning away*.
#[test]
fn rolling_back_an_added_row_only_unstages_it() {
    let list = [listed(Section::Changes, "src/new.rs", Mark::Added, None)];
    assert_eq!(
        rollback_of(&list, Path::new("src/new.rs")),
        Some(Write::Unstage(paths(&["src/new.rs"])))
    );
}

/// **A conflicted row is not rolled back**, and the menu offers no item.
///
/// `restore --source=HEAD --staged --worktree` on an unmerged path
/// *succeeds*: it writes `HEAD`'s side into the index and the worktree,
/// which takes the path out of the conflicted state — so the merge in
/// progress is left standing with the incoming side silently gone and
/// nothing anywhere saying it went. That is not what *Rollback* says it
/// does; a person reads it as *undo my changes*, and it would be
/// *resolve this in favour of ours*, which is a different word and a
/// different gesture.
///
/// Where an unversioned row is refused because git would refuse it, this
/// one is refused because git would **not**: the guard has to be ours.
/// The merge pass decision 21 named is where resolving belongs.
#[test]
fn a_conflicted_row_is_never_rolled_back() {
    let mut conflicted = listed(Section::Changes, "src/lib.rs", Mark::Modified, None);
    conflicted.row.unmerged = true;
    let list = [
        conflicted,
        listed(Section::Changes, "src/other.rs", Mark::Modified, None),
    ];
    assert_eq!(
        rollback_of(&list, Path::new("src/lib.rs")),
        None,
        "restoring a conflicted path resolves the conflict, which Rollback does not say"
    );
    assert_eq!(
        rollback_of(&list, Path::new("src/other.rs")),
        Some(Write::Rollback(paths(&["src/other.rs"]))),
        "and the row beside it is untouched by the rule"
    );
}

/// **An unversioned row has nothing to go back to**, so there is no
/// write and the menu offers no item: git would refuse the path outright
/// — *pathspec did not match any file known to git* — and a sheet whose
/// only possible ending is a refusal is worse than no sheet.
#[test]
fn an_unversioned_row_is_never_rolled_back() {
    let list = [listed(
        Section::Unversioned,
        "notes.txt",
        Mark::Untracked,
        None,
    )];
    assert_eq!(rollback_of(&list, Path::new("notes.txt")), None);
    assert_eq!(
        rollback_of(&list, Path::new("gone-since.txt")),
        None,
        "a row that left the list under the pointer is nothing to act on"
    );
}

/// A group's *all* is one write with every path, not one write per
/// row: one process, one walk after it.
#[test]
fn a_group_is_one_write_with_every_path() {
    let write = Write::Stage(paths(&["a.rs", "b.rs", "c/d.rs"]));
    assert_eq!(
        write.pathspecs().unwrap(),
        paths(&["a.rs", "b.rs", "c/d.rs"])
    );
    assert_eq!(write.args().len(), 4);
    let write = Write::Unstage(paths(&["a.rs", "b.rs"]));
    assert_eq!(write.pathspecs().unwrap(), paths(&["a.rs", "b.rs"]));
    assert_eq!(write.args().len(), 5);
}

/// Cyrillic is valid UTF-8 and remains the exact path identity. Unsupported
/// byte sequences are tested separately at the actual execution boundary.
#[test]
fn a_non_ascii_path_is_passed_as_it_is() {
    let write = Write::Stage(paths(&["docs/файл.md"]));
    assert_eq!(write.pathspecs().unwrap(), paths(&["docs/файл.md"]));
}

/// `git commit -F <file> --cleanup=whitespace`: the message from a file,
/// so that quotes, newlines and `#` lines arrive intact, and the one
/// cleanup that keeps a line starting with `#` — the default strips it
/// as a comment.
#[test]
fn a_commit_reads_its_message_from_a_file_and_keeps_hash_lines() {
    let write = Write::Commit {
        message_file: PathBuf::from(".guild/COMMIT_MESSAGE"),
    };
    assert_eq!(
        write.args(),
        [
            "commit",
            "-F",
            ".guild/COMMIT_MESSAGE",
            "--cleanup=whitespace"
        ]
    );
}

/// Since M8.3.3 the window switches branches, which decision 21 of the
/// spec turned on. The verb is `switch` and never `checkout`: `checkout`
/// does five other things besides changing branch, one of them the
/// destructive restore of a path, while `switch` refuses everything but
/// a branch — the verb is the guard.
#[test]
fn switching_a_branch_says_switch_and_names_the_branch() {
    let args = Write::Switch("m8-3-3-branches".to_owned()).args();
    assert_eq!(args, ["switch", "m8-3-3-branches"]);
    assert!(
        !args.iter().any(|word| word == "checkout"),
        "checkout does more than this gesture means"
    );
    assert!(
        !args.iter().any(|word| word.starts_with('-')),
        "no flags at all: a name is a name"
    );
}

/// Creating a branch is the same verb with one flag, and the name is
/// the only thing in any write here that a person typed. What the
/// popup would not offer, `branch_list::offers_create` keeps out of the
/// list; what slips through is git's to refuse, in git's words.
#[test]
fn creating_a_branch_is_switch_with_c() {
    let args = Write::Create("feature/what-a-person-types".to_owned()).args();
    assert_eq!(args, ["switch", "-c", "feature/what-a-person-types"]);
    assert!(
        !args.iter().any(|word| word == "-C" || word == "--force"),
        "nothing here overwrites a branch that exists"
    );
}

/// `git branch -d <name>`: the branch's name removed, and only where git
/// agrees the work is not lost.
///
/// **`-d`, never `-D`.** The lower case refuses a branch whose commits
/// are on no other, and that refusal is the whole guard: the window does
/// not judge what is merged, git does. The upper case is the destructive
/// spelling and appears nowhere — there is a test below that greps for
/// it, because a flag is one keystroke from its opposite.
#[test]
fn deleting_a_branch_is_lower_case_d() {
    let write = Write::DeleteBranch("spike/old".to_owned());
    assert_eq!(write.args(), ["branch", "-d", "--", "spike/old"]);
}

/// `git stash apply <ref>`: the entry's changes on top of the working
/// tree, and the entry still in the list.
///
/// **`apply`, not `pop`.** `pop` drops the entry when it succeeds, so a
/// conflict half-applied would leave a person with a mess and no stash
/// to go back to. Dropping is its own gesture with its own bin.
#[test]
fn applying_a_stash_keeps_it() {
    let write = Write::ApplyStash("stash@{2}".to_owned());
    assert_eq!(write.args(), ["stash", "apply", "stash@{2}"]);
}

/// `git stash drop <ref>`: the entry gone. The commit rides along and is
/// not an argument — git takes only a position (task 0.1 of M5.11
/// measured *is not a stash reference* for a hash) — so `git::settle`
/// finds the position the commit has at the moment of the drop.
#[test]
fn dropping_a_stash_names_the_reference_git_printed() {
    let write = Write::DropStash {
        reference: "stash@{0}".to_owned(),
        sha: "708c4c945349fbd9c22c91456a50d96a0bfafa20".to_owned(),
    };
    assert_eq!(write.args(), ["stash", "drop", "stash@{0}"]);
}

/// A scratch repository with a commit and one stash per subject, oldest
/// first — so the last is `stash@{0}`. **These two tests run git**, which
/// the rest of this file does not: what they pin is that a drop reaches
/// the entry the person was shown after the list moved, and that is
/// git's list moving, not ours. The stashes are made with `stash push`,
/// which is not a write of the crate's; every drop is the crate's own.
fn stash_repository(name: &str, subjects: &[&str]) -> Scratch {
    let dir = Scratch::new(&format!("guild-stash-drop-{name}-"));
    let git = |args: &[&str]| {
        guild_desktop::git::run(&dir, args).unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Guild"]);
    git(&["config", "user.email", "guild@example.invalid"]);
    std::fs::write(dir.join("a.txt"), "base\n").unwrap();
    git(&["add", "a.txt"]);
    git(&["commit", "-q", "-m", "base"]);
    for subject in subjects {
        push_stash(&dir, subject);
    }
    dir
}

fn push_stash(dir: &Path, subject: &str) {
    std::fs::write(dir.join("a.txt"), format!("{subject}\n")).unwrap();
    guild_desktop::git::run(dir, &["stash", "push", "-q", "-m", subject]).expect("git stash push");
}

fn listed_stashes(dir: &Path) -> Vec<guild_desktop::git::stashes::Stash> {
    let bytes = guild_desktop::git::stashes(dir).expect("the stash list");
    guild_desktop::git::stashes::parse(&bytes).expect("the stash format")
}

fn subjects(dir: &Path) -> Vec<String> {
    listed_stashes(dir)
        .into_iter()
        .map(|stash| stash.subject.trim_start_matches("On main: ").to_owned())
        .collect()
}

/// **A drop takes the entry that was shown, not the position it had.**
/// The list is read with *one* and *two*; *three* is pushed after it,
/// which moves *two* from `stash@{0}` to `stash@{1}`; the drop of what
/// was shown as `stash@{0}` then takes *two* and leaves *three*.
#[test]
fn a_drop_by_commit_removes_the_entry_that_was_shown() {
    use guild_desktop::git::{Settled, settle, write};
    let dir = stash_repository("moved", &["one", "two"]);
    let shown = listed_stashes(&dir);
    let two = shown
        .iter()
        .find(|s| s.subject.ends_with("two"))
        .unwrap()
        .clone();
    assert_eq!(two.reference, "stash@{0}");

    push_stash(&dir, "three");
    let asked = Write::DropStash {
        reference: two.reference.clone(),
        sha: two.sha.clone(),
    };
    let Settled::Ready(ready) = settle(&dir, asked) else {
        panic!("the entry is still listed");
    };
    assert_eq!(ready.args(), ["stash", "drop", "stash@{1}"]);
    write(&dir, &ready).expect("git dropped it");
    assert_eq!(subjects(&dir), ["three", "one"]);
}

/// **An entry gone since the list was read is not dropped, and neither is
/// its neighbour.** *two* is dropped by another hand; the drop the person
/// asked for comes back `StashGone` without running, and *one* — which
/// now holds *two*'s old position — is still there.
#[test]
fn a_drop_whose_entry_is_gone_is_not_run() {
    use guild_desktop::git::{Settled, settle, write};
    let dir = stash_repository("gone", &["one", "two"]);
    let two = listed_stashes(&dir)
        .into_iter()
        .find(|s| s.subject.ends_with("two"))
        .unwrap();
    let asked = Write::DropStash {
        reference: two.reference.clone(),
        sha: two.sha.clone(),
    };
    write(&dir, &asked).expect("dropped by another hand");
    assert_eq!(subjects(&dir), ["one"]);

    match settle(&dir, asked) {
        Settled::StashGone(Write::DropStash { sha, .. }) => assert_eq!(sha, two.sha),
        other => panic!("expected the entry to be gone, got {other:?}"),
    }
    assert_eq!(subjects(&dir), ["one"], "nothing was dropped");
}

/// Any write but a drop comes back from `settle` as it went in, and git is
/// not asked anything for it — not even in a directory that is no
/// repository at all.
#[test]
fn a_write_that_is_not_a_drop_settles_as_it_is() {
    use guild_desktop::git::{Settled, settle};
    let nowhere = Scratch::absent("guild-settle-");
    let write = Write::ApplyStash("stash@{0}".to_owned());
    match settle(&nowhere, write.clone()) {
        Settled::Ready(ready) => assert_eq!(ready, write),
        other => panic!("{other:?}"),
    }
}

/// A destination whose two branch names agree — the ordinary case —
/// is `push <remote> <branch>`, and nothing else at all.
#[test]
fn a_push_names_the_remote_and_the_branch_and_carries_no_force() {
    let args = Write::push(&Destination {
        remote: "origin".to_owned(),
        branch: "main".to_owned(),
        onto: "main".to_owned(),
        set_upstream: false,
    })
    .args();
    assert_eq!(args, ["push", "origin", "main"]);
    assert!(
        !args
            .iter()
            .any(|word| word.contains("force") || word == "-f"),
        "no force in any spelling: {args:?}"
    );
}

/// **The one bool the write takes.** A branch with no upstream on this
/// remote is pushed with `--set-upstream`, so the next push needs no
/// argument at all — which is what the head says with *Set upstream*.
#[test]
fn a_branch_with_no_upstream_is_pushed_with_set_upstream() {
    let args = Write::push(&Destination {
        remote: "origin".to_owned(),
        branch: "m8-8-4-git".to_owned(),
        onto: "m8-8-4-git".to_owned(),
        set_upstream: true,
    })
    .args();
    assert_eq!(args, ["push", "--set-upstream", "origin", "m8-8-4-git"]);
}

/// **Tags are an argument of the push, not a write of their own** —
/// M8.3.6, *Push tags*: `--tags` for all of them, `--follow-tags`
/// for those on the commits being pushed, nothing when the box is clear.
#[test]
fn tags_are_an_argument_of_push() {
    let to = Destination {
        remote: "origin".to_owned(),
        branch: "main".to_owned(),
        onto: "main".to_owned(),
        set_upstream: false,
    };
    assert_eq!(Write::push(&to).args(), ["push", "origin", "main"]);
    assert_eq!(
        Write::push_with(&to, PushTags::All).args(),
        ["push", "--tags", "origin", "main"]
    );
    assert_eq!(
        Write::push_with(&to, PushTags::CurrentBranch).args(),
        ["push", "--follow-tags", "origin", "main"]
    );
}

/// The unchecked hooks box affects only this push; ordinary callers
/// still run the repository's pre-push hook.
#[test]
fn skipping_hooks_is_explicit_and_does_not_leak_to_the_next_push() {
    use std::os::unix::fs::PermissionsExt;
    let folder = Scratch::new("guild-push-hooks-");
    let work = folder.join("work");
    let remote = folder.join("remote.git");
    std::fs::create_dir_all(&work).unwrap();
    std::fs::create_dir_all(&remote).unwrap();
    guild_desktop::git::run(&remote, &["init", "-q", "--bare"]).unwrap();
    guild_desktop::git::run(&work, &["init", "-q", "-b", "main"]).unwrap();
    guild_desktop::git::run(&work, &["config", "user.name", "Guild"]).unwrap();
    guild_desktop::git::run(&work, &["config", "user.email", "guild@example.invalid"]).unwrap();
    guild_desktop::git::run(
        &work,
        &["remote", "add", "origin", &remote.to_string_lossy()],
    )
    .unwrap();
    std::fs::write(work.join("file"), "contents\n").unwrap();
    guild_desktop::git::run(&work, &["add", "file"]).unwrap();
    guild_desktop::git::run(&work, &["commit", "-q", "-m", "one"]).unwrap();
    let hook = work.join(".git/hooks/pre-push");
    std::fs::write(
        &hook,
        "#!/bin/sh\necho 'blocked by test hook' >&2\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let to = Destination {
        remote: "origin".to_owned(),
        branch: "main".to_owned(),
        onto: "main".to_owned(),
        set_upstream: true,
    };
    assert!(guild_desktop::git::write(&work, &Write::push(&to)).is_err());
    let bypass = Write::push_with_hooks(&to, PushTags::None, false);
    assert!(bypass.args().contains(&"--no-verify".to_owned()));
    guild_desktop::git::write(&work, &bypass).expect("the explicit hook bypass");
    assert!(guild_desktop::git::run(&remote, &["rev-parse", "refs/heads/main"]).is_ok());
    assert!(!Write::push(&to).args().contains(&"--no-verify".to_owned()));
}

/// **A branch that tracks another name is pushed as a pair**, because
/// `push origin bar` means `bar:bar` whatever `bar` tracks: a branch
/// tracking `origin/foo` would otherwise grow a second remote branch
/// and leave the one it tracks behind, and nothing would say so.
#[test]
fn a_push_onto_another_name_spells_both_sides() {
    let args = Write::push(&Destination {
        remote: "origin".to_owned(),
        branch: "bar".to_owned(),
        onto: "foo".to_owned(),
        set_upstream: false,
    })
    .args();
    assert_eq!(args, ["push", "origin", "bar:foo"]);
}

/// **The guard the finishing checklist greps for.** Every destructive
/// spelling git has near these verbs is absent: `-D` deletes an unmerged
/// branch, `--force` overwrites one that exists, `pop` drops what it
/// applied, and `clear` empties the stash entirely.
///
/// **It walks every write there is**, through `one_of_each`, whose
/// `match` the compiler keeps complete. It used to walk a hand-written
/// seven of the ten — `Stage`, `Unstage` and `Commit` were outside it —
/// which is the same hole in a different place as the one below.
///
/// **The match is by substring, and that is what bars
/// `--force-with-lease`.** M8.8's plan said the `--force` row already
/// did — it did not: `["…"].contains(&word.as_str())` is equality, so
/// the one spelling of a force push that a person is likeliest to reach
/// for would have walked straight through the guard that exists to stop
/// it. Every write in this crate is checked against the substring form
/// above; nothing any of them spells contains one of these six.
#[test]
fn no_write_carries_a_destructive_spelling() {
    let mut writes = one_of_each();
    // The one that takes a flag of its own, so `--set-upstream` is
    // walked as well as the plain form.
    writes.push(Write::push(&Destination {
        remote: "origin".to_owned(),
        branch: "main".to_owned(),
        onto: "other".to_owned(),
        set_upstream: true,
    }));
    for write in writes {
        for word in write.args() {
            if let Some(spelling) = destructive(&word) {
                panic!("{word:?} carries {spelling:?}, a spelling that loses work");
            }
        }
    }
}

/// Which spelling that loses work a word of an argv carries, if any.
fn destructive(word: &str) -> Option<&'static str> {
    // A `+` in front of a refspec forces it, which no write may do.
    if word.starts_with('+') {
        return Some("+");
    }
    // `-B` is M12's: `worktree add -B` resets a branch that exists, which
    // loses the commits on it, and `add -b` is the spelling that refuses
    // instead.
    ["-D", "-B", "--force", "-f", "pop", "clear", "--hard"]
        .into_iter()
        .find(|spelling| {
            // These exact stdin options contain "-f" in "-from"/"-file";
            // neither is force. `--ff-only` contains "-f" too, and is the
            // opposite of a force: it refuses everything but a
            // fast-forward — decision 21's auto fast-forward. Exact words
            // only; every other spelling keeps the substring guard.
            let allowed = *spelling == "-f"
                && matches!(
                    word,
                    "--pathspec-from-file=-" | "--pathspec-file-nul" | "--ff-only"
                );
            !allowed && word.contains(spelling)
        })
}

/// **The guard's allowances are exact words, not patterns.** The spellings
/// beside them that do lose work are still caught.
#[test]
fn the_guard_allows_only_its_exact_words() {
    for word in [
        "--force-with-lease",
        "-f",
        "--ff",
        "--ff-only=x",
        "--ff-onlyx",
        "--pathspec-from-file=x",
        "+refs/heads/main:refs/heads/main",
    ] {
        assert!(destructive(word).is_some(), "{word:?} would pass the guard");
    }
    for word in ["--ff-only", "--pathspec-from-file=-", "--pathspec-file-nul"] {
        assert_eq!(destructive(word), None, "{word:?} is an allowed exact word");
    }
}

/// **Auto fast-forward's three writes, spelled once.** The person's git
/// config is not trusted to keep a fast-forward one, so the argv says it:
/// `fetch.showForcedUpdates=true` keeps git's non-fast-forward refusal on
/// (off, a local fetch replaces a diverged branch), `--refmap=` stops a
/// configured fetch refspec moving another ref, and the emptied
/// `mergeOptions`, `--no-squash --commit` and `--strategy=ort` stop a
/// configured `--squash` or `-s ours` (or `pull.twohead=ours`) turning
/// `--ff-only` into a squash or a merge commit — each measured on git 2.50.1.
#[test]
fn auto_fast_forward_writes_say_on_the_argv_what_config_could_undo() {
    let oid = "0123456789012345678901234567890123456789";
    assert_eq!(
        Write::FetchSource {
            remote: "origin".into(),
            refspec: "refs/heads/main:refs/remotes/origin/main".into(),
            refuse_forced: true,
        }
        .args(),
        [
            "-c",
            "fetch.showForcedUpdates=true",
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-recurse-submodules",
            "--refmap=",
            "--",
            "origin",
            "refs/heads/main:refs/remotes/origin/main"
        ]
    );
    assert_eq!(
        Write::FastForwardBranch {
            branch: "main".into(),
            oid: oid.into(),
        }
        .args(),
        [
            "-c",
            "fetch.showForcedUpdates=true",
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-recurse-submodules",
            "--refmap=",
            "--",
            ".",
            &format!("{oid}:refs/heads/main")
        ]
    );
    assert_eq!(
        Write::FastForwardCheckout {
            branch: "main".into(),
            oid: oid.into(),
        }
        .args(),
        [
            "-c",
            "merge.autoStash=false",
            "-c",
            "branch.main.mergeOptions=",
            "merge",
            "--ff-only",
            "--no-squash",
            "--commit",
            "--no-autostash",
            "--no-overwrite-ignore",
            "--strategy=ort",
            "--",
            oid
        ]
    );
}

/// The manual Update branch's fetch writes only its named destination too
/// (`--refmap=`), but keeps git's handling of a forced upstream.
#[test]
fn the_manual_update_fetch_writes_only_its_destination() {
    assert_eq!(
        Write::FetchSource {
            remote: "origin".into(),
            refspec: "refs/heads/main:refs/remotes/origin/main".into(),
            refuse_forced: false,
        }
        .args(),
        [
            "fetch",
            "--no-tags",
            "--no-prune",
            "--no-recurse-submodules",
            "--refmap=",
            "--",
            "origin",
            "refs/heads/main:refs/remotes/origin/main"
        ]
    );
}

/// **Every write the window makes, and its verb — and `restore` twice
/// over is two writes rather than one written twice.**
///
/// `Write::Unstage` is `restore --staged` and `Write::Rollback` is
/// `restore --source=HEAD --staged --worktree`: one puts the index back
/// and leaves the disk alone, the other puts both back. They share a
/// verb and mean different things, which is exactly the row a reader
/// skims past as a duplicate — so the name says it.
///
/// **The count is gone from the assertions.** Decision 21 of the spec
/// lifted the limit and said the guards are the rule; the list below is
/// the guard, and a `len()` beside it only made a new write look like a
/// broken test. M8.3 said three, M8.3.3 said eight, M12 says thirteen,
/// and none of those numbers was ever what mattered.
#[test]
fn every_write_is_named_here_and_restore_appearing_twice_is_two_of_them() {
    let every = one_of_each();

    let verbs: Vec<String> = every
        .iter()
        .map(|write| {
            let args = write.args();
            // Global options — literal pathspecs, `-c key=value` — interpret
            // the write; they are not another write.
            let mut verb = 0;
            loop {
                match args.get(verb).map(String::as_str) {
                    Some("--literal-pathspecs") => verb += 1,
                    Some("-c") => verb += 2,
                    _ => break,
                }
            }
            args.get(verb).cloned().unwrap_or_default()
        })
        .collect();

    assert_eq!(
        verbs,
        [
            "fetch", "merge", "clone", "add", "restore", "restore", "commit", "switch", "switch",
            "branch", "stash", "stash", "worktree", "worktree", "worktree", "worktree", "push",
            "fetch", "fetch", "merge", "apply", "apply"
        ],
        "a new write means a new line here, deliberately"
    );
    assert_ne!(
        every[4].args(),
        every[5].args(),
        "the two restores are two writes: one takes the index back, the other the disk as well"
    );
}

/// **The two writes M12 adds, spelled once and nowhere else.**
///
/// `add -b` makes the branch and refuses one that already exists — `-B`
/// would reset it and lose its commits, which is why that spelling is in
/// the list above. `remove` carries no `--force`: git refuses a worktree
/// with a modified or untracked file in it, and that refusal is the
/// guard, as `branch -d`'s is. What git does *not* refuse — deleting the
/// ignored files with the folder — is the window's to ask about, which is
/// decision 6 of M12's plan.
#[test]
fn a_worktree_is_added_with_its_branch_and_removed_without_force() {
    assert_eq!(
        Write::WorktreeAdd {
            path: PathBuf::from("/w/guild-lazy-lsp"),
            branch: "lazy-lsp".to_owned(),
            base: None,
        }
        .args(),
        ["worktree", "add", "-b", "lazy-lsp", "/w/guild-lazy-lsp"]
    );
    // A base is git's own start point, the last word — M13.1.
    assert_eq!(
        Write::WorktreeAdd {
            path: PathBuf::from("/w/guild-lazy-lsp"),
            branch: "lazy-lsp".to_owned(),
            base: Some("main".to_owned()),
        }
        .args(),
        [
            "worktree",
            "add",
            "-b",
            "lazy-lsp",
            "/w/guild-lazy-lsp",
            "main"
        ]
    );
    assert_eq!(
        Write::WorktreeRemove(PathBuf::from("/w/guild-lazy-lsp")).args(),
        ["worktree", "remove", "/w/guild-lazy-lsp"]
    );
}

#[test]
fn clone_is_named_but_cannot_bypass_owned_staging_through_ordinary_write() {
    let write = Write::Clone {
        url: "https://example.test/repo.git".into(),
        destination: "repo".into(),
    };
    assert_eq!(
        write.args(),
        [
            "clone",
            "--progress",
            "--",
            "https://example.test/repo.git",
            "repo"
        ]
    );
    let root = Scratch::new("clone-gateway");
    assert!(guild_desktop::git::write(&root, &write).is_err());
    assert_eq!(std::fs::read_dir(&*root).unwrap().count(), 0);
}

#[test]
fn literal_unstage_and_rollback_do_not_touch_matching_siblings() {
    use guild_desktop::git;
    let repo = stash_repository("literal-pathspecs", &[]);
    for path in ["*.txt", "other.txt"] {
        std::fs::write(repo.join(path), "base").unwrap();
    }
    git::run(
        &repo,
        &["--literal-pathspecs", "add", "--", "*.txt", "other.txt"],
    )
    .unwrap();
    git::run(&repo, &["commit", "-q", "-m", "literal fixtures"]).unwrap();
    for path in ["*.txt", "other.txt"] {
        std::fs::write(repo.join(path), "edit").unwrap();
    }
    git::write(&repo, &Write::Stage(paths(&["*.txt", "other.txt"]))).unwrap();
    git::write(&repo, &Write::Unstage(paths(&["*.txt"]))).unwrap();
    assert_eq!(
        git::run(&repo, &["diff", "--cached", "--name-only", "-z"]).unwrap(),
        b"other.txt\0"
    );
    git::write(&repo, &Write::Stage(paths(&["*.txt"]))).unwrap();
    git::write(&repo, &Write::Rollback(paths(&["*.txt"]))).unwrap();
    assert_eq!(std::fs::read(repo.join("*.txt")).unwrap(), b"base");
    assert_eq!(std::fs::read(repo.join("other.txt")).unwrap(), b"edit");
    assert_eq!(
        git::run(&repo, &["diff", "--cached", "--name-only", "-z"]).unwrap(),
        b"other.txt\0"
    );
}

#[test]
fn unsupported_path_bytes_never_retarget_a_replacement_character_sibling() {
    use guild_desktop::git;
    use std::os::unix::ffi::OsStringExt;
    let repo = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    std::fs::write(repo.path().join("�.txt"), "original").unwrap();
    git::write(repo.path(), &Write::Stage(paths(&["�.txt"]))).unwrap();
    git::run(
        repo.path(),
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=t@example.invalid",
            "commit",
            "-qm",
            "base",
        ],
    )
    .unwrap();
    std::fs::write(repo.path().join("�.txt"), "staged").unwrap();
    git::write(repo.path(), &Write::Stage(paths(&["�.txt"]))).unwrap();
    std::fs::write(repo.path().join("�.txt"), "working").unwrap();
    let index = git::run(repo.path(), &["show", ":�.txt"]).unwrap();
    // Construct the unsupported identity; no invalid-byte filename is created.
    let raw = PathBuf::from(std::ffi::OsString::from_vec(b"\xff.txt".to_vec()));
    for write in [
        Write::Stage(vec![raw.clone()]),
        Write::Unstage(vec![raw.clone()]),
        Write::Rollback(vec![raw]),
    ] {
        assert!(
            git::write(repo.path(), &write).is_err(),
            "must explicitly refuse {write:?}"
        );
        assert_eq!(git::run(repo.path(), &["show", ":�.txt"]).unwrap(), index);
        assert_eq!(
            std::fs::read_to_string(repo.path().join("�.txt")).unwrap(),
            "working"
        );
    }
}

#[test]
fn a_large_explicit_path_set_uses_bounded_literal_transport() {
    use guild_desktop::git;
    let repo = tempfile::tempdir().unwrap();
    assert!(
        std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(repo.path())
            .status()
            .unwrap()
            .success()
    );
    let arg_max = std::process::Command::new("getconf")
        .arg("ARG_MAX")
        .output()
        .unwrap();
    let arg_max: usize = String::from_utf8(arg_max.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let prefix = format!(
        "{}/{}/{}",
        "a".repeat(200),
        "b".repeat(200),
        "c".repeat(200)
    );
    std::fs::create_dir_all(repo.path().join(&prefix)).unwrap();
    let count = arg_max / (prefix.len() + 10) + 100;
    let mut selected = Vec::new();
    for n in 0..count {
        let path = PathBuf::from(format!("{prefix}/{n:07}.txt"));
        std::fs::write(repo.path().join(&path), "data").unwrap();
        selected.push(path);
    }
    std::fs::write(repo.path().join(&prefix).join("excluded.txt"), "excluded").unwrap();
    assert!(
        selected
            .iter()
            .map(|p| p.as_os_str().len() + 1)
            .sum::<usize>()
            > arg_max
    );
    let discovered =
        guild_desktop::changes::discover_directory(repo.path(), Path::new(&prefix)).unwrap();
    let targets: Vec<_> = discovered
        .files
        .into_iter()
        .filter(|p| p.file_name().unwrap() != "excluded.txt")
        .collect();
    assert_eq!(targets, selected);
    let write = Write::Stage(targets);
    git::write(repo.path(), &write).unwrap();
    assert!(write.args().iter().map(|s| s.len() + 1).sum::<usize>() < 256);
    let staged = git::run(repo.path(), &["ls-files", "-z"]).unwrap();
    let staged: Vec<_> = staged
        .split(|b| *b == 0)
        .filter(|p| !p.is_empty())
        .collect();
    assert_eq!(staged.len(), selected.len());
    assert_eq!(
        staged,
        selected
            .iter()
            .map(|p| p.as_os_str().as_encoded_bytes())
            .collect::<Vec<_>>()
    );
}

#[test]
fn embedded_nul_identities_never_split_into_real_pathspecs() {
    use guild_desktop::git;
    for operation in 0..3 {
        let repo = tempfile::tempdir().unwrap();
        git::run(repo.path(), &["init", "-q"]).unwrap();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(repo.path().join(name), "base").unwrap();
        }
        git::write(repo.path(), &Write::Stage(paths(&["a.txt", "b.txt"]))).unwrap();
        git::run(
            repo.path(),
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=t@example.invalid",
                "commit",
                "-qm",
                "base",
            ],
        )
        .unwrap();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(repo.path().join(name), "index").unwrap();
        }
        git::write(repo.path(), &Write::Stage(paths(&["a.txt", "b.txt"]))).unwrap();
        for name in ["a.txt", "b.txt"] {
            std::fs::write(repo.path().join(name), "working").unwrap();
        }
        let before = git::run(repo.path(), &["ls-files", "--stage", "-z"]).unwrap();
        // Construct an invalid identity; never create a NUL filename.
        let invalid = vec![PathBuf::from("a.txt\0b.txt")];
        let write = match operation {
            0 => Write::Stage(invalid),
            1 => Write::Unstage(invalid),
            _ => Write::Rollback(invalid),
        };
        let error = git::write(repo.path(), &write)
            .expect_err("NUL must not split one identity into two files");
        assert!(
            error.to_string().contains("NUL") && !error.to_string().contains("UTF-8"),
            "{error}"
        );
        assert_eq!(
            git::run(repo.path(), &["ls-files", "--stage", "-z"]).unwrap(),
            before
        );
        for name in ["a.txt", "b.txt"] {
            assert_eq!(std::fs::read(repo.path().join(name)).unwrap(), b"working");
        }
    }
}
