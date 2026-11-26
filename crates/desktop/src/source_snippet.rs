//! Interactive source text that leaves focus and row activation with its host.
use gpui::{
    App, Context, ElementId, Entity, HighlightStyle, MouseButton, StyledText, Window, div,
    prelude::*,
};
use std::{ops::Range, sync::Arc};

pub struct SourceSnapshot {
    text: Arc<str>,
    ranges: Vec<Range<usize>>,
    down: Option<gpui::Point<gpui::Pixels>>,
}
impl SourceSnapshot {
    pub fn new(text: impl Into<Arc<str>>) -> Self {
        Self {
            text: text.into(),
            ranges: Vec::new(),
            down: None,
        }
    }
    /// Assemble only a bounded displayed snapshot; never flatten large blame documents.
    pub fn from_lines<'a>(lines: impl Iterator<Item = &'a str> + Clone) -> (Self, Vec<usize>) {
        let bytes = lines.clone().try_fold(0usize, |n, line| {
            n.checked_add(line.len() + 1)
                .filter(|n| *n as u64 <= crate::file_tab::HIGHLIGHT_LIMIT)
        });
        if bytes.is_none_or(|n| n as u64 > crate::file_tab::HIGHLIGHT_LIMIT) {
            return (Self::new(""), Vec::new());
        }
        let mut text = String::with_capacity(bytes.unwrap());
        let mut offsets = Vec::new();
        for line in lines {
            offsets.push(text.len());
            text.push_str(line);
            text.push('\n');
        }
        (Self::new(text), offsets)
    }
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.ranges
    }
    fn click(&mut self, at: Option<usize>, cx: &mut Context<Self>) {
        self.ranges = at
            .filter(|_| self.text.len() as u64 <= crate::file_tab::HIGHLIGHT_LIMIT)
            .and_then(|at| crate::source_occurrences::identifier(&self.text, at))
            .map(|word| crate::source_occurrences::mentions(&self.text, word))
            .unwrap_or_default();
        cx.notify();
    }
}

