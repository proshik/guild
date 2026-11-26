//! A menu's card, its rows and its separators — M12.2 task 8.
//!
//! **Measured rather than remembered**: section 5 and 6.17 of the design
//! system carry the numbers and `theme.rs` the tokens. One module for every
//! menu of Guild's own — the right button's, a panel head's ⋮, the Log's
//! filters — so a menu cannot look like one in one place and like another in
//! the next.

use gpui::prelude::*;
use gpui::{SharedString, div, px, rgb};

use crate::theme::{self, Theme};

/// The look every card of ours shares: a menu's radius, its background
/// and border, and a shadow under it — M12.7 task 9, so a picker's card
/// is built on the same look as a menu's rather than a copy of its
/// tokens that can drift from it.
pub(crate) fn card_look(theme: Theme) -> gpui::Div {
    div()
        .rounded(theme::radius::ROW)
        .bg(rgb(theme.surface.card))
        .border_1()
        .border_color(theme::card_outline(theme))
        .shadow_lg()
}

/// The card a menu's rows sit on.
pub(crate) fn card(theme: Theme) -> gpui::Div {
    card_look(theme)
        .flex()
        .flex_col()
        .min_w(theme::space::MENU_MIN)
        .py(theme::space::MENU_PADDING)
        .text_size(theme::size::BODY)
        .text_color(rgb(theme.text.primary))
        // A press on the card is the card's: the layer that closes the
        // menu lies under it, and a hitbox does not hide what is behind it
        // by covering it.
        .occlude()
}

/// The inset every row keeps from its card, whether the card is a menu's
/// or a picker's — M12.7 task 9: the highlight is inset from the edge by
/// the card's own radius (`mx`), and the text sits a further `px` in, so
/// together it lands `MENU_INSET` from the card's edge.
pub(crate) fn row_inset() -> gpui::Rems {
    gpui::Rems(theme::space::MENU_INSET.0 - theme::radius::ROW.0)
}

/// A hairline between two groups of a menu.
pub(crate) fn separator(theme: Theme) -> gpui::AnyElement {
    separator_look(theme).into_any_element()
}

fn separator_look(theme: Theme) -> gpui::Div {
    div()
        .flex_none()
        .h(px(1.0))
        .mx(theme::radius::ROW)
        .my(theme::space::MENU_RULE)
        .bg(theme::card_outline(theme))
}

/// A group's name over its rows — M12.7: small faint type at the rows'
/// inset, the shape the agents list and the servers list head their
/// groups with.
pub(crate) fn heading(label: SharedString, theme: Theme) -> gpui::AnyElement {
    div()
        .debug_selector(|| "popup-heading".into())
        .flex()
        .flex_none()
        .items_center()
        .h(theme::space::MENU_HEADING)
        .px(theme::space::MENU_INSET)
        .text_size(theme::size::EYEBROW)
        .text_color(rgb(theme.text.faint))
        .child(div().min_w_0().truncate().child(label))
        .into_any_element()
}

/// A footnote under a rule at a menu's foot — M12.7: the Log's filter
/// menus' *Press ⌫ to clear the filter*, measure for measure.
///
/// **The rule is drawn only when something stands above it** — a residual
/// fix: an empty agents list is a single `Note`, *No agents open*, and it
/// was always given the rule that separates a footnote from the rows
/// above it — a line with nothing above it to separate from anything.
/// `preceded` is the caller's own answer, since only it knows whether the
/// note is the first thing in its card.
pub(crate) fn note(label: SharedString, theme: Theme, preceded: bool) -> gpui::AnyElement {
    let selector = format!("popup-note-{label}");
    div()
        .debug_selector(move || selector.clone())
        .flex_none()
        .h(theme::space::TREE_ROW)
        .flex()
        .items_center()
        .px(theme::space::CARD_PADDING)
        .mx(theme::space::TREE_INSET)
        .when(preceded, |line| {
            line.mt(px(2.0))
                .border_t_1()
                .border_color(rgb(theme.surface.divider))
        })
        .text_size(theme::size::EYEBROW)
        .text_color(rgb(theme.text.faint))
        // **A footnote says something about the menu; it never sizes
        // it** — seen on 2026-09-25, Open In's folder made the card as wide
        // as the screen. No width of its own, the whole row's at least,
        // and an ellipsis where the rows leave it too little.
        .child(div().w(px(0.0)).min_w_full().truncate().child(label))
        .into_any_element()
}

/// A toolbar icon as a row's mark: `text.secondary`, or `accent.rust` on a
/// row that loses something. What `row` drew for an icon until M12.7,
/// when the slot began to take any drawing.
pub(crate) fn icon(path: &'static str, loses: bool, theme: Theme) -> gpui::AnyElement {
    crate::icons::toolbar(path)
        .size(theme::space::MENU_ICON)
        .text_color(rgb(if loses {
            theme.accent.rust
        } else {
            theme.text.secondary
        }))
        .into_any_element()
}

/// The colour of a row's trailing, secondary words — an Update branch
/// source's `→ origin/main` or "local only": `text.faint` at rest and one
/// step up, `text.tertiary`, on the lit row, whose `control_hover` fill
/// all but swallows faint. The start screen's recent rows raise their
/// trailing time the same step on the same condition.
pub fn trailing_ink(theme: Theme, lit: bool) -> u32 {
    if lit {
        theme.text.tertiary
    } else {
        theme.text.faint
    }
}

