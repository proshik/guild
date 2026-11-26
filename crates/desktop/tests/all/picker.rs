//! The picker: the one model every popup's list is chosen from.
//!
//! Decision 1 of M8.9. Recent Projects, Recent Files, the run list, the
//! branch popup and the font list each wrote the same seven rules for
//! themselves, and wrote two of them differently — two popups wrapped at
//! the bottom and three clamped. What is checked here is the rules once,
//! without a window.

use gpui::Keystroke;
use guild_desktop::edit::Edit;
use guild_desktop::picker::{Jump, Key, Lit, Picker, Take, key, page};

fn rows(count: usize) -> Vec<Lit> {
    vec![Lit::Row; count]
}

/// A heading, then two rows, a heading, then two rows — the branch
/// popup's shape when a name is typed.
fn sections() -> Vec<Lit> {
    vec![
        Lit::Heading,
        Lit::Row,
        Lit::Row,
        Lit::Heading,
        Lit::Row,
        Lit::Row,
    ]
}

fn typed(text: &str) -> Edit {
    Edit::Insert(text.to_owned())
}

/// Past the bottom is the top and above the top is the bottom, in every
/// popup. Popups cycle; a log of ten thousand commits does not,
/// and the docked lists keep `pick::step`.
#[test]
fn the_arrows_wrap_at_both_ends() {
    let list = rows(3);
    let mut picker = Picker::default();
    picker.settle(&list, 0);
    assert_eq!(picker.lit(), Some(0));

    picker.step(1, &list);
    assert_eq!(picker.lit(), Some(1));
    picker.step(1, &list);
    picker.step(1, &list);
    assert_eq!(picker.lit(), Some(0), "past the last row is the first");
    picker.step(-1, &list);
    assert_eq!(picker.lit(), Some(2), "above the first is the last");

    // A page is clamped at the end it runs into, and wraps only from the
    // end itself — a page down near the bottom is the bottom, not the top.
    let long = rows(20);
    let mut picker = Picker::default();
    picker.settle(&long, 0);
    picker.step(15, &long);
    assert_eq!(picker.lit(), Some(15));
    picker.step(15, &long);
    assert_eq!(picker.lit(), Some(19), "a page stops at the last row");
    picker.step(15, &long);
    assert_eq!(picker.lit(), Some(0), "and from the last row it wraps");

    let mut empty = Picker::default();
    empty.step(1, &[]);
    assert_eq!(empty.lit(), None, "an empty list has nothing to light");
}

/// A heading is a row that is never lit: stepped over, settled below,
/// and deaf to the pointer.
#[test]
fn a_heading_is_never_lit() {
    let list = sections();
    let mut picker = Picker::default();
    picker.settle(&list, 0);
    assert_eq!(
        picker.lit(),
        Some(1),
        "a list that starts with a heading lights below it"
    );

    picker.step(1, &list);
    assert_eq!(picker.lit(), Some(2));
    picker.step(1, &list);
    assert_eq!(picker.lit(), Some(4), "the heading between is stepped over");
    picker.step(-1, &list);
    assert_eq!(picker.lit(), Some(2));

    picker.step(-1, &list);
    picker.step(-1, &list);
    assert_eq!(
        picker.lit(),
        Some(5),
        "above the first row wraps past the heading"
    );

    assert!(
        !picker.point_at(3, &list),
        "the pointer over a heading changes nothing"
    );
    assert_eq!(picker.lit(), Some(5));

    picker.jump(Jump::First, &list);
    assert_eq!(picker.lit(), Some(1));
    picker.jump(Jump::Last, &list);
    assert_eq!(picker.lit(), Some(5));

    let mut headings_only = Picker::default();
    headings_only.settle(&[Lit::Heading, Lit::Heading], 0);
    assert_eq!(
        headings_only.lit(),
        None,
        "headings alone are nothing to take"
    );
}

