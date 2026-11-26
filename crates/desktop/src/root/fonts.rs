//! Choosing a face: the list, and what taking a row does.
//!
//! M5.9. It was a button that cycled the installed families one click at
//! a time — forty clicks on a machine with forty of them, and the person
//! who wanted the one they had went round again. Since M8.9 the list is a
//! picker, `Holds::Fonts`, hanging from the button that opens it.

use gpui::{Context, Window};

use crate::Root;
use crate::fonts::{self, Which};
use crate::picker::Take;
use crate::root::picker::{Focus, Holds, Scroll};

impl Root {
    /// Open the list for one of the two settings, or close it if it is
    /// the one that is open.
    ///
    /// **The installed families are read here, once**: `all_font_names`
    /// enumerates and sorts every installed face through CoreText, and
    /// M8.1 measured two views calling it per frame at ten times what a
    /// keystroke cost. A list that read it per render would be that
    /// mistake again, in a place where a person is typing.
    pub(crate) fn toggle_font_picker(
        &mut self,
        which: Which,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.font_list_for() == Some(which) {
            self.close_picker(Focus::Back, window, cx);
            return;
        }
        let installed = window.text_system().all_font_names();
        self.open_picker(
            Holds::Fonts(which, installed),
            None,
            Scroll::children(),
            window,
            cx,
        );
    }

    /// Which setting the font list is choosing for, while it is up.
    pub(crate) fn font_list_for(&self) -> Option<Which> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::Fonts(which, _)) => Some(*which),
            _ => None,
        }
    }

    /// The rows the list is showing.
    pub(crate) fn font_rows(&self) -> Vec<fonts::Row> {
        match self.picker.as_ref().map(|open| &open.holds) {
            Some(Holds::Fonts(which, installed)) => {
                fonts::rows(*which, self.picker_query(), installed)
            }
            _ => Vec::new(),
        }
    }

    /// `enter`, and a click on a row.
    pub(crate) fn fonts_take(
        &mut self,
        row: usize,
        _take: Take,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(which) = self.font_list_for() else {
            return;
        };
        let Some(row) = self.font_rows().get(row).cloned() else {
            return;
        };
        self.close_picker(Focus::Back, window, cx);
        self.set_font(which, row.family, cx);
    }
}
