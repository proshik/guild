//! The column's Sessions mode, drawn: a workspace's agent sessions under
//! their urgency's heading in a list that scrolls, and at the column's foot
//! — pinned, never scrolled — the collapsed sections' headings stacked
//! above a hairline and Settled, whose rows are muted and scroll inside at
//! most half the page. What is listed where is `session_list::page`; the
//! measures are the worktree row's, so the two modes read as one column.

use gpui::{Context, IntoElement, SharedString, div, prelude::*, px, relative, rgb};

use crate::Root;
use crate::i18n::Text;
use crate::session_list::{Key, Line, Urgency};
use crate::theme;

/// The air around the foot's parts — the stack, Settled and its heading:
/// half the card gap, so it follows the interface's size as the gap does.
const FOOT_GAP: gpui::Rems = gpui::Rems(theme::space::CARD_GAP.0 / 2.0);

/// What a section's heading says.
fn section_text(urgency: Urgency) -> Text {
    match urgency {
        Urgency::Waiting => Text::SessionsWaitingForYou,
        Urgency::Working => Text::SessionsWorking,
        Urgency::Done => Text::SessionsDone,
        Urgency::Idle => Text::SessionsIdle,
    }
}

/// A row's element id and debug name: an open session by its id, a
/// closed one by its harness's.
fn row_name(key: &Key) -> String {
    match key {
        Key::Open(id) => format!("column-session-{id}"),
        Key::Closed { id, .. } => format!("column-settled-row-{id}"),
    }
}