/// **A list with rows keeps one of them lit**, or Enter answers
/// nothing — including when the list shrinks under the lit row.
#[test]
fn a_list_with_rows_keeps_one_lit() {
    let mut picker = Picker::default();
    picker.settle(&rows(5), 0);
    picker.step(-1, &rows(5));
    assert_eq!(picker.lit(), Some(4));

    picker.settle(&rows(2), 0);
    assert_eq!(
        picker.lit(),
        Some(1),
        "a shorter list keeps its last row lit"
    );

    picker.settle(&[], 0);
    assert_eq!(picker.lit(), None);
    picker.settle(&rows(3), 0);
    assert_eq!(picker.lit(), Some(0), "rows that come back light again");

    // A list whose lit row became a heading lights the next row.
    let mut picker = Picker::default();
    picker.settle(&rows(4), 0);
    picker.step(1, &rows(4));
    picker.settle(&[Lit::Row, Lit::Heading, Lit::Row], 0);
    assert_eq!(picker.lit(), Some(2));
}

/// What was lit was lit in the old list: a letter lights the first row
/// of the new one, so Enter takes the row a person has just narrowed to.
#[test]
fn typing_lights_the_first_row_of_the_new_list() {
    let mut picker = Picker::default();
    picker.settle(&rows(6), 0);
    picker.step(1, &rows(6));
    picker.step(1, &rows(6));
    assert_eq!(picker.lit(), Some(2));

    picker.edit(typed("g"));
    assert_eq!(picker.query(), "g");
    picker.settle(&rows(4), 0);
    assert_eq!(picker.lit(), Some(0));

    picker.edit(typed("u"));
    picker.settle(&sections(), 0);
    assert_eq!(picker.lit(), Some(1), "the first row, below a heading");
}

/// The row a popup opens on is its own rule — Recent Files opens below
/// the file in front — and it is applied once. Typing must not apply it
/// again, or it fights the query.
#[test]
fn the_opening_row_is_applied_once() {
    let mut picker = Picker::default();
    picker.settle(&rows(4), 1);
    assert_eq!(picker.lit(), Some(1));

    picker.settle(&rows(4), 1);
    assert_eq!(picker.lit(), Some(1));
    picker.step(1, &rows(4));
    picker.settle(&rows(4), 1);
    assert_eq!(
        picker.lit(),
        Some(2),
        "a second settle keeps what the arrows chose"
    );

    picker.edit(typed("x"));
    picker.settle(&rows(4), 1);
    assert_eq!(
        picker.lit(),
        Some(0),
        "after typing, the first row and not the opening one"
    );

    // A list that opens with nothing in it applies the opening row when
    // its rows arrive — an index lands after the popup is up.
    let mut late = Picker::default();
    late.settle(&[], 1);
    assert_eq!(late.lit(), None);
    late.settle(&rows(3), 1);
    assert_eq!(late.lit(), Some(1));
}

/// The pointer moves the one highlight the arrows move, and says whether
/// anything changed — a row already lit is not a redraw.
#[test]
fn the_pointer_moves_the_same_highlight() {
    let list = rows(4);
    let mut picker = Picker::default();
    picker.settle(&list, 0);

    assert!(picker.point_at(2, &list));
    assert_eq!(picker.lit(), Some(2));
    assert!(
        !picker.point_at(2, &list),
        "the row already lit is not a change"
    );

    // A row past the end — the list narrowed under a still pointer —
    // lands on the last row rather than past it.
    assert!(picker.point_at(9, &list));
    assert_eq!(picker.lit(), Some(3));

    picker.step(1, &list);
    assert_eq!(
        picker.lit(),
        Some(0),
        "the arrows move on from where the pointer left it"
    );
}

/// A prefilled query is selected whole: the first letter replaces it.
#[test]
fn a_prefilled_query_is_replaced_by_the_first_letter() {
    let mut picker = Picker::default();
    picker.prefill("history".to_owned());
    assert_eq!(picker.query(), "history");

    picker.edit(typed("r"));
    assert_eq!(picker.query(), "r");
    picker.edit(typed("e"));
    assert_eq!(picker.query(), "re", "only the first letter replaces");
}

