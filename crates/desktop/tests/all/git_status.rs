//! What `git status --porcelain=v2 -z` says, parsed, and what it means
//! for the rows above a change. On captured output, never a repository:
//! `fixtures/porcelain-v2.bin` is git 2.50's answer for a scratch
//! repository with every case the parser knows — a modified file, a
//! staged one, one both staged and modified, an added one, a deleted
//! one, a rename, an unmerged one, an untracked file and directory, an
//! ignored file and directory, a path with a space, a non-ASCII name.

use std::path::{Path, PathBuf};

use guild_desktop::git::status::{Entry, Kind, StatusMap, parse, within};
use guild_desktop::theme::Mark;

const CAPTURED: &[u8] = include_bytes!("../fixtures/porcelain-v2.bin");

fn entry<'a>(entries: &'a [Entry], path: &str) -> &'a Entry {
    entries
        .iter()
        .find(|e| e.path == Path::new(path))
        .unwrap_or_else(|| panic!("no entry for {path}"))
}

/// Every record of the capture parses to what git meant by it.
#[test]
fn every_captured_case_parses() {
    let entries = parse(CAPTURED).expect("porcelain v2");
    assert_eq!(entries.len(), 13, "{entries:#?}");

    let modified = entry(&entries, "src/modified.rs");
    assert_eq!(modified.kind, Kind::Modified);
    assert!(!modified.staged && modified.unstaged);

    let staged = entry(&entries, "src/staged.rs");
    assert!(staged.staged && !staged.unstaged);

    let both = entry(&entries, "src/both.rs");
    assert!(both.staged && both.unstaged);

    assert_eq!(entry(&entries, "src/added.rs").kind, Kind::Added);
    assert_eq!(entry(&entries, "src/deleted.rs").kind, Kind::Deleted);
    assert_eq!(
        entry(&entries, "src/new-name.rs").kind,
        Kind::Renamed {
            from: PathBuf::from("src/old-name.rs")
        },
        "the original path is the record after the rename's"
    );
    assert_eq!(entry(&entries, "src/conflict.rs").kind, Kind::Unmerged);

    let untracked = entry(&entries, "untracked.txt");
    assert_eq!(untracked.kind, Kind::Untracked);
    assert!(!untracked.is_dir);
    let dir = entry(&entries, "newdir");
    assert_eq!(dir.kind, Kind::Untracked);
    assert!(dir.is_dir, "a directory git did not descend into");

    assert_eq!(entry(&entries, "build.log").kind, Kind::Ignored);
    assert!(entry(&entries, "target").is_dir);

    assert_eq!(
        entry(&entries, "with space/file name.md").kind,
        Kind::Modified
    );
    assert_eq!(entry(&entries, "docs/файл.md").kind, Kind::Modified);
}

/// Bytes that are not this format are an error with a place in them,
/// not a panic and not an empty answer.
#[test]
fn what_is_not_porcelain_is_an_error() {
    let err = parse(b"On branch main\0nothing to commit\0").unwrap_err();
    assert_eq!(err.record, 1);
    assert!(parse(b"").unwrap().is_empty(), "no changes is no entries");
    let err = parse(b"2 R. N... 100644 100644 100644 abc def R100 a.rs\0").unwrap_err();
    assert!(err.reason.contains("original path"));
}

/// The map: a changed file marks itself and every directory above it
/// with the strongest mark under each, and nothing else.
#[test]
fn a_change_marks_the_directories_above_it() {
    let map = StatusMap::from_entries(parse(CAPTURED).unwrap());

    assert_eq!(
        map.mark_for(Path::new("src/modified.rs")),
        Some(Mark::Modified)
    );
    assert_eq!(map.mark_for(Path::new("src/added.rs")), Some(Mark::Added));
    assert_eq!(
        map.mark_for(Path::new("src/deleted.rs")),
        Some(Mark::Deleted)
    );
    assert_eq!(
        map.mark_for(Path::new("src/new-name.rs")),
        Some(Mark::Modified)
    );
    assert_eq!(
        map.mark_for(Path::new("untracked.txt")),
        Some(Mark::Untracked)
    );
    assert_eq!(
        map.mark_for(Path::new("src")),
        Some(Mark::Modified),
        "the strongest of what is under it"
    );
    assert_eq!(map.mark_for(Path::new("docs")), Some(Mark::Modified));
    assert_eq!(map.mark_for(Path::new("with space")), Some(Mark::Modified));
    assert_eq!(map.mark_for(Path::new("src/untouched.rs")), None);
    assert_eq!(
        map.mark_for(Path::new("build.log")),
        None,
        "ignored is no mark: the tree already dims it"
    );
    assert_eq!(map.mark_for(Path::new("target")), None);
    assert_eq!(
        map.mark_for(Path::new("")),
        None,
        "the root would say nothing"
    );
}

