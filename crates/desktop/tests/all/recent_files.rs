//! The Recent Files popup: the list, the query, and the row the
//! highlight opens on.
//!
//! All of it without a window. What is checked here is what a caller can
//! reach — and the numbers the popup is drawn with, against the design
//! system, because a token that drifts from the document is a token
//! nobody is checking.

use std::path::{Path, PathBuf};

use gpui::Keystroke;
use guild_desktop::root::recent_files::{
    EXCERPT, REMEMBERED, closes_the_lit_file, excerpt, forget, opening_row, remember, shown,
};
use guild_desktop::scale::BASE;
use guild_desktop::theme::space;

fn files() -> Vec<PathBuf> {
    vec![
        PathBuf::from("crates/desktop/src/root/history.rs"),
        PathBuf::from("crates/desktop/src/lib.rs"),
        PathBuf::from("docs/plan.md"),
    ]
}

/// **The trap that is the whole feature.** `cmd-e` then Enter has to
/// land on the file a person was just in, which it only does if the
/// highlight opens *below* the file already in front.
#[test]
fn cmd_e_then_enter_lands_on_the_last_file() {
    assert_eq!(
        opening_row(3, true),
        1,
        "row 0 is the file being looked at, so the highlight starts under it"
    );
    assert_eq!(opening_row(3, false), 0);
    assert_eq!(opening_row(1, true), 0, "one row has nowhere else to be");
}

/// The query narrows by the whole path, because a name alone does not
/// tell two `mod.rs` apart — and every row keeps its place in the full
/// list, which is what opening and closing count.
#[test]
fn a_query_narrows_by_the_path_and_a_row_keeps_its_place() {
    let all = files();

    assert_eq!(shown("", &all).len(), 3);
    assert_eq!(shown("HISTORY", &all).len(), 1, "case does not matter");
    assert_eq!(
        shown("docs", &all)[0].0,
        2,
        "the third row, in the full list"
    );
    assert_eq!(shown("nothing at all", &all).len(), 0);
}

/// A file is found by its scattered letters, as every popup's list is
/// since M8.9 — `crate::fuzzy`, in path mode.
#[test]
fn a_file_is_found_by_its_scattered_letters() {
    let all = vec![
        PathBuf::from("src/lib.rs"),
        PathBuf::from("src/root/history.rs"),
        PathBuf::from("docs/plan.md"),
    ];

    let rows = shown("rhis", &all);
    assert_eq!(
        rows.first().map(|(index, path)| (*index, *path)),
        Some((1, Path::new("src/root/history.rs"))),
        "rhis is r(oot) and his(tory)"
    );
}

/// Opening a file again moves it to the head; the list holds fifty.
#[test]
fn the_list_is_a_most_recently_used_fifty() {
    let mut list = files();
    remember(&mut list, Path::new("docs/plan.md"));
    assert_eq!(list[0], PathBuf::from("docs/plan.md"));
    assert_eq!(list.len(), 3, "it moved rather than being added");

    forget(&mut list, Path::new("docs/plan.md"));
    assert_eq!(list.len(), 2);

    let mut long = Vec::new();
    for index in 0..120 {
        remember(&mut long, Path::new(&format!("f{index}.rs")));
    }
    assert_eq!(long.len(), REMEMBERED);
    assert_eq!(REMEMBERED, 50, "the common limit");
}

/// Section 6.25's numbers are the tokens, and the document is read
/// rather than trusted: a design system nobody checks is a document that
/// drifts.
#[test]
fn the_popup_is_the_document_s_size() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/design-system.md");
    let document = std::fs::read_to_string(path).expect("the design system");
    let section = document
        .split("### 6.25 Recent Files")
        .nth(1)
        .expect("section 6.25");
    let px = |rems: gpui::Rems| rems.0 * BASE;

    assert!(
        section.contains("520 wide") && section.contains("hangs 120 below"),
        "the document says how wide it is and where it hangs"
    );
    assert_eq!(px(space::RECENT_FILES), 520.0);
    assert_eq!(px(space::RECENT_FILES_TOP), 120.0);

    assert!(section.contains("420 tall"));
    assert_eq!(px(space::RECENT_FILES_LIST), 420.0);

    assert!(section.contains("22 high at radius 6"));
    assert_eq!(px(space::TREE_ROW), 22.0);

    // The same card is the Go to Line field, and the document says so.
    assert!(section.contains("240 wide"));
    assert_eq!(px(space::GOTO_LINE), 240.0);
    assert!(
        section.contains("five lines of code in the"),
        "and the location row's excerpt"
    );
}

/// A location shows five lines with the place in the middle, and the
/// number of the first of them — a row without its line numbers is an
/// excerpt of nowhere.
#[test]
fn a_location_shows_five_lines_around_the_place() {
    let text: String = (0..20).map(|n| format!("line {n}\n")).collect();

    let (lines, first) = excerpt(Some(&text), 10);
    assert_eq!(lines.len(), EXCERPT);
    assert_eq!(first, 9, "two lines above, counted from one");
    assert_eq!(lines[2], "line 10", "the place's own line is the middle");
}

/// Near either end there is nothing beyond the file to show, and the
/// window slides rather than padding with rows that do not exist.
#[test]
fn an_excerpt_is_clamped_to_the_file() {
    let text: String = (0..20).map(|n| format!("line {n}\n")).collect();

    let (lines, first) = excerpt(Some(&text), 0);
    assert_eq!(first, 1);
    assert_eq!(lines[0], "line 0");

    let (lines, first) = excerpt(Some(&text), 19);
    assert_eq!(lines.len(), EXCERPT);
    assert_eq!(first, 16, "the window slid up to keep five lines");
    assert_eq!(lines[4], "line 19");

    let (lines, _) = excerpt(Some("only one\n"), 0);
    assert_eq!(lines, vec!["only one".to_owned()]);
}

/// A file that could not be read has no excerpt, and the row still knows
/// which line it names.
#[test]
fn a_file_that_is_gone_has_no_excerpt() {
    let (lines, first) = excerpt(None, 41);

    assert!(lines.is_empty());
    assert_eq!(first, 42);
}

/// **The Backspace key closes the lit file, and nothing else does.** While
/// nothing is typed on the Files face, the Switcher closes the file on
/// Backspace — with a modifier too, as the popup did before M8.9. Task 3
/// read the key by what it means to the line, so `ctrl-w` and `ctrl-u`,
/// which mean *take a word* and *take the line* to readline, closed a file
/// as well: a hand that rubs out an empty query out of habit lost a tab.
#[test]
fn only_the_backspace_key_closes_the_lit_file() {
    let stroke = |text: &str| Keystroke::parse(text).expect("a stroke parses");

    for key in ["backspace", "alt-backspace", "cmd-backspace"] {
        assert!(closes_the_lit_file(&stroke(key), true, ""), "{key}");
        assert!(
            !closes_the_lit_file(&stroke(key), true, "lib"),
            "{key} over a query is the query's"
        );
        assert!(
            !closes_the_lit_file(&stroke(key), false, ""),
            "{key} on the Locations face closes nothing"
        );
    }
    for key in ["ctrl-w", "ctrl-u", "delete", "a"] {
        assert!(!closes_the_lit_file(&stroke(key), true, ""), "{key}");
    }
}
