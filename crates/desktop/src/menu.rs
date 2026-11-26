//! The menu bar.
//!
//! The menu is built from the same actions as the keymap: the system finds
//! and shows the shortcut next to each item itself. So an item cannot
//! drift from its shortcut — what binds them is the action, not a string
//! placed beside it.
//!
//! On macOS an application without a menu looks unfinished even to people
//! who never open it: that is where "About", "Hide" and "Quit" live, which
//! everyone reaches for out of habit.

use std::path::Path;

use gpui::{App, Menu, MenuItem, OsAction};

use crate::i18n::{Text, Ui};

// The whole module, for the reason `root::render` gives.
use crate::actions::*;

/// Build and install the menu.
///
/// Rebuilt when the language changes **and when the recent list does**:
/// the menu bar is drawn by the system, and it will not learn about either
/// on its own.
///
/// File groups project creation and opening before file and tab actions.
pub fn install(ui: Ui, recent: &[&Path], cx: &mut App) {
    let t = |text: Text| ui.text(text).to_owned();
    cx.set_menus(vec![
        Menu {
            disabled: false,
            name: "Guild".into(),
            items: vec![
                MenuItem::action(t(Text::MenuAbout), About),
                MenuItem::action(t(Text::MenuUpdateGuild), UpdateGuild),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuSettings), OpenSettings),
                MenuItem::separator(),
                // Services, "Hide" and "Quit" are drawn by the system:
                // our own would look the same but work worse.
                MenuItem::os_submenu(t(Text::MenuServices), gpui::SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuHide), Hide),
                MenuItem::action(t(Text::MenuHideOthers), HideOthers),
                MenuItem::action(t(Text::MenuShowAll), ShowAll),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuQuit), Quit),
            ],
        },
        Menu {
            disabled: false,
            name: t(Text::MenuFile).into(),
            items: vec![
                MenuItem::submenu(Menu {
                    disabled: false,
                    name: t(Text::MenuNew).into(),
                    items: vec![
                        MenuItem::action(t(Text::MenuNewProject), NewProject),
                        MenuItem::action(t(Text::MenuExistingSources), AddProject),
                        MenuItem::action(t(Text::GetFromVersionControl), GetFromVersionControl),
                    ],
                }),
                MenuItem::action(t(Text::MenuOpenProject), AddProject),
                MenuItem::submenu(recent_menu(ui, recent)),
                MenuItem::action(t(Text::MenuCloseProject), CloseProject),
                MenuItem::separator(),
                MenuItem::action(t(Text::ActionPush), Push),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuSave), SaveFile),
                MenuItem::action(t(Text::MenuSaveAll), SaveAllFiles),
                MenuItem::action(t(Text::MenuReloadFile), ReloadFile),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuCloseTab), CloseTab),
                MenuItem::action(t(Text::MenuCloseOthers), CloseOtherTabs),
                MenuItem::action(t(Text::MenuCloseAll), CloseAllTabs),
                MenuItem::action(t(Text::MenuReopenTab), ReopenTab),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuSettings), OpenSettings),
            ],
        },
        Menu {
            disabled: false,
            name: t(Text::MenuEdit).into(),
            items: vec![
                // `os_action` ties the item to the system's own action:
                // without it macOS will not enable the item at the right
                // moment and will not hand over the shortcut.
                MenuItem::Action {
                    checked: false,
                    disabled: false,
                    name: t(Text::MenuCopy).into(),
                    action: Box::new(Copy),
                    os_action: Some(OsAction::Copy),
                },
                MenuItem::Action {
                    checked: false,
                    disabled: false,
                    name: t(Text::MenuPaste).into(),
                    action: Box::new(Paste),
                    os_action: Some(OsAction::Paste),
                },
                MenuItem::Action {
                    checked: false,
                    disabled: false,
                    name: t(Text::MenuSelectAll).into(),
                    action: Box::new(SelectAll),
                    os_action: Some(OsAction::SelectAll),
                },
                MenuItem::separator(),
                // M5.12: a terminal's find bar. Grey until something on
                // the focus path handles `Find` — the terminal, from task 9.
                MenuItem::action(t(Text::MenuFind), Find),
                // M8.9: the popup, and the Search tool stays in View.
                MenuItem::action(t(Text::MenuFindInFiles), FindInFiles),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuClearTerminal), ClearTerminal),
            ],
        },
        // A menu of its own, as IDEs commonly have one: the three
        // deeds of the Output panel, and the panel itself stays in View
        // with the other tools.
        Menu {
            disabled: false,
            name: t(Text::MenuRun).into(),
            items: vec![
                MenuItem::action(t(Text::RunStart), Run),
                MenuItem::action(t(Text::RunStop), StopRun),
                MenuItem::separator(),
                MenuItem::action(t(Text::RunClear), ClearRun),
                MenuItem::separator(),
                MenuItem::action(t(Text::GoToDefinition), GoToDefinition),
                MenuItem::action(t(Text::ActionFindUsages), FindUsages),
            ],
        },
        Menu {
            disabled: false,
            name: t(Text::MenuView).into(),
            items: vec![
                MenuItem::action(t(Text::MenuLarger), IncreaseFontSize),
                MenuItem::action(t(Text::MenuSmaller), DecreaseFontSize),
                MenuItem::action(t(Text::MenuActualSize), ResetFontSize),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuSessionList), ToggleSessions),
                // What the window shows of the projects, above what it
                // shows of one project. M12.
                MenuItem::action(t(Text::MenuToggleColumn), ToggleSidebar),
                MenuItem::action(t(Text::MenuPreviousWorkspace), PreviousWorkspace),
                MenuItem::action(t(Text::MenuNextWorkspace), NextWorkspace),
                MenuItem::separator(),
                // The areas — M12.8. The native menu has no row drawn
                // faint with its reason, so *Close Area* on the last area
                // is taken and answers with a notice in the `⋮`'s words.
                MenuItem::action(t(Text::MenuSplitAreaRight), SplitAreaRight),
                MenuItem::action(t(Text::MenuSplitAreaDown), SplitAreaDown),
                MenuItem::action(t(Text::MenuSplitPaneRight), SplitPaneRight),
                MenuItem::action(t(Text::MenuSplitPaneDown), SplitPaneDown),
                MenuItem::action(t(Text::MenuCloseArea), CloseArea),
                MenuItem::separator(),
                MenuItem::action(t(Text::PanelProject), ShowExplorerPanel),
                MenuItem::action(t(Text::ToolCommit), ShowCommitPanel),
                MenuItem::action(t(Text::Log), ShowLogPanel),
                MenuItem::action(t(Text::PanelOutput), ShowOutputPanel),
                MenuItem::action(t(Text::PanelTerminal), ShowTerminalPanel),
                MenuItem::action(t(Text::ToolProblems), ShowProblems),
                MenuItem::action(t(Text::ToolFind), ShowFindPanel),
                MenuItem::separator(),
                // A submenu per tool: where it goes. Decision 2 of M8.3.1 —
                // the menu before the drag, because a menu dispatches an
                // action a test can reach and a person whose drag went
                // wrong needs a way that cannot.
                MenuItem::submenu(Menu {
                    disabled: false,
                    name: format!("{} \u{2014} {}", t(Text::PanelProject), t(Text::MoveTo)).into(),
                    items: vec![
                        MenuItem::action(t(Text::DockLeft), MoveExplorerToLeft),
                        MenuItem::action(t(Text::DockRight), MoveExplorerToRight),
                        MenuItem::action(t(Text::DockBottom), MoveExplorerToBottom),
                    ],
                }),
                MenuItem::submenu(Menu {
                    disabled: false,
                    name: format!("{} \u{2014} {}", t(Text::ToolCommit), t(Text::MoveTo)).into(),
                    items: vec![
                        MenuItem::action(t(Text::DockLeft), MoveCommitToLeft),
                        MenuItem::action(t(Text::DockRight), MoveCommitToRight),
                        MenuItem::action(t(Text::DockBottom), MoveCommitToBottom),
                    ],
                }),
                MenuItem::submenu(Menu {
                    disabled: false,
                    name: format!("{} \u{2014} {}", t(Text::Log), t(Text::MoveTo)).into(),
                    items: vec![
                        MenuItem::action(t(Text::DockLeft), MoveLogToLeft),
                        MenuItem::action(t(Text::DockRight), MoveLogToRight),
                        MenuItem::action(t(Text::DockBottom), MoveLogToBottom),
                    ],
                }),
                MenuItem::submenu(Menu {
                    disabled: false,
                    name: format!("{} \u{2014} {}", t(Text::PanelOutput), t(Text::MoveTo)).into(),
                    items: vec![
                        MenuItem::action(t(Text::DockLeft), MoveOutputToLeft),
                        MenuItem::action(t(Text::DockRight), MoveOutputToRight),
                        MenuItem::action(t(Text::DockBottom), MoveOutputToBottom),
                    ],
                }),
                MenuItem::action(t(Text::RevealActiveFile), RevealFile),
                MenuItem::separator(),
                MenuItem::action(t(Text::ShowDiff), ShowDiff),
                MenuItem::action(t(Text::ShowChanges), ShowChanges),
                MenuItem::action(t(Text::MenuCommit), Commit),
                MenuItem::action(t(Text::Annotate), Annotate),
                MenuItem::separator(),
                // Not named "Enter Full Screen": macOS binds its own
                // shortcut to a menu item with that exact name, and two
                // things on one stroke is one of them doing nothing.
                MenuItem::action(t(Text::MenuFullScreen), ToggleFullScreen),
            ],
        },
        Menu {
            disabled: false,
            name: t(Text::MenuGo).into(),
            items: vec![
                // Where the caret has been, first — the common order,
                // and the reason the menu is called Navigate.
                MenuItem::action(t(Text::MenuBack), Back),
                MenuItem::action(t(Text::MenuForward), Forward),
                MenuItem::action(t(Text::MenuLastEdit), LastEditLocation),
                MenuItem::separator(),
                // M8.9: the one popup behind four gestures, above the lists
                // of what was lately — the common Navigate menu's order.
                MenuItem::action(t(Text::MenuSearchEverywhere), SearchEverywhere),
                MenuItem::action(t(Text::MenuGoToFile), GoToFile),
                MenuItem::action(t(Text::MenuGoToSymbol), GoToSymbol),
                MenuItem::action(t(Text::MenuFindAction), FindAction),
                MenuItem::action(t(Text::MenuRecentFiles), RecentFiles),
                MenuItem::action(t(Text::MenuRecentLocations), RecentLocations),
                MenuItem::action(t(Text::MenuGoToLine), GoToLine),
                MenuItem::action(t(Text::MenuSelectInExplorer), RevealFile),
                MenuItem::separator(),
                MenuItem::action(t(Text::MenuNextTab), NextTab),
                MenuItem::action(t(Text::MenuPreviousTab), PreviousTab),
                // Beside the tab cycle: it is a jump to a tab, chosen by
                // who is waiting rather than by where it sits. M5.10.
                MenuItem::action(t(Text::MenuGoToWaitingSession), GoToWaitingSession),
                MenuItem::separator(),
                // The popup before the cycle: this is the item that
                // carries the stroke now, and the one below it says by
                // sitting there that stepping is still an action a person
                // can bind.
                MenuItem::action(t(Text::MenuRecentProjects), RecentProjects),
                MenuItem::action(t(Text::MenuNextProject), NextProject),
            ],
        },
        Menu {
            disabled: false,
            name: t(Text::MenuHelp).into(),
            items: vec![MenuItem::action(t(Text::MenuShowLog), ShowLog)],
        },
    ]);
}

