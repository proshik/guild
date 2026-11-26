//! Overriding the keymap with a file.
use super::*;
use std::collections::BTreeMap;

/// What prevented reading the keymap.
#[derive(Debug, thiserror::Error)]
pub enum KeymapError {
    /// The file did not parse as JSON.
    #[error("keymap file could not be parsed: {0}")]
    Parse(#[source] serde_json::Error),

    /// No action is declared under that name.
    #[error("unknown action \"{0}\"; the list is in the Help menu")]
    UnknownAction(String),

    /// The shortcut did not parse.
    #[error("shortcut \"{0}\" could not be parsed")]
    BadStroke(String),
}

/// Every action the keymap knows, **in one list**.
///
/// It was four: the enum's variants, `Named::ALL`, `name()` and
/// `binding()` — sixty-five entries each, hand-kept, in three narrow
/// line ranges of this file. Adding one action meant four edits and four
/// conflict sites, which is four times six for the branches of M8.8.
///
/// The keymap file's name **is** the variant's name and the action
/// type's name; all three were already identical for all sixty-five,
/// which is what makes one list enough. A name that must differ from its
/// variant goes through `by_name`'s alias arm, as `ShowChangesPanel`
/// does.
///
/// GPUI's own `actions!` declaration in the module above stays where it
/// is: it declares the types this list refers to, and it is not ours to
/// generate.
macro_rules! named_actions {
    ($($action:ident),+ $(,)?) => {
        /// An action named in the keymap file.
        ///
        /// An enum rather than a string: adding an action and forgetting
        /// to teach the keymap to bind it is a mistake the compiler
        /// should catch, not the person whose "shortcut does not work".
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        pub enum Named {
            $($action),+
        }

        impl Named {
            /// Every action — for the help list and for validating a
            /// file — in the order they are declared, which is the order
            /// the settings draw them in.
            pub const ALL: &'static [Named] = &[$(Named::$action),+];

            /// The name the action is known by in the keymap file.
            ///
            /// Part of the contract with the person editing that file:
            /// renaming the type must not change it. That is now a
            /// promise the generator keeps, rather than a rule
            /// sixty-five arms have to be read against.
            pub fn name(self) -> &'static str {
                match self {
                    $(Named::$action => stringify!($action)),+
                }
            }

            /// A binding for this action.
            pub(super) fn binding(self, stroke: &str, context: Option<&str>) -> KeyBinding {
                match self {
                    $(Named::$action => KeyBinding::new(stroke, $action, context)),+
                }
            }

            /// The action itself, boxed.
            ///
            /// For a menu item that wants to **show** a stroke without
            /// dispatching it: `PopupMenuItem::action` draws the binding
            /// it finds for the action, and `PopupMenu::confirm` is
            /// `if let Some(handler) … else if let Some(action) …`, so an
            /// item that also has a handler runs the handler and never
            /// dispatches this. That is what lets a deed carry a stroke
            /// honestly — see `view::menu::stroke_for`.
            pub fn action(self) -> Box<dyn gpui::Action> {
                match self {
                    $(Named::$action => Box::new($action)),+
                }
            }
        }
    };
}

