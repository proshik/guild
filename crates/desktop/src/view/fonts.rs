//! The list a face is chosen from, hanging under the button that opens
//! it.
//!
//! Section 6.18's card, because it is the same gesture as the branch
//! popup and the run list: a field that narrows, rows taken with `enter`,
//! one highlight moved by the pointer.
//!
//! **Every row is drawn in the face it names.** That is the whole reason
//! a list beats a cycle: a person choosing a font is choosing how letters
//! look, and a list of names in one face tells them nothing.

use gpui::prelude::*;
use gpui::{Context, div, rgb};

use crate::fonts::{From, Which};
use crate::i18n::Text;
use crate::view::picker::{Card, Place};
use crate::{Root, theme};

impl Root {
    /// The button a setting is changed with: the family it is on now, and
    /// the list under it.
    pub(crate) fn render_font_button(
        &self,
        which: Which,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let chosen = match which {
            Which::Ui => self.settings.ui_font.clone(),
            Which::Code => self.settings.code_font.clone(),
            Which::Terminal => self.settings.terminal_font.clone(),
        };
        let label = chosen
            .clone()
            .unwrap_or_else(|| ui.text(Text::FontFromSystem).to_owned());
        // Drawn in the face it names, so the button is a sample of what
        // it will do.
        let face = chosen.unwrap_or_else(|| which.default_face().to_owned());

        div()
            .id("setting-ui-font")
            .tab_index(0)
            .border_1()
            .border_color(rgb(theme.surface.divider))
            .focus(|style| style.border_color(rgb(theme.accent.patina)))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .h(theme::settings::CONTROL)
            .px(theme::space::CARD_PADDING)
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.raised))
            .cursor_pointer()
            .text_size(theme::size::UI)
            .text_color(rgb(theme.text.primary))
            .font_family(face)
            .child(label)
            .child(crate::icons::chevron(true).text_color(rgb(theme.text.faint)))
            .on_click(cx.listener(move |root, _event, window, cx| {
                cx.stop_propagation();
                root.toggle_font_picker(which, window, cx);
            }))
            .child({
                // Where the button ended up, for the list that hangs from
                // it. One cell per button: they are drawn in the same
                // frame, and a single shared one would keep whichever was
                // drawn last.
                let at = self.font_at[which as usize].clone();
                gpui::canvas(
                    move |bounds, _window, _cx| at.set(bounds.bottom_left()),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .into_any_element()
    }

    /// The card the font list answers with, hanging from the button of
    /// the setting it is choosing for.
    pub(crate) fn fonts_card(&self, cx: &mut Context<Root>) -> Card {
        let theme = self.theme;
        let ui = self.ui();
        let which = self.font_list_for().unwrap_or(Which::Ui);
        let rows = self.font_rows();

        let head = div()
            .flex()
            .items_center()
            .h(theme::space::BRANCH_FIELD)
            .px(theme::space::CARD_PADDING)
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .text_size(theme::size::UI)
            .child(self.picker_field(ui.text(Text::FontSearch).to_owned(), theme.surface.card, cx))
            .into_any_element();

        let rows = if rows.is_empty() {
            div()
                .p(theme::space::CARD_PADDING)
                .text_size(theme::size::UI)
                .text_color(rgb(theme.text.faint))
                .child(ui.text(Text::FontNoneFound))
                .into_any_element()
        } else {
            let lit = self.picker_lit_row();
            let items = rows
                .iter()
                .enumerate()
                .map(|(index, row)| self.render_font_row(which, index, row, lit == Some(index), cx))
                .collect();
            self.picker_children("font-rows", items, Some(theme::space::BRANCH_LIST), cx)
        };

        Card {
            id: "font-card",
            place: Place::Hanging {
                from: self.font_at[which as usize].get(),
                width: theme::space::BRANCH_POPUP,
            },
            backdrop: true,
            head,
            rows,
            preview: None,
            foot: None,
        }
    }

    /// One row: the family, drawn in itself, with where it comes from at
    /// the right.
    fn render_font_row(
        &self,
        which: Which,
        index: usize,
        row: &crate::fonts::Row,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let label = row
            .family
            .clone()
            .unwrap_or_else(|| ui.text(Text::FontFromSystem).to_owned());
        // The row is a sample: a face nobody can see is a name nobody can
        // choose between.
        let face = row
            .family
            .clone()
            .unwrap_or_else(|| which.default_face().to_owned());

        self.picker_item(index, div().id(("font-row", index)), cx)
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .h(theme::space::PICKER_ROW)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(face)
                    .text_size(theme::size::UI)
                    .text_color(rgb(theme.text.primary))
                    .child(label),
            )
            .children(source_label(row.from).map(|text| {
                div()
                    .flex_none()
                    .text_size(theme::size::EYEBROW)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(text))
            }))
            .into_any_element()
    }
}

/// What a row is, when it is worth saying. *From the system* says it in
/// its own words already.
fn source_label(from: From) -> Option<Text> {
    match from {
        From::System => None,
        From::Bundled => Some(Text::FontBundled),
        From::Installed => None,
    }
}
