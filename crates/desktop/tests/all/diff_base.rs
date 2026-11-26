//! The two texts a diff's base names — decision 5 of M8.3. Pure: the
//! tests read what would be asked of git, and which side is the window's
//! working text rather than git's — and one test on a real repository,
//! because what a spec *reads* is git's to say.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::diff_tab::{Side, sides};
use guild_session::Base;

fn blob(spec: &str) -> Side {
    Side::Blob(spec.to_owned())
}

/// `HEAD` against the working tree: M8.2's diff, the default.
#[test]
fn head_is_the_last_commit_against_the_working_text() {
    assert_eq!(
        sides(&Base::Head, Path::new("src/lib.rs")),
        (blob("HEAD:./src/lib.rs"), Side::Working)
    );
}

/// The staged group's diff: what a commit would take, the index
/// against `HEAD` — the new side is the index, not the working tree.
#[test]
fn index_is_the_last_commit_against_the_index() {
    assert_eq!(
        sides(&Base::Index, Path::new("src/lib.rs")),
        (blob("HEAD:./src/lib.rs"), blob(":./src/lib.rs"))
    );
}

/// The unstaged group's diff: what is not yet staged, the index against
/// the working tree.
#[test]
fn unstaged_is_the_index_against_the_working_text() {
    assert_eq!(
        sides(&Base::Unstaged, Path::new("src/lib.rs")),
        (blob(":./src/lib.rs"), Side::Working)
    );
}

/// A commit's diff: its parent against it, both from git. A path with a
/// space or a non-ASCII name is spelled as it is; git takes the spec as
/// one argument.
///
/// **No `./` here, and that is not an oversight.** A commit's paths come
/// from `git show --name-status`, which prints them relative to the
/// repository whatever folder it runs in, and `sha:path` reads a path that
/// way. The working tree's bases take the path the tree drew, relative to
/// the project, which is what `./` tells git. Decision 18 of M5.11.
#[test]
fn a_commit_is_its_parent_against_itself() {
    assert_eq!(
        sides(
            &Base::Commit("a1b2c3d".into()),
            Path::new("with space/файл.md")
        ),
        (
            blob("a1b2c3d^:with space/файл.md"),
            blob("a1b2c3d:with space/файл.md")
        )
    );
}

/// `HEAD:./path` from a folder is the folder's file; `HEAD:path` was the
/// repository root's file of the same name, read without a word — task
/// 0.1 of M5.11 measured `git show HEAD:Cargo.toml` from `sub/` printing
/// the root's `Cargo.toml`.
#[test]
fn a_blob_of_a_folder_is_the_folders_file_and_not_the_roots() {
    let dir = Scratch::new("guild-diff-base-folder-");
    let sub = dir.join("sub");
    std::fs::create_dir_all(sub.join("src")).unwrap();
    let git = |args: &[&str]| {
        guild_desktop::git::run(&dir, args).unwrap_or_else(|err| panic!("git {args:?}: {err}"));
    };
    git(&["init", "-q", "-b", "main"]);
    git(&["config", "user.name", "Guild"]);
    git(&["config", "user.email", "guild@example.invalid"]);
    std::fs::write(dir.join("Cargo.toml"), "the root's\n").unwrap();
    std::fs::write(sub.join("Cargo.toml"), "the folder's\n").unwrap();
    std::fs::write(sub.join("src/lib.rs"), "fn lib() {}\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "base"]);

    let text = |side: Side| {
        let Side::Blob(spec) = side else {
            panic!("a blob side")
        };
        let bytes = guild_desktop::git::blob(&sub, &spec)
            .unwrap_or_else(|err| panic!("{spec}: {err}"))
            .unwrap_or_else(|| panic!("{spec} named nothing"));
        String::from_utf8(bytes).unwrap()
    };
    let (head, _) = sides(&Base::Head, Path::new("Cargo.toml"));
    assert_eq!(text(head), "the folder's\n");
    let (_, index) = sides(&Base::Index, &PathBuf::from("src/lib.rs"));
    assert_eq!(text(index), "fn lib() {}\n");
}