named_actions! {
    NewSession,
    NewTerminal,
    CloseTab,
    ReopenTab,
    CloseOtherTabs,
    CloseAllTabs,
    SaveFile,
    SaveAllFiles,
    ReloadFile,
    RevealFile,
    ShowDiff,
    ShowChanges,
    Commit,
    Annotate,
    NextChange,
    PreviousChange,
    StageHunk,
    UnstageHunk,
    RefreshChanges,
    ShowSelectedDiff,
    IncludeSelectedChanges,
    ExcludeSelectedChanges,
    TickAllChanges,
    UntickAllChanges,
    CollapseChanges,
    ExpandChanges,
    ShowExplorerPanel,
    ShowCommitPanel,
    ShowLogPanel,
    ShowOutputPanel,
    ShowTerminalPanel,
    FindInFiles,
    ReplaceInFiles,
    ShowSearchPanel,
    Run,
    StopRun,
    ClearRun,
    GoToDefinition,
    MoveExplorerToLeft,
    MoveExplorerToRight,
    MoveExplorerToBottom,
    MoveCommitToLeft,
    MoveCommitToRight,
    MoveCommitToBottom,
    MoveLogToLeft,
    MoveLogToRight,
    MoveLogToBottom,
    MoveOutputToLeft,
    MoveOutputToRight,
    MoveOutputToBottom,
    NextTab,
    PreviousTab,
    AddProject,
    OpenSelectedApp,
    SelectOpenApp,
    NewProject,
    GetFromVersionControl,
    CloseProject,
    CopyWorktreePath,
    RemoveWorktree,
    ViewMergeRequest,
    NextProject,
    PreviousProject,
    NextWorkspace,
    PreviousWorkspace,
    // M12.8 — the areas.
    SplitAreaRight,
    SplitAreaDown,
    SplitPaneRight,
    SplitPaneDown,
    CloseArea,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    // Declared by M8.8.0 for the six branches; see `actions!` above.
    Back,
    Forward,
    RecentFiles,
    RecentLocations,
    LastEditLocation,
    GoToLine,
    ShowTabList,
    Push,
    Rollback,
    AddToVcs,
    NewFile,
    NewDirectory,
    RenameFile,
    MoveFile,
    CutFiles,
    JumpToSource,
    DeleteFile,
    ShowProblems,
    FindUsages,
    ShowFindPanel,
    NextUsage,
    PreviousUsage,
    RecentProjects,
    ToggleSessions,
    ToggleTerminalJobs,
    ToggleUsage,
    GoToWaitingSession,
    ToggleSidebar,
    ToggleLeftPanel,
    ToggleRightPanel,
    ToggleBottomPanel,
    WidenPanel,
    NarrowPanel,
    StretchToLeft,
    StretchToRight,
    StretchToTop,
    StretchToBottom,
    OpenSettings,
    ShowLog,
    ToggleFullScreen,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    ClearTerminal,
    Copy,
    Paste,
    // M5.12 — the terminal for agents.
    SelectAll,
    Find,
    Replace,
    // M8.9 — search and navigation.
    SearchEverywhere,
    GoToFile,
    GoToSymbol,
    FindAction,
}

impl Named {
    /// The action a keymap file's name refers to.
    fn by_name(name: &str) -> Option<Self> {
        // M8.3 wrote this name into keymap files; the panel is the Commit
        // panel since M8.3.1, and the file still binds it.
        if name == "ShowChangesPanel" {
            return Some(Named::ShowCommitPanel);
        }
        // M12 wrote the sidebar's toggle as the column's, and the person
        // found the name silly — M12.3.
        if name == "ToggleColumn" {
            return Some(Named::ToggleSidebar);
        }
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.name() == name)
    }
}

/// A person's own shortcuts, by action.
///
/// The file is written the other way round — "shortcut: action" —
/// because that is how a keymap reads to someone editing it by hand.
/// The settings need it by action, to show one row per thing that can
/// be done.
///
/// A corrupted file leaves the defaults in force rather than stopping
/// the application: an environment without your shortcuts is
/// inconvenient, one that will not open because of them is useless.
pub fn overrides() -> BTreeMap<Named, String> {
    let Ok(text) = std::fs::read_to_string(crate::paths::keymap_file()) else {
        return BTreeMap::new();
    };

    match parse(&text) {
        Ok(pairs) => pairs
            .into_iter()
            .map(|(stroke, action)| (action, stroke))
            .collect(),
        Err(err) => {
            tracing::warn!(%err, "keymap file not parsed, defaults are in effect");
            BTreeMap::new()
        }
    }
}

/// Is there a keymap file that cannot be read?
///
/// [`overrides`] answers with the defaults when the file will not
/// parse, which is right — a broken keymap must not stop the window
/// opening. But it leaves a person with shortcuts they did not choose
/// and no reason given, and the reason went to a log they are not
/// reading. The window asks this so it can say so out loud.
///
/// A missing file is not unreadable: most people have never written
/// one.
pub fn unreadable() -> bool {
    match std::fs::read_to_string(crate::paths::keymap_file()) {
        Ok(text) => parse(&text).is_err(),
        Err(_) => false,
    }
}

/// Write a person's own shortcuts.
///
/// Only what differs from the defaults is written. A file listing
/// every action would have to be migrated whenever a default changes,
/// and a person who never touched a shortcut would silently keep the
/// old one.
pub fn save(overrides: &BTreeMap<Named, String>) -> std::io::Result<()> {
    let path = crate::paths::keymap_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let by_stroke: BTreeMap<&str, &str> = overrides
        .iter()
        .map(|(action, stroke)| (stroke.as_str(), action.name()))
        .collect();

    let text = serde_json::to_string_pretty(&by_stroke)
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    std::fs::write(path, text)
}

