//! Explorer drag sources and clipped row targets, independent of dock/tab drags.
use crate::{Root, root::tree_drag::FileDrag, theme};
use gpui::{Context, ExternalPaths, Render, Window, div, prelude::*, rgb};
use std::path::PathBuf;
struct Preview {
    names: String,
    theme: theme::Theme,
}
impl Render for Preview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(theme::space::CARD_INNER_GAP)
            .rounded(theme::radius::ROW)
            .bg(rgb(self.theme.surface.raised))
            .text_color(rgb(self.theme.text.primary))
            .child(self.names.clone())
    }
}
pub(crate) fn row(
    element: gpui::Stateful<gpui::Div>,
    path: PathBuf,
    is_dir: bool,
    project_row: bool,
    project: guild_session::ProjectId,
    theme: theme::Theme,
    cx: &mut Context<Root>,
) -> gpui::Stateful<gpui::Div> {
    let destination = crate::file_operations::drop_target(&path, is_dir);
    let hovered = destination.clone();
    let highlight = destination.clone();
    let element = inbound(element, destination.clone(), project_row, theme, cx);
    let root = cx.weak_entity();
    let element = element
        .drag_over::<FileDrag>(move |style, drag, _, cx| {
            if project_row
                && root
                    .read_with(cx, |root, _| root.tree_drop_allowed(drag, &highlight))
                    .unwrap_or(false)
            {
                // The dock's drop colour, apart from the selection's — decision 54.
                style.bg(rgb(theme.accent.brass_bg))
            } else {
                style
            }
        })
        .on_drag_move(
            cx.listener(move |root, event: &gpui::DragMoveEvent<FileDrag>, _, cx| {
                let Some(tree) = root.tree() else {
                    return;
                };
                let bounds = tree.scroll.0.borrow().base_handle.bounds();
                if crate::view::tabs::answers_drag(event.bounds, bounds, event.event.position) {
                    let drag = event.drag(cx).clone();
                    if project_row {
                        root.hover_tree_drag(&drag, &hovered, cx);
                    } else {
                        root.clear_tree_hover();
                    }
                }
            }),
        )
        .on_drop(cx.listener(move |root, drag: &FileDrag, window, cx| {
            cx.stop_propagation();
            if project_row {
                root.drop_tree_drag(drag, destination.clone(), window, cx);
            } else {
                root.clear_tree_drag(cx);
            }
        }));
    if !project_row || !crate::root::tree::writable_row(&path) {
        return element;
    }
    let root = cx.weak_entity();
    let out = root.clone();
    element
        .on_drag(FileDrag::new(project, path), move |drag, _, window, cx| {
            let sources = root
                .update(cx, |root, cx| root.begin_tree_drag(drag, window, cx))
                .unwrap_or_default();
            let names = sources
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            cx.new(|_| Preview { names, theme })
        })
        .external_drag_payload(move |drag: &FileDrag, _, cx| {
            out.update(cx, |root, cx| root.hand_tree_drag_out(drag, cx))
                .ok()
                .flatten()
        })
}

/// What another application drags over a row: the same highlight and hover
/// clock as an Explorer drag, and a copy on release. A row outside the
/// project takes the drop so nothing falls through to the root behind it.
fn inbound(
    element: gpui::Stateful<gpui::Div>,
    destination: PathBuf,
    project_row: bool,
    theme: theme::Theme,
    cx: &mut Context<Root>,
) -> gpui::Stateful<gpui::Div> {
    let root = cx.weak_entity();
    let highlight = destination.clone();
    let hovered = destination.clone();
    element
        .drag_over::<ExternalPaths>(move |style, paths, _, cx| {
            if project_row
                && root
                    .read_with(cx, |root, _| root.inbound_allowed(paths, &highlight))
                    .unwrap_or(false)
            {
                // The dock's drop colour, apart from the selection's — decision 54.
                style.bg(rgb(theme.accent.brass_bg))
            } else {
                style
            }
        })
        .on_drag_move(cx.listener(
            move |root, event: &gpui::DragMoveEvent<ExternalPaths>, window, cx| {
                let Some(tree) = root.tree() else {
                    return;
                };
                let bounds = tree.scroll.0.borrow().base_handle.bounds();
                if crate::view::tabs::answers_drag(event.bounds, bounds, event.event.position) {
                    let paths = event.drag(cx).clone();
                    if project_row {
                        root.hover_inbound(&paths, &hovered, window, cx);
                    } else {
                        root.clear_tree_hover();
                    }
                }
            },
        ))
        .on_drop(cx.listener(move |root, paths: &ExternalPaths, window, cx| {
            cx.stop_propagation();
            if project_row {
                root.drop_inbound(paths, destination.clone(), window, cx);
            } else {
                root.clear_tree_hover();
            }
        }))
}
