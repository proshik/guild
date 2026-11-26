//! The Search panel: a toolbar and what it found. M8.7.
//!
//! The toolbar is the Log's, in the same heights and the same colours —
//! two native text fields, and three
//! toggles that are the same button drawn three times. Under it the
//! results, and at the right of the toolbar what the search came to.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, SharedString, div, px, rgb, uniform_list};

use crate::Root;
use crate::i18n::Text;
use crate::root::search::Typing;
use crate::search::Toggle;
use crate::theme;

impl Root {
    /// The panel's body: the toolbar, and the rows under it.
    pub(crate) fn render_search(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let state = self.search_state()?;
        let focus = state.focus.clone();

        Some(
            div()
                .id("search")
                // **The focus goes on the panel, toolbar included** — the
                // Log's rule, and for the reason its comment gives: an
                // element that tracks focus takes it on mouse-down and
                // stops the root taking it back, so the pressed element
                // has to be the focusable one.
                .track_focus(&focus)
                .key_context("search")
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if root.search_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(self.render_search_toolbar(cx))
                .child(self.render_search_results(cx))
                .into_any_element(),
        )
    }

    /// What to look for, where to look, and how to read it — then, at
    /// the right, what came back.
    fn render_search_toolbar(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        div()
            .flex_none()
            .flex()
            // **It wraps, and the toolbar grows.** Five controls do not
            // fit a 300-pixel dock, and a tool that can be moved to one
            // must not lose them there: a person who turned on *match
            // case* at the bottom and then moved the panel left would
            // have no way to turn it off. Seen in the window — the Log
            // answers the same question with `log_layout`, which is a
            // measurement because its three columns cannot wrap; these
            // controls can.
            .flex_wrap()
            .items_center()
            .gap(theme::space::TREE_GAP)
            .min_h(theme::space::LOG_TOOLBAR)
            .py(theme::space::CARD_INNER_GAP)
            .px(theme::space::TREE_PADDING)
            .child(self.render_search_field(Typing::Text, cx))
            .child(self.render_search_toggle(Toggle::MatchCase, "Aa", Text::SearchMatchCase, cx))
            .child(self.render_search_toggle(Toggle::WholeWord, "W", Text::SearchWholeWord, cx))
            .child(self.render_search_toggle(Toggle::Regex, ".*", Text::SearchRegex, cx))
            .child(self.render_search_field(Typing::Mask, cx))
            .children(self.render_search_scope(cx))
            .child(div().flex_1())
            .child(self.render_search_count())
            .into_any_element()
    }

