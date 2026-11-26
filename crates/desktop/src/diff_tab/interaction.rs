//! Transient read-only diff gestures. Text always comes from a source snapshot.

use super::{DiffEvent, DiffTab, Line};
use crate::diff::document::DisplayRow;
use gpui::{Bounds, Context, Pixels, Point, ShapedLine, Size, Window, point, px};
use gpui_component::scroll::ScrollbarHandle;
use std::{cell::RefCell, collections::HashMap, ops::Range, path::PathBuf, rc::Rc};

/// The transient file popup has no revision authority of its own.
pub(crate) struct FilePicker {
    pub focus: gpui::FocusHandle,
    pub selected: usize,
    pub folded: std::collections::HashSet<String>,
    pub scroll: gpui::UniformListScrollHandle,
    pub at: Point<Pixels>,
}

#[derive(Clone, Default)]
pub(crate) struct Horizontal(pub Rc<RefCell<HorizontalState>>);
#[derive(Default)]
pub(crate) struct HorizontalState {
    pub bounds: Bounds<Pixels>,
    pub width: Pixels,
    pub offset: Pixels,
}
impl ScrollbarHandle for Horizontal {
    fn viewport_bounds(&self) -> Bounds<Pixels> {
        self.0.borrow().bounds
    }
    fn offset(&self) -> Point<Pixels> {
        point(-self.0.borrow().offset, px(0.))
    }
    fn set_offset(&self, offset: Point<Pixels>) {
        let mut state = self.0.borrow_mut();
        state.offset = (-offset.x)
            .max(px(0.))
            .min((state.width - state.bounds.size.width).max(px(0.)));
    }
    fn content_size(&self) -> Size<Pixels> {
        let s = self.0.borrow();
        gpui::size(s.width.max(s.bounds.size.width), s.bounds.size.height)
    }
}

#[derive(Clone)]
pub(crate) struct HitLine {
    pub file: usize,
    pub side: usize,
    pub line: usize,
    pub bounds: Bounds<Pixels>,
    pub shaped: ShapedLine,
    pub horizontal: usize,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextSelection {
    pub path: PathBuf,
    pub side: usize,
    pub anchor: usize,
    pub head: usize,
}
impl TextSelection {
    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }
}

pub(crate) struct SourceMentions {
    pub selection: TextSelection,
    pub document: std::sync::Arc<crate::diff::document::Document>,
    pub ranges: [Vec<Range<usize>>; 2],
}

type Refinements = HashMap<(usize, usize, usize), [Vec<Range<usize>>; 2]>;

pub(crate) struct Viewer {
    pub horizontal: [Horizontal; 3],
    pub hits: Rc<RefCell<Vec<HitLine>>>,
    pub selection: Option<TextSelection>,
    pub occurrences: Option<SourceMentions>,
    pub selecting: bool,
    pub pointer: Option<Point<Pixels>>,
    pub edge_pending: bool,
    pub resizing: bool,
    pub ratio: f32,
    pub minimum_ratio: f32,
    pub maximum_ratio: f32,
    pub viewport: Bounds<Pixels>,
    pub active_change: Option<(usize, usize)>,
    pub active_file: usize,
    pub focus_requested: bool,
    pub file_anchor: Option<usize>,
    pub width_key: String,
    pub number_key: String,
    pub number_width: Pixels,
    pub refinements: Refinements,
}
impl Default for Viewer {
    fn default() -> Self {
        Self {
            horizontal: Default::default(),
            hits: Default::default(),
            selection: None,
            occurrences: None,
            selecting: false,
            pointer: None,
            edge_pending: false,
            resizing: false,
            ratio: 0.5,
            minimum_ratio: 0.2,
            maximum_ratio: 0.8,
            viewport: Default::default(),
            active_change: None,
            active_file: 0,
            focus_requested: false,
            file_anchor: None,
            width_key: String::new(),
            number_key: String::new(),
            number_width: px(0.),
            refinements: HashMap::new(),
        }
    }
}
impl Viewer {
    pub fn invalidate(&mut self) {
        self.selection = None;
        self.occurrences = None;
        self.selecting = false;
        self.active_change = None;
        self.width_key.clear();
        self.refinements.clear();
        for h in &self.horizontal {
            h.0.borrow_mut().width = px(0.);
        }
    }
}

