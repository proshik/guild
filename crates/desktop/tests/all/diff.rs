//! The hunks between two texts, on hand-written pairs: the rows a list
//! draws, with the numbers a gutter shows.

use std::time::{Duration, Instant};

use guild_desktop::diff::{Hunk, Row, counts, hunks, rows};

fn added(new: usize, text: &str) -> Row {
    Row::Added {
        new,
        text: text.to_owned(),
    }
}
fn removed(old: usize, text: &str) -> Row {
    Row::Removed {
        old,
        text: text.to_owned(),
    }
}
fn context(old: usize, new: usize, text: &str) -> Row {
    Row::Context {
        old,
        new,
        text: text.to_owned(),
    }
}

/// Two equal texts have nothing to say.
#[test]
fn equal_texts_give_no_hunks() {
    assert!(hunks("a\nb\n", "a\nb\n").is_empty());
    assert_eq!(counts(&hunks("a\n", "a\n")), (0, 0));
}

/// One added line in the middle: one hunk, three lines of context on
/// each side, and the numbers of both sides right.
#[test]
fn one_added_line_is_one_hunk_with_its_context() {
    let old = "1\n2\n3\n4\n5\n6\n7\n8\n9\n";
    let new = "1\n2\n3\n4\nnew\n5\n6\n7\n8\n9\n";
    let found = hunks(old, new);
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].rows,
        vec![
            Row::Header {
                old_start: 2,
                old_len: 6,
                new_start: 2,
                new_len: 7
            },
            context(2, 2, "2"),
            context(3, 3, "3"),
            context(4, 4, "4"),
            added(5, "new"),
            context(5, 6, "5"),
            context(6, 7, "6"),
            context(7, 8, "7"),
        ]
    );
    assert_eq!(counts(&found), (1, 0));
}

/// A change at the very top has no context above it, and a removed line
/// keeps the old side's number.
#[test]
fn a_change_at_the_top_has_no_context_above() {
    let found = hunks("first\na\nb\nc\nd\n", "a\nb\nc\nd\n");
    assert_eq!(
        found[0].rows,
        vec![
            Row::Header {
                old_start: 1,
                old_len: 4,
                new_start: 1,
                new_len: 3
            },
            removed(1, "first"),
            context(2, 1, "a"),
            context(3, 2, "b"),
            context(4, 3, "c"),
        ]
    );
}

/// Two changes close together share a hunk; far apart they are two.
#[test]
fn nearby_changes_share_a_hunk_and_distant_ones_do_not() {
    let base: Vec<String> = (1..=20).map(|n| n.to_string()).collect();
    let mut close = base.clone();
    close[2] = "x".into();
    close[8] = "y".into(); // six lines apart: the contexts touch
    let found = hunks(&(base.join("\n") + "\n"), &(close.join("\n") + "\n"));
    assert_eq!(found.len(), 1, "six apart is one hunk: {found:#?}");

    let mut far = base.clone();
    far[2] = "x".into();
    far[12] = "y".into(); // ten apart: three lines of context each leave a gap
    let found = hunks(&(base.join("\n") + "\n"), &(far.join("\n") + "\n"));
    assert_eq!(found.len(), 2, "ten apart is two hunks: {found:#?}");
    assert_eq!(counts(&found), (2, 2));
}

/// A last line without a newline is a line of its own with git's note
/// after it, not a changed line hidden in the ending.
#[test]
fn a_missing_final_newline_is_noted_not_hidden() {
    let found = hunks("a\nb", "a\nb\n");
    let all = rows(&found);
    assert_eq!(
        all,
        vec![
            Row::Header {
                old_start: 1,
                old_len: 2,
                new_start: 1,
                new_len: 2
            },
            context(1, 1, "a"),
            removed(2, "b"),
            Row::NoNewline,
            added(2, "b"),
        ]
    );
}

/// Line endings are not changes: CRLF on one side and LF on the other
/// diff by content.
#[test]
fn line_endings_do_not_count_as_changes() {
    assert!(hunks("a\r\nb\r\n", "a\nb\n").is_empty());
    let found = hunks("a\r\nb\r\n", "a\nc\n");
    assert_eq!(counts(&found), (1, 1));
    assert_eq!(rows(&found)[2], removed(2, "b"), "no \\r in the text");
}

