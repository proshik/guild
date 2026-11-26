//! The tab strip's half of the right-button menu.
//!
//! **This is the surface that made the deeds deeds.** A tab's *Close*
//! has to close the tab under the pointer, and the `CloseTab` action
//! closes the tab in front — so a person who right-clicks a background
//! tab and picks Close would lose the wrong file. The items carry their
//! target; a stroke is shown beside one only where the two would do the
//! same thing, which `super::stroke_for` decides.
//!
//! The menu shows a stroke on more items than a tab menu usually does,
//! because M8.8.0 gave `CloseOtherTabs` and `CloseAllTabs` strokes and
//! those items are then honest on the tab in front. Since M12.8 a tab
//! also goes to another area from here, *Move to Area ▸*, and divides,
//! *Split Tab Right* and *Split Tab Down* — split items on panes rather
//! than editor windows. What is deliberately not here is *Pin Tab* and
//! *Keep Tab Open* — there is no pin and no preview tab to make them
//! mean anything — and the *Copy Path/Reference* options that name a
//! project model Guild does not have: content root, source root,
//! repository root, a hosting URL.

use gpui::{ClipboardItem, Context, Window};
use guild_session::layout::{Axis, Direction};

use super::{Deed, Entry, PathDeed, Target};
use crate::Root;
use crate::i18n::Text;

/// What only a tab can do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabDeed {
    /// Give a session's tab a name of the person's own — M12.1.
    Rename,
    /// Close this tab, asking if it holds unsaved text.
    Close,
    /// Close every other tab of this project.
    CloseOthers,
    /// Close every tab of this project, this one included.
    CloseAll,
    /// Close the tabs before this one in the strip.
    CloseLeft,
    /// Close the tabs after this one in the strip.
    CloseRight,
    /// The file's name alone, on the clipboard.
    CopyFileName,
    /// The path inside the project with the caret's line after it —
    /// what one pastes into a terminal or into a message to an agent.
    CopyPathWithLine,
    /// Move this tab to the area an arrow from its own reaches — M12.8,
    /// decision 8: its panes, its process and its state go with it.
    MoveToward(Direction),
    /// Divide this tab's area along the axis and move the tab into the
    /// new one — one gesture, decision 21.
    MoveToNewArea(Axis),
    /// Divide this tab's focused pane, the new, empty pane to the right —
    /// M12.8, decision 21; the tab comes forward to show it.
    SplitRight,
    /// The same, the new pane below.
    SplitDown,
}

/// A tab's menu.
///
/// The place in the strip decides three of the items. *Close the others*
/// is worth offering only where there are others; *to the left* and *to
/// the right* only where there is something on that side. An item that
/// would do nothing is an item that makes a person doubt the menu.
///
/// `has_file` takes the path half away — a session tab has no file behind
/// it, and neither has the diff of a whole tree.
///
/// `session` puts *Rename Tab…* first — what a tab is called is what it
/// is — and only there: a file is its file's name and a diff its files'.
///
/// `neighbours` — left, right, up, down — says which areas an arrow from
/// the tab's own area reaches; *Move to Area ▸* names those by where they
/// are, then offers a new area to the right or below (M12.8, decision 21);
/// *Split Tab Right* and *Split Tab Down* follow it, in the same group.
pub fn entries_for_tab(
    has_file: bool,
    session: bool,
    index: usize,
    of: usize,
    neighbours: [bool; 4],
) -> Vec<Entry> {
    let mut entries = Vec::new();
    if session {
        entries.push(Entry::Item(Text::MenuRenameTab, Deed::Tab(TabDeed::Rename)));
        entries.push(Entry::Separator);
    }
    entries.push(Entry::Item(Text::MenuCloseTab, Deed::Tab(TabDeed::Close)));
    if of > 1 {
        entries.push(Entry::Item(
            Text::MenuCloseOthers,
            Deed::Tab(TabDeed::CloseOthers),
        ));
    }
    entries.push(Entry::Item(
        Text::MenuCloseAll,
        Deed::Tab(TabDeed::CloseAll),
    ));
    if index > 0 {
        entries.push(Entry::Item(
            Text::MenuCloseLeft,
            Deed::Tab(TabDeed::CloseLeft),
        ));
    }
    if index + 1 < of {
        entries.push(Entry::Item(
            Text::MenuCloseRight,
            Deed::Tab(TabDeed::CloseRight),
        ));
    }
    // Where the tab can go, set apart from the closings by a rule, as the
    // mockup draws it.
    entries.push(Entry::Separator);
    entries.push(Entry::Submenu(
        Text::MenuMoveToArea,
        move_to_area(neighbours),
    ));
    entries.push(Entry::Item(
        Text::MenuSplitPaneRight,
        Deed::Tab(TabDeed::SplitRight),
    ));
    entries.push(Entry::Item(
        Text::MenuSplitPaneDown,
        Deed::Tab(TabDeed::SplitDown),
    ));
    if has_file {
        entries.push(Entry::Separator);
        entries.push(Entry::Item(
            Text::MenuShowDiff,
            Deed::Path(PathDeed::ShowDiff),
        ));
        entries.push(Entry::Separator);
        entries.push(Entry::Item(
            Text::MenuCopyFileName,
            Deed::Tab(TabDeed::CopyFileName),
        ));
        entries.push(Entry::Item(
            Text::MenuCopyPath,
            Deed::Path(PathDeed::CopyPath),
        ));
        entries.push(Entry::Item(
            Text::MenuCopyPathWithLine,
            Deed::Tab(TabDeed::CopyPathWithLine),
        ));
        entries.push(Entry::Item(
            Text::MenuCopyFullPath,
            Deed::Path(PathDeed::CopyFullPath),
        ));
        entries.push(Entry::Item(Text::MenuReveal, Deed::Path(PathDeed::Reveal)));
    }
    entries
}

