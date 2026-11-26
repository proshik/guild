//! Forwarding surface for the pinned public VTE Handler. Keep every method forwarded.
use super::*;
impl<H: Handler, F: FnMut(&mut H, Semantic)> Handler for Forward<'_, H, F> {
    fn set_title(&mut self, a0: Option<String>) {
        if let Some(key) = a0.as_ref()
            && let Some((created, event)) = self
                .pending
                .remove(key)
                .or_else(|| self.status_pending.remove(key))
        {
            // A status report has no screen position to go stale.
            if created.elapsed() < std::time::Duration::from_secs(1)
                || matches!(event, Semantic::ProgramStatus(_))
            {
                (self.semantic)(self.handler, event);
            } else {
                // A prompt's position is stale, but that a prompt began is not.
                let prompt = matches!(&event, Semantic::Command(p) if p.starts_with('A'));
                (self.semantic)(self.handler, Semantic::Revoke);
                if prompt {
                    (self.semantic)(self.handler, Semantic::PromptStarted);
                }
            }
            return;
        }
        self.handler.set_title(a0);
    }
    fn set_cursor_style(&mut self, a0: Option<CursorStyle>) {
        self.handler.set_cursor_style(a0);
    }
    fn set_cursor_shape(&mut self, a0: CursorShape) {
        self.handler.set_cursor_shape(a0);
    }
    fn input(&mut self, a0: char) {
        (self.semantic)(self.handler, Semantic::BeforeInput);
        self.handler.input(a0);
        (self.semantic)(self.handler, Semantic::AfterInput);
    }
    fn goto(&mut self, a0: i32, a1: usize) {
        self.handler.goto(a0, a1);
    }
    fn goto_line(&mut self, a0: i32) {
        self.handler.goto_line(a0);
    }
    fn goto_col(&mut self, a0: usize) {
        self.handler.goto_col(a0);
    }
    fn insert_blank(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::ClearLine);
        self.handler.insert_blank(a0);
    }
    fn move_up(&mut self, a0: usize) {
        self.handler.move_up(a0);
    }
    fn move_down(&mut self, a0: usize) {
        self.handler.move_down(a0);
    }
    fn identify_terminal(&mut self, a0: Option<char>) {
        self.handler.identify_terminal(a0);
    }
    fn device_status(&mut self, a0: usize) {
        self.handler.device_status(a0);
    }
    fn move_forward(&mut self, a0: usize) {
        self.handler.move_forward(a0);
    }
    fn move_backward(&mut self, a0: usize) {
        self.handler.move_backward(a0);
    }
    fn move_down_and_cr(&mut self, a0: usize) {
        self.handler.move_down_and_cr(a0);
    }
    fn move_up_and_cr(&mut self, a0: usize) {
        self.handler.move_up_and_cr(a0);
    }
    fn put_tab(&mut self, a0: u16) {
        self.handler.put_tab(a0);
    }
    fn backspace(&mut self) {
        self.handler.backspace();
    }
    fn carriage_return(&mut self) {
        self.handler.carriage_return();
    }
    fn linefeed(&mut self) {
        (self.semantic)(self.handler, Semantic::BeforeLinefeed);
        self.handler.linefeed();
        (self.semantic)(self.handler, Semantic::AfterLinefeed);
    }
    fn bell(&mut self) {
        self.handler.bell();
    }
    fn substitute(&mut self) {
        self.handler.substitute();
    }
    fn newline(&mut self) {
        (self.semantic)(self.handler, Semantic::BeforeLinefeed);
        self.handler.newline();
        (self.semantic)(self.handler, Semantic::AfterLinefeed);
    }
    fn set_horizontal_tabstop(&mut self) {
        self.handler.set_horizontal_tabstop();
    }
    fn scroll_up(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.scroll_up(a0);
    }
    fn scroll_down(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.scroll_down(a0);
    }
    fn insert_blank_lines(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.insert_blank_lines(a0);
    }
    fn delete_lines(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.delete_lines(a0);
    }
    fn erase_chars(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::ClearLine);
        self.handler.erase_chars(a0);
    }
    fn delete_chars(&mut self, a0: usize) {
        (self.semantic)(self.handler, Semantic::ClearLine);
        self.handler.delete_chars(a0);
    }
    fn move_backward_tabs(&mut self, a0: u16) {
        self.handler.move_backward_tabs(a0);
    }
    fn move_forward_tabs(&mut self, a0: u16) {
        self.handler.move_forward_tabs(a0);
    }
    fn save_cursor_position(&mut self) {
        self.handler.save_cursor_position();
    }
    fn restore_cursor_position(&mut self) {
        self.handler.restore_cursor_position();
    }
    fn clear_line(&mut self, a0: LineClearMode) {
        (self.semantic)(self.handler, Semantic::ClearLine);
        self.handler.clear_line(a0);
    }
    fn clear_screen(&mut self, a0: ClearMode) {
        (self.semantic)(
            self.handler,
            match &a0 {
                ClearMode::Below => Semantic::ClearBelow,
                ClearMode::Above => Semantic::ClearAbove,
                _ => Semantic::Invalidate,
            },
        );
        self.handler.clear_screen(a0);
    }
    fn clear_tabs(&mut self, a0: TabulationClearMode) {
        self.handler.clear_tabs(a0);
    }
    fn set_tabs(&mut self, a0: u16) {
        self.handler.set_tabs(a0);
    }
    fn reset_state(&mut self) {
        (self.semantic)(self.handler, Semantic::Reset);
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.reset_state();
        (self.semantic)(self.handler, Semantic::Region(1, None));
    }
    fn reverse_index(&mut self) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.reverse_index();
    }
    fn terminal_attribute(&mut self, a0: Attr) {
        self.handler.terminal_attribute(a0);
    }
    fn set_mode(&mut self, a0: Mode) {
        self.handler.set_mode(a0);
    }
    fn unset_mode(&mut self, a0: Mode) {
        self.handler.unset_mode(a0);
    }
    fn report_mode(&mut self, a0: Mode) {
        self.handler.report_mode(a0);
    }
    fn set_private_mode(&mut self, a0: PrivateMode) {
        self.handler.set_private_mode(a0);
        (self.semantic)(self.handler, Semantic::CheckScreen);
    }
    fn unset_private_mode(&mut self, a0: PrivateMode) {
        self.handler.unset_private_mode(a0);
        (self.semantic)(self.handler, Semantic::CheckScreen);
    }
    fn report_private_mode(&mut self, a0: PrivateMode) {
        self.handler.report_private_mode(a0);
    }
    fn set_scrolling_region(&mut self, a0: usize, a1: Option<usize>) {
        self.handler.set_scrolling_region(a0, a1);
        (self.semantic)(self.handler, Semantic::Region(a0, a1));
    }
    fn set_keypad_application_mode(&mut self) {
        self.handler.set_keypad_application_mode();
    }
    fn unset_keypad_application_mode(&mut self) {
        self.handler.unset_keypad_application_mode();
    }
    fn set_active_charset(&mut self, a0: CharsetIndex) {
        self.handler.set_active_charset(a0);
    }
    fn configure_charset(&mut self, a0: CharsetIndex, a1: StandardCharset) {
        self.handler.configure_charset(a0, a1);
    }
    fn set_color(&mut self, a0: usize, a1: Rgb) {
        self.handler.set_color(a0, a1);
    }
    fn dynamic_color_sequence(&mut self, a0: String, a1: usize, a2: &str) {
        self.handler.dynamic_color_sequence(a0, a1, a2);
    }
    fn reset_color(&mut self, a0: usize) {
        self.handler.reset_color(a0);
    }
    fn clipboard_store(&mut self, a0: u8, a1: &[u8]) {
        self.handler.clipboard_store(a0, a1);
    }
    fn clipboard_load(&mut self, a0: u8, a1: &str) {
        self.handler.clipboard_load(a0, a1);
    }
    fn decaln(&mut self) {
        (self.semantic)(self.handler, Semantic::Invalidate);
        self.handler.decaln();
    }
    fn push_title(&mut self) {
        self.handler.push_title();
    }
    fn pop_title(&mut self) {
        self.handler.pop_title();
    }
    fn text_area_size_pixels(&mut self) {
        self.handler.text_area_size_pixels();
    }
    fn text_area_size_chars(&mut self) {
        self.handler.text_area_size_chars();
    }
    fn set_mouse_cursor_icon(&mut self, icon: cursor_icon::CursorIcon) {
        self.handler.set_mouse_cursor_icon(icon);
    }
    fn set_hyperlink(&mut self, a0: Option<Hyperlink>) {
        self.handler.set_hyperlink(a0);
    }
    fn report_keyboard_mode(&mut self) {
        self.handler.report_keyboard_mode();
    }
    fn push_keyboard_mode(&mut self, a0: KeyboardModes) {
        self.handler.push_keyboard_mode(a0);
    }
    fn pop_keyboard_modes(&mut self, a0: u16) {
        self.handler.pop_keyboard_modes(a0);
    }
    fn set_keyboard_mode(&mut self, a0: KeyboardModes, a1: KeyboardModesApplyBehavior) {
        self.handler.set_keyboard_mode(a0, a1);
    }
    fn set_modify_other_keys(&mut self, a0: ModifyOtherKeys) {
        self.handler.set_modify_other_keys(a0);
    }
    fn report_modify_other_keys(&mut self) {
        self.handler.report_modify_other_keys();
    }
    fn set_scp(&mut self, a0: ScpCharPath, a1: ScpUpdateMode) {
        self.handler.set_scp(a0, a1);
    }
}
