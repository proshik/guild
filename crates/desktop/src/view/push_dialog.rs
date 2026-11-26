//! Push Commits in a window of its own — M8.3.6.
//!
//! The branch and commits are beside their combined files, toolbar and
//! optional details. Surfaces and controls use the selected Guild palette. The foot
//! holds Help, tags, Git hooks, Cancel and Push. Both splits drag.
//!
//! **A view over `Root`'s push state, not a copy** (decision 2): what the
//! window shows is `root.push.of(project)`, and every gesture goes through
//! the calls the window already has — so the status bar's count and this
//! window cannot disagree.

use crate::view::tooltip::Hint as _;
use std::collections::HashSet;
use std::path::PathBuf;

use gpui::prelude::*;
use gpui::{
    Context, Entity, FocusHandle, MouseButton, MouseMoveEvent, Pixels, Point, Subscription, Task,
    WeakEntity, Window, div, px, rgb,
};
use guild_session::ProjectId;

use crate::files_tree::{self, Row};
use crate::git::log::FileChange;
use crate::git::{Destination, GitError, PushTags};
use crate::i18n::{Text, Ui};
use crate::split::Split;
use crate::windows::Satellite;
use crate::{Root, icons, theme};

mod speed_search;
#[cfg(test)]
mod tests;
use speed_search::{PushList, PushSearch};
#[cfg(test)]
mod speed_search_tests;

/// The foot's height, design system §6.31.
const FOOT: f32 = 48.0;
/// The right side's toolbar.
const TOOLBAR: f32 = 30.0;
/// Below this logical width, actions get their own row (§6.31).
const FOOT_ONE_ROW: f32 = 900.0;
/// The narrowest either side of a split may be.
const MIN_SIDE: f32 = 160.0;
/// The narrowest the tree or the message may be.
const MIN_PANE: f32 = 80.0;

/// Which split a drag is moving.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Grip {
    /// The commits beside the files.
    Sides,
    /// The files over the message.
    Panes,
}

#[derive(Clone, Copy)]
enum Popup {
    Tags,
    Remote,
    Target,
    Push,
    Help,
}

/// Push Commits.
pub(crate) struct PushDialog {
    root: WeakEntity<Root>,
    project: ProjectId,
    theme: theme::Theme,
    colours: theme::push::Colours,
    focus: FocusHandle,
    focused: bool,
    folded: HashSet<String>,
    repository_folded: bool,
    /// The file row last clicked — what the eye opens.
    selected_file: Option<PathBuf>,
    file_selection: crate::file_selection::FileSelection,
    file_focus: FocusHandle,
    file_scroll: gpui::ScrollHandle,
    commit_focus: FocusHandle,
    commit_scroll: gpui::ScrollHandle,
    speed: Option<PushSearch>,
    /// *Push tags* ticked.
    tags_on: bool,
    /// Which tags, when ticked.
    tags: PushTags,
    run_hooks: bool,
    details: bool,
    preview_on: bool,
    preview: Option<Entity<crate::diff_tab::DiffTab>>,
    preview_key: Option<(String, PathBuf)>,
    preview_heard: Option<Subscription>,
    _poll: Task<()>,
    popup: Option<(Popup, Point<Pixels>)>,
    popup_index: usize,
    target_text: String,
    target_selected: bool,
    sides: Split,
    panes: Split,
    drag: Option<(Grip, Point<Pixels>, Split)>,
}

/// What one render needs of `Root`, read once and let go of.
struct Seen {
    ui: Ui,
    look: crate::file_tab::Look,
    name: String,
    ahead: Option<Result<(), String>>,
    remotes: Vec<String>,
    to: Option<Destination>,
    commits: Vec<(String, String)>,
    opened: Option<String>,
    selected: Vec<String>,
    detail: Option<String>,
    files: Option<Result<Vec<FileChange>, String>>,
    running: bool,
    refused: Option<String>,
}

impl PushDialog {
    fn foot_rows(window: &Window) -> f32 {
        let scale = f32::from(window.rem_size()) / crate::scale::BASE;
        if f32::from(window.viewport_size().width) < FOOT_ONE_ROW * scale {
            2.0
        } else {
            1.0
        }
    }

    /// The palette the window is drawn with.
    #[cfg(test)]
    pub(crate) fn theme(&self) -> theme::Theme {
        self.theme
    }

    /// A file's diff in the preview, as a selected row would show it.
    #[cfg(test)]
    pub(crate) fn show_preview_for_test(&mut self, tab: Entity<crate::diff_tab::DiffTab>) {
        self.preview = Some(tab);
    }

    /// Draw with the main window's palette in effect, the preview included.
    pub(crate) fn set_theme(
        &mut self,
        theme: theme::Theme,
        look: crate::file_tab::Look,
        ui: crate::i18n::Ui,
        cx: &mut Context<Self>,
    ) {
        self.colours = theme::push::colours(theme);
        self.theme = theme;
        if let Some(preview) = &self.preview {
            let mut look = look;
            look.theme = theme;
            preview.update(cx, |tab, cx| tab.restyle(look, ui, cx));
        }
        cx.notify();
    }

    pub(crate) fn new(
        root: WeakEntity<Root>,
        project: ProjectId,
        theme: theme::Theme,
        cx: &mut Context<Self>,
    ) -> Self {
        let splits = root
            .upgrade()
            .and_then(|root| root.read(cx).settings.push_splits)
            .unwrap_or((theme::push::LEFT_SHARE, 0.68));
        let colours = theme::push::colours(theme);
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(16))
                    .await;
                if this
                    .update(cx, |this, cx| {
                        if this.preview_on
                            && let Some(tab) = &this.preview
                        {
                            tab.update(cx, |tab, cx| tab.poll(cx));
                        }
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            root,
            project,
            theme: colours.theme,
            colours,
            focus: cx.focus_handle(),
            focused: false,
            folded: HashSet::new(),
            repository_folded: false,
            selected_file: None,
            file_selection: Default::default(),
            file_focus: cx.focus_handle(),
            file_scroll: gpui::ScrollHandle::new(),
            commit_focus: cx.focus_handle(),
            commit_scroll: gpui::ScrollHandle::new(),
            speed: None,
            tags_on: false,
            tags: PushTags::All,
            run_hooks: true,
            details: true,
            preview_on: false,
            preview: None,
            preview_key: None,
            preview_heard: None,
            _poll: poll,
            popup: None,
            popup_index: 0,
            target_text: String::new(),
            target_selected: false,
            sides: Split::new(splits.0),
            panes: Split::new(splits.1),
            drag: None,
        }
    }

