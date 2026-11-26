//! The Find tool — its searches as tabs in the dock head, the toolbar down
//! the left, the tree, the preview beside it — and the balloon over the
//! caret. M8.15: the Find tool window and its two balloons.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{
    Anchor as Corner, AnyElement, Context, SharedString, anchored, deferred, div, point, px,
    relative, rgb, uniform_list,
};

use crate::Root;
use crate::actions::keymap::Named;
use crate::i18n::Text;
use crate::icons;
use crate::root::usages::{FindTab, Mood};
use crate::search_tree;
use crate::theme;
use crate::usages::{Group, Row};

/// The narrowest body that draws the preview beside the list: a list and a
/// preview of 260 points each. Below it the list has the width alone.
const PREVIEW_FROM: f32 = 520.0;

/// The balloon's widest.
const BALLOON_WIDTH: f32 = 460.0;

impl Root {
    /// The searches, a pill each with its cross, for the dock head.
    pub(crate) fn render_find_tabs(&self, cx: &mut Context<Root>) -> Option<AnyElement> {
        let state = self.find_state()?;
        let theme = self.theme;
        let ui = self.ui();
        let active = state.active;
        let project = self.active_project?;
        let tabs = state.tabs.iter().enumerate().map(|(index, tab)| {
            let chosen = index == active;
            div()
                .id(("find-tab", index))
                .flex_none()
                .h(theme::space::HEAD_TAB)
                .flex()
                .items_center()
                .gap(px(4.0))
                .px(theme::space::CARD_GAP)
                .rounded(theme::radius::STATUS_BUTTON)
                .cursor_pointer()
                .text_size(theme::size::EYEBROW)
                .when(chosen, |t| {
                    t.bg(rgb(theme.surface.raised))
                        .text_color(rgb(theme.text.primary))
                })
                .when(!chosen, |t| {
                    t.text_color(rgb(theme.text.faint))
                        .hover(|t| t.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                })
                // **Not in capitals**, unlike Git's two faces: this is a
                // name from the code, and `Contract` in capitals is a
                // different word.
                .child(ui.usages_title(&tab.ask.name))
                .child(
                    div()
                        .id(("find-tab-close", index))
                        .flex_none()
                        .group("find-tab-close")
                        .child(
                            crate::icons::glyph("icons/close.svg", theme::size::EYEBROW)
                                .text_color(rgb(theme.text.faint))
                                .group_hover("find-tab-close", |cross| {
                                    cross.text_color(rgb(theme.text.primary))
                                }),
                        )
                        .on_click(cx.listener(move |root, _event, _window, cx| {
                            // The cross before the pill: a child fires
                            // first, and the pill would bring the closed
                            // tab to the front.
                            cx.stop_propagation();
                            root.close_find_tab_at(project, index, cx);
                        })),
                )
                .on_click(cx.listener(move |root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.select_find_tab(index, cx);
                }))
        });
        Some(
            div()
                .flex()
                .flex_1()
                .min_w_0()
                .overflow_hidden()
                .items_center()
                .gap(theme::space::STATUS_BUTTON_GAP)
                .children(tabs)
                .into_any_element(),
        )
    }

