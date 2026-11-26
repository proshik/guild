//! Application actions and keyboard shortcuts.
//!
//! An action is a name, not a closure. Everything else is built from the
//! name: a menu item finds its own shortcut, the keymap can be overridden
//! by a file, and in the future a command list can be searched.
//!
//! The default keymap lives here rather than in a file next to the
//! program: the application has to work even if the settings file is
//! corrupted or deleted. The file only **overrides** — see [`keymap`].

pub mod keymap;
pub mod labels;

use gpui::{App, KeyBinding, actions};

/// Open a directory from the recent list.
///
/// Carries the position rather than the path: a menu item holds an action,
/// and a `PathBuf` in it would be a second copy of the list that can drift
/// from the one on disk. The position is resolved against the list at the
/// moment of the click.
///
/// `no_json` because the keymap cannot bind it: "the third recent
/// directory" is not a shortcut anybody wants.
#[derive(Clone, PartialEq, Eq, gpui::Action)]
#[action(namespace = guild, no_json)]
pub struct OpenRecent {
    /// The position in the recent list, newest first.
    pub index: usize,
}

/// Put the focus on a project by its place in the row.
///
/// Carries the position rather than the identifier, for the same reason
/// [`OpenRecent`] does: the position is what a person sees and counts, and
/// an identifier baked into a binding would outlive the project it names.
///
/// `no_json` because the keymap file maps one stroke to one action *name*,
/// and "the third project" is nine names for one action. The strokes are
/// fixed in [`rebind`] and said so there.
#[derive(Clone, PartialEq, Eq, gpui::Action)]
#[action(namespace = guild, no_json)]
pub struct SelectProject {
    /// The position in the project row, counting from zero.
    pub index: usize,
}

/// How many projects get a stroke of their own.
///
/// Nine, because `cmd-0` already resets the font size and because a row
/// with more than nine projects in it is a row nobody is reading by
/// number.
pub const NUMBERED_PROJECTS: usize = 9;

actions!(
    guild,
    [
        /// A new Claude session in the current project.
        NewSession,
        /// A new terminal in the current project's directory.
        NewTerminal,
        /// Close the current tab.
        CloseTab,
        /// Bring back the tab that was closed last.
        ReopenTab,
        /// Close every tab of this project but the one in front.
        CloseOtherTabs,
        /// Close every tab of this project.
        CloseAllTabs,
        /// Write the file in front to disk.
        SaveFile,
        /// Write every edited file to disk.
        SaveAllFiles,
        /// Read the file in front from disk again, dropping what was
        /// typed — after asking, if anything was.
        ReloadFile,
        /// Select the file in front in the tree, and focus the tree.
        RevealFile,
        /// The diff of the file in front against the last commit.
        ShowDiff,
        /// The diff of the working tree: every changed file.
        ShowChanges,
        /// Commit what is staged, with the message in the box.
        Commit,
        /// Who wrote each line of the file in front, and when.
        Annotate,
        /// The diff's next hunk.
        NextChange,
        /// The diff's previous hunk.
        PreviousChange,
        /// The shown change into the index: the diff's current change, or
        /// the block under the editor's caret. GLD-026.
        StageHunk,
        /// The shown change back out of the index.
        UnstageHunk,
        /// Ask git again for the changes.
        RefreshChanges,
        /// The diff of the selected change.
        ShowSelectedDiff,
        /// Every change into the commit.
        IncludeSelectedChanges,
        ExcludeSelectedChanges,
        TickAllChanges,
        /// Nothing into the commit.
        UntickAllChanges,
        /// Fold both sections of the Commit panel.
        CollapseChanges,
        /// Unfold them.
        ExpandChanges,
        /// The dock that has the Explorer, on it — or closed, if it is up.
        ShowExplorerPanel,
        /// The dock that has the Commit panel, on it.
        ShowCommitPanel,
        /// The dock that has the Log, on it.
        ShowLogPanel,
        /// The dock that has the Output, on it.
        ShowOutputPanel,
        ShowTerminalPanel,
        /// Find text across the project, in a popup with a preview. M8.7
        /// made it the dock; since M8.9 it is the popup, and the dock is
        /// [`ShowSearchPanel`] — a keymap file that binds this name meant
        /// Find in Files.
        FindInFiles,
        ReplaceInFiles,
        /// The dock that has Search, on it, with the field ready for a
        /// word. `FindInFiles` until M8.9.
        ShowSearchPanel,
        /// Run the last command again — or, with none chosen yet, open
        /// the list of what this project can run.
        Run,
        /// Stop the command that is running.
        StopRun,
        /// Empty the Output panel's screen.
        ClearRun,
        /// Go to where the name under the caret is defined.
        GoToDefinition,
        /// Explorer to the left dock.
        MoveExplorerToLeft,
        /// Explorer to the right dock.
        MoveExplorerToRight,
        /// Explorer to the bottom dock.
        MoveExplorerToBottom,
        /// Commit to the left dock.
        MoveCommitToLeft,
        /// Commit to the right dock.
        MoveCommitToRight,
        /// Commit to the bottom dock.
        MoveCommitToBottom,
        /// Log to the left dock.
        MoveLogToLeft,
        /// Log to the right dock.
        MoveLogToRight,
        /// Log to the bottom dock.
        MoveLogToBottom,
        /// Output to the left dock.
        MoveOutputToLeft,
        /// Output to the right dock.
        MoveOutputToRight,
        /// Output to the bottom dock.
        MoveOutputToBottom,
        /// The next tab.
        NextTab,
        /// The previous tab.
        PreviousTab,
        /// Add a project — an existing directory.
        AddProject,
        OpenSelectedApp,
        SelectOpenApp,
        /// Create a project: a new directory, then open it.
        NewProject,
        GetFromVersionControl,
        /// Close the active project.
        CloseProject,
        /// Copy the active worktree folder as an absolute path.
        CopyWorktreePath,
        /// Ask to remove the active non-primary worktree.
        RemoveWorktree,
        /// Open the active worktree's merge request in the browser —
        /// GLD-063.
        ViewMergeRequest,
        /// What this is and which version of it.
        About,
        /// Rebuild the Guild you work in from `main`, and restart into it.
        /// M5.13.
        UpdateGuild,
        /// The next project.
        NextProject,
        /// The project before this one in the row, wrapping — the other
        /// half of the `ctrl-tab` cycle.
        PreviousProject,
        /// The workspace to the right, wrapping at the end.
        NextWorkspace,
        /// The workspace to the left, wrapping at the beginning.
        PreviousWorkspace,
        /// Divide the focused area, the new one to the right — M12.8.
        SplitAreaRight,
        /// Divide the focused area, the new one below it — M12.8.
        SplitAreaDown,
        /// Divide the focused pane of the tab in front, the new, empty
        /// one to the right — M12.8, decision 13.
        SplitPaneRight,
        /// Divide the focused pane, the new one below it — M12.8.
        SplitPaneDown,
        /// Close the focused area; its tabs go to the area that takes its
        /// room. The last area stays — M12.8.
        CloseArea,
        /// The keys to the place on the left — M12.8.
        FocusLeft,
        /// The keys to the place on the right.
        FocusRight,
        /// The keys to the place above.
        FocusUp,
        /// The keys to the place below.
        FocusDown,
        // ── Declared by M8.8.0 for the branches that build them. ──────
        //
        // Each exists, is bound, and does nothing until its branch wires
        // a handler. They are here rather than in six branches because
        // this file and the keymap's list are what those branches would
        // otherwise all edit.
        /// The place visited before this one. M8.8.2.
        Back,
        /// Forward again, after `Back`. M8.8.2.
        Forward,
        /// The files visited lately, as a list to choose from. M8.8.2.
        RecentFiles,
        /// The places visited lately, with the code around each. M8.8.2.
        RecentLocations,
        /// Back to where the last edit was made. M8.8.2.
        LastEditLocation,
        /// A line and a column in the file in front. M8.8.2.
        GoToLine,
        /// The tabs the strip cannot show. M8.8.1.
        ShowTabList,
        /// Send the branch's commits to its remote. M8.8.4.
        Push,
        /// Throw away what changed in the selected files. M8.8.4.
        Rollback,
        /// Start tracking the selected files. M8.8.4.
        AddToVcs,
        /// A new file beside the selection in the tree. M8.8.3.
        NewFile,
        /// A new directory beside the selection in the tree. M8.8.3.
        NewDirectory,
        /// Rename what is selected in the tree. M8.8.3.
        RenameFile,
        MoveFile,
        CutFiles,
        /// Open the file selected in the tree, and go to it — *Jump to
        /// Source*.
        JumpToSource,
        /// Move what is selected in the tree to the Trash. M8.8.3.
        DeleteFile,
        /// The dock that has Problems, on it. M8.8.5.
        ShowProblems,
        /// Where the name under the caret is used, in the Find tool —
        /// *Find Usages*. M8.15.
        FindUsages,
        /// The dock that has the Find tool, on it. M8.15.
        ShowFindPanel,
        /// The Find tool's next usage, opened — *Next Occurrence*.
        NextUsage,
        /// The Find tool's previous usage, opened.
        PreviousUsage,
        /// The list of directories opened before, to pick one from.
        RecentProjects,
        /// Show the session list.
        ToggleSessions,
        ToggleTerminalJobs,
        ToggleUsage,
        /// Bring the session that needs the person to the front: the
        /// blocked first, then the finished, each longest waiting first.
        /// M5.10.
        GoToWaitingSession,
        /// Open the settings.
        OpenSettings,
        /// Close the panel that is over the tab — settings or the session
        /// list.
        ClosePanel,
        /// The diff window's next file — M8.3.6.
        NextDiffFile,
        /// The diff window's previous file.
        PreviousDiffFile,
        /// Hide the sidebar — the projects and their worktrees down the
        /// left — or bring it back. M12; `ToggleColumn` until M12.3, and a
        /// keymap file's old name still reaches it.
        ToggleSidebar,
        /// Show or hide the project panel on the left.
        ToggleLeftPanel,
        /// Show or hide the panel on the right.
        ToggleRightPanel,
        /// Show or hide the output panel along the bottom.
        ToggleBottomPanel,
        /// Make the panel last opened bigger.
        WidenPanel,
        /// Make it smaller.
        NarrowPanel,
        /// Move the focused panel's border left — Stretch to Left: the
        /// right panel grows, the left one shrinks. M12.3.
        StretchToLeft,
        /// Move it right.
        StretchToRight,
        /// Move the bottom panel's border up.
        StretchToTop,
        /// Move it down.
        StretchToBottom,
        /// Forget the list of recently opened directories.
        ClearRecent,
        /// Show the log.
        ShowLog,
        /// Fill the screen, and back.
        ToggleFullScreen,
        /// Larger.
        IncreaseFontSize,
        /// Smaller.
        DecreaseFontSize,
        /// Back to the original font size.
        ResetFontSize,
        /// Copy the selection.
        Copy,
        /// Paste.
        Paste,
        /// Select everything the thing in front holds — a terminal's
        /// history and screen. M5.12.
        SelectAll,
        /// Find in the focused file or terminal, using that host's local search.
        Find,
        /// Replace text in an editable file, using the editor's local search.
        Replace,
        /// The next match of an open find bar, further back. Bound only
        /// in `terminal`, in `ordered`'s context-bound block. M5.12.
        FindNext,
        /// The previous match, further down. M5.12.
        FindPrevious,
        /// One popup for files, symbols, actions and text — a double
        /// Shift, detected rather than bound. M8.9.
        SearchEverywhere,
        /// Search Everywhere on its Files tab: a file by a few of its
        /// letters. M8.9.
        GoToFile,
        /// Search Everywhere on its Symbols tab: a function, a type or a
        /// module by its name. M8.9.
        GoToSymbol,
        /// Search Everywhere on its Actions tab: anything the window can
        /// do, by its name. M8.9.
        FindAction,
        /// Clear the terminal.
        ClearTerminal,
        /// Quit the application.
        Quit,
        /// Hide Guild.
        Hide,
        /// Hide the other applications.
        HideOthers,
        /// Show all applications.
        ShowAll,
    ]
);

