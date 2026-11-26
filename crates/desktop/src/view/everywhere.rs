//! Search Everywhere's card — section 6.33 of the design system.
//!
//! A floating picker card (section 6.32), resized and remembered like
//! Recent Projects: the head is the five tabs with *Include non-project
//! items* at their right, then the field; the rows are one `uniform_list`,
//! because *Files* and *Symbols* can hold tens of thousands (decision 4).

use gpui::prelude::*;
use gpui::{Context, HighlightStyle, MouseButton, StyledText, Window, div, px, rgb};

use crate::Root;
use crate::everywhere::{Among, Empty};
use crate::i18n::Text;
use crate::root::everywhere::Item;
use crate::symbols::{SymbolKind, Where};
use crate::theme;
use crate::view::picker::{Card, Place};

impl Root {
    /// What Search Everywhere draws.
    pub(crate) fn everywhere_card(&self, cx: &mut Context<Root>) -> Card {
        let (among, libraries) = self
            .everywhere()
            .map_or((Among::All, false), |popup| (popup.among, popup.libraries));

        let head = div()
            .flex()
            .flex_col()
            .child(self.render_everywhere_tabs(among, libraries, cx))
            .child(self.render_everywhere_query(cx))
            .into_any_element();

        Card {
            id: "everywhere",
            place: Place::Floating {
                remembered: |settings| &mut settings.everywhere,
                top: crate::root::recent::picker_top(),
                size: None,
            },
            backdrop: true,
            head,
            rows: self.render_everywhere_rows(among, cx),
            // Under the rows while it is on, for a tab whose rows are
            // places; the preview itself is group C's painting.
            preview: (self.settings.everywhere_preview && among != Among::Actions)
                .then(|| self.render_preview(cx))
                .flatten()
                .map(|preview| {
                    div()
                        .flex_none()
                        .h(theme::space::PREVIEW)
                        .border_t_1()
                        .border_color(rgb(self.theme.surface.divider))
                        .overflow_hidden()
                        .child(preview)
                        .into_any_element()
                }),
            foot: Some(
                div()
                    .flex_none()
                    .px(theme::space::CARD_PADDING)
                    .py(theme::space::CARD_GAP)
                    .border_t_1()
                    .border_color(rgb(self.theme.surface.divider))
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.faint))
                    .child(if self.ui().0 == guild_session::Language::Russian {
                        "↵ Открыть · Tab Категория · Esc Закрыть"
                    } else {
                        "↵ Open · Tab Category · Esc Close"
                    })
                    .into_any_element(),
            ),
        }
    }

    /// The five tabs, and *Include non-project items* at their right.
    fn render_everywhere_tabs(
        &self,
        chosen: Among,
        libraries: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        let tab = |among: Among, cx: &mut Context<Root>| {
            let chosen = among == chosen;
            div()
                .id(("everywhere-tab", among as usize))
                .flex()
                .items_center()
                .h(theme::space::BRANCH_TAB)
                .px(theme::space::CARD_PADDING)
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .text_size(theme::size::UI)
                .when(chosen, |tab| {
                    tab.bg(rgb(theme.surface.raised))
                        .text_color(rgb(theme.text.primary))
                })
                .when(!chosen, |tab| {
                    tab.text_color(rgb(theme.text.faint))
                        .hover(|tab| tab.bg(crate::theme::control_hover(theme, theme.surface.card)))
                })
                .child(ui.text(tab_label(among)))
                // A tab is a button in the card's handle: a press on it is
                // not the start of a drag.
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation()
                })
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.everywhere_choose(among, cx);
                }))
        };

        // Wrapping, so a card a hand made narrow keeps both switches — a
        // state that can be reached and not left is a bug with a name here.
        div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .p(theme::space::CARD_GAP)
            .children(Among::ALL.map(|among| tab(among, cx)))
            .child(div().flex_1())
            .when(chosen != Among::Actions, |row| {
                row.child(self.render_everywhere_switch(
                    "everywhere-preview",
                    self.settings.everywhere_preview,
                    Text::EverywherePreview,
                    |root, cx| root.everywhere_toggle_preview(cx),
                    cx,
                ))
            })
            .when(
                matches!(chosen, Among::All | Among::Files | Among::Symbols),
                |row| {
                    row.child(self.render_everywhere_switch(
                        "everywhere-libraries",
                        libraries,
                        Text::EverywhereNonProject,
                        |root, cx| root.everywhere_toggle_libraries(cx),
                        cx,
                    ))
                },
            )
            .into_any_element()
    }

    /// The field, under the tabs and over a hairline.
    fn render_everywhere_query(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        div()
            .flex()
            .items_center()
            .h(theme::space::BRANCH_FIELD)
            .mx(theme::space::CARD_PADDING)
            .mb(theme::space::CARD_GAP)
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .border_1()
            .border_color(rgb(theme.surface.divider))
            .text_size(theme::size::UI)
            .child(self.picker_field(
                ui.text(Text::EverywhereTypeToSearch).to_owned(),
                theme.surface.card,
                cx,
            ))
            .into_any_element()
    }

    /// The list, or the note that says which emptiness it is.
    fn render_everywhere_rows(&self, among: Among, cx: &mut Context<Root>) -> gpui::AnyElement {
        let count = self.everywhere_count();
        if count == 0 {
            return self.render_everywhere_nothing(among);
        }
        self.picker_rows(
            "everywhere-list",
            count,
            theme::space::PICKER_ROW,
            |root: &mut Root, row, lit, _window: &mut Window, cx: &mut Context<Root>| {
                root.render_everywhere_row(row, lit, cx)
            },
            cx,
        )
    }

    /// Nothing to show — and *not indexed yet*, *type something* and
    /// *nothing matches* are three different emptinesses.
    fn render_everywhere_nothing(&self, among: Among) -> gpui::AnyElement {
        let theme = self.theme;
        let typed = !self.picker_query().trim().is_empty();
        let project = self.everywhere_project();
        // A window with no project has no index to wait for.
        let files_ready = project.is_none_or(|project| self.index_ready(project));
        let symbols_ready = project.is_none_or(|project| self.symbols_ready(project));
        let walking = self.everywhere().is_some_and(|popup| popup.text.running());
        let note = match crate::everywhere::empty(among, typed, files_ready, symbols_ready, walking)
        {
            Empty::Indexing => Text::EverywhereIndexing,
            Empty::TypeToSearch => Text::EverywhereTypeToSearch,
            Empty::Searching => Text::SearchRunning,
            Empty::NothingMatches => Text::SearchNothingFound,
        };
        // A pattern that will not compile is said in the regex crate's own
        // words, as the Search tool says it.
        let refused = (among == Among::Text)
            .then(|| {
                self.everywhere()
                    .and_then(|popup| popup.text.refused().map(str::to_owned))
            })
            .flatten();

        div()
            .px(theme::space::CARD_PADDING)
            .py(theme::space::PANEL_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.faint))
            .child(refused.unwrap_or_else(|| self.ui().text(note).to_owned()))
            .into_any_element()
    }

    /// One row, whatever it is.
    fn render_everywhere_row(
        &mut self,
        row: usize,
        lit: bool,
        _cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let body = match self.everywhere_item(row) {
            Some(Item::Action(index)) => self.render_everywhere_action(index),
            Some(Item::File(index)) => self.render_everywhere_file(index),
            Some(Item::Symbol(index)) => self.render_everywhere_symbol(index),
            Some(Item::Hit(index)) => self.render_everywhere_hit(index),
            Some(Item::Heading(among)) => {
                // Never lit, and at the row's own height: the list is one
                // `uniform_list`, which gives every row one height. The
                // inset matches a row's own, as every heading in the
                // picker family does — M12.7 task 9.
                return div()
                    .flex()
                    .items_end()
                    .size_full()
                    .px(theme::space::MENU_INSET)
                    .pb(px(4.0))
                    .text_size(theme::size::EYEBROW)
                    .text_color(rgb(theme.text.faint))
                    .child(self.ui().text(tab_label(among)))
                    .into_any_element();
            }
            Some(Item::More(among)) => self.render_everywhere_more(among),
            None => div().into_any_element(),
        };
        div()
            .flex()
            .items_center()
            .h_full()
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(lit, |this| this.bg(rgb(theme.surface.raised)))
            .child(body)
            .into_any_element()
    }

    /// An action: its label with the letters that matched, and its stroke
    /// at the right. **Dimmed where the focus behind the popup does not
    /// handle it**, and Enter takes nothing there.
    fn render_everywhere_action(&self, index: usize) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let Some(popup) = self.everywhere() else {
            return div().into_any_element();
        };
        let Some((action, marks)) = popup.found.get(index) else {
            return div().into_any_element();
        };
        let available = self
            .everywhere_action(index)
            .is_some_and(|(_, available)| available);
        let label = crate::actions::labels::label(*action, ui);
        let stroke = popup.strokes.get(action).cloned();

        div()
            .flex()
            .items_center()
            .w_full()
            .gap(theme::space::CARD_INNER_GAP)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme::size::UI)
                    .text_color(rgb(if available {
                        theme.text.primary
                    } else {
                        theme.text.faint
                    }))
                    .child(marked(label, marks, theme.accent.brass)),
            )
            .children(stroke.map(|stroke| {
                div()
                    .flex_none()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(stroke)
            }))
            .into_any_element()
    }

    /// A file: its icon, its name with the letters that matched, and the
    /// directory it is in — an ignored one dimmed as the tree dims it.
    fn render_everywhere_file(&self, index: usize) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(project) = self.everywhere_project() else {
            return div().into_any_element();
        };
        let Some(file) = self.ranked_files(project).get(index) else {
            return div().into_any_element();
        };
        let path = file.path.to_string_lossy().into_owned();
        let name = file
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let parent = file
            .path
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default();
        // The marks count characters of the whole path; the name is its
        // last characters, and the directory its first.
        let before = (path.chars().count() - name.chars().count()) as u32;
        let (in_parent, in_name): (Vec<u32>, Vec<u32>) =
            file.marks.iter().partition(|mark| **mark < before);
        let in_name: Vec<u32> = in_name.into_iter().map(|mark| mark - before).collect();

        div()
            .flex()
            .items_center()
            .w_full()
            .gap(theme::space::CARD_INNER_GAP)
            .child(
                crate::icons::icon(&crate::icons::for_file(
                    &name,
                    crate::icons::Palette::from(theme.kind),
                ))
                .flex_none()
                .when(file.ignored, |icon| icon.opacity(0.5)),
            )
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::UI)
                    .text_color(rgb(if file.ignored {
                        theme.text.faint
                    } else {
                        theme.text.primary
                    }))
                    .child(marked(name, &in_name, theme.accent.brass)),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(marked(parent, &in_parent, theme.accent.brass)),
            )
            .into_any_element()
    }

    /// A symbol: a letter for its kind, its name with the letters that
    /// matched, and what it is in and where at the right.
    fn render_everywhere_symbol(&self, index: usize) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(project) = self.everywhere_project() else {
            return div().into_any_element();
        };
        let Some(symbol) = self.ranked_symbols(project).get(index) else {
            return div().into_any_element();
        };
        let (path, line) = match &symbol.at {
            Where::Project { path, line, .. }
            | Where::Library { path, line, .. }
            | Where::Outside { path, line, .. } => (path, *line),
        };
        let place = format!("{}:{}", path.to_string_lossy(), line + 1);

        div()
            .flex()
            .items_center()
            .w_full()
            .gap(theme::space::CARD_INNER_GAP)
            .child(
                div()
                    .flex_none()
                    .w(px(12.0))
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(badge(symbol.kind)),
            )
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::UI)
                    .text_color(rgb(theme.text.primary))
                    .child(marked(
                        symbol.name.clone(),
                        &symbol.marks,
                        theme.accent.brass,
                    )),
            )
            .children(symbol.container.clone().map(|container| {
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(container)
            }))
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .flex()
                    .justify_end()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(place),
            )
            .into_any_element()
    }
}

