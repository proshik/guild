//! Blame decorates the retained editor; it never owns a second text view.
use super::*;
use crate::git::{
    GitError,
    blame::{self, Line, Run},
};
use crate::view::tooltip::Hint as _;
#[cfg(test)]
mod tests;
#[derive(Default)]
pub(super) struct Inline {
    enabled: bool,
    generation: u64,
    mapping: u64,
    runs: Vec<Run>,
    pub(super) lines: Vec<Option<Line>>,
    error: Option<String>,
    mapped_text: Option<gpui_component::Rope>,
    labels: Vec<String>,
}
pub struct BlameRequest {
    pub generation: u64,
    pub path: PathBuf,
}
#[derive(Clone)]
pub struct RevealCommit(pub Line);
impl gpui::EventEmitter<BlameRequest> for FileTab {}
impl gpui::EventEmitter<RevealCommit> for FileTab {}
impl FileTab {
    pub fn blame_enabled(&self) -> bool {
        self.blame.enabled && !self.read_only && !self.relocating
    }
    pub(crate) fn toggle_blame(&mut self, cx: &mut Context<Self>) {
        if self.read_only {
            return;
        }
        self.blame.enabled = !self.blame.enabled;
        self.refresh_blame(cx);
        cx.notify();
    }
    pub(crate) fn refresh_blame(&mut self, cx: &mut Context<Self>) {
        self.blame.generation += 1;
        self.blame.mapping += 1;
        self.blame.runs.clear();
        self.blame.lines.clear();
        self.blame.error = None;
        if self.blame_enabled() && !self.relocating {
            cx.emit(BlameRequest {
                generation: self.blame.generation,
                path: self.relative.clone(),
            });
        }
        cx.notify();
    }
    pub(crate) fn blame_landed(
        &mut self,
        generation: u64,
        path: &Path,
        runs: Result<Vec<Run>, GitError>,
        cx: &mut Context<Self>,
    ) {
        if !self.blame_enabled()
            || self.relocating
            || generation != self.blame.generation
            || path != self.relative
        {
            return;
        }
        match runs {
            Ok(runs) => {
                self.blame.runs = runs;
                self.remap_blame(cx);
            }
            Err(error) => {
                self.blame.error = Some(error.to_string());
                cx.notify();
            }
        }
    }
    pub(super) fn remap_blame(&mut self, cx: &mut Context<Self>) {
        if !self.blame_enabled() {
            return;
        }
        self.blame.mapping += 1;
        self.blame.lines.clear();
        cx.notify();
        let mapping = self.blame.mapping;
        let generation = self.blame.generation;
        let runs = self.blame.runs.clone();
        let mapped_text = self.editor.as_ref().map(|e| e.read(cx).text().clone());
        let text = mapped_text
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| self.pending.clone())
            .unwrap_or_else(|| self.clean.clone());
        cx.spawn(async move |file, cx| {
            let lines = cx
                .background_executor()
                .spawn(async move { blame::map(&runs, &text) })
                .await;
            let _ = file.update(cx, |file, cx| {
                if file.blame_enabled()
                    && file.blame.mapping == mapping
                    && file.blame.generation == generation
                {
                    file.blame.lines = lines;
                    file.blame.labels = file
                        .blame
                        .lines
                        .iter()
                        .flatten()
                        .map(Line::compact)
                        .collect::<std::collections::HashSet<_>>()
                        .into_iter()
                        .collect();
                    file.blame.mapped_text = mapped_text;
                    cx.notify();
                }
            });
        })
        .detach();
    }
    pub(super) fn blame_width(&self, window: &Window) -> gpui::Pixels {
        if !self.blame_enabled() {
            return px(0.);
        }
        let font = window.text_style().font();
        let size = theme::size::META.to_pixels(window.rem_size());
        let labels = self.blame.labels.iter().map(String::as_str).chain([
            self.ui.text(Label::AnnotateNotCommitted),
            self.ui.text(Label::LogLoading),
            self.ui.text(Label::BlameUnavailable),
        ]);
        let width = labels
            .map(|label| gutter::text_width(label, size, font.clone(), window))
            .fold(px(0.), gpui::Pixels::max);
        (width + theme::space::CARD_GAP.to_pixels(window.rem_size()) * 2.)
            .min(theme::space::ANNOTATE_COLUMN.to_pixels(window.rem_size()))
    }
    pub(super) fn blame_cell(&self, row: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let theme = self.look.theme;
        let line = self.blame.lines.get(row).cloned().flatten();
        let text = line.as_ref().map(Line::compact).unwrap_or_else(|| {
            self.ui
                .text(if self.blame.error.is_some() {
                    Label::BlameUnavailable
                } else if self.blame.lines.is_empty() {
                    Label::LogLoading
                } else {
                    Label::AnnotateNotCommitted
                })
                .to_owned()
        });
        let tooltip = line
            .as_ref()
            .map(Line::tooltip)
            .unwrap_or_else(|| self.blame.error.clone().unwrap_or_else(|| text.clone()));
        div()
            .id(("inline-blame", row))
            .debug_selector(|| "inline-blame-cell".into())
            .size_full()
            .flex()
            .items_center()
            .px(theme::space::CARD_GAP)
            .text_size(theme::size::META)
            .text_color(rgb(theme.text.faint))
            .child(div().min_w_0().truncate().child(text))
            .hint_delay()
            .tooltip(move |_, cx| {
                cx.new(|_| crate::view::tooltip::Tooltip::details(tooltip.clone(), theme))
                    .into()
            })
            .when_some(line, |cell, line| {
                cell.cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(
                        cx.listener(move |file, event: &gpui::ClickEvent, window, cx| {
                            cx.stop_propagation();
                            // Revalidate against the latest mapped buffer when a previously
                            // painted cell receives a click after an edit.
                            if file.blame_enabled()
                                && file.gutter_row_contains(row, event.position(), window, cx)
                                && file.blame.lines.get(row).and_then(Option::as_ref) == Some(&line)
                                && file.editor.as_ref().is_some_and(|e| {
                                    Some(e.read(cx).text()) == file.blame.mapped_text.as_ref()
                                })
                            {
                                cx.emit(RevealCommit(line.clone()));
                            }
                        }),
                    )
            })
            .into_any_element()
    }
}

#[cfg(feature = "native-preview")]
impl FileTab {
    pub(crate) fn native_occurrence_blame(&mut self, runs: Vec<Run>, cx: &mut Context<Self>) {
        self.blame.enabled = true;
        self.blame.runs = runs;
        self.remap_blame(cx);
    }
}
