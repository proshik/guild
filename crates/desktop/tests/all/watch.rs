//! The disk watcher: the batching is pure and tested as such; the thread
//! is checked once, on a real directory, for several things at once —
//! a watcher is a system resource, like a pty.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use crate::scratch::Scratch;
use guild_desktop::watch::{Batch, Watcher};
use ignore::gitignore::{Gitignore, GitignoreBuilder};

/// Rules from one line, the way a `.gitignore` would give them.
fn ignore_matching(line: &str) -> Gitignore {
    let mut builder = GitignoreBuilder::new("/");
    builder.add_line(None, line).expect("a valid pattern");
    builder.build().expect("rules")
}

fn temp(name: &str) -> Scratch {
    Scratch::new(&format!("guild-watch-{name}-"))
}

/// Events are coalesced to the directories they touch, in order of first
/// appearance, and a path under an ignored directory does not reach the
/// window at all.
#[test]
fn a_batch_is_the_set_of_directories_touched() {
    let mut batch = Batch::new(&ignore_matching("/target"));
    batch.push(Path::new("src/lib.rs"));
    batch.push(Path::new("src/main.rs"));
    batch.push(Path::new("target/debug/app"));
    batch.push(Path::new("Cargo.toml"));

    assert_eq!(batch.directories(), [Path::new("src"), Path::new("")]);
    assert_eq!(
        batch.files(),
        [
            Path::new("src/lib.rs"),
            Path::new("src/main.rs"),
            Path::new("Cargo.toml")
        ]
    );
    assert!(!batch.is_empty());
}

/// A directory created or removed is an event on its parent: the parent's
/// listing is what changed, and a directory is nothing a tab shows.
#[test]
fn a_directory_that_appears_is_a_change_in_its_parent() {
    let mut batch = Batch::new(&ignore_matching("/target"));
    batch.push_directory(Path::new("src/new"));
    batch.push_directory(Path::new("target/debug"));

    assert_eq!(batch.directories(), [Path::new("src")]);
    assert!(batch.files().is_empty());

    let mut empty = Batch::new(&ignore_matching("/target"));
    empty.push(Path::new("target/debug/app"));
    assert!(
        empty.is_empty(),
        "nothing under an ignored directory is news"
    );

    // The ignored directory itself appearing is news for its parent: the
    // tree shows it, dimmed. And an ignored file is still a file a tab
    // may be showing.
    let mut appeared = Batch::new(&ignore_matching("/target"));
    appeared.push_directory(Path::new("target"));
    appeared.push(Path::new("build.log"));
    assert_eq!(appeared.directories(), [Path::new("")]);
    assert_eq!(appeared.files(), [Path::new("build.log")]);
}

/// A change under `.git` that moves the marks — a commit writes `HEAD`
/// and the index, a `git add` the index, a branch a ref — says `git` and
/// names no file: the window has no tab for `.git/index`. What git
/// writes for itself — objects, logs, the index's lock — is nothing.
#[test]
fn a_batch_that_saw_the_index_says_git_and_names_no_file() {
    let mut batch = Batch::new(&ignore_matching("/target"));
    batch.push(Path::new(".git/index"));
    batch.push(Path::new(".git/HEAD"));
    batch.push(Path::new(".git/refs/heads/main"));
    assert!(batch.git());
    assert!(batch.files().is_empty() && batch.directories().is_empty());
    assert!(!batch.is_empty(), "there is something to say");

    let mut noise = Batch::new(&ignore_matching("/target"));
    noise.push(Path::new(".git/objects/ab/cdef0123"));
    noise.push(Path::new(".git/index.lock"));
    noise.push(Path::new(".git/logs/HEAD"));
    assert!(!noise.git());
    assert!(noise.is_empty(), "nothing the window needs to hear");

    // The other half of the answer, and what the Log rests on since
    // M8.3.2: an ordinary write is not a change under `.git`. The marks
    // are asked for again either way; the log is asked only when this is
    // true, so that a save does not throw away a page a person scrolled
    // to.
    let mut ordinary = Batch::new(&ignore_matching("/target"));
    ordinary.push(Path::new("src/lib.rs"));
    ordinary.push(Path::new("README.md"));
    assert!(!ordinary.git(), "a saved file did not move HEAD");
    assert!(!ordinary.is_empty(), "but the window does hear about it");
}

