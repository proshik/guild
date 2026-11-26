//! The native URL clone form, using the same card, fields and tokens as Guild.
use crate::{
    Root,
    clone_project::{Field, Phase},
    i18n::Text,
    theme,
};
use gpui::prelude::*;
use gpui::{Context, IntoElement, MouseButton, deferred, div, rgb};

impl Root {
    pub(crate) fn render_clone_project(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let open = self.clone_project.as_ref().filter(|open| open.visible)?;
        let theme = self.theme;
        let ui = self.ui();
        let busy = open.phase.busy();
        let failed = matches!(open.phase, Phase::Failed { .. });
        let completed = matches!(open.phase, Phase::Succeeded(_));
        let enabled = completed || (!busy && open.form.input_error().is_none());
        let field =
            |id: &'static str, which: Field, label: Text, text: &str, cx: &mut Context<Self>| {
                div()
                    .flex()
                    .flex_col()
                    .gap(theme::space::CARD_GAP)
                    .child(
                        div()
                            .text_color(rgb(theme.text.secondary))
                            .child(ui.text(label)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(theme::space::CARD_GAP)
                            .child(
                                div()
                                    .id(id)
                                    .flex_1()
                                    .min_w_0()
                                    .h(theme::space::MENU_ROW)
                                    .px(theme::space::BUTTON_SIDE)
                                    .flex()
                                    .items_center()
                                    .rounded(theme::radius::ROW)
                                    .bg(rgb(theme.surface.inset))
                                    .border_1()
                                    .border_color(rgb(if open.field == which {
                                        theme.accent.patina
                                    } else {
                                        theme.surface.divider
                                    }))
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .child(
                                        crate::text_field::TextField::new(
                                            id,
                                            text,
                                            cx.listener(move |root, value: &String, _, cx| {
                                                if let Some(open) = root.clone_project.as_mut() {
                                                    match which {
                                                        Field::Url => {
                                                            open.form.set_url(value.clone())
                                                        }
                                                        Field::Directory => {
                                                            open.form.set_directory(value.clone())
                                                        }
                                                        _ => {}
                                                    }
                                                    open.problem = open.form.input_error();
                                                }
                                                cx.notify();
                                            }),
                                        )
                                        .disabled(busy || completed)
                                        .when(open.field == which, |input| input.focus(&open.focus))
                                        .on_focus(cx.listener(move |root, _, _, cx| {
                                            if let Some(open) = root.clone_project.as_mut() {
                                                open.field = which;
                                            }
                                            cx.notify();
                                        }))
                                        .on_key(
                                            cx.listener(|root, event, window, cx| {
                                                root.clone_project_key(event, window, cx);
                                            }),
                                        ),
                                    ),
                            )
                            .when(which == Field::Directory, |row| {
                                row.child(
                                    div()
                                        .id("clone-choose")
                                        .flex_shrink_0()
                                        .px(theme::space::BUTTON_SIDE)
                                        .h(theme::space::MENU_ROW)
                                        .flex()
                                        .items_center()
                                        .rounded(theme::radius::ROW)
                                        .bg(rgb(theme.surface.raised))
                                        .border_1()
                                        .border_color(rgb(if open.field == Field::Choose {
                                            theme.accent.patina
                                        } else {
                                            theme.surface.divider
                                        }))
                                        .text_color(rgb(if busy || completed {
                                            theme.text.muted
                                        } else {
                                            theme.text.primary
                                        }))
                                        .child(ui.text(Text::CloneChoose))
                                        .when(!busy && !completed, |button| {
                                            button.cursor_pointer().on_click(cx.listener(
                                                |root, _, window, cx| {
                                                    root.clone_project_choose(window, cx)
                                                },
                                            ))
                                        }),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.text.secondary))
                            .child(ui.text(if which == Field::Url {
                                Text::CloneHelp
                            } else {
                                Text::CloneDirectoryHelp
                            })),
                    )
            };
        let status = match &open.phase {
            Phase::Editing => Text::CloneHelp,
            Phase::Running => Text::CloneRunning,
            Phase::Cancelling => Text::CloneCancelling,
            Phase::Failed { .. } => Text::CloneFailed,
            Phase::Cancelled { .. } => Text::CloneCancelled,
            Phase::Succeeded(_) => Text::CloneComplete,
        };
        let retained = match &open.phase {
            Phase::Failed { retained, .. } | Phase::Cancelled { retained } => retained.as_ref(),
            Phase::Succeeded(path) => Some(path),
            _ => None,
        };
        let problem = open.problem.as_ref().or(match &open.phase {
            Phase::Failed { message, .. } => Some(message),
            _ => None,
        });
        let card = super::popup::card_look(theme)
            .id("clone-project-form")
            .track_focus(&open.focus)
            .key_context("clone-project")
            .occlude()
            .flex()
            .flex_col()
            .gap(theme::space::CARD_INNER_GAP)
            .w(theme::space::START_COLUMN)
            .max_w_full()
            .mt(theme::space::RECENT_FILES_TOP)
            .p(theme::space::PANEL_PADDING)
            .text_size(theme::size::UI)
            .text_color(rgb(theme.text.primary))
            .child(
                div()
                    .text_size(theme::size::BODY)
                    .child(ui.text(Text::CloneTitle)),
            )
            .child(field(
                "clone-url",
                Field::Url,
                Text::CloneUrl,
                &open.form.url,
                cx,
            ))
            .child(field(
                "clone-directory",
                Field::Directory,
                Text::CloneDirectory,
                &open.form.directory,
                cx,
            ))
            .when(
                !matches!(open.phase, Phase::Editing) || problem.is_some(),
                |card| {
                    card.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(theme::space::CARD_GAP)
                            .p(theme::space::BUTTON_SIDE)
                            .bg(rgb(theme.surface.inset))
                            .border_l_2()
                            .border_color(rgb(if failed || problem.is_some() {
                                theme.accent.rust
                            } else {
                                theme.accent.patina
                            }))
                            .when(!matches!(open.phase, Phase::Editing), |block| {
                                block.child(
                                    div()
                                        .text_color(rgb(if failed {
                                            theme.accent.rust
                                        } else {
                                            theme.text.primary
                                        }))
                                        .child(ui.text(status)),
                                )
                            })
                            .when(failed, |block| {
                                block.child(div().child(ui.text(Text::CloneFailureHelp)))
                            })
                            .children(open.problem.as_ref().map(|message| {
                                div()
                                    .text_color(rgb(theme.accent.rust))
                                    .child(message.clone())
                            }))
                            .children(retained.map(|path| {
                                div().text_color(rgb(theme.text.secondary)).child(format!(
                                    "{} {}",
                                    ui.text(Text::CloneRetained),
                                    path.display()
                                ))
                            }))
                            .when(busy && !open.diagnostics.is_empty(), |block| {
                                block.child(
                                    div()
                                        .text_size(theme::size::META)
                                        .text_color(rgb(theme.text.secondary))
                                        .child(
                                            open.diagnostics
                                                .lines()
                                                .last()
                                                .unwrap_or_default()
                                                .to_owned(),
                                        ),
                                )
                            })
                            .when(!open.diagnostics.is_empty() || failed, |block| {
                                block
                                    .child(
                                        div()
                                            .id("clone-details-toggle")
                                            .cursor_pointer()
                                            .flex()
                                            .items_center()
                                            .gap(theme::space::TREE_INSET)
                                            .text_color(rgb(theme.text.secondary))
                                            .child(crate::icons::inline_chevron(
                                                open.details,
                                                theme::size::META,
                                                theme.text.secondary,
                                            ))
                                            .child(ui.text(Text::CloneDetails))
                                            .on_click(cx.listener(|root, _, _, cx| {
                                                if let Some(open) = root.clone_project.as_mut() {
                                                    open.details = !open.details;
                                                }
                                                cx.notify();
                                            })),
                                    )
                                    .when(open.details, |block| {
                                        block.child(
                                            div()
                                                .id("clone-details")
                                                .max_h(theme::space::PREVIEW)
                                                .overflow_y_scroll()
                                                .font_family(self.code_font.clone())
                                                .text_size(theme::size::META)
                                                .children(match &open.phase {
                                                    Phase::Failed { message, .. } => {
                                                        Some(div().child(message.clone()))
                                                    }
                                                    _ => None,
                                                })
                                                .child(div().child(open.diagnostics.clone())),
                                        )
                                    })
                            }),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .pt(theme::space::BUTTON_SIDE)
                    .border_t_1()
                    .border_color(rgb(theme.surface.divider))
                    .gap(theme::space::CARD_GAP)
                    .when(busy, |row| {
                        row.child(
                            div()
                                .id("clone-hide")
                                .cursor_pointer()
                                .child(ui.text(Text::CloneHide))
                                .on_click(cx.listener(|root, _, window, cx| {
                                    root.hide_clone_project(window, cx)
                                })),
                        )
                    })
                    .child(
                        div()
                            .id("clone-cancel")
                            .cursor_pointer()
                            .px(theme::space::BUTTON_SIDE)
                            .h(theme::space::BUTTON)
                            .flex()
                            .items_center()
                            .rounded(theme::radius::ROW)
                            .bg(rgb(if open.field == Field::Cancel {
                                theme.surface.raised
                            } else {
                                theme.surface.card
                            }))
                            .child(ui.text(Text::Cancel))
                            .on_click(cx.listener(|root, _, window, cx| {
                                root.cancel_clone_project(window, cx)
                            })),
                    )
                    .when(!busy, |row| {
                        row.child(
                            div()
                                .id("clone-submit")
                                .cursor_pointer()
                                .px(theme::space::BUTTON_SIDE)
                                .h(theme::space::BUTTON)
                                .flex()
                                .items_center()
                                .rounded(theme::radius::ROW)
                                .bg(rgb(if enabled {
                                    theme.accent.brass
                                } else {
                                    theme.surface.raised
                                }))
                                .text_color(rgb(if enabled {
                                    theme.surface.app
                                } else {
                                    theme.text.muted
                                }))
                                .border_1()
                                .border_color(rgb(if open.field == Field::Submit {
                                    theme.accent.patina
                                } else {
                                    theme.surface.card
                                }))
                                .child(ui.text(if completed {
                                    Text::CloneOpen
                                } else {
                                    Text::CloneStart
                                }))
                                .on_click(cx.listener(move |root, _, _, cx| {
                                    if enabled {
                                        root.start_clone_project(cx);
                                    }
                                })),
                        )
                    }),
            );
        Some(
            deferred(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .justify_center()
                    .items_start()
                    .child(
                        div()
                            .id("clone-backdrop")
                            .absolute()
                            .size_full()
                            .occlude()
                            .bg(super::goto_line::dim(theme.surface.backdrop))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|root, _, window, cx| {
                                    cx.stop_propagation();
                                    root.hide_clone_project(window, cx);
                                }),
                            ),
                    )
                    .child(card),
            )
            .priority(2)
            .into_any_element(),
        )
    }
}
