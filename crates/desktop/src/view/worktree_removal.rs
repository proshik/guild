//! One modal confirmation shared by the column, menu and keyboard action.
use crate::{Root, i18n::Text, theme};
use gpui::{Context, FocusHandle, MouseButton, Window, div, prelude::*, rgb};

pub(crate) struct Confirmation {
    pub target: crate::git::worktree_removal::Target,
    pub delete_branch: bool,
    pub focus: [FocusHandle; 3], // checkbox, Cancel, Delete
    previous: Option<FocusHandle>,
}

impl Root {
    pub(crate) fn show_worktree_removal(
        &mut self,
        target: crate::git::worktree_removal::Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.put_fields_away();
        self.worktree_tooltips
            .update(cx, |overlay, cx| overlay.hide(cx));
        // Invalidate an already scheduled landing before the modal owns input.
        self.workspace_swipe_landing = self.workspace_swipe_landing.wrapping_add(1);
        self.workspace_swipe = crate::swipe::Detector::default();
        self.workspace_swipe_offset = 0.;
        let focus = [cx.focus_handle(), cx.focus_handle(), cx.focus_handle()];
        let previous = window.focused(cx);
        window.focus(&focus[1], cx); // a bare Return must not delete by accident
        self.worktree_removal = Some(Confirmation {
            delete_branch: target.can_delete_branch(),
            target,
            focus,
            previous,
        });
        cx.notify();
    }

