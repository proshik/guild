//! A search's hits as a tree of files — M8.7's task 4, from outside.

use std::collections::HashSet;
use std::path::PathBuf;

use guild_desktop::search::Hit;
use guild_desktop::search_tree::{self, Row};

/// A hit with nothing interesting about it but where it is.
fn hit(path: &str, line: u32) -> Hit {
    Hit {
        path: PathBuf::from(path),
        line,
        text: format!("line {line}"),
        ranges: Vec::new(),
        cut: false,
        column: 0,
    }
}

/// What each row says, in one line, so a test can read the shape.
fn shape(rows: &[Row]) -> Vec<String> {
    rows.iter()
        .map(|row| match row {
            Row::Directory {
                label, hits, depth, ..
            } => format!("{depth} dir {label} ({hits})"),
            Row::File {
                name, hits, depth, ..
            } => format!("{depth} file {name} ({hits})"),
            Row::Hit { at, depth } => format!("{depth} hit {at}"),
        })
        .collect()
}

#[test]
fn hits_are_grouped_under_their_files_and_directories() {
    let hits = vec![
        hit("src/one.rs", 3),
        hit("src/one.rs", 9),
        hit("src/two.rs", 1),
    ];

    let rows = search_tree::of(&hits, &HashSet::new());

    assert_eq!(
        shape(&rows),
        [
            "0 dir src (3)",
            "1 file one.rs (2)",
            "2 hit 0",
            "2 hit 1",
            "1 file two.rs (1)",
            "2 hit 2",
        ]
    );
}

#[test]
fn a_run_of_single_child_directories_is_one_row() {
    // The rows a person walks past to reach a file are the ones worth
    // removing — `files_tree`'s rule, and the same one here.
    let hits = vec![hit("crates/desktop/tests/all/search.rs", 1)];

    let rows = search_tree::of(&hits, &HashSet::new());

    assert_eq!(
        shape(&rows),
        [
            "0 dir crates/desktop/tests/all (1)",
            "1 file search.rs (1)",
            "2 hit 0",
        ]
    );
}

#[test]
fn a_folded_file_keeps_its_row_and_loses_its_hits() {
    let hits = vec![hit("src/one.rs", 3), hit("src/two.rs", 1)];
    let folded = HashSet::from([PathBuf::from("src/one.rs")]);

    let rows = search_tree::of(&hits, &folded);

    assert_eq!(
        shape(&rows),
        [
            "0 dir src (2)",
            "1 file one.rs (1)",
            "1 file two.rs (1)",
            "2 hit 1",
        ],
        "the file is still there to unfold, and still says how many it has"
    );
}

#[test]
fn a_folded_directory_takes_everything_under_it() {
    let hits = vec![hit("src/one.rs", 3), hit("docs/a.md", 1)];
    let folded = HashSet::from([PathBuf::from("src")]);

    let rows = search_tree::of(&hits, &folded);

    assert_eq!(
        shape(&rows),
        [
            "0 dir docs (1)",
            "1 file a.md (1)",
            "2 hit 1",
            "0 dir src (1)"
        ]
    );
}

#[test]
fn a_file_at_the_root_needs_no_directory_row() {
    let hits = vec![hit("README.md", 2)];

    let rows = search_tree::of(&hits, &HashSet::new());

    assert_eq!(shape(&rows), ["0 file README.md (1)", "1 hit 0"]);
}

#[test]
fn a_row_says_what_it_would_fold() {
    // The click and the keyboard both need this: a row is folded by its
    // path, and a hit row has none because a hit does not fold.
    let hits = vec![hit("src/one.rs", 3)];

    let rows = search_tree::of(&hits, &HashSet::new());

    assert_eq!(
        rows.iter().map(search_tree::foldable).collect::<Vec<_>>(),
        [
            Some(PathBuf::from("src")),
            Some(PathBuf::from("src/one.rs")),
            None,
        ]
    );
}

#[test]
fn nothing_found_is_no_rows_at_all() {
    assert!(search_tree::of(&[], &HashSet::new()).is_empty());
}