    /// The Find tool's body, or `None` while it has no search — the dock
    /// then draws its note.
    pub(crate) fn render_find(&self, cx: &mut Context<Root>) -> Option<AnyElement> {
        let state = self.find_state()?;
        let tab = state.tab()?;
        let theme = self.theme;
        let focus = state.focus.clone();
        let body_at = state.body_at.clone();
        let wide = body_at
            .get()
            .is_none_or(|bounds| f32::from(bounds.size.width) >= PREVIEW_FROM);
        let fraction = state.split.fraction();

        let list = self.render_find_list(tab, cx);
        let body = div()
            .relative()
            .flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .min_h_0()
                    .when(wide, |list| list.w(relative(fraction)))
                    .when(!wide, |list| list.flex_1())
                    .child(list),
            )
            .when(wide, |body| {
                body.child(self.render_find_grip(cx))
                    .child(self.render_find_preview())
            })
            .child(
                gpui::canvas(
                    move |bounds, _window, _cx| body_at.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                // Pinned to the corner: an absolute box with no inset sits
                // where it would have flowed — after the body, at the bottom.
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            );

        Some(
            div()
                .id("find")
                // **The focus goes on the tool, toolbar included** — the
                // Search tool's rule: an element that tracks focus takes it
                // on mouse-down, so the pressed element has to be the
                // focusable one.
                .track_focus(&focus)
                .key_context("find")
                .on_action(cx.listener(|root, _: &crate::actions::Copy, _window, cx| {
                    root.copy_find_files(cx);
                    cx.stop_propagation();
                }))
                .on_action(
                    cx.listener(|root, _: &crate::actions::SelectAll, _window, cx| {
                        root.select_all_find_files(cx);
                        cx.stop_propagation();
                    }),
                )
                .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                    if root.find_key(event, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .flex()
                .flex_1()
                .min_h_0()
                .size_full()
                .child(self.render_find_toolbar(cx))
                .child(
                    div()
                        .w(px(1.0))
                        .h_full()
                        .flex_none()
                        .bg(rgb(theme.surface.divider)),
                )
                .child(body)
                .into_any_element(),
        )
    }

    /// The column of buttons down the left of the Find tool.
    fn render_find_toolbar(&self, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        const TOOLS: [(&str, Text, Option<Named>); 5] = [
            ("ui/tool-refresh.svg", Text::FindRerun, None),
            (
                "ui/arrow-up.svg",
                Text::ActionPreviousUsage,
                Some(Named::PreviousUsage),
            ),
            (
                "ui/arrow-down.svg",
                Text::ActionNextUsage,
                Some(Named::NextUsage),
            ),
            ("ui/tool-expand.svg", Text::FindExpandAll, None),
            ("ui/tool-collapse.svg", Text::FindCollapseAll, None),
        ];
        div()
            .flex_none()
            .flex()
            .flex_col()
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .py(theme::space::STATUS_BUTTON_GAP)
            .px(theme::space::STATUS_BUTTON_GAP)
            .children(
                TOOLS
                    .into_iter()
                    .enumerate()
                    .map(|(index, (icon, label, action))| {
                        div()
                            .id(("find-tool", index))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(theme::space::HEAD_BUTTON)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .hover(|button| {
                                button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                            })
                            .child(icons::toolbar(icon).text_color(rgb(theme.text.tertiary)))
                            .hint_delay()
                            .tooltip(move |_window, app| {
                                app.new(|_cx| match action {
                                    Some(action) => crate::view::tooltip::Tooltip::for_action(
                                        ui.text(label),
                                        action,
                                        theme,
                                    ),
                                    None => {
                                        crate::view::tooltip::Tooltip::text(ui.text(label), theme)
                                    }
                                })
                                .into()
                            })
                            .on_click(cx.listener(move |root, _event, window, cx| {
                                cx.stop_propagation();
                                match index {
                                    0 => root.rerun_find(window, cx),
                                    1 => root.step_usage(false, window, cx),
                                    2 => root.step_usage(true, window, cx),
                                    3 => root.fold_every_find_row(false, cx),
                                    _ => root.fold_every_find_row(true, cx),
                                }
                            }))
                    }),
            )
            .into_any_element()
    }

    /// The tree of a search: the target, the groups, their files and lines.
    fn render_find_list(&self, tab: &FindTab, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        if tab.searching && tab.found.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text.faint))
                .child(ui.text(Text::UsagesSearching).to_owned())
                .into_any_element();
        }
        let rows = tab.rows();
        let hits = tab.found.hits.clone();
        let folds = tab.folds.clone();
        let selected = tab.selected;
        let files = tab.files.clone();
        let name = tab.ask.name.clone();
        let target = tab.target.as_ref().map(|target| target.shown.clone());
        let family = self.code_font.clone();
        let focus = self.find_state().map(|state| state.focus.clone());
        let count = rows.len();
        div()
            .id("find-results")
            .flex_1()
            .min_h_0()
            .child(
                uniform_list(
                    "find-rows",
                    count,
                    cx.processor(move |root, range: std::ops::Range<usize>, window, cx| {
                        // Blue while the tool has the keys — decision 54.
                        let focused = focus
                            .as_ref()
                            .is_some_and(|focus| crate::view::tree::keys_in(focus, window, cx));
                        range
                            .map(|index| {
                                let row = &rows[index];
                                // A file row lit by the keyboard or a fold, with
                                // no file selected, is the selection too.
                                let lit = crate::usages::file_of(row).map_or(
                                    selected == Some(index),
                                    |k| {
                                        files.contains(&k)
                                            || (files.selected.is_empty()
                                                && selected == Some(index))
                                    },
                                );
                                let shut = folds.is_folded(row);
                                find_row(
                                    row,
                                    index,
                                    crate::theme::Selected::of(lit, focused),
                                    shut,
                                    &hits,
                                    &name,
                                    target.as_deref(),
                                    &family,
                                    theme,
                                    ui,
                                    root,
                                    cx,
                                )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&tab.scroll)
                .size_full(),
            )
            .into_any_element()
    }

    /// The line between the list and the preview, dragged.
    fn render_find_grip(&self, cx: &mut Context<Root>) -> AnyElement {
        let theme = self.theme;
        div()
            .id("find-grip")
            .debug_selector(|| "find-grip".into())
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .w(px(9.0))
            .h_full()
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .child(div().w(px(1.0)).h_full().bg(rgb(theme.surface.divider)))
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|root, event: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    root.start_find_split(event.position, cx);
                }),
            )
            .into_any_element()
    }

    /// The file of the lit usage, around its line — the whole height of the
    /// pane, painted by `desktop::syntax` as the popups' preview is.
    fn render_find_preview(&self) -> AnyElement {
        let theme = self.theme;
        let pane = div()
            .id("find-preview")
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .bg(rgb(theme.surface.sunken))
            .py(px(4.0))
            .font_family(self.code_font.clone())
            .text_size(theme::size::META)
            .flex()
            .flex_col();
        let lit_is_library = self
            .find_state()
            .and_then(|state| state.tab())
            .and_then(|tab| {
                let row = tab.rows().get(tab.selected?)?.clone();
                let at = crate::usages::usage_of(&row)?;
                Some(at >= tab.found.project)
            })
            .unwrap_or(false);
        let shown = self.find_preview_shown().filter(|_| !lit_is_library);
        let Some(shown) = shown else {
            let note = lit_is_library.then(|| self.ui().text(Text::UsagesNoPreview).to_owned());
            return pane
                .items_center()
                .justify_center()
                .text_color(rgb(theme.text.faint))
                .children(note)
                .into_any_element();
        };
        let excerpt = &shown.excerpt;
        let rows = excerpt.lines.iter().enumerate().map(|(at, line)| {
            let number = excerpt.first + at as u32;
            let own = number == shown.asked.line;
            let marks = shown
                .asked
                .hit
                .as_ref()
                .filter(|_| own)
                .and_then(|hit| crate::preview::marks(line, hit))
                .unwrap_or(&[]);
            let spans = excerpt.spans.get(at).map(Vec::as_slice).unwrap_or(&[]);
            div()
                .flex()
                .flex_none()
                .w_full()
                .when(own, |row| row.bg(rgb(theme.surface.raised)))
                .child(
                    div()
                        .flex_none()
                        .w(theme::space::DIFF_NUMBERS)
                        .pr(px(8.0))
                        .flex()
                        .justify_end()
                        .text_color(rgb(theme.text.gutter))
                        .child((number + 1).to_string()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_color(rgb(theme.text.primary))
                        .child(
                            crate::source_snippet::SourceSnippet::new(
                                ("find-source", at),
                                line.clone(),
                                crate::view::preview::line_highlights(line, spans, marks, &theme),
                                theme,
                            )
                            .source(&shown.source, shown.offsets.get(at).copied().unwrap_or(0)),
                        ),
                )
        });
        pane.children(rows).into_any_element()
    }

    /// The balloon over the caret, when there is one.
    pub(crate) fn render_balloon(&self, _cx: &mut Context<Root>) -> Option<AnyElement> {
        let balloon = self.balloon.as_ref()?;
        let theme = self.theme;
        let (ground, edge, icon, tint) = match balloon.mood {
            Mood::Refusal => (
                theme.accent.rust_bg,
                theme.accent.rust,
                "ui/rail-problems.svg",
                theme.accent.rust,
            ),
            Mood::Report => (
                theme.surface.raised,
                theme.surface.divider,
                "ui/rail-search.svg",
                theme.text.tertiary,
            ),
        };
        let lines = balloon.lines.clone();
        let card = div()
            .max_w(px(BALLOON_WIDTH))
            .flex()
            .items_start()
            .gap(theme::space::CARD_GAP)
            .p(theme::space::CARD_PADDING)
            .bg(rgb(ground))
            .border_1()
            .border_color(rgb(edge))
            .rounded(theme::radius::CARD)
            .shadow_md()
            .text_size(theme::size::UI)
            .text_color(rgb(theme.text.primary))
            .child(
                div()
                    .flex_none()
                    .pt(px(2.0))
                    .child(icons::toolbar(icon).text_color(rgb(tint))),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .children(lines.into_iter().map(|line| div().child(line))),
            );
        // Above the name, and below when the name is too
        // near the top of the window for the card to fit.
        let at = balloon.at;
        let above = f32::from(at.origin.y) > 90.0;
        let (corner, place) = if above {
            (
                Corner::BottomLeft,
                point(at.origin.x, at.origin.y - px(4.0)),
            )
        } else {
            (
                Corner::TopLeft,
                point(at.origin.x, at.origin.y + at.size.height + px(4.0)),
            )
        };
        Some(
            deferred(
                anchored()
                    .position(place)
                    .anchor(corner)
                    .snap_to_window_with_margin(px(8.0))
                    .child(card),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

/// One row of a Find tab, whichever kind it is.
#[allow(clippy::too_many_arguments)]
fn find_row(
    row: &Row,
    index: usize,
    lit: crate::theme::Selected,
    shut: bool,
    hits: &[crate::search::Hit],
    name: &str,
    target: Option<&str>,
    family: &SharedString,
    theme: crate::theme::Theme,
    ui: crate::i18n::Ui,
    root: &Root,
    cx: &mut Context<Root>,
) -> AnyElement {
    let depth = match row {
        Row::Target | Row::Group { .. } => 0,
        Row::Item { row, .. } => match row {
            search_tree::Row::Directory { depth, .. }
            | search_tree::Row::File { depth, .. }
            | search_tree::Row::Hit { depth, .. } => depth + 1,
        },
    };
    let line = div()
        .id(SharedString::from(format!("find-row-{index}")))
        .flex()
        .w_full()
        .items_center()
        .gap(theme::space::CARD_GAP)
        .h(px(26.0))
        .pr(theme::space::TREE_PADDING)
        .pl(theme::space::TREE_PADDING + theme::space::TREE_INDENT * depth as f32)
        .text_size(theme::size::META)
        // The Explorer's row look — decision 54. No Git tint here.
        .map(|line| crate::view::tree::ground(line, theme, None, lit, theme.surface.app))
        // A click lights and previews; a second one opens, or folds.
        .on_click(
            cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                cx.stop_propagation();
                if event.click_count() >= 2
                    && !event.modifiers().platform
                    && !event.modifiers().shift
                {
                    root.activate_find_row(index, window, cx);
                } else {
                    root.find_row_click(index, event.modifiers(), window, cx);
                }
            }),
        );
    let chevron = |shut: bool| {
        div()
            .id(SharedString::from(format!("find-fold-{index}")))
            .flex_none()
            .size(theme::space::CHEVRON)
            .child(icons::chevron(!shut).text_color(rgb(theme.text.faint)))
            .on_click(cx.listener(move |root, _event, _window, cx| {
                cx.stop_propagation();
                root.toggle_find_row(index, cx);
            }))
    };
    let count = |n: usize| {
        div()
            .flex_none()
            .text_size(theme::size::EYEBROW)
            .text_color(rgb(theme.text.faint))
            .child(ui.usages_count(n))
    };

    match row {
        Row::Target => line
            .child(
                div()
                    .flex_none()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text.primary))
                    .child(name.to_owned()),
            )
            .children(target.map(|target| {
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(rgb(theme.text.faint))
                    .child(target.to_owned())
            }))
            .into_any_element(),
        Row::Group { group, count: n } => line
            .child(chevron(shut))
            .child(
                div()
                    .flex_none()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(theme.text.primary))
                    .child(
                        ui.text(match group {
                            Group::Project => Text::UsagesInProjectFiles,
                            Group::Libraries => Text::UsagesInLibraries,
                        })
                        .to_owned(),
                    ),
            )
            .child(count(*n))
            .into_any_element(),
        Row::Item { row, group } => match row {
            search_tree::Row::Directory { label, hits: n, .. } => line
                .child(chevron(shut))
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.secondary))
                        .child(label.clone()),
                )
                .child(count(*n))
                .into_any_element(),
            search_tree::Row::File { name, hits: n, .. } => line
                .child(chevron(shut))
                .child(
                    icons::icon(&icons::for_file(name, icons::Palette::from(theme.kind)))
                        .flex_none(),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.primary))
                        .child(name.clone()),
                )
                .child(count(*n))
                .into_any_element(),
            search_tree::Row::Hit { at, .. } => {
                let Some(hit) = hits.get(*at) else {
                    return line.into_any_element();
                };
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
                            .child(crate::view::search::painted_bold(
                                hit,
                                {
                                    let tab = root.find_state().and_then(|state| state.tab());
                                    let uri = tab
                                        .and_then(|tab| tab.found.links.get(*at))
                                        .map(|link| &link.target_uri);
                                    crate::view::search::result_identity(
                                        root.active_project,
                                        (group, uri),
                                        hit,
                                        tab.map_or(0, |tab| tab.serial),
                                    )
                                },
                                theme,
                            )),
                    )
                    .into_any_element()
            }
        },
    }
}

#[cfg(test)]
mod occurrence_tests;