/// *Move to Area ▸*'s rows: an area on each side there is one, in the
/// arrows' order, a rule when there was any, then the two new areas.
fn move_to_area(neighbours: [bool; 4]) -> Vec<Entry> {
    let sides = [
        (Direction::Left, Text::MenuAreaLeft),
        (Direction::Right, Text::MenuAreaRight),
        (Direction::Up, Text::MenuAreaAbove),
        (Direction::Down, Text::MenuAreaBelow),
    ];
    let mut rows: Vec<Entry> = sides
        .into_iter()
        .zip(neighbours)
        .filter(|(_, there)| *there)
        .map(|((direction, text), _)| Entry::Item(text, Deed::Tab(TabDeed::MoveToward(direction))))
        .collect();
    if !rows.is_empty() {
        rows.push(Entry::Separator);
    }
    rows.push(Entry::Item(
        Text::MenuNewAreaRight,
        Deed::Tab(TabDeed::MoveToNewArea(Axis::Horizontal)),
    ));
    rows.push(Entry::Item(
        Text::MenuNewAreaBelow,
        Deed::Tab(TabDeed::MoveToNewArea(Axis::Vertical)),
    ));
    rows
}

impl Root {
    /// Carry out a tab deed on the tab the pointer was on.
    pub(super) fn run_tab_deed(
        &mut self,
        deed: TabDeed,
        target: &Target,
        window: &mut Window,
        cx: &mut Context<Root>,
    ) {
        match deed {
            TabDeed::Rename => {
                if let Target::Tab { id, .. } = target {
                    self.start_renaming_tab(*id, window, cx);
                }
            }
            TabDeed::Close => {
                if let Target::Tab { id, .. } = target {
                    self.close_tab_asking(*id, window, cx);
                }
            }
            // **The pressed tab's own strip**, not the focused area's —
            // M12.8: a tab's menu is about the row it was opened on, and
            // with two areas that row need not be the one with the keys.
            TabDeed::CloseOthers | TabDeed::CloseAll => {
                let Target::Tab { id, .. } = target else {
                    return;
                };
                let keep = matches!(deed, TabDeed::CloseOthers).then_some(*id);
                let tabs: Vec<crate::TabId> = self
                    .strip_of(*id)
                    .into_iter()
                    .filter(|id| Some(*id) != keep)
                    .collect();
                self.close_tabs_asking(tabs, window, cx);
            }
            TabDeed::CloseLeft | TabDeed::CloseRight => {
                let Target::Tab { id, index, .. } = target else {
                    return;
                };
                // The row's own index rather than a search for its id:
                // the strip is what the person is pointing at, and the
                // index came from the strip.
                let left = matches!(deed, TabDeed::CloseLeft);
                let tabs: Vec<crate::TabId> = self
                    .strip_of(*id)
                    .into_iter()
                    .enumerate()
                    .filter(|(at, _)| if left { at < index } else { at > index })
                    .map(|(_, tab)| tab)
                    .collect();
                self.close_tabs_asking(tabs, window, cx);
            }
            TabDeed::MoveToward(direction) => {
                if let Target::Tab { id, .. } = target {
                    self.move_tab_toward(*id, direction, window, cx);
                }
            }
            TabDeed::MoveToNewArea(axis) => {
                if let Target::Tab { id, .. } = target {
                    self.move_tab_to_new_area(*id, axis, window, cx);
                }
            }
            TabDeed::SplitRight | TabDeed::SplitDown => {
                if let Target::Tab { id, .. } = target {
                    let axis = match deed {
                        TabDeed::SplitRight => Axis::Horizontal,
                        _ => Axis::Vertical,
                    };
                    self.split_tab(*id, axis, window, cx);
                }
            }
            TabDeed::CopyFileName => {
                if let Some(name) = target
                    .path()
                    .and_then(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                {
                    cx.write_to_clipboard(ClipboardItem::new_string(name));
                }
            }
            TabDeed::CopyPathWithLine => {
                let Target::Tab { id, path, .. } = target else {
                    return;
                };
                let Some(path) = path else {
                    return;
                };
                let line = self.caret_line(*id, cx);
                let text = format!("{}:{line}", path.display());
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        }
    }

    /// Which line the caret is on in a tab, counting from one.
    ///
    /// **A tab that has never been drawn has no editor**, which is the
    /// trap the autosave table and M8.5 both met: the buffer is made at
    /// the first render, not when the tab is. Such a tab reads as line
    /// one, which is where its caret will be the moment it is shown.
    fn caret_line(&self, id: crate::TabId, cx: &Context<Root>) -> u32 {
        let Some(tab) = self.tabs.iter().find(|tab| tab.id == id) else {
            return 1;
        };
        let Some(crate::Body::File { tab: view, .. }) = tab.body() else {
            return 1;
        };
        view.read(cx)
            .editor()
            .map(|editor| editor.read(cx).cursor_position().line + 1)
            .unwrap_or(1)
    }
}
