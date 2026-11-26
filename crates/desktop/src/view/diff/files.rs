//! Inspection file picker: the shared file tree, without a second revision owner.
use super::*;
use crate::diff_tab::FilePicker;
use crate::files_tree::{self, Row};
use gpui::{
    KeyDownEvent, MouseButton, Pixels, Point, UniformListScrollHandle, anchored, deferred, point,
};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
};

impl DiffTab {
    fn picker_paths(&self) -> Vec<PathBuf> {
        if self.stepping().is_some() && !self.inspection_paths.is_empty() {
            self.inspection_paths.clone()
        } else {
            self.paths()
        }
    }
    fn picker_rows(&self) -> Vec<Row> {
        let files = self
            .picker_paths()
            .into_iter()
            .map(|path| crate::git::log::FileChange {
                path,
                from: None,
                mark: theme::Mark::Modified,
            })
            .collect::<Vec<_>>();
        let rows = files_tree::of(&files);
        match &self.file_picker {
            Some(picker) => files_tree::visible(&rows, &picker.folded)
                .into_iter()
                .cloned()
                .collect(),
            None => rows,
        }
    }
    pub(super) fn open_file_picker(
        &mut self,
        at: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_picker.is_some() {
            self.close_file_picker(window, cx);
            return;
        }
        self.authors_menu = None;
        self.viewer.selecting = false;
        self.viewer.resizing = false;
        let paths = self.picker_paths();
        let index = self
            .stepping()
            .map_or_else(|| self.current_file(), |(index, _)| index);
        let selected = self
            .picker_rows()
            .iter()
            .position(|row| matches!(row, Row::File { path, .. } if Some(path) == paths.get(index)))
            .unwrap_or(0);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        let scroll = UniformListScrollHandle::new();
        scroll.scroll_to_item(selected, gpui::ScrollStrategy::Center);
        self.file_picker = Some(FilePicker {
            focus,
            selected,
            folded: HashSet::new(),
            scroll,
            at,
        });
        cx.notify();
    }
    pub(super) fn close_file_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.file_picker = None;
        self.focus_handle(cx).focus(window, cx);
        cx.notify();
    }
    fn take_picker_row(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        match self.picker_rows().get(row).cloned() {
            Some(Row::Directory { key, .. }) => {
                if let Some(picker) = &mut self.file_picker {
                    picker.selected = row;
                    if !picker.folded.remove(&key) {
                        picker.folded.insert(key);
                    }
                }
                cx.notify();
            }
            Some(Row::File { path, .. }) => {
                if let Some(index) = self.picker_paths().iter().position(|p| p == &path) {
                    self.close_file_picker(window, cx);
                    if self.stepping().is_some()
                        && !self.inspection_paths.is_empty()
                        && let Some(key) = self.inspection_key().map(str::to_owned)
                    {
                        cx.emit(DiffEvent::SelectFile { index, key });
                    } else {
                        self.choose_file(index, cx);
                    }
                }
            }
            None => {}
        }
    }
    pub(super) fn file_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.file_picker.is_none() {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        let rows = self.picker_rows();
        let last = rows.len().saturating_sub(1);
        let picker = self.file_picker.as_mut().unwrap();
        match event.keystroke.key.as_str() {
            "escape" => {
                self.close_file_picker(window, cx);
                return;
            }
            "enter" | "space" => {
                let row = picker.selected;
                self.take_picker_row(row, window, cx);
                return;
            }
            "up" => picker.selected = picker.selected.saturating_sub(1),
            "down" => picker.selected = (picker.selected + 1).min(last),
            "home" => picker.selected = 0,
            "end" => picker.selected = last,
            "pageup" => picker.selected = picker.selected.saturating_sub(10),
            "pagedown" => picker.selected = (picker.selected + 10).min(last),
            "left" | "right" => {
                if let Some(Row::Directory { key, .. }) = rows.get(picker.selected) {
                    if event.keystroke.key == "left" {
                        picker.folded.insert(key.clone());
                    } else {
                        picker.folded.remove(key);
                    }
                }
            }
            _ => return,
        }
        picker
            .scroll
            .scroll_to_item(picker.selected, gpui::ScrollStrategy::Top);
        cx.notify();
    }
    pub(super) fn render_file_picker(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let picker = self.file_picker.as_ref()?;
        let t = self.look().theme;
        let rows = self.picker_rows();
        let selected = picker.selected;
        let paths = self.picker_paths();
        let current = self
            .stepping()
            .map_or_else(|| self.current_file(), |(index, _)| index);
        let current_path = paths.get(current).cloned();
        let indices: HashMap<_, _> = paths
            .into_iter()
            .enumerate()
            .map(|(index, path)| (path, index))
            .collect();
        let folded = picker.folded.clone();
        let list_height = (window.rem_size() * theme::space::RECENT_FILES_LIST.0).min(
            (window.viewport_size().height
                - window.rem_size()
                    * (theme::space::DIFF_TOOLBAR.0
                        + theme::space::TREE_ROW.0
                        + theme::space::MENU_PADDING.0 * 2.))
                .max(window.rem_size() * theme::space::PICKER_ROW.0),
        );
        let content = crate::view::popup::card(t)
            .id("diff-file-picker")
            .debug_selector(|| "diff-file-picker".into())
            .track_focus(&picker.focus)
            .role(gpui::Role::Tree)
            .aria_label(self.viewer_label("Inspection files", "Файлы сравнения"))
            .key_context("diff-file-picker")
            .on_key_down(
                cx.listener(|this, event, window, cx| this.file_picker_key(event, window, cx)),
            )
            .w(theme::space::BRANCH_POPUP)
            .max_w(window.viewport_size().width - px(16.))
            .child(crate::view::popup::heading(
                self.viewer_label("Inspection files", "Файлы сравнения")
                    .into(),
                t,
            ))
            .child(
                div()
                    .relative()
                    .h(list_height)
                    .min_h_0()
                    .child(
                        uniform_list(
                            "diff-picker-list",
                            rows.len(),
                            cx.processor(move |_this, range: std::ops::Range<usize>, _, cx| {
                                range
                                    .map(|row_index| {
                                        let (name, depth, path, directory) = match &rows[row_index]
                                        {
                                            Row::Directory {
                                                path, depth, key, ..
                                            } => (path.clone(), *depth, None, Some(key.clone())),
                                            Row::File {
                                                name, depth, path, ..
                                            } => (name.clone(), *depth, Some(path.clone()), None),
                                        };
                                        let is_current = path.is_some() && path == current_path;
                                        let file_index = path
                                            .as_ref()
                                            .and_then(|path| indices.get(path).copied());
                                        let icon = if let Some(key) = &directory {
                                            crate::icons::for_dir(
                                                std::path::Path::new(&name),
                                                !folded.contains(key),
                                                crate::icons::Palette::from(t.kind),
                                            )
                                        } else {
                                            crate::icons::for_file(
                                                &name,
                                                crate::icons::Palette::from(t.kind),
                                            )
                                        };
                                        let tint = rgb(if is_current {
                                            t.accent.brass
                                        } else {
                                            t.text.primary
                                        });
                                        div()
                                            .id(("diff-picker-row", row_index))
                                            .role(gpui::Role::TreeItem)
                                            .aria_label(path.as_ref().map_or_else(
                                                || name.clone(),
                                                |path| path.display().to_string(),
                                            ))
                                            .aria_selected(row_index == selected)
                                            .when_some(directory.as_ref(), |row, key| {
                                                row.aria_expanded(!folded.contains(key))
                                            })
                                            .debug_selector(move || {
                                                file_index.map_or_else(
                                                    || format!("diff-picker-folder-{row_index}"),
                                                    |index| format!("diff-file-choice-{index}"),
                                                )
                                            })
                                            .h(theme::space::PICKER_ROW)
                                            .flex()
                                            .items_center()
                                            .gap(theme::space::TREE_GAP)
                                            .mx(theme::radius::ROW)
                                            .rounded(theme::radius::ROW)
                                            .pl(theme::space::MENU_INSET
                                                + theme::space::TREE_INDENT * depth as f32)
                                            .pr(theme::space::MENU_INSET)
                                            .when(row_index == selected, |row| {
                                                row.bg(rgb(t.surface.raised))
                                            })
                                            .text_color(tint)
                                            // The row's colour reaches its name, not its
                                            // svgs: GPUI paints one only in a colour set on it.
                                            .when_some(directory, |row, key| {
                                                row.child(
                                                    crate::icons::chevron(!folded.contains(&key))
                                                        .text_color(tint),
                                                )
                                            })
                                            .child(crate::icons::icon(&icon).flex_none())
                                            .child(div().flex_1().min_w_0().truncate().child(name))
                                            .when(is_current, |row| {
                                                row.child(
                                                    crate::icons::check()
                                                        .flex_none()
                                                        .text_color(tint),
                                                )
                                            })
                                            .on_mouse_move(cx.listener(move |this, _, _, cx| {
                                                if let Some(picker) = &mut this.file_picker
                                                    && picker.selected != row_index
                                                {
                                                    picker.selected = row_index;
                                                    cx.notify();
                                                }
                                            }))
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.take_picker_row(row_index, window, cx);
                                            }))
                                            .into_any_element()
                                    })
                                    .collect()
                            }),
                        )
                        .track_scroll(&picker.scroll)
                        .size_full()
                        .pr(theme::space::MARK_BAR),
                    )
                    .child(
                        div()
                            .absolute()
                            .right_0()
                            .top_0()
                            .bottom_0()
                            .id("diff-picker-scrollbar")
                            .debug_selector(|| "diff-picker-scrollbar".into())
                            .w(theme::space::MARK_BAR)
                            .child(
                                gpui_component::scroll::Scrollbar::vertical(&picker.scroll)
                                    .id("diff-picker-vertical")
                                    .viewport_from_layout()
                                    .mode(gpui_component::scroll::ScrollbarMode::Always)
                                    .styles(|s| s.thumb(|s| s.bg(rgb(t.text.gutter)))),
                            ),
                    ),
            )
            .into_any_element();
        Some(
            div()
                .children([
                    deferred(
                        anchored().position(point(px(0.), px(0.))).child(
                            div()
                                .w(window.viewport_size().width)
                                .h(window.viewport_size().height)
                                .occlude()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, window, cx| {
                                        cx.stop_propagation();
                                        this.close_file_picker(window, cx);
                                    }),
                                ),
                        ),
                    )
                    .into_any_element(),
                    deferred(
                        anchored()
                            .position(picker.at)
                            .snap_to_window_with_margin(px(8.))
                            .child(content),
                    )
                    .into_any_element(),
                ])
                .into_any_element(),
        )
    }
}
