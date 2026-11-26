//! Native recovery for hidden jobs, including jobs whose original session closed.
use super::controls::ButtonText;
use crate::root::picker::Focus;
use crate::view::tooltip::Hint as _;
use crate::{Root, i18n::Text, terminal_job::OutputState, theme};
use gpui::{AnimationExt as _, Context, MouseButton, anchored, deferred, div, prelude::*, px, rgb};
use gpui_component::{
    ActiveTheme as _, Sizable,
    button::{Button, ButtonVariants},
};
use guild_session::{
    overlay::Target,
    terminal_job::{JobReceipt, JobState, Presentation, Scope},
};

fn trigger(theme: theme::Theme, open: bool) -> gpui::Stateful<gpui::Div> {
    div()
        .id("terminal-jobs-trigger")
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .debug_selector(|| "terminal-jobs-trigger".into())
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(theme::space::STATUS_BUTTON)
        .rounded_full()
        .border_1()
        .border_color(theme::settings::outline(theme))
        .bg(rgb(theme.surface.card))
        .cursor_pointer()
        .when(open, |button| button.bg(rgb(theme.surface.raised)))
        .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.card)))
        // Root must not replace the previous body focus before on_click.
        .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

/// Full original paths remain reachable without expanding the history row.
struct ContextTip {
    summary: Vec<String>,
    text: String,
    theme: theme::Theme,
}
impl gpui::Render for ContextTip {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) -> impl IntoElement {
        let margin = theme::space::CARD_GAP.to_pixels(window.rem_size());
        let width = theme::space::USAGE_CARD
            .to_pixels(window.rem_size())
            .min((window.viewport_size().width - margin * 2.).max(px(0.)));
        let height = theme::space::TERMINAL_JOBS_HEIGHT
            .to_pixels(window.rem_size())
            .min((window.viewport_size().height - margin * 2.).max(px(0.)));
        div()
            .id("terminal-context-detail")
            .debug_selector(|| "terminal-context-detail".into())
            .w(width)
            .max_h(height)
            .overflow_y_scroll()
            .px(theme::space::MENU_INSET)
            .py(theme::space::MENU_PADDING)
            .rounded(theme::radius::ROW)
            .bg(rgb(self.theme.surface.raised))
            .text_size(theme::size::META)
            .font_family(cx.theme().font_family.clone())
            .text_color(rgb(self.theme.text.secondary))
            .whitespace_normal()
            .children(self.summary.iter().map(|line| {
                let selector = format!("terminal-context:{line}");
                div()
                    .debug_selector(move || selector.clone())
                    .child(line.clone())
            }))
            .child(
                div()
                    .debug_selector(|| "terminal-context-text".into())
                    .child(self.text.clone()),
            )
    }
}

impl Root {
    /// Measure each native button and retain its parent focus without adding a Tab stop.
    fn terminal_job_action(
        &self,
        id: guild_session::terminal_job::JobId,
        name: &'static str,
        button: Button,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let list = self.terminal_list.as_ref().expect("open terminal history");
        let action = list
            .actions
            .borrow_mut()
            .entry((id, name))
            .or_insert_with(|| crate::root::terminal_jobs::Action {
                focus: cx.focus_handle(),
                bounds: Default::default(),
            })
            .clone();
        let bounds = action.bounds.clone();
        div()
            .id((name, id.as_u128() as u64))
            .track_focus(&action.focus)
            .relative()
            .flex_none()
            .child(button)
            .child(
                gpui::canvas(move |at, _, _| bounds.set(Some(at)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
    }

    pub(crate) fn render_terminal_job_trigger(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let label = self.ui().text(Text::TerminalJobs);
        trigger(theme, self.terminal_list.is_some())
            .child(
                crate::icons::toolbar("ui/workspace-terminal.svg")
                    .text_color(rgb(theme.text.tertiary)),
            )
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| {
                    super::tooltip::Tooltip::for_action(
                        label,
                        crate::actions::keymap::Named::ToggleTerminalJobs,
                        theme,
                    )
                })
                .into()
            })
            .on_click(cx.listener(|root, _, window, cx| {
                root.toggle_terminal_list(window, cx);
                cx.stop_propagation();
            }))
            .into_any_element()
    }