/// Select whole composed characters. Each cluster is classified at most once;
/// neither the left prefix nor any intermediate word string is constructed.
fn word_selection(text: &str, at: usize) -> Option<Range<usize>> {
    word_selection_with(text, at, |cluster| {
        cluster.chars().any(|ch| ch.is_alphanumeric() || ch == '_')
    })
}

fn word_selection_with(
    text: &str,
    at: usize,
    mut is_word: impl FnMut(&str) -> bool,
) -> Option<Range<usize>> {
    let boundaries = crate::system::grapheme_boundaries(text);
    let index = boundaries
        .partition_point(|byte| *byte <= at)
        .checked_sub(1)?;
    let end = *boundaries.get(index + 1)?;
    if !is_word(&text[boundaries[index]..end]) {
        return None;
    }
    let mut left = index;
    while left > 0 && is_word(&text[boundaries[left - 1]..boundaries[left]]) {
        left -= 1;
    }
    let mut right = index + 1;
    while right + 1 < boundaries.len() && is_word(&text[boundaries[right]..boundaries[right + 1]]) {
        right += 1;
    }
    Some(boundaries[left]..boundaries[right])
}

impl DiffTab {
    pub(crate) fn refresh_occurrences(&mut self) {
        let Some(selection) = self
            .viewer
            .selection
            .as_ref()
            .filter(|s| s.range().is_empty())
        else {
            self.viewer.occurrences = None;
            return;
        };
        let Some(document) = self.documents.get(&selection.path) else {
            self.viewer.occurrences = None;
            return;
        };
        if self.viewer.occurrences.as_ref().is_some_and(|cached| {
            cached.selection == *selection && std::sync::Arc::ptr_eq(&cached.document, document)
        }) {
            return;
        }
        let source = &document.sources[selection.side].text;
        let word = (source.len() as u64 <= crate::file_tab::HIGHLIGHT_LIMIT)
            .then(|| crate::source_occurrences::identifier(source, selection.head))
            .flatten();
        let ranges = document.sources.each_ref().map(|source| {
            word.map(|word| crate::source_occurrences::mentions(&source.text, word))
                .unwrap_or_default()
        });
        self.viewer.occurrences = Some(SourceMentions {
            selection: selection.clone(),
            document: document.clone(),
            ranges,
        });
    }

