//! The sessions popup over the status bar's counters, keeping the generic
//! menu entries for introspection.
//!
//! The historical list the status bar's counter opened — M12.2 task 5, a menu of the
//! engine since M12.7 task 4.
//!
//! **A group per kind, a choice per agent, a check on the front.** The
//! model stays [`crate::agents`]: `groups` is still where the kinds are
//! sorted and an agent's own row built. What is here is the turn from
//! that list onto the engine's `Entry`s, at the moment the counter is
//! pressed — `Target::Agents` carries the groups and the front session
//! rather than reading them live, the tab strip's own pattern for a menu
//! whose rows are the window's data rather than one fixed set.
//!
//! **The place is the row's `detail`** — fix round 1. It rode in the
//! label after an en dash until then, `name — workspace · project`: two
//! agents can share a name, and a check drawn beside a fused string reads
//! worse than one beside a plain name with its place faint underneath —
//! the same reading the project popup's folder and session count need.

use guild_session::SessionId;

use super::{Choice, Deed, Entry, Label, Mark};
use crate::agents::Agent;
use crate::i18n::Text;

/// The counter's list, from the agents open in the window and the
/// session behind the tab in front — `None` when the front is not an
/// agent's tab, or there is none.
///
/// A window with nothing open is a single note, the empty list's own
/// words, rather than a heading with nothing under it.
pub fn agents(groups: &[crate::agents::Group], front: Option<SessionId>) -> Vec<Entry> {
    if groups.is_empty() {
        return vec![Entry::Note(Label::Text(Text::AgentsNone))];
    }
    let mut entries = Vec::new();
    for group in groups {
        entries.push(Entry::Heading(Label::Text(kind_text(group.kind))));
        for agent in &group.rows {
            let place = agent.workspace.as_ref().map_or_else(
                || agent.project.clone(),
                |workspace| format!("{workspace} · {}", agent.project),
            );
            entries.push(Entry::Choice(Choice {
                label: Label::Named(agent.name.clone()),
                mark: Mark::Status(agent.session),
                deed: Deed::ShowSession(agent.session),
                check: Some(Some(agent.session) == front),
                enabled: true,
                detail: Some(Label::Named(place)),
                trailing: None,
            }));
        }
    }
    entries
}

/// The kind's own name, in the translation table.
pub(crate) fn kind_text(kind: Agent) -> Text {
    match kind {
        Agent::ClaudeCode => Text::AgentClaudeCode,
        Agent::OpenCode => Text::AgentOpenCode,
        Agent::Codex => Text::AgentCodex,
    }
}

use crate::{Root, theme};
use gpui::{Context, Window, div, prelude::*, px, rgb};
use guild_session::status::Status;

/// The word a state is said in beside its mark, the mark's colour.
pub(crate) fn state_word(status: Status) -> Text {
    match status {
        Status::Waiting(_) => Text::SessionStateWaiting,
        Status::Failed => Text::SessionStateFailed,
        Status::Working => Text::SessionStateWorking,
        Status::Done => Text::SessionStateDone,
        Status::Idle => Text::SessionStateIdle,
    }
}

/// The colour of that word: the mark's own.
pub(crate) fn state_colour(status: Status, palette: theme::Theme) -> u32 {
    match status {
        Status::Working => palette.text.secondary,
        status => crate::view::activity::color(status, palette),
    }
}

/// A row's state mark; an idle one is a muted dot rather than nothing, so
/// its word has a mark to sit beside.
pub(crate) fn state_mark(status: Status, palette: theme::Theme) -> gpui::AnyElement {
    crate::view::activity::mark(status, palette, true, status == Status::Idle)
}