    fn terminal_original_context(
        &self,
        receipt: &JobReceipt,
        scope: Scope,
    ) -> (Vec<String>, String) {
        let ui = self.ui();
        let context = receipt.launch_context.as_deref();
        let session = context
            .and_then(|c| c.session_label.as_deref())
            .unwrap_or(ui.text(Text::TerminalUnknownSession));
        let root = context
            .and_then(|c| c.root_label.as_deref())
            .unwrap_or(ui.text(Text::TerminalUnknownRoot));
        let repo = context
            .and_then(|c| c.repository_label.as_deref())
            .unwrap_or(ui.text(Text::TerminalUnknownRepository));
        let branch = context
            .and_then(|c| c.branch.as_deref())
            .map(|branch| format!("{}: {branch}", ui.text(Text::TerminalObservedBranch)))
            .unwrap_or_else(|| ui.text(Text::TerminalUnknownBranch).into());
        let selected = self.terminal_list.as_ref().map(|list| &list.labels);
        let matching_worktree = context.is_some_and(|context| {
            context.root_label.is_some()
                && context.root_path.as_ref().is_some_and(|path| {
                    selected.is_some_and(|selected| {
                        selected.root_label.is_some() && selected.root_path.as_ref() == Some(path)
                    })
                })
        });
        let summary = match scope {
            Scope::All if context.is_some() => vec![
                format!("{}: {repo}", ui.text(Text::TerminalContextProject)),
                format!("{}: {root}", ui.text(Text::TerminalContextWorktree)),
                branch.clone(),
                format!("{}: {session}", ui.text(Text::TerminalContextSession)),
            ],
            Scope::Worktree if matching_worktree => vec![
                branch.clone(),
                format!("{}: {session}", ui.text(Text::TerminalContextSession)),
            ],
            Scope::Project | Scope::Worktree if context.is_some() => vec![
                format!("{}: {root}", ui.text(Text::TerminalContextWorktree)),
                branch.clone(),
                format!("{}: {session}", ui.text(Text::TerminalContextSession)),
            ],
            Scope::All => vec![format!("{repo} / {root} · {branch}"), session.to_owned()],
            Scope::Project => vec![format!("{root} · {branch}"), session.to_owned()],
            _ => vec![session.to_owned(), branch.clone()],
        };
        let path = |path: Option<&std::path::Path>| {
            path.map(|p| {
                p.to_string_lossy()
                    .chars()
                    .map(|c| if c.is_control() { '�' } else { c })
                    .collect::<String>()
            })
        };
        let root_path = path(context.and_then(|c| c.root_path.as_deref()))
            .unwrap_or_else(|| ui.text(Text::TerminalUnknownRoot).into());
        let repository = path(context.and_then(|c| c.repository.as_deref()))
            .unwrap_or_else(|| ui.text(Text::TerminalUnknownRepository).into());
        let cwd = path(context.map(|c| c.program_cwd.as_path()))
            .unwrap_or_else(|| ui.text(Text::TerminalUnknownCwd).into());
        let labelled_path = |label: Text, name: &str, path: &str| {
            if name == path {
                format!("{}: {name}", ui.text(label))
            } else {
                format!("{}: {name}\n{path}", ui.text(label))
            }
        };
        let detail = format!(
            "{}\n{}\n{}: {cwd}",
            labelled_path(Text::TerminalContextProject, repo, &repository),
            labelled_path(Text::TerminalContextWorktree, root, &root_path),
            ui.text(Text::TerminalProgramCwd)
        );
        (summary, detail)
    }

