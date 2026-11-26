//! The Output panel: a toolbar, a terminal, and what became of the run.
//!
//! Section 6.20 of the design system. The toolbar is the Log's — 32 high,
//! inside the panel, under the head — and the terminal below it is the
//! same one a tab has, which is decision 1 of M8.4: `cargo`, `clippy` and
//! `just` colour their output and redraw with carriage returns, and only
//! a pty makes them do it at all.
//!
//! **The state at the right is why the panel exists.** `just check 2>&1 |
//! tail` exits 0 whatever happened inside it, and this project has
//! already closed three commits on a check that read as green.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, MouseButton, div, rgb};

use crate::i18n::Text;
use crate::view::picker::{Card, Place};
use crate::{Root, icons, run, theme};

#[derive(Clone, Copy, PartialEq)]
pub(crate) struct TabFrame {
    project: guild_session::ProjectId,
    id: u64,
    index: usize,
    width: gpui::Pixels,
}

impl Root {
    /// The panel: the toolbar, and under it the run's terminal or the
    /// line that says there has not been one.
    pub(crate) fn render_run(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let ui = self.ui();
        let view = self.active_run().and_then(|run| run.view.clone());
        let target = self.active_project.and_then(|project| {
            self.runs
                .get(&project)
                .and_then(|tabs| tabs.active())
                .map(|entry| (project, entry.id))
        });

        Some(
            div()
                .flex()
                .flex_col()
                .size_full()
                .child(self.render_run_tabs(cx))
                .child(self.render_run_toolbar(cx))
                .child(match view {
                    // `min_h_0`, without which a flex item's minimum size
                    // is its own content and the terminal pushes the box
                    // open from the inside instead of filling it.
                    // The right button records the run's terminal and
                    // opens its menu, with no strokes — M5.12.
                    Some(view) => div()
                        .flex_1()
                        .min_h_0()
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |root, event: &gpui::MouseDownEvent, window, cx| {
                                if let Some((project, id)) = target {
                                    root.note_terminal_menu(
                                        crate::view::menu::TerminalAt::Run(project, id),
                                        cx,
                                    );
                                    root.open_context_menu(event.position, window, cx);
                                }
                            }),
                        )
                        .child(view)
                        .into_any_element(),
                    None => div()
                        .flex_1()
                        .min_h_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(theme::size::UI)
                        .text_color(rgb(theme.text.faint))
                        .child(
                            self.active_run()
                                .and_then(|run| match &run.state {
                                    run::State::Failed { reason } => Some(reason.clone()),
                                    _ => None,
                                })
                                .unwrap_or_else(|| ui.text(Text::RunNothingYet).to_owned()),
                        )
                        .into_any_element(),
                })
                .into_any_element(),
        )
    }

    fn render_run_tabs(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let project = self.active_project;
        let tabs = project.and_then(|p| self.runs.get(&p));
        let selected = tabs.and_then(|t| t.active()).map(|e| e.id);
        let reveal = tabs
            .and_then(|tabs| tabs.entries().iter().position(|e| Some(e.id) == selected))
            .map(|index| (project.unwrap(), selected.unwrap(), index));
        let frame = self.run_tab_frame.clone();
        let scroll = self.run_tab_scroll.clone();
        div()
            .id("run-tabs")
            .debug_selector(|| "run-tab-strip".into())
            .flex()
            .flex_none()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.run_tab_scroll)
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, window, _| {
                        // Closing an earlier tab changes position without changing
                        // the selected identity. Reveal that new layout too.
                        let next = reveal.map(|(project, id, index)| TabFrame {
                            project,
                            id,
                            index,
                            width: bounds.size.width,
                        });
                        if frame.replace(next) != next
                            && let Some((_, _, index)) = reveal
                        {
                            let scroll = scroll.clone();
                            window.on_next_frame(move |window, _| {
                                scroll.scroll_to_item(index + 1);
                                window.refresh();
                            });
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .gap(theme::space::CARD_GAP)
            .px(theme::space::CARD_PADDING)
            .children(
                tabs.into_iter()
                    .flat_map(|tabs| tabs.entries())
                    .map(|entry| {
                        let id = entry.id;
                        let project = project.unwrap();
                        let details = entry.invocation.details();
                        div()
                            .id(("run-tab", id))
                            .flex()
                            .flex_none()
                            .items_center()
                            .h(theme::space::LOG_TOOLBAR)
                            .gap(theme::space::CARD_GAP)
                            .px(theme::space::CARD_GAP)
                            .max_w(theme::space::RUN_COMMAND)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            .when(selected == Some(id), |s| {
                                s.bg(rgb(theme.surface.raised))
                                    .debug_selector(|| "selected-run-tab".into())
                            })
                            .when(selected != Some(id), |s| {
                                s.hover(|s| {
                                    s.bg(crate::theme::control_hover(theme, theme.surface.panel))
                                })
                            })
                            .on_click(
                                cx.listener(move |root, _, _, cx| root.select_run(project, id, cx)),
                            )
                            .child(crate::icons::dot(state_colour(&entry.value.state, theme)))
                            .child(
                                div()
                                    .id(("run-caption", id))
                                    .min_w_0()
                                    .truncate()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(theme.text.secondary))
                                    .hint_delay()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            crate::view::tooltip::Tooltip::details(
                                                details.clone(),
                                                theme,
                                            )
                                        })
                                        .into()
                                    })
                                    .child(entry.invocation.title()),
                            )
                            .child(
                                div()
                                    .id(("close-run", id))
                                    .flex_none()
                                    .size(theme::space::HEAD_BUTTON)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(theme::radius::ROW)
                                    .hover(move |s| {
                                        s.bg(theme::close_hover(theme, selected == Some(id)))
                                    })
                                    .hint_delay()
                                    .tooltip(move |_, cx| {
                                        cx.new(|_| {
                                            crate::view::tooltip::Tooltip::text(
                                                ui.text(Text::MenuCloseTab),
                                                theme,
                                            )
                                        })
                                        .into()
                                    })
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        cx.stop_propagation();
                                        root.close_run_asking(project, id, window, cx);
                                    }))
                                    .child(
                                        icons::toolbar("icons/close.svg")
                                            .text_color(rgb(theme.text.tertiary)),
                                    ),
                            )
                    }),
            )
            .into_any_element()
    }

    /// Run, Stop and Clear at the left, the command in the middle, and
    /// what became of it at the right — what to do, what it is, how it
    /// went.
    fn render_run_toolbar(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let run = self.active_run();
        let live = run.is_some_and(|run| !run.state.is_over());
        let command = self.last_command();

        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .h(theme::space::LOG_TOOLBAR)
            .px(theme::space::CARD_PADDING)
            .child(self.render_run_button(
                "run",
                "ui/tool-run.svg",
                if live { Text::RunAgain } else { Text::RunStart },
                // Never a dead button: with nothing chosen it says what
                // is missing, and task 5 gives it the list to open.
                command.is_some(),
                Text::RunChoose,
                |root, cx| root.rerun(cx),
                cx,
            ))
            .child(self.render_run_button(
                "stop",
                "ui/tool-stop.svg",
                Text::RunStop,
                live,
                Text::RunNotRunning,
                |root, cx| root.stop_run(cx),
                cx,
            ))
            .child(self.render_run_button(
                "clear",
                "ui/tool-clear.svg",
                Text::RunClear,
                run.is_some_and(|run| run.view.is_some()),
                Text::RunNotRunning,
                |root, cx| root.clear_run(cx),
                cx,
            ))
            // The command, in the code face because it is a command —
            // the same face the terminal under it draws with — and a
            // button, because the list of what a project can run hangs
            // from it.
            .child(
                div()
                    .id("run-command")
                    .relative()
                    .flex()
                    // Sized to what it says, up to a limit, rather than
                    // stretched across the toolbar: a chevron four
                    // hundred pixels from the words it belongs to reads
                    // as two controls. **And it is what gives way** when
                    // the panel is a side dock — the command truncates,
                    // and the state at the far end, which is the point of
                    // the panel, stays whole.
                    .flex_shrink(1.)
                    .max_w(theme::space::RUN_COMMAND)
                    .min_w_0()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .px(theme::space::CARD_GAP)
                    .h(theme::space::HEAD_BUTTON)
                    .rounded(theme::radius::ROW)
                    .cursor_pointer()
                    .hover(|button| {
                        button.bg(crate::theme::control_hover(theme, theme.surface.card))
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .font_family(self.code_font.clone())
                            .text_size(theme::size::UI)
                            .text_color(rgb(if command.is_some() {
                                theme.text.secondary
                            } else {
                                theme.text.faint
                            }))
                            .child(command.unwrap_or_else(|| ui.text(Text::RunChoose).to_owned())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .child(icons::chevron(true).text_color(rgb(theme.text.faint))),
                    )
                    .on_click(cx.listener(|root, _event, window, cx| {
                        cx.stop_propagation();
                        root.toggle_run_picker(window, cx);
                    }))
                    // Where it ended up, for the list that hangs from it:
                    // a canvas rather than an update, because prepaint is
                    // inside this very frame.
                    .child({
                        let at = self.run_at.clone();
                        gpui::canvas(
                            move |bounds, _window, _cx| at.set(bounds.bottom_left()),
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    }),
            )
            .child(div().flex_1())
            .children(run.map(|run| {
                let taken = run::elapsed(run.taken());
                div()
                    .flex_none()
                    .text_size(theme::size::META)
                    .text_color(rgb(state_colour(&run.state, theme)))
                    .child(ui.run_state(&run.state, &taken))
            }))
            .into_any_element()
    }

    /// One toolbar button: the shape the Commit panel's toolbar has.
    ///
    /// A button that would do nothing is greyed and says why — the rule
    /// that came out of `cmd-k` returning silently when a commit was
    /// refused.
    #[allow(clippy::too_many_arguments)]
    fn render_run_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: Text,
        enabled: bool,
        why_not: Text,
        deed: fn(&mut Root, &mut Context<Root>),
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let tooltip = ui.text(if enabled { label } else { why_not }).to_owned();
        let action = match id {
            "run" => Some(crate::actions::keymap::Named::Run),
            "stop" => Some(crate::actions::keymap::Named::StopRun),
            "clear" => Some(crate::actions::keymap::Named::ClearRun),
            _ => None,
        };

        div()
            .id(gpui::ElementId::Name(format!("run-tool-{id}").into()))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .when(enabled, |button| {
                button.cursor_pointer().hover(|button| {
                    button.bg(crate::theme::control_hover(theme, theme.surface.card))
                })
            })
            .child(icons::toolbar(icon).text_color(rgb(if enabled {
                theme.text.tertiary
            } else {
                theme.text.faint
            })))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| match action.filter(|_| enabled) {
                    Some(action) => {
                        crate::view::tooltip::Tooltip::for_action(tooltip.clone(), action, theme)
                    }
                    None => crate::view::tooltip::Tooltip::text(tooltip.clone(), theme),
                })
                .into()
            })
            .on_click(cx.listener(move |root, _event, _window, cx| {
                // The button is inside the panel, and the panel has
                // handlers of its own: a gesture aimed at something
                // inside a row says so.
                cx.stop_propagation();
                if enabled {
                    deed(root, cx);
                }
            }))
            .into_any_element()
    }
}

