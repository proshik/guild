//! A quit that comes from outside the window — the Dock's *Quit*, a
//! logout, a restart — asks the questions `cmd-q` asks. M5.11's
//! decision 20.
//!
//! AppKit's half is in [`crate::system`]: `applicationShouldTerminate:`
//! reads whether anything is at stake and, when something is, cancels and
//! leaves a request. This half runs on the watch loop's clock, which ticks
//! whether the window paints or not — a value written by `render` would go
//! stale in a window lying under others, and a session started meanwhile
//! would be quit from the Dock without a word.

use gpui::{Context, Window};

use crate::Root;
use crate::system;

impl Root {
    /// Tell AppKit's side whether a quit has anything at stake, and ask
    /// the window's questions for a quit AppKit cancelled. Called on every
    /// tick of the watch loop.
    ///
    /// Asked from inside `Root`'s own update, with the window in hand,
    /// which is where `may_close_window` runs for the close button too.
    /// `crate::quit` needs no `defer` here: GPUI's `Platform::quit` sends
    /// `terminate:` asynchronously.
    pub(crate) fn settle_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        system::set_quit_needs_asking(self.quit_needs_asking(cx));
        if !system::take_quit_request() {
            return;
        }
        // **A question on the screen is already the answer.** The menu
        // stays live under a sheet, and a second Quit from the Dock would
        // otherwise queue a second sheet. `asking` covers the window's own
        // questions — the unsaved files and, since M5.11's merge, the
        // running sessions; the platform's attached sheet covers a sheet
        // of anything else's.
        let question_up = self.asking || system::has_sheet(window);
        match answer(true, question_up) {
            Answer::Nothing => {}
            Answer::Show => bring_forward(window, cx),
            Answer::ShowAndAsk => {
                bring_forward(window, cx);
                if self.may_close_window(window, cx) {
                    crate::quit(cx);
                }
            }
        }
    }

    /// Whether quitting now would lose something: a file tab with unsaved
    /// text, or a session running in a tab — what `may_close_window` asks
    /// about.
    fn quit_needs_asking(&self, cx: &Context<Root>) -> bool {
        !self.dirty_file_tabs(None, cx).is_empty() || self.running_sessions() > 0
    }
}

/// What a tick does about a quit AppKit cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Answer {
    /// No request, nothing to do.
    Nothing,
    /// Bring the window forward: the question already up is the answer.
    Show,
    /// Bring the window forward and ask.
    ShowAndAsk,
}

/// From whether AppKit left a request, and whether a question is already
/// up. Pure, so the half that decides is checked without a window.
fn answer(requested: bool, question_up: bool) -> Answer {
    match (requested, question_up) {
        (false, _) => Answer::Nothing,
        (true, true) => Answer::Show,
        (true, false) => Answer::ShowAndAsk,
    }
}

/// Put the window where a person can answer it: the application to the
/// front, the window back from the Dock if it was minimised, and made key.
///
/// The Dock's Quit and a logout arrive while another application is in
/// front — a `cmd-q` never does, being a menu stroke of the front
/// application — and a sheet hangs from its window wherever that window
/// is. Asked there, the question is on a window nobody can see, and the
/// Quit looks as though it did nothing. `cx.activate(true)` is
/// `activateIgnoringOtherApps:` and brings back a hidden application;
/// GPUI's `activate_window` is `makeKeyAndOrderFront:` alone, which is
/// why the window is deminiaturised by `system` first.
fn bring_forward(window: &mut Window, cx: &mut Context<Root>) {
    cx.activate(true);
    system::deminiaturize(window);
    window.activate_window();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A quit from outside is asked where it can be seen.** The Dock's
    /// Quit and a logout reach Guild while another application is in front,
    /// often with the window minimised or under others, and a sheet hangs
    /// from the window it is asked in — so a question asked without
    /// bringing the window forward looks like a Quit that did nothing. And
    /// a second Quit under a question already up asks nothing new, but
    /// still shows the question, or it is dropped where nobody sees why.
    #[test]
    fn a_quit_from_outside_brings_its_question_forward() {
        assert_eq!(answer(false, false), Answer::Nothing);
        assert_eq!(answer(false, true), Answer::Nothing);
        assert_eq!(answer(true, false), Answer::ShowAndAsk);
        assert_eq!(answer(true, true), Answer::Show);
    }
}
