//! The file-name index: what it holds, how it follows the disk, how it
//! ranks — M8.9 group B's task 9. Declared by the seams commit, which
//! gives every file of the pass its place in `main.rs`.
//!
//! The trees are built on disk, because what is being tested is the walk
//! and the ignore rules together; a fake filesystem would test the fake.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scratch::Scratch;
use guild_desktop::file_index::{self, Entry, Index, Under};

/// A tree of our own, named after the test, and canonical — `/var` is
/// `/private/var`. **Removed when the test lets go of it**, so a run
/// leaves nothing in `$TMPDIR`.
fn tree(name: &str, files: &[&str]) -> Tree {
    let root = Scratch::new(&format!("guild-file-index-{name}-"));
    for path in files {
        write(&root, path, "x");
    }
    Tree {
        path: root.canonicalize().expect("a canonical root"),
        _folder: root,
    }
}

/// A tree on disk for one test: its canonical path, and the folder it is
/// in, held so that it goes when the tree does.
struct Tree {
    path: PathBuf,
    _folder: Scratch,
}

impl std::ops::Deref for Tree {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

fn write(root: &Path, path: &str, text: &str) {
    let full = root.join(path);
    if let Some(parent) = full.parent() {
        std::fs::create_dir_all(parent).expect("a directory");
    }
    std::fs::write(&full, text).expect("a file");
}

/// What a walk found, as `(path, ignored)` in path order.
fn walked(root: &Path) -> Vec<(String, bool)> {
    let mut found: Vec<(String, bool)> =
        file_index::walk(root, Under::Project, &AtomicBool::new(false))
            .into_iter()
            .map(|entry: Entry| {
                assert_eq!(entry.under, Under::Project);
                (entry.path.to_string_lossy().into_owned(), entry.ignored)
            })
            .collect();
    found.sort();
    found
}

/// An index of the project's walk.
fn indexed(root: &Path) -> Index {
    let mut index = Index::default();
    index.replace(
        Under::Project,
        file_index::walk(root, Under::Project, &AtomicBool::new(false)),
    );
    index
}

/// What an index holds for the project, in path order.
fn held(index: &Index) -> Vec<String> {
    let mut paths: Vec<String> = index
        .entries()
        .filter(|entry| entry.under == Under::Project)
        .map(|entry| entry.path.to_string_lossy().into_owned())
        .collect();
    paths.sort();
    paths
}

/// The paths a ranking answers, in its order.
fn ranked(rows: &[guild_desktop::file_index::Row]) -> Vec<String> {
    rows.iter()
        .map(|row| row.path.to_string_lossy().into_owned())
        .collect()
}

#[test]
fn the_index_is_what_the_tree_shows() {
    let root = tree(
        "shows",
        &[
            "src/main.rs",
            ".github/workflows/ci.yml",
            ".git/config",
            ".guild/state.json",
            "local.env",
            "target/debug/build.rs",
        ],
    );
    write(&root, ".gitignore", "local.env\ntarget/\n");

    let found = walked(&root);

    assert_eq!(
        found,
        vec![
            (".github/workflows/ci.yml".to_owned(), false),
            (".gitignore".to_owned(), false),
            ("local.env".to_owned(), true),
            ("src/main.rs".to_owned(), false),
        ],
        "a dotfile in, git's and Guild's own out, an ignored file in and \
         marked, the inside of an ignored directory out"
    );

    // A linked worktree's `.git` is a file, and the second pass keeps
    // every file the first did not yield — task 0 found exactly that.
    let worktree = tree("worktree", &["src/lib.rs"]);
    write(&worktree, ".git", "gitdir: /somewhere/else\n");
    assert_eq!(
        walked(&worktree),
        vec![("src/lib.rs".to_owned(), false)],
        "git's pointer is not the project's file"
    );
}

#[test]
fn a_nested_gitignore_is_obeyed_and_not_only_the_root_s() {
    let root = tree(
        "nested",
        &["sub/code.rs", "sub/secret.txt", "sub/build/out.o", "top.rs"],
    );
    write(&root, "sub/.gitignore", "secret.txt\nbuild/\n");

    assert_eq!(
        walked(&root),
        vec![
            ("sub/.gitignore".to_owned(), false),
            ("sub/code.rs".to_owned(), false),
            ("sub/secret.txt".to_owned(), true),
            ("top.rs".to_owned(), false),
        ]
    );
}

#[test]
fn a_batch_adds_a_file_that_appeared_and_drops_one_that_went() {
    let root = tree("batch", &["src/old.rs", "src/kept.rs"]);
    write(&root, ".gitignore", "*.log\n");
    let mut index = indexed(&root);

    std::fs::remove_file(root.join("src/old.rs")).expect("removed");
    write(&root, "src/new.rs", "x");
    write(&root, "src/debug.log", "x");
    file_index::apply(
        &mut index,
        &root,
        Under::Project,
        &[
            PathBuf::from("src/old.rs"),
            PathBuf::from("src/new.rs"),
            PathBuf::from("src/debug.log"),
        ],
        &[PathBuf::from("src")],
    );

    assert_eq!(
        held(&index),
        vec![".gitignore", "src/debug.log", "src/kept.rs", "src/new.rs"]
    );
    let log = index
        .entries()
        .find(|entry| entry.path == Path::new("src/debug.log"))
        .expect("the log is held");
    assert!(
        log.ignored,
        "a file the root's rules name is marked ignored"
    );

    // A directory that appeared, with a file already in it, is walked —
    // the batch names only where it appeared.
    write(&root, "fresh/deep/one.rs", "x");
    file_index::apply(
        &mut index,
        &root,
        Under::Project,
        &[PathBuf::from("fresh/deep/one.rs")],
        &[PathBuf::from("fresh/deep"), PathBuf::from("")],
    );
    assert!(
        held(&index).contains(&"fresh/deep/one.rs".to_owned()),
        "a new directory's files arrive: {:?}",
        held(&index)
    );
}

#[test]
fn a_directory_that_went_takes_its_files_with_it() {
    let root = tree("gone", &["sub/a.rs", "sub/inner/b.rs", "stay.rs"]);
    let mut index = indexed(&root);

    std::fs::remove_dir_all(root.join("sub")).expect("removed");
    file_index::apply(
        &mut index,
        &root,
        Under::Project,
        &[],
        &[PathBuf::from("sub")],
    );
    assert_eq!(held(&index), vec!["stay.rs"]);

    // A rename reaches the batch as a path no longer there, taken for a
    // file (`watch::absorb`), and its parent.
    let root = tree("renamed", &["old/a.rs", "stay.rs"]);
    let mut index = indexed(&root);
    std::fs::rename(root.join("old"), root.join("new")).expect("renamed");
    file_index::apply(
        &mut index,
        &root,
        Under::Project,
        &[PathBuf::from("old")],
        &[PathBuf::from("")],
    );
    assert_eq!(held(&index), vec!["new/a.rs", "stay.rs"]);
}

#[test]
fn an_ignored_file_never_outranks_one_that_is_not() {
    let root = tree("ignored-rank", &["main.rs", "mxaxixn.rs"]);
    write(&root, ".gitignore", "main.rs\n");
    let index = indexed(&root);

    let rows = file_index::rank("main", &index, &[PathBuf::from("main.rs")], false, 50);

    assert_eq!(ranked(&rows), vec!["mxaxixn.rs", "main.rs"]);
    assert!(rows[1].ignored);
    assert!(rows[1].recent, "recent, and still below");
    assert_eq!(rows[1].marks, vec![0, 1, 2, 3], "marks are characters");
}

#[test]
fn an_empty_query_answers_the_recent_files_that_still_exist() {
    let root = tree("empty", &["a.rs", "b.rs", "c.rs"]);
    let index = indexed(&root);
    let recent = [
        PathBuf::from("c.rs"),
        PathBuf::from("gone.rs"),
        PathBuf::from("a.rs"),
    ];

    let rows = file_index::rank("", &index, &recent, false, 50);

    assert_eq!(ranked(&rows), vec!["c.rs", "a.rs"], "recency's order");
    assert!(rows.iter().all(|row| row.recent && row.marks.is_empty()));
    assert_eq!(
        ranked(&file_index::rank("  ", &index, &recent, false, 1)),
        vec!["c.rs"],
        "the limit holds, and spaces are nothing typed"
    );
}

#[test]
fn being_recent_breaks_a_tie_and_nothing_more() {
    let root = tree("tie", &["a/foo.rs", "b/foo.rs", "fxoxo.rs"]);
    let index = indexed(&root);

    let none = file_index::rank("foo", &index, &[], false, 50);
    assert_eq!(ranked(&none)[..2], ["a/foo.rs", "b/foo.rs"]);
    assert_eq!(none[0].marks.len(), 3);

    let recent = [PathBuf::from("b/foo.rs"), PathBuf::from("fxoxo.rs")];
    let rows = file_index::rank("foo", &index, &recent, false, 50);
    assert_eq!(
        ranked(&rows),
        vec!["b/foo.rs", "a/foo.rs", "fxoxo.rs"],
        "the tie goes to the recent file, and a recent worse match stays worse"
    );
}

#[test]
fn a_library_is_ranked_only_when_asked() {
    let root = tree("project", &["src/walk.rs"]);
    let library = tree("library", &["src/walker.rs"]);
    let mut index = indexed(&root);
    index.replace(
        Under::Library(0),
        file_index::walk(&library, Under::Library(0), &AtomicBool::new(false)),
    );

    let project = file_index::rank("walk", &index, &[], false, 50);
    assert_eq!(ranked(&project), vec!["src/walk.rs"]);

    let both = file_index::rank("walk", &index, &[], true, 50);
    assert_eq!(both.len(), 2);
    assert!(
        both.iter()
            .any(|row| row.under == Under::Library(0) && row.path == Path::new("src/walker.rs"))
    );
}

#[test]
fn a_big_batch_or_a_changed_ignore_file_walks_again() {
    let few: Vec<PathBuf> = (0..3)
        .map(|n| PathBuf::from(format!("src/{n}.rs")))
        .collect();
    assert!(!file_index::walks_again(&few));

    let many: Vec<PathBuf> = (0..=file_index::APPLY_AT_MOST)
        .map(|n| PathBuf::from(format!("src/{n}.rs")))
        .collect();
    assert!(file_index::walks_again(&many), "a checkout of a branch");

    assert!(
        file_index::walks_again(&[PathBuf::from("sub/.gitignore")]),
        "what is ignored may have changed anywhere below it"
    );
    assert!(file_index::walks_again(&[PathBuf::from(".ignore")]));
}

/// Basename relevance must survive a long directory and recent weaker matches.
#[test]
fn search_filename_relevance_precedes_fuzzy_score_and_recency() {
    for (query, preferred, weaker) in [
        (
            "config.rs",
            "very/long/project/source/config.rs",
            "config.rs.backup",
        ),
        (
            "config",
            "very/long/project/source/config.rs",
            "config-extra.rs",
        ),
        (
            "config",
            "very/long/project/source/config-extra.rs",
            "c/o/n/f/i/g.rs",
        ),
        (
            "src/config",
            "very/long/project/src/config.rs",
            "s/r/c/config.rs",
        ),
        ("КОНФИГ", "very/long/project/конфиг.rs", "конфиг-extra.rs"),
    ] {
        let mut index = Index::default();
        index.replace(
            Under::Project,
            [preferred, weaker]
                .into_iter()
                .map(|path| Entry {
                    path: path.into(),
                    ignored: false,
                    under: Under::Project,
                })
                .collect(),
        );
        let rows = file_index::rank(query, &index, &[weaker.into()], false, 50);
        assert_eq!(rows.len(), 2, "both candidates match {query}");
        assert_eq!(rows[0].path, Path::new(preferred), "query {query}");
        for row in rows {
            let text = row.path.to_str().unwrap();
            assert!(
                row.marks
                    .iter()
                    .all(|mark| (*mark as usize) < text.chars().count())
            );
        }
    }
}