    fn seen(&self, cx: &mut Context<Self>) -> Option<Seen> {
        let root = self.root.upgrade()?;
        let root = root.read(cx);
        let ui = root.ui();
        let project = root.desk.project(self.project)?;
        let name = project
            .repository
            .as_deref()
            .and_then(|key| root.desk.repository_name(key))
            .map(str::to_owned)
            .unwrap_or_else(|| project.name.clone());
        let state = root.push.of(self.project);
        let ahead = state.and_then(|s| s.ahead.as_ref()).map(|a| match a {
            Ok(_) => Ok(()),
            Err(GitError::NoGit) => Err(ui.text(Text::DiffNoGit).to_owned()),
            Err(GitError::NoRepository) => Err(ui.text(Text::DiffNoRepository).to_owned()),
            Err(GitError::Failed(words)) => Err(words.clone()),
        });
        let remotes = state
            .and_then(|s| s.ahead.as_ref())
            .and_then(|a| a.as_ref().ok())
            .map(|a| a.remotes.clone())
            .unwrap_or_default();
        let commits = state.map_or_else(Vec::new, |s| {
            s.commits()
                .iter()
                .map(|c| (c.sha.clone(), c.summary.clone()))
                .collect()
        });
        let opened = state.and_then(|s| s.opened.clone());
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let selected = state.map_or_else(Vec::new, |s| s.selected.shas.clone());
        let detail = state.filter(|_| selected.len() == 1).and_then(|s| {
            let sha = &selected[0];
            let commit = s.commits().iter().find(|c| &c.sha == sha)?;
            let mut text = commit.summary.clone();
            if !commit.body.trim().is_empty() {
                text.push_str("\n\n");
                text.push_str(commit.body.trim_end());
            }
            text.push_str(&format!(
                "\n\n{} <{}>, {}\n{}",
                commit.author,
                commit.email,
                ui.commit_date(commit.time, now, crate::system::utc_offset()),
                commit.sha
            ));
            Some(text)
        });
        let files = state.zip(opened.as_ref()).and_then(|(s, sha)| {
            s.files.get(sha).map(|f| match f {
                Ok(files) => Ok(files.clone()),
                Err(err) => Err(err.to_string()),
            })
        });
        Some(Seen {
            ui,
            look: root.file_look(),
            name,
            ahead,
            remotes,
            to: state.and_then(|s| s.destination()),
            commits,
            opened,
            selected,
            detail,
            files,
            running: state.is_some_and(|s| s.running),
            refused: state.and_then(|s| s.refused.clone()),
        })
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        let tags = if self.tags_on {
            self.tags
        } else {
            PushTags::None
        };
        let project = self.project;
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, cx| {
                root.push_from_dialog(project, tags, self.run_hooks, cx)
            });
        }
    }

    fn open_file(
        &mut self,
        sha: &str,
        files: &[FileChange],
        path: &PathBuf,
        cx: &mut Context<Self>,
    ) {
        let ordered = inspection_paths(files);
        let index = ordered.iter().position(|p| p == path).unwrap_or(0);
        let project = self.project;
        let sha = sha.to_owned();
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, cx| {
                root.open_diff_window(project, sha, ordered, index, cx);
            });
        }
    }

    fn end_drag(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_none() {
            return;
        }
        let splits = (self.sides.fraction(), self.panes.fraction());
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, _| root.remember_push_splits(splits));
        }
        cx.notify();
    }

    fn resize_events(&self, cx: &Context<Self>) -> gpui::AnyElement {
        crate::view::resize::capture(
            cx,
            |this, event, window, cx| {
                if this.drag.is_none() {
                    return false;
                }
                this.dragged(event, window, cx);
                true
            },
            |this, event, window, cx| {
                if event.button != MouseButton::Left || this.drag.is_none() {
                    return false;
                }
                this.dragged(
                    &MouseMoveEvent {
                        position: event.position,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: event.modifiers,
                    },
                    window,
                    cx,
                );
                this.end_drag(cx);
                true
            },
        )
    }

    fn dragged(&mut self, event: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) {
        let Some((grip, from, held)) = self.drag else {
            return;
        };
        // A release outside the window can go missing: no button held
        // means the drag is over.
        if event.pressed_button != Some(MouseButton::Left) {
            self.end_drag(cx);
            return;
        }
        let size = window.viewport_size();
        match grip {
            Grip::Sides => {
                let delta = f32::from(event.position.x - from.x);
                self.sides = held;
                self.sides.drag(delta, f32::from(size.width), MIN_SIDE);
            }
            Grip::Panes => {
                let delta = f32::from(event.position.y - from.y);
                let scale = f32::from(window.rem_size()) / crate::scale::BASE;
                let total =
                    f32::from(size.height) - (FOOT * Self::foot_rows(window) + TOOLBAR) * scale;
                self.panes = held;
                self.panes.drag(delta, total, MIN_PANE);
            }
        }
        cx.notify();
    }

    fn grip(&self, grip: Grip, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let vertical = grip == Grip::Sides;
        let line = div()
            .when(vertical, |l| l.w(px(1.0)).h_full())
            .when(!vertical, |l| l.h(px(1.0)).w_full())
            // Nothing lights under the hand or while dragging — the cursor
            // says it, 2026-09-18.
            .bg(rgb(theme.surface.divider));
        div()
            .id(if vertical {
                "push-grip-sides"
            } else {
                "push-grip-panes"
            })
            .debug_selector(move || {
                if vertical {
                    "push-grip-sides"
                } else {
                    "push-grip-panes"
                }
                .into()
            })
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .when(vertical, |g| g.w(px(9.0)).h_full())
            .when(!vertical, |g| g.h(px(9.0)).w_full())
            .cursor(if vertical {
                gpui::CursorStyle::ResizeLeftRight
            } else {
                gpui::CursorStyle::ResizeUpDown
            })
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &gpui::MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    let held = match grip {
                        Grip::Sides => this.sides,
                        Grip::Panes => this.panes,
                    };
                    this.drag = Some((grip, event.position, held));
                    cx.notify();
                }),
            )
            .into_any_element()
    }

    fn choose_commit(
        &mut self,
        sha: Option<&str>,
        toggle: bool,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        if self
            .speed
            .as_ref()
            .is_some_and(|s| s.list == PushList::Files)
        {
            self.speed = None;
        }
        self.selected_file = None;
        self.file_selection = Default::default();
        self.preview_key = None;
        let project = self.project;
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, cx| {
                root.push_select(project, sha, toggle, extend, cx)
            });
        }
    }

    fn open_popup(&mut self, popup: Popup, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.popup = Some((popup, at));
        self.popup_index = match popup {
            Popup::Tags if self.tags == PushTags::CurrentBranch => 1,
            Popup::Remote => self.seen(cx).map_or(0, |s| {
                s.to.as_ref()
                    .and_then(|to| s.remotes.iter().position(|remote| *remote == to.remote))
                    .unwrap_or(0)
            }),
            _ => 0,
        };
        if matches!(popup, Popup::Target) {
            self.target_text = self
                .seen(cx)
                .and_then(|s| s.to)
                .map_or_else(String::new, |to| to.onto);
            self.target_selected = true;
        }
        cx.notify();
    }

    fn left(&self, seen: &Seen, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = seen.ui;
        let query = self.speed_query(PushList::Commits).to_owned();
        let branch = seen.selected.is_empty();
        let head = div()
            .id("push-branch")
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::from_px(6.0))
            .h(theme::from_px(28.0))
            .px(theme::from_px(16.0))
            .text_size(theme::size::BODY)
            .when(branch, |h| h.bg(rgb(theme.accent.patina_bg)))
            .cursor_pointer()
            .on_click(
                cx.listener(|this, _event, _window, cx| this.choose_commit(None, false, false, cx)),
            );
        let head = match &seen.to {
            None => head.child(ui.text(if seen.remotes.is_empty() {
                Text::PushNoRemote
            } else {
                Text::PushDetached
            })),
            Some(to) => head
                .child(format!("{} →", to.branch))
                .child(
                    div()
                        .id("push-remote")
                        .flex_none()
                        .text_color(rgb(self.colours.link))
                        .cursor_pointer()
                        .child(to.remote.clone())
                        .on_click(cx.listener(|this, event: &gpui::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.open_popup(Popup::Remote, event.position(), cx);
                        })),
                )
                .child(":")
                .child(
                    div()
                        .id("push-target")
                        .flex_none()
                        .text_color(rgb(self.colours.link))
                        .cursor_pointer()
                        .child(to.onto.clone())
                        .on_click(cx.listener(|this, event: &gpui::ClickEvent, _window, cx| {
                            cx.stop_propagation();
                            this.open_popup(Popup::Target, event.position(), cx);
                        })),
                )
                .when(to.set_upstream, |head| {
                    head.child(
                        div()
                            .px(theme::from_px(4.0))
                            .rounded(theme::radius::CHECKBOX)
                            .text_size(theme::size::META)
                            .text_color(rgb(theme.accent.green))
                            .child(ui.text(Text::PushNew)),
                    )
                }),
        };
        let body = match &seen.ahead {
            None => div()
                .p(theme::from_px(16.0))
                .text_color(rgb(theme.text.faint))
                .child(ui.text(Text::LogLoading))
                .into_any_element(),
            Some(Err(words)) => div()
                .p(theme::from_px(16.0))
                .text_color(rgb(theme.accent.rust))
                .child(words.clone())
                .into_any_element(),
            Some(Ok(())) => div()
                .id("push-commit-scroll")
                .size_full()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.commit_scroll)
                .flex()
                .flex_col()
                .child(head)
                .children(seen.commits.iter().enumerate().map(|(i, (sha, summary))| {
                    let selected = seen.selected.contains(sha);
                    let sha = sha.clone();
                    div()
                        .id(("push-commit", i))
                        .debug_selector(move || format!("push-commit-{i}"))
                        .flex()
                        .flex_none()
                        .items_center()
                        .h(theme::from_px(26.0))
                        .pl(theme::from_px(34.0))
                        .pr(theme::from_px(10.0))
                        .cursor_pointer()
                        .when(selected, |row| row.bg(rgb(theme.accent.patina_bg)))
                        .when(!selected, |row| {
                            row.hover(|row| row.bg(rgb(theme.surface.card)))
                        })
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(theme::size::BODY)
                                .child(crate::view::speed_search::label(
                                    summary.clone(),
                                    &query,
                                    theme,
                                )),
                        )
                        .on_click(
                            cx.listener(move |this, event: &gpui::ClickEvent, window, cx| {
                                window.focus(&this.commit_focus, cx);
                                let held = event.modifiers();
                                this.choose_commit(
                                    Some(&sha),
                                    held.platform || held.control,
                                    held.shift,
                                    cx,
                                );
                            }),
                        )
                }))
                .into_any_element(),
        };
        let body = div()
            .id("push-left")
            .track_focus(&self.commit_focus)
            .size_full()
            .min_h_0()
            .overflow_hidden()
            .py(theme::from_px(6.0))
            .child(body);
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.speed_field(PushList::Commits, seen, cx))
            .child(body)
            .into_any_element()
    }

    fn tool(
        &self,
        id: &'static str,
        icon: &'static str,
        label: String,
        enabled: bool,
        lit: bool,
    ) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .size(theme::space::HEAD_BUTTON)
            .rounded(theme::radius::ROW)
            .when(lit, |b| b.bg(rgb(theme.surface.raised)))
            .when(enabled, |b| {
                b.cursor_pointer()
                    .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.card)))
            })
            .child(icons::toolbar(icon).text_color(rgb(if enabled {
                theme.text.tertiary
            } else {
                theme.text.gutter
            })))
            .hint_delay()
            .tooltip(move |_window, app| {
                app.new(|_cx| crate::view::tooltip::Tooltip::text(label.clone(), theme))
                    .into()
            })
    }

    fn toolbar(&self, seen: &Seen, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = seen.ui;
        let file = self.selected_file.is_some() && matches!(seen.files, Some(Ok(_)));
        let keys: Vec<String> = seen
            .files
            .as_ref()
            .and_then(|f| f.as_ref().ok())
            .map(|f| files_tree::of(f))
            .unwrap_or_default()
            .iter()
            .filter_map(|row| match row {
                Row::Directory { key, .. } => Some(key.clone()),
                _ => None,
            })
            .collect();
        let rule = || {
            div()
                .flex_none()
                .w(px(1.0))
                .h(theme::space::STRIP_RULE)
                .mx(theme::from_px(2.0))
                .bg(rgb(theme.surface.divider))
        };
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::from_px(4.0))
            .h(theme::from_px(TOOLBAR))
            .px(theme::from_px(8.0))
            .border_b_1()
            .border_color(rgb(theme.surface.divider))
            .child(
                self.tool(
                    "push-show-diff",
                    "ui/tool-diff.svg",
                    ui.text(Text::ShowDiff).to_owned(),
                    file,
                    false,
                )
                .when(file, |b| {
                    b.on_click(cx.listener(|this, _event, _window, cx| {
                        if let Some(seen) = this.seen(cx)
                            && let (Some(path), Some(sha), Some(Ok(files))) =
                                (this.selected_file.clone(), seen.opened, seen.files)
                        {
                            this.open_file(&sha, &files, &path, cx);
                        }
                    }))
                }),
            )
            .child(rule())
            .child(
                self.tool(
                    "push-preview",
                    "ui/push-preview.svg",
                    ui.text(Text::PushPreview).to_owned(),
                    file || self.preview_on,
                    self.preview_on,
                )
                .when(file || self.preview_on, |b| {
                    b.on_click(cx.listener(|this, _event, _window, cx| {
                        this.preview_on = !this.preview_on;
                        cx.notify();
                    }))
                }),
            )
            .child(
                self.tool(
                    "push-source",
                    "ui/menu-rename.svg",
                    ui.text(Text::DiffEditSource).to_owned(),
                    file,
                    false,
                )
                .when(file, |b| {
                    b.on_click(cx.listener(|this, _event, _window, cx| {
                        let project = this.project;
                        if let Some(path) = this.selected_file.clone()
                            && let Some(root) = this.root.upgrade()
                        {
                            root.update(cx, |root, cx| root.open_in_main(project, path, 1, cx));
                        }
                    }))
                }),
            )
            .child(rule())
            .child(
                self.tool(
                    "push-details",
                    "ui/push-details.svg",
                    ui.text(Text::PushDetails).to_owned(),
                    true,
                    self.details,
                )
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.details = !this.details;
                    cx.notify();
                })),
            )
            .child(div().flex_1())
            .child(
                self.tool(
                    "push-expand",
                    "ui/push-expand.svg",
                    ui.text(Text::ExpandAll).to_owned(),
                    seen.opened.is_some(),
                    false,
                )
                .on_click(cx.listener(|this, _event, _window, cx| {
                    this.folded.clear();
                    this.repository_folded = false;
                    cx.notify();
                })),
            )
            .child(
                self.tool(
                    "push-collapse",
                    "ui/push-collapse.svg",
                    ui.text(Text::CollapseAll).to_owned(),
                    seen.opened.is_some(),
                    false,
                )
                .on_click(cx.listener(move |this, _event, _window, cx| {
                    this.folded = keys.iter().cloned().collect();
                    this.repository_folded = true;
                    cx.notify();
                })),
            )
            .into_any_element()
    }

    fn sync_preview(&mut self, seen: &Seen, cx: &mut Context<Self>) {
        if !self.preview_on {
            return;
        }
        let Some(key) = seen.opened.clone().zip(self.selected_file.clone()) else {
            return;
        };
        if self.preview_key.as_ref() == Some(&key) {
            return;
        }
        let (folder, mode) = match self.root.upgrade() {
            Some(root) => {
                let root = root.read(cx);
                let Some(project) = root.desk.project(self.project) else {
                    return;
                };
                (project.path.clone(), root.settings.diff_view)
            }
            None => return,
        };
        let mut look = seen.look.clone();
        look.theme = self.theme;
        let (sha, path) = key.clone();
        let ui = seen.ui;
        let ordered = seen
            .files
            .as_ref()
            .and_then(|files| files.as_ref().ok())
            .map(|files| inspection_paths(files))
            .unwrap_or_default();
        let index = ordered.iter().position(|p| *p == path).unwrap_or(0);
        if let Some(tab) = &self.preview {
            tab.update(cx, |tab, cx| {
                tab.show_inspection(&folder, path, sha, cx);
                tab.restyle(look, ui, cx);
                tab.set_inspection_files(ordered, index, cx);
            });
            self.preview_key = Some(key);
            return;
        }
        let tab = cx.new(|cx| {
            let mut tab =
                crate::diff_tab::DiffTab::open_inspection(&folder, path, sha, mode, look, ui, cx);
            tab.set_inspection_files(ordered, index, cx);
            tab
        });
        self.preview_heard = Some(cx.subscribe(
            &tab,
            |this, _tab, event: &crate::diff_tab::DiffEvent, cx| match event {
                crate::diff_tab::DiffEvent::OpenAt { path, line } => {
                    if let Some(root) = this.root.upgrade() {
                        let project = this.project;
                        root.update(cx, |root, cx| {
                            root.open_in_main(project, path.clone(), *line, cx)
                        });
                    }
                }
                crate::diff_tab::DiffEvent::StepFile { forward } => {
                    if let Some(seen) = this.seen(cx)
                        && let Some(Ok(files)) = &seen.files
                    {
                        let paths = inspection_paths(files);
                        if let Some(index) = this
                            .selected_file
                            .as_ref()
                            .and_then(|p| paths.iter().position(|v| v == p))
                            && let Some(next) =
                                crate::diff_window::stepped(index, paths.len(), *forward)
                        {
                            this.select_preview_file(&paths, next, cx);
                        }
                    }
                }
                crate::diff_tab::DiffEvent::SelectFile { index, key } => {
                    if let Some(seen) = this.seen(cx)
                        && seen.opened.as_ref() == Some(key)
                        && let Some(Ok(files)) = &seen.files
                    {
                        this.select_preview_file(&inspection_paths(files), *index, cx);
                    }
                }
                _ => {}
            },
        ));
        self.preview = Some(tab);
        self.preview_key = Some(key);
    }

    fn select_preview_file(&mut self, paths: &[PathBuf], index: usize, cx: &mut Context<Self>) {
        let Some(path) = paths.get(index).cloned() else {
            return;
        };
        self.repository_folded = false;
        self.folded
            .retain(|key| !path.starts_with(std::path::Path::new(key)));
        self.file_selection.click(&path, paths, false, false);
        self.selected_file = Some(path);
        self.preview_key = None;
        cx.notify();
    }

    fn tree(&self, seen: &Seen, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = seen.ui;
        let palette = icons::Palette::from(theme.kind);
        let query = self.speed_query(PushList::Files).to_owned();
        // Blue while the file list has the keys — decision 54.
        // The file list's speed search sits beside the list and has the
        // keys while it is up, so it counts as the list's.
        let focused = crate::view::tree::keys_in(&self.file_focus, window, cx)
            || (window.is_window_active()
                && self
                    .speed
                    .as_ref()
                    .is_some_and(|speed| speed.list == PushList::Files)
                && crate::text_field::focused(window, cx));
        let (sha, files) = match (&seen.opened, &seen.files) {
            (None, _) => {
                return div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(Text::PushEmpty))
                    .into_any_element();
            }
            (Some(_), None) => {
                return div()
                    .p(theme::from_px(16.0))
                    .text_color(rgb(theme.text.faint))
                    .child(ui.text(Text::LogLoading))
                    .into_any_element();
            }
            (Some(_), Some(Err(words))) => {
                return div()
                    .p(theme::from_px(16.0))
                    .text_color(rgb(theme.accent.rust))
                    .child(words.clone())
                    .into_any_element();
            }
            (Some(sha), Some(Ok(files))) => (sha.clone(), files.clone()),
        };
        let rows = files_tree::of(&files);
        let shown: Vec<Row> = files_tree::visible(&rows, &self.folded)
            .into_iter()
            .cloned()
            .collect();
        let root_folded = self.repository_folded;
        let count = files.len();
        let shown = if root_folded { Vec::new() } else { shown };
        let order: Vec<PathBuf> = shown
            .iter()
            .filter_map(|row| match row {
                Row::File { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        let key_lines: Vec<PathBuf> = shown
            .iter()
            .map(|row| match row {
                Row::File { path, .. } => path.clone(),
                Row::Directory { path, .. } => path.into(),
            })
            .collect();
        let select_order = order.clone();
        let keys_order = order.clone();
        let keys_sha = sha.clone();
        let keys_files = files.clone();
        let files = std::rc::Rc::new(files);
        let body = div()
            .id("push-tree")
            .track_focus(&self.file_focus)
            .track_scroll(&self.file_scroll)
            .on_action(
                cx.listener(move |this, _: &crate::actions::SelectAll, _window, cx| {
                    this.file_selection.key("a", &select_order, true, false);
                    this.selected_file = this.file_selection.action_target().cloned();
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .on_action(cx.listener(|this, _: &crate::actions::Copy, _window, cx| {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                    this.file_selection.copied_paths(),
                ));
                cx.stop_propagation();
            }))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if !this.file_focus.is_focused(window) {
                        return;
                    }
                    let key = event.keystroke.key.as_str();
                    let mods = event.keystroke.modifiers;
                    if this
                        .file_selection
                        .key(key, &keys_order, mods.platform, mods.shift)
                    {
                        this.selected_file = this.file_selection.action_target().cloned();
                        if let Some(path) = &this.selected_file
                            && let Some(index) = key_lines.iter().position(|p| p == path)
                        {
                            this.file_scroll.scroll_to_item(index + 1);
                        }
                    } else if key == "c" && mods.platform {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                            this.file_selection.copied_paths(),
                        ));
                    } else if key == "enter" {
                        if let Some(path) = this.selected_file.clone() {
                            this.open_file(&keys_sha, &keys_files, &path, cx);
                        }
                    } else if key == "escape" {
                        window.focus(&this.focus, cx);
                    } else {
                        return;
                    }
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .py(theme::from_px(4.0))
            .child(
                div()
                    .id("push-tree-repository")
                    .flex()
                    .items_center()
                    .gap(theme::from_px(6.0))
                    .h(theme::from_px(24.0))
                    .pl(theme::from_px(12.0))
                    .map(|row| {
                        crate::view::tree::ground(
                            row,
                            theme,
                            None,
                            theme::Selected::No,
                            theme.surface.app,
                        )
                    })
                    .child(icons::chevron(!root_folded).text_color(rgb(theme.text.tertiary)))
                    .child(
                        icons::icon(&icons::for_dir(
                            std::path::Path::new(""),
                            !root_folded,
                            palette,
                        ))
                        .flex_none(),
                    )
                    .child(seen.name.clone())
                    .child(
                        div()
                            .text_color(rgb(theme.text.faint))
                            .child(ui.diff_files(count)),
                    )
                    .child(
                        div()
                            .text_color(rgb(theme.text.faint))
                            .child(seen.name.clone()),
                    )
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.repository_folded = !this.repository_folded;
                        cx.notify();
                    })),
            )
            .children(shown.into_iter().enumerate().map(|(index, row)| {
                let indent = |depth: usize| theme::from_px(12.0 + (depth + 1) as f32 * 18.0);
                match row {
                    Row::Directory {
                        path,
                        files: count,
                        depth,
                        key,
                    } => {
                        let folded = self.folded.contains(&key);
                        let stem = icons::for_dir(std::path::Path::new(&path), !folded, palette);
                        div()
                            .id(("push-dir", index))
                            .flex()
                            .items_center()
                            .gap(theme::from_px(6.0))
                            .h(theme::from_px(24.0))
                            .pl(indent(depth))
                            .pr(theme::from_px(10.0))
                            .map(|row| {
                                crate::view::tree::ground(
                                    row,
                                    theme,
                                    None,
                                    theme::Selected::No,
                                    theme.surface.app,
                                )
                            })
                            .child(icons::chevron(!folded).text_color(rgb(theme.text.tertiary)))
                            .child(icons::icon(&stem).flex_none())
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(theme::size::BODY)
                                    .text_color(rgb(theme.text.primary))
                                    .child(path),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(theme::size::BODY)
                                    .text_color(rgb(theme.text.faint))
                                    .child(ui.diff_files(count)),
                            )
                            .on_click(cx.listener(move |this, _event, _window, cx| {
                                if !this.folded.remove(&key) {
                                    this.folded.insert(key.clone());
                                }
                                cx.notify();
                            }))
                            .into_any_element()
                    }
                    Row::File {
                        name,
                        path,
                        mark,
                        depth,
                        ..
                    } => {
                        let stem = icons::for_file(&name, palette);
                        let selected = self.file_selection.contains(&path);
                        let order = order.clone();
                        let sha = sha.clone();
                        let files = files.clone();
                        div()
                            .id(("push-file", index))
                            .debug_selector(move || format!("push-file-{index}"))
                            .flex()
                            .items_center()
                            .gap(theme::from_px(6.0))
                            .h(theme::from_px(24.0))
                            // No chevron: a file's icon stands where a
                            // directory's chevron does.
                            .pl(indent(depth) + theme::from_px(17.0))
                            .pr(theme::from_px(10.0))
                            .w_full()
                            .map(|row| {
                                crate::view::tree::ground(
                                    row,
                                    theme,
                                    Some(mark),
                                    theme::Selected::of(selected, focused),
                                    theme.surface.app,
                                )
                            })
                            .child(icons::icon(&stem).flex_none())
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(theme::size::BODY)
                                    .text_color(rgb(theme.text.primary))
                                    .child(crate::view::speed_search::label(name, &query, theme)),
                            )
                            // A click selects, a second opens.
                            .on_click(cx.listener(
                                move |this, event: &gpui::ClickEvent, window, cx| {
                                    let mods = event.modifiers();
                                    this.file_selection.click(
                                        &path,
                                        &order,
                                        mods.platform,
                                        mods.shift,
                                    );
                                    this.selected_file =
                                        this.file_selection.action_target().cloned();
                                    window.focus(&this.file_focus, cx);
                                    if event.click_count() >= 2 && !mods.platform && !mods.shift {
                                        this.open_file(&sha, &files, &path, cx);
                                    }
                                    cx.notify();
                                },
                            ))
                            .into_any_element()
                    }
                }
            }));
        div()
            .size_full()
            .flex()
            .flex_col()
            .children(self.speed_field(PushList::Files, seen, cx))
            .child(body)
            .into_any_element()
    }

    fn message(&self, seen: &Seen) -> gpui::AnyElement {
        let theme = self.theme;
        div()
            .id("push-message")
            .size_full()
            .min_h_0()
            .overflow_y_scroll()
            .p(theme::from_px(12.0))
            .text_size(theme::size::UI)
            .text_color(rgb(theme.text.primary))
            .children(seen.detail.clone().map(|text| {
                let mut lines = text.lines().map(str::to_owned);
                let first = lines.next().unwrap_or_default();
                div()
                    .flex()
                    .flex_col()
                    .child(div().font_weight(gpui::FontWeight::BOLD).child(first))
                    .children(lines.map(|line| {
                        if line.is_empty() {
                            div().h(theme::from_px(10.0))
                        } else {
                            div().child(line)
                        }
                    }))
            }))
            .into_any_element()
    }

    fn checkbox(&self, id: &'static str, ticked: bool) -> gpui::Stateful<gpui::Div> {
        let theme = self.theme;
        div()
            .id(id)
            .debug_selector(move || id.to_owned())
            .flex_none()
            .size(theme::space::CHECKBOX)
            .rounded(theme::radius::CHECKBOX)
            .border_1()
            .border_color(rgb(if ticked {
                theme.accent.brass
            } else {
                theme.text.faint
            }))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .when(ticked, |b| {
                b.bg(rgb(theme.accent.brass))
                    .child(icons::check().text_color(rgb(self.colours.on_accent)))
            })
    }

    fn foot(&self, seen: &Seen, window: &Window, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.theme;
        let ui = seen.ui;
        let can = crate::push_dialog::can_push(seen.commits.len(), seen.to.is_some(), seen.running);
        let select = div()
            .id("push-tags-kind")
            .debug_selector(|| "push-tags-kind".to_owned())
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::from_px(8.0))
            .h(theme::from_px(26.0))
            .w(theme::from_px(150.0))
            .px(theme::from_px(10.0))
            .rounded(theme::radius::ROW)
            .border_1()
            .border_color(rgb(self.colours.border))
            .text_size(theme::size::BODY)
            .text_color(rgb(if self.tags_on {
                theme.text.primary
            } else {
                theme.text.gutter
            }))
            .child(
                div()
                    .flex_1()
                    .child(ui.text(if self.tags == PushTags::CurrentBranch {
                        Text::PushTagsCurrent
                    } else {
                        Text::PushTagsAll
                    })),
            )
            .child(icons::chevron(true).text_color(rgb(if self.tags_on {
                theme.text.tertiary
            } else {
                theme.text.gutter
            })))
            .when(self.tags_on, |b| {
                b.cursor_pointer().on_click(cx.listener(
                    |this, event: &gpui::ClickEvent, _window, cx| {
                        this.open_popup(Popup::Tags, event.position(), cx)
                    },
                ))
            });
        let controls = div()
            .id("push-footer-controls")
            .debug_selector(|| "push-footer-controls".to_owned())
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::from_px(10.0))
            .child(
                div()
                    .id("push-help")
                    .debug_selector(|| "push-help".to_owned())
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .size(theme::from_px(22.0))
                    .rounded_full()
                    .border_1()
                    .border_color(rgb(theme.text.faint))
                    .cursor_pointer()
                    .child(icons::toolbar("ui/push-help.svg").text_color(rgb(theme.text.secondary)))
                    .hint_delay()
                    .tooltip(move |_window, app| {
                        app.new(|_cx| {
                            crate::view::tooltip::Tooltip::text(
                                ui.text(Text::MenuHelp).to_owned(),
                                theme,
                            )
                        })
                        .into()
                    })
                    .on_click(cx.listener(|this, event: &gpui::ClickEvent, _window, cx| {
                        this.open_popup(Popup::Help, event.position(), cx)
                    })),
            )
            .child(
                self.checkbox("push-tags", self.tags_on)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.tags_on = !this.tags_on;
                        cx.notify();
                    })),
            )
            .child(div().flex_none().child(ui.text(Text::PushTagsLabel)))
            .child(select)
            .child(
                self.checkbox("push-hooks", self.run_hooks)
                    .on_click(cx.listener(|this, _event, _window, cx| {
                        this.run_hooks = !this.run_hooks;
                        cx.notify();
                    })),
            )
            .child(div().flex_none().child(ui.text(Text::PushHooks)));
        let actions =
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(theme::from_px(10.0))
                .child(
                    div()
                        .id("push-cancel")
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .h(theme::from_px(28.0))
                        .min_w(theme::from_px(88.0))
                        .px(theme::from_px(16.0))
                        .rounded(theme::radius::ROW)
                        .border_1()
                        .border_color(rgb(self.colours.border))
                        .cursor_pointer()
                        .child(ui.text(Text::Cancel))
                        .on_click(|_event, window, _cx| window.remove_window()),
                )
                .child(
                    div()
                        .flex()
                        .flex_none()
                        .h(theme::from_px(28.0))
                        .rounded(theme::radius::ROW)
                        .overflow_hidden()
                        .bg(rgb(if can {
                            theme.accent.brass_bg
                        } else {
                            theme.surface.app
                        }))
                        .text_color(rgb(if can {
                            theme.accent.brass
                        } else {
                            theme.text.gutter
                        }))
                        .child(
                            div()
                                .id("push-go")
                                .flex()
                                .items_center()
                                .justify_center()
                                .min_w(theme::from_px(68.0))
                                .px(theme::from_px(16.0))
                                .child(ui.text(if seen.running {
                                    Text::PushRunning
                                } else {
                                    Text::PushButton
                                }))
                                .when(can, |b| {
                                    b.cursor_pointer().on_click(
                                        cx.listener(|this, _event, _window, cx| this.push(cx)),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .id("push-options")
                                .debug_selector(|| "push-options".to_owned())
                                .flex()
                                .flex_none()
                                .items_center()
                                .justify_center()
                                .w(theme::from_px(28.0))
                                .border_l_1()
                                .border_color(rgb(self.colours.border))
                                // GPUI's SVG painter requires its own colour;
                                // the button's inherited text colour is not used.
                                .child(icons::chevron(true).size(theme::from_px(14.0)).text_color(
                                    rgb(if can {
                                        theme.accent.brass
                                    } else {
                                        theme.text.gutter
                                    }),
                                ))
                                .when(can, |b| {
                                    b.cursor_pointer().on_click(cx.listener(
                                        |this, event: &gpui::ClickEvent, _window, cx| {
                                            this.open_popup(Popup::Push, event.position(), cx)
                                        },
                                    ))
                                }),
                        ),
                );
        let rows = Self::foot_rows(window);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(theme::from_px(10.0))
            .h(theme::from_px(FOOT * rows))
            .px(theme::from_px(16.0))
            .border_t_1()
            .border_color(rgb(theme.surface.divider))
            .text_size(theme::size::BODY)
            .when(rows > 1.0, |foot| {
                foot.flex_col().justify_center().items_start()
            })
            .child(controls)
            .when(rows == 1.0, |foot| foot.child(div().flex_1()))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .when(rows > 1.0, |line| line.w_full())
                    .child(actions),
            )
            .into_any_element()
    }

    fn take_popup(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((popup, _)) = self.popup else {
            return;
        };
        match popup {
            Popup::Tags => {
                self.tags = if index == 0 {
                    PushTags::All
                } else {
                    PushTags::CurrentBranch
                }
            }
            Popup::Remote => {
                if let Some(remote) = self.seen(cx).and_then(|s| s.remotes.get(index).cloned())
                    && let Some(root) = self.root.upgrade()
                {
                    let project = self.project;
                    self.selected_file = None;
                    self.file_selection = Default::default();
                    root.update(cx, |root, cx| root.push_remote_for(project, remote, cx));
                }
            }
            Popup::Target => {
                if !crate::push_dialog::valid_target(&self.target_text) {
                    return;
                }
                if let Some(root) = self.root.upgrade() {
                    let (project, target) = (self.project, self.target_text.clone());
                    self.selected_file = None;
                    self.file_selection = Default::default();
                    root.update(cx, |root, cx| root.push_target_for(project, target, cx));
                }
            }
            Popup::Push => self.push(cx),
            Popup::Help => {}
        }
        self.popup = None;
        cx.notify();
    }

    fn popup(
        &self,
        seen: &Seen,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (popup, at) = self.popup?;
        let theme = self.theme;
        let ui = seen.ui;
        let rows: Vec<String> = match popup {
            Popup::Tags => vec![
                ui.text(Text::PushTagsAll).to_owned(),
                ui.text(Text::PushTagsCurrent).to_owned(),
            ],
            Popup::Remote => seen.remotes.clone(),
            Popup::Push => vec![ui.text(Text::PushButton).to_owned()],
            _ => Vec::new(),
        };
        let card = div()
            .id("push-popup")
            .occlude()
            .w(theme::from_px(300.0))
            .p(theme::space::MENU_PADDING)
            .rounded(theme::radius::CARD)
            .bg(rgb(theme.surface.card))
            .border_1()
            .border_color(theme::card_outline(theme))
            .text_size(theme::size::BODY)
            .children(rows.into_iter().enumerate().map(|(i, text)| {
                div()
                    .id(("push-popup-row", i))
                    .debug_selector(move || format!("push-popup-row-{i}"))
                    .h(theme::space::MENU_ROW)
                    .px(theme::from_px(9.0))
                    .flex()
                    .items_center()
                    .rounded(theme::radius::ROW)
                    .when(i == self.popup_index, |b| b.bg(rgb(theme.accent.patina_bg)))
                    .cursor_pointer()
                    .hover(|b| b.bg(crate::theme::control_hover(theme, theme.surface.card)))
                    .child(text)
                    .on_click(cx.listener(move |this, _event, _window, cx| {
                        cx.stop_propagation();
                        this.take_popup(i, cx);
                    }))
            }))
            .when(matches!(popup, Popup::Help), |card| {
                card.child(
                    div()
                        .w_full()
                        .p(theme::from_px(9.0))
                        .child(ui.text(Text::PushHelp)),
                )
            })
            .when(matches!(popup, Popup::Target), |card| {
                card.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(theme::from_px(8.0))
                        .p(theme::from_px(9.0))
                        .child(ui.text(Text::PushTarget))
                        .child(
                            div()
                                .id("push-target-input")
                                .h(theme::from_px(28.0))
                                .w_full()
                                .flex()
                                .items_center()
                                .px(theme::from_px(8.0))
                                .rounded(theme::radius::ROW)
                                .border_1()
                                .border_color(rgb(
                                    if crate::push_dialog::valid_target(&self.target_text) {
                                        self.colours.border
                                    } else {
                                        theme.accent.rust
                                    },
                                ))
                                .bg(rgb(theme.surface.app))
                                .child(
                                    crate::text_field::TextField::new(
                                        "push-target-text",
                                        self.target_text.clone(),
                                        cx.listener(|this, value: &String, _, cx| {
                                            this.target_text = value.clone();
                                            this.target_selected = false;
                                            cx.notify();
                                        }),
                                    )
                                    .autofocus()
                                    .focus(&self.focus)
                                    .selected(self.target_selected)
                                    .on_key(cx.listener(
                                        |this, event: &gpui::KeyDownEvent, window, cx| {
                                            match event.keystroke.key.as_str() {
                                                "escape" => {
                                                    this.popup = None;
                                                    window.focus(&this.focus, cx);
                                                    cx.notify();
                                                }
                                                "enter" => {
                                                    this.take_popup(this.popup_index, cx);
                                                    window.focus(&this.focus, cx);
                                                }
                                                "tab" => {
                                                    if event.keystroke.modifiers.shift {
                                                        window.focus_prev(cx);
                                                    } else {
                                                        window.focus_next(cx);
                                                    }
                                                }
                                                _ => {}
                                            }
                                        },
                                    )),
                                ),
                        )
                        .child(
                            div()
                                .id("push-target-apply")
                                .h(theme::from_px(28.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(theme::radius::ROW)
                                .bg(rgb(theme.accent.brass_bg))
                                .text_color(rgb(theme.accent.brass))
                                .cursor_pointer()
                                .child(ui.text(Text::PushTargetApply))
                                .on_click(
                                    cx.listener(|this, _event, _window, cx| this.take_popup(0, cx)),
                                ),
                        ),
                )
            });
        Some(
            div()
                .children([
                    gpui::deferred(
                        gpui::anchored().position(Point::default()).child(
                            div()
                                .size_full()
                                .occlude()
                                .w(window.viewport_size().width)
                                .h(window.viewport_size().height)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _event, _window, cx| {
                                        cx.stop_propagation();
                                        this.popup = None;
                                        cx.notify();
                                    }),
                                ),
                        ),
                    )
                    .into_any_element(),
                    gpui::deferred(
                        gpui::anchored()
                            .position(at)
                            .snap_to_window_with_margin(px(8.0))
                            .child(card),
                    )
                    .priority(1)
                    .into_any_element(),
                ])
                .into_any_element(),
        )
    }
}