/// The flattened rows keep every hunk's header in front of it.
#[test]
fn rows_keep_every_header() {
    let base: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
    let mut edited = base.clone();
    edited[1] = "x".into();
    edited[25] = "y".into();
    let all = rows(&hunks(
        &(base.join("\n") + "\n"),
        &(edited.join("\n") + "\n"),
    ));
    let headers = all
        .iter()
        .filter(|r| matches!(r, Row::Header { .. }))
        .count();
    assert_eq!(headers, 2);
    assert!(matches!(all[0], Row::Header { .. }));
}

/// The 3,000-line file of task 0 with fifty edits, in the time task 0
/// measured — with room for a loaded machine.
#[test]
fn a_large_file_diffs_in_the_time_measured() {
    let base: Vec<String> = (1..=3000)
        .map(|n| format!("line {n} of the file"))
        .collect();
    let mut edited = base.clone();
    for i in 0..50 {
        edited[i * 60 + 3].push_str(" // changed");
    }
    let (old, new) = (base.join("\n") + "\n", edited.join("\n") + "\n");
    let started = Instant::now();
    let found = hunks(&old, &new);
    let took = started.elapsed();
    assert_eq!(found.len(), 50);
    assert_eq!(counts(&found), (50, 50));
    assert!(
        took < Duration::from_millis(200),
        "task 0 measured 11 ms in debug; this took {took:?}"
    );
}

/// The two columns from the rows, as every side-by-side viewer aligns
/// them: a context line on both sides, a removed line on the left with
/// an empty right, an added line on the right with an empty left, and a
/// removed line followed by an added one paired on one row — two
/// removed and one added are one pair and then one alone. The numbers
/// of both sides come through as they were.
#[test]
fn sides_pair_a_removed_line_with_the_added_one_that_follows() {
    use guild_desktop::diff::{Pair, sides};
    let hunk = Hunk {
        old: 0..5,
        new: 0..5,
        rows: vec![
            Row::Header {
                old_start: 1,
                old_len: 5,
                new_start: 1,
                new_len: 5,
            },
            context(1, 1, "same"),
            removed(2, "gone"),
            added(2, "here instead"),
            context(3, 3, "still"),
            removed(4, "first out"),
            removed(5, "second out"),
            added(4, "one in"),
            added(5, "alone in"),
            Row::NoNewline,
        ],
    };
    let pairs = sides(&[hunk]);
    assert_eq!(
        pairs,
        [
            Pair::Header {
                old_start: 1,
                old_len: 5,
                new_start: 1,
                new_len: 5
            },
            Pair::Context {
                old: 1,
                new: 1,
                text: "same".into()
            },
            Pair::Changed {
                old: Some((2, "gone".into())),
                new: Some((2, "here instead".into())),
            },
            Pair::Context {
                old: 3,
                new: 3,
                text: "still".into()
            },
            Pair::Changed {
                old: Some((4, "first out".into())),
                new: Some((4, "one in".into())),
            },
            Pair::Changed {
                old: Some((5, "second out".into())),
                new: Some((5, "alone in".into())),
            },
            Pair::NoNewline,
        ]
    );
}

/// A removed line with nothing added after it has an empty right; an
/// added line with nothing removed before it has an empty left. The
/// pairing does not reach across a context line.
#[test]
fn sides_leave_a_half_empty_when_nothing_faces_it() {
    use guild_desktop::diff::{Pair, sides};
    let hunk = Hunk {
        old: 0..3,
        new: 0..3,
        rows: vec![
            removed(1, "only gone"),
            context(2, 1, "kept"),
            added(2, "only new"),
            context(3, 3, "end"),
        ],
    };
    let pairs = sides(&[hunk]);
    assert_eq!(
        pairs,
        [
            Pair::Changed {
                old: Some((1, "only gone".into())),
                new: None,
            },
            Pair::Context {
                old: 2,
                new: 1,
                text: "kept".into()
            },
            Pair::Changed {
                old: None,
                new: Some((2, "only new".into())),
            },
            Pair::Context {
                old: 3,
                new: 3,
                text: "end".into()
            },
        ]
    );
    assert!(sides(&[]).is_empty());
}
