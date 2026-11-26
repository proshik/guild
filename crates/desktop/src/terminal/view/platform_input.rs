//! The terminal's platform text client. Only the short preedit is editable;
//! terminal output is never exposed as an editable document.
use super::TerminalTab;
use gpui::{Bounds, Context, EntityInputHandler, Pixels, UTF16Selection, Window};
use std::ops::Range;

#[derive(Default)]
pub(super) struct Composition {
    pub text: String,
    pub active: bool,
    pub selection: Range<usize>,
    pub caret: Option<Bounds<Pixels>>,
}
impl Composition {
    fn units(&self) -> usize {
        self.text.encode_utf16().count()
    }
    fn substring(&self, range: Range<usize>) -> Option<String> {
        let units: Vec<_> = self.text.encode_utf16().collect();
        String::from_utf16(units.get(range)?).ok()
    }
    fn replace(&mut self, range: Option<Range<usize>>, text: &str, selected: Option<Range<usize>>) {
        let mut units: Vec<_> = self.text.encode_utf16().collect();
        let range = range.unwrap_or(0..units.len());
        if range.start > range.end || range.end > units.len() {
            return;
        }
        units.splice(range.clone(), text.encode_utf16());
        let Ok(value) = String::from_utf16(&units) else {
            return;
        };
        self.text = value;
        self.active = true;
        let selection =
            selected.unwrap_or(text.encode_utf16().count()..text.encode_utf16().count());
        self.selection = (range.start + selection.start).min(units.len())
            ..(range.start + selection.end).min(units.len());
    }
    pub fn clear(&mut self) {
        self.text.clear();
        self.active = false;
        self.selection = 0..0;
    }
}
impl TerminalTab {
    /// Drop the composition — the one way it is dropped outside a paint —
    /// and ask for the frame without it when one was on the screen: a
    /// cached terminal otherwise replays the underlined text, whether the
    /// program took what was committed or refused it. Answers its text.
    fn clear_composition(&mut self, cx: &mut Context<Self>) -> String {
        let shown = self.composition.active || !self.composition.text.is_empty();
        let text = std::mem::take(&mut self.composition.text);
        self.composition.clear();
        if shown {
            cx.notify();
        }
        text
    }