/// The check column a row reserves: a box the size of a menu's icon,
/// holding the check glyph only when `check` is true — M12.7 final fix.
/// `row` and Recent Projects (task 9: the brass dot for an already-open
/// project became this same column) built the identical markup twice;
/// this is the one place it is drawn now, so the two cannot drift apart
/// the way `CAPTURES`' two tables once did.
pub(crate) fn check_column(check: bool, theme: Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .size(theme::space::MENU_ICON)
        .flex()
        .items_center()
        .justify_center()
        .when(check, |slot| {
            slot.child(
                crate::icons::toolbar("ui/check.svg")
                    .size(theme::space::MENU_ICON)
                    .text_color(rgb(theme.text.secondary)),
            )
        })
        .into_any_element()
}

/// What one row of a menu says.
pub(crate) struct Row {
    /// Its words.
    pub label: SharedString,
    /// What stands in its icon slot, when anything does — M12.7; it was a
    /// toolbar icon's path.
    pub mark: Option<gpui::AnyElement>,
    /// Whether the menu it is in carries icons, so the text of a row
    /// without one still starts where the others' does.
    pub icons: bool,
    /// Whether it is the current item: `Some(true)` draws the check,
    /// `Some(false)` keeps the check's column empty so the words line up,
    /// and `None` is a menu with no current item and no column. M12.7.
    pub check: Option<bool>,
    /// The stroke that does the same, printed.
    pub stroke: Option<String>,
    /// It loses something when taken, and says so in `accent.rust`.
    pub loses: bool,
    /// It opens a submenu, and shows a chevron.
    pub submenu: bool,
    /// The one row the pointer or the keyboard is on.
    pub lit: bool,
    /// It is drawn and cannot be taken: faint, no pointer, never lit.
    /// M12.7.
    pub disabled: bool,
    /// Whatever the caller hangs at the row's end, after the chevron.
    pub trailing: Option<gpui::AnyElement>,
}

/// One row of a menu, without its handlers: the caller adds what a move
/// over it and a click on it do.
pub(crate) fn row(
    id: impl Into<gpui::ElementId>,
    row: Row,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    row_with_number(id, row, None, theme)
}

/// A launcher row with an ordinal before its existing icon and label.
pub(crate) fn numbered_row(
    id: impl Into<gpui::ElementId>,
    row: Row,
    ordinal: usize,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    row_with_number(id, row, Some(ordinal), theme)
}

fn row_with_number(
    id: impl Into<gpui::ElementId>,
    row: Row,
    ordinal: Option<usize>,
    theme: Theme,
) -> gpui::Stateful<gpui::Div> {
    let icon_selector = format!("popup-icon-{}", row.label);
    let label_selector = format!("popup-label-{}", row.label);
    let colour = if row.disabled {
        theme.text.faint
    } else if row.loses {
        theme.accent.rust
    } else {
        theme.text.primary
    };
    div()
        .id(id)
        .flex()
        .flex_none()
        .items_center()
        .gap(theme::space::MENU_ICON_GAP)
        .h(theme::space::MENU_ROW)
        .mx(theme::radius::ROW)
        .px(row_inset())
        .rounded(theme::radius::ROW)
        .when(!row.disabled, |line| line.cursor_pointer())
        .text_color(rgb(colour))
        .when(row.lit && !row.disabled, |line| {
            line.bg(if row.loses {
                rgb(theme.accent.rust_bg)
            } else {
                theme::control_hover(theme, theme.surface.card)
            })
        })
        .when_some(ordinal, |line, ordinal| {
            let selector = format!("popup-number-{ordinal}");
            line.child(
                div()
                    .debug_selector(move || selector.clone())
                    .flex_none()
                    .size(theme::space::MENU_ICON)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(theme::size::META)
                    .text_color(rgb(if row.disabled {
                        theme.text.faint
                    } else {
                        theme.text.tertiary
                    }))
                    .child(ordinal.to_string()),
            )
        })
        .when_some(row.check, |line, checked| {
            line.child(check_column(checked, theme))
        })
        .when(row.icons, |line| {
            line.child(
                div()
                    .debug_selector(move || icon_selector.clone())
                    .flex_none()
                    .size(theme::space::MENU_ICON)
                    .flex()
                    .items_center()
                    .justify_center()
                    .children(row.mark),
            )
        })
        .child(
            div()
                .debug_selector(move || label_selector.clone())
                .flex_1()
                .min_w_0()
                .truncate()
                .child(row.label),
        )
        .children(row.stroke.map(|stroke| {
            let selector = format!("popup-shortcut-{stroke}");
            div()
                .debug_selector(move || selector.clone())
                .flex_none()
                .pl(theme::space::MENU_INSET)
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.faint))
                .child(stroke)
        }))
        .when(row.submenu, |line| {
            line.child(
                crate::icons::toolbar("ui/chevron-right.svg")
                    .flex_none()
                    .text_color(rgb(theme.text.faint)),
            )
        })
        .children(row.trailing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_separator_aligns_with_highlight_and_uses_outline_colour() {
        for theme in theme::Theme::all() {
            let mut line = separator_look(theme);
            let style = line.style();
            assert_eq!(style.margin.left, Some(theme::radius::ROW.into()));
            assert_eq!(style.margin.right, Some(theme::radius::ROW.into()));
            assert_eq!(style.margin.top, Some(theme::space::MENU_RULE.into()));
            assert_eq!(style.margin.bottom, Some(theme::space::MENU_RULE.into()));
            assert_eq!(style.background, Some(theme::card_outline(theme).into()));
        }
        assert!(theme::space::MENU_RULE.0 > theme::from_px(4.0).0);
    }
}