    /// Where the search is looking, when it is not looking at the whole
    /// project — and the cross that widens it back.
    ///
    /// **Drawn only when there is a scope, and never without the cross.**
    /// M8.7 shipped no scope control on purpose; this one arrives from a
    /// tree row, and a narrowing a person cannot see is a panel that has
    /// stopped finding things for no reason they can read. The cross is
    /// the `Branch: HEAD` lesson: a state that can be reached and not
    /// left was reported twice as a broken key.
    fn render_search_scope(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let ui = self.ui();
        let scope = self.search_state()?.query.scope.clone()?;
        Some(
            div()
                .id("search-scope")
                .flex_none()
                .h(theme::space::HEAD_TAB)
                .flex()
                .items_center()
                .gap(theme::space::CARD_INNER_GAP)
                .px(theme::space::CARD_GAP)
                .rounded(theme::radius::ROW)
                .bg(rgb(theme.surface.inset))
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.secondary))
                .child(format!(
                    "{}: {}",
                    ui.text(Text::SearchScope),
                    scope.display()
                ))
                .child(
                    div().id("search-scope-clear").child(
                        crate::icons::glyph("icons/close.svg", theme::size::META)
                            .text_color(rgb(theme.text.faint)),
                    ),
                )
                .cursor_pointer()
                .on_click(cx.listener(|root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.scope_search(None, cx);
                }))
                .into_any_element(),
        )
    }

    /// One of the two fields, edited by the shared InputState adapter with a
    /// native caret and selection, and lit at its border
    /// when the letters are going to it.
    fn render_search_field(&self, which: Typing, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let Some(state) = self.search_state() else {
            return div().into_any_element();
        };
        let (text, placeholder, width) = match which {
            Typing::Text => (
                state.query.text.clone(),
                Text::SearchPlaceholder,
                theme::space::LOG_FIELD,
            ),
            Typing::Mask => (
                state.query.mask.clone(),
                Text::SearchMask,
                theme::space::SEARCH_MASK_FIELD,
            ),
        };
        let empty = text.is_empty();
        let lit = state.typing == which;
        // **The query selected whole**, after `cmd-shift-f` over a query
        // already there: the next edit replaces it. On `accent.patina_bg`,
        // what the Log's menus light their chosen row with — not the
        // editor's `brass_bg`, which the field's own border already
        // spends on *the letters go here*. M5.11, decision 5.
        let selected = which == Typing::Text && state.replacing && !empty;
        // The field that carries the mistake says so, so that a person
        // reading *unclosed group* knows which of the two to fix.
        let wrong = state.walk.refused().is_some()
            && match which {
                Typing::Text => state.query.regular_expression,
                Typing::Mask => !state.query.mask.is_empty(),
            };

        div()
            .id(match which {
                Typing::Text => "search-field",
                Typing::Mask => "search-mask",
            })
            .flex_none()
            .w(width)
            .h(theme::space::HEAD_TAB)
            .flex()
            .items_center()
            .px(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(theme.surface.inset))
            .cursor_pointer()
            .when(selected, |field| field.bg(rgb(theme.accent.patina_bg)))
            .when(lit || wrong, |field| {
                field.border_1().border_color(rgb(if wrong {
                    theme.accent.rust
                } else {
                    theme.accent.brass
                }))
            })
            .text_size(theme::size::META)
            .text_color(rgb(if empty {
                theme.text.faint
            } else {
                theme.text.primary
            }))
            .child(
                crate::text_field::TextField::new(
                    match which {
                        Typing::Text => "search-text",
                        Typing::Mask => "search-mask-input",
                    },
                    text,
                    cx.listener(move |root, value: &String, _, cx| {
                        if let Some(project) = root.active_project {
                            if let Some(state) = root.searches.get_mut(&project) {
                                match which {
                                    Typing::Text => state.query.text = value.clone(),
                                    Typing::Mask => state.query.mask = value.clone(),
                                }
                                state.replacing = false;
                            }
                            root.restart_search(project, cx);
                        }
                    }),
                )
                .placeholder(ui.text(placeholder))
                .list()
                .selected(selected)
                .when(lit, |input| input.focus(&state.focus))
                .on_focus(cx.listener(move |root, _, _, cx| root.type_search_into(which, cx)))
                .on_key(cx.listener(|root, event, window, cx| {
                    root.search_key(event, window, cx);
                    if event.keystroke.key == "tab"
                        && let Some(state) = root.search_state()
                    {
                        window.focus(&state.focus, cx);
                    }
                })),
            )
            .into_any_element()
    }

    /// One of the three toggles: a letter rather than an icon, because
    /// `Aa`, `W` and `.*` are what every search field in this trade
    /// draws and a person reads them without a tooltip.
    fn render_search_toggle(
        &self,
        which: Toggle,
        label: &'static str,
        hint: Text,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let on = self
            .search_state()
            .is_some_and(|state| which.is_on(&state.query));

        div()
            .id(SharedString::from(format!("search-toggle-{label}")))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .size(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(on, |button| button.bg(rgb(theme.accent.brass_bg)))
            .when(!on, |button| {
                button.hover(|button| {
                    button.bg(crate::theme::control_hover(theme, theme.surface.sunken))
                })
            })
            .text_size(theme::size::META)
            .text_color(rgb(if on {
                theme.accent.brass
            } else {
                theme.text.tertiary
            }))
            .child(label)
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| crate::view::tooltip::Tooltip::text(ui.text(hint), theme))
                    .into()
            })
            .on_click(cx.listener(move |root, _event, _window, cx| {
                cx.stop_propagation();
                root.toggle_search(which, cx);
            }))
            .into_any_element()
    }

    /// What the search came to: *searching…* while it walks, then the
    /// counts, and *and more* when a limit stopped it.
    ///
    /// **A count with no note is a lie about the repository.** Task 0
    /// measured a common word at 503,629 hits; five thousand of them
    /// with no word to say so reads as *this is all there is*.
    fn render_search_count(&self) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let Some(state) = self.search_state() else {
            return div().into_any_element();
        };
        // **A pattern nobody can compile is said beside the fields, not
        // in place of the list.** The list is the last answer that made
        // sense, and replacing it with an error takes away the thing the
        // person was reading while they fix a bracket.
        if let Some(said) = state.walk.refused() {
            let whole = said.to_owned();
            return div()
                .id("search-error")
                .flex_none()
                .max_w(px(420.0))
                .overflow_hidden()
                .text_size(theme::size::META)
                .text_color(rgb(theme.accent.rust))
                .child(sentence(said))
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_cx| crate::view::tooltip::Tooltip::details(whole.clone(), theme))
                        .into()
                })
                .into_any_element();
        }
        if state.query.is_empty() || state.walk.hits().is_empty() {
            // Nothing typed and nothing found are the body's to say, and
            // they are different sentences.
            return if state.walk.running() {
                div()
                    .flex_none()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(Text::SearchRunning))
                    .into_any_element()
            } else {
                div().into_any_element()
            };
        }
        let mut said = ui.matches_in_files(state.walk.hits().len(), state.walk.files());
        if state.walk.capped() {
            said.push(' ');
            said.push_str(ui.text(Text::SearchCapped));
        }
        if state.walk.running() {
            said.push_str(" · ");
            said.push_str(ui.text(Text::SearchRunning));
        }
        div()
            .flex_none()
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.faint))
            .child(said)
            .into_any_element()
    }

    /// The hits, as a tree of the files they are in.
    ///
    /// A directory row, a file row, then the lines — the shape the Log's
    /// opened commit already draws, with folding added and the same
    /// single-child collapsing, because the rows a person walks past to
    /// reach a file are the ones worth removing.
    fn render_search_results(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let Some(state) = self.search_state() else {
            return div().flex_1().into_any_element();
        };
        let presentation = state.presentation();
        let selected = state.selected;
        let files = state.files.clone();
        let results_focus = state.results_focus.clone();
        let scroll = state.scroll.clone();
        let rows = presentation.rows.clone();
        let count = rows.len();
        if count == 0 {
            // **Three emptinesses, three sentences.** Nothing typed is
            // not nothing found, and neither is a pattern that will not
            // compile — which the toolbar is already saying, so the body
            // says nothing rather than saying it twice.
            let note = if state.walk.refused().is_some() || state.walk.running() {
                None
            } else if state.query.is_empty() {
                Some(Text::PanelSearchEmpty)
            } else {
                Some(Text::SearchNothingFound)
            };
            return div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .text_size(theme::size::UI)
                .text_color(rgb(theme.text.faint))
                .children(note.map(|note| self.ui().text(note).to_owned()))
                .into_any_element();
        }
        // **The line is code, so it is drawn in the code face.** A hit
        // read in the interface font is a line that does not look like
        // the line it came from — the indentation lies and the operators
        // are the wrong width.
        let family = self.code_font.clone();

        div()
            .id("search-results")
            .track_focus(&state.results_focus)
            .on_action(cx.listener(|root, _: &crate::actions::Copy, _window, cx| {
                root.copy_search_files(cx);
                cx.stop_propagation();
            }))
            .on_action(
                cx.listener(|root, _: &crate::actions::SelectAll, _window, cx| {
                    root.select_all_search_files(cx);
                    cx.stop_propagation();
                }),
            )
            .on_key_down(cx.listener(|root, event, window, cx| {
                if root.search_results_key(event, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .flex_1()
            .min_h_0()
            .child(
                uniform_list(
                    "search-rows",
                    count,
                    cx.processor(move |root, range: std::ops::Range<usize>, window, cx| {
                        // Blue while the results have the keys — decision 54.
                        let focused = crate::view::tree::keys_in(&results_focus, window, cx);
                        range
                            .map(|index| {
                                row(
                                    &rows[index],
                                    index,
                                    crate::theme::Selected::of(
                                        match &rows[index] {
                                            crate::search_tree::Row::File { path, .. } => {
                                                files.contains(path)
                                            }
                                            _ => selected == Some(index),
                                        },
                                        focused,
                                    ),
                                    &presentation.hits,
                                    &presentation.folded,
                                    &family,
                                    theme,
                                    root,
                                    cx,
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&scroll)
                .size_full(),
            )
            .into_any_element()
    }
}

/// One row of the results, whichever kind it is.
#[allow(clippy::too_many_arguments)]
fn row(
    row: &crate::search_tree::Row,
    index: usize,
    lit: crate::theme::Selected,
    hits: &[crate::search::Hit],
    folded: &std::collections::HashSet<std::path::PathBuf>,
    family: &SharedString,
    theme: crate::theme::Theme,
    root: &mut Root,
    cx: &mut Context<Root>,
) -> gpui::AnyElement {
    use crate::search_tree::Row;

    let depth = match row {
        Row::Directory { depth, .. } | Row::File { depth, .. } | Row::Hit { depth, .. } => *depth,
    };
    let line = div()
        .id(SharedString::from(format!("search-row-{index}")))
        .debug_selector(move || format!("search-row-{index}"))
        .flex()
        // A row as wide as its content passes for a highlight that stops
        // at the text — M8.1's trap, and the reason this is here.
        .w_full()
        .items_center()
        .gap(theme::space::CARD_GAP)
        .h(theme::space::SEARCH_ROW)
        .pr(theme::space::TREE_PADDING)
        .pl(theme::space::TREE_PADDING + theme::space::TREE_INDENT * depth as f32)
        .text_size(theme::size::META)
        // The Explorer's row look — decision 54. No Git tint here.
        .map(|line| crate::view::tree::ground(line, theme, None, lit, theme.surface.app));

    match row {
        Row::Directory {
            label,
            path,
            hits: n,
            ..
        } => {
            let shut = folded.contains(path);
            let at = path.clone();
            line.child(chevron(shut, theme))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.secondary))
                        .child(label.clone()),
                )
                .child(count(*n, theme))
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.toggle_search_fold(at.clone(), cx);
                }))
                .into_any_element()
        }
        Row::File {
            name,
            path,
            hits: n,
            ..
        } => {
            let shut = folded.contains(path);
            let at = path.clone();
            line.child(chevron(shut, theme))
                .child(
                    crate::icons::icon(&crate::icons::for_file(
                        name,
                        crate::icons::Palette::from(theme.kind),
                    ))
                    .flex_none(),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.primary))
                        .child(name.clone()),
                )
                .child(count(*n, theme))
                .on_click(
                    cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                        cx.stop_propagation();
                        root.search_file_click(at.clone(), event.modifiers(), window, cx);
                    }),
                )
                .into_any_element()
        }
        Row::Hit { at, .. } => {
            let Some(hit) = hits.get(*at) else {
                return line.into_any_element();
            };
            let which = *at;
            line.font_family(family.clone())
                .child(
                    div()
                        .flex_none()
                        .w(theme::space::SEARCH_LINE_NUMBER)
                        .text_color(rgb(theme.text.gutter))
                        .child(hit.line.to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_color(rgb(theme.text.secondary))
                        .child(painted(
                            hit,
                            result_identity(
                                root.active_project,
                                crate::Origin::Project,
                                hit,
                                root.search_state().map_or(0, |state| state.walk.answered()),
                            ),
                            theme,
                        )),
                )
                .on_click(cx.listener(move |root, _event, window, cx| {
                    cx.stop_propagation();
                    root.clear_search_file_selection(cx);
                    root.open_search_hit(which, window, cx);
                }))
                .into_any_element()
        }
    }
}

