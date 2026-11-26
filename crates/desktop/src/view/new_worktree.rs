//! The popup a project's `+` opens: one field, for the branch — M12.2
//! task 10.
//!
//! It opened the branch popup under the title bar until M12.2, which offers
//! what is typed as a branch *and* as a worktree — one popup for three
//! gestures, and the person asked why a `+` on a project opened a list of
//! branches somewhere else. It opens where it was pressed now, and makes a
//! worktree through `write_worktree`, the one path the branch popup and
//! `guild worktree create` share.

use gpui::prelude::*;
use gpui::{Context, MouseButton, anchored, deferred, div, px, rgb};

use crate::i18n::Text;
use crate::{Root, theme};

/// What the popup is making, while it is up.
pub(crate) struct NewWorktree {
    /// The root whose `HEAD` the worktree starts from.
    pub project: guild_session::ProjectId,
    /// The branch, as typed.
    pub typed: String,
    /// Where it was asked for.
    pub at: gpui::Point<gpui::Pixels>,
}

impl Root {
    /// Open the popup for `project`'s repository at `at`.
    pub(crate) fn open_new_worktree(
        &mut self,
        project: guild_session::ProjectId,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) {
        // Every other field goes: one field takes the keyboard at a time.
        self.put_fields_away();
        self.new_worktree = Some(NewWorktree {
            project,
            typed: String::new(),
            at,
        });

        // An editor behind it binds `enter`, and a binding is dispatched
        // before the key reaches the window: the field takes the focus.
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Put the popup away and the focus back on what is drawn.
    pub(crate) fn close_new_worktree(&mut self, window: &mut gpui::Window, cx: &mut Context<Root>) {
        if self.new_worktree.take().is_some() {
            self.refocus(window, cx);
        }
    }

    /// A key pressed while the popup is up.
    ///
    /// Return makes the worktree; a refusal — no git thread yet, no
    /// repository — is a notice and the field stays, with what was typed.
    /// git's own refusal arrives later in git's words, as it does from the
    /// branch popup.
    pub(crate) fn new_worktree_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Root>,
    ) -> bool {
        let Some(open) = self.new_worktree.as_ref() else {
            return false;
        };
        let stroke = &event.keystroke;
        match stroke.key.as_str() {
            "escape" => self.close_new_worktree(window, cx),
            "enter" => {
                let (project, name) = (open.project, open.typed.trim().to_owned());
                if name.is_empty() {
                    return true;
                }
                match self.write_worktree(project, &name, None) {
                    Ok(_) => self.close_new_worktree(window, cx),
                    Err(said) => self.warn(said),
                }
            }
            _ => return false,
        }
        cx.notify();
        true
    }

    /// The popup, when it is up: a layer that closes it, and the card where
    /// it was asked for — the field, and what the worktree starts from.
    pub(crate) fn render_new_worktree(&self, cx: &mut Context<Root>) -> Option<gpui::AnyElement> {
        let open = self.new_worktree.as_ref()?;
        let theme = self.theme;
        let ui = self.ui();
        let base = self
            .desk
            .project(open.project)
            .and_then(|project| self.branch_of_worktree(&project.path));
        let viewport = self.viewport();

        let backdrop = deferred(
            anchored().position(gpui::Point::default()).child(
                div()
                    .w(viewport.width)
                    .h(viewport.height)
                    .occlude()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|root, _event, window, cx| {
                            cx.stop_propagation();
                            root.close_new_worktree(window, cx);
                        }),
                    ),
            ),
        );
        let field = div()
            .flex()
            .items_center()
            .h(theme::space::MENU_ROW)
            .px(theme::space::MENU_INSET)
            .child(
                crate::text_field::TextField::new(
                    "new-worktree-input",
                    open.typed.clone(),
                    cx.listener(|root, value: &String, _, cx| {
                        if let Some(open) = root.new_worktree.as_mut() {
                            open.typed = value.clone();
                        }
                        cx.notify();
                    }),
                )
                .focus(&self.focus)
                .autofocus()
                .placeholder(ui.text(Text::BranchSearch))
                .on_key(cx.listener(|root, event, window, cx| {
                    root.new_worktree_key(event, window, cx);
                })),
            );
        let card = crate::view::popup::card(theme)
            .w(theme::space::NEW_WORKTREE)
            .child(field)
            .children(base.map(|branch| {
                div()
                    .flex()
                    .items_center()
                    .h(px(24.0))
                    .px(theme::space::MENU_INSET)
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(ui.new_worktree_from(&branch))
            }));
        let popup = deferred(
            anchored()
                .position(open.at)
                .snap_to_window_with_margin(px(8.0))
                .child(card),
        )
        .priority(1);
        Some(div().child(backdrop).child(popup).into_any_element())
    }
}