    pub(crate) fn render_terminal_jobs(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let list = self.terminal_list.as_ref()?;
        let ui = self.ui();
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        let receipts = self.terminal_jobs.receipt_snapshot();
        let context = self.terminal_history_context();
        let groups = context.select(list.scope, &receipts);
        let empty = groups.ongoing.is_empty() && groups.completed.is_empty();
        let mut rows = div()
            .id("terminal-jobs-rows")
            .flex()
            .flex_col()
            .overflow_y_scroll()
            .track_scroll(&list.scroll)
            .debug_selector(|| "terminal-job-rows".into())
            .min_h_0();
        let mut preceding_group = false;
        for (label, group) in [
            (Text::TerminalOngoing, groups.ongoing),
            (Text::TerminalCompleted, groups.completed),
        ] {
            if group.is_empty() {
                continue;
            }
            if preceding_group {
                rows = rows.child(super::popup::separator(self.theme));
            }
            preceding_group = true;
            rows = rows.child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .h(theme::space::HEAD_BUTTON)
                    .px(theme::space::MENU_INSET)
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.secondary))
                    .child(format!("{} · {}", ui.text(label), group.len())),
            );
            for receipt in group {
                let id = receipt.id;
                let has_output = self.terminal_jobs.terminal(&id).is_some();
                let output = self.terminal_jobs.output_state(&id, cx);
                let show_output = matches!(output, Some(OutputState::Live | OutputState::Retained));
                let activity = self.review_activity.snapshots.get(&id);
                let title = if receipt.title == "revust" {
                    activity
                        .filter(|a| !a.title.is_empty())
                        .map(|a| a.title.clone())
                        .unwrap_or_else(|| receipt.title.clone())
                } else {
                    receipt.title.clone()
                };
                let (summary, detail) = self.terminal_original_context(receipt, list.scope);
                let theme = self.theme;
                let status = activity
                    .filter(|a| !a.final_result && !a.stale && receipt.state == JobState::Running)
                    .and_then(|a| review_phase(&a.phase))
                    .map(|t| ui.text(t).to_owned())
                    .unwrap_or_else(|| ui.terminal_status(receipt));
                let explanation = match output {
                    Some(OutputState::Empty) => Some(Text::TerminalOutputEmpty),
                    Some(OutputState::Deleted) => Some(Text::TerminalOutputDeleted),
                    _ => None,
                };
                // Animate a decorative sibling, keeping action/tooltip identity stable.
                let hovered = list.hovered == Some(id);
                let background = div().absolute().size_full();
                let background = if hovered || list.fading == Some(id) {
                    background
                        .with_animation(
                            (
                                if hovered {
                                    "terminal-job-hover-in"
                                } else {
                                    "terminal-job-hover-out"
                                },
                                id.as_u128() as u64,
                            ),
                            gpui::Animation::new(theme::motion::TAB_SETTLE).with_max_fps(20.),
                            move |background, progress| {
                                background.bg(theme::blend(
                                    crate::theme::control_hover(theme, theme.surface.card).into(),
                                    rgb(theme.surface.card).into(),
                                    if hovered { progress } else { 1. - progress },
                                ))
                            },
                        )
                        .into_any_element()
                } else {
                    background.into_any_element()
                };
                let mut row = div()
                    .id(("terminal-job-row", id.as_u128() as u64))
                    .w_full()
                    .flex_none()
                    .flex()
                    .flex_col()
                    .min_w_0()
                    .gap(theme::space::MENU_ICON_GAP)
                    .px(theme::space::MENU_INSET)
                    .pt(theme::space::MENU_ICON_GAP)
                    .pb(theme::space::CARD_GAP)
                    .relative()
                    .child(background)
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap(theme::space::MENU_ICON_GAP)
                            .child(
                                div()
                                    .id(("terminal-job-title", id.as_u128() as u64))
                                    .debug_selector(|| "terminal-job-title".into())
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(title)
                                    .text_color(rgb(self.theme.text.primary)),
                            )
                            .child(
                                div()
                                    .max_w(gpui::relative(0.5))
                                    .min_w_0()
                                    .whitespace_normal()
                                    .text_right()
                                    .text_size(theme::size::META)
                                    .child(status)
                                    .text_color(rgb(if receipt.state == JobState::Running {
                                        self.theme.accent.patina
                                    } else {
                                        self.theme.text.secondary
                                    })),
                            ),
                    )
                    .on_hover(cx.listener(move |root, over: &bool, _, cx| {
                        if let Some(list) = root.terminal_list.as_mut() {
                            if *over {
                                list.hovered = Some(id);
                            } else if list.hovered == Some(id) {
                                list.hovered = None;
                                list.fading = Some(id);
                            }
                            cx.notify();
                        }
                    }));
                if let Some(activity) = activity {
                    row = row.child(self.render_review_activity(receipt, activity, cx));
                }
                let metadata = div()
                    .min_w_0()
                    .flex_1()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap(theme::space::MENU_ICON_GAP)
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.secondary))
                    .child(div().flex_none().child(ui.terminal_started(
                        receipt.started_at_ms,
                        now,
                        crate::system::utc_offset(),
                    )))
                    .when_some(explanation, |line, text| {
                        line.child(
                            div()
                                .id(("terminal-output-note", id.as_u128() as u64))
                                .debug_selector(|| "terminal-output-explanation".into())
                                .min_w_0()
                                .truncate()
                                .child(ui.text(if text == Text::TerminalOutputEmpty {
                                    Text::TerminalNoSavedOutput
                                } else {
                                    text
                                }))
                                .tooltip(move |_, app| {
                                    app.new(|_| super::tooltip::Tooltip::text(ui.text(text), theme))
                                        .into()
                                }),
                        )
                    });
                let mut buttons = div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(theme::space::MENU_ICON_GAP);
                let action_style = |primary: bool, cx: &gpui::App| {
                    let style = super::controls::ghost(theme, false, cx).foreground(
                        rgb(if primary {
                            theme.text.primary
                        } else {
                            theme.text.secondary
                        })
                        .into(),
                    );
                    if primary {
                        style.color(rgb(theme.surface.raised).into())
                    } else {
                        style
                    }
                };
                // An icon on a secondary action, in that button's foreground:
                // GPUI paints an svg only in a colour set on the svg itself,
                // and the button's colour reaches its text, never its icon.
                let action_icon = |path: &'static str| {
                    crate::icons::toolbar(path).text_color(rgb(theme.text.secondary))
                };

                if self.terminal_jobs.can_open_archive(&id) {
                    buttons = buttons.child(
                        self.terminal_job_action(
                            id,
                            "open-run",
                            Button::new(("terminal-open-run", id.as_u128() as u64))
                                .small()
                                .debug_selector(|| "terminal-open-run".into())
                                .accessibility_label(ui.text(Text::TerminalOpenRun))
                                .custom(action_style(true, cx))
                                .child(
                                    div()
                                        .text_size(theme::size::META)
                                        .child(ui.text(Text::TerminalOpenRun)),
                                )
                                .on_click(cx.listener(move |root, _, window, cx| {
                                    root.close_terminal_list(Focus::Back, window, cx);
                                    if let Err(error) = root.open_revmux_archive(&id, window, cx) {
                                        root.fail(error);
                                    }
                                    cx.notify();
                                })),
                            cx,
                        ),
                    );
                }

                if show_output {
                    buttons = buttons.child(
                        self.terminal_job_action(
                            id,
                            "show",
                            Button::new(("terminal-show", id.as_u128() as u64))
                                .debug_selector(|| "terminal-show".into())
                                .small()
                                .custom(action_style(!self.terminal_jobs.can_open_archive(&id), cx))
                                .accessibility_label(ui.text(Text::TerminalShowOutput))
                                .when(!self.terminal_jobs.can_open_archive(&id), |button| {
                                    button.child(
                                        div()
                                            .text_size(theme::size::META)
                                            .child(ui.text(Text::TerminalShowOutput)),
                                    )
                                })
                                .when(self.terminal_jobs.can_open_archive(&id), |button| {
                                    button
                                        .w(theme::space::HEAD_BUTTON)
                                        .px_0()
                                        .child(action_icon("ui/push-preview.svg"))
                                        .tooltip(ui.text(Text::TerminalShowOutput))
                                })
                                .on_click(cx.listener(move |root, _, window, cx| {
                                    let target = root
                                        .active_tab
                                        .and_then(|id| root.tabs.iter().find(|t| t.id == id))
                                        .map(|tab| Target::Pane(tab.panes.focused()));
                                    let Some(target) = target else {
                                        root.warn(root.ui().text(Text::TerminalSelectPane).into());
                                        return;
                                    };
                                    root.close_terminal_list(Focus::Back, window, cx);
                                    if let Err(error) =
                                        root.reattach_terminal_job(&id, target, window, cx)
                                    {
                                        root.fail(error);
                                    }
                                    cx.notify();
                                })),
                            cx,
                        ),
                    );
                    if receipt.presentation == Presentation::Shown {
                        buttons = buttons.child(
                            self.terminal_job_action(
                                id,
                                "hide",
                                Button::new(("terminal-hide", id.as_u128() as u64))
                                    .small()
                                    .custom(action_style(false, cx))
                                    .w(theme::space::HEAD_BUTTON)
                                    .px_0()
                                    .accessibility_label(ui.text(Text::PanelHide))
                                    .tooltip(ui.text(Text::PanelHide))
                                    .child(action_icon("ui/head-hide.svg"))
                                    .on_click(cx.listener(move |root, _, window, cx| {
                                        if let Some(receipt) = root.terminal_jobs.receipt(&id) {
                                            root.dismiss_overlay(
                                                &format!("terminal-{id}"),
                                                &receipt.owner.to_string(),
                                                false,
                                                window,
                                                cx,
                                            );
                                        }
                                    })),
                                cx,
                            ),
                        );
                    }
                }
                if !receipt.state.is_terminal() {
                    buttons = buttons.child(
                        self.terminal_job_action(
                            id,
                            "cancel",
                            Button::new(("terminal-cancel", id.as_u128() as u64))
                                .debug_selector(|| "terminal-cancel".into())
                                .small()
                                .custom(action_style(false, cx))
                                .w(theme::space::HEAD_BUTTON)
                                .px_0()
                                .accessibility_label(ui.text(if receipt.is_archive_view() {
                                    Text::TerminalCloseViewer
                                } else {
                                    Text::Cancel
                                }))
                                .tooltip(ui.text(if receipt.is_archive_view() {
                                    Text::TerminalCloseViewer
                                } else {
                                    Text::Cancel
                                }))
                                .child(action_icon("ui/tool-stop.svg"))
                                .on_click(cx.listener(move |root, _, _, cx| {
                                    if let Err(error) =
                                        root.terminal_jobs.cancel(&id, "User cancellation", cx)
                                    {
                                        root.fail(error);
                                    }
                                    cx.notify();
                                })),
                            cx,
                        ),
                    );
                } else if has_output {
                    buttons = buttons.child(
                        self.terminal_job_action(
                            id,
                            "release",
                            Button::new(("terminal-release", id.as_u128() as u64))
                                .small()
                                .debug_selector(|| "terminal-release".into())
                                .custom(action_style(false, cx))
                                .w(theme::space::HEAD_BUTTON)
                                .px_0()
                                .accessibility_label(ui.text(Text::TerminalDeleteOutput))
                                .tooltip(ui.text(Text::TerminalDeleteOutput))
                                .child(action_icon("ui/menu-remove.svg"))
                                .on_click(cx.listener(move |root, _, window, cx| {
                                    if let Some(receipt) = root.terminal_jobs.receipt(&id) {
                                        root.dismiss_overlay(
                                            &format!("terminal-{id}"),
                                            &receipt.owner.to_string(),
                                            true,
                                            window,
                                            cx,
                                        );
                                    }
                                    if let Err(error) = root.terminal_jobs.release(&id) {
                                        root.fail(error);
                                    }
                                    root.terminal_geometry.remove(&id);
                                    cx.notify();
                                })),
                            cx,
                        ),
                    );
                }
                buttons = buttons.child(
                    div()
                        .id(gpui::ElementId::Name(
                            format!("terminal-job-info:{:?}:{id}", list.scope).into(),
                        ))
                        .debug_selector(move || format!("terminal-job-info:{id}"))
                        .aria_label(ui.text(Text::TerminalJobDetails))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(theme::space::HEAD_BUTTON)
                        .rounded(theme::radius::ROW)
                        .group("terminal-job-info")
                        .hover(|icon| {
                            icon.bg(crate::theme::control_hover(theme, theme.surface.card))
                        })
                        .child(
                            div().debug_selector(|| "terminal-job-info".into()).child(
                                // The colour is the svg's own, and so is its
                                // hover: an svg is painted only in a colour set
                                // on it, never in its parent's.
                                crate::icons::toolbar("ui/job-info.svg")
                                    .text_color(rgb(theme.text.secondary))
                                    .group_hover("terminal-job-info", |icon| {
                                        icon.text_color(rgb(theme.text.primary))
                                    }),
                            ),
                        )
                        .hoverable_tooltip(move |_, app| {
                            app.new(|_| ContextTip {
                                summary: summary.clone(),
                                text: detail.clone(),
                                theme,
                            })
                            .into()
                        }),
                );
                row = row.child(
                    div()
                        .debug_selector(|| "terminal-job-footer".into())
                        .min_w_0()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap(theme::space::MENU_ICON_GAP)
                        .child(metadata)
                        .child(buttons),
                );
                if let Some(error) = receipt.error.clone() {
                    row = row.child(
                        div()
                            .whitespace_normal()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.accent.rust))
                            .child(error),
                    );
                }
                rows = rows.child(row);
            }
        }
        if empty {
            rows = rows.child(
                div()
                    .px(theme::space::MENU_INSET)
                    .py(theme::space::MENU_PADDING)
                    .child(ui.text(if list.scope == Scope::All {
                        Text::TerminalNoJobs
                    } else {
                        Text::TerminalScopeEmpty
                    })),
            );
        }
        if empty && let Some(wider) = list.scope.wider() {
            let wider = if wider.available(&context) {
                wider
            } else {
                Scope::All
            };
            rows = rows.child(
                Button::new("terminal-wider")
                    .small()
                    .debug_selector(|| "terminal-wider".into())
                    .custom(super::controls::ghost(self.theme, false, cx))
                    .ui_label(ui.terminal_scope(wider))
                    .on_click(cx.listener(move |root, _, window, cx| {
                        if let Some(list) = root.terminal_list.as_ref() {
                            list.focus.focus(window, cx);
                        }
                        root.select_terminal_scope(wider, cx);
                        cx.stop_propagation();
                    })),
            );
        }
        let anchor = std::rc::Rc::new(std::cell::Cell::new(None));
        let measured = anchor.clone();
        let scope_button = Button::new("terminal-job-scope")
            .small()
            .debug_selector(|| "terminal-job-scope".into())
            .custom(super::controls::ghost(self.theme, false, cx))
            .ui_label(ui.terminal_scope(list.scope))
            .dropdown_caret(true)
            .relative()
            .child(
                gpui::canvas(
                    move |bounds, _, _| measured.set(Some(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_click(cx.listener(move |root, _, window, cx| {
                cx.stop_propagation();
                if let Some(bounds) = anchor.get()
                    && let Some(list) = root.terminal_list.as_ref()
                {
                    let popup = list.focus.clone();
                    root.open_menu(
                        crate::view::menu::Target::TerminalJobScopes { popup },
                        crate::view::menu::Anchor::Below(bounds),
                        window,
                        cx,
                    );
                }
            }));
        let opening_label = match list.scope {
            Scope::Session => list
                .labels
                .session_label
                .as_deref()
                .unwrap_or(ui.text(Text::TerminalUnknownSession)),
            Scope::Worktree => list
                .labels
                .root_label
                .as_deref()
                .unwrap_or(ui.text(Text::TerminalUnknownRoot)),
            Scope::Project => list
                .labels
                .repository_label
                .as_deref()
                .unwrap_or(ui.text(Text::TerminalUnknownRepository)),
            Scope::All => ui.text(Text::TerminalAllContexts),
        };
        let project = list.labels.repository_label.as_deref();
        let subtitle = match project {
            Some(project) if project != opening_label => format!("{project} · {opening_label}"),
            _ => opening_label.to_owned(),
        };
        let viewport = self.viewport();
        let margin = self.rem_px(theme::space::CARD_GAP);
        let top = self
            .rem_px(theme::space::TITLEBAR)
            .min(f32::from(viewport.height));
        let width = self
            .rem_px(theme::space::TERMINAL_JOBS_WIDTH)
            .min((f32::from(viewport.width) - 2. * margin).max(0.));
        let height = self
            .rem_px(theme::space::TERMINAL_JOBS_HEIGHT)
            .min((f32::from(viewport.height) - top - margin).max(0.));
        let backdrop = deferred(
            anchored().position(gpui::Point::default()).child(
                div()
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _, window, cx| {
                            root.close_terminal_list(Focus::Back, window, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|root, _, window, cx| {
                            root.close_terminal_list(Focus::Back, window, cx);
                            cx.stop_propagation();
                        }),
                    ),
            ),
        )
        .priority(1);
        let card = super::popup::card_look(self.theme)
            .id("terminal-jobs-panel")
            .w(px(width))
            .max_h(px(height))
            .occlude()
            .flex()
            .flex_col()
            .min_h_0()
            .overflow_hidden()
            .py(theme::space::MENU_PADDING)
            .text_size(theme::size::BODY)
            .text_color(rgb(self.theme.text.primary))
            .debug_selector(|| "terminal-jobs-panel".into())
            .track_focus(&list.focus)
            .tab_group()
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    root.close_terminal_list(Focus::Back, window, cx);
                    cx.stop_propagation();
                } else {
                    root.terminal_list_tab(event, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(div().track_focus(&list.tab_start).w_0().h_0().flex_none())
            .child(
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_between()
                    .min_w_0()
                    .px(theme::space::MENU_INSET)
                    .gap(theme::space::CARD_GAP)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(ui.text(Text::TerminalJobs)),
                    )
                    .child(scope_button),
            )
            .child(
                div()
                    .flex_none()
                    .min_w_0()
                    .truncate()
                    .px(theme::space::MENU_INSET)
                    .py(theme::space::MENU_PADDING)
                    .text_size(theme::size::META)
                    .text_color(rgb(self.theme.text.secondary))
                    .child(subtitle),
            )
            .child(rows)
            .child(div().track_focus(&list.tab_end).w_0().h_0().flex_none());
        let card = deferred(
            anchored()
                .anchor(gpui::Anchor::TopRight)
                .position(gpui::point(viewport.width - px(margin), px(top)))
                .snap_to_window_with_margin(px(margin))
                .child(card),
        )
        .priority(2);
        Some(div().child(backdrop).child(card).into_any_element())
    }
}

fn review_spinner(theme: theme::Theme) -> gpui::AnyElement {
    div()
        .debug_selector(|| "review-running-spinner".into())
        .child(super::activity::ring(rgb(theme.accent.patina), true))
        .into_any_element()
}

fn review_phase(phase: &str) -> Option<Text> {
    match phase {
        "find" => Some(Text::ReviewAnalyze),
        "synthesis" => Some(Text::ReviewConsolidate),
        "verify" => Some(Text::ReviewVerify),
        _ => None,
    }
}
impl Root {
    fn render_review_activity(
        &self,
        receipt: &JobReceipt,
        activity: &crate::review_activity::Snapshot,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let id = receipt.id;
        let expanded = self
            .terminal_list
            .as_ref()
            .is_some_and(|l| l.review_details.contains(&id));
        let mut view = div()
            .flex()
            .flex_col()
            .min_w_0()
            .gap(theme::space::MENU_ICON_GAP)
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.secondary))
            .debug_selector(|| "review-activity".into());
        let running = receipt.state == JobState::Running
            && !activity.final_result
            && !activity.stale
            && review_phase(&activity.phase).is_some();
        let progress = match activity.phase.as_str() {
            "find" => Text::ReviewAnalyzeProgress,
            "synthesis" => Text::ReviewConsolidateProgress,
            "verify" => Text::ReviewVerifyProgress,
            _ => Text::ReviewNoFindingsYet,
        };
        let mut line = div()
            .flex()
            .items_center()
            .gap(theme::space::MENU_ICON_GAP)
            .min_w_0();
        if running {
            line = line.child(review_spinner(theme));
        }
        line = line.child(div().flex_1().min_w_0().whitespace_normal().child(ui.text(
            if activity.stale {
                Text::ReviewStale
            } else if activity.legacy {
                Text::ReviewLegacy
            } else if activity.incomplete {
                Text::ReviewIncomplete
            } else if activity.final_result {
                Text::ReviewFinal
            } else {
                progress
            },
        )));
        line = line.child(
            self.terminal_job_action(
                id,
                "findings",
                Button::new(("review-details", id.as_u128() as u64))
                    .small()
                    .debug_selector(|| "review-details".into())
                    .custom(super::controls::ghost(theme, expanded, cx))
                    .accessibility_label(ui.text(Text::ReviewDetails))
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .child(ui.text(Text::ReviewDetails)),
                    )
                    .on_click(cx.listener(move |root, _, _, cx| {
                        if let Some(list) = root.terminal_list.as_mut()
                            && !list.review_details.remove(&id)
                        {
                            list.review_details.insert(id);
                        }
                        cx.notify();
                    })),
                cx,
            ),
        );
        view = view.child(line);
        if running && !activity.narration.is_empty() {
            view = view.child(div().whitespace_normal().child(activity.narration.clone()));
        }
        let (serious, warnings, unknown) = activity.counts();
        let mut counts = div()
            .flex()
            .flex_wrap()
            .gap(theme::space::MENU_ICON_GAP)
            .items_center()
            .debug_selector(|| "review-counts".into());
        if activity.clean && !activity.stale {
            counts = counts.child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .text_color(rgb(theme.accent.green))
                    .child(
                        crate::icons::glyph("ui/check.svg", theme::size::META)
                            .text_color(rgb(theme.accent.green)),
                    )
                    .child(ui.text(Text::ReviewClean)),
            );
        } else if !activity.findings.is_empty() {
            for (count, label, color, icon) in [
                (serious, Text::ReviewSerious, theme.accent.rust, "!"),
                (warnings, Text::ReviewWarnings, theme.accent.brass, "△"),
                (unknown, Text::ReviewUnknown, theme.text.secondary, "?"),
            ] {
                if count > 0 {
                    counts = counts.child(
                        div()
                            .text_color(rgb(color))
                            .child(format!("{icon} {}: {count}", ui.text(label))),
                    );
                }
            }
            counts = counts.child(format!(
                "{}{}",
                if activity.final_result {
                    format!("{} · ", ui.text(Text::ReviewFinal))
                } else {
                    String::new()
                },
                ui.text(if activity.assessed {
                    Text::ReviewAssessed
                } else {
                    Text::ReviewProvisional
                })
            ));
        } else if !activity.categorized.is_empty() {
            counts = counts.child(format!(
                "{}: {}",
                ui.text(Text::ReviewOther),
                activity.categorized.len()
            ));
        } else if !activity.unavailable && !activity.final_result {
            counts = counts.child(ui.text(Text::ReviewNoFindingsYet));
        }
        view = view.child(counts);
        if !expanded && let Some(finding) = activity.findings.first() {
            view = view.child(
                div()
                    .whitespace_normal()
                    .text_color(rgb(theme.text.primary))
                    .child(crate::title::truncate(&finding.title, 240)),
            );
        }
        if expanded {
            let mut details = div()
                .id(("review-finding-details", id.as_u128() as u64))
                .debug_selector(|| "review-finding-details".into())
                .max_h(theme::space::TERMINAL_JOBS_HEIGHT)
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap(theme::space::CARD_GAP)
                .p(theme::space::MENU_PADDING)
                .rounded(theme::radius::ROW)
                .bg(rgb(theme.surface.raised));
            for finding in &activity.findings {
                let (label, color) = match finding.severity.as_str() {
                    "critical" | "major" => (Text::ReviewSerious, theme.accent.rust),
                    "minor" => (Text::ReviewWarnings, theme.accent.brass),
                    _ => (Text::ReviewUnknown, theme.text.secondary),
                };
                details = details.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::space::MENU_ICON_GAP)
                        .whitespace_normal()
                        .child(div().text_color(rgb(color)).child(format!(
                            "{} · {}",
                            ui.text(label),
                            finding.title.chars().take(240).collect::<String>()
                        )))
                        .child(finding.body.chars().take(16 << 10).collect::<String>())
                        .child(format!(
                            "{}:{} · {}",
                            finding.file.chars().take(512).collect::<String>(),
                            finding.line,
                            finding.verdict
                        )),
                );
            }
            for (category, finding) in &activity.categorized {
                let category = *category;
                let label = match category {
                    crate::review_activity::Category::OpenQuestion => Text::ReviewOpenQuestion,
                    crate::review_activity::Category::PreExisting => Text::ReviewPreExisting,
                    crate::review_activity::Category::Immaterial => Text::ReviewImmaterial,
                    crate::review_activity::Category::Rejected => Text::ReviewRejected,
                };
                details = details.child(
                    div()
                        .debug_selector(move || format!("review-category-{category:?}"))
                        .flex()
                        .flex_col()
                        .gap(theme::space::MENU_ICON_GAP)
                        .whitespace_normal()
                        .child(ui.text(label))
                        .child(
                            div()
                                .text_color(rgb(theme.text.primary))
                                .child(finding.title.chars().take(240).collect::<String>()),
                        )
                        .child(finding.body.chars().take(16 << 10).collect::<String>())
                        .child(format!(
                            "{}:{}",
                            finding.file.chars().take(512).collect::<String>(),
                            finding.line
                        )),
                );
            }
            if activity.other > 0 {
                details = details.child(format!(
                    "{}: {}",
                    ui.text(Text::ReviewOther),
                    activity.other
                ));
            }
            if activity.dropped > 0 {
                details = details.child(format!(
                    "{}: {}",
                    ui.text(Text::ReviewDropped),
                    activity.dropped
                ));
            }
            if !activity.reported.is_empty() {
                details = details.child(div().whitespace_normal().child(format!(
                    "{}\n{}",
                    ui.text(Text::ReviewReported),
                    activity.reported
                )));
            }
            view = view.child(details);
        }
        view.into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{FocusHandle, Modifiers, Render, TestAppContext, Window};
    struct ClipPreview {
        offset: f32,
        running: bool,
        renders: std::rc::Rc<std::cell::Cell<usize>>,
    }
    impl Render for ClipPreview {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            self.renders.set(self.renders.get() + 1);
            div()
                .flex()
                .flex_col()
                .w(px(30.))
                .h(px(20.))
                .overflow_hidden()
                .child(div().flex_none().h(px(self.offset)))
                .child(div().flex_none().when(self.running, |d| {
                    d.child(review_spinner(theme::Theme::dark()))
                }))
        }
    }
    #[gpui::test]
    fn clipped_review_spinner_stops_and_reappears(cx: &mut TestAppContext) {
        let renders = std::rc::Rc::new(std::cell::Cell::new(0));
        let mut view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let entity = cx.new(|_| ClipPreview {
                offset: 40.,
                running: true,
                renders: renders.clone(),
            });
            view = Some(entity.clone());
            gpui_component::Root::new(entity, window, cx)
        });
        let view = view.unwrap();
        for (offset, running, expected) in [
            (40., true, 0),
            (0., true, 1),
            (40., true, 0),
            (0., true, 1),
            (0., false, 0),
        ] {
            view.update(cx, |v, cx| {
                v.offset = offset;
                v.running = running;
                cx.notify();
            });
            cx.run_until_parked();
            super::super::activity::budget_tests::redraws_in_tick(cx, &renders);
            assert_eq!(
                super::super::activity::budget_tests::redraws_in_tick(cx, &renders),
                expected,
                "offset={offset}, running={running}"
            );
        }
    }
    struct Host {
        root: FocusHandle,
        body: FocusHandle,
        saved: Option<FocusHandle>,
    }
    impl Render for Host {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .track_focus(&self.root)
                .size_full()
                .flex()
                .flex_col()
                .child(div().id("body").track_focus(&self.body).h(px(100.)))
                .child(
                    trigger(theme::Theme::default(), false)
                        .debug_selector(|| "trigger".into())
                        .on_click(cx.listener(|host, _, window, cx| {
                            host.saved = window.focused(cx);
                            cx.notify();
                        })),
                )
        }
    }
    #[gpui::test]
    fn terminal_jobs_trigger_keeps_the_body_focus_until_opening(cx: &mut TestAppContext) {
        let mut host = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            crate::actions::install(cx);
            let view = cx.new(|cx| Host {
                root: cx.focus_handle(),
                body: cx.focus_handle(),
                saved: None,
            });
            view.read(cx).body.clone().focus(window, cx);
            host = Some(view.clone());
            gpui_component::Root::new(view, window, cx)
        });
        cx.simulate_resize(gpui::size(px(400.), px(200.)));
        let button = cx.debug_bounds("trigger").unwrap();
        cx.simulate_click(button.center(), Modifiers::default());
        cx.update(|_, cx| {
            let host = host.as_ref().unwrap().read(cx);
            assert_eq!(
                host.saved,
                Some(host.body.clone()),
                "mouse-down must not replace the body with Root before the popup records it"
            );
        });
    }
}
