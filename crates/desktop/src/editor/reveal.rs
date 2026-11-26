//! Guild jump behavior implemented with upstream layout and scroll APIs.
use gpui::{Context, Window, px};
use gpui_component::input::{EditorState, Position};

/// A jump keeps an already visible row still and centers an off-screen row.
pub trait EditorReveal {
    /// Move the caret now, then reveal it after the editor has painted its layout.
    fn set_cursor_position_centred(
        &mut self,
        position: Position,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) where
        Self: Sized;
}

impl EditorReveal for EditorState {
    fn set_cursor_position_centred(
        &mut self,
        position: Position,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self.line_height().map(|_| self.scroll_offset());
        self.set_cursor_position(position, window, cx);
        let target = self.cursor();
        let editor = cx.weak_entity();
        // Frame callbacks run before drawing. The second callback sees the
        // layout produced by the first frame, including a newly opened editor.
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |_, cx| {
                let _ = editor.update(cx, |state, cx| {
                    // A newer caret movement supersedes this reveal.
                    if state.cursor() != target && state.selected_range().start != target {
                        return;
                    }
                    let Some((cursor, line)) = state.cursor_layout() else {
                        return;
                    };
                    let viewport = state.input_bounds();
                    let mut scroll = state.scroll_offset();
                    let row_top = cursor.top() - viewport.top() - (line - cursor.size.height) / 2.;
                    let was_visible = previous.is_some_and(|old| {
                        row_top + old.y >= px(0.) && row_top + old.y + line <= viewport.size.height
                    });
                    scroll.y = if was_visible {
                        previous.expect("visible requires previous layout").y
                    } else {
                        -(row_top - viewport.size.height / 2. + line / 2.).max(px(0.))
                    };
                    // Upstream clamps the deferred offset to the actual content.
                    state.set_scroll_offset(scroll, cx);
                });
            });
        });
    }
}