impl Root {
    /// `workspace`'s Sessions page, in place of its projects: the list of
    /// expanded sections takes all the room above the foot; the foot holds
    /// the collapsed sections, the hairline and Settled.
    pub(super) fn render_session_list(
        &self,
        workspace: guild_session::WorkspaceId,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let page = self.session_page(workspace);
        let chosen = (workspace == self.active_workspace)
            .then(|| self.session_chosen())
            .flatten();
        let palette = self.theme;
        let active = workspace == self.active_workspace;
        let nothing = page.list.is_empty() && page.collapsed.is_empty() && page.settled.is_none();
        let list = div()
            .id(SharedString::from(format!("column-sessions-{workspace}")))
            .debug_selector(|| "column-sessions".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .px(theme::column::LIST_INSET)
            .pt(px(4.0))
            .pb(theme::space::CARD_GAP)
            .overflow_y_scroll()
            .when(active, |list| list.track_scroll(&self.session_scroll))
            .when(nothing, |list| {
                list.child(
                    div()
                        .pl(px(10.0))
                        .pt(px(4.0))
                        .text_size(theme::size::META)
                        .text_color(rgb(palette.text.faint))
                        .child(self.ui().text(Text::AgentsNone).to_owned()),
                )
            })
            .children(page.list.iter().map(|line| match line {
                Line::Section(urgency, count) => {
                    self.render_session_section(workspace, *urgency, *count, true, cx)
                }
                Line::Row(key) => {
                    self.render_session_row(workspace, key, chosen.as_ref(), false, cx)
                }
            }));
        // The collapsed sections, pinned under the list in urgency order: a
        // click puts one back in its place above.
        let stack = (!page.collapsed.is_empty()).then(|| {
            div()
                .debug_selector(|| "column-sessions-stack".into())
                .flex()
                .flex_col()
                .flex_none()
                .px(theme::column::LIST_INSET)
                .py(FOOT_GAP)
                .children(page.collapsed.iter().map(|(urgency, count)| {
                    self.render_session_section(workspace, *urgency, *count, false, cx)
                }))
        });
        // Settled, apart from every status section: under a hairline at the
        // very foot, at most half the page, its rows scrolling inside.
        let settled = page.settled.map(|zone| {
            div()
                .debug_selector(|| "column-settled".into())
                .flex()
                .flex_col()
                .flex_none()
                .max_h(relative(0.5))
                .px(theme::column::LIST_INSET)
                .pb(FOOT_GAP)
                .child(
                    div()
                        .flex_none()
                        .border_t_1()
                        .border_color(rgb(palette.surface.divider)),
                )
                .child(self.render_settled_head(workspace, zone.count, zone.open, cx))
                .when(zone.open, |settled| {
                    settled.child(
                        div()
                            .id(SharedString::from(format!(
                                "column-settled-rows-{workspace}"
                            )))
                            .debug_selector(|| "column-settled-rows".into())
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .overflow_y_scroll()
                            .when(active, |rows| rows.track_scroll(&self.settled_scroll))
                            .children(zone.rows.iter().map(|key| {
                                self.render_session_row(workspace, key, chosen.as_ref(), true, cx)
                            })),
                    )
                })
        });
        div()
            .debug_selector(|| "column-sessions-page".into())
            .relative()
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .child(list)
            .children(stack)
            .children(settled)
            .into_any_element()
    }

    /// A status section's heading — Settled's anatomy, so every heading
    /// stands on one line: a chevron that says whether it is expanded, its
    /// name and its count. A click collapses it into the stack at the foot,
    /// or expands it back into its place.
    fn render_session_section(
        &self,
        workspace: guild_session::WorkspaceId,
        urgency: Urgency,
        count: usize,
        expanded: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        self.fold_head(
            workspace,
            format!("column-sessions-section-{urgency:?}"),
            expanded,
            section_text(urgency),
            count,
        )
        .on_click(cx.listener(move |root, _, _, cx| {
            cx.stop_propagation();
            root.toggle_session_section(urgency, cx);
        }))
        .into_any_element()
    }

    /// Settled's heading, a status section's anatomy. A click opens or
    /// folds it.
    fn render_settled_head(
        &self,
        workspace: guild_session::WorkspaceId,
        count: usize,
        open: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        self.fold_head(
            workspace,
            "column-settled-head".into(),
            open,
            Text::SessionsSettled,
            count,
        )
        .mt(FOOT_GAP)
        .on_click(cx.listener(|root, _, _, cx| {
            cx.stop_propagation();
            root.toggle_settled(cx);
        }))
        .into_any_element()
    }

    /// The one heading every fold of the page draws: a chevron, a name, a
    /// count, as faint as the column's *Pinned* label. Its element is the
    /// workspace's own: a swipe draws two pages, and two headings of one id
    /// would share one press.
    fn fold_head(
        &self,
        workspace: guild_session::WorkspaceId,
        selector: String,
        open: bool,
        name: Text,
        count: usize,
    ) -> gpui::Stateful<gpui::Div> {
        let palette = self.theme;
        div()
            .id(SharedString::from(format!("{selector}-{workspace}")))
            .debug_selector(move || selector.clone())
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::activity::COUNT_GAP)
            .h(px(24.0))
            .pl(px(6.0))
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .text_size(theme::size::META)
            .text_color(rgb(palette.text.faint))
            .hover(|head| head.bg(crate::theme::control_hover(palette, palette.surface.panel)))
            .child(
                crate::icons::glyph(
                    if open {
                        "ui/chevron-down.svg"
                    } else {
                        "ui/chevron-right.svg"
                    },
                    theme::space::CHEVRON,
                )
                .text_color(rgb(palette.text.muted)),
            )
            .child(self.ui().text(name).to_owned())
            .child(count.to_string())
    }

