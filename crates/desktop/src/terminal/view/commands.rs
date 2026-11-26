//! Shell boundary actions never send command text to the child.
use super::TerminalTab;
use gpui::{ClipboardItem, Context};
impl TerminalTab {
    pub(crate) fn command_status(&self) -> (bool, bool, bool, Option<i32>) {
        self.emulator.command_context(self.command_point)
    }
    pub(crate) fn navigate_command(&mut self, older: bool, cx: &mut Context<Self>) -> bool {
        if !self.emulator.navigate_command(older) {
            return false;
        }
        self.scrolled_at = Some(std::time::Instant::now());
        self.scroll.clear();
        cx.notify();
        true
    }
    pub(crate) fn command_output(&mut self, copy: bool, cx: &mut Context<Self>) {
        if !self.emulator.select_command_output(self.command_point) {
            return;
        }
        if copy && let Some(text) = self.emulator.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        cx.notify();
    }
}
