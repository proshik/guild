//! What an action is called in a flat list of actions — Find Action's rows.
//!
//! M8.9, decision 15. **[`label`] is an exhaustive `match` over [`Named`]**,
//! so an action added without a name does not compile. Its words come from
//! four places, and seams item 5's table and task 0's walk say which:
//!
//! - **the menu's word, where it reads alone** — *Close Tab*, *Back*,
//!   *Go to Line…*; one word for one deed in a menu and in this list, so a
//!   person who knows the one knows the other;
//! - **an `Action…` label of `i18n`, where it does not** — the View menu's
//!   *Larger* is a size only beside *Smaller*, and *Session List* is a
//!   noun in a list of deeds;
//! - **`Ui::tool_window`** for the six tool windows, named from
//!   `view::panels::title_of` so the list, the status bar and a dock's head
//!   cannot call one tool three things;
//! - **`Ui::move_tool`** for the twelve moves, which the menu spells as
//!   *Left*, *Right* and *Bottom* under a submenu nothing here has.

use guild_session::{Dock, Tool};

use crate::i18n::{Text, Ui};
use crate::view::panels::title_of;

use super::keymap::Named;

/// The name `action` is listed under in `ui`'s language.
pub fn label(action: Named, ui: Ui) -> String {
    use Named as N;
    let text = |text: Text| ui.text(text).to_owned();
    let window = |tool: Tool| ui.tool_window(title_of(tool));
    let moved = |tool: Tool, dock: Dock| ui.move_tool(title_of(tool), dock);

    match action {
        N::NewSession => text(Text::ActionNewSession),
        N::NewTerminal => text(Text::ActionNewTerminal),
        N::CloseTab => text(Text::MenuCloseTab),
        N::ReopenTab => text(Text::MenuReopenTab),
        N::CloseOtherTabs => text(Text::ActionCloseOtherTabs),
        N::CloseAllTabs => text(Text::ActionCloseAllTabs),
        N::SaveFile => text(Text::ActionSaveFile),
        N::SaveAllFiles => text(Text::ActionSaveAllFiles),
        N::ReloadFile => text(Text::MenuReloadFile),
        // Two menu words, *Reveal Active File* and *Select in Explorer*;
        // one label per action, and the second is the common IDE's.
        N::RevealFile => text(Text::MenuSelectInExplorer),
        N::ShowDiff => text(Text::ShowDiff),
        N::ShowChanges => text(Text::ShowChanges),
        N::Commit => text(Text::MenuCommit),
        N::Annotate => text(Text::Annotate),
        N::NextChange => text(Text::ActionNextChange),
        N::PreviousChange => text(Text::ActionPreviousChange),
        N::StageHunk => text(Text::ActionStageHunk),
        N::UnstageHunk => text(Text::ActionUnstageHunk),
        N::RefreshChanges => text(Text::ActionRefreshChanges),
        N::ShowSelectedDiff => text(Text::ActionShowSelectedDiff),
        N::IncludeSelectedChanges => text(Text::ActionIncludeSelectedChanges),
        N::ExcludeSelectedChanges => text(Text::ActionExcludeSelectedChanges),
        N::TickAllChanges => text(Text::ActionTickAllChanges),
        N::UntickAllChanges => text(Text::ActionUntickAllChanges),
        N::CollapseChanges => text(Text::ActionCollapseChanges),
        N::ExpandChanges => text(Text::ActionExpandChanges),
        N::ShowExplorerPanel => window(Tool::Explorer),
        N::ShowCommitPanel => window(Tool::Commit),
        N::ShowLogPanel => window(Tool::Git),
        N::ShowOutputPanel => window(Tool::Output),
        N::ShowTerminalPanel => window(Tool::Terminal),
        N::FindInFiles => text(Text::MenuFindInFiles),
        N::ReplaceInFiles => text(Text::MenuReplaceInFiles),
        N::ShowSearchPanel => window(Tool::Search),
        N::Run => text(Text::ActionRun),
        N::StopRun => text(Text::ActionStopRun),
        N::ClearRun => text(Text::ActionClearRun),
        N::GoToDefinition => text(Text::ActionGoToDefinition),
        N::MoveExplorerToLeft => moved(Tool::Explorer, Dock::Left),
        N::MoveExplorerToRight => moved(Tool::Explorer, Dock::Right),
        N::MoveExplorerToBottom => moved(Tool::Explorer, Dock::Bottom),
        N::MoveCommitToLeft => moved(Tool::Commit, Dock::Left),
        N::MoveCommitToRight => moved(Tool::Commit, Dock::Right),
        N::MoveCommitToBottom => moved(Tool::Commit, Dock::Bottom),
        N::MoveLogToLeft => moved(Tool::Git, Dock::Left),
        N::MoveLogToRight => moved(Tool::Git, Dock::Right),
        N::MoveLogToBottom => moved(Tool::Git, Dock::Bottom),
        N::MoveOutputToLeft => moved(Tool::Output, Dock::Left),
        N::MoveOutputToRight => moved(Tool::Output, Dock::Right),
        N::MoveOutputToBottom => moved(Tool::Output, Dock::Bottom),
        N::NextTab => text(Text::MenuNextTab),
        N::PreviousTab => text(Text::MenuPreviousTab),
        N::OpenSelectedApp => text(Text::OpenSelectedApp),
        N::SelectOpenApp => text(Text::OpenInButton),
        N::AddProject => text(Text::ActionAddProject),
        N::GetFromVersionControl => text(Text::GetFromVersionControl),
        N::NewProject => text(Text::ActionNewProject),
        N::CloseProject => text(Text::MenuCloseProject),
        N::CopyWorktreePath => text(Text::ActionCopyWorktreePath),
        N::RemoveWorktree => text(Text::MenuRemoveWorktree),
        N::ViewMergeRequest => text(Text::MenuViewMr),
        N::NextProject => text(Text::MenuNextProject),
        N::PreviousProject => text(Text::ActionPreviousProject),
        N::NextWorkspace => text(Text::MenuNextWorkspace),
        N::PreviousWorkspace => text(Text::MenuPreviousWorkspace),
        N::SplitAreaRight => text(Text::MenuSplitAreaRight),
        N::SplitAreaDown => text(Text::MenuSplitAreaDown),
        N::SplitPaneRight => text(Text::MenuSplitPaneRight),
        N::SplitPaneDown => text(Text::MenuSplitPaneDown),
        N::CloseArea => text(Text::MenuCloseArea),
        N::FocusLeft => text(Text::ActionFocusLeft),
        N::FocusRight => text(Text::ActionFocusRight),
        N::FocusUp => text(Text::ActionFocusUp),
        N::FocusDown => text(Text::ActionFocusDown),
        N::Back => text(Text::MenuBack),
        N::Forward => text(Text::MenuForward),
        N::RecentFiles => text(Text::MenuRecentFiles),
        N::RecentLocations => text(Text::MenuRecentLocations),
        N::LastEditLocation => text(Text::MenuLastEdit),
        N::GoToLine => text(Text::MenuGoToLine),
        N::ShowTabList => text(Text::ActionShowTabList),
        N::Push => text(Text::ActionPush),
        // The Commit panel's right button — no menu bar item.
        N::Rollback => text(Text::MenuRollback),
        N::AddToVcs => text(Text::MenuAddToVcs),
        N::NewFile => text(Text::ActionNewFile),
        N::NewDirectory => text(Text::ActionNewDirectory),
        N::MoveFile => text(Text::MenuMove),
        N::CutFiles => text(Text::MenuCutFiles),
        N::RenameFile => text(Text::ActionRenameFile),
        N::JumpToSource => text(Text::ActionJumpToSource),
        N::DeleteFile => text(Text::ActionDeleteFile),
        N::ShowProblems => window(Tool::Problems),
        N::FindUsages => text(Text::ActionFindUsages),
        N::ShowFindPanel => window(Tool::Find),
        N::NextUsage => text(Text::ActionNextUsage),
        N::PreviousUsage => text(Text::ActionPreviousUsage),
        N::RecentProjects => text(Text::MenuRecentProjects),
        N::ToggleSessions => text(Text::ActionToggleSessions),
        N::ToggleTerminalJobs => text(Text::TerminalJobs),
        N::ToggleUsage => text(Text::ActionBarUsage),
        N::GoToWaitingSession => text(Text::MenuGoToWaitingSession),
        N::ToggleSidebar => text(Text::ActionToggleSidebar),
        N::ToggleLeftPanel => text(Text::ActionToggleLeftPanel),
        N::ToggleRightPanel => text(Text::ActionToggleRightPanel),
        N::ToggleBottomPanel => text(Text::ActionToggleBottomPanel),
        N::WidenPanel => text(Text::ActionWidenPanel),
        N::NarrowPanel => text(Text::ActionNarrowPanel),
        N::StretchToLeft => text(Text::ActionStretchToLeft),
        N::StretchToRight => text(Text::ActionStretchToRight),
        N::StretchToTop => text(Text::ActionStretchToTop),
        N::StretchToBottom => text(Text::ActionStretchToBottom),
        N::OpenSettings => text(Text::MenuSettings),
        N::ShowLog => text(Text::ActionShowLog),
        N::ToggleFullScreen => text(Text::ActionToggleFullScreen),
        N::IncreaseFontSize => text(Text::ActionIncreaseFontSize),
        N::DecreaseFontSize => text(Text::ActionDecreaseFontSize),
        N::ResetFontSize => text(Text::ActionResetFontSize),
        N::ClearTerminal => text(Text::MenuClearTerminal),
        N::Copy => text(Text::MenuCopy),
        N::Paste => text(Text::MenuPaste),
        N::SelectAll => text(Text::MenuSelectAll),
        N::Find => text(Text::ActionFind),
        N::Replace => text(Text::ActionReplace),
        N::SearchEverywhere => text(Text::MenuSearchEverywhere),
        N::GoToFile => text(Text::MenuGoToFile),
        N::GoToSymbol => text(Text::MenuGoToSymbol),
        N::FindAction => text(Text::MenuFindAction),
    }
}

/// Whether Find Action lists `action` at all.
///
/// **An action no focus can ever handle is not offered**: a row that does
/// nothing on Enter is worse than no row. Task 0 walked all 94 for an
/// `on_action` and found one without — `ShowTabList`, the tab strip's `⋮`,
/// declared and bound to nothing (`N9`). An action that only *some* focus
/// handles — `Copy` in a terminal — is listed and dimmed where it is not
/// available, which the popup asks the window when it opens.
pub fn offered(action: Named) -> bool {
    !matches!(action, Named::ShowTabList)
}
