//! What a panel's ⋮ offers, and what it deliberately does not.

use guild_desktop::actions::keymap::Named;
use guild_desktop::dock_menu::{Item, fitting, head_tools, items, items_with};
use guild_desktop::view::menu::{Entry, step};
use guild_session::Dock;

/// The two other docks, then a rule, then *Hide* — and never the dock the
/// panel is already in, which would be an item that does nothing.
#[test]
fn the_menu_offers_the_other_docks_and_then_hiding() {
    assert_eq!(
        items(Dock::Bottom),
        vec![
            Item::MoveHeading,
            Item::MoveTo(Dock::Left),
            Item::MoveTo(Dock::Right),
            Item::Separator,
            Item::Hide,
        ]
    );

    for dock in Dock::ALL {
        let rows = items(*dock);
        assert!(
            !rows.contains(&Item::MoveTo(*dock)),
            "{dock:?} is where it already is"
        );
        assert_eq!(rows.len(), 5, "a heading, two docks, a rule and Hide");
        assert_eq!(rows.last(), Some(&Item::Hide), "hiding is the last word");
    }
}

/// The order is the status bar's, so the two lists cannot disagree about
/// which dock is which.
#[test]
fn the_docks_keep_the_order_the_status_bar_gives_them() {
    let moves: Vec<Dock> = items(Dock::Left)
        .into_iter()
        .filter_map(|item| match item {
            Item::MoveTo(dock) => Some(dock),
            _ => None,
        })
        .collect();
    assert_eq!(moves, vec![Dock::Bottom, Dock::Right]);
}

/// **How many of a toolbar's buttons fit, and what that leaves for a ⋮** —
/// M12.1 task 3. All of them when there is room for all; otherwise as many
/// as fit beside the ⋮ the rest fold into; none when not even one does,
/// and then the ⋮ holds them all. The ⋮ is never what gives way.
#[test]
fn a_toolbar_shows_what_fits_beside_the_fold() {
    // six buttons of 24 with gaps of 4 are 164; the fold costs a gap and 22
    let (button, gap, fold) = (24.0, 4.0, 4.0 + 22.0);
    assert_eq!(fitting(6, button, gap, fold, 500.0), 6);
    assert_eq!(fitting(6, button, gap, fold, 164.0), 6, "exactly enough");
    assert_eq!(
        fitting(6, button, gap, fold, 163.0),
        5,
        "five are 136, and the fold 26 more"
    );
    assert_eq!(fitting(6, button, gap, fold, 161.0), 4);
    assert_eq!(
        fitting(6, button, gap, fold, 50.0),
        1,
        "one button and the fold, exactly"
    );
    assert_eq!(fitting(6, button, gap, fold, 49.0), 0, "only the fold");
    assert_eq!(fitting(6, button, gap, fold, 0.0), 0);
    assert_eq!(fitting(0, button, gap, fold, 0.0), 0, "nothing to show");
    for room in 0..200 {
        let shown = fitting(6, button, gap, fold, room as f32);
        let buttons = shown as f32 * button + shown.saturating_sub(1) as f32 * gap;
        let used = if shown == 6 { buttons } else { buttons + fold };
        assert!(
            shown == 0 || used <= room as f32,
            "{shown} shown in {room}: {used}"
        );
    }
}

/// **The head's name keeps its width before the toolbar takes the rest** —
/// the Commit head at 230 was six buttons and *COM*. What the buttons
/// have is the head less its padding, the bar, the gaps and the name, so
/// at 230 with *COMMIT* (about 48 at the default scale) four buttons stand
/// beside the ⋮ and Collapse All and Expand All fold into it; with no name
/// to keep all six fit, which was the old budget. At the dock's minimum,
/// 160, one button and the ⋮. Whatever the width, the name, the buttons
/// shown, the ⋮ when it is there and the head's own chrome never exceed
/// the head — that is the name never being cut while a button could go.
#[test]
fn a_head_keeps_its_name_before_its_toolbar() {
    let rem = 16.0;
    let commit = 48.0;
    assert_eq!(head_tools(6, 230.0, rem, 0.0), 6, "no name: the old budget");
    assert_eq!(head_tools(6, 230.0, rem, commit), 4);
    assert_eq!(head_tools(6, 160.0, rem, commit), 1);
    assert_eq!(head_tools(6, 400.0, rem, commit), 6);
    // A larger interface is a larger name and larger buttons alike.
    assert!(head_tools(6, 230.0, 20.0, commit * 1.25) < 4);
    // padding 13 and 8, the bar 24 and two gaps of 2; a button 24, the ⋮ 2 + 24
    let chrome = 13.0 + 8.0 + 24.0 + 2.0 * 2.0;
    for width in 160..=600 {
        let width = width as f32;
        let shown = head_tools(6, width, rem, commit);
        let buttons = shown as f32 * 24.0 + shown.saturating_sub(1) as f32 * 2.0;
        let fold = if shown == 6 { 0.0 } else { 2.0 + 24.0 };
        assert!(
            commit + buttons + fold + chrome <= width,
            "{shown} shown at {width}"
        );
    }
}

/// **What folded is first in the ⋮, then the dock's own items** — the
/// buttons are what the person reached for, and where the panel may go is
/// what it always offers.
#[test]
fn a_fold_puts_the_folded_buttons_before_the_dock_s_own_items() {
    assert_eq!(items_with(Dock::Left, &[]), items(Dock::Left));
    let folded = items_with(Dock::Left, &[Named::CollapseChanges, Named::ExpandChanges]);
    assert_eq!(folded[0], Item::Action(Named::CollapseChanges));
    assert_eq!(folded[1], Item::Action(Named::ExpandChanges));
    assert_eq!(folded[2], Item::Separator);
    assert_eq!(&folded[3..], items(Dock::Left).as_slice());
}

/// **The card is the engine's now** — M12.7 task 3: the model above turns
/// into the engine's own `Entry`s, a heading that is not a place, `Move
/// to` over the other docks, a rule, then `Hide`.
#[test]
fn the_menu_is_the_engine_s_heading_items_rule_and_hide() {
    let list = guild_desktop::view::menu::dock::dock_head(Dock::Bottom, &[]);
    assert!(matches!(list[0], Entry::Heading(_)));
    assert!(matches!(list[3], Entry::Separator));
    assert_eq!(
        step(&list, None, true),
        Some(1),
        "the heading is not a place"
    );
}
