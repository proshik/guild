//! A terminal's half of the right-button menu — M5.12, task 13.
//!
//! A tab's terminal and the Output panel's are one terminal, and they get
//! one menu: the Edit menu's terminal half — Copy when there is a
//! selection, Paste, Select All; Find…; Clear Terminal. Section 6.17 of the
//! design system.
//!
//! **Strokes on a tab's terminal, none on the Output panel's** — decision 14
//! of the plan, M8.8's decision 5. Every item here has an action that does
//! the same thing to the terminal holding the focus, and the menu gives the
//! focus back to the tab in front when it closes: on a tab's terminal that
//! is the terminal the item acted on, and on the Output's it is not, so a
//! stroke shown there would promise the tab's terminal.

use gpui::{Context, Entity, Window};

use super::{Deed, Entry, Target};
use crate::Root;
use crate::i18n::Text;
use crate::terminal::view::TerminalTab;

/// Which terminal a menu is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalAt {
    /// The terminal of a tab.
    Tab(crate::TabId),
    /// The Output panel's, by the project whose run it shows.
    Run(guild_session::ProjectId, u64),
    Panel(guild_session::ProjectId, u64),
}

/// What only a terminal can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalDeed {
    /// The selection, on the clipboard.
    Copy,
    /// The clipboard, typed into the program — bracketed when it asked.
    Paste,
    /// The history and the screen, selected.
    SelectAll,
    /// The find bar, open over this terminal.
    Find,
    /// The screen and the history, emptied.
    Clear,
    PreviousCommand,
    NextCommand,
    SelectCommandOutput,
    CopyCommandOutput,
}

/// A terminal's menu. `selection` is whether anything is selected that a
/// Copy would take.
pub fn entries_for_terminal(selection: bool) -> Vec<Entry> {
    let mut entries = Vec::new();
    if selection {
        entries.push(Entry::Item(
            Text::MenuCopy,
            Deed::Terminal(TerminalDeed::Copy),
        ));
    }
    entries.extend([
        Entry::Item(Text::MenuPaste, Deed::Terminal(TerminalDeed::Paste)),
        Entry::Item(Text::MenuSelectAll, Deed::Terminal(TerminalDeed::SelectAll)),
        Entry::Separator,
        Entry::Item(Text::MenuFind, Deed::Terminal(TerminalDeed::Find)),
        Entry::Separator,
        Entry::Item(Text::MenuClearTerminal, Deed::Terminal(TerminalDeed::Clear)),
    ]);
    entries
}

impl Root {
    /// The terminal a menu's target names, while it is still there.
    pub(super) fn terminal_at(&self, at: TerminalAt) -> Option<Entity<TerminalTab>> {
        match at {
            TerminalAt::Tab(id) => self
                .tabs
                .iter()
                .find(|tab| tab.id == id)
                .and_then(crate::Tab::body)
                .and_then(|body| match body {
                    crate::Body::Terminal { view, .. } => Some(view.clone()),
                    _ => None,
                }),
            TerminalAt::Panel(project, id) => self
                .panel_terminals
                .get(&project)
                .and_then(|tabs| tabs.get(id))
                .map(|entry| entry.value.clone()),
            TerminalAt::Run(project, id) => self
                .runs
                .get(&project)
                .and_then(|tabs| tabs.get(id))
                .and_then(|entry| entry.value.view.clone()),
        }
    }

    /// Record a right-button press on a terminal as the menu's target.
    ///
    /// The selection is asked here, once, at the press: the target is a
    /// record the menu reads on every frame it is up, the way a tab's
    /// records its place in the strip, and the question takes the grid's
    /// lock each time it is asked.
    pub(crate) fn note_terminal_menu(&mut self, at: TerminalAt, cx: &mut Context<Root>) {
        let Some(view) = self.terminal_at(at) else {
            return;
        };
        let selection = view.read(cx).has_selection();
        self.note_menu_target(Target::Terminal { at, selection });
    }

    /// Carry out what a terminal's menu offers.
    ///
    /// `take_menu_item` has given the focus back before this runs, so the
    /// find bar's focus is the last word and not the tab's.
    pub(super) fn run_terminal_deed(
        &mut self,
        deed: TerminalDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        let Target::Terminal { at, .. } = target else {
            return;
        };
        let at = *at;
        let Some(view) = self.terminal_at(at) else {
            return;
        };
        match deed {
            TerminalDeed::Copy => view.update(cx, |terminal, cx| terminal.copy(cx)),
            TerminalDeed::Paste => {
                view.update(cx, |terminal, cx| terminal.paste_with_input(window, cx))
            }
            TerminalDeed::SelectAll => view.update(cx, |terminal, cx| terminal.select_all(cx)),
            TerminalDeed::Find => view.update(cx, |terminal, cx| terminal.open_find(window, cx)),
            TerminalDeed::Clear => view.update(cx, |terminal, cx| terminal.clear(cx)),
            TerminalDeed::PreviousCommand => {
                view.update(cx, |terminal, cx| terminal.navigate_command(true, cx));
            }
            TerminalDeed::NextCommand => {
                view.update(cx, |terminal, cx| terminal.navigate_command(false, cx));
            }
            TerminalDeed::SelectCommandOutput => {
                view.update(cx, |terminal, cx| terminal.command_output(false, cx))
            }
            TerminalDeed::CopyCommandOutput => {
                view.update(cx, |terminal, cx| terminal.command_output(true, cx))
            }
        }
    }
}
