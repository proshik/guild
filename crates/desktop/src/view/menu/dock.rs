//! A panel head's `⋮` — M12.7: `dock_menu`'s pure model, drawn by the
//! right button's engine instead of the card `view::dock_menu` used to
//! roll by hand.
//!
//! **The model stays the model.** `dock_menu::items` and `items_with` are
//! still where *where may this go* and *what folded* are worked out —
//! the two controls, the docks in the status bar's order, the folded
//! toolbar first — and their own tests still read them without a window.
//! What is here is the turn from `dock_menu::Item` onto `Entry`, and the
//! two writes a row of it makes.

use guild_session::Dock;

use super::{Choice, Deed, Entry, Label, Mark, Target};
use crate::Root;
use crate::actions::keymap::Named;
use crate::dock_menu::{self, Item};
use crate::i18n::Text;
use crate::view::changes::COMMIT_TOOLS;

/// What the panel's own two items do — the tool it shows may move to
/// another dock, or the panel is put away, which the bar beside the `⋮`
/// also does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockDeed {
    /// Move the tool this dock shows to another dock.
    MoveTo(Dock),
    /// Put the panel away.
    Hide,
}

/// The `⋮`'s items for `dock`, `folded`'s buttons first — the engine's
/// rows over `dock_menu::items_with`'s model.
///
/// **A folded button keeps its icon**, which the old card never drew —
/// `dock_menu.rs`'s own rows were plain text throughout. `COMMIT_TOOLS` is
/// the one table both the toolbar and this read, so a button folded into
/// the `⋮` is still the button it was.
pub fn dock_head(dock: Dock, folded: &[Named]) -> Vec<Entry> {
    dock_menu::items_with(dock, folded)
        .into_iter()
        .map(|item| match item {
            Item::MoveHeading => Entry::Heading(Label::Text(Text::MoveTo)),
            Item::MoveTo(to) => Entry::Item(dock_text(to), Deed::Dock(DockDeed::MoveTo(to))),
            Item::Separator => Entry::Separator,
            Item::Hide => Entry::Item(Text::PanelHide, Deed::Dock(DockDeed::Hide)),
            Item::Action(action) => {
                let tool = COMMIT_TOOLS.iter().find(|(_, _, named)| *named == action);
                Entry::Choice(Choice {
                    label: Label::Text(tool.map_or(Text::Refresh, |(_, label, _)| *label)),
                    mark: tool.map_or(Mark::None, |(icon, _, _)| Mark::Icon(icon)),
                    deed: Deed::Action(action),
                    check: None,
                    enabled: true,
                    detail: None,
                    trailing: None,
                })
            }
        })
        .collect()
}

/// The label for moving to a dock, in the status bar's own words.
fn dock_text(dock: Dock) -> Text {
    match dock {
        Dock::Left => Text::DockLeft,
        Dock::Right => Text::DockRight,
        Dock::Bottom => Text::DockBottom,
    }
}

impl Root {
    /// The two things a dock head's `⋮` may do.
    ///
    /// A folded toolbar button is `Deed::Action`, the `⋮`'s own kind of
    /// item since task 2: `run_deed` dispatches it through
    /// `window.dispatch_action`, which reaches the same `on_action`
    /// handler `run_commit_tool` called directly — `bind_git` binds all
    /// six at the root, so wherever the menu was opened from is beside
    /// the point.
    pub(crate) fn run_dock_deed(
        &mut self,
        deed: DockDeed,
        target: &Target,
        cx: &mut gpui::Context<Self>,
    ) {
        let Target::DockHead { dock, .. } = target else {
            return;
        };
        match deed {
            DockDeed::MoveTo(to) => self.move_shown_tool(*dock, to, cx),
            DockDeed::Hide => self.hide_dock(*dock, cx),
        }
    }
}