    pub(super) fn cancel_platform_composition(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_meta = None;
        if !self.composition.active {
            return;
        }
        self.clear_composition(cx);
        self.discarding_composition = true;
        let entity = cx.entity();
        let handle = window.window_handle();
        // Deferred effects run before the next input frame installs a new
        // handler. Native callbacks can therefore see only the old client,
        // whose focus/teardown gates refuse insertion.
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                crate::system::discard_terminal_marked_text(window);
                entity.update(cx, |tab, _| tab.discarding_composition = false);
            });
        });
    }

    pub(super) fn commit_before(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> bool {
        if !self.composition.active {
            return false;
        }
        let text = self.clear_composition(cx);
        self.pending_meta = None;
        self.platform_insert(&text, cx);
        self.discarding_composition = true;
        let entity = cx.entity();
        let handle = window.window_handle();
        cx.defer(move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                if entity.read(cx).focus.is_focused(window) {
                    crate::system::discard_terminal_marked_text(window);
                }
                entity.update(cx, |tab, cx| {
                    tab.discarding_composition = false;
                    then(tab, window, cx);
                });
            });
        });
        true
    }
    pub(crate) fn paste_with_input(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_before(window, cx, |tab, _, cx| tab.paste(cx)) {
            return;
        }
        self.paste(cx);
    }
    pub(super) fn send_pending_meta(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(keystroke) = self.pending_meta.take() else {
            return false;
        };
        self.dispatch_meta = true;
        self.on_key(
            &gpui::KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            },
            cx,
        );
        self.dispatch_meta = false;
        true
    }
    pub(super) fn platform_insert(&mut self, text: &str, cx: &mut Context<Self>) {
        if text.is_empty() {
            return;
        }
        self.dismiss_restore(cx);
        if !self.send_visible(text.as_bytes(), cx) {
            return;
        }
        self.raw_input();
        self.prompt_input = true;
        self.draft_input = true;
        self.emulator.clear_selection();
        self.emulator.scroll_to_bottom();
        self.scroll.clear();
        self.caret.typed(std::time::Instant::now());
        cx.notify();
    }
    pub(super) fn paint_composition(
        &self,
        family: &str,
        font_size: Pixels,
        cell: gpui::Size<Pixels>,
        window: &mut Window,
        cx: &mut gpui::App,
    ) {
        if !self.focus.is_focused(window) || self.composition.text.is_empty() {
            return;
        }
        let Some(at) = self.composition.caret else {
            return;
        };
        let run = gpui::TextRun {
            len: self.composition.text.len(),
            font: gpui::font(family),
            color: self.palette.cursor(),
            background_color: Some(self.palette.background()),
            underline: Some(gpui::UnderlineStyle {
                thickness: gpui::px(1.),
                color: None,
                wavy: false,
            }),
            strikethrough: None,
        };
        let line = window.text_system().shape_line(
            self.composition.text.clone().into(),
            font_size,
            &[run],
            None,
        );
        if let Some(bounds) = self.bounds {
            window.with_content_mask(
                Some(gpui::ContentMask {
                    bounds: crate::terminal::geometry::content(bounds),
                }),
                |window| {
                    let _ = line.paint(
                        at.origin,
                        cell.height,
                        gpui::TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                },
            );
        }
    }
}
impl EntityInputHandler for TerminalTab {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let text = self.composition.substring(range.clone())?;
        *adjusted = Some(range);
        Some(text)
    }
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        self.focus.is_focused(window).then(|| UTF16Selection {
            range: self.composition.selection.clone(),
            reversed: false,
        })
    }
    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.composition.active.then(|| 0..self.composition.units())
    }
    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_composition(cx);
        self.pending_meta = None;
    }
    fn replace_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.discarding_composition {
            return;
        }
        if !self.focus.is_focused(window) {
            self.clear_composition(cx);
            return;
        }
        let composing = self.composition.active;
        self.clear_composition(cx);
        if !composing && self.send_pending_meta(cx) {
            return;
        }
        self.pending_meta = None;
        self.platform_insert(text, cx);
    }
    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.discarding_composition || !self.focus.is_focused(window) {
            return;
        }
        if text.len() > crate::terminal::emulator::INPUT_BYTES {
            return;
        }
        self.pending_meta = None;
        let previous = self.composition.text.clone();
        let previous_selection = self.composition.selection.clone();
        self.composition.replace(range, text, selected);
        if self.composition.text.len() > crate::terminal::emulator::INPUT_BYTES {
            self.composition.text = previous;
            self.composition.selection = previous_selection;
            return;
        }
        self.raw_input();
        self.prompt_input = true;
        self.draft_input = true;
        cx.notify();
    }
    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let mut bounds = self.composition.caret?;
        if self.composition.text.is_empty() {
            return Some(bounds);
        }
        let start = self.composition.substring(0..range.start)?.len();
        let end = self.composition.substring(0..range.end)?.len();
        let family = self.font_family(window);
        let run = gpui::TextRun {
            len: self.composition.text.len(),
            font: gpui::font(family),
            color: self.palette.cursor(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = window.text_system().shape_line(
            self.composition.text.clone().into(),
            gpui::px(self.font_size),
            &[run],
            None,
        );
        let x0 = line.x_for_index(start);
        let x1 = line.x_for_index(end);
        bounds.origin.x += x0;
        bounds.size.width = (x1 - x0).max(gpui::px(1.));
        if let Some(viewport) = self.bounds {
            let content = crate::terminal::geometry::content(viewport);
            bounds.origin.x = bounds.origin.x.min(content.right() - gpui::px(1.));
            bounds.size.width = bounds.size.width.min(content.right() - bounds.origin.x);
        }
        Some(bounds)
    }
    fn character_index_for_point(
        &mut self,
        _: gpui::Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        Some(self.composition.selection.end)
    }
    fn accepts_text_input(&self, window: &mut Window, _: &mut Context<Self>) -> bool {
        self.focus.is_focused(window) && !self.facts.exited()
    }
    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.composition.units())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn marked_ranges_use_utf16_and_replacement_does_not_commit() {
        let mut c = Composition::default();
        c.replace(None, "に😀", Some(1..3));
        assert_eq!(c.units(), 3);
        assert_eq!(c.substring(1..3).as_deref(), Some("😀"));
        assert!(c.substring(1..2).is_none());
        c.replace(Some(1..3), "ほん", Some(2..2));
        assert_eq!(c.text, "にほん");
        assert_eq!(c.selection, 3..3);
        c.clear();
        c.clear();
        assert_eq!(c.units(), 0);
        assert_eq!(c.selection, 0..0);
    }
}

