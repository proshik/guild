//! The ⋮ at a panel head's right, and the trigger that opens it.
//!
//! Two controls, at the head's right: a ⋮ that opens the tool window's
//! own menu and a bar that puts the panel away. What the menu offers is
//! [`crate::dock_menu`] turned into the engine's rows by
//! [`crate::view::menu::dock`]; this is the button, drawn the way every
//! trigger of the engine's menus is since M12.7 — the tab strip's three,
//! task 2's pattern.
//!
//! **The card and its backdrop are the engine's now.** Until M12.7 this
//! module rolled its own: a hand-built card, `deferred`, `anchored`,
//! `occlude`, a full-window backdrop that closed it on a click elsewhere
//! — the window had already learned all of that once, for the right
//! button. What is left here is recording where the ⋮ opens under, and
//! asking `Root::open_menu` to draw it.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, div, px, rgb};
use guild_session::Dock;

use crate::Root;
use crate::actions::keymap::Named;
use crate::i18n::Text;
use crate::view::menu::{Anchor, Target};
use crate::{icons, theme};

impl Root {
    /// Whether the dock's own `⋮` is the menu the engine has open.
    pub(crate) fn dock_menu_open(&self, dock: Dock) -> bool {
        matches!(
            self.context_menu.as_ref().map(|open| &open.target),
            Some(Target::DockHead { dock: at, .. }) if *at == dock
        )
    }

    /// The ⋮ button.
    ///
    /// **Hung from the head's own bounds, not the button's** — §6.17's
    /// *four under the head*: `panels.rs` records the head's place with a
    /// canvas the same way this crate's other triggers record theirs, and
    /// the card's top is put four pixels under its bottom edge, the gap
    /// every menu of ours keeps under what opened it.
    pub(crate) fn render_dock_options(
        &self,
        dock: Dock,
        // A toolbar's buttons that did not fit, shown first — M12.1.
        folded: &[Named],
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let open = self.dock_menu_open(dock);
        let head_at = self.dock_head_at.clone();
        let folded: Vec<Named> = folded.to_vec();

        div()
            .id(("dock-options", dock as usize))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .cursor_pointer()
            .when(open, |button| button.bg(rgb(theme.surface.raised)))
            .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .child(icons::toolbar("ui/head-options.svg").text_color(rgb(theme.text.secondary)))
            // A tooltip on a button that opens a popup is drawn over the
            // popup — dropped while it is up, the tab strip's own rule.
            .when(!open, |button| {
                let label = ui.text(Text::PanelOptions).to_owned();
                button.hint_delay().tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::text(label.clone(), theme))
                        .into()
                })
            })
            .on_click(
                cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    let anchor = match head_at.get()[dock as usize] {
                        Some(bounds) => Anchor::Below(gpui::Bounds::new(
                            bounds.origin,
                            gpui::size(bounds.size.width, bounds.size.height + px(4.0)),
                        )),
                        None => Anchor::Pointer(event.position()),
                    };
                    root.open_menu(
                        Target::DockHead {
                            dock,
                            folded: folded.clone(),
                        },
                        anchor,
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
    }
}