    pub(crate) fn change_targets(&self) -> Vec<(usize, usize, usize)> {
        let mut out = Vec::new();
        for (index, line) in self.lines.iter().enumerate() {
            if let Line::Source {
                file,
                row: DisplayRow::Text {
                    block: Some(block), ..
                },
            } = line
                && out.last().is_none_or(|(f, b, _)| f != file || b != block)
            {
                out.push((*file, *block, index));
            }
        }
        out
    }
    pub(crate) fn current_change(&self) -> Option<usize> {
        let targets = self.change_targets();
        if let Some((f, b)) = self.viewer.active_change {
            return targets
                .iter()
                .position(|(file, block, _)| *file == f && *block == b);
        }
        let top = self.top_row();
        targets.iter().rposition(|(_, _, row)| *row <= top)
    }
    pub(crate) fn can_step_change(&self, forward: bool) -> bool {
        let count = self.change_targets().len();
        if forward {
            self.current_change().is_none_or(|i| i + 1 < count) && count > 0
        } else {
            self.current_change().is_some_and(|i| i > 0)
        }
    }
    pub(crate) fn step_file(&mut self, forward: bool, cx: &mut Context<Self>) {
        if let Some((index, total)) = self.stepping() {
            if (forward && index + 1 < total) || (!forward && index > 0) {
                cx.emit(DiffEvent::StepFile { forward });
            }
        } else {
            let file = self.current_file();
            let next = if forward {
                file.checked_add(1).filter(|i| *i < self.files().len())
            } else {
                file.checked_sub(1)
            };
            if let Some(file) = next {
                self.choose_file(file, cx);
            }
        }
    }
    pub(crate) fn choose_file(&mut self, file: usize, cx: &mut Context<Self>) {
        if file >= self.files.len() {
            return;
        }
        self.viewer.active_file = file;
        self.viewer.file_anchor = Some(file);
        self.viewer.active_change = None;
        self.viewer.selection = None;
        if let Some(row) = super::header_index(&self.lines, file) {
            self.scroll_to_row(row, cx);
        }
        cx.notify();
    }
    pub(crate) fn current_file(&self) -> usize {
        self.viewer
            .active_change
            .map(|(file, _)| file)
            .or(self.viewer.file_anchor)
            .unwrap_or_else(|| {
                super::file_at(&self.lines, self.top_row()).unwrap_or(self.viewer.active_file)
            })
            .min(self.files.len().saturating_sub(1))
    }
    pub(crate) fn project_preserving_position(&mut self, cx: &mut Context<Self>) {
        let offset = -f32::from(self.scroll.0.borrow().base_handle.offset().y);
        let top = (offset / self.row_px.max(1.)).floor().max(0.) as usize;
        let anchor = self.lines.get(top).cloned();
        let path = anchor
            .as_ref()
            .and_then(line_file)
            .and_then(|file| self.files.get(file))
            .map(|f| f.path.clone());
        let prefer_old = self
            .viewer
            .selection
            .as_ref()
            .is_some_and(|s| Some(&s.path) == path.as_ref() && s.side == 0);
        self.project_rows();
        if self.viewer.active_change.is_some() {
            self.reveal_active(cx);
            return;
        }
        let Some(anchor) = anchor else {
            return;
        };
        let target = self.lines.iter().position(|line| {
            let candidate = line_file(line)
                .and_then(|f| self.files.get(f))
                .map(|f| &f.path);
            if candidate != path.as_ref() {
                return false;
            }
            match (&anchor, line) {
                (
                    Line::Source {
                        row: DisplayRow::Text { old: a, new: b, .. },
                        ..
                    },
                    Line::Source {
                        row: DisplayRow::Text { old, new, .. },
                        ..
                    },
                ) => {
                    if (prefer_old && a.is_some()) || b.is_none() {
                        old == a
                    } else {
                        new == b
                    }
                }
                (
                    Line::Source {
                        row: DisplayRow::Fold { equal: a, .. },
                        ..
                    },
                    Line::Source {
                        row: DisplayRow::Fold { equal: b, .. },
                        ..
                    },
                ) => a == b,
                (
                    Line::Source {
                        row: DisplayRow::Fold { equal, .. },
                        ..
                    },
                    Line::Source {
                        row: DisplayRow::Text { new, .. },
                        ..
                    },
                ) => path
                    .as_ref()
                    .and_then(|p| self.documents.get(p))
                    .and_then(|d| d.equals.get(*equal))
                    .is_some_and(|b| {
                        let leading = if b.old.start == 0 && b.new.start == 0 {
                            0
                        } else {
                            3
                        };
                        *new == Some(b.new.start + leading)
                    }),
                (
                    Line::Source {
                        row: DisplayRow::NoNewline { side: a },
                        ..
                    },
                    Line::Source {
                        row: DisplayRow::NoNewline { side: b },
                        ..
                    },
                ) => a == b,
                (Line::File { .. }, Line::File { .. })
                | (Line::Note { .. }, Line::Note { .. })
                | (Line::Listed { .. }, Line::Listed { .. })
                | (Line::Summary, Line::Summary) => true,
                _ => false,
            }
        });
        if let Some(target) = target {
            self.scroll
                .scroll_to_item_strict(target, gpui::ScrollStrategy::Top);
        }
    }