    /// One session: its state mark, its project and how long ago it last
    /// did something; its name; its branch with what the branch changed,
    /// and its agent's logo — three lines of the worktree row's measures.
    /// A Settled row is muted: a rank fainter, its tile and logo faint.
    fn render_session_row(
        &self,
        workspace: guild_session::WorkspaceId,
        key: &Key,
        chosen: Option<&Key>,
        settled: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let palette = self.theme;
        let ui = self.ui();
        let line_height = px(20. * self.settings.ui_font_size / crate::scale::BASE_UI_FONT_SIZE);
        let selected = chosen == Some(key);
        let (place, agent, status, last) = match key {
            Key::Open(id) => {
                // The agent the tab holds, as the popup and the filter
                // see it — a shell running Codex is Codex.
                let agent = self
                    .tab_of_session(*id)
                    .and_then(|tab| self.tabs.iter().find(|t| t.id == tab))
                    .and_then(|tab| {
                        tab.bodies()
                            .find(|body| body.session() == Some(*id))
                            .and_then(|body| self.body_agent(body))
                    });
                (
                    self.session_place(*id),
                    agent.unwrap_or(crate::agents::Agent::ClaudeCode),
                    Some(self.status_of(*id)),
                    self.session_last(*id),
                )
            }
            Key::Closed { harness, .. } => {
                let found = self.settled_past(key);
                (
                    found
                        .map(|p| self.project_place(p.project, workspace))
                        .unwrap_or_default(),
                    crate::agents::Agent::from_harness(*harness),
                    None,
                    found.map(|p| p.past.touched),
                )
            }
        };
        let age = crate::agents::relative_time(last, std::time::SystemTime::now(), ui);
        let name = self.session_row_name(key);
        let slot = div()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .w(theme::activity::SLOT_WIDTH)
            .h(theme::activity::SLOT_HEIGHT)
            .child(match status {
                Some(status) => crate::view::menu::agents::state_mark(status, palette),
                // A conversation no tab holds: it is in the history.
                None => crate::icons::glyph("ui/workspace-history.svg", theme::size::META)
                    .text_color(rgb(palette.text.faint))
                    .into_any_element(),
            });
        let tile = div()
            .flex_none()
            .when(settled, |tile| tile.opacity(crate::view::kind::FAINT))
            .child(match place.icon {
                Some(icon) => div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(theme::space::ICON)
                    .rounded(theme::radius::ROW)
                    .bg(rgb(super::tile_fill(palette, &place.project)))
                    .child(
                        gpui::svg()
                            .path(icon)
                            .size(theme::size::META)
                            .text_color(rgb(palette.surface.app)),
                    ),
                None => {
                    super::letter_tile(palette, &place.project, self.rem_px(theme::space::ICON))
                }
            });
        let indent = theme::activity::SLOT_WIDTH + px(6.0);
        let changed =
            place
                .lines
                .filter(|(added, removed)| *added > 0 || *removed > 0)
                .map(|(added, removed)| {
                    div()
                        .flex()
                        .flex_none()
                        .gap(px(5.0))
                        .font_family(self.code_font.clone())
                        .text_size(theme::size::META)
                        .when(added > 0, |numbers| {
                            numbers.child(
                                div()
                                    .text_color(rgb(palette.accent.green))
                                    .child(format!("+{}", crate::column::compact_lines(added))),
                            )
                        })
                        .when(removed > 0, |numbers| {
                            numbers.child(div().text_color(rgb(palette.accent.rust)).child(
                                format!("\u{2212}{}", crate::column::compact_lines(removed)),
                            ))
                        })
                });
        let clicked = key.clone();
        let menu_key = key.clone();
        let selector = row_name(key);
        let closed = status.is_none();
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .flex()
            .flex_col()
            .flex_none()
            .pl(px(6.0))
            .pr(px(10.0))
            .pt(px(4.0))
            .pb(px(4.0))
            .rounded(theme::radius::WORKTREE_ROW)
            // The outline is always there and only coloured when chosen,
            // as the worktree row's: the row does not move by a point.
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_pointer()
            .when(selected, |row| {
                row.bg(crate::theme::column_selected(palette))
                    .border_color(rgb(palette.text.gutter))
            })
            .when(!selected, |row| {
                row.hover(|row| row.bg(crate::theme::control_hover(palette, palette.surface.panel)))
            })
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .h(line_height)
                    .min_w_0()
                    .child(slot)
                    .child(tile)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(theme::size::META)
                            .text_color(rgb(if settled {
                                palette.text.tertiary
                            } else {
                                palette.text.secondary
                            }))
                            .child(place.project.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(theme::size::META)
                            .text_color(rgb(palette.text.faint))
                            .child(age),
                    ),
            )
            .child(
                div()
                    .h(line_height)
                    .pl(indent)
                    .min_w_0()
                    .truncate()
                    .text_size(theme::size::BODY)
                    .text_color(rgb(if closed || settled {
                        palette.text.tertiary
                    } else if selected {
                        palette.text.primary
                    } else {
                        palette.text.secondary
                    }))
                    .child(name),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .h(line_height)
                    .pl(indent)
                    .min_w_0()
                    .child(
                        crate::icons::glyph("ui/workspace-git-branch.svg", theme::size::META)
                            .flex_none()
                            .text_color(rgb(palette.text.faint)),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(theme::size::META)
                            .text_color(rgb(if settled {
                                palette.text.faint
                            } else {
                                palette.text.tertiary
                            }))
                            .child(place.branch.clone().unwrap_or_else(|| "—".into())),
                    )
                    .when(place.primary, |line| {
                        line.child(
                            crate::icons::glyph("ui/star-filled.svg", theme::size::META)
                                .flex_none()
                                .text_color(rgb(palette.text.faint)),
                        )
                    })
                    .children(changed)
                    .child(div().flex_1())
                    .child(crate::view::kind::harness_mark(
                        agent.mark(),
                        palette,
                        theme::activity::RING,
                        if settled {
                            crate::view::kind::Shade::Disabled
                        } else {
                            crate::view::kind::Shade::Full
                        },
                    )),
            )
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |root, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    root.open_menu(
                        crate::view::menu::Target::SessionRow {
                            key: menu_key.clone(),
                        },
                        crate::view::menu::Anchor::Pointer(event.position),
                        window,
                        cx,
                    );
                }),
            )
            .on_click(cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                if workspace != root.active_workspace {
                    root.select_workspace(workspace, cx);
                }
                root.activate_session(clicked.clone(), window, cx);
            }))
            .into_any_element()
    }

    /// The heading's filter in Sessions mode: the sort button's look,
    /// opening the agent, state and project choices.
    pub(super) fn session_filter_button(
        &self,
        workspace: guild_session::WorkspaceId,
        cx: &mut Context<Root>,
    ) -> impl IntoElement {
        use gpui_component::{
            Sizable as _,
            button::{Button, ButtonVariants as _},
        };
        let anchor = std::rc::Rc::new(std::cell::Cell::new(None));
        let measured = anchor.clone();
        let filtered = !self.session_filter.is_empty();
        let button = Button::new("session-filter")
            .flex_none()
            .small()
            .min_w(theme::space::HEAD_BUTTON)
            .child(
                gpui_component::Icon::default()
                    .path("ui/project-sort.svg")
                    .size(theme::size::META),
            )
            .custom(
                crate::view::controls::ghost(self.theme, filtered, cx).foreground(
                    rgb(if filtered {
                        self.theme.text.primary
                    } else {
                        self.theme.text.faint
                    })
                    .into(),
                ),
            )
            .debug_selector(|| "session-filter".into())
            .tooltip(self.ui().text(Text::SessionFilter).to_owned())
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| measured.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .on_click(cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                if let Some(bounds) = anchor.get() {
                    root.open_menu(
                        crate::view::menu::Target::SessionFilter {
                            workspace,
                            projects: root.session_filter_projects(workspace),
                        },
                        crate::view::menu::Anchor::Below(bounds),
                        window,
                        cx,
                    );
                }
            }));
        div()
            .flex_none()
            .w(theme::column::ACTION_TARGET)
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .child(button)
    }
}
