//! The column of projects and their worktrees — section 6.29, M12.
//!
//! Down the left of the window, beside the docks and the bottom one,
//! because it is where a person changes *what* they are working on rather
//! than *how* they are looking at it. It replaced the row of projects
//! M5.1 put under the title bar: a row was right while a project was a
//! directory and there were three of them, and a project is a repository
//! with several worktrees now.

use crate::view::tooltip::Hint as _;
use gpui::prelude::*;
use gpui::{Context, Render, SharedString, Window, div, px, rgb};
use gpui_component::ElementExt as _;

use crate::column::{Group, Row, row_key};
use crate::i18n::Text;
use crate::{Root, theme};

pub(crate) mod project_drag;
mod project_groups;
mod sessions;
pub(crate) mod spacers;
mod worktree_tabs;

#[derive(Debug, Clone)]
pub(crate) struct DragProject {
    token: uuid::Uuid,
    workspace: guild_session::WorkspaceId,
    /// The repository's key, which its project group is kept by.
    key: std::path::PathBuf,
    projects: Vec<guild_session::ProjectId>,
}

/// A project group's header held by the pointer: the group goes where it
/// is let go of, among the column's top-level items.
#[derive(Debug, Clone)]
pub(crate) struct DragGroup {
    token: uuid::Uuid,
    workspace: guild_session::WorkspaceId,
    id: guild_session::project_group::ProjectGroupId,
}

/// The line a drag in the column draws for the place before or after a row's
/// top-level item: in the middle of the gap between items, from `left` to
/// the row's right edge. Nothing for a row's middle.
pub(super) fn drop_line(
    theme: theme::Theme,
    zone: crate::column::Zone,
    left: gpui::Pixels,
    first: bool,
) -> Option<gpui::Div> {
    // Half the list's twelve-point gap, less half the line — except above
    // the list's first row, where the list's own top edge would cut it off:
    // there it sits in the list's four points of top padding.
    let reach = px(-7.0);
    let above = if first { px(-3.0) } else { reach };
    let line = div()
        .debug_selector(|| "column-drop-line".into())
        .absolute()
        .left(left)
        .right_0()
        .h(px(2.0))
        .rounded(px(1.0))
        .bg(rgb(theme.accent.brass));
    match zone {
        crate::column::Zone::Before => Some(line.top(above)),
        crate::column::Zone::After => Some(line.bottom(reach)),
        crate::column::Zone::Middle => None,
    }
}

/// The side of a project head's tile, in points.
const TILE: f32 = 18.0;

/// The square a project's head starts with: its letter, or an icon, on
/// `fill`, `side` points across — the head's [`TILE`], or smaller where a
/// list row has an icon's room. Its corner and letter keep the head's
/// proportions at any side.
fn tile(fill: u32, ink: u32, side: f32) -> gpui::Div {
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .size(px(side))
        .rounded(px(5.0 * side / TILE))
        .bg(rgb(fill))
        .text_size(theme::from_px(10.0 * side / TILE))
        .font_weight(gpui::FontWeight::BOLD)
        .text_color(rgb(ink))
}

/// A repository's colour: the accent [`crate::column::tile_colour`] names
/// for its name — the column's tile, the agents list's and Settings'.
pub(crate) fn tile_fill(palette: theme::Theme, name: &str) -> u32 {
    match crate::column::tile_colour(name) {
        crate::column::Tile::Brass => palette.accent.brass,
        crate::column::Tile::Patina => palette.accent.patina,
        crate::column::Tile::Violet => palette.accent.violet,
        crate::column::Tile::Rust => palette.accent.rust,
        crate::column::Tile::Green => palette.accent.green,
    }
}

/// The letter a repository's tile shows: its name's first, in capitals.
pub(crate) fn tile_letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|first| first.to_uppercase().collect())
        .unwrap_or_default()
}

/// A repository's letter tile as the column draws it, `side` points across:
/// its first letter on its own colour.
pub(crate) fn letter_tile(palette: theme::Theme, name: &str, side: f32) -> gpui::Div {
    tile(tile_fill(palette, name), palette.surface.app, side).child(tile_letter(name))
}

/// What makes a row a project group's folder — GLD-055.
#[derive(Clone)]
pub(crate) struct FolderLook {
    /// The group it is the folder of.
    pub group: guild_session::project_group::ProjectGroupId,
    /// Its second line: how many members sit in it.
    pub meta: String,
    /// Whether a member is outside it, which the line says in brass.
    pub outside: bool,
}

/// How a worktree row's action lights under the pointer: over the row it
/// is drawn on, in rust when it removes something.
#[derive(Clone, Copy)]
struct ActionLight {
    destructive: bool,
    ground: gpui::Rgba,
}

struct ProjectDragPreview {
    name: String,
    theme: theme::Theme,
    /// Where in the dragged head the press was. GPUI draws a preview with
    /// that point under the pointer, which put the pill over the name of
    /// the row being aimed at; it is moved clear of the pointer instead.
    grabbed: gpui::Point<gpui::Pixels>,
}

/// How far right of and below the pointer the dragged pill starts.
const PREVIEW_CLEARANCE: f32 = 14.0;

impl Render for ProjectDragPreview {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .pl(self.grabbed.x + px(PREVIEW_CLEARANCE))
            .pt(self.grabbed.y + px(PREVIEW_CLEARANCE))
            .child(
                div()
                    .debug_selector(|| "column-drag-preview".into())
                    .px(px(10.0))
                    .h(px(28.0))
                    .flex()
                    .items_center()
                    .rounded(px(8.0))
                    .bg(rgb(self.theme.surface.raised))
                    .text_color(rgb(self.theme.text.primary))
                    .child(self.name.clone()),
            )
    }
}

impl Root {
    /// How wide the column is drawn — by `render_column` and by the cached
    /// region's own layout node, which must agree: GPUI lays a cached view
    /// out from the style it is handed, not from what it draws.
    pub(crate) fn column_width(&self) -> gpui::Pixels {
        px(self.settings.column_width)
    }

    /// The view that draws `part` — made the first time it is asked for,
    /// kept for the window's life.
    pub(crate) fn region(
        &mut self,
        part: crate::view::region::Part,
        cx: &mut Context<Root>,
    ) -> gpui::Entity<crate::view::region::Region> {
        let root = cx.entity();
        self.regions
            .entry(part)
            .or_insert_with(|| cx.new(|cx| crate::view::region::Region::new(&root, part, cx)))
            .clone()
    }

    /// The column, or the strip it folds into.
    pub(crate) fn render_column(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let theme = self.theme;

        // Hidden, and the pointer is not at the edge: nothing at all.
        // A column folded to a strip of tiles was M12's first answer and
        // the person's answer to that was that a strip is still a side
        // panel — task 10. Who is waiting is not lost with it: M5.10 put
        // that in the status bar and on the Dock icon.
        if self.settings.column_folded && !self.column_peeking {
            return div().into_any_element();
        }
        // Out because the pointer brought it out: it is drawn over the
        // work rather than beside it, so nothing reflows for a panel that
        // is about to go away again.
        let peeking = self.column_peeking;

        use crate::actions::{StretchToBottom, StretchToLeft, StretchToRight, StretchToTop};
        use guild_session::Stretch;
        div()
            // The panels' focus, strokes and Escape, for the sidebar too —
            // M12.3: the person asked why it was the one panel a stretch
            // did not reach.
            .track_focus(&self.sidebar_focus)
            .on_key_down(cx.listener(|root, event: &gpui::KeyDownEvent, window, cx| {
                if crate::view::controls::tab_navigation(event, window, cx) {
                    cx.stop_propagation();
                    return;
                }
                let stroke = &event.keystroke;
                // The Sessions page's arrows and Enter, as a list's: the
                // arrows choose, Enter opens what is chosen.
                if root.sessions_mode() && stroke.modifiers == gpui::Modifiers::none() {
                    match stroke.key.as_str() {
                        "up" | "down" => {
                            cx.stop_propagation();
                            root.step_session_cursor(stroke.key == "down", cx);
                            return;
                        }
                        "enter" => {
                            if let Some(key) = root.session_chosen() {
                                cx.stop_propagation();
                                root.activate_session(key, window, cx);
                                return;
                            }
                        }
                        _ => {}
                    }
                }
                if stroke.key == "escape" && stroke.modifiers == gpui::Modifiers::none() {
                    cx.stop_propagation();
                    root.refocus(window, cx);
                }
            }))
            .on_action(cx.listener(|root, _: &StretchToLeft, window, cx| {
                root.stretch_sidebar(Stretch::Left, window, cx)
            }))
            .on_action(cx.listener(|root, _: &StretchToRight, window, cx| {
                root.stretch_sidebar(Stretch::Right, window, cx)
            }))
            .on_action(cx.listener(|root, _: &StretchToTop, window, cx| {
                root.stretch_sidebar(Stretch::Top, window, cx)
            }))
            .on_action(cx.listener(|root, _: &StretchToBottom, window, cx| {
                root.stretch_sidebar(Stretch::Bottom, window, cx)
            }))
            .flex()
            .flex_none()
            .flex_col()
            .w(self.column_width())
            .h_full()
            // The border hangs at the panel's own right edge rather than
            // between two flex children: a column that is sometimes drawn
            // over the work has no sibling to sit beside.
            .relative()
            .bg(rgb(theme.surface.panel))
            // **The peeking panel takes the mouse.** Every hitbox under
            // the pointer is hovered, not only the front one, so without
            // this a click on a row would also reach whatever it is drawn
            // over. It is the same cure the popups carry.
            .when(peeking, |column| {
                column
                    .occlude()
                    .border_r_1()
                    .border_color(rgb(theme.surface.divider))
            })
            .child(self.project_drag.measure(project_drag::Surface::Column))
            .child(self.render_workspace_pages(peeking, cx))
            // Outside the scrolling pages: pinned and peeking own one copy.
            .child(
                div()
                    .debug_selector(|| "workspace-column-controls".into())
                    .flex()
                    .flex_none()
                    .min_w_0()
                    .p(theme::space::WORK_INSET)
                    .child(self.render_workspace_switcher(cx)),
            )
            // **Last, or the rows paint over it.** An absolutely placed
            // element is still painted in its own place in the tree, so a
            // border drawn before the list showed through only where no
            // row had a background of its own — the lit row and the row
            // under the pointer cut it into pieces. Seen in the window,
            // M12 task 14.
            .when(peeking, |column| column.child(self.render_column_edge(cx)))
            .child(self.worktree_tooltips.clone())
            .into_any_element()
    }

