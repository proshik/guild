//! A project group's header in the column, and the list's empty end that
//! takes a dragged project out of its group.
//!
//! The header is a node of the list's tree, at the level of the ungrouped
//! projects, and drawn so it is never read as one of them: a folder where a
//! project has its letter tile, the name in `text.secondary`, and how many
//! projects it holds. A click collapses the group, the right button opens
//! its menu, a dragged project let go of on its middle joins the group, and
//! the header itself drags the group to another place.

use gpui::prelude::*;
use gpui::{Context, SharedString, div, px, rgb};
use guild_session::WorkspaceId;

use super::{DragGroup, DragProject, ProjectDragPreview};
use crate::column::{Dragged, DropOn, DropRow, Head, Listed, Zone};
use crate::view::tooltip::Hint as _;
use crate::{Root, theme};

/// How far an expanded header with no folder reaches into the repository
/// under it — a negative bottom margin, which the item's bounds leave out:
/// its spacer must carry it too, or every spaced header moves what follows.
pub(super) fn head_overlap(head: &Head) -> Option<gpui::Pixels> {
    (!head.collapsed && head.folder.is_none()).then_some(px(-6.0))
}

impl Root {
    pub(super) fn render_project_group_head(
        &self,
        workspace: WorkspaceId,
        head: &Head,
        first: bool,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let id = head.id;
        let target = crate::view::menu::Target::ProjectGroup { workspace, id };
        let target_more = target.clone();
        let search = (workspace == self.active_workspace)
            .then_some(self.sidebar_speed.as_ref())
            .flatten();
        // The search's current match: lit as a project's head is.
        let candidate = search.is_some_and(|s| {
            s.current.as_ref() == Some(&crate::column::Listed::Group(id))
                && !crate::speed_search::ranges(&head.name, &s.query).is_empty()
        });
        let query = search.map(|s| s.query.clone()).unwrap_or_default();
        let row = Listed::Group(id);
        let kind = DropRow::Head {
            collapsed: head.collapsed,
        };
        let hint = self
            .column_drop_hint
            .as_ref()
            .filter(|hint| hint.workspace == workspace && hint.row == row)
            .map(|hint| hint.zone);
        let drag_root = cx.weak_entity();
        let drag_name = head.name.clone();
        let drag = DragGroup {
            token: uuid::Uuid::new_v4(),
            workspace,
            id,
        };
        div()
            .id(SharedString::from(format!("project-group-{id}")))
            .debug_selector(move || format!("project-group-{id}"))
            .relative()
            .child(self.project_drag.measure_row(workspace, row.clone()))
            .capture_any_mouse_down(|event, window, _| {
                if event.button == gpui::MouseButton::Left {
                    window.prevent_default();
                }
            })
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.0))
            .h(px(28.0))
            // Bound to the repositories under it rather than floating
            // halfway between them and the section above: the list's gap
            // is the air *between* projects. Collapsed, nothing is under it;
            // with a folder, the folder's row is, in the same list item.
            .when_some(head_overlap(head), |row, overlap| row.mb(overlap))
            .px(theme::column::HEADER_PADDING)
            .rounded(px(8.0))
            .cursor_pointer()
            .text_size(theme::size::BODY)
            .text_color(rgb(theme.text.secondary))
            .when(candidate, |row| row.bg(rgb(theme.surface.raised)))
            .hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            // The middle takes a project into the group; the edges are the
            // places before and after the group, for a project or a group.
            .when(hint == Some(Zone::Middle), |row| {
                row.bg(rgb(theme.accent.brass_bg))
            })
            .children(hint.and_then(|zone| super::drop_line(theme, zone, px(0.0), first)))
            .on_drag_move(cx.listener({
                let row = row.clone();
                move |root, event: &gpui::DragMoveEvent<DragProject>, _, cx| {
                    let dragged = event.drag(cx);
                    if dragged.workspace != workspace {
                        return;
                    }
                    // Already a member: its middle would change nothing.
                    let member = root
                        .desk
                        .workspace(workspace)
                        .and_then(|holder| holder.project_group_of(&dragged.key))
                        .is_some_and(|group| group.id == id);
                    root.hover_column_drop(
                        workspace,
                        row.clone(),
                        crate::column::Top::Group(id),
                        kind,
                        Dragged::Project,
                        |zone| member && zone == Zone::Middle,
                        event.bounds,
                        event.event.position,
                        cx,
                    );
                }
            }))
            .on_drag_move(cx.listener({
                let row = row.clone();
                move |root, event: &gpui::DragMoveEvent<DragGroup>, _, cx| {
                    let dragged = event.drag(cx);
                    if dragged.workspace != workspace {
                        return;
                    }
                    let itself = dragged.id == id;
                    root.hover_column_drop(
                        workspace,
                        row.clone(),
                        crate::column::Top::Group(id),
                        kind,
                        Dragged::Group,
                        |_| itself,
                        event.bounds,
                        event.event.position,
                        cx,
                    );
                }
            }))
            .on_drop(cx.listener({
                let row = row.clone();
                move |root, dragged: &DragProject, window, cx| {
                    if dragged.workspace == workspace
                        && root.active_workspace == workspace
                        && (!root.settings.column_folded || root.column_peeking)
                    {
                        root.drop_project_on_row(
                            workspace,
                            &dragged.key,
                            &dragged.projects,
                            row.clone(),
                            None,
                            kind,
                            window,
                            cx,
                        );
                    }
                }
            }))
            .on_drop(cx.listener({
                let row = row.clone();
                move |root, dragged: &DragGroup, window, cx| {
                    if dragged.workspace == workspace
                        && root.active_workspace == workspace
                        && (!root.settings.column_folded || root.column_peeking)
                        && dragged.id != id
                    {
                        root.drop_group_on_row(
                            workspace,
                            dragged.id,
                            row.clone(),
                            None,
                            kind,
                            window,
                            cx,
                        );
                    }
                }
            }))
            .on_drag(drag, move |drag, grabbed, window, cx| {
                let _ = drag_root.update(cx, |root, cx| {
                    root.begin_project_drag(
                        drag.workspace,
                        drag.token,
                        Some(crate::column::Top::Group(drag.id)),
                        None,
                        window,
                        cx,
                    )
                });
                cx.new(|_| ProjectDragPreview {
                    name: drag_name.clone(),
                    theme,
                    grabbed,
                })
            })
            .on_click(cx.listener(move |root, _event, window, cx| {
                window.focus(&root.sidebar_focus, cx);
                root.toggle_project_group(workspace, id, cx);
            }))
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |root: &mut Root, _event, _window, _cx| {
                    root.note_menu_target(target.clone());
                }),
            )
            // A folder where a project has its letter tile, in the tile's
            // room so the names line up: a group is never read as a project.
            .child(
                div()
                    .debug_selector(|| "project-group-icon".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .child(
                        crate::icons::toolbar("ui/workspace-folder.svg")
                            .text_color(rgb(theme.text.muted)),
                    ),
            )
            .child(
                div()
                    .debug_selector(|| "project-group-label".into())
                    .min_w_0()
                    .truncate()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(crate::view::speed_search::label(
                        head.name.clone(),
                        &query,
                        theme,
                    )),
            )
            // How many projects it holds, always, quietly: open or shut, a
            // group says it is one.
            .child(
                div()
                    .debug_selector(|| "project-group-count".into())
                    .flex_none()
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(head.count.to_string()),
            )
            .child(
                div()
                    .debug_selector(|| "project-group-chevron".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .child(
                        crate::icons::chevron(!head.collapsed)
                            .size(px(11.0))
                            .text_color(rgb(theme.text.muted)),
                    ),
            )
            .child(div().flex_1())
            // **What a project's head has, on a group's** — GLD-055
            // decision 15: `…` opens the menu the right button opens, `+`
            // adds projects to the group. The same glyphs and measures as
            // the project's head; on both, `+` adds something under the row.
            .child(self.group_head_button(
                format!("project-group-menu-{id}"),
                "project-group-menu",
                "ui/head-more.svg",
                self.ui().text(crate::i18n::Text::PanelOptions).to_owned(),
                cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    root.note_menu_target(target_more.clone());
                    root.open_context_menu(event.position(), window, cx);
                }),
            ))
            .child(self.group_add_button(
                workspace,
                id,
                cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                    cx.stop_propagation();
                    let choices = root.group_add_choices(workspace, id);
                    if choices.is_empty() {
                        return;
                    }
                    root.note_menu_target(crate::view::menu::Target::ProjectGroupAdd {
                        workspace,
                        id,
                        choices,
                    });
                    root.open_context_menu(event.position(), window, cx);
                }),
            ))
    }

    /// The header's `+` — GLD-055 decisions 16 and 29: live while a project
    /// of the workspace is outside the group; with none, a faint glyph at the
    /// faint-mark opacity, as the title bar's unavailable controls are, with
    /// no hover and no press, its tooltip saying why.
    fn group_add_button(
        &self,
        workspace: WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let ui = self.ui();
        if self.group_can_add(workspace, id) {
            let hint = ui.text(crate::i18n::Text::GroupAddProjects).to_owned();
            return self
                .group_head_button(
                    format!("project-group-add-{id}"),
                    "project-group-add",
                    "ui/head-plus.svg",
                    hint.clone(),
                    on_click,
                )
                .hint_delay()
                .tooltip(move |_window, app| {
                    app.new(|_| crate::view::tooltip::Tooltip::text(hint.clone(), theme))
                        .into()
                });
        }
        let hint = ui.text(crate::i18n::Text::GroupNothingToAdd).to_owned();
        div()
            .id(SharedString::from(format!("project-group-add-{id}")))
            .debug_selector(|| "project-group-add-disabled".into())
            .role(gpui::Role::Button)
            .aria_label(hint.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::column::ACTION_TARGET)
            .rounded(theme::radius::ROW)
            // The press is the button's, and does nothing: the header's own
            // click would fold the group.
            .on_click(|_, _, cx| cx.stop_propagation())
            // Faint and at the faint-mark opacity, as the title bar's Open In
            // is without a project: in Dark `text.faint` alone is the live
            // glyph's `text.muted` to the eye.
            .child(
                crate::icons::toolbar("ui/head-plus.svg")
                    .text_color(rgb(theme.text.faint))
                    .opacity(crate::view::kind::FAINT),
            )
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_| crate::view::tooltip::Tooltip::text(hint.clone(), theme))
                    .into()
            })
    }

    /// One of a group header's two buttons, drawn as a project head's are:
    /// the action target's square, lit over the head's own hover.
    fn group_head_button(
        &self,
        id: String,
        selector: &'static str,
        icon: &'static str,
        label: String,
        on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(SharedString::from(id))
            .debug_selector(move || selector.into())
            .role(gpui::Role::Button)
            .aria_label(label)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::column::ACTION_TARGET)
            .rounded(theme::radius::ROW)
            .cursor_pointer()
            .hover(|button| {
                button.bg(crate::theme::control_hover_over(
                    theme,
                    crate::theme::control_hover(theme, theme.surface.panel),
                ))
            })
            .on_click(on_click)
            .child(crate::icons::toolbar(icon).text_color(rgb(theme.text.muted)))
    }

    /// The rest of the list below the last repository: a project dropped
    /// here leaves its group. Drawn only while the workspace has a group,
    /// and invisible until a project is dragged over it.
    pub(super) fn render_ungrouped_drop(
        &self,
        workspace: WorkspaceId,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(SharedString::from(format!(
                "column-ungrouped-drop-{workspace}"
            )))
            .debug_selector(|| "column-ungrouped-drop".into())
            .flex_1()
            .min_h(px(28.0))
            .rounded(px(8.0))
            .drag_over::<DragProject>(move |style, dragged, _, _| {
                if dragged.workspace == workspace {
                    style.bg(rgb(theme.accent.brass_bg))
                } else {
                    style
                }
            })
            // A target of its own: a line from a row above does not stand.
            .on_drag_move(cx.listener(
                move |root, event: &gpui::DragMoveEvent<DragProject>, _, cx| {
                    if event.bounds.contains(&event.event.position) {
                        root.set_column_drop_hint(None, cx);
                    }
                },
            ))
            .on_drop(
                cx.listener(move |root, dragged: &DragProject, _window, cx| {
                    if dragged.workspace == workspace
                        && root.active_workspace == workspace
                        && (!root.settings.column_folded || root.column_peeking)
                    {
                        root.clear_project_drag(cx);
                        root.drop_repository(
                            workspace,
                            &dragged.key,
                            &dragged.projects,
                            DropOn::Ungrouped,
                            None,
                            cx,
                        );
                    }
                }),
            )
    }
}