/// The stroke bound to an action **now**, not its default.
///
/// A person who rebound it is exactly the person a default would mislead,
/// and every place that shows a shortcut — a tooltip, the hints on an
/// empty screen — has to show the same one the keyboard obeys.
pub fn stroke_for(action: keymap::Named) -> Option<String> {
    effective()
        .into_iter()
        .find(|(_, bound)| *bound == action)
        .map(|(stroke, _)| stroke)
}

/// A stroke as a person reads it rather than as the keymap writes it.
///
/// `cmd-shift-right` is how it is stored and `⌘⇧→` is how it is printed on
/// every other macOS surface a person has seen.
///
/// **A sequence is split on its spaces first**, stroke by stroke: Search
/// Everywhere's double Shift is `shift shift` (`double_tap::DOUBLE_SHIFT`),
/// and split on hyphens alone it read as one key named *Shift shift*.
pub fn printed(stroke: &str) -> String {
    stroke
        .split(' ')
        // A literal minus is the final empty part of GPUI's `cmd--`.
        // Omit the terminator so it contributes exactly one key glyph.
        .flat_map(|stroke| stroke.split_terminator('-'))
        .map(|part| match part {
            "" => "-",
            "cmd" => "⌘",
            "shift" => "⇧",
            "alt" => "⌥",
            "ctrl" => "⌃",
            "left" => "←",
            "right" => "→",
            "up" => "↑",
            "down" => "↓",
            other => other,
        })
        .map(str::to_owned)
        .map(shout)
        .collect::<Vec<_>>()
        .join("")
}

/// A key as it is printed on the key.
///
/// One letter is engraved in capitals; a word is not — `ESCAPE` shouts
/// where the keyboard says `Escape`.
fn shout(key: String) -> String {
    if key.chars().count() == 1 {
        return key.to_uppercase();
    }
    let mut chars = key.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => key,
    }
}

