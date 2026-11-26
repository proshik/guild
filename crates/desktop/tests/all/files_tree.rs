//! A commit's files as a tree. Pure: paths in, rows out, no window.

use std::path::PathBuf;

use guild_desktop::files_tree::{Row, of, visible};
use guild_desktop::git::log::FileChange;
use guild_desktop::theme::Mark;

fn changed(paths: &[&str]) -> Vec<FileChange> {
    paths
        .iter()
        .map(|path| FileChange {
            path: PathBuf::from(path),
            mark: Mark::Modified,
            from: None,
        })
        .collect()
}

fn shown(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Directory {
                path, files, depth, ..
            } => {
                format!("{}{} ({files})", "  ".repeat(*depth), path)
            }
            Row::File { name, depth, .. } => format!("{}{name}", "  ".repeat(*depth)),
        })
        .collect()
}

/// **A chain of directories with one child each is one row.**
/// `crates/desktop/tests/all` is a single line, as in every file tree
/// worth reading: four rows of nothing but a name is four
/// rows a person has to walk past to reach the files.
#[test]
fn a_chain_of_single_child_directories_is_one_row() {
    let rows = of(&changed(&[
        "crates/desktop/tests/all/a.rs",
        "crates/desktop/tests/all/b.rs",
    ]));

    assert_eq!(
        shown(&rows),
        ["crates/desktop/tests/all (2)", "  a.rs", "  b.rs"]
    );
}

/// Where a directory has two children the chain stops: that is the fork
/// the collapsing exists to show.
#[test]
fn a_fork_ends_the_chain() {
    let rows = of(&changed(&[
        "crates/desktop/src/lib.rs",
        "crates/session/src/lib.rs",
    ]));

    assert_eq!(
        shown(&rows),
        [
            "crates (2)",
            "  desktop/src (1)",
            "    lib.rs",
            "  session/src (1)",
            "    lib.rs",
        ]
    );
}

/// A file at the top has no directory row at all.
#[test]
fn a_file_at_the_root_stands_alone() {
    let rows = of(&changed(&["README.md", "Cargo.toml"]));

    assert_eq!(shown(&rows), ["Cargo.toml", "README.md"]);
}

/// The count on a directory is every file under it, however deep — which
/// is what makes it worth reading before opening anything.
#[test]
fn a_directory_counts_everything_beneath_it() {
    let rows = of(&changed(&["a/b/one.rs", "a/c/two.rs", "a/c/three.rs"]));

    assert_eq!(shown(&rows)[0], "a (3)");
}

/// Directories before files, and each set in order: a tree read by the
/// eye needs the same order every time.
#[test]
fn directories_come_before_files_and_both_are_sorted() {
    let rows = of(&changed(&["z.rs", "a/one.rs", "b.rs"]));

    assert_eq!(shown(&rows), ["a (1)", "  one.rs", "b.rs", "z.rs"]);
}

/// A rename keeps where it came from: the row says the new name and the
/// tree puts it where the new name lives.
#[test]
fn a_rename_carries_its_old_path() {
    let files = vec![FileChange {
        path: PathBuf::from("src/new.rs"),
        mark: Mark::Modified,
        from: Some(PathBuf::from("src/old.rs")),
    }];
    let rows = of(&files);

    let Row::File { from, name, .. } = &rows[1] else {
        panic!("a file under a directory");
    };
    assert_eq!(name, "new.rs");
    assert_eq!(from.as_deref(), Some(std::path::Path::new("src/old.rs")));
}

/// Nothing in, nothing out — a commit that changed no file is a commit
/// git will not make, but an empty list must not panic.
#[test]
fn no_files_is_no_rows() {
    assert!(of(&[]).is_empty());
}

/// **A folded directory hides what is under it** — M8.3.6, the tree in
/// Push Commits. Folded by its full path, which a collapsed chain's label is
/// not: `Atlas/Localization` and `Localization` under another root are
/// two directories.
#[test]
fn a_folded_directory_hides_what_is_under_it() {
    let files = changed(&[
        "Atlas/Localization/L10n.swift",
        "Atlas/MenuBar/HeaderMetric.swift",
        "Atlas/MenuBar/PopoverView.swift",
        "AtlasTests/HeaderMetricTests.swift",
        "CHANGELOG.md",
    ]);
    let rows = of(&files);
    let keys: Vec<String> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Directory { key, .. } => Some(key.clone()),
            Row::File { .. } => None,
        })
        .collect();
    assert_eq!(
        keys,
        ["Atlas", "Atlas/Localization", "Atlas/MenuBar", "AtlasTests"]
    );

    let all: Vec<Row> = visible(&rows, &Default::default())
        .into_iter()
        .cloned()
        .collect();
    assert_eq!(all, rows, "nothing folded shows everything");

    let folded = ["Atlas/MenuBar".to_owned()].into_iter().collect();
    let shown_rows: Vec<Row> = visible(&rows, &folded).into_iter().cloned().collect();
    assert_eq!(
        shown(&shown_rows),
        [
            "Atlas (3)",
            "  Localization (1)",
            "    L10n.swift",
            "  MenuBar (2)",
            "AtlasTests (1)",
            "  HeaderMetricTests.swift",
            "CHANGELOG.md",
        ]
    );

    let folded = ["Atlas".to_owned()].into_iter().collect();
    let shown_rows: Vec<Row> = visible(&rows, &folded).into_iter().cloned().collect();
    assert_eq!(
        shown(&shown_rows),
        [
            "Atlas (3)",
            "AtlasTests (1)",
            "  HeaderMetricTests.swift",
            "CHANGELOG.md",
        ]
    );
}

#[test]
fn commit_tree_root_folding_visible_selection_and_reveal() {
    use guild_desktop::files_tree::Tree;
    let mut tree = Tree::new(&changed(&["src/a/b.rs", "src/other/c.rs", "README.md"]));
    tree.toggle(Some("src/a"));
    assert_eq!(
        tree.paths(),
        [PathBuf::from("src/other/c.rs"), PathBuf::from("README.md")]
    );
    tree.collapse_all();
    assert!(tree.visible().is_empty());
    tree.toggle(None);
    assert_eq!(tree.paths(), [PathBuf::from("README.md")]);
    assert_eq!(tree.reveal(std::path::Path::new("src/a/b.rs")), Some(3));
    assert!(tree.folded.contains("src/other"));
    assert_eq!(
        tree.paths(),
        [PathBuf::from("src/a/b.rs"), PathBuf::from("README.md")]
    );
    tree.expand_all();
    assert_eq!(tree.paths().len(), 3);
    assert_eq!(tree.reveal(std::path::Path::new("missing")), None);
}
