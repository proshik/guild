//! Which branch a project is on.
//!
//! Out here rather than beside the code because every one of these goes
//! in through the public door — which makes them a check on that door as
//! well as on the behaviour behind it.

use crate::scratch::Scratch;
use guild_desktop::git::*;
use std::path::Path;

fn temp(name: &str) -> Scratch {
    Scratch::new(&format!("guild-git-{name}-"))
}

fn repository(name: &str, head: &str) -> Scratch {
    let dir = temp(name);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::write(dir.join(".git/HEAD"), head).unwrap();
    dir
}

#[test]
fn an_ordinary_branch_is_named() {
    let dir = repository("branch", "ref: refs/heads/main\n");

    assert_eq!(branch(&dir).as_deref(), Some("main"));
}

/// A branch name may have slashes in it, and only the last part is the
/// name people use.
#[test]
fn a_branch_with_slashes_keeps_its_last_part() {
    let dir = repository("slashes", "ref: refs/heads/feature/scrollbar\n");

    assert_eq!(branch(&dir).as_deref(), Some("scrollbar"));
}

/// A detached head is a hash. All forty characters say no more than
/// the first seven and take the width of the whole widget.
#[test]
fn a_detached_head_is_a_short_hash() {
    let dir = repository("detached", "1a2b3c4d5e6f7a8b9c0d1e2f3a4b5c6d7e8f9a0b\n");

    assert_eq!(branch(&dir).as_deref(), Some("1a2b3c4"));
}

/// Where this product is going: a task gets its own worktree, and
/// there `.git` is a file pointing elsewhere. A reader that assumes a
/// directory shows nothing exactly where branches start to matter.
#[test]
fn a_worktree_follows_its_pointer() {
    let real = temp("worktree-real");
    std::fs::write(real.join("HEAD"), "ref: refs/heads/task-42\n").unwrap();

    let tree = temp("worktree");
    std::fs::write(tree.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();

    assert_eq!(branch(&tree).as_deref(), Some("task-42"));
}

/// **A folder opened inside its repository names the repository's
/// branch.** `.git` is at the repository's root, not in the folder, and a
/// reader that looked only there showed an empty widget for a project git
/// itself is perfectly happy in. The nearest `.git` above wins, as git's
/// own discovery has it. Decision 18 of M5.11.
#[test]
fn a_folder_inside_a_repository_names_the_repositorys_branch() {
    let dir = repository("inside", "ref: refs/heads/main\n");
    let sub = dir.join("sub/deeper");
    std::fs::create_dir_all(&sub).unwrap();

    assert_eq!(branch(&dir.join("sub")).as_deref(), Some("main"));
    assert_eq!(branch(&sub).as_deref(), Some("main"));
}

/// A project need not be a repository, and an empty widget is better
/// than the word "none" in the title bar.
#[test]
fn a_directory_without_a_repository_has_no_branch() {
    let dir = temp("plain");

    assert_eq!(branch(&dir), None);
}

/// The directory may vanish while the window is open — a project can
/// be moved or deleted from under it.
#[test]
fn a_missing_directory_is_not_a_panic() {
    assert_eq!(branch(Path::new("/no/such/place/at/all")), None);
}
