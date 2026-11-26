//! The strip along the bottom: where you are, in one line.
//!
//! Not a panel. It never closes and has no shortcut, because a control that
//! hides the thing telling you where you are is a control nobody wants
//! twice.
//!
//! Since M8.3.1 it also holds the tools' buttons, each lit while its dock
//! is open on it — section 6.11. A button is the one mouse route to a
//! tool, and the handle it is dragged by. Buttons follow saved dock assignment:
//! left, bottom centered and right. File and server facts remain at the right.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, div, rgb};
use gpui_component::Sizable;
use guild_session::Tool;

use crate::Body;
use crate::actions::keymap::Named;
use crate::highlight::LineEnding;
use crate::i18n::Text;
use crate::view::drag::{DragPreview, DragTool};
use crate::{Root, icons, theme};

impl Root {
    pub(crate) fn render_status(
        &mut self,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        let tools_bottom = theme::space::STATUS_BOTTOM;
        let origin = if self.settings.column_folded {
            0.0
        } else {
            self.settings.column_width
        };
        let geometry = self.panel_geometry().relative_to(origin);
        let metric_scale = self.settings.ui_font_size / crate::scale::BASE_UI_FONT_SIZE;
        let intrinsic = f32::from(self.status_tools_width.get()) * metric_scale;
        let gap = crate::frame::icon_gap(
            self.settings.ui_font_size,
            geometry.work_width,
            intrinsic,
            f32::from(self.status_right_width.get()) * metric_scale,
        );
        // What the server's line may take: what the centred tools and the
        // stable right side leave, both measured on the last frame.
        let tools_width = intrinsic + self.footer_tools_count().saturating_sub(1) as f32 * gap;
        let tools_right = geometry.center() + tools_width / 2.0;
        let bar_width = f32::from(self.viewport.width) - origin;
        let server_room = crate::frame::server_room(
            self.settings.ui_font_size,
            bar_width,
            tools_right,
            f32::from(self.status_right_width.get()) * metric_scale,
        );
        let right_room =
            crate::frame::right_room(self.settings.ui_font_size, bar_width, tools_right);
        let bar = div()
            .id("window-footer-bar")
            .debug_selector(|| "window-footer-bar".into())
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::space::PANEL_PADDING)
            .h(theme::space::STATUS_BAR)
            // Larger targets fit the unchanged footer with a smaller bottom inset.
            .pl(gpui::px(
                geometry.column + self.rem_px(theme::space::WORK_INSET),
            ))
            .pr(theme::space::WORK_INSET)
            .pb(theme::space::STATUS_BOTTOM)
            // **The same ground as the frame, and a hairline above it.**
            // It was `surface.sunken` — a third grey under a strip that
            // does the same job as the panels beside it, and it read as
            // something bolted on. M12 task 12.
            .bg(rgb(theme.surface.panel))
            // Under the cards on the window's colour, nothing below it —
            // section 4.1, M12.6. No hairline: the card above ends where
            // the status bar begins.
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.faint))
            // Branches follow the leftmost card; bottom tools are centered separately.
            .child(
                div()
                    .debug_selector(|| "footer-left-tools".into())
                    .children(self.render_tools(
                        self.footer_tools(guild_session::Dock::Left),
                        gap,
                        cx,
                    )),
            )
            .child(
                div().flex().flex_1().justify_end().items_center().child(
                    div()
                        .relative()
                        .debug_selector(|| "footer-right-tools".into())
                        .flex()
                        .flex_none()
                        .min_w_0()
                        .max_w(gpui::px(right_room))
                        // What does not fit leaves on the left, where the
                        // server's line shrinks first: the stable side stays
                        // at the inset, where the minimum keeps room for it.
                        .justify_end()
                        .items_center()
                        .gap(theme::space::PANEL_PADDING)
                        .children(self.render_server_state(server_room, window, cx))
                        // The active worktree's merge request — GLD-063:
                        // shrinks with the server's line, never measured.
                        .children(self.render_merge_request_status())
                        // Only what stays is measured for the window's
                        // minimum: the server's line comes and goes on its
                        // own, and gives way instead — `frame::server_room`.
                        .child(
                            div()
                                .relative()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(theme::space::PANEL_PADDING)
                                .children(self.render_file_facts(cx))
                                .children(self.render_tools(
                                    self.footer_tools(guild_session::Dock::Right),
                                    gap,
                                    cx,
                                ))
                                .child({
                                    let width = self.status_right_width.clone();
                                    let root = cx.entity().downgrade();
                                    gpui::canvas(
                                        move |bounds, _, cx| {
                                            let measured = bounds.size.width / metric_scale;
                                            if width.get() != measured {
                                                width.set(measured);
                                                let root = root.clone();
                                                cx.defer(move |cx| {
                                                    let _ = root.update(cx, |_, cx| cx.notify());
                                                });
                                            }
                                        },
                                        |_, _, _, _| {},
                                    )
                                    .absolute()
                                    .size_full()
                                }),
                        ),
                ),
            )
            .into_any_element();
        div()
            .relative()
            .flex()
            .flex_col()
            .flex_none()
            .child(bar)
            .child(
                div()
                    .absolute()
                    .left(gpui::px(geometry.column))
                    .w(gpui::px(geometry.work_width))
                    .bottom(tools_bottom)
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .relative()
                            .flex()
                            .flex_none()
                            .debug_selector(|| "footer-centered-tools".into())
                            .children(self.render_centered_tools(gap, cx))
                            .child({
                                let width = self.status_tools_width.clone();
                                let root = cx.entity().downgrade();
                                let gaps = self.footer_tools_count().saturating_sub(1) as f32 * gap;
                                gpui::canvas(
                                    move |bounds, _, cx| {
                                        let intrinsic =
                                            (bounds.size.width - gpui::px(gaps)) / metric_scale;
                                        if width.get() != intrinsic {
                                            width.set(intrinsic);
                                            let root = root.clone();
                                            cx.defer(move |cx| {
                                                let _ = root.update(cx, |_, cx| cx.notify());
                                            });
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full()
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn footer_text_width(
        &self,
        text: &str,
        size: gpui::Rems,
        window: &gpui::Window,
    ) -> f32 {
        f32::from(
            window
                .text_system()
                .shape_line(
                    text.to_owned().into(),
                    gpui::px(self.rem_px(size)),
                    &[gpui::TextRun {
                        len: text.len(),
                        font: gpui::font(
                            self.settings
                                .ui_font
                                .clone()
                                .unwrap_or_else(|| crate::fonts::DEFAULT.ui.to_owned()),
                        ),
                        color: rgb(self.theme.text.tertiary).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                )
                .width,
        )
    }

    /// The active worktree's MR on the right — GLD-063, the approved
    /// *Status bar* board: the hover card's first failing line and
    /// `!N ↗`, both in the verdict's colour; `!N ↗` opens the MR. No
    /// action button.
    pub(crate) fn render_merge_request_status(&self) -> Option<gpui::AnyElement> {
        let path = self
            .active_project
            .and_then(|project| self.desk.project(project))
            .map(|project| project.path.clone())?;
        let crate::forge::Shown::Mr { mr, .. } = self.forge_shown(&path) else {
            return None;
        };
        let theme = self.theme;
        let ui = self.ui();
        let colour = crate::view::forge::verdict_colour(
            guild_session::merge_requests::status::verdict(mr),
            theme,
        );
        let line = guild_session::merge_requests::status::first_failing(mr)
            .map(|line| ui.mr_status_line(&line.check));
        let url = mr.web_url.clone();
        Some(
            div()
                .debug_selector(|| "footer-merge-request".into())
                .flex()
                .min_w_0()
                .items_center()
                .gap(theme::space::PANEL_PADDING)
                .text_color(rgb(colour))
                .when_some(line, |status, line| {
                    status.child(div().min_w_0().truncate().child(line))
                })
                .child(
                    div()
                        .id("footer-merge-request-link")
                        .debug_selector(|| "footer-merge-request-link".into())
                        .flex_none()
                        .cursor_pointer()
                        .child(format!("!{} ↗", mr.iid))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_current_branch(
        &self,
        label_width: f32,
        cx: &mut Context<Root>,
    ) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .flex()
            .flex_none()
            .items_center()
            .when_some(self.branch.clone(), |bar, branch| {
                // The branch is not decoration here: a task carries one,
                // parallel tasks are isolated by worktree, and review is
                // read against it. Which one is checked out is something
                // to know at a glance rather than to ask a terminal for.
                //
                // Since M8.3.3 it is also the way in: a click opens the
                // popup with the branches and the stash, which hangs
                // from this very element — `anchored` inside a
                // `deferred`, so nobody has to measure where the label
                // ended up after a project name of any length.
                bar.child(
                    div()
                        .id("branch-button")
                        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .debug_selector(|| "branch-button".into())
                        .flex()
                        .items_center()
                        .gap(theme::space::STATUS_BUTTON_GAP)
                        .h(theme::space::TITLE_CHIP)
                        // Back's round stroke reaches x=4/24 of its SVG.
                        // Compensate the shaped branch glyph's own left bearing.
                        .pl(gpui::px(
                            (self.rem_px(theme::space::STATUS_BUTTON)
                                - self.rem_px(theme::space::HEAD_ICON))
                                / 2.0
                                + self.rem_px(theme::space::HEAD_ICON) * 4.0 / 24.0
                                - self.rem_px(theme::size::META) * BRANCH_INK,
                        ))
                        .pr(theme::space::WORK_INSET)
                        .rounded(theme::radius::ROW)
                        .cursor_pointer()
                        .hover(|button| {
                            button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                        })
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.tertiary))
                        .child(
                            crate::icons::glyph("ui/workspace-git-branch.svg", theme::size::META)
                                .debug_selector(|| "branch-glyph".into())
                                .text_color(rgb(theme.text.tertiary)),
                        )
                        .child(div().max_w(gpui::px(label_width)).truncate().child(branch))
                        .child(crate::icons::inline_chevron(
                            true,
                            theme::size::EYEBROW,
                            theme.text.tertiary,
                        ))
                        .on_click(cx.listener(|root, _event, window, cx| {
                            // The bar above turns a double click into a
                            // zoom; the project button says why this is
                            // here.
                            cx.stop_propagation();
                            root.toggle_branch_popup(window, cx);
                        }))
                        // Where it ended up, for the popup that hangs
                        // from it: a canvas rather than an update,
                        // because prepaint is inside this very frame.
                        .child({
                            let at = self.branch_at.clone();
                            gpui::canvas(
                                move |bounds, _window, _cx| {
                                    at.set(gpui::point(bounds.left(), bounds.bottom()))
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .size_full()
                        }),
                )
            })
            .into_any_element()
    }

    /// The title bar's ↓: a click, or Enter, Space or an arrow on it, opens
    /// the source popup hanging under it, where a source is chosen and
    /// then confirmed. Its hint is one short line; auto fast-forward's
    /// last skip of this branch adds the one line its spec asks for.
    pub(crate) fn render_branch_update(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = self.ui();
        let open = self.branch_face() == Some(crate::root::branches::Face::Sources);
        let busy = self
            .active_project
            .is_some_and(|p| self.updating_branches.contains_key(&p));
        let title = ui.text(if busy {
            Text::UpdateBranchRunning
        } else {
            Text::UpdateBranch
        });
        // Auto fast-forward's last skip of this branch, where Update branch
        // already explains itself — decision 21.
        let skipped = self.auto_ff_skip_line(crate::root::auto_ff::Surface::Hint);
        let at = self.update_branch_at.clone();
        div()
            .id("update-branch")
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .debug_selector(|| "update-branch".into())
            .relative()
            .flex_none()
            // Where it ended up, for the popup that hangs from it: a
            // canvas, because prepaint is inside this very frame — and a
            // keyboard-opened popup has its anchor before it is drawn.
            .child(
                gpui::canvas(
                    move |bounds, _, _| at.set(gpui::point(bounds.left(), bounds.bottom())),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .track_focus(&self.update_branch_focus)
            .tab_index(0)
            .flex()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .when(!busy, |button| button.cursor_pointer())
            .hover(|s| s.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .when(open, |button| {
                button.bg(crate::theme::control_hover(theme, theme.surface.panel))
            })
            .text_color(rgb(theme.text.tertiary))
            .when_else(
                busy,
                |button| button.child(gpui_component::spinner::Spinner::new().small()),
                |button| {
                    button.child(
                        icons::toolbar("ui/arrow-down.svg").text_color(rgb(theme.text.tertiary)),
                    )
                },
            )
            .when(!open, |button| {
                button.hint_delay().tooltip(move |_, app| {
                    app.new(|_| match &skipped {
                        Some(skipped) => crate::view::tooltip::Tooltip::details(
                            format!("{title}\n{skipped}"),
                            theme,
                        ),
                        None => crate::view::tooltip::Tooltip::text(title, theme),
                    })
                    .into()
                })
            })
            .on_click(cx.listener(|root, _, window, cx| {
                cx.stop_propagation();
                root.open_update_sources(window, cx);
            }))
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if matches!(
                    event.keystroke.key.as_str(),
                    "enter" | "space" | "up" | "down"
                ) {
                    root.open_update_sources(window, cx);
                    cx.stop_propagation();
                }
            }))
            .into_any_element()
    }

    /// What the server of the file in front is doing, when that is worth
    /// saying — *server starting…*, *Indexing*, *cargo check*.
    ///
    /// **A language with no server says nothing here.** It said *no
    /// server* for every Go, Kotlin and TypeScript file, for ever — a
    /// fact about the language rather than about this moment, and one
    /// the bar repeated a hundred times a day. It belongs where a person
    /// asks for something a server would have answered, and `f12` says
    /// it there.
    ///
    /// A server that is listening and quiet also says nothing: a person
    /// does not need to be told that things are as they should be.
    fn render_server_state(
        &self,
        room: f32,
        window: &gpui::Window,
        cx: &mut Context<Root>,
    ) -> Option<gpui::AnyElement> {
        use crate::root::lsp::ServerState;
        let theme = self.theme;
        let ui = self.ui();
        let (project, path) = self.file_in_front()?;
        let language = crate::highlight::Language::of(&path)?;
        let state = self.server_state(project, &path);
        let faint = rgb(theme.text.faint);

        // **Something that moves, M8.5.1.** A grey word alone was not read
        // as work under way; a wheel turns beside it.
        let spinner = || {
            use gpui_component::Sizable as _;
            gpui_component::spinner::Spinner::new()
                .with_size(gpui_component::Size::Size(gpui::px(
                    theme::space::STATUS_ICON.0
                        * f32::from(crate::scale::rem_size(self.settings.ui_font_size)),
                )))
                .color(faint.into())
        };
        // The room is the window's, not the line's: the line gives way to
        // it — `frame::server_parts` — and says the whole of itself in the
        // tooltip when anything was cut.
        let measure =
            |text: &str| self.footer_text_width(&one_line(text), theme::size::META, window);
        let step = self.rem_px(theme::space::STATUS_BUTTON_GAP);
        let pad = 2.0 * self.rem_px(theme::space::CARD_GAP);
        // Whether a line of one text, after its lead, is cut by the room.
        // A line broken in the server's words is joined on the bar, so its
        // hint says it as it was, fitting or not.
        let cut = |lead: f32, text: &str| {
            text.contains(['\r', '\n']) || pad + lead + step + measure(text) > room
        };
        let (row, whole) = match state {
            ServerState::None | ServerState::Ready | ServerState::Disabled => return None,
            ServerState::IdentityPending => {
                let text = ui.text(Text::ScopedIdentityPending);
                (
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.text.faint))
                        .child(text),
                    cut(-step, text).then(|| text.to_owned()),
                )
            }
            ServerState::IdentityFailed(_) => {
                let text = ui.text(Text::ScopedUnavailable);
                (
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(theme.accent.rust))
                        .child(text),
                    cut(-step, text).then(|| text.to_owned()),
                )
            }
            ServerState::Starting => {
                let text = ui.text(Text::ServerStarting);
                (
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(theme::space::STATUS_BUTTON_GAP)
                        .child(div().flex_none().child(spinner()))
                        .child(div().min_w_0().truncate().child(text.to_owned())),
                    cut(self.rem_px(theme::space::STATUS_ICON), text).then(|| text.to_owned()),
                )
            }
            ServerState::Doing(line) => {
                let percent = line.percentage.map(|p| format!("{p}%"));
                let more = (line.more > 0).then(|| format!("+{}", line.more));
                let dot = 2.0 * step + measure("·");
                // A server's own words may run over lines; the bar is one.
                let (title, detail) = (one_line(&line.title), line.detail.as_deref().map(one_line));
                let capped = detail.as_deref().is_some_and(|detail| {
                    measure(detail) > self.rem_px(theme::space::SERVER_DETAIL)
                });
                let fit = crate::frame::server_parts(
                    room,
                    crate::frame::ServerLine {
                        frame: pad + self.rem_px(theme::space::STATUS_ICON),
                        title: step + measure(&title),
                        // A detail cut to a bare ellipsis says nothing:
                        // the least worth drawing is its first few letters.
                        detail: detail.as_deref().map_or(0.0, |detail| {
                            let least = detail.chars().take(4).collect::<String>() + "…";
                            dot + measure(detail).min(measure(&least))
                        }),
                        detail_full: detail.as_deref().map_or(0.0, |detail| {
                            dot + measure(detail).min(self.rem_px(theme::space::SERVER_DETAIL))
                        }),
                        more: more.as_deref().map_or(0.0, |more| step + measure(more)),
                        bar: if percent.is_some() {
                            step + self.rem_px(theme::space::PROGRESS_WIDTH)
                        } else {
                            0.0
                        },
                        percent: percent.as_deref().map_or(0.0, |p| step + measure(p)),
                    },
                );
                let broken = line.title.contains(['\r', '\n'])
                    || line
                        .detail
                        .as_deref()
                        .is_some_and(|d| d.contains(['\r', '\n']));
                let whole = (fit.cut || capped && fit.detail || broken).then(|| {
                    [
                        Some(line.title.clone()),
                        line.detail.as_ref().map(|detail| format!("· {detail}")),
                        percent.clone(),
                        more.clone(),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ")
                });
                (
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(theme::space::STATUS_BUTTON_GAP)
                        .child(div().flex_none().child(spinner()))
                        .when(fit.title, |row| {
                            // The detail is what shrinks; a title alone is
                            // cut only when it is all the line has to say.
                            row.child(if fit.detail {
                                div().flex_none().child(title)
                            } else {
                                div().min_w_0().truncate().child(title)
                            })
                        })
                        // A dot between them: *Building mix compiling: …* read as
                        // one sentence in the window.
                        .children(detail.filter(|_| fit.detail).map(|detail| {
                            div()
                                .flex()
                                .min_w_0()
                                .gap(theme::space::STATUS_BUTTON_GAP)
                                .child(div().flex_none().child("·"))
                                .child(
                                    div()
                                        .min_w_0()
                                        .max_w(theme::space::SERVER_DETAIL)
                                        .truncate()
                                        .child(detail),
                                )
                        }))
                        .children(percent.map(|percent| {
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(theme::space::STATUS_BUTTON_GAP)
                                .when(fit.bar, |group| {
                                    group.child(
                                        gpui_component::progress::Progress::new("server-progress")
                                            .value(line.percentage.unwrap_or_default() as f32)
                                            .bg(rgb(theme.accent.patina))
                                            .w(theme::space::PROGRESS_WIDTH)
                                            .h(theme::space::PROGRESS_HEIGHT),
                                    )
                                })
                                .child(percent)
                        }))
                        .children(
                            more.filter(|_| fit.more)
                                .map(|more| div().flex_none().child(more)),
                        ),
                    whole,
                )
            }
            // **A fault is drawn in rust**, and names the command: a
            // server that is not there must not look like a server that
            // is ready, which is what saying nothing did.
            ServerState::Failed(fault) => {
                let text =
                    ui.server_fault(language.server.first().copied().unwrap_or_default(), &fault);
                let whole = cut(self.rem_px(theme::space::DISC), &text).then(|| text.clone());
                (
                    div()
                        .flex()
                        .min_w_0()
                        .items_center()
                        .gap(theme::space::STATUS_BUTTON_GAP)
                        .child(
                            div()
                                .debug_selector(|| "failed-server-disc".into())
                                .flex_none()
                                .size(theme::space::DISC)
                                .rounded_full()
                                .bg(rgb(theme.accent.rust)),
                        )
                        .child(div().min_w_0().truncate().child(one_line(&text))),
                    whole,
                )
            }
        };

        let tooltip = ui.text(Text::ServersTooltip).to_owned();
        let tooltip = match whole {
            Some(whole) => format!("{tooltip}\n{whole}"),
            None => tooltip,
        };
        let at = self.server_state_at.clone();
        Some(
            div()
                .id("server-state")
                .debug_selector(|| "server-state".into())
                .flex()
                .flex_shrink(1.)
                .min_w_0()
                .max_w(gpui::px(room))
                .overflow_hidden()
                .px(theme::space::CARD_GAP)
                .rounded(theme::radius::ROW)
                .cursor_pointer()
                .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                .text_color(faint)
                .child(row)
                // Not over the list it opened: the tip sat on the card's
                // last row in the window.
                .when(!self.showing_servers, |button| {
                    button.hint_delay().tooltip(move |_window, app| {
                        app.new(|_cx| {
                            if tooltip.contains('\n') {
                                crate::view::tooltip::Tooltip::details(tooltip.clone(), theme)
                            } else {
                                crate::view::tooltip::Tooltip::text(tooltip.clone(), theme)
                            }
                        })
                        .into()
                    })
                })
                .relative()
                .child(
                    gpui::canvas(move |bounds, _window, _cx| at.set(bounds), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .on_click(cx.listener(|root, _event, _window, cx| {
                    cx.stop_propagation();
                    root.toggle_servers(cx);
                }))
                .into_any_element(),
        )
    }

    /// The three facts about the file in front: the language it was
    /// taken for, how its lines end, and where the caret is.
    ///
    /// **Nothing at all when the tab in front is not a file.** A bar
    /// that goes on saying `1:1` over a terminal is answering a question
    /// nobody asked; a widget appears only where it applies.
    ///
    /// The language is here because it is the only way to notice that a
    /// file was taken for the *wrong* one — `lock` is read by four
    /// different grammars and `.ini` by TOML's. The endings are a
    /// **label and not a menu**: changing a file's separators is a write
    /// to that file, and nobody asked for one.
    fn render_file_facts(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let theme = self.theme;
        let ui = self.ui();
        let Some(Body::File { tab, .. }) = self.active_body() else {
            return None;
        };
        // **An ordinary read.** What took the window down for the diff
        // header was `cx.entity().read(cx)` — an entity reading *itself*
        // mid-update. This is `Root`'s render reading a different
        // entity, which is what `root::lsp` already does.
        let wrap_tab = tab.clone();
        let file = tab.read(cx);
        let wrapped = file.soft_wrap();
        let can_wrap =
            file.editor().is_some() && (file.image_zoom().is_none() || file.image_source_visible());
        let language = file.language().map_or_else(
            || ui.text(Text::LanguagePlain).to_owned(),
            |language| language.name.to_owned(),
        );
        let ending = match file.line_ending() {
            LineEnding::Lf => "LF",
            LineEnding::Crlf => "CRLF",
        };
        // The component counts from zero and every editor a person has
        // used counts from one. `52:11` is the caret's
        // character rather than its byte, which is what `cursor_position`
        // answers.
        let caret = file.editor().map(|editor| {
            let at = editor.read(cx).cursor_position();
            format!("{}:{}", at.line + 1, at.character + 1)
        });

        Some(
            div()
                .debug_selector(|| "file-facts".into())
                .flex()
                .flex_none()
                .items_center()
                .gap(theme::space::STATUS_GROUP_GAP)
                .when(can_wrap, |bar| {
                    bar.child(
                        div()
                            .id("file-soft-wrap")
                            .debug_selector(|| "file-soft-wrap".into())
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .size(theme::space::STATUS_BUTTON)
                            .rounded(theme::radius::STATUS_BUTTON)
                            .cursor_pointer()
                            .when(wrapped, |b| b.bg(rgb(theme.surface.raised)))
                            .text_color(rgb(if wrapped {
                                theme.text.primary
                            } else {
                                theme.text.tertiary
                            }))
                            .hover(|b| {
                                b.bg(crate::theme::control_hover(theme, theme.surface.panel))
                            })
                            .child("↩")
                            .on_click(cx.listener(move |_root, _, window, cx| {
                                cx.stop_propagation();
                                wrap_tab.update(cx, |file, cx| file.toggle_soft_wrap(window, cx));
                                cx.notify();
                            }))
                            .hint_delay()
                            .tooltip(move |_, app| {
                                app.new(|_| {
                                    crate::view::tooltip::Tooltip::text(
                                        ui.text(if wrapped {
                                            Text::SoftWrapOn
                                        } else {
                                            Text::SoftWrapOff
                                        }),
                                        theme,
                                    )
                                })
                                .into()
                            }),
                    )
                })
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(theme.text.muted))
                        .child(language),
                )
                .child(
                    div()
                        .id("line-endings")
                        .flex_none()
                        .child(ending)
                        .hint_delay()
                        .tooltip(move |_window, app| {
                            app.new(|_cx| {
                                crate::view::tooltip::Tooltip::text(
                                    ui.text(Text::LineEndings),
                                    theme,
                                )
                            })
                            .into()
                        }),
                )
                .children(caret.map(|caret| {
                    div()
                        .id("caret-place")
                        .flex_none()
                        // Wide enough for four digits and a colon, and
                        // fixed: a widget that grows a digit at line 100
                        // would drag its neighbours along with it.
                        .min_w(theme::space::SEARCH_LINE_NUMBER)
                        .flex()
                        .justify_end()
                        .cursor_pointer()
                        .child(caret)
                        // **The two halves meet through the keymap, not
                        // through shared code.** M8.8.2 owns the jump;
                        // what this widget does is press the same key a
                        // person would have pressed.
                        .on_click(cx.listener(|_root, _event, window, cx| {
                            cx.stop_propagation();
                            window.dispatch_action(Box::new(crate::actions::GoToLine), cx);
                        }))
                        .hint_delay()
                        .tooltip(move |_window, app| {
                            app.new(|_cx| {
                                crate::view::tooltip::Tooltip::for_action(
                                    ui.text(Text::GoToLine),
                                    Named::GoToLine,
                                    theme,
                                )
                            })
                            .into()
                        })
                }))
                .into_any_element(),
        )
    }
}

impl Root {
    /// Saved dock order owns footer placement; visibility remains the tool's rule.
    pub(crate) fn footer_tools(&self, dock: guild_session::Dock) -> Vec<Tool> {
        self.docks()
            .tools_in(dock)
            .into_iter()
            .filter(|tool| {
                !Tool::RETIRED.contains(tool) && (!tool.on_demand() || self.tool_is_up(*tool))
            })
            .collect()
    }

    fn footer_tools_count(&self) -> usize {
        self.footer_tools(guild_session::Dock::Bottom).len()
    }

    fn render_centered_tools(&self, gap: f32, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        self.render_tools(self.footer_tools(guild_session::Dock::Bottom), gap, cx)
    }

    /// Every footer placement shares the tool's action, state, tooltip and drag handle.
    fn render_tools(
        &self,
        tools: Vec<Tool>,
        gap: f32,
        cx: &mut Context<Root>,
    ) -> Option<gpui::AnyElement> {
        if tools.is_empty() {
            return None;
        }
        let theme = self.theme;
        let ui = self.ui();
        // **Read once for the whole row, not per button.** Only the
        // Problems button wants it, and only when that tool is in this
        // dock — but the alternative is a walk of the diagnostics map
        // inside a closure that runs for every tool of every dock.
        let (errors, warnings) = if tools.contains(&Tool::Problems) {
            crate::root::problems::counts(&self.problems().0)
        } else {
            (0, 0)
        };
        Some(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(gpui::px(gap))
                .justify_center()
                .children(tools.into_iter().map(|tool| {
                    let up = self.tool_is_up(tool);
                    // The name comes from the one place that has it, so
                    // the button and the head it opens agree; the action
                    // is the button's own, for the tooltip's stroke.
                    let label = crate::view::panels::title_of(tool);
                    let action = match tool {
                        Tool::Explorer => Named::ShowExplorerPanel,
                        Tool::Commit => Named::ShowCommitPanel,
                        Tool::Git => Named::ShowLogPanel,
                        Tool::Output => Named::ShowOutputPanel,
                        Tool::Terminal => Named::ShowTerminalPanel,
                        Tool::Search => Named::ShowSearchPanel,
                        Tool::Problems => Named::ShowProblems,
                        Tool::Push => Named::Push,
                        Tool::Find => Named::ShowFindPanel,
                    };
                    // **The Problems button *is* the count**, decision
                    // 14 — one control and not two. It is the project's
                    // number and not the file in front's, because that
                    // is what the panel behind it lists: a button saying
                    // *0* over a list of forty would be lying about what
                    // pressing it opens.
                    let counted = tool == Tool::Problems && errors + warnings > 0;
                    let said = counted.then(|| ui.diagnostics(errors, warnings)).flatten();
                    div()
                        .id(("tool-button", tool as usize))
                        .debug_selector(move || format!("footer-tool-{tool:?}"))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .gap(theme::space::STATUS_BUTTON_GAP)
                        .h(theme::space::STATUS_BUTTON)
                        // Square while it is only an icon, and grown by
                        // its padding once it carries numbers.
                        .when(!counted, |b| b.w(theme::space::STATUS_BUTTON))
                        .when(counted, |b| b.px(theme::space::CARD_GAP))
                        .rounded(theme::radius::STATUS_BUTTON)
                        .cursor_pointer()
                        .when(up, |b| b.bg(rgb(theme.surface.raised)))
                        .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                        .child(icons::tool(tool).text_color(rgb(if up {
                            theme.text.primary
                        } else {
                            theme.text.tertiary
                        })))
                        .children(counted.then(|| {
                            div()
                                .flex_none()
                                .text_size(theme::size::EYEBROW)
                                // Rust while anything is an error, brass
                                // when the worst of it is a warning: the
                                // two colours the squiggles already use.
                                .text_color(rgb(if errors > 0 {
                                    theme.accent.rust
                                } else {
                                    theme.accent.brass
                                }))
                                .child(if warnings > 0 && errors > 0 {
                                    format!("{errors} · {warnings}")
                                } else {
                                    (errors + warnings).to_string()
                                })
                        }))
                        .hint_delay()
                        .tooltip(move |_window, app| {
                            let label = match &said {
                                // The numbers on the face are two digits
                                // with a dot between them; the tooltip is
                                // where they are named.
                                Some(said) => format!("{} · {said}", ui.text(label)),
                                None => ui.text(label).to_owned(),
                            };
                            app.new(|_cx| {
                                crate::view::tooltip::Tooltip::for_action(label, action, theme)
                            })
                            .into()
                        })
                        .on_click(cx.listener(move |root, _event, _window, cx| {
                            cx.stop_propagation();
                            root.show_tool(tool, cx);
                        }))
                        // The handle a tool is dragged by: the button itself
                        // travels with the pointer. Section 6.15.
                        .on_drag(DragTool(tool), move |drag, _point, _window, cx| {
                            cx.new(|_| DragPreview {
                                tool: drag.0,
                                theme,
                            })
                        })
                }))
                .into_any_element(),
        )
    }
}

/// How far into its box the branch icon's ink begins: Lucide's
/// `git-branch` has its lower circle at x = 6 − 3, less half its stroke —
/// 2 of 24. The title bar's branch lines that ink up with Back's.
const BRANCH_INK: f32 = 2.0 / 24.0;

/// The most of a server's words the bar shapes: past it the line is cut by
/// the room anyway, and a message of megabytes would be copied and shaped
/// on every frame. The hint has all of it.
const ONE_LINE_CHARS: usize = 512;

/// A server's words on the status bar's one line: GPUI shapes a line
/// without breaks, and the hint keeps them.
fn one_line(text: &str) -> String {
    let head = match text.char_indices().nth(ONE_LINE_CHARS) {
        Some((end, _)) => &text[..end],
        None => text,
    };
    head.split(['\r', '\n'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