/// The "Open Recent" submenu.
///
/// The items carry a position rather than a path: the list they index into
/// is the one on disk, so a menu built a moment ago cannot open a
/// directory that has since moved up or out.
///
/// An empty list still gets a submenu, with one dead item saying so. A
/// submenu that appears and disappears is harder to find than one that is
/// always in the same place.
fn recent_menu(ui: Ui, recent: &[&Path]) -> Menu {
    let mut items: Vec<MenuItem> = recent
        .iter()
        .enumerate()
        .map(|(index, path)| MenuItem::action(label_for(path), OpenRecent { index }))
        .collect();

    if items.is_empty() {
        // `MenuItem::action` is the only kind that shows text; without an
        // action the system draws nothing at all. Pointing it at
        // ClearRecent is harmless — there is nothing to clear.
        items.push(MenuItem::action(
            ui.text(Text::MenuNoRecent).to_owned(),
            ClearRecent,
        ));
    } else {
        items.push(MenuItem::separator());
        items.push(MenuItem::action(
            ui.text(Text::MenuClearRecent).to_owned(),
            ClearRecent,
        ));
    }

    Menu {
        disabled: false,
        name: ui.text(Text::MenuOpenRecent).to_owned().into(),
        items,
    }
}

/// How a directory is named in the menu.
///
/// The directory's own name, with the parent after it: half the projects a
/// person has are called `src` or `app`, and a column of identical names
/// is not a menu.
fn label_for(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    match path.parent().and_then(Path::file_name) {
        Some(parent) => format!("{name} — {}", parent.to_string_lossy()),
        None => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half the projects a person has are called `src`. A column of
    /// identical names is not a menu.
    #[test]
    fn a_directory_is_named_with_its_parent() {
        assert_eq!(
            label_for(Path::new("/Users/someone/work/guild")),
            "guild — work"
        );
    }

    #[test]
    fn a_top_level_directory_is_named_on_its_own() {
        assert_eq!(label_for(Path::new("/guild")), "guild");
    }

    /// The root has no name of its own; the label must still be something
    /// rather than an empty item.
    #[test]
    fn the_root_still_gets_a_label() {
        assert!(!label_for(Path::new("/")).is_empty());
    }
}

#[cfg(test)]
mod settings_tests {
    use super::*;

    #[gpui::test]
    fn file_menu_opens_settings_in_both_languages(cx: &mut gpui::TestAppContext) {
        for language in [
            crate::i18n::Language::English,
            crate::i18n::Language::Russian,
        ] {
            cx.update(|cx| {
                let ui = Ui(language);
                install(ui, &[], cx);
                let menus = cx.get_menus().unwrap();
                let file = menus
                    .iter()
                    .find(|menu| menu.name.as_ref() == ui.text(Text::MenuFile))
                    .unwrap();
                assert!(file.items.iter().any(|item| matches!(item,
                    gpui::OwnedMenuItem::Action {name, action, disabled: false, ..}
                    if name == ui.text(Text::MenuSettings) && action.as_any().is::<OpenSettings>()
                )), "File menu must expose the existing Settings action");
            });
        }
    }
}
