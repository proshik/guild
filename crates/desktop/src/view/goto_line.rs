//! Go to Line:Column — `cmd-g`.
//!
//! One field and nothing else. A small popup near the caret with the
//! file's range as its hint is the common shape; ours hangs where the
//! Recent Files popup does, because a field that appears next to a caret a person has
//! just stopped looking at is a field they then have to find.
//!
//! **`cmd-g` and not `⌘L`** — decision 4 of M8.8. `cmd-l` is the
//! right panel, one third of a family this window has had since M5.7 and
//! uses dozens of times a day.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

use crate::Root;
use crate::i18n::Text;
use crate::theme;

impl Root {
    /// The field, over everything.
    pub(crate) fn render_goto_line(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(query) = self.goto_line_query() else {
            return div().into_any_element();
        };
        let ui = self.ui();

        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .items_start()
            .justify_center()
            .child(
                div()
                    .id("goto-line-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(dim(theme.surface.backdrop))
                    .on_click(
                        cx.listener(|root, _event, window, cx| root.toggle_goto_line(window, cx)),
                    ),
            )
            .child(
                div()
                    .mt(theme::space::RECENT_FILES_TOP)
                    .w(theme::space::GOTO_LINE)
                    // The backdrop under it must not count a press aimed
                    // at the field: a hitbox does not hide what is behind
                    // it by covering it.
                    .occlude()
                    .flex()
                    .items_center()
                    .px(theme::space::CARD_PADDING)
                    .py(px(10.0))
                    .rounded(theme::radius::CARD)
                    .bg(rgb(theme.surface.card))
                    .border_1()
                    .border_color(theme::card_outline(theme))
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.primary))
                    .child(
                        crate::text_field::TextField::new(
                            "goto-line-input",
                            query,
                            cx.listener(|root, value: &String, _, cx| {
                                root.set_goto_line_query(value.clone());
                                cx.notify();
                            }),
                        )
                        .placeholder(ui.text(Text::GoToLinePlaceholder))
                        .focus(&self.focus)
                        .autofocus()
                        .selected(true)
                        .on_key(cx.listener(|root, event, window, cx| {
                            root.goto_line_key(event, window, cx);
                        })),
                    ),
            )
            .into_any_element()
    }
}

/// The backdrop, dimmed rather than opaque: the file behind is what the
/// number is about.
pub(crate) fn dim(colour: u32) -> gpui::Hsla {
    let mut shade: gpui::Hsla = gpui::rgb(colour).into();
    shade.a = 0.32;
    shade
}