impl gpui::Render for PushDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focused {
            self.focused = true;
            window.focus(&self.focus, cx);
        }
        if let Some(root) = self.root.upgrade() {
            root.update(cx, |root, _| {
                root.remember_satellite(Satellite::Push, window);
            });
        }
        let theme = self.theme;
        let frame = crate::view::satellite::frame(&self.focus, theme, window, cx);
        self.speed_update(None, false, cx);
        let Some(seen) = self.seen(cx) else {
            // The project closed under the window: nothing to push from.
            return frame.into_any_element();
        };
        if let Some(Ok(files)) = &seen.files
            && self
                .selected_file
                .as_ref()
                .is_some_and(|path| !files.iter().any(|file| &file.path == path))
        {
            self.selected_file = None;
            self.preview_key = None;
        }
        let paths = if self.repository_folded {
            Vec::new()
        } else {
            seen.files
                .as_ref()
                .and_then(|f| f.as_ref().ok())
                .map(|files| {
                    let rows = files_tree::of(files);
                    files_tree::visible(&rows, &self.folded)
                        .into_iter()
                        .filter_map(|row| match row {
                            Row::File { path, .. } => Some(path.clone()),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default()
        };
        self.file_selection.retain(&paths);
        self.selected_file = self.file_selection.action_target().cloned();
        self.sync_preview(&seen, cx);
        let size = window.viewport_size();
        let (left_width, _) = self.sides.sizes(f32::from(size.width) - 9.0, MIN_SIDE);
        // Two lines of git's verdict — `refusal` keeps it to about that.
        let scale = f32::from(window.rem_size()) / crate::scale::BASE;
        let refused_height = if seen.refused.is_some() {
            FOOT * scale
        } else {
            0.0
        };
        let (tree_height, _) = self.panes.sizes(
            f32::from(size.height)
                - (FOOT * Self::foot_rows(window) + TOOLBAR) * scale
                - refused_height
                - 9.0,
            MIN_PANE,
        );
        let can = crate::push_dialog::can_push(seen.commits.len(), seen.to.is_some(), seen.running);
        let details = self.details && seen.detail.is_some();
        let preview = (self.preview_on
            && self.selected_file.is_some()
            && self
                .preview_key
                .as_ref()
                .is_some_and(|key| seen.opened.as_ref() == Some(&key.0)))
        .then(|| self.preview.clone())
        .flatten();

        frame
            .bg(rgb(theme.surface.app))
            .capture_key_down(
                cx.listener(|this, event: &gpui::KeyDownEvent, _window, cx| {
                    let Some((popup, _)) = this.popup else {
                        return;
                    };
                    if matches!(popup, Popup::Target) {
                        return;
                    }
                    cx.stop_propagation();
                    let key = event.keystroke.key.as_str();
                    if key == "escape" {
                        this.popup = None;
                        cx.notify();
                        return;
                    }
                    if key == "enter" {
                        this.take_popup(this.popup_index, cx);
                        return;
                    }
                    if key == "up" || key == "down" {
                        let count = match popup {
                            Popup::Tags => 2,
                            Popup::Remote => this.seen(cx).map_or(0, |s| s.remotes.len()),
                            _ => 1,
                        };
                        if count > 0 {
                            this.popup_index = if key == "down" {
                                (this.popup_index + 1) % count
                            } else {
                                (this.popup_index + count - 1) % count
                            };
                            cx.notify();
                        }
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::NextChange, _window, cx| {
                    if let Some(tab) = &this.preview {
                        tab.update(cx, |tab, cx| tab.step_change(true, cx));
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::PreviousChange, _window, cx| {
                    if let Some(tab) = &this.preview {
                        tab.update(cx, |tab, cx| tab.step_change(false, cx));
                    }
                }),
            )
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, _window, cx| {
                    if event.keystroke.key == "enter" && can {
                        cx.stop_propagation();
                        this.push(cx);
                    }
                }),
            )
            .child(self.resize_events(cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .flex_none()
                            .w(px(left_width))
                            .h_full()
                            .child(self.left(&seen, cx)),
                    )
                    .child(self.grip(Grip::Sides, cx))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(self.toolbar(&seen, cx))
                            .child(
                                div()
                                    .min_h_0()
                                    .when(details, |d| d.flex_none().h(px(tree_height)))
                                    .when(!details, |d| d.flex_1())
                                    .child(self.tree(&seen, window, cx)),
                            )
                            .when(details, |d| {
                                d.child(self.grip(Grip::Panes, cx))
                                    .child(div().flex_1().min_h_0().child(self.message(&seen)))
                            })
                            .relative()
                            .children(preview.map(|tab| {
                                div()
                                    .occlude()
                                    .absolute()
                                    .top(theme::from_px(TOOLBAR))
                                    .bottom_0()
                                    .left_0()
                                    .right_0()
                                    .bg(rgb(theme.surface.app))
                                    .child(tab)
                            })),
                    ),
            )
            .children(seen.refused.clone().map(|words| {
                div()
                    .flex()
                    .flex_col()
                    .flex_none()
                    .justify_center()
                    .h(px(refused_height))
                    .overflow_hidden()
                    .px(theme::from_px(16.0))
                    .bg(rgb(theme.accent.rust_bg))
                    .text_size(theme::size::UI)
                    .text_color(rgb(theme.accent.rust))
                    .children(
                        crate::push_dialog::refusal(&words)
                            .lines()
                            .take(2)
                            .map(|line| div().min_w_0().truncate().child(line.to_owned()))
                            .collect::<Vec<_>>(),
                    )
            }))
            .child(self.foot(&seen, window, cx))
            .children(self.popup(&seen, window, cx))
            .into_any_element()
    }
}

/// Inspection and its embedded preview step through the same unfolded tree order.
fn inspection_paths(files: &[FileChange]) -> Vec<PathBuf> {
    files_tree::of(files)
        .into_iter()
        .filter_map(|row| match row {
            Row::File { path, .. } => Some(path),
            Row::Directory { .. } => None,
        })
        .collect()
}
