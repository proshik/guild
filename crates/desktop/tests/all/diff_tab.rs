//! The marks on the diff's bar: where each hunk sits as a share of the
//! list, pure — decision 7 of M8.3.1.

use std::path::Path;

use guild_desktop::diff_tab::{Painted, change_positions};

/// A hunk's mark sits where the hunk is: a header at row 25 of 100 is a
/// quarter of the way down the bar.
#[test]
fn marks_are_placed_by_the_headers_share_of_the_list() {
    assert_eq!(change_positions(&[0, 25, 50], 100), [0.0, 0.25, 0.5]);
    assert!(change_positions(&[], 100).is_empty());
    assert_eq!(
        change_positions(&[3], 0),
        [0.0],
        "an empty list puts the mark at the top rather than dividing by zero"
    );
}

/// A diff is coloured on both sides or on neither. One coloured column
/// beside a plain one reads as "the other side is not code", which is a
/// worse answer than two plain columns.
#[test]
fn a_diff_is_coloured_on_both_sides_or_on_neither() {
    let painted = Painted::of(Path::new("src/lib.rs"), "fn a() {}\n", "fn b() {}\n");
    assert!(painted.is_some(), "rust has a grammar");

    assert!(
        Painted::of(Path::new("notes.txt"), "one\n", "two\n").is_none(),
        "no grammar on either side means neither side is coloured"
    );
}

/// **A row is coloured only if it is the line the spans were taken
/// from.** The rows come from `git diff` and the spans from parsing the
/// blobs; they can disagree over CRLF, over a rev that moved under an
/// open tab, or over an off-by-one anywhere. Colouring such a row paints
/// a keyword-red `if` on a line that has no `if` in it, with no gap and
/// nothing in any log.
#[test]
fn a_row_that_is_not_its_line_is_drawn_plain() {
    let painted = Painted::of(Path::new("src/lib.rs"), "", "fn a() {}\nlet b = 1;\n")
        .expect("rust has a grammar");

    let spans = painted.new.of_line(1, "fn a() {}");
    assert!(
        spans.is_some_and(|spans| !spans.is_empty()),
        "the line as it is"
    );

    assert!(
        painted.new.of_line(1, "fn a() {} ").is_none(),
        "a trailing space is a different line"
    );
    assert!(
        painted.new.of_line(2, "fn a() {}").is_none(),
        "the right text at the wrong number is still wrong"
    );
    assert!(
        painted.new.of_line(99, "fn a() {}").is_none(),
        "past the end is not a panic"
    );
    assert!(
        painted.new.of_line(0, "fn a() {}").is_none(),
        "lines are numbered from one, and zero is not one of them"
    );
}

/// The spans follow the side they belong to: what `HEAD` has and what the
/// working tree has are parsed apart.
#[test]
fn each_side_is_parsed_on_its_own() {
    let painted = Painted::of(Path::new("src/lib.rs"), "const A: u8 = 1;\n", "fn a() {}\n")
        .expect("rust has a grammar");
    assert!(painted.old.of_line(1, "const A: u8 = 1;").is_some());
    assert!(painted.old.of_line(1, "fn a() {}").is_none());
    assert!(painted.new.of_line(1, "fn a() {}").is_some());
}

/// Past the limit a diff is plain, and it is **the tab's** limit: a file
/// coloured in one place and plain in another is the thing this pass
/// exists to stop.
#[test]
fn a_file_past_the_limit_is_plain_on_both_sides() {
    let big = "// a line\n"
        .repeat((guild_desktop::file_tab::HIGHLIGHT_LIMIT as usize / "// a line\n".len()) + 1);
    assert!(
        Painted::of(Path::new("src/lib.rs"), "fn a() {}\n", &big).is_none(),
        "one side past the limit leaves both plain"
    );
}