/// The default shortcuts: the one place a stroke is tied to an action.
///
/// The bindings and the list shown in the settings are both built from
/// this, so the settings cannot advertise a shortcut that is bound to
/// nothing — which is the shortcut a person then reports as broken.
///
/// The strokes are the ones people already know from other environments:
/// `Cmd+T` opens a tab, `Cmd+W` closes one, `Cmd+,` leads to the settings.
/// Inventing our own would mean making people relearn for nothing.
fn defaults() -> Vec<(&'static str, keymap::Named)> {
    use keymap::Named;

    vec![
        ("cmd-t", Named::NewSession),
        // A terminal moves off cmd-shift-t so that the stroke can mean
        // what it means in every editor and every browser: bring back the
        // tab that was just closed. Reaching for it and getting a new tab
        // instead is exactly what went wrong.
        ("cmd-alt-t", Named::NewTerminal),
        ("cmd-shift-t", Named::ReopenTab),
        ("cmd-w", Named::CloseTab),
        ("cmd-s", Named::SaveFile),
        ("cmd-alt-s", Named::SaveAllFiles),
        // The common IDE stroke for Show Diff; the working tree's beside it.
        ("cmd-d", Named::ShowDiff),
        ("cmd-shift-d", Named::ShowChanges),
        // The common IDE strokes for the next and the previous change in a
        // diff.
        ("f7", Named::NextChange),
        ("shift-f7", Named::PreviousChange),
        // **The digits are the tool windows**, the common IDE
        // arrangement and what was asked for by name. They were the nine
        // projects until M8.8; the projects are a cycle now, below.
        // `cmd-6` for Problems is the number other IDEs give it too,
        // arrived at by counting rather than by copying, which is a
        // coincidence worth taking.
        ("cmd-1", Named::ShowExplorerPanel),
        ("cmd-k", Named::ShowCommitPanel),
        ("cmd-9", Named::ShowLogPanel),
        ("cmd-4", Named::ShowOutputPanel),
        ("alt-f12", Named::ShowTerminalPanel),
        // `cmd-5` was the Search *tool* until 2026-09-23. It has no stroke
        // and no button now: the Find window, reached from the popup's
        // *Open in Find Window*. `ShowSearchPanel` stays as a name a
        // keymap file may bind.
        ("cmd-6", Named::ShowProblems),
        // Find uses Cmd+3; Git Log uses Cmd+9.
        ("cmd-3", Named::ShowFindPanel),
        // **The `cmd-shift-` trio retires with the digits it duplicates.**
        // Keeping it was the first instinct — a person has had `cmd-shift-e`
        // for the Explorer since M5.7 — and it is the wrong trade: those
        // three letters are ones the common IDE keymap spends elsewhere, and
        // `cmd-shift-e` in particular is Recent Locations, which this
        // milestone is building. Two strokes for a tool that already has a
        // digit is not worth a letter a different gesture needs.
        //
        // `cmd-shift-f` stays, because it is **the common** stroke for
        // Find in Files rather than a duplicate of `cmd-5`.
        ("cmd-shift-f", Named::FindInFiles),
        ("cmd-shift-r", Named::ReplaceInFiles),
        ("cmd-shift-]", Named::NextTab),
        ("cmd-shift-[", Named::PreviousTab),
        // **The projects take the tab cycle.** `cmd-tab` was asked for
        // and cannot be had: task 0 measured it — the window server's
        // switcher runs and the application never sees the event, while
        // `ctrl-tab` arrives. It was `NextTab`'s second stroke and
        // `NextTab` keeps its pair of brackets above.
        ("ctrl-tab", Named::NextProject),
        ("ctrl-shift-tab", Named::PreviousProject),
        // Workspaces sit above projects. Three modifiers leave the editor's
        // ordinary and selection arrows untouched.
        ("cmd-shift-alt-right", Named::NextWorkspace),
        ("cmd-shift-alt-left", Named::PreviousWorkspace),
        // **The areas and panes — M12.8, the person's strokes.** `ctrl-cmd`
        // is the window's own family (`ctrl-cmd-f` is full screen), free
        // in the component — whose one `ctrl-cmd` binding is the
        // character palette's space — and in the terminal, which forwards
        // no platform key. The arrows walk every place by direction; the
        // shifted pair divides the area, and `w` beside them closes it.
        ("ctrl-cmd-left", Named::FocusLeft),
        ("ctrl-cmd-right", Named::FocusRight),
        ("ctrl-cmd-up", Named::FocusUp),
        ("ctrl-cmd-down", Named::FocusDown),
        ("ctrl-cmd-shift-right", Named::SplitAreaRight),
        ("ctrl-cmd-shift-down", Named::SplitAreaDown),
        // The tab's pair, `alt` for the level in: the area splits with
        // shift, the tab inside it with option — decision 14.
        ("ctrl-cmd-alt-right", Named::SplitPaneRight),
        ("ctrl-cmd-alt-down", Named::SplitPaneDown),
        // Closing an area, the same family — the person's, 2026-09-30,
        // after trying the window: the area deeds are one family.
        ("ctrl-cmd-shift-w", Named::CloseArea),
        ("cmd-o", Named::OpenSelectedApp),
        ("cmd-shift-n", Named::NewProject),
        // The approved Open In pair; AddProject and GoToFile remain remappable.
        ("cmd-shift-o", Named::SelectOpenApp),
        ("cmd-alt-shift-o", Named::RecentProjects),
        ("cmd-alt-o", Named::GoToSymbol),
        // Find Action. Search Everywhere has no stroke here: it is
        // a double Shift, which the window detects rather than binds.
        ("cmd-shift-a", Named::FindAction),
        ("cmd-shift-s", Named::ToggleSessions),
        ("ctrl-cmd-j", Named::ToggleTerminalJobs),
        ("ctrl-cmd-u", Named::ToggleUsage),
        // M5.10: to whoever is waiting. No environment a person comes from
        // has this gesture, so no stroke is anybody's habit; `u` is free,
        // and it sits beside `s` for the session list it is a shortcut
        // through.
        ("cmd-shift-u", Named::GoToWaitingSession),
        ("cmd-,", Named::OpenSettings),
        ("cmd-=", Named::IncreaseFontSize),
        ("cmd-+", Named::IncreaseFontSize),
        ("cmd--", Named::DecreaseFontSize),
        ("cmd-0", Named::ResetFontSize),
        // The column folds beside the panels, one stroke along from the
        // left dock's: they are the two things down that edge, and a
        // person reaching for one has the other under the same finger.
        // M12.
        ("cmd-shift-b", Named::ToggleSidebar),
        ("cmd-b", Named::ToggleLeftPanel),
        ("cmd-l", Named::ToggleRightPanel),
        ("cmd-j", Named::ToggleBottomPanel),
        // The arrows rather than brackets: cmd-shift-[ and cmd-shift-] are
        // already the tab strokes, and a stroke that means two things means
        // neither.
        ("cmd-shift-right", Named::WidenPanel),
        ("cmd-shift-left", Named::NarrowPanel),
        // The common macOS IDE strokes for Stretch to Left, Right, Top and
        // Bottom. They act on the panel with the focus and
        // on nothing else: with a tab focused no handler is on the path,
        // and the stroke goes on to the program in it.
        ("ctrl-alt-left", Named::StretchToLeft),
        ("ctrl-alt-right", Named::StretchToRight),
        ("ctrl-alt-up", Named::StretchToTop),
        ("ctrl-alt-down", Named::StretchToBottom),
        // Cmd+R belongs to local Replace; Run remains available as an action.
        ("cmd-r", Named::Replace),
        // The common second stroke for it. `cmd-b`, the usual first, is
        // the left panel's here, and a stroke that means two things means
        // neither.
        ("f12", Named::GoToDefinition),
        // Find Usages and its Next and Previous Occurrence, all three as
        // the common macOS IDE keymap has them. M8.15.
        ("alt-f7", Named::FindUsages),
        ("cmd-alt-down", Named::NextUsage),
        ("cmd-alt-up", Named::PreviousUsage),
        // Cmd+K now toggles the Commit tool through ShowCommitPanel above.
        // The Commit action remains available for explicit user overrides.
        // Clearing moves one modifier further out: `cmd-shift-k` is
        // Push in the common IDE keymap, which M8.8.4 builds, and a person
        // used to it presses it meaning that. The shell's own `ctrl-l` still
        // clears and still reaches the program untouched.
        ("cmd-shift-alt-k", Named::ClearTerminal),
        ("ctrl-cmd-f", Named::ToggleFullScreen),
        // Copy and Paste, for the whole window. Since M5.12 a terminal
        // handles both as actions on its own element — `terminal/view/
        // input.rs`'s `bind_keys` — which is also what enables the Edit
        // menu's items over it, and its find bar takes both for its query.
        // In a file the editor's own `cmd-c` and `cmd-v` still win: they
        // are bound in `Input`, deeper than these.
        ("cmd-c", Named::Copy),
        ("cmd-v", Named::Paste),
        // Select All stays with the component in Input. Local Find is
        // handled by FileTab or the terminal; ordered() gives FileTab its
        // own scoped binding so custom Find shortcuts reach the same host.
        ("cmd-a", Named::SelectAll),
        ("cmd-f", Named::Find),
        // ── The twenty-nine that had none. M8.8's request 13. ─────────
        //
        // An action with no stroke is a menu item that looks unfinished
        // and a gesture a hand cannot reach; every one of these has been
        // in the window for passes without one. The common IDE stroke
        // where there is one for the same thing, ours where there is not,
        // and the reason on the row wherever the two differ.
        //
        // The tabs. The common two, and `cmd-alt-w` for the third
        // because the common *close all* on macOS is
        // `ctrl-shift-cmd-w` — three modifiers for a thing pressed with
        // one hand while the other is on the trackpad.
        ("cmd-shift-w", Named::CloseOtherTabs),
        ("cmd-alt-w", Named::CloseAllTabs),
        // Reload remains a menu/custom-keymap action: Cmd+Shift+R is
        // explicitly assigned to Replace in Files. Reveal uses Finder's stroke.
        ("cmd-alt-r", Named::RevealFile),
        // Annotate has no common default stroke; this is the letter of
        // the word, and `cmd-alt-a` is the common *Add to VCS*, which
        // M8.8.4 takes.
        ("cmd-alt-b", Named::Annotate),
        // The Commit panel. Its refresh is `cmd-alt-y` — the common
        // *Update Project* family — and nothing else here has a common
        // stroke at all, so these are ours and they are near the panel's
        // own `cmd-k`.
        ("cmd-alt-y", Named::RefreshChanges),
        ("cmd-alt-d", Named::ShowSelectedDiff),
        ("cmd-alt-i", Named::TickAllChanges),
        ("cmd-alt-u", Named::UntickAllChanges),
        // The two folds, the common pair for a tree.
        ("cmd-alt--", Named::CollapseChanges),
        ("cmd-alt-=", Named::ExpandChanges),
        // Run has no default shortcut; the two beside it stop and clear
        // it, and the common Stop is `cmd-f2` — a function key on a laptop
        // that needs a modifier to be a function key at all.
        ("cmd-alt-period", Named::StopRun),
        ("cmd-alt-k", Named::ClearRun),
        // The Log's face. `ShowLog` is the Git tool with the log shown,
        // which `cmd-9` already reaches; this is the second door and it
        // is the View menu's item made reachable rather than a new
        // gesture.
        ("cmd-alt-l", Named::ShowLog),
        // Closing a project. Deliberately **not** `cmd-shift-w`, which
        // is *close the other tabs* above: closing a project stops every
        // session in it, and a stroke one shift away from a tab gesture
        // is a stroke that will one day be pressed by mistake.
        ("cmd-alt-shift-w", Named::CloseProject),
        ("cmd-shift-c", Named::CopyWorktreePath),
        ("cmd-alt-shift-r", Named::RemoveWorktree),
        // View MR (GLD-063); bound to nothing before.
        ("cmd-alt-g", Named::ViewMergeRequest),
        // The twelve `Move*To*`. These stay without strokes, and that is
        // a decision rather than an oversight: twelve strokes for a
        // gesture done once a month would spend twelve of the letters
        // left, and the View menu and a drag both already reach them.
        // The keymap file is where anyone who moves docks daily binds
        // the three they use.
        //
        // ── What the six branches of M8.8 will need. ──────────────────
        //
        // Declared here, with their strokes, **before** the branches are
        // cut. Each is bound to an action that exists and does nothing
        // yet; its handler arrives with the branch that owns it. The
        // alternative is six branches editing this list and the keymap's
        // one, which is the collision this whole commit exists to avoid
        // — and a stroke chosen twice is a test that passes on every
        // branch alone and fails on the merge.
        //
        // Navigating — M8.8.2. The common four.
        //
        // **Both common pairs, and the arrows first — M8.10.** The common
        // macOS IDE keymap carries `cmd-[` and `cmd-alt-left` for the same
        // action, and on some layouts the first cannot be typed at all:
        // on a Russian one that key makes `х`. The first
        // binding is the one the menu bar and a tooltip show, so the one
        // a hand can reach goes first. An override still replaces every
        // default of its action, so a person who binds Back elsewhere
        // loses both at once, which is what the settings then show.
        ("cmd-alt-left", Named::Back),
        ("cmd-alt-right", Named::Forward),
        ("cmd-[", Named::Back),
        ("cmd-]", Named::Forward),
        ("cmd-e", Named::RecentFiles),
        ("cmd-shift-e", Named::RecentLocations),
        ("cmd-shift-backspace", Named::LastEditLocation),
        // **Not the common `cmd-l`**, which is the right panel here and has
        // been since M5.7 — one third of the `cmd-b`/`cmd-l`/`cmd-j`
        // family a person presses dozens of times a day. `cmd-g` is free,
        // is one letter from the word, and is what `$default` means by
        // *goto* before the macOS keymap moves it.
        ("cmd-g", Named::GoToLine),
        // Git — M8.8.4. Both the common IDE strokes.
        ("cmd-shift-k", Named::Push),
        ("cmd-alt-z", Named::Rollback),
        ("cmd-alt-a", Named::AddToVcs),
        // Explorer file operations: explicit user decision supersedes M8.8.3's
        // unbound Rename/Delete rationale. These are scoped to list focus below.
        ("cmd-n", Named::NewFile),
        ("cmd-alt-n", Named::NewDirectory),
        ("shift-f6", Named::RenameFile),
        ("f6", Named::MoveFile),
        ("cmd-x", Named::CutFiles),
        ("cmd-backspace", Named::DeleteFile),
        ("f4", Named::JumpToSource),
    ]
}

/// The shortcuts in force: the defaults, with a person's own on top.
///
/// An override **replaces** an action's defaults rather than adding to
/// them. Adding would leave the old stroke working too, while the settings
/// showed only the new one — a shortcut that does something the interface
/// does not admit to. The new Open In defaults also yield to explicit strokes
/// for existing actions, preserving user choices without changing other contexts.
pub fn effective() -> Vec<(String, keymap::Named)> {
    effective_with(keymap::overrides())
}

fn effective_with(
    overrides: std::collections::BTreeMap<keymap::Named, String>,
) -> Vec<(String, keymap::Named)> {
    let mut bindings: Vec<(String, keymap::Named)> = defaults()
        .into_iter()
        .filter(|(stroke, action)| {
            !overrides.contains_key(action)
                && !(matches!(
                    action,
                    keymap::Named::OpenSelectedApp
                        | keymap::Named::SelectOpenApp
                        | keymap::Named::Find
                        | keymap::Named::Replace
                        | keymap::Named::FindInFiles
                        | keymap::Named::ReplaceInFiles
                ) && overrides.values().any(|custom| {
                    gpui::Keystroke::parse(custom).ok() == gpui::Keystroke::parse(stroke).ok()
                }))
        })
        .map(|(stroke, action)| (stroke.to_owned(), action))
        .collect();

    bindings.extend(
        overrides
            .into_iter()
            .map(|(action, stroke)| (stroke, action)),
    );
    bindings
}