impl Root {
    /// A switch in the head: a checkbox and its word, which a press does
    /// not take for the start of a drag of the card.
    fn render_everywhere_switch(
        &self,
        id: &'static str,
        on: bool,
        label: Text,
        flip: fn(&mut Root, &mut Context<Root>),
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .flex()
            .items_center()
            .gap(theme::space::TREE_GAP)
            .px(theme::space::CARD_GAP)
            .cursor_pointer()
            .text_size(theme::size::META)
            .text_color(rgb(if on {
                theme.text.primary
            } else {
                theme.text.faint
            }))
            .child(check_box(on, theme))
            .child(self.ui().text(label))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .on_click(cx.listener(move |root, _event, _window, cx| {
                cx.stop_propagation();
                flip(root, cx);
            }))
    }

    /// A hit of *Text*: the line in the code face with its matches in
    /// brass, and the file and the line at the right.
    fn render_everywhere_hit(&self, index: usize) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(hit) = self
            .everywhere()
            .and_then(|popup| popup.text.hits().get(index))
        else {
            return div().into_any_element();
        };
        let name = hit
            .path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| hit.path.to_string_lossy().into_owned());
        // The ranges were found in this text, through the cut, so they are
        // painted as they are rather than searched for again — and only
        // where they still fall on character boundaries of what is drawn.
        let text = hit.text.trim_end().to_owned();
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = hit
            .ranges
            .iter()
            .filter(|range| {
                range.end <= text.len()
                    && text.is_char_boundary(range.start)
                    && text.is_char_boundary(range.end)
            })
            .map(|range| {
                (
                    range.clone(),
                    HighlightStyle {
                        color: Some(rgb(theme.accent.brass).into()),
                        ..Default::default()
                    },
                )
            })
            .collect();

        div()
            .flex()
            .items_center()
            .w_full()
            .gap(theme::space::CARD_INNER_GAP)
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .font_family(self.code_font.clone())
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.primary))
                    .child(
                        crate::source_snippet::SourceSnippet::new(
                            ("everywhere-source", index),
                            text,
                            highlights,
                            theme,
                        )
                        .identity(crate::view::search::result_identity(
                            self.everywhere_project(),
                            crate::Origin::Project,
                            hit,
                            self.everywhere().map_or(0, |popup| popup.text.answered()),
                        )),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(format!("{name}:{}", hit.line)),
            )
            .into_any_element()
    }

    /// *N more* under a section of *All*.
    fn render_everywhere_more(&self, among: Among) -> gpui::AnyElement {
        let theme = self.theme;
        let (files, symbols, actions, _) = self.everywhere_section_lengths();
        let count = match among {
            Among::Files => files,
            Among::Symbols => symbols,
            _ => actions,
        };
        div()
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.faint))
            .child(
                self.ui()
                    .more_rows(count.saturating_sub(crate::everywhere::SECTION)),
            )
            .into_any_element()
    }
}

