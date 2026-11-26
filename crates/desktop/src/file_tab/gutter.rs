//! Native gutter cells aligned to the retained component's painted layout.
use gpui::{prelude::*, *};
use gpui_component::{Rope, RopeExt, input::EditorState};
use std::{cell::RefCell, rc::Rc};

// Observe actual paint submissions, not prepainted hitboxes. Native captures
// separately verify the pixels; GPUI's default test platform has no rasterizer.
#[cfg(test)]
thread_local! {
    static PAINTED_NUMBERS: RefCell<Vec<(usize, Bounds<Pixels>)>> = const { RefCell::new(Vec::new()) };
}
#[cfg(test)]
fn painted_numbers() -> Vec<(usize, Bounds<Pixels>)> {
    PAINTED_NUMBERS.with(|numbers| numbers.borrow().clone())
}

/// Only gutter metadata is shaped here; the retained Editor owns file text.
pub(crate) fn text_width(text: &str, size: Pixels, font: Font, window: &Window) -> Pixels {
    window
        .text_system()
        .shape_line(
            text.to_owned().into(),
            size,
            &[TextRun {
                len: text.len(),
                font,
                color: black(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        )
        .width
}

/// Folded offsets can project to the next visible line's start through the
/// public range API. Keep only the last buffer row at each measured position.
fn visible_rows(state: &EditorState) -> Vec<(usize, Bounds<Pixels>)> {
    let Some(visible) = state.visible_row_range() else {
        return Vec::new();
    };
    let Some(height) = state.line_height() else {
        return Vec::new();
    };
    let viewport = state.input_bounds();
    let mut rows: Vec<(usize, Bounds<Pixels>)> = Vec::new();
    for row in visible.rev() {
        let offset = state.text().line_start_offset(row);
        let Some(bounds) = state.range_to_bounds(&(offset..offset)) else {
            continue;
        };
        if bounds.top() < viewport.top()
            || bounds.top() + height > viewport.bottom()
            || rows
                .last()
                .is_some_and(|(_, last)| last.top() == bounds.top())
        {
            continue;
        }
        rows.push((row, bounds));
    }
    rows.reverse();
    rows
}

#[derive(Clone, PartialEq)]
pub(super) struct Frame {
    bounds: Bounds<Pixels>,
    scroll: Point<Pixels>,
    text: Rope,
    font: f32,
    family: SharedString,
    viewport: Size<Pixels>,
    scale: f32,
    input_bounds: Bounds<Pixels>,
    width: Pixels,
    marker_width: Pixels,
    rows: Vec<(usize, Bounds<Pixels>)>,
}
pub(super) struct Numbers {
    pub font: Font,
    pub size: Pixels,
    pub start: Pixels,
    pub width: Pixels,
    pub padding: Pixels,
    pub normal: Hsla,
    pub current: Hsla,
}

pub(super) struct MarkerLane {
    pub items: std::sync::Arc<[crate::line_changes::Marker]>,
    /// Parallel to `items`: the marker's block is wholly in the index and
    /// is drawn as an outline of its colour — GLD-026. Shorter or empty
    /// while the offers are not in.
    pub staged: std::sync::Arc<[bool]>,
    pub width: Pixels,
    pub stripe: Pixels,
    pub deleted: Pixels,
    pub colors: [u32; 3],
}

pub(super) type MarkerClick = Rc<dyn Fn(usize, Bounds<Pixels>, &mut Window, &mut App)>;

pub(super) type Snapshot = Rc<RefCell<Option<Frame>>>;
pub(super) struct Gutter {
    pub body: AnyElement,
    pub editor: Entity<EditorState>,
    pub snapshot: Snapshot,
    pub font: f32,
    pub width: Pixels,
    pub cells: Vec<(usize, AnyElement)>,
    numbers: Numbers,
    ready: Vec<AnyElement>,
    markers: Option<MarkerLane>,
    marker_click: Option<MarkerClick>,
    marker_cells: Vec<AnyElement>,
}
impl Gutter {
    pub fn new(
        body: AnyElement,
        editor: Entity<EditorState>,
        snapshot: Snapshot,
        font: f32,
        width: Pixels,
        cells: Vec<(usize, AnyElement)>,
        numbers: Numbers,
    ) -> Self {
        Self {
            body,
            editor,
            snapshot,
            font,
            width,
            cells,
            numbers,
            ready: Vec::new(),
            markers: None,
            marker_click: None,
            marker_cells: Vec::new(),
        }
    }
    pub fn git_markers(mut self, markers: Option<MarkerLane>) -> Self {
        self.markers = markers;
        self
    }
    pub fn on_git_marker(mut self, click: Option<MarkerClick>) -> Self {
        self.marker_click = click;
        self
    }
    fn frame(&self, bounds: Bounds<Pixels>, window: &Window, cx: &App) -> Frame {
        let state = self.editor.read(cx);
        Frame {
            bounds,
            scroll: state.scroll_offset(),
            text: state.text().clone(),
            font: self.font,
            family: self.numbers.font.family.clone(),
            viewport: window.viewport_size(),
            scale: window.scale_factor(),
            input_bounds: state.input_bounds(),
            width: self.width,
            marker_width: self.markers.as_ref().map_or(px(0.), |lane| lane.width),
            rows: visible_rows(state),
        }
    }
}

/// The visual stripe lane is not part of the number/Annotate action target.
pub(super) fn marker_contains(snapshot: &Snapshot, point: Point<Pixels>) -> bool {
    snapshot.borrow().as_ref().is_some_and(|frame| {
        frame.marker_width > px(0.)
            && point.x >= frame.bounds.left() + frame.width - frame.marker_width
            && point.x < frame.bounds.left() + frame.width
            && point.y >= frame.input_bounds.top()
            && point.y < frame.input_bounds.bottom()
    })
}
impl IntoElement for Gutter {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}
impl Element for Gutter {
    type RequestLayoutState = ();
    type PrepaintState = ();
    fn id(&self) -> Option<ElementId> {
        None
    }
    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        w: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.body.request_layout(w, cx), ())
    }
    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.body.prepaint(w, cx);
        // Upstream publishes geometry during paint. Never attach an actionable
        // cell to the previous frame after edits, scroll, resize or font changes.
        if self.snapshot.borrow().as_ref() != Some(&self.frame(bounds, w, cx)) {
            return;
        }
        let positions = {
            let state = self.editor.read(cx);
            let Some(height) = state.line_height() else {
                return;
            };
            visible_rows(state)
                .into_iter()
                .map(|(row, bounds)| (row, (bounds.top(), height)))
                .collect::<std::collections::HashMap<_, _>>()
        };
        if let (Some(lane), Some(click)) = (&self.markers, &self.marker_click) {
            let frame = self.frame(bounds, w, cx);
            for (index, rect) in marker_bounds(
                lane,
                bounds.left() + self.width - lane.width,
                self.editor.read(cx),
            ) {
                let click = click.clone();
                let editor = self.editor.clone();
                let snapshot = self.snapshot.clone();
                let painted = frame.clone();
                let mut cell = div()
                    .id(("git-marker", index))
                    .debug_selector(move || format!("git-marker-{index}"))
                    .w(rect.size.width)
                    .h(rect.size.height)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |_, window, cx| {
                        if snapshot.borrow().as_ref() == Some(&painted)
                            && frame_current(&painted, editor.read(cx), window)
                        {
                            click(index, rect, window, cx);
                            cx.stop_propagation();
                        }
                    })
                    .into_any_element();
                cell.prepaint_as_root(
                    rect.origin,
                    size(
                        AvailableSpace::Definite(rect.size.width),
                        AvailableSpace::Definite(rect.size.height),
                    ),
                    w,
                    cx,
                );
                self.marker_cells.push(cell);
            }
        }
        for (row, mut cell) in self.cells.drain(..) {
            let Some((top, height)) = positions.get(&row).copied() else {
                continue;
            };
            cell.prepaint_as_root(
                point(bounds.left(), top),
                size(
                    AvailableSpace::Definite(self.width),
                    AvailableSpace::Definite(height),
                ),
                w,
                cx,
            );
            self.ready.push(cell);
        }
    }
    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        w: &mut Window,
        cx: &mut App,
    ) {
        self.body.paint(w, cx);
        let frame = self.frame(bounds, w, cx);
        let matches = self.snapshot.borrow().as_ref() == Some(&frame);
        if !matches {
            w.request_animation_frame();
        }
        // Numbers are visual metadata, so draw them from the geometry the
        // editor just published. Actionable cells still require a stable frame.
        #[cfg(test)]
        PAINTED_NUMBERS.with(|numbers| numbers.borrow_mut().clear());
        if let Some(height) = self.editor.read(cx).line_height() {
            let current = self.editor.read(cx).cursor_position().line as usize;
            let lane = &self.numbers;
            let mask = ContentMask {
                bounds: Bounds::new(
                    point(bounds.left() + lane.start, frame.input_bounds.top()),
                    size(lane.width, frame.input_bounds.size.height),
                ),
            };
            w.with_content_mask(Some(mask), |w| {
                for (row, position) in &frame.rows {
                    let text = (row + 1).to_string();
                    let line = w.text_system().shape_line(
                        text.clone().into(),
                        lane.size,
                        &[TextRun {
                            len: text.len(),
                            font: lane.font.clone(),
                            color: if *row == current {
                                lane.current
                            } else {
                                lane.normal
                            },
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    let origin = point(
                        bounds.left() + lane.start + lane.width - lane.padding - line.width,
                        position.top(),
                    );
                    if line
                        .paint(origin, height, TextAlign::Left, None, w, cx)
                        .is_ok()
                    {
                        #[cfg(test)]
                        PAINTED_NUMBERS
                            .with(|numbers| numbers.borrow_mut().push((*row, *position)));
                    }
                }
            });
        }
        if let Some(lane) = &self.markers {
            paint_markers(
                lane,
                bounds.left() + self.width - lane.width,
                self.editor.read(cx),
                w,
            );
        }
        *self.snapshot.borrow_mut() = Some(frame);
        if matches {
            for cell in &mut self.marker_cells {
                cell.paint(w, cx);
            }
            for cell in &mut self.ready {
                cell.paint(w, cx);
            }
        }
    }
}

/// Read geometry published by the editor on this very paint, including wrap,
/// folding and scrolling. No copied component padding or row-height arithmetic.
fn frame_current(frame: &Frame, state: &EditorState, window: &Window) -> bool {
    frame.scroll == state.scroll_offset()
        && frame.text == *state.text()
        && frame.viewport == window.viewport_size()
        && frame.scale == window.scale_factor()
        && frame.input_bounds == state.input_bounds()
        && frame.rows == visible_rows(state)
}
fn paint_markers(lane: &MarkerLane, left: Pixels, state: &EditorState, window: &mut Window) {
    for (index, rect) in marker_bounds(lane, left, state) {
        let color = match lane.items[index].kind {
            crate::line_changes::Kind::Added => lane.colors[0],
            crate::line_changes::Kind::Modified => lane.colors[1],
            crate::line_changes::Kind::Deleted => lane.colors[2],
        };
        if lane.staged.get(index).copied().unwrap_or(false) {
            window.paint_quad(gpui::outline(rect, rgb(color), gpui::BorderStyle::Solid));
        } else {
            window.paint_quad(fill(rect, rgb(color)));
        }
    }
}
fn marker_bounds(
    lane: &MarkerLane,
    left: Pixels,
    state: &EditorState,
) -> Vec<(usize, Bounds<Pixels>)> {
    use crate::line_changes::Kind;
    let Some(visible) = state.visible_row_range() else {
        return Vec::new();
    };
    let viewport = state.input_bounds();
    let mut positions = std::collections::BTreeMap::new();
    let mut last_top = None;
    for row in visible.rev() {
        let start = state.text().line_start_offset(row);
        let end = if row + 1 < state.text().lines_len() {
            state.text().line_start_offset(row + 1).saturating_sub(1)
        } else {
            state.text().len()
        };
        let Some(at) = state.range_to_bounds(&(start..start)) else {
            continue;
        };
        if last_top == Some(at.top()) {
            continue;
        }
        last_top = Some(at.top());
        let Some(full) = state.range_to_bounds(&(start..end)) else {
            continue;
        };
        positions.insert(row, full);
    }
    let mut bounds = Vec::new();
    for (index, marker) in lane.items.iter().enumerate() {
        let eof = marker.row >= state.text().lines_len();
        let row = marker.row.min(state.text().lines_len().saturating_sub(1));
        let Some(at) = positions.get(&row) else {
            continue;
        };
        let (top, height) = match marker.kind {
            Kind::Added | Kind::Modified => (at.top(), at.size.height),
            Kind::Deleted => (
                if eof {
                    at.bottom() - lane.deleted
                } else {
                    at.top()
                },
                lane.deleted,
            ),
        };
        let bottom = (top + height).min(viewport.bottom());
        let top = top.max(viewport.top());
        if bottom > top {
            let width = if marker.kind == Kind::Deleted {
                lane.width
            } else {
                lane.stripe
            };
            bounds.push((
                index,
                Bounds::new(point(left, top), size(width, bottom - top)),
            ));
        }
    }
    bounds
}

impl super::FileTab {
    /// Gutter cells are siblings of Input. Forward their real wheel delta using
    /// the retained editor's measured line height and public clamped scroll API.
    pub(super) fn scroll_gutter(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.editor.as_ref() else {
            return;
        };
        editor.update(cx, |state, cx| {
            let height = state.line_height().unwrap_or(window.line_height());
            state.set_scroll_offset(state.scroll_offset() + event.delta.pixel_delta(height), cx);
        });
        cx.stop_propagation();
    }

    /// A deferred component scroll may land during paint after cells prepainted.
    /// Recheck actual public geometry before any stale hitbox can invoke a task.
    pub(super) fn gutter_row_contains(
        &self,
        row: usize,
        point: Point<Pixels>,
        window: &Window,
        cx: &App,
    ) -> bool {
        let Some(editor) = self.editor.as_ref() else {
            return false;
        };
        let state = editor.read(cx);
        let snapshot = self.gutter_snapshot.borrow();
        let Some(frame) = snapshot.as_ref() else {
            return false;
        };
        // A callback can outlive the geometry it was registered against, even
        // before the next render. Reject that interval instead of acting on it.
        if frame.scroll != state.scroll_offset()
            || frame.font != self.look.font_size
            || frame.family != self.look.family
            || frame.viewport != window.viewport_size()
            || frame.scale != window.scale_factor()
            || frame.input_bounds != state.input_bounds()
            || frame.text != *state.text()
        {
            return false;
        }
        let rows = visible_rows(state);
        if frame.rows != rows {
            return false;
        }
        let Some((_, bounds)) = rows.into_iter().find(|(visible, _)| *visible == row) else {
            return false;
        };
        let Some(height) = state.line_height() else {
            return false;
        };
        point.y >= bounds.top() && point.y < bounds.top() + height
    }
}

#[cfg(test)]
mod tests;