    pub(crate) fn close_worktree_removal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(open) = self.worktree_removal.take() {
            if let Some(previous) = open.previous {
                window.focus(&previous, cx);
            } else {
                self.refocus(window, cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn confirm_worktree_removal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.worktree_removal else {
            return;
        };
        let target = open.target.clone();
        let delete_branch = open.delete_branch;
        let project = self
            .desk
            .projects()
            .find(|project| project.path == target.path)
            .map(|project| project.id);
        let primary = self
            .worktrees
            .values()
            .any(|list| list.first().is_some_and(|row| row.path == target.path));
        // Recheck even while a different workspace is in front. The worker
        // independently re-reads git identity/locks/dirty state before writing.
        if !self.worktree_removal_allowed(&target.path, project, primary, cx) {
            self.close_worktree_removal(window, cx);
            return;
        }
        let Some(runner) = self
            .removal_runner(&target.path)
            .filter(|id| self.status_workers.contains_key(id))
        else {
            self.warn(self.ui().text(Text::WorktreeRemovalUnavailable).to_owned());
            cx.notify();
            return;
        };
        self.close_worktree_removal(window, cx);
        if let Some(project) = project {
            self.close_worktree_now(project, cx);
        }
        let queued = self.status_workers.get(&runner).is_some_and(|worker| {
            worker.try_run(crate::git::Write::WorktreeRemoveConfirmed {
                target,
                delete_branch,
            })
        });
        if !queued {
            self.warn(self.ui().text(Text::WorktreeRemovalUnavailable).to_owned());
        }
        cx.notify();
    }

    /// Trap keys in the modal before the editor or terminal can bind them.
    pub(crate) fn worktree_removal_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(open) = &self.worktree_removal else {
            return false;
        };
        let at = open
            .focus
            .iter()
            .position(|focus| focus.is_focused(window))
            .unwrap_or(1);
        match event.keystroke.key.as_str() {
            "escape" => self.close_worktree_removal(window, cx),
            "tab" => {
                let step = if event.keystroke.modifiers.shift {
                    2
                } else {
                    1
                };
                let mut next = (at + step) % 3;
                if next == 0 && !open.target.can_delete_branch() {
                    next = (next + step) % 3;
                }
                window.focus(&open.focus[next], cx);
            }
            "enter" | "space"
                if !event.keystroke.modifiers.platform
                    && !event.keystroke.modifiers.control
                    && !event.keystroke.modifiers.alt =>
            {
                match at {
                    0 => {
                        if let Some(open) = &mut self.worktree_removal
                            && open.target.can_delete_branch()
                        {
                            open.delete_branch = !open.delete_branch;
                            cx.notify();
                        }
                    }
                    1 => self.close_worktree_removal(window, cx),
                    _ => self.confirm_worktree_removal(window, cx),
                }
            }
            _ => {}
        }
        true
    }

    pub(crate) fn render_worktree_removal(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let open = self.worktree_removal.as_ref()?;
        let palette = self.theme;
        let ui = self.ui();
        let allowed = open.target.can_delete_branch();
        let branch = open.target.branch.clone().unwrap_or_else(|| {
            ui.text(
                if open
                    .target
                    .head
                    .as_deref()
                    .is_some_and(|head| head.bytes().any(|b| b != b'0'))
                {
                    Text::WorktreeDetached
                } else {
                    Text::WorktreeBranchUnavailable
                },
            )
            .into()
        });
        let hint = if allowed {
            ui.text(Text::WorktreeDeleteBranchHint)
        } else {
            ui.text(Text::WorktreeBranchProtected)
        }
        .to_owned();
        let checkbox = div()
            .id("worktree-removal-checkbox")
            .debug_selector(|| "worktree-removal-checkbox".into())
            .track_focus(&open.focus[0])
            .role(gpui::Role::CheckBox)
            .aria_toggled(if open.delete_branch {
                gpui::Toggled::True
            } else {
                gpui::Toggled::False
            })
            .aria_description(hint.clone())
            .aria_label(ui.text(Text::WorktreeDeleteBranch))
            .flex()
            .items_center()
            .gap(theme::space::CARD_GAP)
            .min_w_0()
            .text_size(theme::size::BODY)
            .text_color(rgb(if allowed {
                palette.text.secondary
            } else {
                palette.text.faint
            }))
            .when(allowed, |row| row.cursor_pointer())
            .focus(|row| {
                row.bg(rgb(palette.surface.raised))
                    .rounded(theme::radius::ROW)
            })
            .tooltip(move |_, app| {
                app.new(|_| super::tooltip::Tooltip::text(hint.clone(), palette))
                    .into()
            })
            .on_click(cx.listener(|root, _, _, cx| {
                if let Some(open) = &mut root.worktree_removal
                    && open.target.can_delete_branch()
                {
                    open.delete_branch = !open.delete_branch;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .size(theme::space::HEAD_ICON)
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(theme::radius::CHECKBOX)
                    .border_1()
                    .border_color(rgb(palette.text.faint))
                    .when(open.delete_branch, |box_| {
                        box_.bg(rgb(palette.accent.patina))
                            .border_color(rgb(palette.accent.patina))
                            .child(crate::icons::check().text_color(rgb(palette.surface.app)))
                    }),
            )
            .child(ui.text(Text::WorktreeDeleteBranch));
        let cancel = div()
            .id("worktree-removal-cancel")
            .debug_selector(|| "worktree-removal-cancel".into())
            .track_focus(&open.focus[1])
            .role(gpui::Role::Button)
            .aria_label(ui.text(Text::Cancel))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(theme::worktree_confirmation::BUTTON)
            .px(theme::worktree_confirmation::GAP)
            .rounded_full()
            .border_1()
            .border_color(theme::card_outline(palette))
            .text_color(rgb(palette.text.primary))
            .cursor_pointer()
            .hover(|b| b.bg(crate::theme::control_hover(palette, palette.surface.card)))
            .focus(|b| b.border_color(rgb(palette.accent.patina)))
            .child(ui.text(Text::Cancel))
            .on_click(cx.listener(|root, _, window, cx| root.close_worktree_removal(window, cx)));
        let delete = div()
            .id("worktree-removal-delete")
            .debug_selector(|| "worktree-removal-delete".into())
            .track_focus(&open.focus[2])
            .role(gpui::Role::Button)
            .aria_label(ui.text(Text::WorktreeDelete))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h(theme::worktree_confirmation::BUTTON)
            .px(theme::worktree_confirmation::GAP)
            .rounded_full()
            .border_1()
            .border_color(rgb(palette.accent.rust))
            .bg(rgb(palette.accent.rust))
            .text_color(rgb(palette.surface.app))
            .cursor_pointer()
            .hover(|b| b.opacity(theme::worktree_confirmation::HOVER))
            .focus(|b| b.border_color(rgb(palette.text.primary)))
            .child(ui.text(Text::WorktreeDelete))
            .on_click(cx.listener(|root, _, window, cx| root.confirm_worktree_removal(window, cx)));
        let card = super::popup::card_look(palette)
            .id("worktree-removal-card")
            .debug_selector(|| "worktree-removal-card".into())
            .key_context("WorktreeRemoval")
            .role(gpui::Role::Dialog)
            .aria_label(ui.text(Text::WorktreeDeleteTitle))
            .occlude()
            .flex()
            .flex_col()
            .rounded(theme::radius::CARD)
            .w(theme::worktree_confirmation::WIDTH)
            .max_w(
                self.viewport().width
                    - gpui::px(self.rem_px(theme::worktree_confirmation::WINDOW_INSET) * 2.),
            )
            .p(theme::worktree_confirmation::PADDING)
            .text_size(theme::size::BODY)
            .line_height(theme::worktree_confirmation::LINE)
            .text_color(rgb(palette.text.primary))
            .child(
                div()
                    .text_size(theme::size::CARD_TITLE)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child(ui.text(Text::WorktreeDeleteTitle)),
            )
            .child(
                div()
                    .mt(theme::worktree_confirmation::TITLE_GAP)
                    .text_color(rgb(palette.text.secondary))
                    .child(ui.text(Text::WorktreeDeleteDescription)),
            )
            .child(
                div()
                    .id("worktree-removal-branch")
                    .debug_selector(|| "worktree-removal-branch".into())
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .h(theme::worktree_confirmation::CONTEXT)
                    .mt(theme::worktree_confirmation::GAP)
                    .px(theme::space::CARD_GAP)
                    .rounded(theme::radius::ROW)
                    .bg(rgb(palette.surface.raised))
                    .border_1()
                    .border_color(theme::card_outline(palette))
                    .child(
                        crate::icons::toolbar("ui/workspace-git-branch.svg")
                            .text_color(rgb(palette.text.secondary)),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(branch)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(theme::space::CARD_GAP)
                    .mt(theme::worktree_confirmation::GAP)
                    .child(checkbox)
                    .child(div().flex_1())
                    .child(cancel)
                    .child(delete),
            );
        // GPUI dispatches key-bound actions before key events. Stop them on
        // the focused modal's bubble path before a background Root handler.
        let card = crate::actions::keymap::Named::ALL
            .iter()
            .fold(card, |card, action| {
                card.on_boxed_action(&*action.action(), |_, _, cx| cx.stop_propagation())
            });
        Some(
            gpui::deferred(
                gpui::anchored().position(gpui::Point::default()).child(
                    div()
                        .id("worktree-removal-scrim")
                        .debug_selector(|| "worktree-removal-scrim".into())
                        .w(self.viewport().width)
                        .h(self.viewport().height)
                        .flex()
                        .items_center()
                        .justify_center()
                        .occlude()
                        .bg(gpui::black().opacity(theme::worktree_confirmation::DIM))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .child(card),
                ),
            )
            .priority(2)
            .into_any_element(),
        )
    }
}
