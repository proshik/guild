//! Identifier occurrences, independent of Find, diagnostics and selection.
use std::ops::Range;

use gpui::{App, AppContext, Context, Entity, Subscription};
use gpui_component::Rope;
use gpui_component::input::{
    EditorState, RangeDecoration, RangeDecorationCollection, RangeDecorationStyle,
};

use crate::source_occurrences::{identifier, mentions};

/// Owned by a file tab, for exactly the lifetime of its current editor.
/// Notifications cover clicks, keys and edits; FileTab also synchronizes reloads.
/// Decoration-only notifications and caret blinking do no text scan.
pub struct Occurrences {
    snapshot: Option<Rope>,
    text: String,
    selection: Range<usize>,
    any_selection: bool,
    word: Option<String>,
    decorations: RangeDecorationCollection,
    color: gpui::Rgba,
    _observe: Subscription,
}

impl Occurrences {
    pub fn attach(
        editor: &Entity<EditorState>,
        theme: crate::theme::Theme,
        cx: &mut App,
    ) -> Entity<Self> {
        let decorations = editor.update(cx, |editor, cx| {
            editor.create_range_decorations_collection(Vec::new(), cx)
        });
        cx.new(|cx: &mut Context<Self>| {
            cx.on_release(|this, cx| this.decorations.dispose(cx))
                .detach();
            let mut this = Self {
                snapshot: None,
                text: String::new(),
                selection: usize::MAX..usize::MAX,
                any_selection: false,
                word: None,
                decorations,
                color: theme.identifier_occurrence(),
                _observe: cx.observe(editor, |this, editor, cx| this.refresh(&editor, cx)),
            };
            this.refresh(editor, cx);
            this
        })
    }

    pub(crate) fn refresh(&mut self, editor: &Entity<EditorState>, cx: &mut Context<Self>) {
        let state = editor.read(cx);
        // O(1) identity comparison of an unmodified rope clone; unlike Change,
        // this also sees set_value/reloads. No flattening on render or blinking.
        let changed = self.snapshot.as_ref().is_none_or(|snapshot| {
            !ropey::extra::esoterica::ropes_are_instances(snapshot, state.text())
        });
        let selection = state.selected_range();
        // File editors are never masked: is_copyable includes every cursor's
        // selection, whereas selected_range reports only the active cursor.
        let any_selection = state.is_copyable();
        if !changed && selection == self.selection && any_selection == self.any_selection {
            return;
        }
        if changed {
            self.snapshot = Some(state.text().clone());
            // Check the rope's byte length BEFORE flattening. Supported files
            // above the existing syntax limit must not allocate/scour a copy.
            self.text = if state.text().len() as u64 <= crate::file_tab::HIGHLIGHT_LIMIT {
                state.value().to_string()
            } else {
                String::new()
            };
        }
        self.selection = selection.clone();
        self.any_selection = any_selection;
        let word = (!any_selection && selection.is_empty())
            .then(|| identifier(&self.text, selection.start))
            .flatten();
        if !changed && word == self.word.as_deref() {
            return;
        }
        let ranges = word
            .map(|word| mentions(&self.text, word))
            .unwrap_or_default();
        self.word = word.map(str::to_owned);
        self.paint(ranges, cx);
    }

    /// Follow the file's palette without touching caret, text or undo history.
    pub fn restyle(&mut self, theme: crate::theme::Theme, cx: &mut Context<Self>) {
        let color = theme.identifier_occurrence();
        if color != self.color {
            self.color = color;
            self.paint(self.ranges(cx), cx);
        }
    }

    fn paint(&self, ranges: Vec<Range<usize>>, cx: &mut App) {
        self.decorations.set(
            ranges
                .into_iter()
                .map(|range| {
                    RangeDecoration::new(range)
                        .with_style(RangeDecorationStyle::Fill)
                        .with_color(self.color.into())
                })
                .collect(),
            cx,
        );
    }

    /// The same UTF-8 ranges supplied to the editor's geometric paint layer.
    pub fn ranges(&self, cx: &App) -> Vec<Range<usize>> {
        self.decorations.get_ranges(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    // Private storage checks guard the allocation boundary without exporting
    // instrumentation. The initial space keeps the unfixed test safe: it fails
    // before moving into eight million occurrences and allocating decorations.
    #[gpui::test]
    fn supported_large_rope_is_never_flattened_for_occurrences(cx: &mut TestAppContext) {
        let mut source = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let text = format!(" {}", "a;".repeat(8_000_000));
            assert!((text.len() as u64) < crate::file_tab::OPEN_LIMIT);
            let editor = cx.new(|cx| EditorState::new(window, cx).default_value(text));
            let occurrences = Occurrences::attach(&editor, crate::theme::Theme::default(), cx);
            source = Some((editor, occurrences));
            gpui::Empty
        });
        let (editor, occurrences) = source.unwrap();
        cx.update(|_, cx| {
            assert!(
                occurrences.read(cx).text.is_empty(),
                "do not flatten above HIGHLIGHT_LIMIT"
            );
            assert!(occurrences.read(cx).ranges(cx).is_empty());
            editor.update(cx, |editor, cx| editor.set_selected_range(1..1, cx));
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(occurrences.read(cx).text.is_empty());
            assert!(occurrences.read(cx).ranges(cx).is_empty());
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| editor.set_value("name name", window, cx))
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(occurrences.read(cx).ranges(cx), vec![0..4, 5..9]));
    }
}