/// What colour a state is drawn in. Zero is the only one that needs no
/// reading, and it is the only one in `accent.green`.
fn state_colour(state: &run::State, theme: theme::Theme) -> u32 {
    match state {
        run::State::Running => theme.accent.brass,
        run::State::Finished { code: 0 } => theme.accent.green,
        run::State::Finished { .. } | run::State::Failed { .. } => theme.accent.rust,
        run::State::Stopped => theme.text.faint,
    }
}

impl Root {
    /// The card the list of commands answers with, hanging from the
    /// command that opens it.
    ///
    /// Section 6.18's shape, because it is the same gesture as the branch
    /// popup: a field, a list, and a first row that offers what was typed
    /// when it is not in the list yet. The field is in the code face,
    /// because what is typed there is a command.
    pub(crate) fn run_card(&self, cx: &mut Context<Root>) -> Card {
        let theme = self.theme;
        let ui = self.ui();
        let rows = self.run_rows();

        let head = div()
            .flex()
            .items_center()
            .h(theme::space::BRANCH_FIELD)
            .px(theme::space::CARD_PADDING)
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .font_family(self.code_font.clone())
            .text_size(theme::size::UI)
            .child(self.picker_field(
                ui.text(Text::RunPlaceholder).to_owned(),
                theme.surface.card,
                cx,
            ))
            .into_any_element();

        let rows = if rows.is_empty() {
            div()
                .p(theme::space::CARD_PADDING)
                .text_size(theme::size::UI)
                .text_color(rgb(theme.text.faint))
                .child(ui.text(Text::RunNothingToOffer))
                .into_any_element()
        } else {
            let lit = self.picker_lit_row();
            let items = rows
                .iter()
                .enumerate()
                .map(|(index, row)| self.render_run_row(index, row, lit == Some(index), cx))
                .collect();
            self.picker_children("run-rows", items, Some(theme::space::BRANCH_LIST), cx)
        };

        Card {
            id: "run-card",
            place: Place::Hanging {
                from: self.run_at.get(),
                width: theme::space::BRANCH_POPUP,
            },
            backdrop: true,
            head,
            rows,
            preview: None,
            foot: None,
        }
    }