    fn render_workspace_pages(&self, peeking: bool, cx: &mut Context<Root>) -> gpui::Div {
        let width = self.settings.column_width;
        let offset = self.workspace_swipe_offset;
        let workspaces = self.desk.workspaces();
        self.project_drag.retain(workspaces);
        self.column_spacers.retain(workspaces);
        let at = workspaces
            .iter()
            .position(|workspace| workspace.id == self.active_workspace)
            .unwrap_or(0);
        let neighbour = if offset < 0.0 {
            workspaces
                .get(at + 1)
                .map(|workspace| (workspace.id, offset + width))
        } else if offset > 0.0 && at > 0 {
            workspaces
                .get(at - 1)
                .map(|workspace| (workspace.id, offset - width))
        } else {
            None
        };
        div()
            .relative()
            .flex()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            // Only pages participate in the carousel. The marks below own
            // horizontal overflow for legacy workspace counts.
            .child({
                let root = cx.entity().downgrade();
                gpui::canvas(
                    |_, _, _| {},
                    move |bounds, _, window, _| {
                        let root = root.clone();
                        window.on_mouse_event(
                            move |event: &gpui::ScrollWheelEvent, phase, _, app| {
                                if phase != gpui::DispatchPhase::Capture
                                    || !bounds.contains(&event.position)
                                {
                                    return;
                                }
                                let taken = root
                                    .update(app, |root, cx| root.workspace_swipe_heard(event, cx))
                                    .unwrap_or(false);
                                if taken {
                                    app.stop_propagation();
                                }
                            },
                        );
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            })
            .child(
                self.render_workspace_page(self.active_workspace, peeking, cx)
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(offset))
                    .w(px(width)),
            )
            .children(neighbour.map(|(workspace, left)| {
                self.render_workspace_page(workspace, peeking, cx)
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(left))
                    .w(px(width))
            }))
    }