#[test]
fn enter_after_typing_is_on_a_hit_not_on_its_file() {
    // Row 0 is always a directory or a file, and Enter on either folds —
    // which is what `pick::settle` lit on a walk's first batch until
    // M5.11. The rule is checked here; that `take` uses it is window
    // check 1, because `SearchState` needs a `Context<Root>`.
    let hits = vec![hit("crates/a/b.rs", 3), hit("c.rs", 1)];

    let rows = search_tree::of(&hits, &HashSet::new());
    assert_eq!(
        search_tree::first_hit(&rows),
        Some(2),
        "the collapsed directory, the file, then the hit"
    );

    let folded = HashSet::from([PathBuf::from("crates/a/b.rs"), PathBuf::from("c.rs")]);
    let rows = search_tree::of(&hits, &folded);
    assert_eq!(search_tree::first_hit(&rows), None);
}

#[test]
fn a_kept_hit_is_found_again_after_its_line_moved() {
    // M5.11, decision 4: the highlight stays on the same hit through a
    // refresh, found by its path and its text nearest its old line — a
    // path and a line would lose it to the very prepend that made the
    // answer stale.
    let hits = vec![
        hit("docs/a.md", 1),
        // The same text further away, found first: the nearest wins, not
        // the first.
        Hit {
            text: "needle in docs".to_owned(),
            ..hit("docs/a.md", 12)
        },
        Hit {
            text: "needle in docs".to_owned(),
            ..hit("docs/a.md", 5)
        },
        Hit {
            text: "needle in docs".to_owned(),
            ..hit("docs/b.md", 3)
        },
        hit("docs/a.md", 9),
    ];
    let rows = search_tree::of(&hits, &HashSet::new());
    let kept = search_tree::Kept {
        path: PathBuf::from("docs/a.md"),
        line: 3,
        text: "needle in docs".to_owned(),
    };

    let found = search_tree::row_of_kept(&rows, &hits, &kept).expect("the hit is still there");

    assert_eq!(rows[found], Row::Hit { at: 2, depth: 2 });

    let gone = search_tree::Kept {
        text: "not there".to_owned(),
        ..kept
    };
    assert_eq!(search_tree::row_of_kept(&rows, &hits, &gone), None);
}

#[test]
fn the_highlight_stays_on_its_row_while_the_walk_lands_more_files() {
    // M5.11, decisions 2 and 4. Each batch of a walk is one file, `ignore`
    // walks in the directory's own order and the tree is sorted, so a
    // file that lands later can move rows in front of the lit one or
    // split a collapsed chain. A highlight kept by its number then sits
    // on a file, and Enter folds it — the bug decision 2 exists to fix.
    // So the highlight is kept by what it is on.
    let is_hit = |rows: &[Row], index: Option<usize>| {
        matches!(
            index.and_then(|index| rows.get(index)),
            Some(Row::Hit { .. })
        )
    };
    let growths = [
        // A later file sorts in front: `src` is a directory, and
        // directories come first.
        (vec![hit("README.md", 2)], hit("src/a.rs", 1)),
        // A later file splits the chain `crates/a` into `crates` and `a`.
        (vec![hit("crates/a/b.rs", 3)], hit("crates/c/d.rs", 1)),
    ];
    for (before, later) in growths {
        let rows = search_tree::of(&before, &HashSet::new());
        let lit = search_tree::lit_row(&rows, &before, None);
        assert!(
            is_hit(&rows, lit),
            "{:?}: the first batch lights a hit",
            shape(&rows)
        );
        let held = search_tree::lit_of(&rows, &before, lit.expect("lit"));
        let file = search_tree::lit_of(&rows, &before, lit.expect("lit") - 1);

        let mut after = before.clone();
        after.push(later);
        let grown = search_tree::of(&after, &HashSet::new());

        // The number is what the rule was, and it is on something else now.
        assert!(
            !is_hit(&grown, lit),
            "{:?}: the old row's number is no longer a hit",
            shape(&grown)
        );
        // Nobody has moved: the walk's first hit leads, batch after batch.
        let first = search_tree::lit_row(&grown, &after, None);
        assert!(is_hit(&grown, first), "{:?}", shape(&grown));
        assert_eq!(first, search_tree::first_hit(&grown));
        // A hit the highlight was held on is found where it went.
        let found = search_tree::lit_row(&grown, &after, held.as_ref());
        let Some(Row::Hit { at, .. }) = found.map(|index| &grown[index]) else {
            panic!("{:?}: the held hit is lost", shape(&grown));
        };
        assert_eq!(after[*at].path, before[0].path);
        // And a file row it was held on is that file's row.
        let found = search_tree::lit_row(&grown, &after, file.as_ref());
        let Some(Row::File { path, .. }) = found.map(|index| &grown[index]) else {
            panic!("{:?}: the held file is lost", shape(&grown));
        };
        assert_eq!(path, &before[0].path);
    }
}