#[cfg(test)]
mod platform_tests {
    use gpui::EntityInputHandler;
    #[gpui::test]
    fn platform_composition_commits_once_and_rejects_background_insert(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.executor().allow_parking();
        let mut terminal = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            crate::editor::init(cx);
            let tab = super::super::open_command(
                portable_pty::CommandBuilder::new("/bin/cat"),
                std::path::Path::new("/"),
                super::super::Look::of(&Default::default(), Default::default()),
                None,
                cx,
            )
            .unwrap();
            terminal = Some(tab.clone());
            gpui_component::Root::new(tab, window, cx)
        });
        let terminal = terminal.unwrap();
        cx.update(|window, cx| {
            terminal.update(cx, |tab, cx| {
                window.focus(&tab.focus, cx);
                tab.replace_and_mark_text_in_range(None, "に😀", Some(1..3), window, cx);
                assert_eq!(tab.marked_text_range(window, cx), Some(0..3));
                assert_eq!(
                    tab.selected_text_range(false, window, cx).unwrap().range,
                    1..3
                );
                tab.composition.caret = Some(gpui::Bounds::new(
                    gpui::point(gpui::px(20.), gpui::px(30.)),
                    gpui::size(gpui::px(8.), gpui::px(16.)),
                ));
                let start = tab
                    .bounds_for_range(0..0, Default::default(), window, cx)
                    .unwrap();
                let selected = tab
                    .bounds_for_range(1..3, Default::default(), window, cx)
                    .unwrap();
                assert!(selected.origin.x > start.origin.x);
                assert!(selected.size.width > gpui::px(1.));
                let before = tab.input_epoch;
                tab.replace_text_in_range(None, "日本😀", window, cx);
                tab.unmark_text(window, cx);
                assert_eq!(
                    tab.input_epoch,
                    before + 1,
                    "unmark must not send a second commit"
                );
                assert!(tab.marked_text_range(window, cx).is_none());
                let other = cx.focus_handle();
                window.focus(&other, cx);
                let before = tab.input_epoch;
                tab.replace_text_in_range(None, "background", window, cx);
                assert_eq!(tab.input_epoch, before, "background callback must not type");
                window.focus(&tab.focus, cx);
                let mut stroke = gpui::Keystroke::parse("alt-b").unwrap();
                stroke.key_char = Some("∫".into());
                tab.on_key(
                    &gpui::KeyDownEvent {
                        keystroke: stroke,
                        is_held: false,
                        prefer_character_input: false,
                    },
                    cx,
                );
                let before = tab.input_epoch;
                tab.replace_and_mark_text_in_range(None, "´", Some(1..1), window, cx);
                tab.replace_text_in_range(None, "é", window, cx);
                assert_eq!(
                    tab.input_epoch,
                    before + 2,
                    "dead-key preedit must supersede pending meta encoding"
                );
                tab.emulator.hang_up_for_quit();
            })
        });
    }
}