/// The one line of a refusal that a toolbar can hold.
///
/// **The regex crate says four lines**: the header, the pattern it built,
/// a caret under the place, and the sentence. All four in a 32-pixel
/// toolbar is four lines drawn over the panel above it — seen in the
/// window. The sentence is the last of them and is the half a person
/// acts on; the whole diagram is the tooltip, because the caret is what
/// says *where*.
fn sentence(said: &str) -> String {
    said.lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .unwrap_or(said)
        .to_owned()
}

/// The fold marker: right when shut, down when open — the tree's own,
/// in the tree's own size.
fn chevron(shut: bool, theme: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .size(theme::space::CHEVRON)
        .child(crate::icons::chevron(!shut).text_color(rgb(theme.text.faint)))
        .into_any_element()
}

/// How many hits are under a row, at its right.
fn count(hits: usize, theme: crate::theme::Theme) -> gpui::AnyElement {
    div()
        .flex_none()
        .text_size(theme::size::EYEBROW)
        .text_color(rgb(theme.text.faint))
        .child(hits.to_string())
        .into_any_element()
}

/// The line, with what matched in bold and the primary colour — the
/// Find tool, where the usage is the name the person asked about rather
/// than a word they typed. M8.15.
pub(crate) fn painted_bold(
    hit: &crate::search::Hit,
    identity: SharedString,
    theme: crate::theme::Theme,
) -> gpui::AnyElement {
    source_hit(hit, identity, theme, true)
}