    /// One row: the line in the code face, and what it came from at the
    /// right — nothing for a line the person is typing.
    fn render_run_row(
        &self,
        index: usize,
        row: &run::Row,
        lit: bool,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();

        self.picker_item(index, div().id(("run-row", index)), cx)
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .h(theme::space::PICKER_ROW)
            .mx(theme::radius::ROW)
            .px(crate::view::popup::row_inset())
            .rounded(theme::radius::ROW)
            .when(lit, |row| row.bg(rgb(theme.surface.raised)))
            .cursor_pointer()
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_family(self.code_font.clone())
                    .text_size(theme::size::UI)
                    .text_color(rgb(match row.source {
                        run::Source::Typed => theme.accent.brass,
                        _ => theme.text.primary,
                    }))
                    .child(row.line.clone()),
            )
            .children(source_label(row.source).map(|label| {
                div()
                    .flex_none()
                    .text_size(theme::size::EYEBROW)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(label))
            }))
            .into_any_element()
    }
}

/// What a row is, when it is worth saying: a line being typed says it in
/// its colour instead.
fn source_label(source: run::Source) -> Option<Text> {
    match source {
        run::Source::Typed => None,
        run::Source::Recent => Some(Text::RunRecent),
        run::Source::Declared => Some(Text::RunDeclared),
    }
}