impl Root {
    pub(super) fn agents_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = self.context_menu.as_ref() else {
            return false;
        };
        let selected = open.agent_selected;
        let ids = self.popup_sessions();
        match event.keystroke.key.as_str() {
            "escape" => self.close_context_menu(window, cx),
            "up" | "down" => {
                let down = event.keystroke.key == "down";
                let at = selected.and_then(|id| ids.iter().position(|row| *row == id));
                let next = if ids.is_empty() {
                    None
                } else {
                    Some(match at {
                        Some(at) if down => (at + 1) % ids.len(),
                        Some(at) => (at + ids.len() - 1) % ids.len(),
                        None if down => 0,
                        None => ids.len() - 1,
                    })
                };
                if let Some(open) = self.context_menu.as_mut() {
                    open.agent_selected = next.map(|at| ids[at]);
                    if let Some(next) = next {
                        open.scroll.scroll_to_item(next);
                    }
                }
            }
            "enter" => {
                if let Some(id) = selected.filter(|id| ids.contains(id)) {
                    self.take_deed(Deed::ShowSession(id), window, cx);
                }
            }
            // Swallowed: no text may reach the terminal behind the popup.
            _ => {}
        }
        cx.notify();
        true
    }

    pub(super) fn render_agents_card(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let open = self.context_menu.as_ref().expect("agents menu");
        let selected = open.agent_selected;
        let rows = self.open_agent_rows();
        let ids = self.popup_sessions();
        let palette = self.theme;
        let ui = self.ui();
        let width = self
            .rem_px(theme::space::BRANCH_POPUP)
            .min((f32::from(self.viewport.width) - 16.).max(0.));
        let list = div()
            .id("session-navigator-rows")
            .flex()
            .flex_col()
            .min_h_0()
            .max_h(theme::space::BRANCH_LIST)
            .overflow_y_scroll()
            .track_scroll(&open.scroll)
            .when(ids.is_empty(), |list| {
                list.child(
                    div()
                        .p(theme::space::PANEL_PADDING)
                        .text_color(rgb(palette.text.secondary))
                        .child(ui.text(Text::AgentsNone)),
                )
            })
            .children(ids.iter().filter_map(|id| {
                let agent = rows.iter().find(|row| row.session == *id)?;
                Some(self.render_agent_row(agent, selected == Some(*id), cx))
            }));
        let footer = div()
            .flex()
            .flex_col()
            .flex_none()
            .border_t_1()
            .border_color(rgb(palette.surface.divider))
            .mx(theme::space::CARD_GAP)
            .child(
                div()
                    .debug_selector(|| "session-navigator-keys".into())
                    .py(theme::space::CARD_GAP)
                    .text_size(theme::size::EYEBROW)
                    .text_color(rgb(palette.text.tertiary))
                    .child(ui.text(Text::SessionsKeys)),
            );
        crate::view::popup::card(palette)
            .id("session-navigator")
            .debug_selector(|| "session-navigator".into())
            .w(px(width))
            .min_w_0()
            // Cap the whole card, including scaled padding and its fixed footer.
            // Flex shrink gives the scrollable rows the remaining actual space.
            .max_h((self.counter_at.get().top() - px(super::MARGIN)).max(px(0.)))
            .overflow_hidden()
            .child(list)
            .child(footer)
            .into_any_element()
    }

    fn render_agent_row(
        &self,
        agent: &crate::agents::Open,
        selected: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let palette = self.theme;
        let ui = self.ui();
        let session = agent.session;
        let current = self.front_session() == Some(session);
        let place = self.session_place(session);
        let project_name = if place.project.is_empty() {
            agent.project.clone()
        } else {
            place.project.clone()
        };
        let color = crate::view::column::tile_fill(palette, &project_name);
        let tile = div()
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::ROW)
            .bg(rgb(color))
            .text_color(rgb(palette.surface.app))
            .when_some(place.icon, |tile, icon| {
                tile.child(
                    gpui::svg()
                        .path(icon)
                        .size(theme::space::ICON)
                        .text_color(rgb(palette.surface.app)),
                )
            })
            .when(place.icon.is_none(), |tile| {
                tile.child(crate::view::column::tile_letter(&project_name))
            })
            .child(
                div()
                    .absolute()
                    .right(px(-4.))
                    .bottom(px(-3.))
                    .rounded_full()
                    .bg(rgb(palette.surface.card))
                    .child(crate::view::kind::harness_mark(
                        agent.kind.mark(),
                        palette,
                        theme::space::ICON,
                        crate::view::kind::Shade::Full,
                    )),
            );
        let status = self.status_of(session);
        let age = crate::agents::relative_time(
            self.session_last(session),
            std::time::SystemTime::now(),
            ui,
        );
        // Project · branch — and the workspace ahead of them when the desk
        // has more than one, since the popup lists every workspace's.
        let mut where_ = project_name.clone();
        if let Some(branch) = &place.branch {
            where_ = format!("{where_} · {branch}");
        }
        if let Some(workspace) = &agent.workspace {
            where_ = format!("{workspace} · {where_}");
        }
        let state = div()
            .flex()
            .flex_col()
            .flex_none()
            .items_end()
            .gap(theme::space::CARD_GAP)
            .child(
                div()
                    .debug_selector(move || format!("session-row-state-{session}"))
                    .flex()
                    .items_center()
                    .gap(theme::activity::STATE_GAP)
                    .text_size(theme::size::META)
                    .text_color(rgb(state_colour(status, palette)))
                    .child(state_mark(status, palette))
                    .child(ui.text(state_word(status))),
            )
            .child(
                div()
                    .text_size(theme::size::META)
                    .text_color(rgb(palette.text.tertiary))
                    .child(age),
            );
        div()
            .id(format!("session-row-{session}"))
            .debug_selector(move || format!("session-row-{session}"))
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::PANEL_PADDING)
            .mx(theme::space::CARD_GAP)
            .p(theme::space::CARD_GAP)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .when(selected, |r| r.bg(rgb(palette.surface.raised)))
            .child(tile)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .gap(theme::space::CARD_GAP)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::space::CARD_GAP)
                            .child(div().min_w_0().truncate().child(agent.name.clone()))
                            .when(current, |r| {
                                r.child(
                                    crate::icons::glyph("ui/check.svg", theme::size::UI)
                                        .text_color(rgb(palette.accent.patina)),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(palette.text.secondary))
                            .truncate()
                            .child(where_),
                    ),
            )
            .child(state)
            .on_mouse_move(
                cx.listener(move |root, event: &gpui::MouseMoveEvent, _, cx| {
                    if let Some(open) = root.context_menu.as_mut()
                        && super::moved_since_open(event.position, open.opened_at)
                        && open.agent_selected != Some(session)
                    {
                        open.agent_selected = Some(session);
                        cx.notify();
                    }
                }),
            )
            .on_click(cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                root.take_deed(Deed::ShowSession(session), window, cx);
            }))
            .into_any_element()
    }
}
