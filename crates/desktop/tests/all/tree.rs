//! The file tree as a model: what is in a directory, what is expanded,
//! what is ignored, in what order. Out here because the window only
//! reads it, and everything about it that can be wrong can be wrong
//! without a window.

use std::path::{Path, PathBuf};

use crate::scratch::Scratch;
use guild_desktop::tree::Tree;

/// A directory in the temporary path with the given entries: a trailing
/// `/` makes a directory, `name=content` writes a file with content, a
/// bare name writes an empty file. Parents are created on the way.
fn temp_project(name: &str, entries: &[&str]) -> Scratch {
    let dir = Scratch::new(&format!("guild-tree-{name}-"));
    for entry in entries {
        if let Some(d) = entry.strip_suffix('/') {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        } else {
            let (path, content) = entry.split_once('=').unwrap_or((entry, ""));
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
    }
    dir
}

fn names(tree: &Tree) -> Vec<String> {
    tree.rows()
        .iter()
        .map(|row| row.name().to_owned())
        .collect()
}

/// The rows under the root: what the root's own row heads.
fn entries(tree: &Tree) -> Vec<String> {
    names(tree).into_iter().skip(1).collect()
}

fn depths(tree: &Tree) -> Vec<usize> {
    tree.rows().iter().map(|row| row.depth()).collect()
}

/// Directories first, then files; names without regard to case; `.git`
/// not there at all.
#[test]
fn the_root_is_ordered_like_a_finder_and_hides_git() {
    let dir = temp_project(
        "order",
        &[
            "src/",
            "Cargo.toml",
            "README.md",
            "build.rs",
            ".git/HEAD",
            "docs/",
        ],
    );
    let tree = Tree::open(&dir);
    assert_eq!(
        entries(&tree),
        ["docs", "src", "build.rs", "Cargo.toml", "README.md"]
    );
}

/// A staging folder no copy of this window is running is a folder like any
/// other: one left behind by a crash or a failed cleanup must stay where the
/// person can see it and remove it. Hiding a running copy's folder is the
/// tree module's own test, since only the copier can say which is running.
#[test]
fn a_staging_folder_no_copy_is_running_is_shown() {
    let dir = temp_project(
        "staging",
        &[
            ".guild-transfer-0b5c2f3e-6a1d-4c8e-9f27-1d2e3f4a5b6c/item=partial",
            "src/",
        ],
    );
    let tree = Tree::open(&dir);
    assert_eq!(
        entries(&tree),
        [
            ".guild-transfer-0b5c2f3e-6a1d-4c8e-9f27-1d2e3f4a5b6c",
            "src"
        ]
    );
}

/// A directory is not read until it is expanded — its rows do not exist,
/// and neither does the cost of reading it. Collapsing forgets nothing.
#[test]
fn a_directory_is_read_when_expanded_and_not_before() {
    let dir = temp_project("lazy", &["src/main.rs", "src/lib.rs"]);
    let mut tree = Tree::open(&dir);
    assert_eq!(entries(&tree), ["src"]);
    assert!(!tree.is_loaded(Path::new("src")));

    tree.expand(Path::new("src"));
    assert_eq!(entries(&tree), ["src", "lib.rs", "main.rs"]);
    assert_eq!(depths(&tree), [0, 1, 2, 2]);
    assert!(tree.is_loaded(Path::new("src")));

    tree.collapse(Path::new("src"));
    assert_eq!(entries(&tree), ["src"]);
    assert!(
        tree.is_loaded(Path::new("src")),
        "collapsing forgets nothing"
    );
}

/// What `.gitignore` names is shown dimmed, not hidden: a person looking
/// for a build product should find it rather than wonder where it went.
/// Everything under an ignored directory is ignored too.
#[test]
fn ignored_entries_are_marked_and_still_there() {
    let dir = temp_project(
        "ignored",
        &[
            "target/debug/app",
            "src/lib.rs",
            ".gitignore=/target\n*.log\n",
            "build.log",
        ],
    );
    let mut tree = Tree::open(&dir);
    let flags: Vec<(String, bool)> = tree
        .rows()
        .iter()
        .skip(1)
        .map(|row| (row.name().to_owned(), row.is_ignored()))
        .collect();
    assert_eq!(
        flags,
        [
            ("src".to_owned(), false),
            ("target".to_owned(), true),
            (".gitignore".to_owned(), false),
            ("build.log".to_owned(), true),
        ]
    );

    tree.expand(Path::new("target"));
    tree.expand(Path::new("target/debug"));
    assert!(
        tree.rows()
            .iter()
            .filter(|row| row.path().starts_with("target"))
            .all(|row| row.is_ignored()),
        "everything under an ignored directory is ignored"
    );
}

/// Reveal expands every directory on the way and answers the row.
#[test]
fn reveal_opens_the_way_to_a_file() {
    let dir = temp_project("reveal", &["crates/desktop/src/lib.rs", "README.md"]);
    let mut tree = Tree::open(&dir);
    let row = tree
        .reveal(Path::new("crates/desktop/src/lib.rs"))
        .expect("found");
    // **One row for the whole chain, since M8.10**: `crates` holds only
    // `desktop`, which holds only `src`. `names` says the deepest;
    // `label` is what the row draws.
    assert_eq!(entries(&tree), ["src", "lib.rs", "README.md"]);
    assert_eq!(
        tree.rows()[1].label(),
        "crates/desktop/src",
        "the chain says every name it stands for"
    );
    assert_eq!(row, 2, "after the root row and the chain's");
    assert_eq!(tree.reveal(Path::new("crates/nowhere.rs")), None);
}

/// A re-read of a loaded directory keeps what is expanded under it and
/// picks up what appeared and what went.
#[test]
fn refreshing_a_directory_keeps_the_expansion_and_follows_the_disk() {
    let dir = temp_project("refresh", &["src/a.rs", "src/deep/x.rs"]);
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("src"));
    tree.expand(Path::new("src/deep"));
    assert_eq!(entries(&tree), ["src", "deep", "x.rs", "a.rs"]);

    std::fs::write(dir.join("src/b.rs"), "").unwrap();
    std::fs::remove_file(dir.join("src/a.rs")).unwrap();
    tree.refresh(Path::new("src"));
    assert_eq!(entries(&tree), ["src", "deep", "x.rs", "b.rs"]);

    tree.refresh(Path::new("never/loaded"));
    assert_eq!(
        entries(&tree),
        ["src", "deep", "x.rs", "b.rs"],
        "a refresh of nothing changes nothing"
    );
}