    pub(crate) fn expand_context(&mut self, file: usize, equal: usize, cx: &mut Context<Self>) {
        if let Some(f) = self.files.get(file) {
            self.expanded
                .entry(f.path.clone())
                .or_default()
                .insert(equal);
            self.project_preserving_position(cx);
            cx.notify();
        }
    }
    pub(crate) fn reveal_active(&mut self, cx: &mut Context<Self>) {
        if let Some((file, block)) = self.viewer.active_change
            && let Some((_, _, row)) = self
                .change_targets()
                .into_iter()
                .find(|(f, b, _)| *f == file && *b == block)
        {
            self.scroll_to_row(row, cx);
        }
    }
    pub(crate) fn copy_text(&self, cx: &mut Context<Self>) {
        if let Some(sel) = &self.viewer.selection
            && let Some(doc) = self.documents.get(&sel.path)
            && let Some(text) = doc.sources[sel.side].text.get(sel.range())
            && !text.is_empty()
        {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
        }
    }
    pub(crate) fn select_all_text(&mut self, cx: &mut Context<Self>) {
        let file = self.current_file();
        let Some(f) = self.files.get(file) else {
            return;
        };
        let side = self.viewer.selection.as_ref().map_or(1, |s| s.side);
        let Some(doc) = self.documents.get(&f.path) else {
            return;
        };
        self.viewer.selection = Some(TextSelection {
            path: f.path.clone(),
            side,
            anchor: 0,
            head: doc.sources[side].text.len(),
        });
        cx.notify();
    }
    pub(crate) fn hit_text(
        &self,
        position: Point<Pixels>,
        locked: Option<(&PathBuf, usize)>,
    ) -> Option<(PathBuf, usize, usize)> {
        let hits = self.viewer.hits.borrow();
        let hit = hits
            .iter()
            .filter(|hit| {
                locked.is_none_or(|(path, side)| {
                    self.files.get(hit.file).is_some_and(|f| &f.path == path) && hit.side == side
                })
            })
            .min_by_key(|hit| {
                let dy = if position.y < hit.bounds.top() {
                    hit.bounds.top() - position.y
                } else if position.y > hit.bounds.bottom() {
                    position.y - hit.bounds.bottom()
                } else {
                    px(0.)
                };
                let dx = if position.x < hit.bounds.left() {
                    hit.bounds.left() - position.x
                } else if position.x > hit.bounds.right() {
                    position.x - hit.bounds.right()
                } else {
                    px(0.)
                };
                (f32::from(dy) * 10000. + f32::from(dx)) as i64
            })?;
        if locked.is_none() && !hit.bounds.contains(&position) {
            return None;
        }
        let path = &self.files.get(hit.file)?.path;
        let doc = self.documents.get(path)?;
        let source = &doc.sources[hit.side];
        let range = source.lines.get(hit.line)?;
        let x = position.x - hit.bounds.left()
            + self.viewer.horizontal[hit.horizontal].0.borrow().offset;
        let index = hit.shaped.closest_index_for_x(x);
        let boundaries = crate::system::grapheme_boundaries(source.line(hit.line));
        let index = boundaries
            .into_iter()
            .min_by_key(|b| b.abs_diff(index))
            .unwrap_or(0);
        Some((path.clone(), hit.side, range.start + index))
    }
    pub(crate) fn begin_selection(
        &mut self,
        event: &gpui::MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((path, side, at)) = self.hit_text(event.position, None) else {
            return;
        };
        window.focus(&self.focus, cx);
        self.viewer.pointer = Some(event.position);
        self.viewer.file_anchor = self.files.iter().position(|f| f.path == path);
        self.viewer.active_change = None;
        let anchor = self
            .viewer
            .selection
            .as_ref()
            .filter(|s| event.modifiers.shift && s.path == path && s.side == side)
            .map_or(at, |s| s.anchor);
        self.viewer.selection = Some(TextSelection {
            path,
            side,
            anchor,
            head: at,
        });
        if event.click_count >= 2 {
            if let Some(sel) = self.viewer.selection.as_mut()
                && let Some(doc) = self.documents.get(&sel.path)
            {
                let source = &doc.sources[sel.side];
                let range = if event.click_count >= 3 {
                    let line = source
                        .lines
                        .partition_point(|r| r.start <= at)
                        .saturating_sub(1);
                    source.lines.get(line).map(|r| {
                        r.start
                            ..source
                                .lines
                                .get(line + 1)
                                .map_or(source.text.len(), |r| r.start)
                    })
                } else {
                    word_selection(source.text.as_ref(), at)
                };
                if let Some(range) = range {
                    sel.anchor = range.start;
                    sel.head = range.end;
                }
            }
            self.viewer.selecting = false;
            cx.stop_propagation();
            cx.notify();
            return;
        }
        self.viewer.selecting = true;
        cx.stop_propagation();
        cx.notify();
    }
    pub(crate) fn drag_selection(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        self.viewer.pointer = Some(position);
        let Some(sel) = self.viewer.selection.as_ref() else {
            return;
        };
        let side = sel.side;
        if let Some((_, _, at)) = self.hit_text(position, Some((&sel.path, sel.side)))
            && let Some(sel) = self.viewer.selection.as_mut()
        {
            sel.head = at;
        }
        let bounds = self.viewer.viewport;
        let delta = if position.y < bounds.top() {
            px(20.)
        } else if position.y > bounds.bottom() {
            -px(20.)
        } else {
            px(0.)
        };
        if delta != px(0.) {
            let base = &self.scroll.0.borrow().base_handle;
            let mut offset = base.offset();
            offset.y = (offset.y + delta).min(px(0.)).max(-base.max_offset().y);
            base.set_offset(offset);
        }
        let horizontal = if self.mode() == guild_session::DiffView::Unified {
            2
        } else {
            side
        };
        let handle = &self.viewer.horizontal[horizontal];
        let bounds = handle.0.borrow().bounds;
        let dx = if position.x < bounds.left() {
            px(20.)
        } else if position.x > bounds.right() {
            -px(20.)
        } else {
            px(0.)
        };
        if dx != px(0.) {
            let mut offset = handle.offset();
            offset.x += dx;
            handle.set_offset(offset);
        }
        cx.notify();
    }
    pub(crate) fn resize_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let bounds = self.viewer.viewport;
        self.viewer.ratio = (f32::from(position.x - bounds.left())
            / f32::from(bounds.size.width).max(1.))
        .clamp(self.viewer.minimum_ratio, self.viewer.maximum_ratio);
        cx.notify();
    }
}

