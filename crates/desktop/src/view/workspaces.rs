//! The one-line field used to make or rename a workspace, and the icon
//! picker its menu opens.

use gpui::prelude::*;
use gpui::{Context, MouseButton, SharedString, anchored, deferred, div, px, rgb};
use guild_session::WorkspaceId;

use crate::i18n::Text;
use crate::{Root, theme};

#[derive(Clone)]
pub(crate) enum IconTarget {
    Workspace(WorkspaceId),
    Repository(std::path::PathBuf),
}
impl Root {
    fn picker_icon(&self, target: &IconTarget) -> Option<String> {
        match target {
            IconTarget::Workspace(id) => self.desk.workspace(*id).and_then(|w| w.icon.clone()),
            IconTarget::Repository(key) => self
                .settings
                .repository_settings
                .get(key)
                .and_then(|p| p.icon.clone()),
        }
    }
}

/// How many of the icon picker's cells sit in a row.
///
/// The card is a fixed `px(260.0)` wide with `px(8.0)` of padding on
/// each side and a `px(4.0)` gap between cells, and a cell is
/// `px(30.0)`: `30n + 4(n - 1) ≤ 244` fits seven and not eight. Kept as
/// a constant rather than measured from a laid-out frame because
/// `grid_step` has to agree with the flex-wrap row a key press before
/// paint ever sees.
const ICON_COLUMNS: usize = 7;

/// Where an arrow, `up`/`down`/`left`/`right`, takes the icon picker's
/// highlight — pure, so a test can walk it without a window.
///
/// **The grid's addressable space is one number wider than its icons**:
/// index `0` is *No icon*, a row of its own above the grid, and `1..`
/// are the cells in `WORKSPACE_NAMES` order, `columns` to a row. `up`
/// from the first row of cells reaches `0`, and `down` from `0` reaches
/// the first cell — the only two moves that cross between the row and
/// the grid.
///
/// `left`/`right` move by one within a row and `up`/`down` by `columns`;
/// none of the four wrap, and a move that would leave the grid — off
/// either end of a row, or off a ragged last row that is shorter than
/// `columns` — leaves the highlight where it was. `at: None` (nothing
/// lit yet) answers `current` if there is one, `0` otherwise: the first
/// key only reveals where the selection already is, the way `pick::step`
/// answers the first row of a list rather than also stepping past it.
pub fn grid_step(
    len: usize,
    columns: usize,
    at: Option<usize>,
    key: &str,
    current: Option<usize>,
) -> Option<usize> {
    if len == 0 || columns == 0 {
        return None;
    }
    let last = len - 1;
    let Some(at) = at.map(|index| index.min(last)) else {
        return Some(current.map(|index| index.min(last)).unwrap_or(0));
    };
    // Index 0 is *No icon*, alone above the grid.
    if at == 0 {
        return Some(if key == "down" && len > 1 { 1 } else { 0 });
    }
    let cell = at - 1;
    let cells = len - 1;
    let row = cell / columns;
    let col = cell % columns;
    let moved = match key {
        "left" if col > 0 => Some(cell - 1),
        "right" if col + 1 < columns && cell + 1 < cells => Some(cell + 1),
        "up" if row == 0 => return Some(0),
        "up" => Some(cell - columns),
        "down" if cell + columns < cells => Some(cell + columns),
        _ => None,
    };
    Some(moved.map_or(at, |cell| cell + 1))
}

