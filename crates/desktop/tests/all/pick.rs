//! Moving a selection with the arrows: the one arithmetic both lists use.

use guild_desktop::pick::step;

/// An empty list has nothing to select, whichever way the arrow points.
#[test]
fn an_empty_list_has_nothing_to_select() {
    assert_eq!(step(0, None, true), None);
    assert_eq!(step(0, None, false), None);
    assert_eq!(step(0, Some(3), true), None, "even from a stale index");
}

/// From nothing, either arrow lands on the first row: the list is read
/// from the top, and this is what the Commit panel has done since M8.3.
#[test]
fn from_nothing_both_arrows_land_on_the_first_row() {
    assert_eq!(step(5, None, true), Some(0));
    assert_eq!(step(5, None, false), Some(0));
    assert_eq!(step(1, None, true), Some(0));
}

/// One step at a time, and the ends hold: a list that wrapped round
/// would take a person to the far end of a log they were reading.
#[test]
fn a_step_is_one_row_and_the_ends_hold() {
    assert_eq!(step(5, Some(0), true), Some(1));
    assert_eq!(step(5, Some(3), true), Some(4));
    assert_eq!(step(5, Some(4), true), Some(4), "the last row holds");

    assert_eq!(step(5, Some(4), false), Some(3));
    assert_eq!(step(5, Some(1), false), Some(0));
    assert_eq!(step(5, Some(0), false), Some(0), "the first row holds");
}

/// A single row is both ends at once.
#[test]
fn one_row_is_both_ends() {
    assert_eq!(step(1, Some(0), true), Some(0));
    assert_eq!(step(1, Some(0), false), Some(0));
}

/// An index past the end — a list that came back shorter under the
/// selection — is read as the last row, so that what this answers can
/// always be used to index the list.
#[test]
fn an_index_past_the_end_comes_back_into_the_list() {
    assert_eq!(step(3, Some(9), true), Some(2));
    assert_eq!(step(3, Some(9), false), Some(1));
}

/// **A list with rows always has one of them lit.**
///
/// The branch popup kept `None` after every keystroke, and `enter` then
/// did nothing at all: a person typed a new branch name, saw *Create
/// branch …* offered, pressed the obvious key and the popup sat there.
/// Its neighbour, the Recent Projects list, has always lit a row — this
/// is that rule, written once so the two cannot drift apart.
#[test]
fn a_list_that_has_rows_has_one_of_them_lit() {
    use guild_desktop::pick::settle;

    assert_eq!(settle(0, None), None, "nothing to light");
    assert_eq!(settle(0, Some(3)), None, "and nothing to keep lit");
    assert_eq!(settle(5, None), Some(0), "the top, where the eye is");
    assert_eq!(settle(5, Some(2)), Some(2), "a choice already made stands");
    assert_eq!(
        settle(2, Some(7)),
        Some(1),
        "a narrowed list keeps the highlight inside itself"
    );
}