/// The expanded set survives a round trip through its own list — that is
/// what the state file stores — and comes back expanded on open.
#[test]
fn the_expanded_set_comes_back_on_open() {
    let dir = temp_project("expanded", &["a/b/c.rs", "d/e.rs"]);
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("a"));
    tree.expand(Path::new("a/b"));
    let expanded: Vec<PathBuf> = tree.expanded().map(Path::to_path_buf).collect();
    assert_eq!(
        expanded,
        [PathBuf::new(), PathBuf::from("a"), PathBuf::from("a/b")],
        "the root is expanded too, and stored as the empty path"
    );

    let back = Tree::with_expanded(&dir, &expanded);
    // `a` holds only `b`, so the two are one row since M8.10.
    assert_eq!(entries(&back), ["b", "c.rs", "d"]);
    assert_eq!(back.rows()[1].label(), "a/b");
}

/// Opening this repository reads its root and nothing else: `target/` is
/// ignored and unread, and the open is a blink. The time is printed
/// rather than asserted — a machine under load would fail a number.
///
/// **The claim about `target/` is made only when there is one.** It
/// asserts the design decision that an ignored entry is *dimmed rather
/// than hidden*, which needs an ignored entry to be there — and a
/// checkout that has never been built has none. A worktree with its own
/// `CARGO_TARGET_DIR` is the case that found this: the test reddened a
/// branch that had not touched a tree, and an hour went into reading it
/// as a bug in the pass rather than a precondition in the test. The
/// other two claims hold in every checkout and are asserted
/// unconditionally.
#[test]
fn opening_this_repository_does_not_read_target() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let started = std::time::Instant::now();
    let tree = Tree::open(&root);
    let elapsed = started.elapsed();
    eprintln!("Tree::open on the repository: {elapsed:?}");
    assert!(!tree.is_loaded(Path::new("target")));
    assert!(tree.rows().iter().all(|row| row.name() != ".git"));

    if !root.join("target").is_dir() {
        eprintln!("no target/ in this checkout: the dimming claim is not made");
        return;
    }
    assert!(
        tree.rows()
            .iter()
            .any(|row| row.name() == "target" && row.is_ignored()),
        "target is there, dimmed"
    );
}

/// The root is the first row — the project's own name, expanded, with
/// everything one level in — and it folds
/// like any directory. Revealing a file opens it again.
#[test]
fn the_root_is_the_first_row_and_folds() {
    let dir = temp_project("root", &["src/", "README.md"]);
    let mut tree = Tree::open(&dir);
    let rows = tree.rows();
    let root = rows.first().expect("the root's row");
    assert_eq!(root.name(), dir.file_name().unwrap().to_str().unwrap());
    assert_eq!(root.depth(), 0);
    assert!(root.is_dir() && root.is_expanded() && !root.is_ignored());
    assert_eq!(root.path(), Path::new(""));
    assert_eq!(depths(&tree), [0, 1, 1]);

    tree.collapse(Path::new(""));
    assert_eq!(names(&tree).len(), 1, "a folded root is one row");
    assert!(!tree.rows()[0].is_expanded());

    assert_eq!(
        tree.reveal(Path::new("README.md")),
        Some(2),
        "reveal opens the root again"
    );
}

