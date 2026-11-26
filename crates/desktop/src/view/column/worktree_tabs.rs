//! A worktree's tabs, listed under its row in the column, in Guild's own
//! parts.
//!
//! Only the tabs that hold a session — an agent or a shell — in their
//! strips' order, each wearing its harness's mark, its strip's name and its
//! strip's status mark. A thin connector joins them to the worktree above.
//! A click brings the tab forward by the agents list's own path; under the
//! pointer the status gives its slot to a `×` that closes the tab as the
//! strip's cross does, asking first when that would interrupt an agent.
//! The worktree row's chevron collapses the list, per workspace; collapsed,
//! the row says how many tabs it holds, and its own activity mark — which
//! already sums its sessions — says whether one of them wants the person.

use gpui::prelude::*;
use gpui::{Context, SharedString, div, px, rgb};
use guild_session::WorkspaceId;

use crate::column::Row;
use crate::root::column::ColumnTab;
use crate::{Root, theme};

/// Where the connector runs, from a worktree row's left edge: under the
/// middle of the row's `⎇` — its border, its padding, the activity slot,
/// the glyph's margin and half its width.
const CONNECTOR_X: f32 = 1.0 + 6.0 + 14.0 + 2.0 + 7.0;
/// How far the elbow reaches before a tab's row begins.
const ELBOW: f32 = 8.0;
/// A tab row's pitch.
const ROW: f32 = 28.0;
/// The height of a lit tab row's ground, centred in its pitch — so two
/// lit rows never touch.
const FILL: f32 = 25.0;
/// Air between the worktree's card and its first tab row, so the card's
/// outline and a lit row's ground stay three apart; the
/// connector crosses it.
const GAP: f32 = 3.0 - (ROW - FILL) / 2.0;

impl Root {
    /// The opened worktree's tabs as the column lists them; nothing for an
    /// unopened worktree, which has no tabs.
    pub(super) fn worktree_tab_rows(
        &self,
        workspace: WorkspaceId,
        row: &Row,
        cx: &Context<Root>,
    ) -> Vec<ColumnTab> {
        match row {
            Row::Opened { project, .. } => self.column_tabs(workspace, *project, cx),
            Row::Unopened { .. } => Vec::new(),
        }
    }

