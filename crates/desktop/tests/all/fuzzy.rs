//! A list found by its scattered letters: the one matcher every popup's
//! query goes through, and what it marks.
//!
//! Decision 5 of M8.9. The examples are task 0's, measured against
//! `nucleo-matcher` 0.3.1 before anything rested on them.

use guild_desktop::fuzzy::{Shape, find, rank, spans};

/// The audit's own example: four letters, each the start of something.
#[test]
fn a_query_finds_a_subsequence_of_the_path() {
    let found = find("gdsk", "guild/crates/desktop", Shape::Path).expect("gdsk finds it");
    assert!(found.score > 0);
    assert_eq!(
        found.marks.len(),
        4,
        "one mark per letter: {:?}",
        found.marks
    );
    assert!(find("gdskq", "guild/crates/desktop", Shape::Path).is_none());
}

/// `fb` is `foo_bar` before it is two letters somewhere in `xfxxbx`.
#[test]
fn a_match_on_word_boundaries_outranks_a_scattered_one() {
    let items = ["xfxxbx.rs", "foo_bar.rs"];
    let ranked = rank("fb", &items, |item| item, Shape::Path);
    let order: Vec<&str> = ranked.iter().map(|(index, _)| items[*index]).collect();
    assert_eq!(order, ["foo_bar.rs", "xfxxbx.rs"]);
    assert_eq!(ranked[0].1.marks, [0, 4]);
}

/// Nobody holds shift to search, and a name typed in capitals is the same
/// name.
#[test]
fn case_is_ignored_both_ways() {
    assert!(find("ORCA", "orca", Shape::Name).is_some());
    assert!(find("orca", "ORCA", Shape::Name).is_some());
    assert!(find("ORCA", "src/orca.rs", Shape::Path).is_some());
}

/// Words separated by spaces each have to be there.
#[test]
fn every_word_must_match() {
    assert!(find("crates desk", "crates/desktop", Shape::Path).is_some());
    assert!(find("crates zzz", "crates/desktop", Shape::Path).is_none());
}

/// fzf's syntax is off: `^` is a character a file name can hold, not an
/// anchor.
#[test]
fn a_pattern_character_is_a_character() {
    assert!(find("^a", "ab", Shape::Name).is_none());
    assert!(find("^a", "x^a", Shape::Name).is_some());
    assert!(find("!a", "a", Shape::Name).is_none());
}

/// Nothing typed is every row, in the order the list already had —
/// recency, git's order, the recipe order — and nothing marked.
#[test]
fn an_empty_query_keeps_the_list_s_order() {
    let items = ["zeta", "alpha", "mid"];
    let ranked = rank("", &items, |item| item, Shape::Path);
    let order: Vec<usize> = ranked.iter().map(|(index, _)| *index).collect();
    assert_eq!(order, [0, 1, 2]);
    assert!(ranked.iter().all(|(_, found)| found.marks.is_empty()));
    let spaces = rank("   ", &items, |item| item, Shape::Path);
    assert_eq!(spaces.len(), 3, "a query of spaces is nothing typed");
}

/// Two rows that score the same keep the list's order between them.
#[test]
fn ties_keep_the_list_s_order() {
    let items = ["b/one.rs", "a/one.rs", "c/one.rs"];
    let ranked = rank("one", &items, |item| item, Shape::Path);
    assert_eq!(ranked.len(), 3);
    assert!(
        ranked
            .windows(2)
            .all(|pair| pair[0].1.score == pair[1].1.score)
    );
    let order: Vec<usize> = ranked.iter().map(|(index, _)| *index).collect();
    assert_eq!(order, [0, 1, 2]);
}

/// The marks are characters, and a painter needs bytes on character
/// boundaries — a Cyrillic directory would otherwise take the window down.
#[test]
fn the_marks_are_byte_ranges_on_character_boundaries() {
    let path = "заметки/main.rs";
    let found = find("змmain", path, Shape::Path).expect("found");
    let ranges = spans(path, &found.marks);
    assert!(!ranges.is_empty());
    let mut painted = String::new();
    for range in &ranges {
        assert!(path.is_char_boundary(range.start) && path.is_char_boundary(range.end));
        painted.push_str(&path[range.clone()]);
    }
    assert_eq!(painted.chars().count(), found.marks.len());
    assert!(
        painted.ends_with("main"),
        "neighbours are merged: {ranges:?}"
    );
    assert_eq!(
        ranges.last().unwrap(),
        &(path.find("main").unwrap()..path.find("main").unwrap() + 4)
    );

    let found = find("mod", "заметки/mod.rs", Shape::Path).expect("found");
    assert_eq!(
        found.marks,
        [8, 9, 10],
        "characters, where bytes would be 15-17"
    );
}

/// A combining accent is its own character; the mark after it is still
/// the letter it names. With the matcher's grapheme feature on, this is a
/// byte offset or a grapheme index by accident of the text.
#[test]
fn a_combining_mark_does_not_shift_the_marks() {
    let path = "e\u{301}x.rs";
    let found = find("x", path, Shape::Path).expect("found");
    let ranges = spans(path, &found.marks);
    let x = path.find('x').unwrap();
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0], x..x + 1);
}