/// Walking the rows is not editing: the prefill stays, and stays
/// selected.
#[test]
fn an_arrow_keeps_a_prefilled_query() {
    let list = rows(3);
    let mut picker = Picker::default();
    picker.prefill("history".to_owned());
    picker.settle(&list, 0);
    picker.step(1, &list);
    picker.jump(Jump::Last, &list);
    assert_eq!(picker.query(), "history");

    picker.edit(typed("x"));
    assert_eq!(picker.query(), "x", "still selected after the arrows");
}

/// Every rub takes a selected query whole, whichever rub it is.
#[test]
fn backspace_takes_a_prefill_whole() {
    for rub in [Edit::Rub, Edit::RubWord, Edit::RubLine] {
        let mut picker = Picker::default();
        picker.prefill("src/root".to_owned());
        picker.edit(rub.clone());
        assert_eq!(picker.query(), "", "{rub:?}");
        picker.edit(typed("ab"));
        picker.edit(Edit::Rub);
        assert_eq!(
            picker.query(),
            "a",
            "and after it, a rub is one letter again"
        );
    }
}

fn stroke(text: &str) -> Keystroke {
    Keystroke::parse(text).expect("a stroke parses")
}

/// One stroke means one thing in every popup: a popup that read `tab` as
/// a letter while the next one switched tabs would be two popups a
/// person alternates between disagreeing.
#[test]
fn keys_mean_the_same_in_every_popup() {
    assert_eq!(key(&stroke("escape")), Key::Close);
    assert_eq!(key(&stroke("enter")), Key::Take(Take::Enter));
    assert_eq!(key(&stroke("cmd-enter")), Key::Take(Take::Aside));
    // `cmd-enter` exactly: Find in Files' *Open in Find Window* is that
    // stroke, and a `cmd-shift-enter` or `cmd-alt-enter` is a shortcut on
    // its way somewhere else, not a second spelling of it.
    assert_eq!(key(&stroke("cmd-shift-enter")), Key::Other);
    assert_eq!(key(&stroke("cmd-alt-enter")), Key::Other);
    assert_eq!(key(&stroke("down")), Key::Step(1));
    assert_eq!(key(&stroke("up")), Key::Step(-1));
    assert_eq!(key(&stroke("pagedown")), Key::Page(1));
    assert_eq!(key(&stroke("pageup")), Key::Page(-1));
    assert_eq!(key(&stroke("home")), Key::Jump(Jump::First));
    assert_eq!(key(&stroke("end")), Key::Jump(Jump::Last));
    assert_eq!(key(&stroke("tab")), Key::NextTab);
    assert_eq!(key(&stroke("shift-tab")), Key::PreviousTab);
    assert_eq!(key(&stroke("alt-up")), Key::History(-1));
    assert_eq!(key(&stroke("alt-down")), Key::History(1));

    // Then the line's own rules, which are `desktop::edit`'s.
    let mut letter = stroke("a");
    letter.key_char = Some("a".to_owned());
    assert_eq!(key(&letter), Key::Edit(Edit::Insert("a".to_owned())));
    assert_eq!(key(&stroke("backspace")), Key::Edit(Edit::Rub));
    assert_eq!(key(&stroke("alt-backspace")), Key::Edit(Edit::RubWord));
    assert_eq!(key(&stroke("cmd-v")), Key::Edit(Edit::Paste));

    // And a shortcut on its way elsewhere is nobody's here.
    assert_eq!(key(&stroke("cmd-q")), Key::Other);
    assert_eq!(key(&stroke("cmd-down")), Key::Other);
}

/// A page is the rows in view less one, so the row at the bottom of the
/// old view is at the top of the new one — and five before anything has
/// been measured.
#[test]
fn a_page_is_the_rows_in_view_less_one() {
    assert_eq!(page(280.0, 28.0), 9);
    assert_eq!(page(290.0, 28.0), 9, "a row cut by the edge is not in view");
    assert_eq!(page(40.0, 28.0), 1, "at least one");
    assert_eq!(page(0.0, 28.0), 5, "before the first frame");
    assert_eq!(page(280.0, 0.0), 5);
}