struct Link {
    source: Entity<SourceSnapshot>,
    identity: Option<gpui::SharedString>,
    _observe: gpui::Subscription,
}
impl Link {
    fn new(
        source: Entity<SourceSnapshot>,
        identity: Option<gpui::SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe = cx.observe(&source, |_, _, cx| cx.notify());
        Self {
            source,
            identity,
            _observe: observe,
        }
    }
}
#[derive(IntoElement)]
pub struct SourceSnippet {
    id: ElementId,
    text: String,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    theme: crate::theme::Theme,
    source: Option<(Entity<SourceSnapshot>, usize)>,
    identity: Option<gpui::SharedString>,
}
impl SourceSnippet {
    /// One displayed source line, with ordered disjoint syntax/search runs.
    pub fn new(
        id: impl Into<ElementId>,
        text: impl Into<String>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
        theme: crate::theme::Theme,
    ) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            highlights,
            theme,
            source: None,
            identity: None,
        }
    }
    /// The host's project/origin/file and displayed result generation.
    /// Equal text from a different source must not inherit a previous click.
    pub fn identity(mut self, identity: impl Into<gpui::SharedString>) -> Self {
        self.identity = Some(identity.into());
        self
    }
    pub fn source(mut self, source: &Entity<SourceSnapshot>, offset: usize) -> Self {
        self.source = Some((source.clone(), offset));
        self
    }
}
impl RenderOnce for SourceSnippet {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let link = window.use_keyed_state(self.id.clone(), cx, |_, cx| {
            let source = self
                .source
                .as_ref()
                .map(|(s, _)| s.clone())
                .unwrap_or_else(|| cx.new(|_| SourceSnapshot::new(self.text.clone())));
            Link::new(source, self.identity.clone(), cx)
        });
        link.update(cx, |link, cx| {
            if let Some((source, _)) = &self.source {
                if link.source != *source {
                    *link = Link::new(source.clone(), self.identity.clone(), cx);
                }
            } else {
                let replaced = link.identity != self.identity;
                link.identity = self.identity.clone();
                link.source.update(cx, |state, _| {
                    if replaced || state.text.as_ref() != self.text {
                        *state = SourceSnapshot::new(self.text.clone());
                    }
                });
            }
        });
        let source = link.read(cx).source.clone();
        let offset = self.source.as_ref().map_or(0, |(_, offset)| *offset);
        let ranges = crate::source_occurrences::within(
            source.read(cx).ranges(),
            &(offset..offset + self.text.len()),
        );
        // A single disjoint style stream retains syntax and search foregrounds.
        let mut cuts = vec![0, self.text.len()];
        for range in self.highlights.iter().map(|(r, _)| r).chain(&ranges) {
            cuts.extend([
                range.start.min(self.text.len()),
                range.end.min(self.text.len()),
            ]);
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut syntax_index = 0;
        let mut occurrence_index = 0;
        let highlights = cuts
            .windows(2)
            .filter_map(|pair| {
                let range = pair[0]..pair[1];
                if range.is_empty() {
                    return None;
                }
                // Both inputs are ordered disjoint runs. Merge once rather
                // than rescanning every syntax span for each visible segment.
                while self
                    .highlights
                    .get(syntax_index)
                    .is_some_and(|(r, _)| r.end <= range.start)
                {
                    syntax_index += 1;
                }
                while ranges
                    .get(occurrence_index)
                    .is_some_and(|r| r.end <= range.start)
                {
                    occurrence_index += 1;
                }
                let mut style = self
                    .highlights
                    .get(syntax_index)
                    .filter(|(r, _)| r.start <= range.start && range.end <= r.end)
                    .map(|(_, s)| *s)
                    .unwrap_or_default();
                if ranges
                    .get(occurrence_index)
                    .is_some_and(|r| r.start <= range.start && range.end <= r.end)
                    && style.background_color.is_none()
                {
                    style.background_color = Some(self.theme.identifier_occurrence().into());
                }
                Some((range, style))
            })
            .collect::<Vec<_>>();
        let text = StyledText::new(self.text).with_highlights(highlights);
        let layout = text.layout().clone();
        let down = source.clone();
        let moved = source.downgrade();
        div()
            .relative()
            .w_full()
            .whitespace_nowrap()
            .debug_selector(|| "source-snippet".into())
            .id(self.id)
            .child(text)
            .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                down.update(cx, |state, cx| {
                    state.down = Some(event.position);
                    let at = (event.click_count == 1 && !event.modifiers.shift)
                        .then(|| {
                            layout
                                .index_for_position(event.position)
                                .ok()
                                .map(|at| offset + at)
                        })
                        .flatten();
                    state.click(at, cx);
                });
                // Never consume the host's click, activation or focus gesture.
            })
            .child(
                gpui::canvas(
                    |_, _, _| (),
                    move |_, _, window, _| {
                        let movement = moved.clone();
                        window.on_mouse_event(move |event: &gpui::MouseMoveEvent, phase, _, cx| {
                            if phase != gpui::DispatchPhase::Capture {
                                return;
                            }
                            let _ = movement.update(cx, |state, cx| {
                                if event.pressed_button != Some(MouseButton::Left) {
                                    state.down = None;
                                } else if state
                                    .down
                                    .is_some_and(|p| (event.position - p).magnitude() > 3.)
                                    && !state.ranges.is_empty()
                                {
                                    state.click(None, cx);
                                }
                            });
                        });
                        let released = moved.clone();
                        window.on_mouse_event(move |_: &gpui::MouseUpEvent, phase, _, cx| {
                            if phase == gpui::DispatchPhase::Capture {
                                let _ = released.update(cx, |state, _| state.down = None);
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Private retained storage is the acceptance boundary: a skipped annotation
    // must not first assemble the entire large blame/excerpt document.
    #[test]
    fn source_snapshot_assembly_stops_before_flattening_large_documents() {
        let line = "a;".repeat(200);
        let (source, offsets) =
            SourceSnapshot::from_lines(std::iter::repeat_n(line.as_str(), 40_000));
        assert!(source.text.is_empty());
        assert!(offsets.is_empty());
        let (source, offsets) = SourceSnapshot::from_lines(["имя", "имя"].into_iter());
        assert_eq!(source.text.as_ref(), "имя\nимя\n");
        assert_eq!(offsets, vec![0, 7]);
    }
}
