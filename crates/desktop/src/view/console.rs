//! The Git tool's two tabs, and the Console behind the second one.
//!
//! **A tab strip inside a tool is new in M8.3.4.** The dock head already
//! draws tabs and they mean *which tool*; a second row inside the body
//! means *which face of this tool*.
//! Its shape is the branch popup's (section 6.18) rather than the dock
//! head's, so that two rows of tabs, one above the other, cannot be
//! mistaken for one another.

use gpui::prelude::*;
use gpui::{Context, div, rgb};
use guild_session::GitFace;

use gpui_component::input::Textarea;

use crate::Root;
use crate::i18n::Text;
use crate::theme;

impl Root {
    /// The Git tool: the strip, and whichever face it names.
    pub(crate) fn render_git_tool(
        &self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> Option<gpui::AnyElement> {
        let face = self.git_face();
        let body = match face {
            GitFace::Log => self.render_log(window, cx)?,
            GitFace::Console => self.render_console(cx),
        };

        Some(body)
    }

    /// *Log* and *Console*, and which of them is showing.
    pub(crate) fn render_git_tabs(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let face = self.git_face();
        let theme = self.theme;
        let ui = self.ui();

        let tab = |wanted: GitFace, label: Text, id: &'static str| {
            let chosen = face == wanted;
            div()
                .id(id)
                .flex_none()
                .h(theme::space::HEAD_TAB)
                .flex()
                .items_center()
                .px(theme::space::CARD_GAP)
                .rounded(theme::radius::STATUS_BUTTON)
                .cursor_pointer()
                .text_size(theme::size::EYEBROW)
                // The head's face, M12.4: capitals, semibold.
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .when(chosen, |t| {
                    t.bg(rgb(theme.surface.raised))
                        .text_color(rgb(theme.text.primary))
                })
                .when(!chosen, |t| {
                    t.text_color(rgb(theme.text.faint))
                        .hover(|t| t.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                })
                .child(ui.text(label).to_uppercase())
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.show_git_face(wanted, cx);
                }))
        };

        div()
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .child(tab(GitFace::Log, Text::Log, "git-tab-log"))
            .child(tab(GitFace::Console, Text::ConsoleTab, "git-tab-console"))
            .into_any_element()
    }

    /// What the window told git, newest at the bottom.
    ///
    /// **Newest at the bottom**, as a terminal is: a console is read
    /// downwards, and the place a person looks after pressing a button is
    /// the end. In the code face, because it is a command line and a
    /// proportional one would not line up with the terminal in the next
    /// tab.
    fn render_console(&self, _cx: &mut Context<Root>) -> gpui::AnyElement {
        let ui = self.ui();
        let Some(console) = self.active_project.and_then(|id| self.consoles.get(&id)) else {
            return self.console_note(ui.text(Text::ConsoleEmpty));
        };
        let Some((editor, _)) = self.console_view.as_ref().filter(|_| !console.is_empty()) else {
            return self.console_note(ui.text(Text::ConsoleEmpty));
        };

        // **The component's editor, read-only** — 2026-09-23: a `div` a
        // line could not be selected, and what git said is exactly what a
        // person copies into a question. It grows to its text; this box
        // scrolls, so that the window can put the newest line in view
        // without moving the editor's caret, which takes the keyboard.
        // The red of a refused command went with the `div`s — the
        // person's choice.
        div()
            .id("git-console")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.console_scroll)
            .p(theme::space::CARD_PADDING)
            .child(
                Textarea::new(editor)
                    .disabled(true)
                    .appearance(false)
                    .font_family(self.code_font.clone())
                    .text_size(theme::size::META),
            )
            .into_any_element()
    }

    /// One faint line where a console would be.
    fn console_note(&self, text: &str) -> gpui::AnyElement {
        div()
            .flex_1()
            .min_h_0()
            .p(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(self.theme.text.faint))
            .child(text.to_owned())
            .into_any_element()
    }
}