/// Install the shortcuts in force, replacing whatever was bound before.
///
/// Called again whenever a person changes one: a shortcut that only takes
/// effect after a restart is a shortcut people believe is broken.
pub fn rebind(cx: &mut App) {
    cx.clear_key_bindings();
    let inherited = cx
        .try_global::<Inherited>()
        .map(|inherited| inherited.0.clone())
        .unwrap_or_default();
    cx.bind_keys(ordered(effective(), &inherited));
}

/// The bindings that were in the keymap before Guild bound its own — the
/// editor component's, and the `NoAction` unbinds `editor::init` lays over
/// them — taken once, when `editor::init` has run.
///
/// **Kept so that every rebind can put them back.** `rebind` has to clear
/// the keymap to drop a stroke a person moved, and until M5.11 it bound
/// only ours after the clear: the first click on a Settings slot took
/// Backspace, Enter and the arrows out of every editor until a restart.
pub struct Inherited(pub Vec<KeyBinding>);

impl gpui::Global for Inherited {}

/// The actions whose stroke wins over the editor's own inside an input.
/// Find in Files must not become the component's local Replace. File-local
/// Find/Replace are separately scoped to FileTab below.
///
/// The common IDE keymap gives `cmd-[` to Back in the editor, and the
/// component's Outdent there turned a navigation into an edit that autosave
/// wrote 300 ms later. A person who gives Back a stroke the editor also uses
/// has chosen Back. Everything else in a buffer stays the component's —
/// `cmd-c` and `cmd-v` included: Guild's Copy and Paste are handled by a
/// terminal and its find bar (M5.12), and in a buffer they are the
/// component's to answer.
///
/// **Their second binding is bound in the buffer's own context**, `Input`,
/// and not at full depth: it comes after every inherited binding, and at
/// full depth it would also beat whatever the component's menus, lists
/// and dialogs — and a context-bound binding of ours — put on the stroke a
/// person gives Back. The menu bar still shows the first binding, the one
/// with no context: GPUI's macOS menu takes the earliest that matches.
const WIN_IN_A_BUFFER: &[keymap::Named] = &[
    keymap::Named::Back,
    keymap::Named::Forward,
    keymap::Named::FindInFiles,
    keymap::Named::ReplaceInFiles,
];

/// Where [`WIN_IN_A_BUFFER`]'s second binding matches: in a buffer, and not
/// while a stroke is being recorded.
const BUFFER_NOT_RECORDING: &str = "Input && !recording";

/// The whole keymap, in the order that decides who wins a stroke.
///
/// GPUI ranks the bindings that match by the depth of the context they
/// matched at, and two of one depth by the order they were bound — the
/// later wins. A `!recording` binding of ours matches at full depth
/// everywhere, and so does a binding with no context at all, so what
/// decides a tie is **the order, by level**:
///
/// 1. the inherited bindings at the window's own level — no context, or
///    `Root` or `guild` — among them the inspector's `cmd-alt-i` of a debug
///    build and our own `NoAction` unbinds; bound first, so that ours beat
///    them, and `cmd-alt-i` is `TickAllChanges` with the root's handle
///    focused rather than a `NoAction` that does nothing;
/// 2. ours, with `!recording`;
/// 3. the context-bound and system ones;
/// 4. the inherited bindings of every deeper context — `Input`, the
///    component's menus and lists — after ours, which is what lets a
///    buffer keep its keys;
/// 5. ours again for [`WIN_IN_A_BUFFER`], after those, in `Input` alone.
///
/// M5.11's decision 12.
pub fn ordered(
    effective: Vec<(String, keymap::Named)>,
    inherited: &[KeyBinding],
) -> Vec<KeyBinding> {
    let (window_level, deeper): (Vec<&KeyBinding>, Vec<&KeyBinding>) = inherited
        .iter()
        .partition(|binding| at_the_windows_level(binding));

    let mut bindings: Vec<KeyBinding> = window_level.into_iter().cloned().collect();

    // While a shortcut is being recorded none of them may fire: the stroke
    // being pressed is the answer to a question, not a command. The
    // predicate is what makes that possible without a special case in the
    // event handler.
    bindings.extend(effective.iter().map(|(stroke, action)| {
        action.binding(
            stroke,
            Some(if *action == keymap::Named::Replace {
                "!(!guild-file) && !recording && !guild-field"
            } else if matches!(
                action,
                keymap::Named::NewFile
                    | keymap::Named::NewDirectory
                    | keymap::Named::RenameFile
                    | keymap::Named::MoveFile
                    | keymap::Named::CutFiles
            ) {
                "tree && !recording && !Input && !guild-field"
            } else if *action == keymap::Named::DeleteFile {
                "(tree || changes) && !recording && !Input && !guild-field"
            } else if matches!(
                action,
                keymap::Named::Rollback
                    | keymap::Named::AddToVcs
                    | keymap::Named::IncludeSelectedChanges
                    | keymap::Named::ExcludeSelectedChanges
            ) {
                "changes && !recording && !Input && !guild-field"
            } else {
                "!recording"
            }),
        )
    }));

    // Escape closes the panel — but only while a panel is open. Bound in
    // the panel's own context, because in a terminal Escape belongs to the
    // program: it is how one leaves insert mode in `vim`. It is not in
    // `defaults` because a context-bound stroke cannot be expressed in the
    // keymap file, and offering it there would be a promise we do not
    // keep.
    bindings.push(KeyBinding::new("escape", ClosePanel, Some("panel")));
    // A diff owns Escape while selecting or resizing; its key listener cancels
    // the gesture before a containing panel can consume the stroke.
    bindings.push(KeyBinding::new(
        "escape",
        gpui::NoAction {},
        Some("diff-gesture"),
    ));
    // **…and not over a popup's card.** A popup reads its own Escape
    // (M8.9's decision 2), and a binding is matched before any key
    // listener: with the session list up, Escape over Recent Projects
    // closed the list behind the popup and left the popup standing. A
    // `NoAction` in the deeper context takes the stroke back for the
    // card's own handler.
    bindings.push(KeyBinding::new("escape", gpui::NoAction {}, Some("picker")));

    // A clone form owns its keys, including user-defined background shortcuts.
    for stroke in effective.iter().map(|(stroke, _)| stroke.as_str()).chain([
        "tab",
        "shift-tab",
        "enter",
        "escape",
        "backspace",
        "alt-backspace",
        "cmd-backspace",
        "ctrl-w",
        "ctrl-u",
        "cmd-v",
    ]) {
        bindings.push(KeyBinding::new(
            stroke,
            gpui::NoAction {},
            Some("clone-project"),
        ));
    }

    // Settings search owns editing strokes, even if a keymap overrides them.
    for stroke in [
        "escape",
        "enter",
        "backspace",
        "alt-backspace",
        "cmd-backspace",
        "ctrl-w",
        "ctrl-u",
        "cmd-v",
    ] {
        bindings.push(KeyBinding::new(
            stroke,
            gpui::NoAction {},
            Some("settings-search"),
        ));
    }

    // **Next and previous match in a terminal's find bar** — M5.12's
    // decision 10. `cmd-g` is `GoToLine`'s everywhere else, and in a
    // terminal it has to step the find instead. GPUI ranks what matched by
    // depth, then by order, the later winning: a `!recording` binding
    // matches at the full depth of the focus's contexts, and so does a
    // `terminal` one while `terminal` is the innermost — so bound **here,
    // after ours**, it wins. That is also why the find bar names no
    // context of its own: under one, `terminal` would match a level
    // shallower and `GoToLine` would take the stroke and do nothing in a
    // terminal. Not in `defaults` for `ClosePanel`'s reason: the keymap
    // file cannot say a context.
    bindings.push(KeyBinding::new("cmd-g", FindNext, Some("terminal")));
    // **The diff window's files, the common IDE strokes** — M8.3.6, read
    // off a macOS keymap. Only in that window: elsewhere `ctrl-shift-←` and
    // `→` are the editor's and the terminal's.
    bindings.push(KeyBinding::new(
        "ctrl-shift-right",
        NextDiffFile,
        Some("diff-window"),
    ));
    bindings.push(KeyBinding::new(
        "ctrl-shift-left",
        PreviousDiffFile,
        Some("diff-window"),
    ));
    bindings.push(KeyBinding::new(
        "cmd-shift-g",
        FindPrevious,
        Some("terminal"),
    ));

    // **The digits were the projects and are the tools now** — M8.8's
    // request 9, and the common IDE arrangement. `SelectProject` keeps its
    // action and its menu items: the project row is still clickable and
    // the numbered form is still what a menu item dispatches. What it
    // has lost is nine strokes it was holding, which the six tools
    // needed and which a person asked for by name.
    //
    // The cycle that replaces them is `ctrl-tab`, in `defaults` above,
    // because it maps a stroke to one action rather than to nine
    // positions of one — which is the whole reason the numbered form
    // could not live there.

    // The system's own, which are not the person's to rebind: an
    // application that cannot be quit by cmd-q is broken, however the
    // keymap file is written.
    bindings.push(KeyBinding::new("cmd-q", Quit, None));
    bindings.push(KeyBinding::new("cmd-h", Hide, None));
    bindings.push(KeyBinding::new("cmd-alt-h", HideOthers, None));

    bindings.extend(deeper.into_iter().cloned());
    // The line-editing actions in an editable file's own `Input`: after the
    // component's, so they win there, and before every binding a person's
    // keymap makes win inside a buffer, so those still do.
    bindings.extend(crate::editor::editing_bindings());
    bindings.extend(
        effective
            .iter()
            .filter(|(_, action)| WIN_IN_A_BUFFER.contains(action))
            .map(|(stroke, action)| {
                let context = if matches!(
                    action,
                    keymap::Named::FindInFiles | keymap::Named::ReplaceInFiles
                ) {
                    "Input && !recording && !clone-project"
                } else {
                    BUFFER_NOT_RECORDING
                };
                action.binding(stroke, Some(context))
            }),
    );
    // File adapters own local Find/Replace, including user-defined strokes.
    // Dialog inputs and terminal Find fields keep their component editing keys.
    bindings.extend(effective.iter().filter_map(|(stroke, action)| {
        let context = match action {
            keymap::Named::Find => {
                "(guild-file > Input || guild-readonly-file > Input) && !recording && !guild-field"
            }
            keymap::Named::Replace => "guild-file > Input && !recording && !guild-field",
            _ => return None,
        };
        Some(action.binding(stroke, Some(context)))
    }));
    bindings.extend(
        effective
            .iter()
            .filter(|(_, action)| matches!(action, keymap::Named::Find | keymap::Named::Replace))
            .map(|(stroke, action)| {
                action.binding(stroke, Some("guild-local-search > Input && !recording"))
            }),
    );
    bindings.extend(crate::text_field::bindings());
    // Source selection overrides the component document-movement bindings.
    bindings.extend(crate::editor::selection_bindings());
    // A multiline search query keeps newline editing but gives host navigation
    // back to its bar/popup instead of the component's indentation actions.
    for stroke in [
        "tab",
        "shift-tab",
        "alt-up",
        "alt-down",
        "alt-c",
        "alt-w",
        "alt-x",
    ] {
        bindings.push(KeyBinding::new(
            stroke,
            gpui::NoAction {},
            Some("guild-search-multiline > Input"),
        ));
    }
    for stroke in ["alt-up", "alt-down"] {
        bindings.push(KeyBinding::new(
            stroke,
            gpui::NoAction {},
            Some("guild-local-query > Input"),
        ));
    }

    // A drag owns Escape before bound panel/editor actions. Double negation
    // sees the ancestor marker while matching at the deepest focus context.
    bindings.push(KeyBinding::new(
        "escape",
        gpui::NoAction {},
        Some("!(!column-drag)"),
    ));
    bindings
}