/// Search foregrounds and activation stay with the host; source hit testing is shared.
pub(crate) fn painted(
    hit: &crate::search::Hit,
    identity: SharedString,
    theme: crate::theme::Theme,
) -> gpui::AnyElement {
    source_hit(hit, identity, theme, false)
}
/// Identity comes from the owning results, never the displayed label or row index.
pub(crate) fn result_identity(
    project: Option<guild_session::ProjectId>,
    origin: impl std::fmt::Debug,
    hit: &crate::search::Hit,
    generation: u64,
) -> SharedString {
    format!(
        "{project:?}:{origin:?}:{:?}:{}:{generation}",
        hit.path, hit.line
    )
    .into()
}
fn source_hit(
    hit: &crate::search::Hit,
    identity: SharedString,
    theme: crate::theme::Theme,
    bold: bool,
) -> gpui::AnyElement {
    let highlights = hit
        .ranges
        .iter()
        .filter(|r| {
            r.end <= hit.text.len()
                && hit.text.is_char_boundary(r.start)
                && hit.text.is_char_boundary(r.end)
        })
        .map(|range| {
            (
                range.clone(),
                gpui::HighlightStyle {
                    color: Some(
                        rgb(if bold {
                            theme.text.primary
                        } else {
                            theme.accent.brass
                        })
                        .into(),
                    ),
                    font_weight: bold.then_some(gpui::FontWeight::BOLD),
                    ..Default::default()
                },
            )
        })
        .collect();
    crate::source_snippet::SourceSnippet::new(
        SharedString::from(format!("source-hit-{}-{}", hit.path.display(), hit.line)),
        hit.text.clone(),
        highlights,
        theme,
    )
    .identity(identity)
    .into_any_element()
}

#[cfg(test)]
mod occurrence_tests;