/// An untracked directory is one entry — git did not descend — and its
/// mark is the one mark a row inside inherits, because nothing inside
/// was reported. A directory marked from below passes nothing down.
#[test]
fn an_untracked_directory_marks_what_is_inside_and_a_modified_one_does_not() {
    let map = StatusMap::from_entries(parse(CAPTURED).unwrap());
    assert_eq!(map.mark_for(Path::new("newdir")), Some(Mark::Untracked));
    assert_eq!(
        map.mark_for(Path::new("newdir/deep/z.txt")),
        Some(Mark::Untracked),
        "inherited from the untracked directory"
    );
    assert_eq!(
        map.mark_for(Path::new("src/quiet.rs")),
        None,
        "a modified directory says nothing about a clean file in it"
    );
}

/// A directory with an added file and a modified one answers Modified;
/// with a deleted file only, Deleted; the order is M, D, A, ?.
#[test]
fn the_strongest_mark_wins_on_a_directory() {
    let map = StatusMap::from_entries(
        parse(b"1 A. N... 000000 100644 100644 0 0 a/new.rs\x001 .M N... 100644 100644 100644 0 0 a/old.rs\x001 D. N... 100644 000000 000000 0 0 b/gone.rs\x00? c/\x00")
            .unwrap(),
    );
    assert_eq!(map.mark_for(Path::new("a")), Some(Mark::Modified));
    assert_eq!(map.mark_for(Path::new("b")), Some(Mark::Deleted));
    assert_eq!(map.mark_for(Path::new("c")), Some(Mark::Untracked));
    assert_eq!(map.marked(), 6, "four files and directories, plus a and b");
}

/// The map's size is the number of changed paths and their ancestors,
/// never the size of the repository.
#[test]
fn the_map_is_bounded_by_the_changes() {
    let map = StatusMap::from_entries(parse(CAPTURED).unwrap());
    assert_eq!(map.entries().count(), 13);
    assert!(map.marked() < 20, "{}", map.marked());
}

fn changed(path: &str, staged: bool, unstaged: bool) -> Entry {
    Entry {
        path: PathBuf::from(path),
        kind: Kind::Modified,
        staged,
        unstaged,
        is_dir: false,
    }
}

/// **A project opened inside its repository sees only its folder, and
/// what is staged outside it is kept apart.** git's index is the
/// repository's, so a commit from the folder would take a change staged
/// in `../root.txt` with it; the map carries those paths so that the
/// commit can refuse, naming them. Unstaged and untracked changes outside
/// are not a commit's business and are simply not there. Decision 18 of
/// M5.11.
#[test]
fn a_path_staged_outside_the_folder_is_kept_apart() {
    let mut untracked = changed("elsewhere/new.txt", false, false);
    untracked.kind = Kind::Untracked;
    let mut moved_in = changed("sub/moved.rs", true, false);
    moved_in.kind = Kind::Renamed {
        from: PathBuf::from("old.rs"),
    };
    let entries = vec![
        changed("root.txt", true, false),
        changed("other.txt", false, true),
        untracked,
        changed("sub/src/lib.rs", true, true),
        changed("subway/lib.rs", true, false),
        moved_in,
    ];

    let (inside, elsewhere) = within(entries.clone(), Path::new("sub"));
    let paths: Vec<&Path> = inside.iter().map(|e| e.path.as_path()).collect();
    assert_eq!(paths, [Path::new("src/lib.rs"), Path::new("moved.rs")]);
    assert_eq!(
        elsewhere,
        [
            PathBuf::from("root.txt"),
            PathBuf::from("subway/lib.rs"),
            PathBuf::from("old.rs"),
        ],
        "a staged rename from outside takes its old path out of the index too"
    );

    let map = StatusMap::from_entries(inside).with_staged_elsewhere(elsewhere.clone());
    assert_eq!(map.staged_elsewhere(), elsewhere.as_slice());
    assert!(map.entry(Path::new("src/lib.rs")).is_some());

    // The repository itself opened: nothing is outside it.
    let (all, none) = within(entries, Path::new(""));
    assert_eq!(all.len(), 6);
    assert!(none.is_empty());
    assert!(StatusMap::from_entries(all).staged_elsewhere().is_empty());
}
