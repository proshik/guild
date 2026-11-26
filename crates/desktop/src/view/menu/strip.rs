//! The tab strip's three menus — M12.7: the add button's chevron, the
//! chevron over the tabs that cannot be seen, and the `⋮`.
//!
//! Until M12.7 all three were `gpui-component`'s menu, which fixes
//! its rows at 26 and lights a row under the pointer by itself — so the
//! harness menu, which drew a pill of its own inside each row, lit a row
//! twice. They are lists of this engine now, and the card, the rows and
//! the one highlight are the right button's.
//!
//! **Two of them carry data**, the harnesses on the `PATH` and the tabs
//! out of sight, so they are builders the window hands its facts to rather
//! than `Subject`s, which stay `Copy`; the `⋮` is data-free and is one.

use guild_session::{DefaultTab, HarnessKind};

use super::{Choice, Deed, Entry, Label, Mark};
use crate::TabId;
use crate::actions::keymap::Named;
use crate::i18n::Text;

/// The add button's chevron: every kind of tab, a harness the `PATH` does
/// not have drawn and never taken.
///
/// **The terminal is always offered**: it needs nothing on the `PATH`
/// beyond the shell the window already runs. No row is current — the
/// default tab is the button's mark, not a choice here — so the menu has
/// no check column.
pub fn new_tab(available: &[HarnessKind]) -> Vec<Entry> {
    ordered_new_tab(available, &DefaultTab::ALL)
}

/// The click and chord menus share the persisted order and ordinal column.
pub fn ordered_new_tab(available: &[HarnessKind], order: &[DefaultTab]) -> Vec<Entry> {
    order
        .iter()
        .copied()
        .map(|tab| {
            Entry::Choice(Choice {
                label: Label::Text(crate::view::kind::default_tab_text(tab)),
                mark: Mark::Harness(tab),
                deed: Deed::NewTab(tab),
                check: None,
                enabled: harness_of(tab).is_none_or(|kind| available.contains(&kind)),
                detail: None,
                trailing: None,
            })
        })
        .collect()
}

/// The chevron over the tabs: each tab the strip cannot show, by its name
/// and its file's icon when a file stands behind it.
///
/// Deeds, not actions — a row brings *that* tab forward and no stroke
/// does that — so nothing here shows one.
pub(crate) fn hidden_tabs(tabs: &[(TabId, String, Option<String>)]) -> Vec<Entry> {
    tabs.iter()
        .map(|(id, name, icon)| {
            Entry::Choice(Choice {
                label: Label::Named(name.clone()),
                mark: icon.clone().map_or(Mark::None, Mark::File),
                deed: Deed::ShowTab(*id),
                check: None,
                enabled: true,
                detail: None,
                trailing: None,
            })
        })
        .collect()
}

/// The `⋮`: the strip's actions and the area's, aimed at the window, with
/// the rules between the groups.
///
/// **Actions rather than deeds, and the only menu of ours that is.** Every
/// other menu holds deeds aimed at the row under the pointer, because an
/// action reaches the thing in front — decision 20 of the spec. This one
/// hangs off the strip, not off a row, and *Recent Files*, *Reopen Closed
/// Tab*, *Close All Tabs* and *Close Other Tabs* mean the same
/// thing however they are reached. So each shows its own stroke honestly,
/// which is the whole reason for this button. Decision 6 of M8.8.
///
/// **The area's group stands last** — M12.8, decision 21: the area's two
/// splits and the tab's two, with their strokes, then *Close Area*, a
/// `Choice` because it may refuse. A press on another area's `⋮` gives
/// that area the keys first, so the action reaches the area the button
/// belongs to. With one area the row is drawn faint with *the last area
/// stays* in its right slot (decision 9), in place of the stroke it
/// otherwise shows. *Settings* is not here: it opens from the title bar's
/// gear and `cmd-,`.
pub fn tab_actions(areas: usize) -> Vec<Entry> {
    let row = |item: &Option<(Text, Named)>| match item {
        Some((text, named)) => Entry::Item(*text, Deed::Action(*named)),
        None => Entry::Separator,
    };
    let mut entries: Vec<Entry> = ACTIONS.iter().map(row).collect();
    entries.extend(AREA.iter().map(row));
    entries.push(Entry::Choice(Choice {
        label: Label::Text(Text::MenuCloseArea),
        mark: Mark::None,
        deed: Deed::Action(Named::CloseArea),
        check: None,
        enabled: areas > 1,
        detail: (areas == 1).then_some(Label::Text(Text::LastAreaStays)),
        trailing: None,
    }));
    entries
}

/// Which concrete harness a default tab stands for — `None` for the
/// terminal, which is its own row rather than a harness. The one mapping
/// lives in `start`, since M14.5's start screen needs it too.
pub(crate) fn harness_of(tab: DefaultTab) -> Option<HarnessKind> {
    crate::start::harness_of(tab)
}

/// What the `⋮` offers of the strip, in order, with the rule between the
/// groups.
///
/// *Recent Files* first, because that is what a hand reaches this
/// button for; the three that close
/// and reopen together.
const ACTIONS: &[Option<(Text, Named)>] = &[
    Some((Text::MenuRecentFiles, Named::RecentFiles)),
    None,
    Some((Text::MenuReopenTab, Named::ReopenTab)),
    Some((Text::MenuCloseAllTabs, Named::CloseAllTabs)),
    Some((Text::MenuCloseOtherTabs, Named::CloseOtherTabs)),
];

/// The area's actions with strokes, after a rule, and the tab's two —
/// *Split Tab Right* and *Split Tab Down* divide the focused pane of the
/// tab in front (M12.8, decision 21). *Close Area* follows them as a
/// `Choice` of `tab_actions`' own.
const AREA: &[Option<(Text, Named)>] = &[
    None,
    Some((Text::MenuSplitAreaRight, Named::SplitAreaRight)),
    Some((Text::MenuSplitAreaDown, Named::SplitAreaDown)),
    Some((Text::MenuSplitPaneRight, Named::SplitPaneRight)),
    Some((Text::MenuSplitPaneDown, Named::SplitPaneDown)),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every row of the `⋮` names an action that has a stroke, or the
    /// menu draws a label with an empty column beside it and the button's
    /// whole reason for existing — that its items are honest about the
    /// keys — is gone. Here because it reads the private table.
    #[test]
    fn every_row_of_the_dots_menu_has_a_stroke() {
        for (text, named) in ACTIONS.iter().chain(AREA).flatten() {
            assert!(
                crate::actions::stroke_for(*named).is_some(),
                "{text:?} is offered by the ⋮ with no stroke to show"
            );
        }
    }
}