/// What a key pressed while Settings records a shortcut amounts to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recordable {
    /// A modifier on its own: what a hand rests on while reaching for the
    /// rest of the combination — ignored, neither taken nor refused.
    Modifier,
    /// A key with no `cmd`, `ctrl` or `alt` that is not a function key:
    /// refused in the slot, and the recording goes on.
    NeedsModifier,
    /// A shortcut, spelled as the keymap file spells it.
    Stroke(String),
}

/// Whether a pressed key can be a shortcut. M5.11's decision 14.
///
/// **Every key but `f1`–`f24` needs `cmd`, `ctrl` or `alt`.** A letter, a
/// digit, an arrow, space, Return, Backspace, Tab or a punctuation key —
/// with or without `shift` — bound globally would take that key from every
/// shell and every buffer. A function key alone is a stroke, as `f12`
/// already is. Written as that one exception rather than as a list of
/// refused keys, because a list is what leaks.
pub fn recordable(stroke: &gpui::Keystroke) -> Recordable {
    if matches!(
        stroke.key.as_str(),
        "shift" | "control" | "alt" | "platform" | "function"
    ) {
        return Recordable::Modifier;
    }
    let modifiers = &stroke.modifiers;
    if modifiers.platform || modifiers.control || modifiers.alt || function_key(&stroke.key) {
        Recordable::Stroke(stroke.unparse())
    } else {
        Recordable::NeedsModifier
    }
}

/// `f1` to `f24`, and nothing that merely starts with an `f`.
fn function_key(key: &str) -> bool {
    key.strip_prefix('f')
        .filter(|number| !number.starts_with('0'))
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=24).contains(&number))
}

/// Whether an inherited binding sits at the window's own level: no
/// context, or exactly the component's `Root` or our `guild`.
fn at_the_windows_level(binding: &KeyBinding) -> bool {
    use gpui::KeyBindingContextPredicate::Identifier;
    match binding.predicate().as_deref() {
        None => true,
        Some(Identifier(name)) => name.as_ref() == "Root" || name.as_ref() == "guild",
        Some(_) => false,
    }
}

/// Install the keymap and wire up the actions the system performs.
///
/// GPUI provides no `Quit`, `Hide` and the like: the application declares
/// and performs them. Menu items without them would be dead — and those
/// are exactly the ones people reach for first, out of habit.
///
/// `Quit` is not here: it has to ask about unsaved files and running
/// sessions first, which takes the window, and the binary installs it
/// once the window exists. A global listener that quit outright ran
/// before the window could say a word — `cmd-q` lost an edited file
/// without a question, while the close button asked.
pub fn install(cx: &mut App) {
    rebind(cx);

    cx.on_action(|_: &Hide, cx: &mut App| cx.hide());
    cx.on_action(|_: &HideOthers, cx: &mut App| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx: &mut App| cx.unhide_other_apps());
}