    /// A worktree row followed by its list of tabs, unless it has none or
    /// the list is collapsed.
    pub(super) fn with_worktree_tabs(
        &self,
        workspace: WorkspaceId,
        row: &Row,
        element: impl IntoElement,
        cx: &mut Context<Root>,
    ) -> gpui::Div {
        let tabs = self.worktree_tab_rows(workspace, row, cx);
        let folded = self
            .column_row_path(row)
            .is_some_and(|path| self.desk.worktree_tabs_folded(workspace, &path));
        let count = tabs.len();
        let line = rgb(self.theme.surface.divider);
        div()
            .flex()
            .flex_col()
            .child(element)
            .when(!folded && count > 0, |list| {
                list.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .pt(px(GAP))
                        // One line from right under the card's `⎇` down to
                        // the last row's middle, where its elbow turns in.
                        .child(
                            div()
                                .absolute()
                                .left(px(CONNECTOR_X))
                                .top_0()
                                .w(px(1.0))
                                .h(px(GAP + ROW * (count as f32 - 0.5)))
                                .bg(line),
                        )
                        .children(
                            tabs.into_iter()
                                .map(|tab| self.render_worktree_tab(workspace, tab, cx)),
                        ),
                )
            })
    }

    /// The worktree row's own control for its list: the number of tabs
    /// while collapsed, and a chevron — always while collapsed, under the
    /// pointer while expanded. Nothing for a worktree without tabs.
    ///
    /// One button, as the head's `⋯` and `+` are: their target, radius,
    /// hand and hover. It lights over `ground` — the row it is drawn in,
    /// which the pointer is lighting too — since the column's hover over
    /// the panel is that row's own fill and showed nothing.
    pub(super) fn worktree_tabs_toggle(
        &self,
        workspace: WorkspaceId,
        path: Option<std::path::PathBuf>,
        count: usize,
        hovered: bool,
        ground: gpui::Rgba,
        cx: &mut Context<Root>,
    ) -> Option<gpui::Stateful<gpui::Div>> {
        let path = path.filter(|_| count > 0)?;
        let theme = self.theme;
        let folded = self.desk.worktree_tabs_folded(workspace, &path);
        let key = path.display().to_string();
        Some(
            div()
                .id(SharedString::from(format!("worktree-tabs-toggle-{key}")))
                .debug_selector(move || format!("worktree-tabs-toggle-{key}"))
                .role(gpui::Role::Button)
                .aria_label(
                    self.ui()
                        .text(if folded {
                            crate::i18n::Text::WorktreeTabsExpand
                        } else {
                            crate::i18n::Text::WorktreeTabsCollapse
                        })
                        .to_owned(),
                )
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .gap(px(3.0))
                .h(theme::column::ACTION_TARGET)
                .min_w(theme::column::ACTION_TARGET)
                .when(folded, |button| button.px(px(4.0)))
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .text_size(theme::size::META)
                .text_color(rgb(theme.text.faint))
                .hover(move |button| button.bg(crate::theme::control_hover_over(theme, ground)))
                .when(!folded && !hovered, |button| button.invisible())
                .capture_any_mouse_down(|event, window, _| {
                    if event.button == gpui::MouseButton::Left {
                        window.prevent_default();
                    }
                })
                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |root, _, _, cx| {
                    cx.stop_propagation();
                    root.toggle_worktree_tabs(workspace, &path, cx);
                }))
                .when(folded, |button| {
                    button.child(
                        div()
                            .debug_selector(|| "worktree-tabs-count".into())
                            .child(count.to_string()),
                    )
                })
                .child(
                    crate::icons::chevron(!folded)
                        .size(px(11.0))
                        .text_color(rgb(theme.text.muted)),
                ),
        )
    }

    fn render_worktree_tab(
        &self,
        workspace: WorkspaceId,
        tab: ColumnTab,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let id = tab.id;
        let session = tab.session;
        let hovered = self.hovered_column_tab == Some(id);
        let line = rgb(theme.surface.divider);
        let close_label = self.ui().text(crate::i18n::Text::MenuCloseTab);
        div()
            .id(SharedString::from(format!("column-tab-{id}")))
            .debug_selector(move || format!("column-tab-{id}"))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .h(px(ROW))
            .pl(px(CONNECTOR_X + ELBOW))
            // The elbow from the list's one line into this row's middle.
            .child(
                div()
                    .absolute()
                    .left(px(CONNECTOR_X))
                    .top(px(ROW / 2.0))
                    .w(px(ELBOW))
                    .h(px(1.0))
                    .bg(line),
            )
            .child(
                div()
                    .id(SharedString::from(format!("column-tab-card-{id}")))
                    .debug_selector(move || format!("column-tab-card-{id}"))
                    .flex()
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .gap(px(6.0))
                    .h(px(FILL))
                    .pl(px(6.0))
                    .pr(px(4.0))
                    // A flat list row, not a second card: no ground at
                    // rest and a small radius. In front it wears the
                    // column's selection — the selected card's own fill,
                    // without the card's outline — and under the pointer
                    // the column's hover, which is louder.
                    .rounded(theme::radius::WORKTREE_ACTION)
                    .cursor_pointer()
                    .on_hover(cx.listener(move |root, hovered: &bool, _, cx| {
                        let was = root.hovered_column_tab;
                        if *hovered {
                            root.hovered_column_tab = Some(id);
                        } else if root.hovered_column_tab == Some(id) {
                            root.hovered_column_tab = None;
                        }
                        if root.hovered_column_tab != was {
                            cx.notify();
                        }
                    }))
                    .when(tab.front, |card| {
                        card.bg(crate::theme::column_selected(theme))
                    })
                    .when(!tab.front && hovered, |card| {
                        card.bg(crate::theme::control_hover(theme, theme.surface.panel))
                    })
                    // The keys go where the tab's own path sends them, not
                    // to the column the press landed in.
                    .capture_any_mouse_down(|event, window, _| {
                        if event.button == gpui::MouseButton::Left {
                            window.prevent_default();
                        }
                    })
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        root.go_to_session_in_workspace(session, cx);
                    }))
                    .child(crate::view::kind::harness_mark(
                        tab.harness,
                        theme,
                        theme::space::WORKTREE_GLYPH,
                        crate::view::kind::Shade::Full,
                    ))
                    .child(
                        div()
                            .debug_selector(move || format!("column-tab-label-{id}"))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            // Secondary text at the column's secondary size,
                            // whether in front or not: the row's ground says
                            // which tab is in front. `text.secondary`, not
                            // `text.muted`, which is under `theme::LEGIBLE`
                            // on the panel at this size.
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            .child(tab.label),
                    )
                    // One slot for the status and the `×`, so the row does
                    // not move under the pointer.
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(theme::space::WORKTREE_ACTION)
                            .child(
                                div()
                                    .debug_selector(move || format!("column-tab-status-{id}"))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .when(hovered, |mark| mark.invisible())
                                    .child(crate::view::activity::mark(
                                        tab.status,
                                        theme,
                                        workspace == self.active_workspace,
                                        false,
                                    )),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("column-tab-close-{id}")))
                                    .debug_selector(move || format!("column-tab-close-{id}"))
                                    .role(gpui::Role::Button)
                                    .aria_label(close_label)
                                    .absolute()
                                    .top_0()
                                    .left_0()
                                    .size_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::radius::WORKTREE_ACTION)
                                    .group("column-tab-close")
                                    .when(!hovered, |close| close.invisible())
                                    .hover(|close| close.bg(rgb(theme.accent.rust_bg)))
                                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation()
                                    })
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        cx.stop_propagation();
                                        root.close_tab_asking(id, window, cx);
                                    }))
                                    .child(
                                        crate::icons::glyph("icons/close.svg", theme::size::META)
                                            .text_color(rgb(theme.text.faint))
                                            .group_hover("column-tab-close", |cross| {
                                                cross.text_color(rgb(theme.accent.rust))
                                            }),
                                    ),
                            ),
                    ),
            )
    }
}
