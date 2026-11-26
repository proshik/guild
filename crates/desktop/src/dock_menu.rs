//! What a panel's ⋮ offers.
//!
//! A tool window has two controls at its right edge — a ⋮ that opens
//! the window's own menu and a bar that hides it — and this is the menu.
//! Pure, so the shape can be read without a window: what a menu offers is
//! a decision, and a decision is worth a test.
//!
//! **It offers what Guild has, nothing more.** A *View Mode* that floats,
//! undocks and windows a tool does not exist here; ours are docked and
//! nothing else, and a menu naming modes that do not exist is a menu that
//! lies. What is here is where the tool may go and how to put the panel away.

use guild_session::Dock;

use crate::actions::keymap::Named;

/// One line of the menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    /// *Move to*, over the docks under it.
    MoveHeading,
    /// One dock the tool may be moved to.
    MoveTo(Dock),
    /// A hairline.
    Separator,
    /// Put the panel away, which is what the bar beside the ⋮ does.
    Hide,
    /// A button of the tool's own toolbar that did not fit beside the ⋮ —
    /// M12.1.
    Action(Named),
}

/// The menu for the dock a panel is in.
///
/// The dock it is already in is not somewhere to move it: an item that
/// does nothing is an item a person tries once and distrusts after.
pub fn items(current: Dock) -> Vec<Item> {
    let mut rows = vec![Item::MoveHeading];
    rows.extend(
        Dock::ALL
            .iter()
            .filter(|dock| **dock != current)
            .map(|dock| Item::MoveTo(*dock)),
    );
    rows.push(Item::Separator);
    rows.push(Item::Hide);
    rows
}

/// The menu for a dock whose tool folded some of its toolbar into the ⋮ —
/// M12.1 task 3.
///
/// **The folded buttons first**, in the toolbar's own order: they are what
/// the person reached for, and where the panel may go is what the menu
/// always offers. Nothing folded is the plain menu.
pub fn items_with(current: Dock, folded: &[Named]) -> Vec<Item> {
    if folded.is_empty() {
        return items(current);
    }
    let mut rows: Vec<Item> = folded.iter().map(|named| Item::Action(*named)).collect();
    rows.push(Item::Separator);
    rows.extend(items(current));
    rows
}

/// How many of `count` buttons a toolbar shows in `room` — M12.1 task 3.
///
/// **All of them when they fit.** Otherwise as many as fit with `fold` —
/// the ⋮ the rest go into, and the gap before it — beside them, and none
/// when not even one does. The ⋮ is never what gives way: a toolbar that
/// folded its way out of the menu it folds into would be a state reached
/// and not left.
pub fn fitting(count: usize, button: f32, gap: f32, fold: f32, room: f32) -> usize {
    let width = |shown: usize| shown as f32 * button + shown.saturating_sub(1) as f32 * gap;
    if width(count) <= room {
        return count;
    }
    (1..count)
        .rev()
        .find(|shown| width(*shown) + fold <= room)
        .unwrap_or(0)
}

/// How many of `count` buttons a dock head's toolbar shows, in a head
/// `width` wide whose name is `title` wide, at this window's `rem`.
///
/// **The name keeps its width; the toolbar has what is left** — the
/// head's padding, the bar that puts the panel away and the two gaps
/// between the naming, the toolbar and the bar are the design system's
/// rems. M12 task 13 let the name give way entirely, and at 230 the Commit
/// head read *COM* beside six buttons; now the buttons that do not fit go
/// into the ⋮, whose own place `fitting` keeps.
pub fn head_tools(count: usize, width: f32, rem: f32, title: f32) -> usize {
    use crate::theme::space;
    let measure = |rems: gpui::Rems| rems.0 * rem;
    let gap = measure(space::STATUS_BUTTON_GAP);
    let bar = measure(space::STATUS_BUTTON);
    let room =
        width - measure(space::CARD_PADDING) - measure(space::CARD_GAP) - bar - 2.0 * gap - title;
    fitting(count, measure(space::HEAD_BUTTON), gap, gap + bar, room)
}