/// A tab's word.
fn tab_label(among: Among) -> Text {
    match among {
        Among::All => Text::EverywhereAll,
        Among::Files => Text::EverywhereFiles,
        Among::Symbols => Text::EverywhereSymbols,
        Among::Actions => Text::EverywhereActions,
        Among::Text => Text::EverywhereText,
    }
}

/// One letter for a symbol's kind — a letter is not a word, and has
/// nothing to translate.
fn badge(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "f",
        SymbolKind::Method => "m",
        SymbolKind::Class => "C",
        SymbolKind::Struct => "S",
        SymbolKind::Interface => "I",
        SymbolKind::Enum => "E",
        SymbolKind::Module => "M",
        SymbolKind::Macro => "!",
        SymbolKind::Constant => "K",
        SymbolKind::Type => "T",
        SymbolKind::Property => "P",
        SymbolKind::Other => "·",
    }
}

/// Text with the characters `marks` names in `colour`.
///
/// **Through `fuzzy::spans`**: a mark is a character and a highlight a
/// byte range on a character boundary — a path in Cyrillic would take the
/// window down otherwise.
fn marked(text: String, marks: &[u32], colour: u32) -> StyledText {
    let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> =
        crate::fuzzy::spans(&text, marks)
            .into_iter()
            .map(|range| {
                (
                    range,
                    HighlightStyle {
                        color: Some(rgb(colour).into()),
                        ..Default::default()
                    },
                )
            })
            .collect();
    StyledText::new(text).with_highlights(highlights)
}

/// A checkbox, section 6.12's: 14 square, a hairline in `text.faint` when
/// empty, brass with a tick when ticked.
fn check_box(ticked: bool, theme: theme::Theme) -> gpui::Div {
    div()
        .flex_none()
        .size(theme::space::CHECKBOX)
        .rounded(theme::radius::CHECKBOX)
        .border_1()
        .border_color(rgb(if ticked {
            theme.accent.brass
        } else {
            theme.text.faint
        }))
        .flex()
        .items_center()
        .justify_center()
        .when(ticked, |this| {
            this.bg(rgb(theme.accent.brass))
                .child(crate::icons::check().text_color(rgb(theme.surface.panel)))
        })
}

#[cfg(test)]
mod occurrence_tests;