pub(crate) enum WorkspaceField {
    Create {
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
    Rename {
        id: WorkspaceId,
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
    /// *New Group…*: the name of a project group made with this
    /// repository in it.
    NewProjectGroup {
        workspace: WorkspaceId,
        key: std::path::PathBuf,
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
    /// A project let go of on another: whether to merge the two into a new
    /// group, and its name. `target` is the project dropped on, whose place
    /// the group takes.
    MergeProjects {
        workspace: WorkspaceId,
        target: std::path::PathBuf,
        dragged: std::path::PathBuf,
        names: (String, String),
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
    /// *Rename Group…*.
    RenameProjectGroup {
        workspace: WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
    /// The bind question — GLD-055 decision 17: the folder a group's
    /// sessions start in, the suggested one in the field or none.
    BindGroupFolder {
        workspace: WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        /// The group's name, for the question.
        name: String,
        /// Whether a folder was suggested; without one the question says
        /// why the field is empty.
        suggested: bool,
        /// Start a session in the folder once it is bound — the first
        /// group session's question.
        then_session: bool,
        typed: String,
        at: gpui::Point<gpui::Pixels>,
    },
}

impl WorkspaceField {
    fn typed(&self) -> &str {
        match self {
            Self::Create { typed, .. }
            | Self::Rename { typed, .. }
            | Self::NewProjectGroup { typed, .. }
            | Self::MergeProjects { typed, .. }
            | Self::RenameProjectGroup { typed, .. }
            | Self::BindGroupFolder { typed, .. } => typed,
        }
    }
    fn typed_mut(&mut self) -> &mut String {
        match self {
            Self::Create { typed, .. }
            | Self::Rename { typed, .. }
            | Self::NewProjectGroup { typed, .. }
            | Self::MergeProjects { typed, .. }
            | Self::RenameProjectGroup { typed, .. }
            | Self::BindGroupFolder { typed, .. } => typed,
        }
    }
    fn at(&self) -> gpui::Point<gpui::Pixels> {
        match self {
            Self::Create { at, .. }
            | Self::Rename { at, .. }
            | Self::NewProjectGroup { at, .. }
            | Self::MergeProjects { at, .. }
            | Self::RenameProjectGroup { at, .. }
            | Self::BindGroupFolder { at, .. } => *at,
        }
    }
}

impl Root {
    pub(crate) fn open_new_workspace(
        &mut self,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::Create {
            typed: String::new(),
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    pub(crate) fn open_rename_workspace(
        &mut self,
        id: WorkspaceId,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .desk
            .workspace(id)
            .map(|workspace| workspace.name.clone())
        else {
            return;
        };
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::Rename {
            id,
            typed: name,
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// *New Group…*: ask for the name of a group to make with the
    /// repository kept by `key` in it.
    pub(crate) fn open_new_project_group(
        &mut self,
        workspace: WorkspaceId,
        key: std::path::PathBuf,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::NewProjectGroup {
            workspace,
            key,
            typed: String::new(),
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// Ask whether to merge the project dropped, kept by `dragged`, with the
    /// one it was dropped on into a new group — naming both — and for the
    /// group's name.
    pub(crate) fn open_merge_projects(
        &mut self,
        workspace: WorkspaceId,
        target: std::path::PathBuf,
        dragged: std::path::PathBuf,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let shown = self.column_groups_for(workspace);
        let name = |key: &std::path::Path| {
            shown
                .iter()
                .find(|group| group.key == key)
                .map(|group| group.name.clone())
                .unwrap_or_else(|| key.display().to_string())
        };
        let names = (name(&dragged), name(&target));
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::MergeProjects {
            workspace,
            target,
            dragged,
            names,
            typed: String::new(),
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// *Rename Group…*, with the group's name to edit.
    pub(crate) fn open_rename_project_group(
        &mut self,
        workspace: WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group(id))
            .map(|group| group.name.clone())
        else {
            return;
        };
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::RenameProjectGroup {
            workspace,
            id,
            typed: name,
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The bind question — decisions 1 and 17: the suggested folder in the
    /// field, or an empty field and why. With `then_session` the folder,
    /// once bound, gets the group's first session.
    pub(crate) fn open_bind_group_folder(
        &mut self,
        workspace: WorkspaceId,
        id: guild_session::project_group::ProjectGroupId,
        then_session: bool,
        at: gpui::Point<gpui::Pixels>,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let Some(group) = self
            .desk
            .workspace(workspace)
            .and_then(|holder| holder.project_group(id))
        else {
            return;
        };
        let name = group.name.clone();
        // A bound group's question starts from its folder; an unbound
        // one's from the suggestion.
        let start = group
            .folder
            .clone()
            .or_else(|| self.group_folder_suggestion(workspace, id));
        let suggested = start.is_some();
        self.put_fields_away();
        self.workspace_field = Some(WorkspaceField::BindGroupFolder {
            workspace,
            id,
            name,
            suggested,
            then_session,
            typed: start
                .map(|path| crate::root::column::shown_path(&path))
                .unwrap_or_default(),
            at,
        });
        window.focus(&self.focus, cx);
        cx.notify();
    }

    /// The field's folder button: the system's folder panel, starting where
    /// the field points, its answer put in the field.
    fn choose_group_folder(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |root, cx| {
            let Ok(Ok(Some(chosen))) = paths.await else {
                return;
            };
            let Some(path) = chosen.into_iter().next() else {
                return;
            };
            let _ = root.update(cx, |root, cx| {
                if let Some(WorkspaceField::BindGroupFolder { typed, .. }) =
                    root.workspace_field.as_mut()
                {
                    *typed = crate::root::column::shown_path(&path);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn close_workspace_field(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.workspace_field.take().is_some() {
            self.refocus(window, cx);
        }
    }

    pub(crate) fn workspace_field_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.workspace_field.is_none() {
            return false;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.close_workspace_field(window, cx),
            "enter" => self.commit_workspace_field(window, cx),
            _ => return false,
        }
        true
    }

    /// The field's answer — Enter's, and the merge question's button.
    /// A blank name is not taken: the field stays.
    fn commit_workspace_field(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        let Some(field) = self.workspace_field.as_ref() else {
            return;
        };
        let name = field.typed().trim().to_owned();
        if name.is_empty() {
            return;
        }
        match field {
            WorkspaceField::Create { .. } => match self.desk.add_workspace(&name, None) {
                Ok(id) => {
                    self.close_workspace_field(window, cx);
                    self.select_workspace(id, cx);
                }
                Err(refusal) => {
                    self.inform(self.workspace_refusal(refusal));
                    cx.notify();
                }
            },
            WorkspaceField::Rename { id, .. } => {
                let id = *id;
                if self.desk.rename_workspace(id, &name) {
                    self.close_workspace_field(window, cx);
                    self.save();
                    cx.notify();
                }
            }
            WorkspaceField::NewProjectGroup { workspace, key, .. } => {
                let (workspace, key) = (*workspace, key.clone());
                // The repository may have left the workspace while
                // the name was typed; the desk refuses it then.
                self.close_workspace_field(window, cx);
                if self
                    .desk
                    .create_project_group(workspace, &name, &key)
                    .is_some()
                {
                    self.save();
                }
                cx.notify();
            }
            WorkspaceField::MergeProjects {
                workspace,
                target,
                dragged,
                ..
            } => {
                let (workspace, target, dragged) = (*workspace, target.clone(), dragged.clone());
                self.close_workspace_field(window, cx);
                self.merge_projects(workspace, &target, &dragged, &name, cx);
                cx.notify();
            }
            WorkspaceField::RenameProjectGroup { workspace, id, .. } => {
                let (workspace, id) = (*workspace, *id);
                self.close_workspace_field(window, cx);
                if self.desk.rename_project_group(workspace, id, &name) {
                    self.save();
                }
                cx.notify();
            }
            // A refused folder keeps the question open, with the notice
            // saying why: the person corrects the path rather than starts
            // over.
            WorkspaceField::BindGroupFolder {
                workspace,
                id,
                then_session,
                at,
                ..
            } => {
                let (workspace, id, then_session, at) = (*workspace, *id, *then_session, *at);
                let typed = field.typed().to_owned();
                if self
                    .bind_group_folder(workspace, id, &typed, window, cx)
                    .is_some()
                {
                    self.close_workspace_field(window, cx);
                    if then_session {
                        self.new_group_session(workspace, id, at, window, cx);
                    }
                }
                cx.notify();
            }
        }
    }

    pub(crate) fn render_workspace_field(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let field = self.workspace_field.as_ref()?;
        let theme = self.theme;
        let typed = field.typed().to_owned();
        let at = field.at();
        let title = self
            .ui()
            .text(match field {
                WorkspaceField::Create { .. } => Text::WorkspaceNew,
                WorkspaceField::Rename { .. } => Text::WorkspaceRename,
                WorkspaceField::NewProjectGroup { .. } => Text::ProjectGroupNew,
                WorkspaceField::MergeProjects { .. } => Text::ProjectGroupMerge,
                WorkspaceField::RenameProjectGroup { .. } => Text::ProjectGroupRename,
                WorkspaceField::BindGroupFolder { .. } => Text::GroupFolderLine,
            })
            .to_owned();
        let title = match field {
            WorkspaceField::BindGroupFolder { name, .. } => self.ui().group_folder_title(name),
            _ => title,
        };
        // The bind question says what the folder is for, and why its field
        // is empty when nothing was suggested — decision 17.
        let binding = match field {
            WorkspaceField::BindGroupFolder { suggested, .. } => Some(*suggested),
            _ => None,
        };
        // The merge question names both projects, and answers by buttons
        // as well as by Enter and Escape.
        let merging = match field {
            WorkspaceField::MergeProjects {
                names: (dragged, target),
                ..
            } => Some(format!(
                "{dragged} {} {target}",
                self.ui().text(Text::ProjectGroupMergeAnd)
            )),
            _ => None,
        };
        let placeholder = self
            .ui()
            .text(match field {
                WorkspaceField::Create { .. } | WorkspaceField::Rename { .. } => {
                    Text::WorkspaceNamePlaceholder
                }
                WorkspaceField::NewProjectGroup { .. }
                | WorkspaceField::MergeProjects { .. }
                | WorkspaceField::RenameProjectGroup { .. } => Text::ProjectGroupNamePlaceholder,
                WorkspaceField::BindGroupFolder { .. } => Text::GroupFolderPlaceholder,
            })
            .to_owned();
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
                            root.close_workspace_field(window, cx);
                        }),
                    ),
            ),
        );
        // The field is the popup's whole question and has the keys from the
        // start, so its hint stays while it is empty.
        let input = crate::text_field::TextField::new(
            "workspace-name-input",
            typed,
            cx.listener(|root, value: &String, _, cx| {
                if let Some(field) = root.workspace_field.as_mut() {
                    *field.typed_mut() = value.clone();
                }
                cx.notify();
            }),
        )
        .placeholder(placeholder)
        .keep_hint()
        .focus(&self.focus)
        .autofocus()
        .selected(true)
        .on_key(cx.listener(|root, event, window, cx| {
            root.workspace_field_key(event, window, cx);
        }));
        // Under the merge question's text the field would read as a blank
        // line; it is drawn in Settings' outlined box there. Alone in its
        // popup it needs none.
        let field = if binding.is_some() {
            div()
                .px(theme::space::MENU_INSET)
                .py(theme::space::CARD_GAP)
                .child(
                    crate::text_field::frame(theme)
                        .debug_selector(|| "group-folder-field".into())
                        .flex()
                        .items_center()
                        .gap(theme::space::CARD_GAP)
                        .child(div().flex_1().min_w_0().child(input))
                        .child(
                            div()
                                .id("group-folder-choose")
                                .debug_selector(|| "group-folder-choose".into())
                                .role(gpui::Role::Button)
                                .aria_label(self.ui().text(Text::GroupFolderChoose).to_owned())
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .size(theme::column::ACTION_TARGET)
                                .rounded(theme::radius::ROW)
                                .cursor_pointer()
                                .hover(|button| {
                                    button
                                        .bg(crate::theme::control_hover(theme, theme.surface.card))
                                })
                                .on_click(cx.listener(|root, _, _, cx| {
                                    cx.stop_propagation();
                                    root.choose_group_folder(cx);
                                }))
                                .child(
                                    crate::icons::toolbar("ui/workspace-folder.svg")
                                        .text_color(rgb(theme.text.muted)),
                                ),
                        ),
                )
        } else if merging.is_some() {
            div()
                .px(theme::space::MENU_INSET)
                .py(theme::space::CARD_GAP)
                .child(
                    crate::text_field::frame(theme)
                        .debug_selector(|| "merge-projects-field".into())
                        .child(input),
                )
        } else {
            div()
                .flex()
                .items_center()
                .h(theme::space::MENU_ROW)
                .px(theme::space::MENU_INSET)
                .child(input)
        };
        let card = crate::view::popup::card(theme)
            .w(theme::space::NEW_WORKTREE)
            .when(merging.is_some(), |card| {
                card.debug_selector(|| "merge-projects".into())
            })
            .child(
                div()
                    .px(theme::space::MENU_INSET)
                    .pt(px(8.0))
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(title),
            )
            .when(binding.is_some(), |card| {
                card.debug_selector(|| "group-folder".into()).child(
                    div()
                        .px(theme::space::MENU_INSET)
                        .pt(px(4.0))
                        .text_size(theme::size::BODY)
                        .text_color(rgb(theme.text.primary))
                        .child(self.ui().text(Text::GroupFolderLine).to_owned()),
                )
            })
            .when_some(merging.clone(), |card, both| {
                card.child(
                    div()
                        .px(theme::space::MENU_INSET)
                        .pt(px(4.0))
                        .text_size(theme::size::BODY)
                        .text_color(rgb(theme.text.primary))
                        .child(both),
                )
            })
            .child(field)
            .when(binding == Some(false), |card| {
                card.child(
                    div()
                        .px(theme::space::MENU_INSET)
                        .pb(theme::space::CARD_GAP)
                        .debug_selector(|| "group-folder-none".into())
                        .text_size(theme::size::META)
                        .text_color(rgb(theme.text.faint))
                        .child(self.ui().text(Text::GroupFolderNoSuggestion).to_owned()),
                )
            })
            .when(merging.is_some(), |card| {
                card.child(self.merge_projects_buttons(Text::ProjectGroupMergeConfirm, true, cx))
            })
            .when(binding.is_some(), |card| {
                card.child(
                    self.merge_projects_buttons(
                        Text::GroupFolderBind,
                        !self
                            .workspace_field
                            .as_ref()
                            .is_some_and(|field| field.typed().trim().is_empty()),
                        cx,
                    ),
                )
            });
        let popup = deferred(
            anchored()
                .position(at)
                .snap_to_window_with_margin(px(8.0))
                .child(card),
        )
        .priority(2);
        Some(div().child(backdrop).child(popup).into_any_element())
    }

    /// The merge question's two answers, as the worktree sheet draws its
    /// own: Cancel outlined, Merge filled — in patina, since nothing is lost.
    /// The bind question's are the same pair, *Bind* off while the field is
    /// empty.
    fn merge_projects_buttons(
        &self,
        confirm: Text,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let palette = self.theme;
        let ui = self.ui();
        let button = |id: &'static str| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .role(gpui::Role::Button)
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .h(theme::worktree_confirmation::BUTTON)
                .px(theme::worktree_confirmation::GAP)
                .rounded_full()
                .border_1()
                .cursor_pointer()
        };
        let cancel = button("merge-projects-cancel")
            .aria_label(ui.text(Text::Cancel))
            .border_color(theme::card_outline(palette))
            .text_color(rgb(palette.text.primary))
            .hover(|b| b.bg(crate::theme::control_hover(palette, palette.surface.card)))
            .child(ui.text(Text::Cancel))
            .on_click(cx.listener(|root, _, window, cx| root.close_workspace_field(window, cx)));
        let merge = button("merge-projects-confirm")
            .aria_label(ui.text(confirm))
            .border_color(rgb(palette.accent.patina))
            .bg(rgb(palette.accent.patina))
            .text_color(rgb(palette.surface.app))
            .child(ui.text(confirm))
            .when(enabled, |b| {
                b.hover(|b| b.opacity(theme::worktree_confirmation::HOVER))
                    .on_click(
                        cx.listener(|root, _, window, cx| root.commit_workspace_field(window, cx)),
                    )
            })
            .when(!enabled, |b| {
                b.opacity(crate::view::kind::FAINT).cursor_default()
            });
        div()
            .flex()
            .justify_end()
            .gap(theme::space::CARD_GAP)
            .px(theme::space::MENU_INSET)
            .pb(px(8.0))
            .text_size(theme::size::BODY)
            .child(cancel)
            .child(merge)
    }

    /// Put the icon picker away without choosing anything — Escape, and
    /// the backdrop. `close_workspace_field`'s shape: reset what only
    /// this popup owns and give the keyboard back to what was focused
    /// before it opened.
    fn close_workspace_icon_picker(&mut self, window: &mut gpui::Window, cx: &mut Context<Self>) {
        if self.workspace_icon_picker.take().is_some() {
            self.workspace_icon_lit = None;
            self.refocus(window, cx);
        }
    }

    /// Take the icon at the picker's grid index — a click's, and
    /// `enter`'s: 0 is *No icon*, `n` is `WORKSPACE_NAMES[n - 1]`, the
    /// addressing `grid_step` answers in.
    fn take_workspace_icon(
        &mut self,
        id: IconTarget,
        index: usize,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) {
        let name = index
            .checked_sub(1)
            .and_then(|cell| crate::icons::WORKSPACE_NAMES.get(cell));
        match id {
            IconTarget::Workspace(id) => {
                let _ = self.desk.set_icon(id, name.copied());
                self.save();
            }
            IconTarget::Repository(key) => self.commit_scoped_preference(
                crate::root::repository_settings::Edit::Icon(key, name.map(|n| (*n).to_owned())),
                cx,
            ),
        }
        self.workspace_icon_picker = None;
        self.workspace_icon_lit = None;
        self.refocus(window, cx);
        cx.notify();
    }

    /// The icon picker's keys, read where every popup's are — the root's
    /// `capture_key_down`, ahead of whatever is focused behind it. Every
    /// key is swallowed while the picker is up, the menu engine's own
    /// rule: a key that reaches the terminal or the editor behind a
    /// popup is the trap this avoids.
    pub(crate) fn workspace_icon_picker_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut gpui::Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(id) = self.workspace_icon_picker.clone() else {
            return false;
        };
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => self.close_workspace_icon_picker(window, cx),
            "enter" => {
                if let Some(lit) = self.workspace_icon_lit {
                    self.take_workspace_icon(id, lit, window, cx);
                }
            }
            "left" | "right" | "up" | "down" => {
                let current = self
                    .picker_icon(&id)
                    .and_then(|icon| {
                        crate::icons::WORKSPACE_NAMES
                            .iter()
                            .position(|name| *name == icon)
                    })
                    .map(|index| index + 1);
                self.workspace_icon_lit = grid_step(
                    crate::icons::WORKSPACE_NAMES.len() + 1,
                    ICON_COLUMNS,
                    self.workspace_icon_lit,
                    key,
                    current,
                );
                cx.notify();
            }
            _ => {}
        }
        true
    }

    pub(crate) fn render_workspace_icon_picker(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let id = self.workspace_icon_picker.clone()?;
        let theme = self.theme;
        let current = self.picker_icon(&id);
        let lit = self.workspace_icon_lit;
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
                            root.close_workspace_icon_picker(window, cx);
                        }),
                    ),
            ),
        );
        // **One highlight, moved by the pointer on move and by the
        // keyboard** — M12.7 task 8. The old code lit the selected cell
        // *and* whatever the pointer hovered, which could be two cells at
        // once; the current icon is the check badge's claim now; the
        // background is `lit`'s alone.
        let icons =
            crate::icons::WORKSPACE_NAMES
                .iter()
                .enumerate()
                .map(|(at, name)| {
                    let id = id.clone();
                    let name = (*name).to_owned();
                    let cell = at + 1;
                    let selected = current.as_deref() == Some(name.as_str());
                    let lit_here = lit == Some(cell);
                    let path = crate::icons::workspace_path(&name)
                        .expect("every offered icon is vendored");
                    div()
                        .id(SharedString::from(format!("workspace-icon-{name}")))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_center()
                        .size(px(30.0))
                        .rounded(theme::radius::ROW)
                        .cursor_pointer()
                        .when(lit_here, |button| button.bg(rgb(theme.surface.raised)))
                        .on_mouse_move(cx.listener(move |root, _event, _window, cx| {
                            if root.workspace_icon_lit != Some(cell) {
                                root.workspace_icon_lit = Some(cell);
                                cx.notify();
                            }
                        }))
                        .on_click(cx.listener(move |root, _event, window, cx| {
                            root.take_workspace_icon(id.clone(), cell, window, cx);
                        }))
                        .child(crate::icons::toolbar(path).text_color(rgb(theme.text.secondary)))
                        .when(selected, |button| {
                            button.child(
                                div().absolute().top(px(1.0)).right(px(1.0)).child(
                                    crate::icons::check().text_color(rgb(theme.text.secondary)),
                                ),
                            )
                        })
                })
                .collect::<Vec<_>>();
        let none_selected = current.is_none();
        let none = crate::view::popup::row(
            "workspace-icon-none",
            crate::view::popup::Row {
                label: self
                    .ui()
                    .text(if matches!(id, IconTarget::Repository(_)) {
                        Text::ScopedAutomatic
                    } else {
                        Text::WorkspaceNoIcon
                    })
                    .to_owned()
                    .into(),
                mark: None,
                icons: false,
                check: Some(none_selected),
                stroke: None,
                loses: false,
                submenu: false,
                lit: lit == Some(0),
                disabled: false,
                trailing: None,
            },
            theme,
        )
        .on_mouse_move(cx.listener(move |root, _event, _window, cx| {
            if root.workspace_icon_lit != Some(0) {
                root.workspace_icon_lit = Some(0);
                cx.notify();
            }
        }))
        .on_click(cx.listener(move |root, _event, window, cx| {
            root.take_workspace_icon(id.clone(), 0, window, cx);
        }));
        let card = crate::view::popup::card(theme)
            .debug_selector(|| "workspace-icon-popup".into())
            .w(px(260.0))
            .child(
                div()
                    .px(theme::space::MENU_INSET)
                    .pt(px(8.0))
                    .text_size(theme::size::META)
                    .text_color(rgb(theme.text.faint))
                    .child(self.ui().text(Text::WorkspaceChooseIcon).to_owned()),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(4.0))
                    .p(px(8.0))
                    .children(icons),
            )
            .child(none);
        let popup = deferred(
            anchored()
                .position(self.workspace_icon_at.get())
                .snap_to_window_with_margin(px(8.0))
                .child(card),
        )
        .priority(2);
        Some(div().child(backdrop).child(popup).into_any_element())
    }
}