/// **Collapsing takes everything under it with it.**
///
/// Shutting one row and leaving its descendants open looks identical at
/// the moment it is done, and is a different tree the next time the row
/// is opened: the eight directories a person spread out an hour ago come
/// back with it, which is the state they collapsed it to be rid of.
#[test]
fn collapsing_a_directory_collapses_what_is_inside_it() {
    let dir = temp_project(
        "recursive-collapse",
        &[
            "src/root/one.rs",
            "src/view/two.rs",
            "src/view/deep/three.rs",
            "docs/a.md",
        ],
    );
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("src"));
    tree.expand(Path::new("src/root"));
    tree.expand(Path::new("src/view"));
    tree.expand(Path::new("src/view/deep"));
    tree.expand(Path::new("docs"));

    tree.collapse(Path::new("src"));

    let open: Vec<String> = tree
        .expanded()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    assert!(
        !open.iter().any(|p| p.starts_with("src")),
        "nothing under src stays open: {open:?}"
    );
    assert!(
        open.iter().any(|p| p == "docs"),
        "and a sibling is untouched: {open:?}"
    );

    tree.expand(Path::new("src"));
    let shown: Vec<String> = tree
        .rows()
        .iter()
        .map(|row| row.path().to_string_lossy().into_owned())
        .collect();
    assert!(
        shown.iter().any(|p| p == "src/root"),
        "src is open again: {shown:?}"
    );
    assert!(
        !shown.iter().any(|p| p == "src/root/one.rs"),
        "and its children are shut, not as they were an hour ago: {shown:?}"
    );
}

/// Collapsing the project is collapsing the root, and the root is a
/// prefix of everything.
#[test]
fn collapsing_the_project_shuts_the_whole_tree() {
    let dir = temp_project("collapse-all", &["src/a.rs", "docs/b.md"]);
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("src"));
    tree.expand(Path::new("docs"));

    tree.collapse(Path::new(""));

    assert_eq!(tree.expanded().count(), 0);
}

/// **What a typed name means, and what it may not mean.** M8.8.3: the
/// tree's *New File…*, *New Directory…* and *Rename…* all end in one
/// question — which path did that name ask for, under this directory —
/// and the answer has to refuse a name that leaves the directory it was
/// typed in.
#[test]
fn a_typed_name_becomes_a_path_under_the_directory_it_was_typed_in() {
    let src = Path::new("crates/desktop/src");
    assert_eq!(
        guild_desktop::tree::under(src, "tree.rs"),
        Some(PathBuf::from("crates/desktop/src/tree.rs"))
    );
    // Typed at the project's own row, whose path is the empty one.
    assert_eq!(
        guild_desktop::tree::under(Path::new(""), "README.md"),
        Some(PathBuf::from("README.md"))
    );
    // A trailing space is a typo, not a name: a file called `a.rs ` is
    // one a person can never find again in this tree.
    assert_eq!(
        guild_desktop::tree::under(src, "  a.rs  "),
        Some(PathBuf::from("crates/desktop/src/a.rs"))
    );
    // **A `/` makes the directories on the way**, as New File commonly
    // does, which saves a person three gestures.
    assert_eq!(
        guild_desktop::tree::under(src, "view/menu/tree.rs"),
        Some(PathBuf::from("crates/desktop/src/view/menu/tree.rs"))
    );
}

/// A name that would leave the directory is not a name.
///
/// The field is typed into by hand and `..` is one keystroke from `.`.
/// The tree is the one surface of Guild's own that writes to the disk,
/// so the rule that keeps a write inside the project lives here, where a
/// test can read it without a window.
#[test]
fn a_name_that_climbs_out_is_refused() {
    let src = Path::new("crates/desktop/src");
    for typed in [
        "",
        "   ",
        ".",
        "..",
        "../x.rs",
        "a/../../x.rs",
        "/etc/passwd",
    ] {
        assert_eq!(
            guild_desktop::tree::under(src, typed),
            None,
            "{typed:?} was allowed"
        );
    }
}

