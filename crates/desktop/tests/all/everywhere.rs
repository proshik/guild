//! Search Everywhere's tabs, its strokes and its sections — M8.9 group
//! A's tasks 6 and 8, decisions 6 and 15.

use guild_desktop::actions::keymap::Named;
use guild_desktop::actions::labels::label;
use guild_desktop::everywhere::{
    Among, Asked, Empty, SECTION, Slot, Wanted, again, all, empty, first_size, rank_actions,
    split_line, wanted,
};
use guild_desktop::i18n::{Language, Ui};

/// **Tab and Shift-Tab walk the five tabs and wrap**, in order: *All*,
/// *Files*, *Symbols*, *Actions*, *Text*.
#[test]
fn tab_and_shift_tab_walk_the_five_tabs_and_wrap() {
    let order = [
        Among::All,
        Among::Files,
        Among::Symbols,
        Among::Actions,
        Among::Text,
    ];
    for (index, among) in order.iter().enumerate() {
        let next = order[(index + 1) % order.len()];
        let previous = order[(index + order.len() - 1) % order.len()];
        assert_eq!(among.next(), next, "after {among:?}");
        assert_eq!(among.previous(), previous, "before {among:?}");
    }
    assert_eq!(
        Among::Text.next(),
        Among::All,
        "the last wraps to the first"
    );
    assert_eq!(Among::All.previous(), Among::Text, "and back");
}

/// **A tab's own stroke pressed again turns on *Include non-project items***
/// — the familiar gesture, and the only way to it from the keyboard. A double
/// Shift on *All* is *All*'s own stroke.
#[test]
fn a_tab_s_own_stroke_pressed_again_turns_on_non_project_items() {
    for among in [Among::All, Among::Files, Among::Symbols, Among::Actions] {
        assert_eq!(again(among, among), Asked::Libraries, "{among:?} again");
    }
    assert_eq!(Among::of(Named::GoToFile), Some(Among::Files));
    assert_eq!(Among::of(Named::GoToSymbol), Some(Among::Symbols));
    assert_eq!(Among::of(Named::FindAction), Some(Among::Actions));
    assert_eq!(Among::of(Named::SearchEverywhere), Some(Among::All));
    assert_eq!(
        Among::of(Named::CloseTab),
        None,
        "a stroke that is not a tab's"
    );
}

/// **Another tab's stroke switches to it**, and a double Shift over any tab
/// but *All* goes to *All*.
#[test]
fn another_tab_s_stroke_switches_to_it() {
    assert_eq!(
        again(Among::Files, Among::Symbols),
        Asked::Open(Among::Symbols)
    );
    assert_eq!(
        again(Among::Actions, Among::Files),
        Asked::Open(Among::Files)
    );
    assert_eq!(
        again(Among::Text, Among::Actions),
        Asked::Open(Among::Actions)
    );
    for among in [Among::Files, Among::Symbols, Among::Actions, Among::Text] {
        assert_eq!(
            again(among, Among::All),
            Asked::ToAll,
            "a double Shift over {among:?}"
        );
    }
}

/// **Find Action matches the label and the keymap name**, because
/// Settings → Keymap lists the second: `togsid` finds `ToggleSidebar` by
/// its name, and a word of its Russian label finds it in Russian.
///
/// **The name is asked of the Russian list**: its translated label cannot
/// match the Latin keymap query. The action remains ToggleSidebar after
/// the visible label becomes Toggle Workspaces.
#[test]
fn find_action_matches_the_label_and_the_keymap_name() {
    let named = |language| -> Vec<(Named, String)> {
        Named::ALL
            .iter()
            .map(|action| (*action, label(*action, Ui(language))))
            .collect()
    };

    let english = named(Language::English);
    let russian = named(Language::Russian);
    let found = rank_actions("togsid", &russian);
    let (_, marks) = found
        .iter()
        .find(|(action, _)| *action == Named::ToggleSidebar)
        .unwrap_or_else(|| panic!("togsid by the keymap name: {found:?}"));
    assert!(
        marks.is_empty(),
        "found by the name alone, the label has nothing to mark: {marks:?}"
    );

    let found = rank_actions("пространства", &russian);
    let (_, marks) = found
        .iter()
        .find(|(action, _)| *action == Named::ToggleSidebar)
        .unwrap_or_else(|| panic!("пространства by the Russian label: {found:?}"));
    assert_eq!(
        marks.len(),
        "пространства".chars().count(),
        "marked in the label it matched"
    );

    let everything = rank_actions("", &english);
    assert_eq!(
        everything
            .iter()
            .map(|(action, _)| *action)
            .collect::<Vec<_>>(),
        Named::ALL.to_vec(),
        "nothing typed keeps the list's own order"
    );
}

/// **All is Files, then Symbols, then Actions**, each under its heading and
/// each at most `SECTION` rows, with a *More* row under a section that
/// holds more than it shows.
#[test]
fn all_is_files_then_symbols_then_actions_each_capped_with_a_more_row() {
    let slots = all(SECTION + 3, 2, SECTION);
    let mut expected = vec![Slot::Heading(Among::Files)];
    expected.extend((0..SECTION).map(|row| Slot::Row(Among::Files, row)));
    expected.push(Slot::More(Among::Files));
    expected.push(Slot::Heading(Among::Symbols));
    expected.extend((0..2).map(|row| Slot::Row(Among::Symbols, row)));
    expected.push(Slot::Heading(Among::Actions));
    expected.extend((0..SECTION).map(|row| Slot::Row(Among::Actions, row)));
    assert_eq!(
        slots, expected,
        "a section of exactly SECTION has no More row"
    );
}