/// Parse the contents of a keymap file.
///
/// Validates both the action name and the shortcut itself:
/// `KeyBinding::new` panics on a shortcut it cannot parse, and a typo
/// in someone's file would bring the application down at startup.
pub fn parse(text: &str) -> Result<Vec<(String, Named)>, KeymapError> {
    let pairs =
        serde_json::from_str::<BTreeMap<String, String>>(text).map_err(KeymapError::Parse)?;

    pairs
        .into_iter()
        .map(|(stroke, name)| {
            let action = Named::by_name(&name).ok_or(KeymapError::UnknownAction(name))?;
            if gpui::Keystroke::parse(&stroke).is_err() {
                return Err(KeymapError::BadStroke(stroke));
            }
            Ok((stroke, action))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A keymap file from M8.3 binds the Changes panel by its old name,
    /// and the name still reaches the Commit panel.
    #[test]
    fn the_changes_panel_s_old_name_still_binds() {
        let bindings = parse(r#"{"cmd-shift-g":"ShowChangesPanel"}"#).expect("parses");
        assert_eq!(bindings[0].1, Named::ShowCommitPanel);
    }

    /// M12 wrote the column's toggle as `ToggleColumn`; it is the Sidebar
    /// since the person called the old name silly, and a file that binds
    /// it still reaches it.
    #[test]
    fn the_column_s_old_name_still_binds() {
        let bindings = parse(r#"{"cmd-shift-b":"ToggleColumn"}"#).expect("parses");
        assert_eq!(bindings[0].1, Named::ToggleSidebar);
    }

    #[test]
    fn a_keymap_file_is_parsed() {
        let bindings = parse(r#"{"cmd-shift-n":"NewTerminal","ctrl-l":"ClearTerminal"}"#)
            .expect("the keymap parses");

        assert_eq!(bindings.len(), 2);
        assert!(bindings.iter().any(|(_, a)| *a == Named::NewTerminal));
    }

    /// A missing file is not an error: most people have no keymap of
    /// their own, and the application must not report it.
    #[test]
    fn an_empty_keymap_is_fine() {
        assert!(parse("{}").expect("an empty keymap is allowed").is_empty());
    }

    /// A typo in an action name has to be named rather than
    /// swallowed: the person will be hunting for why the shortcut
    /// "does not work".
    #[test]
    fn an_unknown_action_is_named_in_the_error() {
        let err = match parse(r#"{"cmd-j":"FlyAcrossTheSky"}"#) {
            Err(err) => err,
            Ok(_) => panic!("an unknown action must be rejected"),
        };

        assert!(
            err.to_string().contains("FlyAcrossTheSky"),
            "the error must name the culprit: {err}"
        );
    }

    /// A shortcut that does not parse must be rejected **before** the
    /// binding is built: `KeyBinding::new` panics on it, and a typo in
    /// someone's file would bring the application down at startup.
    #[test]
    fn a_broken_stroke_is_rejected_before_it_can_panic() {
        let err = match parse(r#"{"this-is-not-a-shortcut":"CloseTab"}"#) {
            Err(err) => err,
            Ok(_) => panic!("a non-shortcut must be rejected"),
        };

        assert!(err.to_string().contains("this-is-not-a-shortcut"), "{err}");
    }

    /// A corrupted file is an error with a message, not a panic: the
    /// application has to open and work on the defaults.
    #[test]
    fn a_broken_file_is_an_error_not_a_panic() {
        assert!(parse("{this is not json").is_err());
    }

    /// Every action must be findable by its name: otherwise the help
    /// list holds a command that cannot be bound.
    #[test]
    fn every_action_can_be_found_by_its_name() {
        for action in Named::ALL {
            assert_eq!(
                Named::by_name(action.name()),
                Some(*action),
                "action {} cannot be found by name",
                action.name()
            );
        }
    }

    /// Names must not repeat: two actions under one name is a
    /// shortcut that quietly does not work.
    #[test]
    fn names_are_unique() {
        let mut names: Vec<&str> = Named::ALL.iter().map(|a| a.name()).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();

        assert_eq!(count, names.len(), "action names repeat");
    }
}
