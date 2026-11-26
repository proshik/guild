//! The Recent Projects popup.
//!
//! The common IDE gesture: one stroke, a list of the directories opened
//! before, type to narrow it, Enter to open. The list itself is the same one
//! the File menu shows — a popup that disagreed with the menu about what
//! "recent" means would be two lists to keep in step.

use gpui::prelude::*;
use gpui::{Context, div, px, rgb};

use crate::Root;
use crate::i18n::Text;
use crate::root::recent::{home_relative, picker_top};
use crate::theme;
use crate::view::picker::{Card, Place};

impl Root {
    /// The card Recent Projects answers with: the field as its head, the
    /// directories as its rows, floating where it was left and resized
    /// from its edges.
    pub(crate) fn recent_projects_card(&self, cx: &mut Context<Root>) -> Card {
        let theme = self.theme;
        let ui = self.ui();
        let rows = self.recent_rows();
        let lit = self.picker_lit_row();
        let query = self.picker_query().to_owned();
        let home = dirs_home();
        let open: Vec<_> = self
            .desk
            .projects()
            .map(|project| project.path.clone())
            .collect();

        let head = div()
            .px(theme::space::CARD_PADDING)
            .py(px(10.0))
            .bg(rgb(theme.surface.sunken))
            .text_size(theme::size::BODY)
            .child(self.picker_field(
                ui.text(Text::RecentSearch).to_owned(),
                theme.surface.sunken,
                cx,
            ))
            .into_any_element();

        let rows = if rows.is_empty() {
            self.render_recent_nothing(&query, cx)
        } else {
            let items = rows
                .into_iter()
                .enumerate()
                .map(|(row, (index, path))| {
                    let already = open.contains(&path);
                    self.render_recent_row(
                        row,
                        index,
                        &path,
                        home.as_deref(),
                        lit == Some(row),
                        already,
                        cx,
                    )
                })
                .collect();
            self.picker_children("recent-list", items, None, cx)
        };

        Card {
            id: "recent-projects",
            place: Place::Floating {
                remembered: |settings| &mut settings.picker,
                top: picker_top(),
                size: None,
            },
            backdrop: true,
            head,
            rows,
            preview: None,
            foot: None,
        }
    }

    /// One directory.
    /// `row` is the place on screen, `index` the place on disk; the two
    /// stop being the same list the moment a query narrows it. The
    /// highlight counts rows, opening counts the index.
    #[allow(clippy::too_many_arguments)]
    fn render_recent_row(
        &self,
        row: usize,
        index: usize,
        path: &std::path::Path,
        home: Option<&std::path::Path>,
        lit: bool,
        already: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let where_it_is = home_relative(path, home);

        self.picker_item(row, div().id(("recent-row", index)), cx)
            .flex()
            .items_center()
            .gap(theme::space::CARD_INNER_GAP)
            .h(theme::space::PICKER_ROW)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |this| this.bg(rgb(theme.surface.raised)))
            .child(
                // The check column every row of this picker reserves, as a
                // menu's does: the project already open wears it, which is
                // what the brass dot used to mean — M12.7 task 9. Shared
                // with a menu's own row since the final fix wave, so the
                // two markups cannot drift apart.
                crate::view::popup::check_column(already, theme),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(theme::size::CARD_TITLE)
                    .text_color(rgb(theme.text.primary))
                    .child(name),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(where_it_is),
            )
            // No tint of the pointer's own: the pointer moves the one
            // highlight, and a click takes the row — `picker_item`.
            .into_any_element()
    }

    /// Nothing to show — and the two reasons for that are different.
    fn render_recent_nothing(&self, query: &str, _cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .px(theme::space::CARD_PADDING)
            .py(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(ui.text(if query.trim().is_empty() {
                // Nothing has ever been opened. Telling that person to try
                // another query would be advice about a field they have
                // not touched.
                Text::RecentNever
            } else {
                Text::RecentNoMatch
            }))
            .into_any_element()
    }
}

/// The home directory, for writing a path with a `~`.
///
/// `None` when the platform will not say, and then paths are shown in
/// full: a guess at somebody's home directory that is wrong would hide
/// the very part of the path that identifies the project.
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}