// These stay here, and the attempt to move them out is why the note is
// worth writing. Every one of them reads `defaults()` — the table itself,
// which is private and should be: what the crate promises is `effective()`,
// the defaults with a person's overrides already folded in. A test that
// needs the private half is a test that belongs beside it, and pushing it
// out would have meant widening the API for a test's sake.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_overrides_can_take_the_search_default_strokes() {
        use keymap::Named;
        let bindings = effective_with(std::collections::BTreeMap::from([
            (Named::Run, "cmd-shift-f".into()),
            (Named::FindInFiles, "alt-f".into()),
            (Named::ReloadFile, "cmd-shift-r".into()),
        ]));
        let keymap = gpui::Keymap::new(ordered(bindings, &[]));
        for (stroke, action) in [
            ("cmd-shift-f", Named::Run),
            ("cmd-shift-r", Named::ReloadFile),
            ("alt-f", Named::FindInFiles),
        ] {
            let found = first_binding(&keymap, stroke, &["Root", "guild", "Input"])
                .expect("explicit binding");
            assert!(
                found.action().partial_eq(action.action().as_ref()),
                "{stroke}"
            );
        }
    }

    #[test]
    fn cmd_r_is_reserved_for_local_replace() {
        assert!(
            !defaults()
                .iter()
                .any(|(stroke, action)| { *stroke == "cmd-r" && *action == keymap::Named::Run }),
            "Cmd+R is the user-approved local Replace shortcut"
        );
    }

    #[test]
    fn open_in_approved_defaults_replace_old_open_strokes() {
        for (stroke, name) in [
            ("cmd-o", "OpenSelectedApp"),
            ("cmd-shift-o", "SelectOpenApp"),
        ] {
            let matches: Vec<_> = defaults()
                .into_iter()
                .filter(|(key, _)| *key == stroke)
                .collect();
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].1.name(), name);
        }
        assert!(!defaults().iter().any(|(_, action)| matches!(
            action,
            keymap::Named::AddProject | keymap::Named::GoToFile
        )));
    }

    /// The default shortcuts must be parseable: a mistake here would only
    /// show at runtime, and only for whoever presses the keys.
    #[test]
    fn popup_menus_approved_strokes_are_bound() {
        for (stroke, name) in [
            ("cmd-shift-c", "CopyWorktreePath"),
            ("cmd-alt-shift-r", "RemoveWorktree"),
            // GLD-063: View MR.
            ("cmd-alt-g", "ViewMergeRequest"),
        ] {
            assert!(
                defaults()
                    .iter()
                    .any(|(bound, named)| *bound == stroke && named.name() == name),
                "approved {stroke} -> {name} missing"
            );
        }
    }

    #[test]
    fn popup_menus_preserve_every_contract_binding() {
        let current = defaults();
        for line in include_str!("../../tests/fixtures/popup-menus-keymap-baseline.tsv").lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let (stroke, action) = line.split_once('\t').expect("stroke and action");
            assert!(
                current
                    .iter()
                    .any(|(bound, named)| *bound == stroke && named.name() == action),
                "existing {stroke} -> {action} changed"
            );
        }
    }

    #[test]
    fn titlebar_tool_defaults_have_no_other_guild_binding() {
        for (stroke, action) in [
            ("ctrl-cmd-j", keymap::Named::ToggleTerminalJobs),
            ("ctrl-cmd-u", keymap::Named::ToggleUsage),
        ] {
            let key = gpui::Keystroke::parse(stroke).unwrap();
            assert_eq!(
                defaults()
                    .into_iter()
                    .filter(|(bound, _)| gpui::Keystroke::parse(bound).unwrap() == key)
                    .map(|(_, action)| action)
                    .collect::<Vec<_>>(),
                vec![action]
            );
        }
        assert!(defaults().contains(&("cmd-shift-u", keymap::Named::GoToWaitingSession)));
        assert!(defaults().contains(&("cmd-alt-u", keymap::Named::UntickAllChanges)));
    }

    #[test]
    fn the_default_keymap_is_valid() {
        assert!(!defaults().is_empty());
    }

    /// Every action must be reachable from the settings, or it cannot be
    /// bound at all: the editor lists `Named::ALL` and nothing else.
    /// M5.10's stroke: to whoever is waiting. Pinned, because the settings'
    /// keymap list and the Go menu both read it from here, and a stroke
    /// that silently went would leave a menu item with no shortcut.
    #[test]
    fn going_to_the_waiting_session_is_cmd_shift_u() {
        assert!(defaults().contains(&("cmd-shift-u", keymap::Named::GoToWaitingSession)));
    }

    /// M12's stroke: the sidebar folds one along from the left dock's, and
    /// the View menu shows it from here. Pinned for the same reason.
    #[test]
    fn folding_the_sidebar_is_cmd_shift_b() {
        assert!(defaults().contains(&("cmd-shift-b", keymap::Named::ToggleSidebar)));
    }

    #[test]
    fn workspaces_cycle_with_three_modifiers_and_arrows() {
        assert!(defaults().contains(&("cmd-shift-alt-right", keymap::Named::NextWorkspace)));
        assert!(defaults().contains(&("cmd-shift-alt-left", keymap::Named::PreviousWorkspace)));
    }

    #[test]
    fn every_default_belongs_to_a_listed_action() {
        for (_, action) in defaults() {
            assert!(
                keymap::Named::ALL.contains(&action),
                "{} is bound by default but not listed in the settings",
                action.name()
            );
        }
    }

    /// An override replaces an action's default rather than joining it.
    /// Joining would leave the old stroke working while the settings
    /// showed only the new one — a shortcut doing something the interface
    /// does not admit to.
    #[test]
    fn an_override_replaces_the_default_it_covers() {
        let overridden = keymap::Named::CloseTab;
        let defaults: Vec<&str> = defaults()
            .into_iter()
            .filter(|(_, action)| *action == overridden)
            .map(|(stroke, _)| stroke)
            .collect();

        assert!(
            !defaults.is_empty(),
            "the test needs an action with a default"
        );

        // What `effective` does, on a map standing in for the file.
        let mut kept: Vec<(String, keymap::Named)> = defaults_except(overridden);
        kept.push(("cmd-j".to_owned(), overridden));

        assert!(
            !kept
                .iter()
                .any(|(stroke, _)| defaults.contains(&stroke.as_str())),
            "the replaced default must not still be bound"
        );
        assert_eq!(
            kept.iter()
                .filter(|(_, action)| *action == overridden)
                .count(),
            1,
            "and the action has exactly one stroke"
        );
    }

    fn defaults_except(skip: keymap::Named) -> Vec<(String, keymap::Named)> {
        defaults()
            .into_iter()
            .filter(|(_, action)| *action != skip)
            .map(|(stroke, action)| (stroke.to_owned(), action))
            .collect()
    }

    /// A stroke is stored the way a keymap file writes it and read the way
    /// every other macOS surface prints it.
    #[test]
    fn a_stroke_is_printed_the_way_the_platform_prints_it() {
        assert_eq!(printed("cmd-b"), "⌘B");
        assert_eq!(printed("cmd-shift-right"), "⌘⇧→");
        assert_eq!(printed("cmd-alt-t"), "⌘⌥T");
        assert_eq!(printed("escape"), "Escape");
    }

    #[test]
    fn literal_minus_shortcuts_keep_the_key_in_hints() {
        assert_eq!(printed("cmd-alt--"), "⌘⌥-");
        assert_eq!(printed("-"), "-");
        assert_eq!(printed("cmd-alt-="), "⌘⌥=");
        assert_eq!(printed("cmd-k cmd--"), "⌘K⌘-");
    }

    /// **A double tap is two strokes, and printed as two.** Search
    /// Everywhere's default is a double Shift, which the keymap spells as a
    /// sequence — `shift shift` — and Settings → Keymap and Find Action
    /// print as `⇧⇧`. The spaces are split before the hyphens, or
    /// the sequence reads as one key named `shift shift`.
    #[test]
    fn a_double_tap_is_printed_as_two_arrows() {
        assert_eq!(printed("shift shift"), "⇧⇧");
        assert_eq!(printed("cmd-k cmd-s"), "⌘K⌘S");
    }

    /// The familiar shortcuts were not picked at random: people already
    /// know them from other environments, and changing them would mean
    /// making them relearn for nothing.
    #[test]
    fn the_usual_shortcuts_are_where_people_expect_them() {
        let map =
            keymap::parse(r#"{"cmd-t":"NewSession","cmd-w":"CloseTab","cmd-,":"OpenSettings"}"#);

        assert!(map.is_ok(), "the familiar shortcuts must parse");
    }

    /// Every default stroke must actually parse. `KeyBinding::new` panics
    /// on one that does not, so a typo here would bring the application
    /// down at startup — for everyone, not just whoever presses the key.
    #[test]
    fn the_commit_has_a_stroke_and_clearing_moved_off_push() {
        let bound: Vec<&str> = defaults()
            .iter()
            .filter(|(_, action)| *action == keymap::Named::ShowCommitPanel)
            .map(|(stroke, _)| *stroke)
            .collect();
        assert_eq!(
            bound,
            ["cmd-k"],
            "an audit found the commit reachable only from the menu"
        );
        let clear: Vec<&str> = defaults()
            .iter()
            .filter(|(_, action)| *action == keymap::Named::ClearTerminal)
            .map(|(stroke, _)| *stroke)
            .collect();
        // **Clearing has moved twice now**, and both times because a
        // stroke a person coming from another IDE presses meaning something
        // else landed on it. `cmd-k` took it from `cmd-shift-k` in
        // M8.3.2, and M8.8 took `cmd-shift-k` for Push. The shell's own
        // `ctrl-l` clears and reaches the program untouched throughout,
        // which is why this is a stroke worth moving rather than
        // defending.
        assert_eq!(
            clear,
            ["cmd-shift-alt-k"],
            "which is where clearing went when Push took cmd-shift-k"
        );
    }

    #[test]
    fn every_default_stroke_parses() {
        for (stroke, action) in defaults() {
            assert!(
                gpui::Keystroke::parse(stroke).is_ok(),
                "the stroke {stroke} of {} does not parse",
                action.name()
            );
        }
    }

    /// The settings write what a person records into that file, and read
    /// it back to show the row. A name the file does not accept would be
    /// written once and silently lost on the next start.
    #[test]
    fn every_action_survives_a_trip_through_the_keymap_file() {
        for action in keymap::Named::ALL {
            let json = format!(r#"{{"cmd-j":"{}"}}"#, action.name());
            let parsed = keymap::parse(&json).expect("the settings write this");

            assert_eq!(
                parsed.first().map(|(_, action)| *action),
                Some(*action),
                "{} does not survive being written and read back",
                action.name()
            );
        }
    }

    /// One stroke bound to two actions is a shortcut that quietly does
    /// something other than what the list says.
    #[test]
    fn no_stroke_is_bound_twice() {
        let mut strokes: Vec<&str> = defaults().into_iter().map(|(stroke, _)| stroke).collect();
        let count = strokes.len();
        strokes.sort_unstable();
        strokes.dedup();

        assert_eq!(count, strokes.len(), "a stroke is bound to two actions");
    }

    /// Decision 14 of M12.8, the person's: the arrows walk the places, the
    /// shifted pair splits the area, and `ctrl-cmd-shift-w` closes it.
    #[test]
    fn the_areas_have_the_person_s_strokes() {
        let defaults = defaults();
        for (stroke, named) in [
            ("ctrl-cmd-left", keymap::Named::FocusLeft),
            ("ctrl-cmd-right", keymap::Named::FocusRight),
            ("ctrl-cmd-up", keymap::Named::FocusUp),
            ("ctrl-cmd-down", keymap::Named::FocusDown),
            ("ctrl-cmd-shift-right", keymap::Named::SplitAreaRight),
            ("ctrl-cmd-shift-down", keymap::Named::SplitAreaDown),
            ("ctrl-cmd-alt-right", keymap::Named::SplitPaneRight),
            ("ctrl-cmd-alt-down", keymap::Named::SplitPaneDown),
            ("ctrl-cmd-shift-w", keymap::Named::CloseArea),
        ] {
            assert!(defaults.contains(&(stroke, named)), "{stroke}");
        }
    }

    // **`NUMBERED_PROJECTS` has no strokes to guard any more**, and the
    // test that guarded them is gone with them.
    //
    // It watched for a default claiming one of `cmd-1`…`cmd-9`, which
    // `rebind` bound outside `defaults` and which nothing else could
    // therefore see. M8.8's request 9 gave those strokes to the tool
    // windows deliberately — so the guard would now fail on the thing it
    // was written to catch, and "fix the test" would be the wrong
    // reading of it.
    //
    // It is deleted rather than weakened, and this comment stands in its
    // place, because a guard whose subject is gone **passes silently and
    // looks alive** — `just boundaries` was caught by exactly that when
    // the gateway crate went, and went on reporting success for weeks.
    // `no_stroke_is_bound_twice` above covers what is left: the digits
    // are ordinary defaults now, in the same list as everything else,
    // and a second claim on one of them is a duplicate it can see.

    /// Every action either has a default stroke or is one of the
    /// seventeen that deliberately have none — and the list is read in
    /// **both** directions, which is the half that makes it a guard.
    ///
    /// One direction catches a new action arriving unbound: Guild's
    /// settings list every action, and *a stroke in the list that is
    /// bound to nothing is the shortcut a person reports as broken* —
    /// request 13 of M8.8, and the reason `defaults()` and the settings
    /// list come from one table. The other catches the list going stale:
    /// give one of these a stroke and the test fails until the name is
    /// taken out, so the reasons below cannot outlive the decisions they
    /// record. A list that only checked one way would quietly grow into
    /// a list of things nobody has looked at.
    #[test]
    fn jump_to_source_is_f4() {
        assert!(
            defaults().contains(&("f4", keymap::Named::JumpToSource)),
            "the common stroke for opening what the Project view has selected"
        );
        assert_eq!(
            defaults()
                .iter()
                .filter(|(stroke, _)| *stroke == "f4")
                .count(),
            1,
            "one action on f4"
        );
    }

    #[test]
    fn every_action_has_a_stroke_or_a_reason_for_having_none() {
        // The twelve `Move*To*` — task 2 of M8.8 counted them among the
        // twenty-nine and then took them back out: twelve strokes for a
        // gesture done once a month would spend twelve of the letters
        // left, and the View menu and a drag both already reach them.
        //
        // `ShowTabList` is the tab strip's `⋮`, which is a button by
        // nature — the list it opens is *the tabs that did not fit*, and
        // a person who wanted a tab by keyboard has `cmd-1`…`cmd-9`.
        //
        const STROKELESS: &[keymap::Named] = &[
            keymap::Named::IncludeSelectedChanges, // Customizable; Space retains the list toggle convention.
            keymap::Named::StageHunk, // GLD-026: the person chose no default stroke; the keymap can bind it.
            keymap::Named::UnstageHunk, // The same choice as StageHunk.
            keymap::Named::ExcludeSelectedChanges, // Separate explicit choice for mixed membership.
            keymap::Named::AddProject, // User assigned Cmd+O to Open In; still in File menu/custom keymaps.
            keymap::Named::GoToFile,   // User assigned Cmd+Shift+O to the Open In selector.
            keymap::Named::GetFromVersionControl, // Explicit File/empty-window action; no borrowed shortcut.
            keymap::Named::Commit,                // ShowCommitPanel owns the default Cmd+K.
            keymap::Named::Run, // User assigned Cmd+R to local Replace; Run remains rebindable.
            keymap::Named::ReloadFile, // User assigned Cmd+Shift+R to project Replace.
            keymap::Named::MoveExplorerToLeft,
            keymap::Named::MoveExplorerToRight,
            keymap::Named::MoveExplorerToBottom,
            keymap::Named::MoveCommitToLeft,
            keymap::Named::MoveCommitToRight,
            keymap::Named::MoveCommitToBottom,
            keymap::Named::MoveLogToLeft,
            keymap::Named::MoveLogToRight,
            keymap::Named::MoveLogToBottom,
            keymap::Named::MoveOutputToLeft,
            keymap::Named::MoveOutputToRight,
            keymap::Named::MoveOutputToBottom,
            keymap::Named::ShowTabList,
            // A double Shift, which the window detects rather than binds —
            // decision 8 of M8.9. GPUI would match a binding `shift shift`
            // with a pending sequence that waits a second, and counts a
            // shift-click as a tap.
            keymap::Named::SearchEverywhere,
            // The Search tool is the Find window since 2026-09-23: the
            // popup's *Open in Find Window* brings it up, and `cmd-5`
            // read to the person as a second `cmd-shift-f`.
            keymap::Named::ShowSearchPanel,
        ];

        let bound: Vec<keymap::Named> = defaults().into_iter().map(|(_, a)| a).collect();

        for action in keymap::Named::ALL {
            assert!(
                bound.contains(action) || STROKELESS.contains(action),
                "{} has no default stroke and no reason for having none",
                action.name()
            );
        }

        for action in STROKELESS {
            assert!(
                !bound.contains(action),
                "{} has a stroke now; take it out of the list, and its reason with it",
                action.name()
            );
        }
    }

    /// The first binding GPUI would dispatch for `stroke` with these
    /// contexts focused, from outermost to innermost.
    fn first_binding(keymap: &gpui::Keymap, stroke: &str, contexts: &[&str]) -> Option<KeyBinding> {
        let stroke = gpui::Keystroke::parse(stroke).expect("the test's stroke parses");
        let contexts: Vec<gpui::KeyContext> = contexts
            .iter()
            .map(|context| gpui::KeyContext::parse(context).expect("the test's context parses"))
            .collect();
        keymap
            .bindings_for_input(&[stroke], &contexts)
            .0
            .into_iter()
            .next()
    }

    #[test]
    fn log_fold_defaults_match_native_punctuation_in_list_and_search() {
        use keymap::Named;
        let mut keys = gpui::Keymap::default();
        keys.add_bindings(ordered(defaults_as_effective(), &[]));
        for contexts in [
            vec!["Root", "guild", "log"],
            vec!["Root", "guild", "log", "guild-field", "Input"],
        ] {
            for (stroke, action) in [
                ("cmd-alt--", Named::CollapseChanges),
                ("cmd-alt-=", Named::ExpandChanges),
            ] {
                assert_eq!(
                    first_binding(&keys, stroke, &contexts).map(|b| b.action().name()),
                    Some(action.action().name()),
                    "{stroke} in {contexts:?}"
                );
            }
        }
    }

    #[test]
    fn explorer_mutation_shortcuts_are_scoped_and_customizable() {
        use keymap::Named;
        let mut keys = gpui::Keymap::default();
        keys.add_bindings(ordered(defaults_as_effective(), &[]));
        for (stroke, action) in [
            ("shift-f6", Named::RenameFile),
            ("cmd-backspace", Named::DeleteFile),
            ("cmd-alt-n", Named::NewDirectory),
        ] {
            assert_eq!(
                first_binding(&keys, stroke, &["guild", "tree"]).map(|b| b.action().name()),
                Some(action.action().name())
            );
            for context in [
                vec!["guild", "terminal"],
                vec!["guild", "Input"],
                vec!["guild", "tree", "guild-field", "Input"],
            ] {
                assert!(
                    first_binding(&keys, stroke, &context)
                        .is_none_or(|b| b.action().name() != action.action().name()),
                    "{stroke} in {context:?}"
                );
            }
        }
        let mut keys = gpui::Keymap::default();
        keys.add_bindings(ordered(vec![("ctrl-r".into(), Named::RenameFile)], &[]));
        assert!(first_binding(&keys, "ctrl-r", &["guild", "tree"]).is_some());
        assert!(first_binding(&keys, "shift-f6", &["guild", "tree"]).is_none());
        assert!(
            first_binding(&keys, "ctrl-r", &["guild", "tree", "guild-field", "Input"]).is_none()
        );
    }

    #[test]
    fn commit_mutations_and_custom_bindings_do_not_escape_list_focus() {
        use keymap::Named;
        let mut bindings = defaults_as_effective();
        bindings.push(("ctrl-i".into(), Named::IncludeSelectedChanges));
        bindings.push(("ctrl-e".into(), Named::ExcludeSelectedChanges));
        let mut keys = gpui::Keymap::default();
        keys.add_bindings(ordered(bindings, &[]));
        for (stroke, action) in [
            ("cmd-backspace", Named::DeleteFile),
            ("cmd-alt-a", Named::AddToVcs),
            ("cmd-alt-z", Named::Rollback),
            ("ctrl-i", Named::IncludeSelectedChanges),
            ("ctrl-e", Named::ExcludeSelectedChanges),
        ] {
            assert_eq!(
                first_binding(&keys, stroke, &["guild", "changes"]).map(|b| b.action().name()),
                Some(action.action().name())
            );
            for context in [
                vec!["guild", "terminal"],
                vec!["guild", "guild-file", "Input"],
                vec!["guild", "changes", "Input"],
                vec!["guild", "changes", "guild-field"],
            ] {
                assert!(
                    first_binding(&keys, stroke, &context)
                        .is_none_or(|b| b.action().name() != action.action().name()),
                    "{stroke} in {context:?}"
                );
            }
        }
        assert!(!defaults().iter().any(|(_, a)| matches!(
            a,
            Named::IncludeSelectedChanges | Named::ExcludeSelectedChanges
        )));
    }

    #[test]
    fn clone_form_keys_cannot_dispatch_background_actions() {
        let mut keymap = gpui::Keymap::default();
        keymap.add_bindings(ordered(
            vec![
                ("cmd-v".into(), keymap::Named::AddProject),
                ("enter".into(), keymap::Named::NewSession),
                ("ctrl-w".into(), keymap::Named::CloseTab),
            ],
            &[],
        ));
        for stroke in ["cmd-v", "enter", "ctrl-w", "tab", "shift-tab", "escape"] {
            let binding = first_binding(&keymap, stroke, &["guild panel", "clone-project"]);
            assert!(
                binding.is_none_or(|binding| binding.action().as_any().is::<gpui::NoAction>()),
                "{stroke}"
            );
        }
        assert!(first_binding(&keymap, "enter", &["guild"]).is_some());
    }

    #[test]
    fn settings_search_editing_beats_custom_window_shortcuts() {
        let bindings = ordered(
            vec![
                ("cmd-v".into(), keymap::Named::OpenSettings),
                ("ctrl-w".into(), keymap::Named::OpenSettings),
            ],
            &[],
        );
        let mut keymap = gpui::Keymap::default();
        keymap.add_bindings(bindings);
        for stroke in ["cmd-v", "ctrl-w", "escape"] {
            // GPUI consumes NoAction as an explicit refusal to dispatch.
            let binding = first_binding(&keymap, stroke, &["guild panel", "settings-search"]);
            assert!(binding.is_none_or(|binding| binding.action().as_any().is::<gpui::NoAction>()));
        }
        let outside = first_binding(&keymap, "cmd-v", &["guild"]).unwrap();
        assert!(outside.action().as_any().is::<OpenSettings>());
    }

    #[test]
    fn tool_shortcuts_dispatch_without_collisions_and_overrides_keep_priority() {
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &[]));
        for (key, action) in [
            ("cmd-o", keymap::Named::OpenSelectedApp),
            ("cmd-shift-o", keymap::Named::SelectOpenApp),
            ("cmd-k", keymap::Named::ShowCommitPanel),
            ("cmd-3", keymap::Named::ShowFindPanel),
            ("cmd-4", keymap::Named::ShowOutputPanel),
            ("cmd-9", keymap::Named::ShowLogPanel),
            ("alt-f12", keymap::Named::ShowTerminalPanel),
        ] {
            let actual = first_binding(&keymap, key, &["terminal"]).unwrap();
            assert_eq!(
                actual.action().name(),
                action.binding(key, None).action().name()
            );
        }
        let mut custom = defaults_except(keymap::Named::ShowCommitPanel);
        custom.push(("cmd-2".into(), keymap::Named::ShowCommitPanel));
        let keymap = gpui::Keymap::new(ordered(custom, &[]));
        assert!(first_binding(&keymap, "cmd-k", &["terminal"]).is_none());
        assert_eq!(
            first_binding(&keymap, "cmd-2", &["terminal"])
                .unwrap()
                .action()
                .name(),
            keymap::Named::ShowCommitPanel
                .binding("cmd-2", None)
                .action()
                .name()
        );
    }

    fn defaults_as_effective() -> Vec<(String, keymap::Named)> {
        defaults()
            .into_iter()
            .map(|(stroke, action)| (stroke.to_owned(), action))
            .collect()
    }

    /// **Inside a buffer the component's strokes win, except Back and
    /// Forward.** The inherited bindings stand in for the editor
    /// component's own, in its `Input` context.
    ///
    /// Made to fail first with the order `rebind` had — ours, then every
    /// inherited binding — where `cmd-[` found `input::Outdent`: a
    /// navigation that edited the file.
    #[test]
    fn in_a_buffer_cmd_bracket_is_back_and_cmd_c_is_the_editors() {
        use gpui_component::input;
        let inherited = vec![
            KeyBinding::new("cmd-[", input::Outdent, Some("Input")),
            KeyBinding::new("cmd-]", input::Indent, Some("Input")),
            KeyBinding::new("cmd-c", input::Copy, Some("Input")),
            KeyBinding::new("backspace", input::Backspace, Some("Input")),
        ];
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &inherited));
        let buffer = ["Root", "guild", "Input"];

        let name = |stroke| first_binding(&keymap, stroke, &buffer).map(|b| b.action().name());
        assert!(
            first_binding(&keymap, "cmd-[", &buffer).is_some_and(|b| b.action().partial_eq(&Back)),
            "cmd-[ in a buffer found {:?}",
            name("cmd-[")
        );
        assert!(
            first_binding(&keymap, "cmd-]", &buffer)
                .is_some_and(|b| b.action().partial_eq(&Forward)),
            "cmd-] in a buffer found {:?}",
            name("cmd-]")
        );
        assert!(
            first_binding(&keymap, "cmd-c", &buffer)
                .is_some_and(|b| b.action().partial_eq(&input::Copy)),
            "cmd-c in a buffer found {:?}",
            name("cmd-c")
        );
        assert!(
            first_binding(&keymap, "backspace", &buffer)
                .is_some_and(|b| b.action().partial_eq(&input::Backspace)),
            "backspace in a buffer found {:?}",
            name("backspace")
        );
    }

    /// **The second pair, M8.10.** `cmd-alt-left` and `cmd-alt-right`
    /// mean what `cmd-[` and `cmd-]` mean, in a buffer as well — a stroke
    /// that works everywhere but in a file is worse than none.
    #[test]
    fn the_arrows_are_back_and_forward_too() {
        use gpui_component::input;
        let inherited = vec![KeyBinding::new("cmd-alt-left", input::Copy, Some("Input"))];
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &inherited));

        for context in [vec!["Root", "guild"], vec!["Root", "guild", "Input"]] {
            assert!(
                first_binding(&keymap, "cmd-alt-left", &context)
                    .is_some_and(|b| b.action().partial_eq(&Back)),
                "cmd-alt-left in {context:?}"
            );
            assert!(
                first_binding(&keymap, "cmd-alt-right", &context)
                    .is_some_and(|b| b.action().partial_eq(&Forward)),
                "cmd-alt-right in {context:?}"
            );
        }
    }

    #[test]
    fn history_strokes_reach_the_root_from_a_terminal() {
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &[]));
        let context = ["Root", "guild", "terminal"];
        for stroke in ["cmd-[", "cmd-alt-left"] {
            assert!(
                first_binding(&keymap, stroke, &context)
                    .is_some_and(|b| b.action().partial_eq(&Back))
            );
        }
        for stroke in ["cmd-]", "cmd-alt-right"] {
            assert!(
                first_binding(&keymap, stroke, &context)
                    .is_some_and(|b| b.action().partial_eq(&Forward))
            );
        }
    }

    /// **Back and Forward win over a buffer's keys, and only a buffer's.**
    /// Their second binding comes after every inherited one; bound at full
    /// depth it would also beat whatever the component's menus, lists and
    /// dialogs — and a context-bound binding of ours — put on the same
    /// stroke. A stand-in bound on `cmd-[` in `PopupMenu` is the case: in a
    /// menu the menu's key is the menu's.
    #[test]
    fn back_wins_a_stroke_in_a_buffer_and_not_in_a_menu() {
        use gpui_component::input;
        let inherited = vec![
            KeyBinding::new("cmd-[", input::Outdent, Some("Input")),
            KeyBinding::new("cmd-[", input::Indent, Some("PopupMenu")),
        ];
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &inherited));

        let menu = ["Root", "guild", "PopupMenu"];
        assert!(
            first_binding(&keymap, "cmd-[", &menu)
                .is_some_and(|b| b.action().partial_eq(&input::Indent)),
            "cmd-[ in a menu found {:?}",
            first_binding(&keymap, "cmd-[", &menu).map(|b| b.action().name())
        );
        let buffer = ["Root", "guild", "Input"];
        assert!(
            first_binding(&keymap, "cmd-[", &buffer).is_some_and(|b| b.action().partial_eq(&Back)),
            "cmd-[ in a buffer found {:?}",
            first_binding(&keymap, "cmd-[", &buffer).map(|b| b.action().name())
        );
        let window = ["Root", "guild"];
        assert!(
            first_binding(&keymap, "cmd-[", &window).is_some_and(|b| b.action().partial_eq(&Back)),
            "cmd-[ with the root focused found {:?}",
            first_binding(&keymap, "cmd-[", &window).map(|b| b.action().name())
        );
    }

    /// **A shortcut needs `cmd`, `ctrl` or `alt`, unless it is a function
    /// key.** Written as the exception rather than as a list of refused
    /// keys: the first draft of M5.11's plan listed letters, digits and
    /// arrows, and a review found space, Return and punctuation leaking
    /// through it — each of which, bound globally, takes that key from every
    /// shell and every buffer.
    #[test]
    fn only_a_function_key_is_a_shortcut_without_cmd_ctrl_or_alt() {
        let parse =
            |text: &str| gpui::Keystroke::parse(text).unwrap_or_else(|_| panic!("{text} parses"));
        for refused in [
            "l",
            "shift-l",
            "5",
            "down",
            "space",
            "enter",
            "backspace",
            "tab",
            "-",
            "shift-/",
        ] {
            assert_eq!(
                recordable(&parse(refused)),
                Recordable::NeedsModifier,
                "{refused} alone would take the key from every shell"
            );
        }
        for stroke in ["cmd-l", "ctrl-alt-5", "alt-space", "f5", "shift-f12"] {
            assert!(
                matches!(recordable(&parse(stroke)), Recordable::Stroke(_)),
                "{stroke} is a shortcut"
            );
        }
        // A modifier pressed alone, as GPUI reports it: the key is the
        // modifier's own name and its flag is set.
        let alone = |key: &str, modifiers: gpui::Modifiers| gpui::Keystroke {
            modifiers,
            key: key.to_owned(),
            key_char: None,
        };
        assert_eq!(
            recordable(&alone("shift", gpui::Modifiers::shift())),
            Recordable::Modifier
        );
        assert_eq!(
            recordable(&alone("platform", gpui::Modifiers::command())),
            Recordable::Modifier
        );
    }

    /// **In a terminal `cmd-g` steps the find, and everywhere else it goes
    /// to a line** — M5.12's decision 10. And with a context under
    /// `terminal`, `terminal` matches a level shallower and `GoToLine`
    /// wins: the assertion that pins why the find bar names no context.
    ///
    /// Made to fail first with the two bindings placed before Guild's own,
    /// where `cmd-g` in a terminal found `GoToLine`.
    #[test]
    fn in_a_terminal_cmd_g_steps_the_find_and_elsewhere_goes_to_a_line() {
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &[]));
        let found = |stroke, contexts: &[&str]| {
            first_binding(&keymap, stroke, contexts).map(|binding| binding.action().name())
        };

        let terminal = ["Root", "guild", "terminal"];
        assert!(
            first_binding(&keymap, "cmd-g", &terminal)
                .is_some_and(|b| b.action().partial_eq(&FindNext)),
            "cmd-g in a terminal found {:?}",
            found("cmd-g", &terminal)
        );
        assert!(
            first_binding(&keymap, "cmd-shift-g", &terminal)
                .is_some_and(|b| b.action().partial_eq(&FindPrevious)),
            "cmd-shift-g in a terminal found {:?}",
            found("cmd-shift-g", &terminal)
        );

        let window = ["Root", "guild"];
        assert!(
            first_binding(&keymap, "cmd-g", &window)
                .is_some_and(|b| b.action().partial_eq(&GoToLine)),
            "cmd-g with the root focused found {:?}",
            found("cmd-g", &window)
        );

        let under_the_terminal = ["Root", "guild", "terminal", "findbar"];
        assert!(
            first_binding(&keymap, "cmd-g", &under_the_terminal)
                .is_some_and(|b| b.action().partial_eq(&GoToLine)),
            "cmd-g in a context under the terminal found {:?}",
            found("cmd-g", &under_the_terminal)
        );
    }

    gpui::actions!(test_only, [InspectorStandIn]);

    /// **At the window's own level Guild's strokes win.** The component's
    /// `ToggleInspector` is in a private module, so an action of the test's
    /// own stands in for it, bound as the inspector binds it — no context —
    /// with the `NoAction` `editor::init` lays over it in `guild`.
    ///
    /// Made to fail with every inherited binding after ours: with the root
    /// focused the `NoAction` came first and `cmd-alt-i` did nothing.
    #[test]
    fn a_stroke_bound_at_the_windows_level_stays_ours() {
        let inherited = vec![
            KeyBinding::new("cmd-alt-i", InspectorStandIn, None),
            KeyBinding::new("cmd-alt-i", gpui::NoAction {}, Some("guild")),
        ];
        let keymap = gpui::Keymap::new(ordered(defaults_as_effective(), &inherited));
        for contexts in [&["Root", "guild"][..], &["Root", "guild", "changes"][..]] {
            let found = first_binding(&keymap, "cmd-alt-i", contexts);
            assert!(
                found
                    .as_ref()
                    .is_some_and(|b| b.action().partial_eq(&TickAllChanges)),
                "cmd-alt-i with {contexts:?} found {:?}",
                found.map(|b| b.action().name())
            );
        }
    }

    /// cmd-0 resets the font size, and the projects start at one — which
    /// is also how a person counts the row.
    #[test]
    fn the_numbering_starts_at_one_and_leaves_cmd_zero_alone() {
        let strokes: Vec<String> = (0..NUMBERED_PROJECTS)
            .map(|index| format!("cmd-{}", index + 1))
            .collect();

        assert_eq!(strokes.first().map(String::as_str), Some("cmd-1"));
        assert_eq!(strokes.last().map(String::as_str), Some("cmd-9"));
        assert!(!strokes.iter().any(|stroke| stroke == "cmd-0"));
        assert!(
            defaults().iter().any(|(stroke, _)| *stroke == "cmd-0"),
            "cmd-0 is spoken for, which is why the row starts at one"
        );
    }
}