fn line_file(line: &Line) -> Option<usize> {
    match line {
        Line::Source { file, .. }
        | Line::File { file }
        | Line::Note { file }
        | Line::Listed { file }
        | Line::Pair { file, .. }
        | Line::Hunk { file, .. } => Some(*file),
        Line::Summary => None,
    }
}

/// One logical block's extent in the current displayed projection.
#[derive(Clone, Debug)]
pub(crate) struct OverviewRange {
    pub file: usize,
    pub block: usize,
    pub rows: Range<usize>,
    pub kind: super::HunkKind,
}
impl DiffTab {
    pub(crate) fn overview_ranges(&self) -> Vec<OverviewRange> {
        let mut ranges: Vec<OverviewRange> = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            if let Line::Source {
                file,
                row: DisplayRow::Text {
                    block: Some(block), ..
                },
            } = line
            {
                if let Some(last) = ranges.last_mut()
                    && last.file == *file
                    && last.block == *block
                {
                    last.rows.end = row + 1;
                    continue;
                }
                let Some(change) = self
                    .files
                    .get(*file)
                    .and_then(|file| self.documents.get(&file.path))
                    .and_then(|doc| doc.blocks.get(*block))
                else {
                    continue;
                };
                let kind = if change.old.is_empty() {
                    super::HunkKind::Added
                } else if change.new.is_empty() {
                    super::HunkKind::Removed
                } else {
                    super::HunkKind::Changed
                };
                ranges.push(OverviewRange {
                    file: *file,
                    block: *block,
                    rows: row..row + 1,
                    kind,
                });
            }
        }
        ranges
    }
}
pub(crate) fn overview_span(rows: &Range<usize>, total: usize, minimum: f32) -> (f32, f32) {
    let total = total.max(1) as f32;
    let minimum = minimum.clamp(0., 1.);
    let top = (rows.start as f32 / total).min(1. - minimum);
    let end = (rows.end as f32 / total).max(top + minimum).min(1.);
    (top, end - top)
}

#[cfg(test)]
mod tests;