    fn render_workspace_page(
        &self,
        workspace: guild_session::WorkspaceId,
        _peeking: bool,
        cx: &mut Context<Root>,
    ) -> gpui::Div {
        let sections = self.column_sections_for(workspace);
        let grouped = sections.iter().any(|section| section.head.is_some());
        let drawn = self.column_items_from(workspace, &sections);
        let groups: Vec<Group> = sections
            .into_iter()
            .flat_map(|section| section.repositories)
            .collect();
        let mut pinned = Vec::new();
        if let Some(saved) = self.desk.workspace(workspace) {
            for path in &saved.pinned_worktrees {
                for group in &groups {
                    if let Some(index) = group.rows.iter().position(|row| {
                        let title = match row {
                            Row::Opened { title, .. } | Row::Unopened { title, .. } => title,
                        };
                        self.column_row_path(row).as_ref() == Some(path)
                            && !matches!(title.name.as_str(), "main" | "master")
                    }) {
                        pinned.push(group.rows[index].clone());
                    }
                }
            }
        }
        // The last drawn repository of its group: its bottom edge is the
        // place after the group.
        let member = |item: Option<&crate::column::Item>| {
            matches!(
                item,
                Some(crate::column::Item::Repository {
                    section: Some(_),
                    ..
                })
            )
        };
        let last: Vec<bool> = (0..drawn.len())
            .map(|index| member(drawn.get(index)) && !member(drawn.get(index + 1)))
            .collect();
        // Far from the viewport, an item is a spacer of its last height —
        // `spacers`. Only the Projects page has the list.
        let scroll = self.project_drag.scroll(workspace);
        let plan = (!self.sessions_mode()).then(|| {
            let counts = self.column_tab_counts();
            let shapes = drawn
                .iter()
                .zip(&last)
                .map(|(item, last)| self.column_item_shape(workspace, item, *last, &counts))
                .collect();
            self.column_spacers
                .plan(workspace, &scroll, self.column_frame(), shapes)
        });
        // A row drawn last frame and a spacer now never hears its pointer
        // leave: what it owned goes with it — the worktree actions' shared
        // tooltip, if it was this row's, and the row's or its tab's hover.
        if let Some(gone) = plan
            .as_ref()
            .map(|plan| plan.discarded_rows.as_slice())
            .filter(|gone| !gone.is_empty())
        {
            self.forget_spaced_rows(workspace, gone, cx);
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .h(px(30.0))
                    .pl(px(16.0))
                    .pr(theme::space::CARD_PADDING)
                    .gap(theme::space::CARD_GAP)
                    .child(self.render_workspace_identity(workspace, cx)),
            )
            .when(!pinned.is_empty() && !self.sessions_mode(), |page| {
                page.child(
                    div()
                        .flex()
                        .flex_col()
                        .px(px(6.))
                        .flex_none()
                        .max_h(gpui::relative(0.4))
                        .overflow_hidden()
                        .child(
                            div()
                                .text_size(theme::size::META)
                                .text_color(rgb(self.theme.text.faint))
                                .pl(px(10.))
                                .child(self.ui().text(Text::SidebarPinned)),
                        )
                        .child(
                            div()
                                .id(SharedString::from(format!("pinned-worktrees-{workspace}")))
                                .flex_1()
                                .min_h_0()
                                .overflow_y_scroll()
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener(
                                        |root, event: &gpui::MouseDownEvent, window, cx| {
                                            root.open_context_menu(event.position, window, cx)
                                        },
                                    ),
                                )
                                .children(pinned.iter().map(|row| {
                                    let element = self.render_row(workspace, row, cx);
                                    self.with_worktree_tabs(workspace, row, element, cx)
                                })),
                        ),
                )
            })
            .child(
                div()
                    .flex_none()
                    .relative()
                    .h(px(24.0))
                    .pl(px(16.0))
                    .pr(theme::column::LIST_INSET + theme::column::HEADER_PADDING)
                    .flex()
                    .items_center()
                    // A drag a little above the first row is aimed at the
                    // place before it: the heading takes it there.
                    .child(
                        self.project_drag
                            .measure(project_drag::Surface::Heading(workspace)),
                    )
                    .on_drag_move(cx.listener(
                        move |root, event: &gpui::DragMoveEvent<DragProject>, _, cx| {
                            if event.drag(cx).workspace == workspace {
                                root.hover_heading_drop(
                                    workspace,
                                    event.bounds,
                                    event.event.position,
                                    cx,
                                );
                            }
                        },
                    ))
                    .on_drag_move(cx.listener(
                        move |root, event: &gpui::DragMoveEvent<DragGroup>, _, cx| {
                            if event.drag(cx).workspace == workspace {
                                root.hover_heading_drop(
                                    workspace,
                                    event.bounds,
                                    event.event.position,
                                    cx,
                                );
                            }
                        },
                    ))
                    .on_drop(cx.listener(move |root, dragged: &DragProject, _, cx| {
                        if dragged.workspace == workspace && root.active_workspace == workspace {
                            root.drop_project_by_hint(
                                workspace,
                                &dragged.key,
                                &dragged.projects,
                                cx,
                            );
                        }
                    }))
                    .on_drop(cx.listener(move |root, dragged: &DragGroup, _, cx| {
                        if dragged.workspace == workspace && root.active_workspace == workspace {
                            root.drop_group_by_hint(workspace, dragged.id, cx);
                        }
                    }))
                    // The section's heading, the root of the list's tree:
                    // heavier and lighter than the faint labels around it.
                    .text_size(theme::size::META)
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(rgb(self.theme.text.secondary))
                    .child(
                        div()
                            .debug_selector(|| "column-projects-heading".into())
                            .flex_1()
                            .child(self.ui().text(self.column_heading(workspace)).to_owned()),
                    )
                    .when(!self.sessions_mode(), |heading| {
                        heading.child(self.project_sort_button(workspace, cx))
                    })
                    .when(self.sessions_mode(), |heading| {
                        heading.child(self.session_filter_button(workspace, cx))
                    }),
            )
            .when(workspace == self.active_workspace, |page| {
                page.child(if self.sessions_mode() {
                    self.session_speed_field(cx)
                } else {
                    self.sidebar_speed_field(cx)
                })
            })
            .when(self.sessions_mode(), |page| {
                page.child(self.render_session_list(workspace, cx))
            })
            .when(!self.sessions_mode(), |page| {
                page.child(
                    div()
                        .relative()
                        .flex()
                        .flex_col()
                        .flex_1()
                        .min_h_0()
                        .child(
                            self.project_drag
                                .measure(project_drag::Surface::List(workspace)),
                        )
                        .child(
                            div()
                                // An id, because a plain `Div` does not scroll: the
                                // scrolling properties belong to an element the
                                // window can keep state for, as the tab strip's do.
                                .id(SharedString::from(format!("column-groups-{workspace}")))
                                .debug_selector(|| "column-list".into())
                                .relative()
                                .track_scroll(&scroll)
                                .flex()
                                .flex_col()
                                .flex_1()
                                // Twelve points of air between one project and the
                                // next, and no line — M12.4: the
                                // projects ran together with nothing between them.
                                .gap(px(12.0))
                                .px(theme::column::LIST_INSET)
                                .pt(px(4.0))
                                .pb(theme::space::CARD_GAP)
                                // The horizontal half of the trap the panels carry:
                                // without it a list taller than its box pushes the
                                // box open from the inside and nothing scrolls.
                                .min_h_0()
                                .overflow_y_scroll()
                                // One child per header and per repository, so the
                                // scroll handle's item index is the speed search's
                                // row — `sidebar_speed_rows` counts the same list.
                                .children(drawn.iter().enumerate().map(|(index, item)| {
                                    if let Some(height) =
                                        plan.as_ref().and_then(|plan| plan.heights[index])
                                    {
                                        let overlap = match item {
                                            crate::column::Item::Head(head) => {
                                                project_groups::head_overlap(head)
                                            }
                                            crate::column::Item::Repository { .. } => None,
                                        };
                                        return div()
                                            .debug_selector(|| "column-spacer".into())
                                            .flex_none()
                                            .h(height)
                                            .when_some(overlap, |spacer, overlap| {
                                                spacer.mb(overlap)
                                            })
                                            .into_any_element();
                                    }
                                    // A tree under the heading: groups and the
                                    // ungrouped projects a step right of it, a
                                    // group's projects a step further under its
                                    // header — each with its head, worktrees,
                                    // tabs and every target on them.
                                    let first = index == 0;
                                    match item {
                                        // A bound group's folder hangs under its
                                        // header, in the same list item — GLD-055
                                        // decision 2.
                                        crate::column::Item::Head(head) => {
                                            let head_row = self.render_project_group_head(
                                                workspace, head, first, cx,
                                            );
                                            match self.render_group_folder(workspace, head, cx) {
                                                None => head_row
                                                    .ml(theme::column::TOP_INDENT)
                                                    .into_any_element(),
                                                // Never squeezed by a list
                                                // taller than its box, as the
                                                // header alone is not.
                                                Some(folder) => div()
                                                    .flex_none()
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(6.0))
                                                    .ml(theme::column::TOP_INDENT)
                                                    .child(head_row)
                                                    .child(folder)
                                                    .into_any_element(),
                                            }
                                        }
                                        crate::column::Item::Repository { group, section } => self
                                            .render_group(
                                                workspace,
                                                group,
                                                *section,
                                                last[index],
                                                first,
                                                cx,
                                            )
                                            .pl(if section.is_some() {
                                                theme::column::TOP_INDENT
                                                    + theme::column::GROUP_INDENT
                                            } else {
                                                theme::column::TOP_INDENT
                                            })
                                            .into_any_element(),
                                    }
                                }))
                                .when(grouped, |list| {
                                    list.child(self.render_ungrouped_drop(workspace, cx))
                                })
                                // A drop between rows — on the line drawn in the
                                // gap — is the list's: it places what is held as
                                // the line says. Off the list, the line goes.
                                .on_drag_move(cx.listener(
                                    move |root, event: &gpui::DragMoveEvent<DragProject>, _, cx| {
                                        root.leave_column_drop(workspace, event.event.position, cx)
                                    },
                                ))
                                .on_drag_move(cx.listener(
                                    move |root, event: &gpui::DragMoveEvent<DragGroup>, _, cx| {
                                        root.leave_column_drop(workspace, event.event.position, cx)
                                    },
                                ))
                                .on_drop(cx.listener(move |root, dragged: &DragProject, _, cx| {
                                    if dragged.workspace == workspace
                                        && root.active_workspace == workspace
                                        && (!root.settings.column_folded || root.column_peeking)
                                    {
                                        root.drop_project_by_hint(
                                            workspace,
                                            &dragged.key,
                                            &dragged.projects,
                                            cx,
                                        );
                                    }
                                }))
                                .on_drop(cx.listener(move |root, dragged: &DragGroup, _, cx| {
                                    if dragged.workspace == workspace
                                        && root.active_workspace == workspace
                                        && (!root.settings.column_folded || root.column_peeking)
                                    {
                                        root.drop_group_by_hint(workspace, dragged.id, cx);
                                    }
                                }))
                                // The surface's menu, not a row's: the component
                                // hardcodes the element's id, so a menu per row would
                                // be every row drawing the last row's. The row records
                                // what the pointer was on and this reads it a frame
                                // later.
                                .on_mouse_down(
                                    gpui::MouseButton::Right,
                                    cx.listener(
                                        |root, event: &gpui::MouseDownEvent, window, cx| {
                                            root.open_context_menu(event.position, window, cx)
                                        },
                                    ),
                                ),
                        )
                        // After the list's prepaint: a spacer the final offset
                        // brought into view asks for the frame that draws it.
                        .children(plan.map(|plan| {
                            self.column_spacers.verify(workspace, scroll.clone(), plan)
                        })),
                )
            })
    }

    fn render_workspace_identity(
        &self,
        shown: guild_session::WorkspaceId,
        cx: &mut Context<Root>,
    ) -> gpui::Div {
        let theme = self.theme;
        let workspace = self.desk.workspace(shown);
        let name = workspace
            .map(|workspace| workspace.name.clone())
            .unwrap_or_else(|| "Default".to_owned());
        let icon = workspace
            .and_then(|workspace| workspace.icon.as_deref())
            .and_then(crate::icons::workspace_path);

        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |root, event: &gpui::MouseDownEvent, window, cx| {
                    if event.click_count >= 2 {
                        root.open_rename_workspace(shown, event.position, window, cx);
                    }
                }),
            )
            .when_some(icon, |head, icon| {
                head.child(crate::icons::toolbar(icon).text_color(rgb(theme.text.secondary)))
            })
            .when(icon.is_none(), |head| {
                head.child(
                    div()
                        .size(px(6.0))
                        .flex_none()
                        .rounded(px(3.0))
                        .bg(rgb(theme.text.muted)),
                )
            })
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.primary))
                    .child(name),
            )
    }

    pub(crate) fn render_workspace_switcher(
        &self,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let workspaces: Vec<_> = self
            .desk
            .workspaces()
            .iter()
            .map(|workspace| (workspace.id, workspace.name.clone(), workspace.icon.clone()))
            .collect();

        let legacy = workspaces.len() > guild_session::workspace::CREATION_LIMIT;
        let buttons = div()
            .flex()
            .flex_none()
            .when(legacy, |buttons| buttons.mx_auto())
            .when(!legacy, |buttons| {
                buttons.flex_1().min_w_0().justify_center()
            })
            .items_center()
            .gap(theme::space::STATUS_BUTTON_GAP)
            .children(workspaces.into_iter().map(|(id, name, icon)| {
                let selected = id == self.active_workspace;
                let activity = self.workspace_activity(id);
                let hint = self.ui().activity_counts(&name, activity);
                let asset = icon.as_deref().and_then(crate::icons::workspace_path);
                div()
                    .id(SharedString::from(format!("workspace-switch-{id}")))
                    .debug_selector(move || format!("workspace-switch-{id}"))
                    .tab_index(0)
                    .role(gpui::Role::Button)
                    .aria_label(hint.clone())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .relative()
                    .size(theme::space::STATUS_BUTTON)
                    .when(!legacy, |button| button.flex_shrink(1.).min_w_0())
                    .rounded(theme::radius::STATUS_BUTTON)
                    .cursor_pointer()
                    .when(selected, |button| button.bg(rgb(theme.surface.raised)))
                    .when(!selected, |button| {
                        button.hover(|button| {
                            button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                        })
                    })
                    .hint_delay()
                    .tooltip(move |_window, app| {
                        app.new(|_cx| crate::view::tooltip::Tooltip::text(hint.clone(), theme))
                            .into()
                    })
                    .on_click(cx.listener(move |root, _event, _window, cx| {
                        root.select_workspace(id, cx);
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |root, event: &gpui::MouseDownEvent, window, cx| {
                            root.note_menu_target(crate::view::menu::Target::Workspace {
                                id,
                                last: root.desk.workspaces().len() == 1,
                            });
                            root.open_context_menu(event.position, window, cx);
                        }),
                    )
                    .when_some(asset, |button, asset| {
                        button.child(
                            crate::icons::toolbar(asset)
                                .max_w_full()
                                .text_color(rgb(theme.text.secondary)),
                        )
                    })
                    .when(asset.is_none(), |button| {
                        button.child(crate::view::activity::mark(
                            activity.primary(),
                            theme,
                            true,
                            true,
                        ))
                    })
            }))
            .when(self.desk.can_add_workspace(), |switcher| {
                switcher.child(
                    div()
                        .id("workspace-new")
                        .debug_selector(|| "workspace-new".into())
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .size(theme::space::STATUS_BUTTON)
                        .flex_shrink(1.)
                        .min_w_0()
                        .rounded(theme::radius::STATUS_BUTTON)
                        .cursor_pointer()
                        .text_color(rgb(theme.text.muted))
                        .hover(|button| {
                            button.bg(crate::theme::control_hover(theme, theme.surface.panel))
                        })
                        .hint_delay()
                        .tooltip({
                            let label = self.ui().text(Text::WorkspaceNew).to_owned();
                            move |_window, app| {
                                app.new(|_cx| {
                                    crate::view::tooltip::Tooltip::text(label.clone(), theme)
                                })
                                .into()
                            }
                        })
                        .on_click(cx.listener(|root, event: &gpui::ClickEvent, window, cx| {
                            root.open_new_workspace(event.position(), window, cx);
                        }))
                        .child(
                            crate::icons::glyph("ui/head-plus.svg", theme::size::UI)
                                .text_color(rgb(theme.text.muted)),
                        ),
                )
            });
        div()
            .id("workspace-switcher")
            .flex()
            .flex_1()
            .min_w_0()
            .when(legacy, |switcher| switcher.overflow_x_scroll())
            .child(buttons)
    }

    /// A symmetric target on the column boundary, above the adjacent work.
    /// Pinned columns mount it last in the work row; peeking columns keep
    /// it inside their deferred overlay. The footer and popups stay above it.
    pub(crate) fn render_column_edge(&self, cx: &mut Context<Root>) -> gpui::AnyElement {
        let line = div()
            .debug_selector(|| "column-edge-line".into())
            .absolute()
            .left(px((theme::space::COLUMN_RESIZE_REACH
                - theme::space::COLUMN_RESIZE_LINE)
                / 2.0))
            .top(theme::radius::WORK)
            .bottom(theme::radius::WORK)
            .w(px(theme::space::COLUMN_RESIZE_LINE))
            .group_hover("column-edge", |line| line.bg(rgb(self.theme.text.muted)))
            .when(self.column_edge_drag, |line| {
                line.bg(rgb(self.theme.text.muted))
            });

        div()
            .id("column-edge-grip")
            .debug_selector(|| "column-edge-grip".into())
            .absolute()
            .left(self.column_width() - px(theme::space::COLUMN_RESIZE_REACH / 2.0))
            .top_0()
            .bottom_0()
            .w(px(theme::space::COLUMN_RESIZE_REACH))
            .flex()
            .items_center()
            .justify_center()
            .group("column-edge")
            .cursor(gpui::CursorStyle::ResizeLeftRight)
            .occlude()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(|root, event: &gpui::MouseDownEvent, _window, cx| {
                    // Drag ownership is followed by window listeners even
                    // over the occluding grip, peeking column or a popup.
                    cx.stop_propagation();
                    // A second click asks for the width back and must not
                    // also begin a drag: the hand that double-clicks moves
                    // a little, and the reset would be undone at once.
                    if event.click_count >= 2 {
                        root.reset_column_width(cx);
                        return;
                    }
                    root.grab_column_edge(cx);
                }),
            )
            .child(line)
            .into_any_element()
    }

    /// Project actions and right-aligned Sessions share the global footer baseline.
    pub(crate) fn render_column_foot(&self, cx: &mut Context<Root>) -> gpui::Div {
        let ui = self.ui();
        div()
            .debug_selector(|| "workspace-column-actions".into())
            .child(self.project_drag.measure(project_drag::Surface::Footer))
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .min_w_0()
            .h(theme::space::STATUS_BAR)
            .px(theme::space::WORK_INSET)
            .pb(theme::space::STATUS_BOTTOM)
            .bg(rgb(self.theme.surface.panel))
            .child(
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(theme::space::STATUS_BUTTON_GAP)
                    .child(self.foot_button(
                        "column-open",
                        "ui/column-open.svg",
                        ui.text(Text::ColumnOpen).to_owned(),
                        Some(crate::actions::keymap::Named::AddProject),
                        cx.listener(|root, _, _, cx| root.add_project(cx)),
                    ))
                    .child(self.foot_button(
                        "column-get-from-version-control",
                        "ui/arrow-down.svg",
                        ui.text(Text::GetFromVersionControl).to_owned(),
                        None,
                        cx.listener(|root, _, window, cx| root.open_clone_project(window, cx)),
                    )),
            )
            .when(!self.settings.column_folded, |footer| {
                footer.child(
                    div()
                        .flex()
                        .flex_1()
                        .min_w_0()
                        .items_center()
                        .justify_end()
                        .pl(theme::space::CARD_GAP)
                        .child(self.render_session_summary(cx)),
                )
            })
    }

    /// One repository, and its worktrees under it — `last` when it is the
    /// last drawn repository of its group, `first` when it is the list's
    /// first row.
    fn render_group(
        &self,
        workspace: guild_session::WorkspaceId,
        group: &Group,
        section: Option<guild_session::project_group::ProjectGroupId>,
        last: bool,
        first: bool,
        cx: &mut Context<Root>,
    ) -> gpui::Div {
        let theme = self.theme;
        let search = (workspace == self.active_workspace)
            .then_some(self.sidebar_speed.as_ref())
            .flatten();
        let candidate = search.is_some_and(|s| {
            s.current.as_ref() == Some(&crate::column::Listed::Repository(group.key.clone()))
                && !crate::speed_search::ranges(&group.name, &s.query).is_empty()
        });
        // A member whose main checkout is not directly in its group's
        // folder says so on its head — GLD-055 decision 14.
        let outside = section
            .and_then(|id| self.group_folder_match(workspace, id))
            .is_some_and(|found| {
                found
                    .members
                    .iter()
                    .any(|member| member.key == group.key && !member.inside)
            });
        // The repository's tile: its first letter on its own colour, the
        // same colour every time — M12.4.
        let colour = tile_fill(theme, &group.name);
        let repo_icon = self
            .settings
            .repository_settings
            .get(
                crate::column::opened_in(group)
                    .find_map(|project| self.repository_identity(project).map(|r| &r.key))
                    .unwrap_or(&group.key),
            )
            .and_then(|p| p.icon.as_deref())
            .and_then(crate::icons::workspace_path);
        let tile = tile(colour, theme.surface.app, TILE)
            .debug_selector(|| "project-header-tile".into())
            .when_some(repo_icon, |tile, path| {
                tile.child(
                    gpui::svg()
                        .path(path)
                        .size(theme::space::ICON)
                        .text_color(rgb(theme.surface.app)),
                )
            })
            .when(repo_icon.is_none(), |tile| {
                tile.child(tile_letter(&group.name))
            });
        // Any opened root of this repository: git reaches the whole of it
        // from any worktree, and the one in front is chosen by the deed
        // rather than by the menu.
        let projects: Vec<_> = crate::column::opened_in(group).collect();
        let new_here = projects.first().copied();
        // Carry the opened fallback without touching disk during paint. The menu
        // resolves the current main checkout when opened and when acted on.
        let folder = new_here
            .and_then(|project| self.desk.project(project))
            .map(|project| project.path.clone())
            .unwrap_or_else(|| group.key.clone());
        let target = crate::view::menu::Target::Repository {
            workspace,
            projects: projects.clone(),
            key: group.key.clone(),
            folder,
            name: group.name.clone(),
        };
        let menu_target = target.clone();
        let renaming = self
            .renaming_project()
            .filter(|(key, _)| *key == group.key.as_path())
            .map(|(_, typed)| typed.to_owned());
        // **A project folds from its head** — M12.5 decision 18. The fold is
        // the repository's, kept by the key its name is kept by, and a
        // folded head wears what its hidden rows would have said.
        let folded = self.desk.is_folded(&group.key);
        let head_group = SharedString::from(format!("column-head-{}", group.key.display()));
        let fold_key = group.key.clone();
        let typing_name = renaming.is_some();
        let folded_status = folded.then(|| self.column_group_status_in(workspace, group));
        let drag = DragProject {
            token: uuid::Uuid::new_v4(),
            workspace,
            key: group.key.clone(),
            projects: projects.clone(),
        };
        // The whole block — head, worktrees, tabs — is one drop row: its
        // edges are the places before and after its top-level item, its
        // middle this repository's place, as the head's alone was.
        let row = crate::column::Listed::Repository(group.key.clone());
        let beside = match section {
            Some(id) => crate::column::Top::Group(id),
            None => crate::column::Top::Repository(group.key.clone()),
        };
        let kind = match section {
            Some(_) => crate::column::DropRow::Member { last },
            None => crate::column::DropRow::Repository,
        };
        let hint = self
            .column_drop_hint
            .as_ref()
            .filter(|hint| hint.workspace == workspace && hint.row == row)
            .map(|hint| hint.zone);
        let drag_root = cx.weak_entity();
        let drag_name = group.name.clone();
        let own_key = group.key.clone();
        // What is dragged at the top level: this repository, unless it is
        // inside a group.
        let top = section
            .is_none()
            .then(|| crate::column::Top::Repository(group.key.clone()));

        div()
            .debug_selector({
                let key = format!("project-block-{}", group.key.display());
                move || key.clone()
            })
            .relative()
            .flex()
            .flex_col()
            .child(self.project_drag.measure_row(workspace, row.clone()))
            .on_drag_move(cx.listener({
                let row = row.clone();
                let beside = beside.clone();
                let own_key = own_key.clone();
                move |root, event: &gpui::DragMoveEvent<DragProject>, _, cx| {
                    let dragged = event.drag(cx);
                    if dragged.workspace != workspace {
                        return;
                    }
                    let itself = dragged.key == own_key;
                    root.hover_column_drop(
                        workspace,
                        row.clone(),
                        beside.clone(),
                        kind,
                        crate::column::Dragged::Project,
                        |_| itself,
                        event.bounds,
                        event.event.position,
                        cx,
                    );
                }
            }))
            .on_drag_move(cx.listener({
                let row = row.clone();
                let beside = beside.clone();
                move |root, event: &gpui::DragMoveEvent<DragGroup>, _, cx| {
                    let dragged = event.drag(cx);
                    if dragged.workspace != workspace {
                        return;
                    }
                    let own = section == Some(dragged.id);
                    root.hover_column_drop(
                        workspace,
                        row.clone(),
                        beside.clone(),
                        kind,
                        crate::column::Dragged::Group,
                        |_| own,
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
                            section,
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
                        && section != Some(dragged.id)
                    {
                        root.drop_group_on_row(
                            workspace,
                            dragged.id,
                            row.clone(),
                            section,
                            kind,
                            window,
                            cx,
                        );
                    }
                }
            }))
            // A line inside the group stands at its projects' level; any
            // other at the top level's.
            .children(hint.and_then(|zone| {
                let left = if crate::column::inside_group(kind, zone) {
                    theme::column::TOP_INDENT + theme::column::GROUP_INDENT
                } else {
                    theme::column::TOP_INDENT
                };
                drop_line(theme, zone, left, first)
            }))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "project-header-{}",
                        group.key.display()
                    )))
                    .debug_selector({
                        let key = format!("project-header-{}", group.key.display());
                        move || key.clone()
                    })
                    .capture_any_mouse_down(move |event, window, _| {
                        if !typing_name && event.button == gpui::MouseButton::Left {
                            window.prevent_default();
                        }
                    })
                    .when(candidate, |head| head.bg(rgb(theme.surface.raised)))
                    .group(head_group.clone())
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .h(px(28.0))
                    .rounded(px(8.0))
                    // **The head is a button, as the rows are**: the hand,
                    // and the rows' own raised tint under the pointer.
                    .cursor_pointer()
                    .hover(|head| head.bg(crate::theme::control_hover(theme, theme.surface.panel)))
                    // The middle of a drag's target: this repository's place.
                    .when(hint == Some(crate::column::Zone::Middle), |head| {
                        head.bg(rgb(theme.accent.brass_bg))
                    })
                    .on_drag(drag, move |drag, grabbed, window, cx| {
                        let _ = drag_root.update(cx, |root, cx| {
                            root.begin_project_drag(
                                drag.workspace,
                                drag.token,
                                top.clone(),
                                Some(drag.key.clone()),
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
                        // A click that ends the name being typed is the
                        // field's, not a fold.
                        if typing_name {
                            return;
                        }
                        window.focus(&root.sidebar_focus, cx);
                        root.toggle_group_fold(&fold_key, cx);
                    }))
                    .on_mouse_down(
                        gpui::MouseButton::Right,
                        cx.listener(move |root: &mut Root, _event, _window, _cx| {
                            root.note_menu_target(target.clone());
                        }),
                    )
                    .px(theme::column::HEADER_PADDING)
                    .text_size(theme::size::BODY)
                    .text_color(rgb(theme.text.primary))
                    .child(tile)
                    .child(match renaming {
                        // The project's name being typed, in its head — the
                        // tab's field. M12.2.
                        Some(typed) => div()
                            .debug_selector(|| "project-header-rename".into())
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w_0()
                            .px(px(4.0))
                            .rounded(theme::radius::ROW)
                            .border_1()
                            .border_color(rgb(theme.accent.brass))
                            .bg(rgb(theme.surface.card))
                            .child(self.rename_input(typed.to_owned(), cx))
                            // A click anywhere else keeps what was typed, as
                            // the tab's field does.
                            .child(gpui::deferred(
                                gpui::anchored().position(gpui::Point::default()).child(
                                    div()
                                        .w(self.viewport().width)
                                        .h(self.viewport().height)
                                        .occlude()
                                        .on_mouse_down(
                                            gpui::MouseButton::Left,
                                            cx.listener(|root, _event, window, cx| {
                                                cx.stop_propagation();
                                                root.finish_renaming_tab(true, window, cx);
                                            }),
                                        ),
                                ),
                            ))
                            .into_any_element(),
                        None => div()
                            .debug_selector(|| "project-header-label".into())
                            .min_w_0()
                            .truncate()
                            .child(crate::view::speed_search::label(
                                group.name.clone(),
                                search.map(|s| s.query.as_str()).unwrap_or_default(),
                                theme,
                            ))
                            .into_any_element(),
                    })
                    .when(outside && !typing_name, |head| {
                        head.child(
                            div()
                                .debug_selector(|| "project-header-outside".into())
                                .flex_none()
                                .text_size(theme::size::META)
                                .text_color(rgb(theme.accent.brass))
                                .child(self.ui().text(Text::GroupMemberOutside).to_owned()),
                        )
                    })
                    // The chevron says what a click will do: drawn under the
                    // pointer, and always while folded, so a folded project
                    // reads as one at rest — except on a member outside its
                    // group's folder: there its room is the `outside`
                    // note's, so a short name is not cut to fit beside `…`
                    // and `+`; folded, the missing rows say so (GLD-055
                    // decision 22).
                    .when(!outside, |head| {
                        head.child(
                            div()
                                .debug_selector(|| "project-header-chevron".into())
                                .flex()
                                .flex_none()
                                .items_center()
                                .when(!folded, |chevron| {
                                    chevron
                                        .invisible()
                                        .group_hover(head_group.clone(), |chevron| {
                                            chevron.visible()
                                        })
                                })
                                .child(
                                    crate::icons::chevron(!folded)
                                        .size(px(11.0))
                                        .text_color(rgb(theme.text.muted)),
                                ),
                        )
                    })
                    .when_some(folded_status, |head, status| {
                        head.child(div().flex().flex_none().child(crate::view::activity::mark(
                            status,
                            theme,
                            workspace == self.active_workspace,
                            false,
                        )))
                    })
                    // No dot for the group any more — M12.4: every row
                    // under it wears its own.
                    .when(!typing_name, |head| head.child(div().flex_1()))
                    // **The gesture the column exists for, on the row it
                    // belongs to.** Making a worktree was in the foot and in
                    // a menu nobody opens; a `+` on the repository is where a
                    // person reaches for it. Only where a root of this
                    // repository is open: the new worktree is made from the
                    // `HEAD` of one, and there would be none. M12 task 15.
                    // **The project's own menu** — M12.2: the `⋯` opens what
                    // the right button on the head opens.
                    .child(
                        div()
                            .id(SharedString::from(format!(
                                "column-more-{}",
                                group.key.display()
                            )))
                            .debug_selector(|| "project-header-menu".into())
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .size(theme::column::ACTION_TARGET)
                            .rounded(theme::radius::ROW)
                            .cursor_pointer()
                            // Over the head, which the pointer lights too.
                            .hover(|button| {
                                button.bg(crate::theme::control_hover_over(
                                    theme,
                                    crate::theme::control_hover(theme, theme.surface.panel),
                                ))
                            })
                            .on_click(cx.listener(
                                move |root, event: &gpui::ClickEvent, window, cx| {
                                    cx.stop_propagation();
                                    root.note_menu_target(menu_target.clone());
                                    root.open_context_menu(event.position(), window, cx);
                                },
                            ))
                            .child(
                                crate::icons::toolbar("ui/head-more.svg")
                                    .text_color(rgb(theme.text.muted)),
                            ),
                    )
                    .when_some(new_here, |head, project| {
                        head.child(
                            div()
                                .id(SharedString::from(format!("column-new-{}", group.name)))
                                .debug_selector(|| "project-header-new".into())
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
                                .on_click(cx.listener(move |root, _event, window, cx| {
                                    cx.stop_propagation();
                                    root.new_worktree_here(project, window, cx);
                                }))
                                .child(
                                    crate::icons::toolbar("ui/head-plus.svg")
                                        .text_color(rgb(theme.text.muted)),
                                ),
                        )
                    }),
            )
            .when(!folded, |group_div| {
                group_div.children(self.column_rows_shown(workspace, group).map(|row| {
                    let element = self.render_ordered_row(workspace, group, row, cx);
                    self.with_worktree_tabs(workspace, row, element, cx)
                }))
            })
    }

    fn render_ordered_row(
        &self,
        workspace: guild_session::WorkspaceId,
        group: &Group,
        row: &Row,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        use crate::root::column::DragWorktree;
        use guild_session::worktree_order::Insertion;
        let element = self.render_row(workspace, row, cx);
        let Some(path) = self.column_row_path(row) else {
            return element;
        };
        if group.plain {
            return element;
        }
        let (primary, name) = match row {
            Row::Opened { title, .. } | Row::Unopened { title, .. } => {
                (title.primary, title.name.clone())
            }
        };
        let theme = self.theme;
        let mut element = element
            .relative()
            .capture_any_mouse_down(|event, window, _| {
                if event.button == gpui::MouseButton::Left {
                    window.prevent_default();
                }
            });
        if !primary {
            let root = cx.weak_entity();
            element = element.on_drag(
                DragWorktree {
                    workspace,
                    repository: group.key.clone(),
                    path: path.clone(),
                },
                move |_, grabbed, _, cx| {
                    let _ = root.update(cx, |root, cx| {
                        root.worktree_dragging = true;
                        cx.notify();
                    });
                    cx.new(|_| ProjectDragPreview {
                        name: name.clone(),
                        theme,
                        grabbed,
                    })
                },
            );
        }
        // Permanent non-occluding targets preserve click/context-menu hit testing.
        // They paint only during a valid row drag; each half points at its own edge.
        for after in [false, true] {
            if primary && !after {
                continue;
            }
            let insertion = if after {
                Insertion::After(path.clone())
            } else {
                Insertion::Before(path.clone())
            };
            let highlight = insertion.clone();
            let allowed = insertion.clone();
            let repository = group.key.clone();
            let highlighted_repository = repository.clone();
            let allowed_repository = repository.clone();
            let root = cx.weak_entity();
            let can_drop_root = root.clone();
            element = element.child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .h(gpui::relative(0.5))
                    .when(after, |zone| zone.bottom_0().border_b_1())
                    .when(!after, |zone| zone.top_0().border_t_1())
                    .border_color(gpui::transparent_black())
                    .can_drop(move |value, _, cx| {
                        value.downcast_ref::<DragWorktree>().is_some_and(|drag| {
                            can_drop_root
                                .read_with(cx, |root, _| {
                                    root.worktree_drop_order(
                                        drag,
                                        workspace,
                                        &allowed_repository,
                                        &allowed,
                                    )
                                    .is_some()
                                })
                                .unwrap_or(false)
                        })
                    })
                    .drag_over::<DragWorktree>(move |style, drag, _, cx| {
                        if root
                            .read_with(cx, |root, _| {
                                root.worktree_drop_order(
                                    drag,
                                    workspace,
                                    &highlighted_repository,
                                    &highlight,
                                )
                                .is_some()
                            })
                            .unwrap_or(false)
                        {
                            style.border_color(rgb(theme.accent.patina))
                        } else {
                            style
                        }
                    })
                    .on_drop(cx.listener(move |root, drag: &DragWorktree, _, cx| {
                        root.reorder_worktree(drag, workspace, &repository, &insertion, cx);
                    })),
            );
        }
        element
    }

    /// What rows that just became spacers owned, let go of: the tooltip
    /// their action showed, and the worktree or session tab hovered in
    /// them, which GPUI will never tell them the pointer left.
    fn forget_spaced_rows(
        &self,
        workspace: guild_session::WorkspaceId,
        gone: &[spacers::RowShape],
        cx: &mut Context<Root>,
    ) {
        let owned = self
            .worktree_tooltip_owner
            .borrow()
            .as_ref()
            .is_some_and(|owner| gone.iter().any(|row| &row.key == owner));
        if owned {
            self.worktree_tooltip_owner.replace(None);
            self.worktree_tooltips
                .update(cx, |overlay, cx| overlay.hide(cx));
        }
        let worktree = self.hovered_worktree.clone().filter(|(at, path)| {
            *at == workspace && gone.iter().any(|row| row.path.as_ref() == Some(path))
        });
        let tab = self.hovered_column_tab.filter(|id| {
            self.tabs.iter().any(|tab| {
                tab.id == *id
                    && tab.seat.workspace == workspace
                    && gone.iter().any(|row| row.project == Some(tab.seat.project))
            })
        });
        if worktree.is_none() && tab.is_none() {
            return;
        }
        // After this draw, and only if nothing has hovered since.
        let root = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = root.update(cx, |root, cx| {
                if worktree.is_some() && root.hovered_worktree == worktree {
                    root.hovered_worktree = None;
                }
                if tab.is_some() && root.hovered_column_tab == tab {
                    root.hovered_column_tab = None;
                }
                cx.notify();
            });
        });
    }

    fn worktree_action(
        &self,
        id: &'static str,
        row: String,
        icon: &'static str,
        label: String,
        light: ActionLight,
        click: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let ActionLight {
            destructive,
            ground,
        } = light;
        let tint = if destructive {
            theme.accent.rust
        } else {
            theme.text.secondary
        };
        let bounds = std::rc::Rc::new(std::cell::Cell::new(gpui::Bounds::default()));
        let measured = bounds.clone();
        let glyph_selector = format!("{id}-{row}-glyph");
        let overlay = self.worktree_tooltips.clone();
        let clicked_overlay = overlay.clone();
        // Whose request the shared tooltip holds: a row a spacer replaces
        // takes only its own.
        let owner = self.worktree_tooltip_owner.clone();
        let owner_row = row.clone();
        let action = match id {
            "worktree-copy" => Some(crate::actions::keymap::Named::CopyWorktreePath),
            "worktree-delete" => Some(crate::actions::keymap::Named::RemoveWorktree),
            _ => None,
        };
        div()
            .id(id)
            .debug_selector(move || format!("{id}-{row}"))
            .tab_index(0)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::WORKTREE_ACTION)
            .rounded(theme::radius::WORKTREE_ACTION)
            .cursor_pointer()
            .capture_any_mouse_down(|event, window, _cx| {
                if event.button == gpui::MouseButton::Left {
                    window.prevent_default();
                }
            })
            .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
            // Over the row the control is drawn on, not over a surface
            // it may not be: on a row the pointer lights, `raised` was
            // barely apart from the row's own fill.
            .hover(move |button| {
                button.bg(if destructive {
                    gpui::Hsla::from(ground).blend(
                        gpui::Hsla::from(rgb(theme.accent.rust))
                            .opacity(theme::alpha::WORKTREE_DESTRUCTIVE_HOVER),
                    )
                } else {
                    gpui::Hsla::from(crate::theme::control_hover_over(theme, ground))
                })
            })
            // GPUI's cursor tooltip only starts on MouseMove. These controls
            // become visible under a stationary pointer; observe prepaint hover
            // and use Kit's shared timer/anchored overlay instead.
            .on_prepaint(move |value, _, _| measured.set(value))
            .on_hover(move |hovered, window, app| {
                if *hovered {
                    owner.replace(Some(owner_row.clone()));
                } else if owner.borrow().as_ref() == Some(&owner_row) {
                    owner.replace(None);
                }
                overlay.update(app, |overlay, cx| {
                    if *hovered {
                        let label = label.clone();
                        overlay.request_show(
                            gpui_kit::base::TooltipRequest::new(bounds.get(), move |_, app| {
                                app.new(|_| {
                                    crate::view::tooltip::Tooltip::worktree(
                                        label.clone(),
                                        action,
                                        theme,
                                    )
                                })
                                .into()
                            })
                            .placement(gpui_kit::base::Placement::Top),
                            window,
                            cx,
                        );
                    } else {
                        overlay.request_hide(window, cx);
                    }
                });
            })
            .child(
                div()
                    .debug_selector(move || glyph_selector)
                    .size(theme::space::WORKTREE_GLYPH)
                    .child(
                        crate::icons::toolbar(icon)
                            .size_full()
                            .text_color(rgb(tint)),
                    ),
            )
            .on_click(move |event, window, app| {
                clicked_overlay.update(app, |overlay, cx| overlay.hide(cx));
                click(event, window, app);
            })
    }

    /// One worktree — M12.4: what its
    /// agents are doing in the indent, git's branch mark, its name and the
    /// lines its branch added and removed, and under the name how long ago
    /// it was committed to.
    fn render_row(
        &self,
        workspace: guild_session::WorkspaceId,
        row: &Row,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        self.render_row_as(workspace, row, None, cx)
    }

    /// A bound group's folder as the first row under its header — GLD-055
    /// decisions 2, 13 and 14: labelled by its branch or its name, its match
    /// on the second line, its session tabs under it as a worktree's are.
    /// Open or not: one not open is drawn as an unopened worktree is, and
    /// opened on its click (decision 20).
    fn render_group_folder(
        &self,
        workspace: guild_session::WorkspaceId,
        head: &crate::column::Head,
        cx: &mut Context<Root>,
    ) -> Option<gpui::Div> {
        let row = self.group_folder_row(workspace, head)?;
        let (meta, outside) = match self.group_folder_match(workspace, head.id) {
            Some(found) => (
                self.ui().group_inside(found.inside(), found.members.len()),
                !found.matches(),
            ),
            None => (String::new(), false),
        };
        let look = FolderLook {
            group: head.id,
            meta,
            outside,
        };
        let element = self.render_row_as(workspace, &row, Some(look), cx);
        Some(
            div()
                .pl(theme::column::GROUP_INDENT)
                .debug_selector(|| "group-folder-row".into())
                .child(self.with_worktree_tabs(workspace, &row, element, cx)),
        )
    }

    /// A bound group's folder as a worktree row: opened or not, labelled by
    /// its branch or its name. `None` while collapsed or unbound.
    pub(super) fn group_folder_row(
        &self,
        workspace: guild_session::WorkspaceId,
        head: &crate::column::Head,
    ) -> Option<Row> {
        let folder = head.folder.clone().filter(|_| !head.collapsed)?;
        let title = crate::column::Title {
            name: self.group_folder_label(&folder),
            primary: false,
        };
        Some(
            match self
                .desk
                .projects_of(workspace)
                .find(|project| project.path == folder)
            {
                Some(project) => Row::Opened {
                    project: project.id,
                    title,
                    branch: None,
                    committed: None,
                    lines: None,
                },
                None => Row::Unopened {
                    path: folder,
                    title,
                    branch: None,
                    committed: None,
                    lines: None,
                },
            },
        )
    }

    /// A worktree's row, or — with `folder` — a project group's folder drawn
    /// as one, GLD-055 decision 13: a folder where the branch glyph is, the
    /// match line where the age is, *Copy Path* and `+` for its actions, and
    /// the group's menu on the right button.
    fn render_row_as(
        &self,
        workspace: guild_session::WorkspaceId,
        row: &Row,
        folder_look: Option<FolderLook>,
        cx: &mut Context<Root>,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        let ui = self.ui();
        let line_height = px(20. * self.settings.ui_font_size / crate::scale::BASE_UI_FONT_SIZE);
        let (title, project, committed, lines) = match row {
            Row::Opened {
                title,
                project,
                committed,
                lines,
                ..
            } => (title, Some(*project), *committed, *lines),
            Row::Unopened {
                title,
                committed,
                lines,
                ..
            } => (title, None, *committed, *lines),
        };
        // How long ago this worktree's head was committed. The clock is
        // read once per row and not stored: a time that ages while the
        // window is open is a time that has to be redrawn anyway, and the
        // column is redrawn whenever git answers — every three seconds.
        let age = committed.and_then(|at| {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs() as i64;
            (now >= at).then(|| ui.ago(now - at))
        });
        let name = title.name.clone();
        let selected = workspace == self.active_workspace
            && project.is_some()
            && project == self.active_project;
        let activity = project
            .map(|project| {
                self.board.summary(
                    self.desk
                        .sessions_of(guild_session::Seat { workspace, project })
                        .map(|record| record.id),
                )
            })
            .unwrap_or_default();
        let hint = ui.activity_counts(&name, activity);
        let secondary = activity.secondary();
        // The lines its branch added and removed, with what is not
        // committed — `+677 −298`, M12.4. Nothing when
        // there are none.
        let changed =
            lines
                .filter(|(added, removed)| *added > 0 || *removed > 0)
                .map(|(added, removed)| {
                    div()
                        .debug_selector({
                            let key = row_key(workspace, row);
                            move || format!("worktree-diff-{key}")
                        })
                        .flex()
                        .flex_none()
                        .gap(px(5.0))
                        .font_family(self.code_font.clone())
                        .text_size(theme::size::META)
                        // A half that is zero is not drawn:
                        // `−0` in rust is a colour saying
                        // there is something where there
                        // is nothing.
                        .when(added > 0, |numbers| {
                            numbers.child(
                                div()
                                    .text_color(rgb(theme.accent.green))
                                    .child(format!("+{}", crate::column::compact_lines(added))),
                            )
                        })
                        .when(removed > 0, |numbers| {
                            numbers.child(div().text_color(rgb(theme.accent.rust)).child(format!(
                                "\u{2212}{}",
                                crate::column::compact_lines(removed)
                            )))
                        })
                });
        let chosen = row.clone();
        // What the right button's menu is about. An opened row knows its
        // folder through the workspace and an unopened one carries it,
        // because it has no project to ask.
        let folder = match row {
            Row::Opened { project, .. } => self
                .desk
                .project(*project)
                .map(|project| project.path.clone())
                .unwrap_or_default(),
            Row::Unopened { path, .. } => path.clone(),
        };
        let target = match &folder_look {
            Some(look) => crate::view::menu::Target::ProjectGroup {
                workspace,
                id: look.group,
            },
            None => crate::view::menu::Target::Worktree {
                // Whether git holds a lock on it: the menu offers to take a
                // lock off only where there is one. M12 task 18.
                locked: self.lock_on_worktree(&folder).is_some(),
                path: folder.clone(),
                project,
                primary: title.primary,
                mr: self.forge_url(&folder).is_some(),
            },
        };

        // Hover changes layout, not just paint. GPUI group-hover refinements
        // cannot allocate a sibling's width; notify the cached column instead.
        let hover_target = (workspace, folder.clone());
        let actions_visible = self.hovered_worktree.as_ref() == Some(&hover_target);
        // What the row's controls are drawn on while the pointer is in the
        // row — they show only then, or light only then — so that their
        // own hover lights over it rather than repainting the row's fill.
        let row_ground = if selected {
            crate::theme::column_selected(theme)
        } else {
            crate::theme::control_hover(theme, theme.surface.panel)
        };
        // The branch's merge request and its unpushed commits — GLD-063.
        // A group's folder row is not a worktree and gets neither.
        let shown = if folder_look.is_none() {
            self.forge_shown(&folder)
        } else {
            crate::forge::Shown::Nothing
        };
        let ahead = folder_look
            .is_none()
            .then(|| {
                self.worktrees
                    .values()
                    .flatten()
                    .find(|tree| tree.path == folder)
                    .and_then(|tree| tree.ahead)
            })
            .flatten()
            .filter(|n| *n > 0);
        // The badge's ring is the row's own ground, at rest, lit or chosen.
        let ring = if selected || actions_visible {
            row_ground
        } else {
            gpui::rgb(theme.surface.panel)
        };
        let age = match &shown {
            crate::forge::Shown::Mr { mr, .. } => {
                Some(ui.mr_meta(mr.iid, age.as_deref().unwrap_or_default()))
            }
            _ => age,
        };
        let mr_slot = self.worktree_mr_slot(&shown, ahead, ring, folder_look.is_some(), &folder);
        // The chevron over the list of this worktree's tabs, and their
        // number while it is collapsed.
        let tabs_toggle = self.worktree_tabs_toggle(
            workspace,
            project.map(|_| folder.clone()),
            self.worktree_tab_rows(workspace, row, cx).len(),
            actions_visible,
            row_ground,
            cx,
        );
        let pinned = self.worktree_pinned(workspace, &folder);
        let pin_allowed =
            folder_look.is_none() && !matches!(title.name.as_str(), "main" | "master");
        let new_session = folder_look.as_ref().map(|look| look.group);
        let copy_path = folder.clone();
        let pin_path = folder.clone();
        let remove_path = folder.clone();
        let primary = title.primary;
        let actions = div()
            .h(line_height)
            .flex_none()
            .flex()
            .items_center()
            .when(!actions_visible, |actions| actions.invisible().w_0())
            .child(self.worktree_action(
                "worktree-copy",
                row_key(workspace, row),
                "ui/menu-copy.svg",
                ui.text(Text::MenuCopyPath).to_owned(),
                ActionLight {
                    destructive: false,
                    ground: row_ground,
                },
                cx.listener(move |_root, _, _, cx| {
                    cx.stop_propagation();
                    cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                        copy_path.display().to_string(),
                    ));
                }),
            ))
            .when(pin_allowed, |actions| {
                actions.child(
                    self.worktree_action(
                        "worktree-pin",
                        row_key(workspace, row),
                        "ui/head-pin.svg",
                        ui.text(if pinned {
                            Text::WorktreeUnpin
                        } else {
                            Text::WorktreePin
                        })
                        .to_owned(),
                        ActionLight {
                            destructive: false,
                            ground: row_ground,
                        },
                        cx.listener(move |root, _, _, cx| {
                            cx.stop_propagation();
                            root.toggle_worktree_pin(workspace, &pin_path, cx);
                        }),
                    ),
                )
            })
            // A group's folder: a session in it, where a worktree has its
            // removal — decision 13.
            .when_some(new_session, |actions, group| {
                actions.child(self.worktree_action(
                    "group-folder-new",
                    row_key(workspace, row),
                    "ui/head-plus.svg",
                    ui.text(Text::GroupFolderNewSession).to_owned(),
                    ActionLight {
                        destructive: false,
                        ground: row_ground,
                    },
                    cx.listener(move |root, _, window, cx| {
                        cx.stop_propagation();
                        let at = window.mouse_position();
                        root.new_group_session(workspace, group, at, window, cx);
                    }),
                ))
            })
            .when(!primary && new_session.is_none(), |actions| {
                actions.child(self.worktree_action(
                    "worktree-delete",
                    row_key(workspace, row),
                    "ui/menu-remove.svg",
                    ui.text(Text::WorktreeDeleteTooltip).to_owned(),
                    ActionLight {
                        destructive: true,
                        ground: row_ground,
                    },
                    cx.listener(move |root, _, window, cx| {
                        cx.stop_propagation();
                        root.remove_worktree_asking(
                            remove_path.clone(),
                            project,
                            primary,
                            window,
                            cx,
                        );
                    }),
                ))
            });

        div()
            // **The row's own key, never its name.** Two repositories
            // open at once give two rows called `primary`, and one
            // ElementId between them is one row's state — the second
            // row's click went to the first. See `column::row_key`.
            .id(SharedString::from(row_key(workspace, row)))
            .debug_selector({
                let key = row_key(workspace, row);
                move || key.clone()
            })
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(move |root: &mut Root, _event, _window, _cx| {
                    root.note_menu_target(target.clone());
                }),
            )
            .on_hover(cx.listener(move |root, hovered: &bool, _, cx| {
                if *hovered {
                    if root.hovered_worktree.as_ref() != Some(&hover_target) {
                        root.hovered_worktree = Some(hover_target.clone());
                        cx.notify();
                    }
                } else if root.hovered_worktree.as_ref() == Some(&hover_target) {
                    root.hovered_worktree = None;
                    cx.notify();
                }
            }))
            .flex()
            .items_start()
            .pl(px(6.0))
            .pr(px(10.0))
            // Four above and below: a card about fifty
            // high with its two twenty-point lines.
            .pt(px(4.0))
            .pb(px(4.0))
            .rounded(theme::radius::WORKTREE_ROW)
            // The outline is always there and only coloured in front, so
            // the row does not move by a point when it is chosen.
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_pointer()
            // **One row lit, and outlined** — M12.4. Every project's row
            // in front used to be lit, which is the second reason the
            // projects ran together. Lit with
            // the column's faint selection fill — the one its tab in front
            // wears — with the outline carrying the selection, and the
            // hover, louder, apart from both.
            .when(selected, |row| {
                row.bg(crate::theme::column_selected(theme))
                    .border_color(rgb(theme.text.gutter))
            })
            .when(!selected, |row| {
                row.hover(|row| row.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            })
            .on_click(cx.listener(move |root, _event, _window, cx| {
                if workspace != root.active_workspace {
                    root.select_workspace(workspace, cx);
                }
                root.choose_row(&chosen, cx);
            }))
            .child(
                div()
                    .id(SharedString::from(format!(
                        "activity-{}",
                        row_key(workspace, row)
                    )))
                    .tab_index(0)
                    .role(gpui::Role::Button)
                    .aria_label(hint.clone())
                    // Preserve the prior input owner before GPUI's default focus transfer.
                    // Preventing default in capture leaves normal click generation intact.
                    .capture_any_mouse_down(|event, window, _| {
                        if event.button == gpui::MouseButton::Left {
                            window.prevent_default();
                        }
                    })
                    .debug_selector(|| "activity-control".into())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .w(theme::activity::SLOT_WIDTH)
                    .h(theme::activity::SLOT_HEIGHT)
                    .hint_delay()
                    .tooltip(move |_window, app| {
                        app.new(|_| crate::view::tooltip::Tooltip::text(hint.clone(), theme))
                            .into()
                    })
                    .on_click(
                        cx.listener(move |root, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            if let Some(project) = project {
                                root.open_activity(
                                    guild_session::Seat { workspace, project },
                                    event.position(),
                                    window,
                                    cx,
                                );
                            }
                        }),
                    )
                    .on_key_down(cx.listener(
                        move |root, event: &gpui::KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                cx.stop_propagation();
                                if let Some(project) = project {
                                    root.open_activity(
                                        guild_session::Seat { workspace, project },
                                        window.mouse_position(),
                                        window,
                                        cx,
                                    );
                                }
                            }
                        },
                    ))
                    .child(crate::view::activity::mark(
                        activity.primary(),
                        theme,
                        workspace == self.active_workspace,
                        false,
                    )),
            )
            .child(mr_slot)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .ml(px(6.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .min_w_0()
                            .h(line_height)
                            .relative()
                            .child(
                                div()
                                    .debug_selector({
                                        let key = row_key(workspace, row);
                                        move || format!("worktree-label-{key}")
                                    })
                                    .flex_1()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .truncate()
                                            .text_size(theme::size::BODY)
                                            .text_color(rgb(if selected {
                                                theme.text.primary
                                            } else {
                                                theme.text.secondary
                                            }))
                                            .debug_selector(|| "activity-branch".into())
                                            .child(name.clone()),
                                    ),
                            )
                            // **The repository's own checkout wears a star**,
                            // where M12 wrote
                            // the word *primary* in place of its branch.
                            .when(title.primary, |line| {
                                line.child(
                                    crate::icons::glyph("ui/star-filled.svg", theme::size::META)
                                        .text_color(rgb(theme.text.faint)),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .items_center()
                                    .children(changed.filter(|_| !actions_visible))
                                    .child(actions),
                            ),
                    )
                    // Age and independent secondary states share the second line.
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .min_w_0()
                            .h(line_height)
                            .debug_selector(|| "activity-meta-line".into())
                            .overflow_hidden()
                            .child(match &folder_look {
                                // How the group stands against its folder —
                                // decision 14: brass while a member is out.
                                Some(look) => div()
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(theme::size::META)
                                    .text_color(rgb(if look.outside {
                                        theme.accent.brass
                                    } else {
                                        theme.text.faint
                                    }))
                                    .debug_selector(|| "group-folder-match".into())
                                    .child(look.meta.clone()),
                                // `!294 · 1m ago · ↑2` — GLD-063.
                                None => div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .gap(px(4.))
                                    .text_size(theme::size::META)
                                    .child(
                                        div()
                                            .min_w_0()
                                            .truncate()
                                            .text_color(rgb(theme.text.faint))
                                            .debug_selector(|| "activity-age".into())
                                            .child(age.unwrap_or_default()),
                                    )
                                    .when_some(ahead, |line, n| {
                                        line.child(
                                            div()
                                                .flex_none()
                                                .text_color(rgb(theme.text.secondary))
                                                .debug_selector(|| "worktree-ahead".into())
                                                .child(format!("· ↑{n}")),
                                        )
                                    }),
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_none()
                                    .gap(px(4.))
                                    .whitespace_nowrap()
                                    .text_size(theme::size::META)
                                    .debug_selector(|| "activity-secondary".into())
                                    .children(secondary.into_iter().map(|count| {
                                        div()
                                            .debug_selector({
                                                let text = count.text.clone();
                                                move || format!("activity-count-{text}")
                                            })
                                            .text_color(rgb(crate::view::activity::color(
                                                count.status,
                                                theme,
                                            )))
                                            .child(count.text)
                                    })),
                            )
                            .children(tabs_toggle),
                    ),
            )
    }

    /// The leading glyph's 14×18 slot: a group's folder, the plain branch
    /// glyph, or — GLD-063 — the branch's merge request, which a click
    /// opens and whose hover is the path to merge. The plain glyph's hover
    /// says why there is none only where that helps.
    fn worktree_mr_slot(
        &self,
        shown: &crate::forge::Shown<'_>,
        ahead: Option<u32>,
        ring: gpui::Rgba,
        folder_row: bool,
        folder: &std::path::Path,
    ) -> gpui::AnyElement {
        use crate::view::tooltip::Hint as _;
        let theme = self.theme;
        let ui = self.ui();
        let slot = |id: String| {
            div()
                .id(gpui::ElementId::Name(id.into()))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .ml(px(2.0))
                .w(px(14.0))
                .h(px(18.0))
        };
        let plain = |slot: gpui::Stateful<gpui::Div>| {
            slot.child(
                crate::icons::glyph(
                    if folder_row {
                        "ui/workspace-folder.svg"
                    } else {
                        "ui/workspace-git-branch.svg"
                    },
                    theme::size::META,
                )
                .text_color(rgb(theme.text.faint)),
            )
        };
        let key = folder.display().to_string();
        match shown {
            crate::forge::Shown::Mr {
                mr, failure, at, ..
            } => {
                // The age is the last successful read's; the failure only
                // says why the refresh after it did not land (code review
                // r1, bugs+impl-14).
                let card = crate::view::forge::PathCard::of(
                    mr,
                    ahead,
                    failure.map(|(error, _)| ui.mr_failure(error, at.elapsed().as_secs() as i64)),
                    ui,
                    theme,
                );
                let lines = card.lines.clone();
                let (iid, title) = (card.iid, card.title.clone());
                let failure = card.failure.clone();
                let url = mr.web_url.clone();
                slot(format!("worktree-mr-{key}"))
                    .cursor_pointer()
                    .child(crate::view::forge::glyph(mr, theme, ring))
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(move |_, _, cx| {
                        cx.stop_propagation();
                        cx.open_url(&url);
                    })
                    .hint_delay()
                    .tooltip(move |_, app| {
                        let lines = lines.clone();
                        let title = title.clone();
                        let failure = failure.clone();
                        app.new(move |_| crate::view::forge::PathCard {
                            iid,
                            title,
                            lines,
                            failure,
                            theme,
                        })
                        .into()
                    })
                    .into_any_element()
            }
            crate::forge::Shown::Why(why) => plain(slot(format!("worktree-glyph-{key}")))
                .debug_selector(|| "worktree-mr-why".into())
                .hint(ui.mr_why(why), theme)
                .into_any_element(),
            // Asked and never answered: the plain glyph, whose hover says it
            // is unknown and why — not the silence of a branch with no MR
            // (code review r1, adversarial-9).
            crate::forge::Shown::Unknown { error, .. } => {
                plain(slot(format!("worktree-glyph-{key}")))
                    .debug_selector(|| "worktree-mr-unknown".into())
                    .hint(ui.mr_unknown(error), theme)
                    .into_any_element()
            }
            crate::forge::Shown::Nothing => {
                plain(slot(format!("worktree-glyph-{key}"))).into_any_element()
            }
        }
    }

    /// One of the head's two gestures: an icon, with what it does — and its
    /// stroke, when it has one — in its tooltip. The hide button's size.
    fn foot_button(
        &self,
        id: &'static str,
        icon: &'static str,
        label: String,
        action: Option<crate::actions::keymap::Named>,
        on_click: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.into())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::STATUS_BUTTON)
            .rounded(theme::radius::STATUS_BUTTON)
            .cursor_pointer()
            .hover(|button| button.bg(crate::theme::control_hover(theme, theme.surface.panel)))
            .child(crate::icons::toolbar(icon).text_color(rgb(theme.text.secondary)))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| match action {
                    Some(action) => {
                        crate::view::tooltip::Tooltip::for_action(label.clone(), action, theme)
                    }
                    None => crate::view::tooltip::Tooltip::text(label.clone(), theme),
                })
                .into()
            })
            .on_click(on_click)
    }
}

#[cfg(test)]
mod activity_tests;
#[cfg(test)]
mod forge_tests;

#[cfg(test)]
mod order_tests;

#[cfg(test)]
mod sessions_tests;

#[cfg(test)]
mod perf_bench;

#[cfg(test)]
mod spacer_tests;

impl Root {
    fn project_sort_button(
        &self,
        workspace: guild_session::WorkspaceId,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        use gpui_component::{
            Sizable as _,
            button::{Button, ButtonVariants as _},
        };
        let anchor = std::rc::Rc::new(std::cell::Cell::new(None));
        let measured = anchor.clone();
        let button = Button::new("project-sort")
            .flex_none()
            .small()
            .min_w(theme::space::HEAD_BUTTON)
            .child(
                gpui_component::Icon::default()
                    .path("ui/project-sort.svg")
                    .size(theme::size::META),
            )
            .custom(
                crate::view::controls::ghost(self.theme, false, cx)
                    .foreground(rgb(self.theme.text.faint).into()),
            )
            .debug_selector(|| "project-sort".into())
            .tooltip(self.ui().text(Text::ProjectSort).to_owned())
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
                if let Some(bounds) = anchor.get() {
                    root.open_menu(
                        crate::view::menu::Target::ProjectSort { workspace },
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