/// A real watcher on a real directory: a file written appears in a batch
/// within a second, relative to the root, with its directory; a file
/// under `.git` does not. Several things per watcher, on purpose.
#[test]
fn a_written_file_arrives_in_a_batch_within_a_second() {
    let dir = temp("real");
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let watcher = Watcher::start(&dir);
    // FSEvents takes seconds to arm, and a write before that is not
    // seen; the watcher says when it is.
    let arming = Instant::now();
    while !watcher.is_ready() {
        // Thirty seconds, not ten. Arming an FSEvents stream took 2.4 s
        // on an idle machine when M8.1 measured it, and this test runs
        // inside `just check`, which compiles and runs the whole
        // workspace at once — ten seconds went by while the machine was
        // busy elsewhere and the failure read as a broken watcher.
        assert!(
            arming.elapsed() < Duration::from_secs(30),
            "the watcher never became ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(100));

    std::fs::write(dir.join("src/lib.rs"), "fn a() {}\n").unwrap();
    std::fs::write(dir.join(".git/index"), "x").unwrap();

    let started = Instant::now();
    let deadline = started + Duration::from_secs(3);
    let mut files = Vec::new();
    let mut directories = Vec::new();
    while Instant::now() < deadline {
        if let Some(batch) = watcher.try_recv() {
            files.extend(batch.files().iter().cloned());
            directories.extend(batch.directories().iter().cloned());
            if files.iter().any(|f| f == Path::new("src/lib.rs")) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    let waited = started.elapsed();
    assert!(
        files.iter().any(|f| f == Path::new("src/lib.rs")),
        "the file written is in a batch, relative to the root: {files:?}"
    );
    assert!(
        waited < Duration::from_secs(1),
        "and within a second, not {waited:?}"
    );
    assert!(
        directories.iter().any(|d| d == Path::new("src")),
        "and its directory with it: {directories:?}"
    );
    assert!(
        !files.iter().any(|f| f.starts_with(".git")),
        "nothing under .git reaches the window: {files:?}"
    );

    // Closing a project drops its watcher on the window's thread; a
    // drop that stalls would be a window that freezes on close.
    let dropping = Instant::now();
    drop(watcher);
    assert!(
        dropping.elapsed() < Duration::from_millis(500),
        "dropping the watcher took {:?}",
        dropping.elapsed()
    );
}

/// Run git in `dir`, as a fixture does, and insist it worked.
fn git(dir: &Path, args: &[&str]) {
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
}

/// Wait for a batch that says `git`, up to three seconds.
fn heard_git(watcher: &Watcher) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if watcher.try_recv().is_some_and(|batch| batch.git()) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// **A linked worktree's `.git` is a file**, and what moves when a person
/// commits or pushes from a terminal is somewhere else: its `HEAD` and index
/// under the main repository's `worktrees/<name>`, and the refs in the
/// common dir. A watcher of the root alone heard none of it, so the marks,
/// the Log and Push of every worktree stood still after a commit or a push
/// from a shell. Both halves are checked on one watcher.
#[test]
fn a_linked_worktree_hears_its_commits_and_the_shared_refs() {
    let base = temp("linked");
    git(&base, &["init", "-q", "-b", "main", "repo"]);
    let repo = base.join("repo");
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    git(&repo, &["worktree", "add", "-q", "-b", "feat", "../wt"]);
    let worktree = base.join("wt");

    let watcher = Watcher::start(&worktree);
    let arming = Instant::now();
    while !watcher.is_ready() {
        assert!(
            arming.elapsed() < Duration::from_secs(30),
            "the watcher never became ready"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    std::thread::sleep(Duration::from_millis(100));
    while watcher.try_recv().is_some() {}

    // A commit in the worktree: its branch's ref, in the common dir.
    git(
        &worktree,
        &["commit", "-q", "--allow-empty", "-m", "second"],
    );
    assert!(heard_git(&watcher), "a commit in the worktree is heard");
    std::thread::sleep(Duration::from_millis(200));
    while watcher.try_recv().is_some() {}

    // A push, as far as the refs can tell: the remote's branch moves.
    git(&repo, &["update-ref", "refs/remotes/origin/feat", "feat"]);
    assert!(
        heard_git(&watcher),
        "a ref moved in the common dir is heard"
    );
    std::thread::sleep(Duration::from_millis(200));
    while watcher.try_recv().is_some() {}

    // And the worktree's own `HEAD`: a switch writes it.
    git(&worktree, &["switch", "-q", "--detach"]);
    assert!(heard_git(&watcher), "the worktree's own HEAD is heard");

    drop(watcher);
}
