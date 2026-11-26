//! Which tabs the strip cannot show.
//!
//! The chevron at the end of the strip lists what is out of sight, and
//! this is the comparison that decides it — task 0 of M8.8 established
//! that `ScrollHandle` can answer the question at all, so the answer is
//! not *list everything and let the person look*.
//!
//! Out here rather than beside the code because it is the whole of what
//! a caller can reach: a window is needed to draw the menu and nothing is
//! needed to decide what goes in it.

use guild_desktop::view::tab_list::hidden;

/// The chevron is drawn exactly when the answer is not empty, so the
/// two questions — *is anything hidden* and *what* — are one function.
#[test]
fn a_strip_that_fits_hides_nothing_and_a_full_one_names_what_it_cannot_show() {
    assert!(
        hidden((0.0, 400.0), &[]).is_empty(),
        "a project with no tabs has nothing to list"
    );
    assert!(
        hidden((0.0, 400.0), &[(0.0, 120.0), (120.0, 260.0)]).is_empty(),
        "two tabs inside the box are two tabs a person can see"
    );

    assert_eq!(
        hidden(
            (0.0, 400.0),
            &[(0.0, 120.0), (120.0, 260.0), (260.0, 420.0), (420.0, 560.0)]
        ),
        vec![2, 3],
        "the third is clipped by the right edge and the fourth is past it"
    );
}

/// Scrolled along, the tabs that ran off the **left** edge are hidden
/// too. The strip's own scrolling was request 1 and landed before this
/// branch; a chevron that only ever looked right would be useless the
/// moment it was used.
#[test]
fn the_tabs_scrolled_off_the_left_are_hidden_too() {
    assert_eq!(
        hidden(
            (0.0, 400.0),
            &[(-260.0, -140.0), (-140.0, 40.0), (40.0, 300.0)]
        ),
        vec![0, 1],
        "the second straddles the left edge, which is half a name"
    );
}

/// A tab is hidden when any part of it is outside, not only when all of
/// it is. Half a name is not a tab a hand can pick out, and the tab
/// under the chevron itself is exactly the one that would be left
/// unreachable by the other rule.
#[test]
fn a_tab_clipped_by_an_edge_counts_as_hidden() {
    assert_eq!(
        hidden((0.0, 400.0), &[(0.0, 200.0), (200.0, 460.0)]),
        vec![1]
    );
}

/// Layout is floating point and the comparison is against the box's own
/// edges: without a tolerance the last tab that fits exactly could land
/// in the list for ever, and the chevron beside it would never go away.
#[test]
fn a_tab_that_ends_a_hair_past_the_edge_still_fits() {
    assert!(hidden((0.0, 400.0), &[(0.0, 400.2)]).is_empty());
    assert!(hidden((0.0, 400.0), &[(-0.2, 400.0)]).is_empty());
    assert_eq!(hidden((0.0, 400.0), &[(0.0, 401.0)]), vec![0]);
}

use guild_desktop::actions::keymap::Named;
use guild_desktop::i18n::Text;
use guild_desktop::view::menu::{Deed, Entry, Label, Subject, entries, step, strip};
use guild_session::{DefaultTab, HarnessKind};

/// The add button's chevron offers every kind of tab, and a harness the
/// `PATH` does not have is drawn and passed over — M12.7. The terminal
/// needs nothing on the `PATH`, so it is always a place to stand.
#[test]
fn a_harness_that_is_not_on_the_path_is_offered_greyed_and_cannot_be_stood_on() {
    let list = strip::new_tab(&[HarnessKind::Claude]);
    let rows: Vec<(DefaultTab, bool)> = list
        .iter()
        .filter_map(|e| match e {
            Entry::Choice(c) => match c.deed {
                Deed::NewTab(t) => Some((t, c.enabled)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        rows,
        DefaultTab::ALL
            .iter()
            .map(|t| (*t, matches!(t, DefaultTab::Claude | DefaultTab::Terminal)))
            .collect::<Vec<_>>()
    );
    // Codex and OpenCode are skipped by the keyboard.
    assert_eq!(step(&list, Some(0), true), Some(3));
}

/// The `⋮` holds the strip's actions in the common order, then the area's —
/// actions, because this menu hangs off the strip and not off a row.
#[test]
fn the_dots_menu_holds_its_actions_between_two_rules() {
    let list = entries(Subject::TabActions { areas: 2 });
    let deeds: Vec<Deed> = list
        .iter()
        .filter_map(|e| match e {
            Entry::Item(_, d) => Some(*d),
            Entry::Choice(c) => Some(c.deed),
            _ => None,
        })
        .collect();
    assert_eq!(
        deeds,
        [
            Named::RecentFiles,
            Named::ReopenTab,
            Named::CloseAllTabs,
            Named::CloseOtherTabs,
            Named::SplitAreaRight,
            Named::SplitAreaDown,
            Named::SplitPaneRight,
            Named::SplitPaneDown,
            Named::CloseArea,
        ]
        .map(Deed::Action)
    );
    let rules: Vec<usize> = list
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e, Entry::Separator))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(rules, [1, 5], "Recent Files apart, the areas apart");
}

/// Decisions 9 and 21: the last area's *Close Area* is drawn, not taken,
/// with its reason in the row's right slot, where the mockup puts it.
#[test]
fn the_last_area_is_offered_closed_with_its_reason() {
    let close = |list: Vec<Entry>| {
        list.into_iter()
            .find_map(|e| match e {
                Entry::Choice(c) if c.deed == Deed::Action(Named::CloseArea) => Some(c),
                _ => None,
            })
            .expect("the row is there")
    };
    let one = close(entries(Subject::TabActions { areas: 1 }));
    assert!(!one.enabled);
    assert_eq!(one.detail, Some(Label::Text(Text::LastAreaStays)));

    let two = close(entries(Subject::TabActions { areas: 2 }));
    assert!(two.enabled);
    assert_eq!(two.detail, None);
}

/// Every action the `⋮` offers shows its own stroke: the reason the menu
/// holds actions at all is that its strokes are honest.
#[test]
fn every_row_of_the_dots_menu_shows_its_own_stroke() {
    for entry in entries(Subject::TabActions { areas: 2 }) {
        if let Entry::Item(_, deed @ Deed::Action(named)) = entry {
            assert_eq!(
                guild_desktop::view::menu::stroke_for(deed, false),
                Some(named)
            );
        }
    }
}

#[test]
fn launcher_rows_share_configured_order_numbers_and_availability() {
    let order = [
        DefaultTab::Terminal,
        DefaultTab::Codex,
        DefaultTab::Claude,
        DefaultTab::OpenCode,
    ];
    let rows = strip::ordered_new_tab(&[HarnessKind::Claude], &order);
    for (index, entry) in rows.iter().enumerate() {
        let Entry::Choice(choice) = entry else {
            panic!("expected launcher row")
        };
        assert!(matches!(choice.deed, Deed::NewTab(tab) if tab == order[index]));
        assert!(
            choice.detail.is_none(),
            "ordinals must not occupy the trailing detail slot"
        );
        assert_eq!(
            choice.enabled,
            matches!(order[index], DefaultTab::Terminal | DefaultTab::Claude)
        );
    }
}
