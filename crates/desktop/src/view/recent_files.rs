//! The Recent Files popup — `cmd-e`.
//!
//! A picker card (section 6.32) with the size section 6.25 gives it. **It
//! moves by its head and remembers where it was left, and it does not
//! resize** — M12.3 gave it the move, because a person with a file in the
//! middle of the window wanted the popup somewhere else; the size stays
//! the design system's, because this is a switch made in under a second
//! and a popup a person has to size first is not one.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

use crate::Root;
use crate::i18n::Text;
use crate::root::recent_files::{Face, Location};
use crate::theme;
use crate::view::picker::{Card, Place};

impl Root {
    /// The card Recent Files answers with: the query line as its head,
    /// the files or the places as its rows.
    pub(crate) fn recent_files_card(&self, cx: &mut Context<Root>) -> Card {
        let (face, edited) = self
            .picker
            .as_ref()
            .and_then(|open| match &open.holds {
                crate::root::picker::Holds::RecentFiles(popup) => Some((popup.face, popup.edited)),
                _ => None,
            })
            .unwrap_or((Face::Files, false));
        let lit = self.picker_lit_row();
        let query = self.picker_query().to_owned();
        let rem = f32::from(crate::scale::rem_size(self.settings.ui_font_size));

        let rows = match face {
            Face::Files => {
                let rows = self.recent_files_rows();
                if rows.is_empty() {
                    self.render_recent_files_nothing(&query, edited)
                } else {
                    let items = rows
                        .into_iter()
                        .enumerate()
                        .map(|(row, (_, path))| {
                            self.render_recent_file_row(row, &path, lit == Some(row), cx)
                        })
                        .collect();
                    self.picker_children("recent-files-list", items, None, cx)
                }
            }
            Face::Locations => {
                let rows = self.recent_location_rows();
                if rows.is_empty() {
                    self.render_recent_locations_nothing(&query)
                } else {
                    let items = rows
                        .into_iter()
                        .enumerate()
                        .map(|(row, location)| {
                            self.render_recent_location_row(row, &location, lit == Some(row), cx)
                        })
                        .collect();
                    self.picker_children("recent-locations-list", items, None, cx)
                }
            }
        };

        Card {
            id: "recent-files",
            place: Place::Floating {
                remembered: |settings| &mut settings.recent_files,
                top: theme::space::RECENT_FILES_TOP.0 * rem,
                size: Some((theme::space::RECENT_FILES, theme::space::RECENT_FILES_LIST)),
            },
            backdrop: true,
            head: self.render_recent_files_query(face, edited, cx),
            rows,
            preview: None,
            foot: (face == Face::Files).then(|| {
                div()
                    .px(theme::space::CARD_PADDING)
                    .py(theme::space::CARD_GAP)
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.muted))
                    .child(self.ui().text(Text::RecentFilesCloseHint))
                    .into_any_element()
            }),
        }
    }

    /// The line being typed into, with the caret every field of ours has.
    fn render_recent_files_query(
        &self,
        face: Face,
        edited: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        if face == Face::Files {
            return div()
                .flex()
                .items_center()
                .min_w_0()
                .px(theme::space::CARD_PADDING)
                .py(theme::space::CARD_GAP)
                .gap(theme::space::CARD_INNER_GAP)
                .bg(rgb(theme.surface.sunken))
                .text_size(theme::size::UI)
                .child(
                    div()
                        .flex_1()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(ui.text(Text::MenuRecentFiles)),
                )
                .child(
                    div()
                        .id("recent-files-edited")
                        .debug_selector(|| "recent-files-edited".into())
                        .role(gpui::Role::CheckBox)
                        .aria_label(ui.text(Text::RecentFilesEdited))
                        .aria_toggled(if edited {
                            gpui::Toggled::True
                        } else {
                            gpui::Toggled::False
                        })
                        .flex()
                        .items_center()
                        .gap(theme::space::CARD_GAP)
                        .cursor_pointer()
                        .text_color(rgb(theme.text.secondary))
                        .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(move |root, _, _, cx| {
                            root.recent_files_set_edited(!edited, cx);
                            cx.stop_propagation();
                        }))
                        .child(
                            div()
                                .size(theme::space::HEAD_ICON)
                                .flex_none()
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme::radius::CHECKBOX)
                                .border_1()
                                .border_color(rgb(theme.text.faint))
                                .when(edited, |check| {
                                    check
                                        .bg(rgb(theme.accent.patina))
                                        .text_color(rgb(theme.surface.app))
                                        .child(
                                            crate::icons::toolbar("ui/check.svg")
                                                .text_color(rgb(theme.surface.app)),
                                        )
                                }),
                        )
                        .child(ui.text(Text::RecentFilesEdited)),
                )
                .into_any_element();
        }

        div()
            .flex()
            .items_center()
            .px(theme::space::CARD_PADDING)
            .py(px(10.0))
            .bg(rgb(theme.surface.sunken))
            .text_size(theme::size::BODY)
            .child(
                self.picker_field(
                    ui.text(match face {
                        Face::Files => Text::RecentFilesSearch,
                        Face::Locations => Text::RecentLocationsSearch,
                    })
                    .to_owned(),
                    theme.surface.sunken,
                    cx,
                ),
            )
            // What the two gestures that are not typing do, said where
            // they are used rather than in a manual: the second `cmd-e`
            // narrows to what was edited, and Backspace closes a file.
            .child(div().flex_1())
            .when(edited, |line| {
                line.child(
                    div()
                        .mr(theme::space::CARD_INNER_GAP)
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.accent.brass))
                        .child(ui.text(Text::RecentFilesEdited).to_owned()),
                )
            })
            // ⌫ closes a *file*; a place is not a thing that can be
            // closed, so the Locations face does not promise it.
            .when(face == Face::Files, |line| {
                line.child(
                    div()
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(ui.text(Text::RecentFilesCloseHint).to_owned()),
                )
            })
            .into_any_element()
    }

    /// One place: where it is, and the five lines around it.
    ///
    /// **The code is drawn in the code face**, as the search results are
    /// and for the same reason — an excerpt read in the interface font is
    /// a line that does not look like the line it came from. Uncoloured:
    /// the spans would want a parse per row, and this popup is opened and
    /// gone inside a second.
    fn render_recent_location_row(
        &self,
        row: usize,
        location: &Location,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let family: gpui::SharedString = self.code_font.clone();
        let name = location.place.path.to_string_lossy().into_owned();
        let at = location.place.line + 1;

        self.picker_item(row, div().id(("recent-location-row", row)), cx)
            .flex_none()
            .flex()
            .flex_col()
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .py(theme::space::TREE_INSET)
            .gap(px(2.0))
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |this| this.bg(rgb(theme.surface.raised)))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_INNER_GAP)
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.faint))
                            .child(format!(":{at}")),
                    ),
            )
            .children(location.lines.iter().enumerate().map(|(offset, line)| {
                let number = location.first + offset as u32;
                let own = number == at;
                div()
                    .flex()
                    .gap(theme::space::TREE_GAP)
                    .font_family(family.clone())
                    .text_size(theme::size::META)
                    .child(
                        div()
                            .flex_none()
                            .w(theme::space::SEARCH_LINE_NUMBER)
                            .text_color(rgb(theme.text.gutter))
                            .child(number.to_string()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            // The place's own line is the one being
                            // offered; the four around it are context and
                            // say so by being quieter.
                            .text_color(rgb(if own {
                                theme.text.primary
                            } else {
                                theme.text.muted
                            }))
                            .child(line.clone()),
                    )
            }))
            .into_any_element()
    }

    /// Nowhere to go — and the two reasons differ.
    fn render_recent_locations_nothing(&self, query: &str) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .px(theme::space::CARD_PADDING)
            .py(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(ui.text(if query.trim().is_empty() {
                Text::RecentLocationsNever
            } else {
                Text::RecentFilesNoMatch
            }))
            .into_any_element()
    }

    /// One file: its icon, its name, and the directory it is in.
    fn render_recent_file_row(
        &self,
        row: usize,
        path: &std::path::Path,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let parent = path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();

        self.picker_item(row, div().id(("recent-file-row", row)), cx)
            .flex_none()
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .h(theme::space::TREE_ROW)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |this| this.bg(rgb(theme.surface.raised)))
            // No tint of the pointer's own: the pointer moves the one
            // highlight, and a click takes the row — `picker_item`.
            // The file's own icon, from the same table the tree draws
            // from: a file that is one colour in the tree and another
            // here is two answers to one question.
            .child(
                crate::icons::icon(&crate::icons::for_file(
                    &name,
                    crate::icons::Palette::from(theme.kind),
                ))
                .flex_none(),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme::size::UI)
                    .text_color(rgb(theme.text.primary))
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(parent),
            )
            .into_any_element()
    }

    /// Nothing to show — and the three reasons for it are different.
    ///
    /// *No file has been opened yet* is not *nothing matches what you
    /// typed*, and neither is *nothing has been edited this session*. A
    /// note that names the wrong emptiness reads as the tool being
    /// broken.
    fn render_recent_files_nothing(&self, query: &str, edited: bool) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .px(theme::space::CARD_PADDING)
            .py(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(ui.text(match (query.trim().is_empty(), edited) {
                (false, _) => Text::RecentFilesNoMatch,
                (true, true) => Text::RecentFilesNoEdits,
                (true, false) => Text::RecentFilesNever,
            }))
            .into_any_element()
    }
}