/// **A chain of directories with one child each is one row** — compact
/// middle packages, M8.10. Drawn a row each, `com/example/inaction` makes
/// a Kotlin tree half as wide again, and the person asked for one row.
#[test]
fn a_chain_of_single_child_directories_is_one_row() {
    let dir = temp_project(
        "chain",
        &[
            "src/com/example/inaction/App.kt",
            "src/com/example/inaction/Bot.kt",
            "README.md",
        ],
    );
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("src"));

    let labels: Vec<String> = tree
        .rows()
        .iter()
        .map(|row| row.label().to_owned())
        .collect();
    // **The chain starts where the single child does**, which here is
    // `src` itself, rather than under a source root, because we have no
    // source roots.
    assert!(
        labels.contains(&"src/com/example/inaction".to_owned()),
        "the chain is one row: {labels:?}"
    );
    assert!(
        !labels.iter().any(|label| label == "example"),
        "no row of its own for a directory in the middle: {labels:?}"
    );

    let chain = tree
        .rows()
        .iter()
        .find(|row| row.label() == "src/com/example/inaction")
        .map(|row| {
            (
                row.path().to_owned(),
                row.chain_root().to_owned(),
                row.depth(),
            )
        })
        .expect("the chain's row");
    // **The deeds aim at the deepest**, as New… commonly does, and the
    // fold is the topmost — collapsing the row must take the whole chain.
    assert_eq!(chain.0, Path::new("src/com/example/inaction"));
    assert_eq!(chain.1, Path::new("src"));
    // One level under the root, whatever the chain's length.
    assert_eq!(chain.2, 1);

    // The files under it are one level further in, and they are there:
    // expanding the top of a chain reads the whole chain.
    let files: Vec<String> = tree
        .rows()
        .iter()
        .filter(|row| !row.is_dir())
        .map(|row| row.label().to_owned())
        .collect();
    assert!(files.contains(&"App.kt".to_owned()), "{files:?}");

    // Folding it by its root takes the whole chain away.
    tree.collapse(Path::new("src"));
    let labels: Vec<String> = tree
        .rows()
        .iter()
        .map(|row| row.label().to_owned())
        .collect();
    assert!(
        labels.contains(&"src".to_owned()),
        "folded, it is its own name: {labels:?}"
    );
    assert!(!labels.contains(&"App.kt".to_owned()), "{labels:?}");
}

/// A directory with a sibling beside it is not a chain, however deep.
#[test]
fn a_directory_with_two_children_is_a_row_of_its_own() {
    let dir = temp_project(
        "no-chain",
        &["src/com/example/App.kt", "src/com/other/Bot.kt"],
    );
    let mut tree = Tree::open(&dir);
    tree.expand(Path::new("src"));

    let labels: Vec<String> = tree
        .rows()
        .iter()
        .map(|row| row.label().to_owned())
        .collect();
    // `src` holds only `com`, so those two are a chain; `com` holds two, so
    // the chain stops there and its children are rows of their own.
    assert!(labels.contains(&"src/com".to_owned()), "{labels:?}");
    assert!(labels.contains(&"other".to_owned()), "{labels:?}");
    assert!(labels.contains(&"example".to_owned()), "{labels:?}");
}

/// **A selection survives a chain forming and breaking under it.**
///
/// The person found the arrows dead in the tree's directories. They were
/// not: clicking `cfg` expanded it into the chain `cfg/guild`, whose row
/// is called by its deepest path, so the selection — `cfg` — named no row
/// any more. Nothing was lit, and the next `down` found no current row
/// and went to the top. Collapsing does the same the other way: the
/// chain's row becomes `cfg` again and a selection of `cfg/guild` is lost.
///
/// `row_covering` answers for any path on a chain, which is what the
/// selection is settled against after every fold.
#[test]
fn every_path_on_a_chain_finds_the_chains_row() {
    let dir = temp_project("covering", &["cfg/guild/hooks/a.sh", "cfg/guild/b.json"]);
    let mut tree = Tree::open(&dir);

    let row = |tree: &Tree, path: &str| {
        tree.row_covering(Path::new(path))
            .map(|index| tree.rows()[index].path().to_owned())
    };

    // Folded: `cfg` is a row of its own, and what is under it is no row.
    assert_eq!(row(&tree, "cfg"), Some(PathBuf::from("cfg")));
    assert_eq!(row(&tree, "cfg/guild"), None, "not drawn while folded");

    // Expanded into a chain: both paths are the chain's row.
    tree.expand(Path::new("cfg"));
    assert_eq!(row(&tree, "cfg"), Some(PathBuf::from("cfg/guild")));
    assert_eq!(row(&tree, "cfg/guild"), Some(PathBuf::from("cfg/guild")));
    // A child of the chain's end is its own row, not the chain's.
    assert_eq!(
        row(&tree, "cfg/guild/b.json"),
        Some(PathBuf::from("cfg/guild/b.json"))
    );

    // Folded again: the deep path finds the top's row.
    tree.collapse(Path::new("cfg"));
    assert_eq!(
        row(&tree, "cfg/guild"),
        None,
        "folded away, and not the top's row"
    );
    assert_eq!(row(&tree, "cfg"), Some(PathBuf::from("cfg")));

    // The root is its own row, and nothing covers a path not in the tree.
    assert_eq!(row(&tree, ""), Some(PathBuf::from("")));
    assert_eq!(row(&tree, "nowhere"), None);
}