/// A section with no rows has no heading: a title over nothing reads as a
/// section that failed to load.
#[test]
fn an_empty_section_has_no_heading() {
    assert_eq!(
        all(0, 1, 0),
        vec![Slot::Heading(Among::Symbols), Slot::Row(Among::Symbols, 0)]
    );
    assert!(all(0, 0, 0).is_empty());
}

/// **A file and a place are split where the numbers begin** — at the first
/// `:` after which `parse_target` reads the rest, so a colon that is part
/// of the name stays in it.
#[test]
fn a_file_and_a_place_are_split_where_the_numbers_begin() {
    let (name, place) = split_line("history.rs:120:4");
    assert_eq!(name, "history.rs");
    let place = place.expect("a line and a column");
    assert_eq!((place.line, place.column), (Some(120), Some(4)));

    let (name, place) = split_line("history.rs:120");
    assert_eq!(name, "history.rs");
    let place = place.expect("a line alone");
    assert_eq!((place.line, place.column), (Some(120), None));

    assert_eq!(
        split_line("a:b.rs"),
        ("a:b.rs", None),
        "no numbers: the whole text"
    );
    let (name, place) = split_line("a:b.rs:12");
    assert_eq!(name, "a:b.rs", "the colon in the name stays in it");
    assert_eq!(place.and_then(|place| place.line), Some(12));
    assert_eq!(split_line("history.rs"), ("history.rs", None));
}

/// **Symbols and Text ask nothing until something is typed** — a list of
/// every symbol, or a search for the empty string, is not what an empty
/// field means — and an empty *All* is the recent files alone (decision
/// 10), while *Files* is those and *Actions* is every action.
#[test]
fn symbols_and_text_ask_nothing_until_something_is_typed() {
    let nothing = Wanted::default();
    assert_eq!(wanted(Among::Symbols, ""), nothing);
    assert_eq!(
        wanted(Among::Symbols, "   "),
        nothing,
        "spaces are nothing typed"
    );
    assert_eq!(wanted(Among::Text, ""), nothing);
    assert_eq!(
        wanted(Among::Symbols, "gre"),
        Wanted {
            symbols: true,
            ..nothing
        }
    );
    assert_eq!(
        wanted(Among::Text, "needle"),
        Wanted {
            text: true,
            ..nothing
        }
    );
    assert_eq!(
        wanted(Among::Files, ""),
        Wanted {
            files: true,
            ..nothing
        },
        "the recent files"
    );
    assert_eq!(
        wanted(Among::All, ""),
        Wanted {
            files: true,
            ..nothing
        },
        "the recent files, and nothing else"
    );
    assert_eq!(
        wanted(Among::All, "ne"),
        Wanted {
            files: true,
            symbols: true,
            actions: true,
            text: false,
        }
    );
    assert_eq!(
        wanted(Among::Actions, ""),
        Wanted {
            actions: true,
            ..nothing
        }
    );
}

/// **The card first opens at section 6.33's size**: 680 wide, and its list
/// of 440 with the 80 of the head above it — at the default rem of 16, and
/// in proportion at a larger one. The head was once added in pixels to the
/// list in rems and the sum multiplied by the rem, which opened the card
/// 1720 high and let the window's edge decide.
#[test]
fn the_card_first_opens_at_section_6_33_s_size() {
    assert_eq!(first_size(16.0), (680.0, 520.0));
    assert_eq!(first_size(20.0), (850.0, 650.0));
}

/// **An empty list says which emptiness it is**, and *All* with something
/// typed reads both indexes: while either has not landed its first walk it
/// is *Indexing…*, not *Nothing matches* — which would be a claim about a
/// project nobody has finished reading.
#[test]
fn an_empty_list_says_which_emptiness_it_is() {
    let ready = |among, typed| empty(among, typed, true, true, false);
    assert_eq!(empty(Among::All, true, false, true, false), Empty::Indexing);
    assert_eq!(empty(Among::All, true, true, false, false), Empty::Indexing);
    assert_eq!(ready(Among::All, true), Empty::NothingMatches);
    assert_eq!(
        empty(Among::All, false, false, false, false),
        Empty::TypeToSearch,
        "an empty All is the recent files, and asks nothing more"
    );

    assert_eq!(
        empty(Among::Files, false, false, true, false),
        Empty::Indexing
    );
    assert_eq!(ready(Among::Files, true), Empty::NothingMatches);
    assert_eq!(
        empty(Among::Symbols, false, true, false, false),
        Empty::TypeToSearch
    );
    assert_eq!(
        empty(Among::Symbols, true, true, false, false),
        Empty::Indexing
    );
    assert_eq!(ready(Among::Text, false), Empty::TypeToSearch);
    assert_eq!(empty(Among::Text, true, true, true, true), Empty::Searching);
    assert_eq!(ready(Among::Text, true), Empty::NothingMatches);
    assert_eq!(ready(Among::Actions, true), Empty::NothingMatches);
}
