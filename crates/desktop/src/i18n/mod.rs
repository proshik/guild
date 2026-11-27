//! The interface language.
//!
//! The strings live in an enum rather than in files: adding a language and
//! forgetting to translate part of the labels is a mistake the compiler
//! must catch. With files it turns into an empty spot in the interface
//! that users are the first to notice.
//!
//! The price is a recompile for every new language. For a development
//! environment that is acceptable: languages are not added by users.
//!
//! Strings with numbers and paths are separate methods rather than being
//! assembled from pieces at the call site: word order differs between
//! languages, and "5 sessions" against its Russian equivalent is not
//! covered by one concatenation.

pub use guild_session::Language;

mod auto_ff;
mod branch_update;
mod commit;
mod forge;
pub use commit::CommitLabel;
mod english;
mod russian;
use guild_session::{Base, Usage};

/// Every label the window can draw, **in one list**.
///
/// The enum was one list and `tests/all/labels.rs` kept a second by hand
/// — and that second one had silently fallen 81 variants behind. A label
/// missing from it was simply never checked by
/// `no_language_has_an_empty_string`, with nothing to say so, which is
/// the shape of guard this repository has been caught by before: it
/// passes, and it looks alive.
///
/// The two translations stay exhaustive `match`es, because that is the
/// guard that actually works — a new variant does not compile until both
/// languages answer it. What this removes is the third place to remember.
///
/// Banner comments live inside the invocation and survive: comments are
/// not tokens, so the macro never sees them and a reader still does.
macro_rules! labels {
    ($($(#[$meta:meta])* $label:ident),+ $(,)?) => {
        /// An interface label.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Text {
            $($(#[$meta])* $label),+
        }

        impl Text {
            /// Every label, in the order they are declared.
            ///
            /// Generated, so a label cannot be added without being
            /// checked — which is the whole reason this macro exists.
            pub const ALL: &'static [Text] = &[$(Text::$label),+];
        }
    };
}

labels! {
    CommitSelectionChanged,
    CommitRollbackMixed,
    CommitAddMixed,
    CommitTrashMissing,
    CommitDiffDirectories,
    CommitDiffUnavailable,
    CommitInclusionUnavailable,
    CommitRollbackUnavailable,
    CommitAddUnavailable,
    CommitTrashUnavailable,
    CommitExpandDirectory,
    CommitCollapseDirectory,
    CommitSpaceHint,
    CommitDirectoryLoading,
    CommitDirectoryEmpty,
    CommitDirectoryNoEligible,
    CommitDirectoryInvalidName,
    CommitUnsupportedPath,
    CommitPathNul,
    CommitTrashRoot,
    CommitRollbackGroupDetail,

    HarnessFailureNetwork,
    HarnessFailureDetails,
    HarnessRetry,
    HarnessRetryUnknown,
    HarnessRetryManaged,
    HarnessRetryOccupied,
    HarnessRetryDirectory,
    HarnessRetryHeld,
    HarnessRetryIdentity,
    Process,
    ProcessMoveUnavailable,
    ProcessMoveSelectPane,
    ProcessMoveSeat,
    ProcessOpenJob,
    ProcessJobUnavailable,
    ProcessContext,
    ProcessSelectedStep,
    ProcessExecuting,
    ProcessWorker,
    ProcessEvents,
    ProcessNoEvents,
    ProcessContract,
    ProcessEvidence,
    ProcessNoEvidence,
    ProcessRecordedSource,
    ProcessResources,
    ProcessNoResources,
    ProcessSessionUnavailable,
    ProcessFileUnavailable,
    ProcessControlsUnavailable,
    ProcessQuestionUnavailable,
    ProcessPermissionUnavailable,
    ProcessWaitingUnavailable,
    ProcessSelectedFallback,
    ProcessStaleSnapshot,
    ProcessCloseKeepsRunning,
    ProcessSourceDifferent,
    ProcessSourceCurrent,
    ProcessNoAssignment,
    ProcessRole,
    ProcessOutcomes,
    ProcessDependencies,
    ProcessDefinition,
    ProcessOpenFile,
    ProcessUnrecorded,
    ProcessEventStart,
    ProcessEventCheckpoint,
    ProcessEventAssignment,
    ProcessEventDelivery,
    ProcessEventRecovery,
    ProcessEventReport,
    ProcessEventReview,
    ProcessEventCorrection,
    ProcessCorrectionParent,
    ProcessCorrectionRefused,
    ProcessCorrectionSubmitted,
    ProcessCorrectionObserved,
    ProcessCorrectionUnknown,
    ProcessEventCancel,
    ProcessEventReservation,
    ProcessEventUnknown,
    ProcessLightweight,
    ProcessNoReview,
    ProcessPreviousResumeWait,

    ProcessOpen,
    ProcessRecoveryWait,
    ProcessReconcileEvent,
    ProcessReconcileChecklist,
    ProcessReconcileClear,
    ProcessReconcileRecorded,
    ProcessHandoffStartup,
    ProcessHandoffStartupSubmitted,
    ProcessHandoffTitle,
    ProcessHandoffReserved,
    ProcessHandoffReady,
    ProcessHandoffWaiting,
    ProcessHandoffBusy,
    ProcessHandoffDraft,
    ProcessHandoffInputUsed,
    ProcessHandoffInputUnknown,
    ProcessHandoffHolder,
    ProcessHandoffCovered,
    ProcessHandoffTransport,
    ProcessHandoffSnapshot,
    ProcessHandoffSubmitted,
    ProcessHandoffObserved,
    ProcessHandoffAcknowledged,
    ProcessHandoffUnknown,
    ProcessHandoffContinued,

    ProcessReconcileStale,
    ProcessReconcileSource,
    ProcessReconcileContract,
    ProcessReconcileOrigin,
    ProcessReconcileHolder,
    ProcessReconcilePermission,
    ProcessReconcileQuestion,
    ProcessReconcileEffect,
    ProcessReconcileDelivery,
    ProcessReconcileReport,
    ProcessReconcileChildSource,
    ProcessReconcileReview,
    ProcessReconcileResource,
    ProcessReconcileUnknown,
    ProcessReconcileStatus,

    ProcessRecoveryPending,
    ProcessRecoveryLive,
    ProcessRecoveryLaunched,
    ProcessRecoveryAreaFallback,
    ProcessRecover,
    ProcessParentAttention,
    ProcessRecoveryGap,
    ProcessRevision,
    ProcessUnavailable,
    ProcessLoading,
    ProcessReviewUsage,
    ProcessSteps,
    ProcessOrigin,
    ProcessOpenSession,
    ProcessTarget,
    ProcessAssignments,
    ProcessCheckout,
    ProcessSession,
    ProcessArchive,
    ProcessNoJob,
    ProcessBlocked,
    ProcessReady,
    ProcessActive,
    ProcessWaiting,
    ProcessFinished,
    ProcessSkipped,
    ProcessCancelled,
    ProcessUnknown,
    NetworkProxyRoute,
    NetworkDirectRoute,
    NetworkSection,
    NetworkIntro,
    NetworkConnection,
    NetworkApply,
    NetworkHarnesses,
    NetworkHarnessHint,
    NetworkFollow,
    NetworkUseProxy,
    NetworkDirect,
    NetworkMode,
    NetworkModeHint,
    NetworkAddress,
    NetworkAddressHint,
    NetworkBypass,
    NetworkBypassHint,
    NetworkGuild,
    NetworkGuildHint,
    NetworkAgent,
    NetworkAgentHint,
    NetworkTerminal,
    NetworkTerminalHint,
    NetworkGit,
    NetworkGitHint,
    NetworkTools,
    NetworkToolsHint,
    NetworkSystem,
    NetworkCustom,
    NetworkCheck,
    NetworkChecking,
    NetworkTiming,
    NetworkCheckHint,
    NetworkConnected,
    NetworkReachable,
    NetworkEndpointError,
    NetworkNotUsage,
    NetworkAuthError,
    NetworkSystemCredentials,
    NetworkSystemTransport,
    NetworkTimeout,
    NetworkDnsError,
    NetworkRefused,
    NetworkTlsError,
    NetworkUnavailable,
    NetworkInvalid,
    NetworkFailed,

    LspSettings,
    LspDisabled,
    LspEnabled,
    LspInherit,
    LspOn,
    LspOff,
    LspNotStarted,
    LspNotInstalled,
    LspUnprepared,
    LspFailed,
    LspRunning,
    ScopedRepository,
    /// The Repository card's row naming the project group it is in.
    ScopedGroup,
    ScopedWorkspace,
    ScopedWorkspaces,
    ScopedName,
    ScopedNameHint,
    ScopedIconHint,
    ScopedBaseHint,
    ScopedDirectoryHint,
    ScopedDeleteHint,
    ScopedSaveHint,

    ScopedIcon,
    ScopedProjects,
    ScopedMainPath,
    ScopedBase,
    ScopedDirectory,
    ScopedOpenSettings,
    ScopedShared,
    ScopedDefaults,
    ScopedIdentityPending,
    ScopedUnavailable,
    ScopedSave,
    ScopedRetry,
    ScopedChoose,
    ScopedAutomatic,
    ScopedNoMain,
    ScopedFutureOnly,
    ScopedDeleteWorkspace,

    TerminalJobs,
    TerminalThisSession,
    TerminalThisWorktree,
    TerminalThisProject,
    TerminalAllLaunches,
    TerminalOngoing,
    TerminalCompleted,
    TerminalScopeEmpty,
    TerminalNoSession,
    TerminalNoRoot,
    TerminalUnknownSession,
    TerminalUnknownRoot,
    TerminalUnknownRepository,
    TerminalObservedBranch,
    TerminalUnknownBranch,
    TerminalProgramCwd,
    TerminalUnknownCwd,
    TerminalAllContexts,
    TerminalContextProject,
    TerminalContextWorktree,
    TerminalContextSession,
    TerminalHideView,
    TerminalOpeningArchive,
    TerminalViewingArchive,
    TerminalClosingArchive,
    TerminalCloseViewer,

    TerminalNewest,
    TerminalNoJobs,
    TerminalUnknownTime,
    TerminalShowOutput,
    TerminalOpenRun,
    TerminalDeleteOutput,
    TerminalOutputDeleted,
    TerminalOutputEmpty,
    TerminalNoSavedOutput,
    ReviewDetails,
    ReviewAnalyze,
    ReviewConsolidate,
    ReviewVerify,
    ReviewAnalyzeProgress,
    ReviewConsolidateProgress,
    ReviewVerifyProgress,
    ReviewSerious,
    ReviewWarnings,
    ReviewUnknown,
    ReviewProvisional,
    ReviewAssessed,
    ReviewFinal,
    ReviewClean,
    ReviewIncomplete,
    ReviewUnavailable,
    ReviewStale,
    ReviewLegacy,
    ReviewOther,
    ReviewOpenQuestion,
    ReviewPreExisting,
    ReviewImmaterial,
    ReviewRejected,

    ReviewDropped,
    ReviewReported,
    ReviewNoFindingsYet,

    TerminalJobDetails,
    TerminalOutputUnavailable,
    TerminalStarting,
    TerminalRunning,
    TerminalCancelling,
    TerminalFailed,
    TerminalInterrupted,
    TerminalExited,
    TerminalSignal,
    TerminalSelectPane,

    GetFromVersionControl,
    CloneTitle,
    CloneDirectoryHelp,
    CloneFailureHelp,
    CloneUrl,
    CloneDirectory,
    CloneChoose,
    CloneStart,
    CloneRunning,
    CloneCancelling,
    CloneCancelled,
    CloneFailed,
    CloneHelp,
    CloneDetails,
    CloneHide,
    CloneRetained,
    CloneOpen,
    CloneOpenMissing,
    CloneComplete,

    SettingsPreview,
    SettingsLanguage,
    SettingsLocalFiles,
    SettingsColourTheme,
    SettingsTypography,
    SettingsActionBar,
    ActionBarItemOrder,
    ActionBarHint,
    ActionBarUsage,
    ActionBarReset,
    ActionBarAdd,
    ActionBarRemove,
    ActionBarEmpty,
    ActionBarAllShown,
    ActionBarAddHint,
    ActionBarKeyboardHint,

    SettingsSelection,
    LaunchPrefixes,
    LaunchPrefixHint,
    LaunchPrefixLabel,
    LaunchArgumentsLabel,
    SettingsNewSessions,
    SettingsClaudePermissions,
    SettingsStartup,
    SettingsClosing,
    SettingsSaving,
    SettingsDiffs,
    SettingsSearch,
    SettingsResults,
    SettingsNoResults,
    SettingsTrySearch,
    SettingsClear,
    SettingsScope,
    SettingsImmediate,
    SettingsModified,
    SettingsShortcuts,
    SettingsGeneralIntro,
    SettingsAppearanceIntro,
    SettingsTerminalIntro,
    SettingsSessionsIntro,
    SettingsProjectsIntro,
    SettingsEditorIntro,
    SettingsKeymapIntro,
    SettingsResultsIntro,

    UpdaterCleanupRecovery,
    UpdaterJournalWaiting,
    UpdaterRecoveryHelp,
    UpdaterCopyRecovery,
    UpdaterDestination,
    UpdaterPrevious,
    UpdaterLegacyReplacement,
    UpdatesSection,
    UpdatesIntro,
    UpdaterSource,
    UpdaterSourceHint,
    UpdaterChannel,
    UpdaterChannelHint,
    UpdaterDirectory,
    UpdaterDirectoryHint,
    UpdaterStatus,
    UpdaterStatusHint,
    UpdaterAbsolutePath,
    UpdaterDefaultDirectory,
    UpdaterPublicHelp,
    UpdaterLoading,
    UpdaterSaving,
    UpdaterSaveFailed,
    UpdaterPreferenceRecovery,
    UpdaterKeepRequested,
    UpdaterKeepPrevious,
    UpdaterCheck,
    UpdaterChecking,
    UpdaterDownload,
    UpdaterRestart,
    UpdaterReinstall,
    UpdaterInstalling,
    UpdaterVerifying,
    UpdaterEmpty,
    UpdaterInstalled,
    UpdaterCurrentVersion,
    UpdaterCandidate,
    UpdaterLatest,
    UpdaterNoNewer,
    UpdaterDetails,
    UpdaterHistory,
    UpdaterNotes,
    UpdaterUnverified,
    UpdaterCacheWarning,
    UpdaterAccessError,
    UpdaterNetworkError,
    UpdaterRateError,
    UpdaterVerificationError,
    UpdaterStorageError,
    UpdaterRecoveryError,
    UpdaterLocalProducer,
    UpdaterCiProducer,
    UpdaterLegacyInfo,

    // Tabs and projects
    NewSession,
    NewTerminal,
    Session,
    Terminal,
    Shell,
    Settings,
    // The session list
    Open,
    Reopen,
    NoConversation,
    // Empty states
    NoProjects,
    HintNewTerminal,
    HintRecentProjects,
    HintOpenProject,
    HintNewProject,
    HintSettings,
    /// The start screen — M14.5, section 6.35 of the design system.
    StartChoose,
    StartAllTools,
    StartRecent,
    StartAll,
    StartDefault,
    /// A tile's and the default card's name for a shell: capitalised,
    /// where the strip's menu reads the lower-case mode word.
    StartTerminalName,
    StartNotOnPath,
    StartResume,
    StartNoSessions,
    StartSettings,
    StartNewSession,
    NoProject,
    /// The status bar's name for a file no language claims.
    LanguagePlain,
    ClaudeWithoutConversation,
    ChooseConversation,
    QuitTitle,
    QuitAnyway,
    Cancel,
    // Settings
    SettingsTitle,
    SectionGeneral,
    SectionTerminal,
    SectionSessions,
    SectionEditor,
    AutosaveTitle,
    AutosaveHint,
    FileNotSaved,
    SectionKeymap,
    SectionExperimental,
    SettingsExperimentalIntro,
    DevelopmentWorkflowsTitle,
    DevelopmentWorkflowsHint,
    SettingsDone,
    KeymapHint,
    KeymapRecording,
    KeymapReset,
    KeymapTaken,
    KeymapFileTitle,
    RevealFile,
    TabLauncherOrderTitle,
    TabLauncherOrderHint,
    SettingsSessionList,
    SettleAfterTitle,
    SettleAfterHint,
    SettleAfterUnit,
    DefaultTabTitle,
    DefaultTabHint,
    SettingsRendering,
    AdaptiveRefreshTitle,
    AdaptiveRefreshHint,
    CopyOnSelectTitle,
    CopyOnSelectHint,
    FontSizeTitle,
    FontSizeHint,
    PermissionsTitle,
    PermissionsHint,
    NativePermissionsValue,
    CodexSandboxTitle,
    CodexSandboxHint,
    CodexCommandApprovalsTitle,
    CodexCommandApprovalsHint,
    OpenCodePermissionsHint,
    LanguageTitle,
    LanguageHint,
    Yes,
    No,
    // Menu
    MenuFile,
    MenuEdit,
    MenuView,
    MenuGo,
    MenuHelp,
    MenuSettings,
    MenuServices,
    MenuHide,
    MenuHideOthers,
    MenuShowAll,
    MenuQuit,
    MenuRenameTab,
    MenuCloseTab,
    MenuReopenTab,
    // The menu the right button opens. Section 6.16 of the design
    // system; M8.3.2.
    MenuOpen,
    MenuShowDiff,
    MenuExpand,
    MenuCollapse,
    MenuCopyPath,
    MenuCopyFullPath,
    MenuReveal,
    MenuCloseOthers,
    MenuCloseAll,
    // The tab strip's two buttons and what hangs off them; M8.8.1.
    // The `⋮` is about the window rather than about a row, so its
    // closing items say *tabs* where a tab's own menu does not have to.
    TabsHidden,
    TabActions,
    NewTabKinds,
    MenuRecentFiles,
    MenuCloseAllTabs,
    MenuCloseOtherTabs,
    MenuCloseLeft,
    MenuCloseRight,
    MenuCopyFileName,
    MenuCopyPathWithLine,
    BranchesTab,
    StashesTab,
    BranchSearch,
    BranchHint,
    BranchesLocal,
    BranchesNone,
    StashesNone,
    MenuDiffStaged,
    MenuDiffUnstaged,
    MenuInclude,
    MenuExclude,
    MenuShowBranchLog,
    MenuShowHeadLog,
    MenuCopyBranchName,
    MenuCopyRevisionShort,
    MenuCopyRevision,
    MenuCopySubject,
    MenuCopy,
    MenuPaste,
    MenuLarger,
    MenuSmaller,
    MenuActualSize,
    MenuSessionList,
    MenuGoToWaitingSession,
    // The column — M12.
    MenuToggleColumn,
    MenuRemoveWorktree,
    ActionCopyWorktreePath,
    MenuRenameProject,
    MenuMoveToWorkspace,
    MenuMoveToGroup,
    WorktreeTabsExpand,
    WorktreeTabsCollapse,
    MenuNewProjectGroup,
    MenuRemoveFromProjectGroup,
    MenuRenameProjectGroup,
    MenuDeleteProjectGroup,
    ProjectGroupNew,
    ProjectGroupRename,
    ProjectGroupNamePlaceholder,
    /// The question a project dropped on another asks.
    ProjectGroupMerge,
    /// Between the two projects' names in that question.
    ProjectGroupMergeAnd,
    /// Its answer that makes the group.
    ProjectGroupMergeConfirm,
    /// GLD-055: a project group bound to a folder.
    MenuGroupNewSession,
    MenuGroupNewSessionAsk,
    MenuGroupBindFolder,
    MenuGroupChangeFolder,
    GroupFolderLine,
    GroupFolderPlaceholder,
    GroupFolderNoSuggestion,
    GroupFolderChoose,
    GroupFolderBind,
    GroupFolderOwnRoot,
    GroupFolderOtherGroup,
    GroupFolderNewSession,
    GroupAddProjects,
    GroupMemberOutside,
    GroupNothingToAdd,
    GroupFolderLinked,
    GroupFolderGitBusy,
    MenuRemoveProject,
    MenuUnlockWorktree,
    ConfirmRemoveWorktree,
    NoticeWorktreeBusy,
    NoticeWorktreeChanged,
    ColumnNewWorktree,
    AgentClaudeCode,
    AgentOpenCode,
    AgentCodex,
    OpenCodeNeedsV2,
    /// A conversation continued while its last process is still closing.
    ResumeStillClosing,
    AgentsNone,
    AgentsTooltip,
    SessionsClear,
    SessionStateWaiting,
    SessionStateFailed,
    SessionStateWorking,
    SessionStateDone,
    SessionStateIdle,
    SessionsWaitingForYou,
    SessionsWorking,
    SessionsDone,
    SessionsIdle,
    SessionsSettled,
    SessionFilter,
    SessionFilterAgent,
    SessionFilterState,
    SessionFilterProject,
    SessionShowInProjects,
    SessionContinue,
    SessionsKeys,
    NewWorktreePlaceholder,
    ColumnOpen,
    /// The sidebar's heading over its projects — M12.4.
    SidebarProjects,
    SidebarSessions,
    ColumnShowSessions,
    ColumnShowProjects,
    /// The same heading while the workspace has a project group.
    SidebarProjectsAndGroups,
    ProjectSort,
    ProjectSortRecent,
    ProjectSortMost,
    ProjectSortUser,
    ProjectSortManual,
    SidebarPinned,
    WorktreeDeleteTitle,
    WorktreeDeleteTooltip,
    WorktreeDeleteDescription,
    WorktreeDeleteBranch,
    WorktreeDeleteBranchHint,
    WorktreeBranchProtected,
    WorktreeDetached,
    WorktreeBranchUnavailable,
    WorktreeDelete,
    WorktreeRemovalUnavailable,
    WorktreePin,
    WorktreeUnpin,
    MenuFullScreen,
    TerminalPreviousCommand,
    TerminalNextCommand,
    TerminalSelectOutput,
    TerminalCopyOutput,
    TerminalExitStatus,
    MenuClearTerminal,
    MenuNextTab,
    MenuPreviousTab,
    MenuNextProject,
    MenuNextWorkspace,
    MenuPreviousWorkspace,
    /// The areas' deeds — M12.8: the View menu's and the `⋮`'s rows.
    MenuSplitAreaRight,
    MenuSplitAreaDown,
    /// Divide the tab in front, the new pane to the right — M12.8.
    MenuSplitPaneRight,
    /// Divide the tab in front, the new pane below.
    MenuSplitPaneDown,
    MenuCloseArea,
    /// The arrows' names in Find Action — M12.8.
    ActionFocusLeft,
    ActionFocusRight,
    ActionFocusUp,
    ActionFocusDown,
    /// Why *Close Area* is not taken, in the row's stroke slot — lower
    /// case for that.
    LastAreaStays,
    /// The same, as the notice the View menu's *Close Area* posts on the
    /// last area — a sentence, so a capital.
    NoticeLastAreaStays,
    /// A split refused for want of room — decision 17.
    NoticeNoRoomToSplit,
    /// The same refusal for a tab's pane: the tab is what is divided.
    NoticeNoRoomToSplitTab,
    /// A tab's *Move to Area ▸* — M12.8, decision 21: the other areas by
    /// where they are, then the two that make a new one.
    MenuMoveToArea,
    MenuAreaLeft,
    MenuAreaRight,
    MenuAreaAbove,
    MenuAreaBelow,
    MenuNewAreaRight,
    MenuNewAreaBelow,
    WorkspaceNew,
    WorkspaceNamePlaceholder,
    WorkspaceRenameAction,
    WorkspaceRename,
    WorkspaceChangeIcon,
    WorkspaceChooseIcon,
    WorkspaceDelete,
    WorkspaceNoIcon,
    WorkspaceLastCannotDelete,
    WorkspaceCapacity,
    WorkspaceOnlyOne,
    MenuRecentProjects,
    MenuShowLog,
    MenuNew,
    MenuNewProject,
    MenuExistingSources,
    MenuOpenProject,
    MenuCloseProject,
    MenuAbout,
    /// Update Guild…, under About — M5.13; since M5.15 it applies the
    /// staged build, as the title bar's button does.
    MenuUpdateGuild,
    /// The title bar's button, while a newer build is staged — M5.15.
    UpdateButton,
    UpdaterReplaceTitle,
    UpdaterReplaceAction,
    UpdaterUpToDate,
    UpdateTerminal,
    UpdateTerminalUnavailable,
    UpdateSourceLocal,
    /// The title bar's ↓ hint — it opens the source popup.
    UpdateBranch,
    /// The confirming sheet's button.
    UpdateBranchConfirm,
    UpdateSourceSearch,
    UpdateBranchRunning,
    UpdateBranchChoose,
    UpdateBranchUnsaved,
    /// *What's new*'s head, and the button that puts it away — M5.16.
    WhatsNewTitle,
    WhatsNewContinue,
    /// The usage card's rows and words — M12.10.
    UsageSession,
    UsageWeek,
    UsageNothingToRead,
    /// *All providers* — the card's fold, M12.12.
    UsageAllProviders,
    /// *not installed* — no `codex` on the login shell's `PATH`.
    UsageNotInstalled,
    /// *couldn't read* — a provider asked and gave no windows.
    UsageCouldNotRead,
    /// *rate limited* — the server answered 429 and is waited out.
    UsageRateLimited,
    /// *not signed in* — no Claude Code credentials in the Keychain.
    UsageNotSignedIn,
    UsageRefresh,
    UsageAsking,
    /// The Open In button's tooltip — M12.10.
    OpenInButton,
    OpenInAppsTitle,
    OpenInAppsHint,
    OpenInAdd,
    OpenInName,
    OpenInChoose,
    OpenInSearch,
    OpenInRemove,
    OpenInUnavailable,
    OpenInLoading,
    OpenInNoResults,
    OpenInMoveUp,
    OpenInMoveDown,

    OpenSelectedApp,
    /// The click was answered yes and the window is on its way out.
    NoticeRestartingToUpdate,
    /// Update Guild with nothing newer staged.
    NoticeUpdateNothingStaged,
    /// Update Guild in a window that is no bundle — a debug build.
    NoticeUpdateNotABundle,
    /// Update Guild from a bundle that carries no helper.
    NoticeUpdateNoHelper,
    ConfirmClose,
    MenuOpenRecent,
    MenuClearRecent,
    MenuNoRecent,

    // Notices. What used to go only to the log.
    NoticeStateNotSaved,
    NoticeSettingsNotSaved,
    NoticeKeymapNotSaved,
    NoticeKeymapNotRead,
    NoticeNobodyWaiting,
    NoticeWorktreeGone,
    NoticeWorktreeOrderUnsupportedPath,

    // Appearance
    ThemeTitle,
    ThemeHint,
    ThemeSystem,
    ThemeLight,
    ThemeDark,
    CustomizeThemeTitle,
    CustomizeThemeHint,
    CustomizeThemeClose,
    ThemeBackground,
    ThemeAccent,
    ThemeHighlight,
    ThemeHue,
    ThemeSaturation,
    ThemeIntensity,
    ThemeCopyFromLight,
    ThemeCopyFromDark,
    ThemeSaveLight,
    ThemeSaveDark,
    ThemeReset,
    ThemeEyedropper,
    ThemeHexInvalid,
    ThemeDefaultSwatch,
    AppearanceSection,
    UiFontSizeTitle,
    UiFontSizeHint,
    UiFontTitle,
    TerminalFontTitle,
    /// Under *Terminal font* — M12.11.
    TerminalFontHint,
    /// The face of code, apart from the terminal's since M12.11.
    CodeFontTitle,
    CodeFontHint,
    /// The button that opens the list, beside the faces that ship.
    FontCustom,
    /// After Lilex Duo, as upstream labels it.
    FontBeta,
    FontFromSystem,
    LogDirectoryTitle,

    // Panels
    PanelProject,
    PanelSessions,
    PanelOutput,
    PanelTerminal,
    NewPanelTerminal,
    ClosePanelTerminal,
    PanelTerminalEmpty,
    RecentSearch,
    RecentNoMatch,
    RecentNever,
    PanelProjectEmpty,
    TreeEmpty,
    TreeCannotRead,
    // A file in a tab
    ImageSource,
    ImageFit,
    ImageLoading,
    ImageCannotRead,
    FileNotText,
    FileTooLarge,
    FileGone,
    FileCannotRead,
    SettingsMarkdown,
    MarkdownDefaultViewTitle,
    MarkdownDefaultViewHint,
    MarkdownEditor,
    MarkdownSplit,
    MarkdownPreview,
    MarkdownPreviewLimit,
    DiagramResourcesUnavailable,
    DiagramRendering,

    FileChangedOnDisk,
    MenuSave,
    MenuSaveAll,
    MenuReloadFile,
    UnsavedDetail,
    Save,
    DontSave,
    RevealActiveFile,
    NoFileInFront,
    // A diff in a tab
    ShowDiff,
    ShowChanges,
    DiffChanges,
    DiffLoading,
    DiffNoChanges,
    DiffBinary,
    DiffTooLarge,
    DiffNoRepository,
    DiffNoGit,
    // The rail and its faces
    Changes,
    Log,
    PanelChangesEmpty,
    Unversioned,
    // The diff's toolbar
    NextChange,
    PreviousChange,
    RollbackLines,
    BaseStaged,
    BaseCurrent,
    // A diff tab's base, after its name
    BaseIndex,
    BaseUnstaged,
    // The Commit panel's toolbar
    Refresh,
    ShowSelectedDiff,
    TickAll,
    UntickAll,
    CollapseAll,
    ExpandAll,
    // The commit box
    CommitPlaceholder,
    CommitButton,
    CommitNothingStaged,
    CommitNoMessage,
    MenuCommit,
    // The log
    LogLoading,
    /// The filter matched nothing — which is not the same as an empty log.
    LogNoMatches,
    LogLocal,
    LogSelectCommit,
    // The diff view
    DiffViewTitle,
    DiffViewHint,
    DiffSideBySide,
    DiffUnified,
    // Tools and docks
    ToolCommit,
    ToolGit,
    ToolSearch,
    ToolProblems,
    /// The Find tool: the usages of a name, a tab per search. M8.15.
    ToolFind,
    /// The Find tool with no search in it.
    PanelFindEmpty,
    /// The group of a Find tab's usages in the project's own files.
    UsagesInProjectFiles,
    /// The group of usages anywhere else: a jar, a toolchain.
    UsagesInLibraries,
    /// A Find tab whose server has not answered yet.
    UsagesSearching,
    /// The first line of the balloon over a caret that is on no name.
    UsagesCannotSearch,
    /// Its second line.
    UsagesCannotSearchHint,
    /// A usage whose file cannot be previewed — a library's.
    UsagesNoPreview,
    /// The Find tool's toolbar: ask again.
    FindRerun,
    /// The Find tool's toolbar: open every group, directory and file.
    FindExpandAll,
    /// The Find tool's toolbar: shut them.
    FindCollapseAll,
    ConsoleTab,
    LogSearch,
    LogBranchFilter,
    LogUserFilter,
    LogDateFilter,
    LogAnyone,
    LogAnyTime,
    LogLastDay,
    LogLastWeek,
    LogLastMonth,
    LogHead,
    /// The Branch filter unnarrowed: every branch git knows.
    LogAllBranches,
    /// The same, as a row of the branch column rather than a menu item.
    LogAllBranchesRow,
    LogHeadRow,
    LogBranchSearch,
    LogLocalBranches,
    LogRecentBranches,
    LogClearHint,
    ConsoleEmpty,
    MoveTo,
    /// The ⋮ at a panel head's right.
    PanelOptions,
    /// The bar beside it.
    PanelHide,
    DockLeft,
    DockRight,
    DockBottom,
    DockEmpty,
    DockEmptyHint,
    // Annotate
    Annotate,
    AnnotateTab,
    AnnotateNotCommitted,
    PanelLogEmpty,
    PanelSessionsEmpty,
    PanelOutputEmpty,
    PanelSearchEmpty,
    PanelProblemsEmpty,
    SearchPlaceholder,
    SearchMask,
    SearchMatchCase,
    SearchWholeWord,
    SearchRegex,
    SearchNothingFound,
    SearchRunning,
    SearchCapped,
    // The Output panel — M8.4
    RunNothingYet,
    RunStart,
    RunAgain,
    RunClosePrompt,
    RunStopAndClose,
    RunStop,
    RunNotRunning,
    RunClear,
    RunChoose,
    RunTaskSettings,
    RunConfigure,
    RunArgumentsRequired,
    RunVariables,
    RunDefault,
    BlameUnavailable,
    BlameCommitUnavailable,
    BlameFileUnavailable,
    RunSourceChanged,
    RunFailed,
    RunPlaceholder,
    RunRecent,
    RunDeclared,
    RunNothingToOffer,
    MenuRun,
    FontBundled,
    FontSearch,
    FontNoneFound,
    ServerNone,
    ServerStarting,
    /// A server in the list that is listening and quiet. M8.5.1.
    ServerReady,
    /// The list of servers, for a project whose files have wanted none.
    ServersNone,
    /// The status bar's server state, in words.
    ServersTooltip,
    /// The list's button beside a Swift server not started in an Xcode
    /// project: `xcode-build-server config`, 2026-09-28.
    XcodeSetUp,
    /// The same, while the command runs.
    XcodeSettingUp,
    GoToDefinition,
    BaseProposal,
    AcceptProposal,
    AcceptProposalHint,
    RejectProposal,
    RejectProposalHint,
    ProjectsSection,
    ReopenLastTitle,
    ReopenLastHint,
    AutoFastForwardTitle,
    AutoFastForwardHint,
    // GLD-063
    GitLabMrTitle,
    GitLabMrHint,
    MenuViewMr,
    MenuCopyMrUrl,
    ConfirmCloseTitle,
    ConfirmCloseHint,
    ConfirmCloseAgentTitle,
    ConfirmCloseAgentHint,
    ForgetOnCloseTitle,
    ForgetOnCloseHint,

    // ---- M8.8.5 — the status bar and Problems ----
    /// The caret's place at the right of the status bar, as a tooltip:
    /// the widget is a number pair and the tooltip is what pressing it
    /// does.
    GoToLine,
    /// What `LF` and `CRLF` mean, for the widget that is two letters.
    LineEndings,
    SoftWrapOn,
    SoftWrapOff,
    /// The Problems panel with nothing in it because nothing has looked.
    ProblemsNoServer,
    /// The same panel while a server is still starting or indexing.
    ProblemsLooking,
    /// A cap stopped the list. Said, because a count with no note is a
    /// lie about the project.
    ProblemsCapped,
    // The file tree's menu, its fields and its second section — M8.8.3.
    MenuNewFile,
    MenuNewDirectory,
    MenuOpenIn,
    MenuOpenTerminal,
    MenuFindInFiles,
    MenuReplaceInFiles,
    MenuRename,
    MenuMove,
    MenuCutFiles,
    MenuCopyFiles,
    MenuPasteFiles,
    /// The footnote of a directory that is itself a root Guild has open —
    /// GLD-057. One line, so short.
    MenuOpenAsProject,
    /// The footnote of a directory with an open root below it — GLD-057.
    MenuHoldsOpenProject,
    FileTransferCompleted,
    FileTransferRemaining,
    FileClipboardChanged,
    FileTransferBusy,
    FileTransferDestination,
    TrashUnsaved,
    FileTransferCollision,
    FileTransferSkip,
    FileTransferKeepBoth,

    MenuDelete,
    MenuAnnotate,
    /// The line numbers' right button — M12.9.
    MenuAnnotateWithBlame,
    MenuAddToVcs,
    TreeNewFile,
    TreeNewDirectory,
    TreeRename,
    Delete,
    SearchScope,
    ExternalLibraries,
    // Navigating — M8.8.2. One block, added at once rather than a label
    // at a time: an unused variant produces no warning and the two
    // exhaustive matches force both translations to exist, so the cost
    // of adding them early is nothing and the gain is that this branch
    // touches the three shared files once.
    MenuBack,
    MenuForward,
    MenuLastEdit,
    MenuRecentLocations,
    MenuGoToLine,
    MenuSelectInExplorer,
    RecentFilesSearch,
    RecentFilesNever,
    RecentFilesNoMatch,
    RecentFilesNoEdits,
    RecentFilesEdited,
    RecentFilesCloseHint,
    RecentLocationsSearch,
    RecentLocationsNever,
    GoToLinePlaceholder,

    // ---- M8.8.4 — git: the Commit panel's menu, and Push ----
    /// *Rollback…* in the Commit panel's menu. The ellipsis is honest:
    /// a sheet names the file before anything is written.
    MenuRollback,
    /// Its confirming button. Named after what it does, not *OK*.
    Rollback,
    /// The Push tab, in the strip and wherever it is named.
    PushTab,
    PushDialogTitle,
    PushTagsLabel,
    PushTagsAll,
    PushTagsCurrent,
    PushHooks,
    PushDetails,
    PushPreview,
    PushTarget,
    PushTargetApply,
    PushEmpty,
    PushHelp,
    PushNew,
    DiffEditSource,
    DiffWindowNextFile,
    DiffWindowPreviousFile,
    /// The button that pushes.
    PushButton,
    /// The button while git is talking to the network. A push is the
    /// one write here that takes seconds, so it says it is going.
    PushRunning,
    /// The branch has no upstream, so the push sets one — the familiar
    /// wording for the state.
    PushSetUpstream,
    /// A repository with no remote at all: there is nowhere to push,
    /// and that is not the same as nothing to push.
    PushNoRemote,
    /// A detached `HEAD`: there is no branch to send, which is a third
    /// thing again. Without it the head bar would simply be blank.
    PushDetached,
    /// Which remote the commits are going to, over the chooser.
    /// `cmd-alt-z` with nothing selected in the Commit panel. A stroke
    /// has no tooltip to grey out, so it says why it did nothing.
    ChangesSelectRow,
    /// `cmd-alt-z` on a file git has never seen: nothing to put back.
    RollbackNothingToUndo,
    /// `cmd-alt-a` on a file git already has.
    AlreadyInVcs,

    // ---- M5.11 — the audit's sharp edges ----
    // One block, given out before the four branches are cut, for the
    // reason M8.8.2's block gives: the two exhaustive matches force both
    // translations, and no branch has to touch the three shared files.
    /// Why a dependency's source tab did not save. It follows *lib.rs was
    /// not saved:*, so it starts in lower case. Task 6.
    FileReadOnly,
    /// A keymap slot refusing a stroke that would take a key from every
    /// shell and every buffer. Task 14.
    KeymapNeedsModifier,
    /// Its confirming button, named after what it does.
    DropStash,
    /// The entry the person chose has left the list since it was drawn,
    /// so nothing was dropped. Not a git error: git was told nothing.
    NoticeStashGone,
    /// A hunk write found its file, index entry or `HEAD` changed since the change was shown; nothing was written. GLD-026.
    NoticeHunkStale,
    /// git would store this file converted, so a hunk cannot go in as it is.
    NoticeHunkConverted,
    /// An Unstage was written and a commit landed between its check and the write.
    NoticeHunkHeadMoved,
    /// A hunk write git applied whose result could not be read back.
    NoticeHunkUnverified,
    /// A hunk write that ran past its deadline and could not be read back.
    NoticeHunkUnconfirmed,
    /// The control that puts a shown change into the index.
    StageHunk,
    /// The control that takes a shown change back out of the index.
    UnstageHunk,
    /// A hunk action disabled: a staged change straddles the shown block's edge.
    HunkWider,
    /// A hunk action disabled: part of the shown block is staged and part is not.
    HunkMixed,
    /// A hunk action disabled: the shown block differs from the index only in line endings.
    HunkLineEndings,
    /// A file whose changes go into the index only whole: new, deleted, renamed, binary, conflicted.
    HunkWholeFile,
    /// A commit waiting for a hunk write was not made: the write refused.
    NoticeHunkCommitCancelled,
    /// The login shell did not name its `PATH` in time. Task 23.
    NoticeLoginPathFailed,

    // ---- M5.12 — the terminal for agents ----
    // One block, given out before the four branches are cut, for the
    // reason M5.11's block gives. The close sheet reuses `ConfirmClose`
    // and `Cancel` — its title is `Ui::close_tab_title` since 2026-09-28;
    // the find bar `SearchMatchCase`, `SearchRegex` and
    // `SearchNothingFound`; the terminal's menu `MenuCopy`, `MenuPaste`
    // and `MenuClearTerminal`.
    /// The Edit menu's Select All, and a terminal's right-button item.
    MenuSelectAll,
    /// The Edit menu's Find…, and a terminal's right-button item.
    MenuFind,
    /// The find bar's placeholder. Task 9.
    FindPlaceholder,
    /// The find bar's button for the next match, further back.
    FindOlder,
    /// Its button for the previous match, further down.
    FindNewer,
    /// Its cross.
    FindClose,
    /// Under the bar while the program draws in the alternate screen,
    /// which keeps no history.
    FindScreenOnly,

    // ---- M8.9 — search and navigation ----
    // One block, given out by the seams commit before the three branches
    // are cut, for the reason M5.11's block gives. After it no group edits
    // this file: a group changes only the words of its own labels in
    // `english.rs` and `russian.rs`. Reused rather than declared: the
    // `Search…` toggles and notes, `Ui::matches_in_files` and
    // `Ui::notice_link_outside`.
    /// The Go menu's Search Everywhere — a double Shift.
    MenuSearchEverywhere,
    /// The Go menu's Go to File…
    MenuGoToFile,
    /// The Go menu's Go to Symbol…
    MenuGoToSymbol,
    /// The Go menu's Find Action…
    MenuFindAction,
    /// Search Everywhere's tabs: files, symbols and actions together. Group A.
    EverywhereAll,
    /// …the files.
    EverywhereFiles,
    /// …the symbols.
    EverywhereSymbols,
    /// …the actions.
    EverywhereActions,
    /// …the text in the files.
    EverywhereText,
    /// The checkbox at the head's right: the dependencies too.
    EverywhereNonProject,
    /// The toggle for the preview under the rows.
    EverywherePreview,
    /// While an index has not landed its first walk.
    EverywhereIndexing,
    /// A tab that asks nothing until something is typed.
    EverywhereTypeToSearch,
    /// The Find in Files popup's head. Group C.
    FindInFilesTitle,
    ReplaceInFilesTitle,
    /// The pin, while the popup closes after a result is opened.
    FindInFilesPin,
    /// The pin, while it is pinned.
    FindInFilesUnpin,
    /// `cmd-enter`: the search handed to the Search tool.
    FindInFilesOpenInWindow,
    /// The scope chip for the whole project.
    FindInFilesInProject,
    /// The scope chip for one directory.
    FindInFilesDirectory,
    /// The `…` beside it: the platform's chooser.
    FindInFilesChooseDirectory,
    /// The checkbox before the mask field.
    FindInFilesUseMask,
    /// The query field's chevron: what was searched before.
    FindInFilesHistory,
    /// The mask field's chevron.
    FindInFilesMaskHistory,
    /// The head's cross.
    FindInFilesClose,
    // Find Action's words for the actions whose menu word does not read
    // alone in a flat list — decision 15, and task 0's walk of all 94.
    // `actions::labels::label` uses exactly these, the menu's own words
    // for the rest, and `Ui::tool_window` and `Ui::move_tool`.
    /// Find Action's *New Session*.
    ActionNewSession,
    /// Find Action's *New Terminal*.
    ActionNewTerminal,
    /// Find Action's *Close Other Tabs*.
    ActionCloseOtherTabs,
    /// Find Action's *Close All Tabs*.
    ActionCloseAllTabs,
    /// Find Action's *Save File*.
    ActionSaveFile,
    /// Find Action's *Save All Files*.
    ActionSaveAllFiles,
    /// Find Action's *Run Command*.
    ActionRun,
    /// Find Action's *Stop Command*.
    ActionStopRun,
    /// Find Action's *Clear Output*.
    ActionClearRun,
    /// Find Action's *Open Project…*.
    ActionAddProject,
    /// Find Action's *New Project…*.
    ActionNewProject,
    /// Find Action's *Increase Terminal Font Size*.
    ActionIncreaseFontSize,
    /// Find Action's *Decrease Terminal Font Size*.
    ActionDecreaseFontSize,
    /// Find Action's *Reset Terminal Font Size*.
    ActionResetFontSize,
    /// Find Action's *Toggle Sidebar*.
    ActionToggleSidebar,
    /// Find Action's *Go to Definition*.
    ActionGoToDefinition,
    /// *Find Usages*. M8.15.
    ActionFindUsages,
    /// *Next Occurrence* in the Find tool.
    ActionNextUsage,
    /// *Previous Occurrence* in the Find tool.
    ActionPreviousUsage,
    /// Find Action's *Find in Terminal…*.
    ActionFind,
    ActionReplace,
    /// Find Action's *Next Change*.
    ActionNextChange,
    /// Find Action's *Previous Change*.
    ActionPreviousChange,
    /// Find Action's *Stage Hunk* — GLD-026.
    ActionStageHunk,
    /// Find Action's *Unstage Hunk*.
    ActionUnstageHunk,
    /// Find Action's *Refresh Changes*.
    ActionRefreshChanges,
    /// Find Action's *Show Diff of the Selected Change*.
    ActionShowSelectedDiff,
    /// Find Action's *Tick All Changes*.
    ActionIncludeSelectedChanges,
    ActionExcludeSelectedChanges,
    ActionTickAllChanges,
    /// Find Action's *Untick All Changes*.
    ActionUntickAllChanges,
    /// Find Action's *Collapse All Changes*.
    ActionCollapseChanges,
    /// Find Action's *Expand All Changes*.
    ActionExpandChanges,
    /// Find Action's *Previous Project*.
    ActionPreviousProject,
    /// Find Action's *Show Tab List*.
    ActionShowTabList,
    /// Find Action's *Push Commits…*.
    ActionPush,
    /// Find Action's *New File…*.
    ActionNewFile,
    /// Find Action's *New Directory…*.
    ActionNewDirectory,
    /// Find Action's *Rename File…*.
    ActionRenameFile,
    ActionJumpToSource,
    /// Find Action's *Move File to Trash…*.
    ActionDeleteFile,
    /// Find Action's *Toggle Left Panel*.
    ActionToggleLeftPanel,
    /// Find Action's *Toggle Right Panel*.
    ActionToggleRightPanel,
    /// Find Action's *Toggle Bottom Panel*.
    ActionToggleBottomPanel,
    /// Find Action's *Widen Panel*.
    ActionWidenPanel,
    /// Find Action's *Narrow Panel*.
    ActionNarrowPanel,
    /// Find Action's *Stretch to Left*.
    ActionStretchToLeft,
    /// Find Action's *Stretch to Right*.
    ActionStretchToRight,
    /// Find Action's *Stretch to Top*.
    ActionStretchToTop,
    /// Find Action's *Stretch to Bottom*.
    ActionStretchToBottom,
    /// Find Action's *Toggle Session List*.
    ActionToggleSessions,
    /// Find Action's *Toggle Full Screen*.
    ActionToggleFullScreen,
    /// Find Action's *Show Guild's Log*.
    ActionShowLog,
}

/// Labels in the chosen language.
///
/// A wrapper rather than methods on the language itself: the language type
/// lives in the settings and the translations live here, and Rust does not
/// allow adding methods to a foreign type.
#[derive(Debug, Clone, Copy)]
pub struct Ui(pub Language);

/// How much of a name a sheet's question carries: a stash's subject or a
/// session's first words can be a paragraph, and the question is one line.
const SHEET_NAME: usize = 40;

/// A name as a sheet's question says it.
fn sheet_name(name: &str) -> String {
    crate::title::truncate(name.trim(), SHEET_NAME)
}

impl Ui {
    /// The selected area of terminal launch history.
    pub fn terminal_scope(self, scope: guild_session::terminal_job::Scope) -> &'static str {
        use guild_session::terminal_job::Scope;
        self.text(match scope {
            Scope::Session => Text::TerminalThisSession,
            Scope::Worktree => Text::TerminalThisWorktree,
            Scope::Project => Text::TerminalThisProject,
            Scope::All => Text::TerminalAllLaunches,
        })
    }

    /// A stable calendar date and time; legacy jobs keep their unknown time.
    pub fn terminal_started(self, at_ms: Option<u64>, now: i64, offset: i64) -> String {
        let Some(at) = at_ms
            .and_then(|ms| i64::try_from(ms / 1000).ok())
            .and_then(|at| at.checked_add(offset))
        else {
            return self.text(Text::TerminalUnknownTime).to_owned();
        };
        let clock = at.rem_euclid(86_400);
        let time = format!(
            "{:02}:{:02}:{:02}",
            clock / 3600,
            clock % 3600 / 60,
            clock % 60
        );
        if now
            .checked_add(offset)
            .is_some_and(|today| today.div_euclid(86_400) == at.div_euclid(86_400))
        {
            time
        } else {
            format!("{} {time}", numeric_date(at))
        }
    }

    /// Process status is not a review verdict.
    pub fn terminal_status(self, receipt: &guild_session::terminal_job::JobReceipt) -> String {
        use guild_session::terminal_job::JobState;
        let label = match receipt.state {
            JobState::Starting if receipt.is_archive_view() => Text::TerminalOpeningArchive,
            JobState::Running if receipt.is_archive_view() => Text::TerminalViewingArchive,
            JobState::Cancelling if receipt.is_archive_view() => Text::TerminalClosingArchive,
            JobState::Starting => Text::TerminalStarting,
            JobState::Running => Text::TerminalRunning,
            JobState::Cancelling => Text::TerminalCancelling,
            JobState::FailedToStart => Text::TerminalFailed,
            JobState::Interrupted => Text::TerminalInterrupted,
            JobState::Exited => {
                return if let Some(signal) = &receipt.signal {
                    format!("{} {signal}", self.text(Text::TerminalSignal))
                } else {
                    format!(
                        "{} ({})",
                        self.text(Text::TerminalExited),
                        receipt
                            .exit_code
                            .map_or_else(|| "?".into(), |code| code.to_string())
                    )
                };
            }
        };
        self.text(label).to_owned()
    }

    /// The label in this language.
    ///
    /// An exhaustive `match` over two enums: a new language will not
    /// compile until every label is present in it.
    pub fn text(self, text: Text) -> &'static str {
        match self.0 {
            Language::English => english::english(text),
            Language::Russian => russian::russian(text),
        }
    }

    /// A tab label: "Session 2" or "Terminal 2".
    pub fn tab_label(self, is_session: bool, number: usize) -> String {
        let kind = self.text(if is_session {
            Text::Session
        } else {
            Text::Terminal
        });
        match self.0 {
            Language::English => format!("{kind} {number}"),
            Language::Russian => format!("{kind} {number}"),
        }
    }

    /// The bind question's title — GLD-055 decision 17.
    pub fn group_folder_title(self, group: &str) -> String {
        match self.0 {
            Language::English => format!("Bind {group} to a folder"),
            Language::Russian => format!("Привязать {group} к папке"),
        }
    }

    /// A bound path that is not there; Guild makes no folder.
    pub fn group_folder_missing(self, path: &str) -> String {
        match self.0 {
            Language::English => format!("There is no folder at {path}"),
            Language::Russian => format!("Папки {path} нет"),
        }
    }

    /// A bound path that is a file.
    pub fn group_folder_not_a_folder(self, path: &str) -> String {
        match self.0 {
            Language::English => format!("{path} is not a folder"),
            Language::Russian => format!("{path} — не папка"),
        }
    }

    /// Beside a project the header's `+` offers that is in another group.
    pub fn group_member_of(self, group: &str) -> String {
        match self.0 {
            Language::English => format!("in {group}"),
            Language::Russian => format!("в {group}"),
        }
    }

    /// A group folder row's second line — decision 14: how many of the
    /// members sit directly in the folder.
    pub fn group_inside(self, inside: usize, members: usize) -> String {
        match (self.0, inside == members) {
            (Language::English, true) if members == 1 => "1 project inside".to_owned(),
            (Language::English, true) => format!("{members} projects inside"),
            (Language::English, false) => format!("{inside} of {members} inside"),
            (Language::Russian, true) => format!(
                "{} в папке",
                russian_count(members as i64, "проект", "проекта", "проектов")
            ),
            (Language::Russian, false) => format!("{inside} из {members} в папке"),
        }
    }

    /// How many sessions a project has.
    pub fn project_sessions(self, count: usize) -> String {
        match self.0 {
            Language::English => format!("{count} sessions"),
            Language::Russian => format!("сессий {count}"),
        }
    }

    /// What the server says is wrong with the file in front, or nothing
    /// when it says nothing.
    ///
    /// **The count and not a mark**: a squiggle says *there is something
    /// here*, and a person looking at the top of a file wants to know
    /// whether there is anything below. It is the nearest thing this
    /// window has to the gutter the roadmap asked for — the component
    /// owns the gutter, which is decision 10 of M8.3.
    pub fn diagnostics(self, errors: usize, warnings: usize) -> Option<String> {
        if errors == 0 && warnings == 0 {
            return None;
        }
        let mut parts = Vec::new();
        if errors > 0 {
            parts.push(match (self.0, errors) {
                (Language::English, 1) => "1 error".to_owned(),
                (Language::English, n) => format!("{n} errors"),
                (Language::Russian, n) => format!("ошибок {n}"),
            });
        }
        if warnings > 0 {
            parts.push(match (self.0, warnings) {
                (Language::English, 1) => "1 warning".to_owned(),
                (Language::English, n) => format!("{n} warnings"),
                (Language::Russian, n) => format!("предупреждений {n}"),
            });
        }
        Some(parts.join(" · "))
    }

    /// A Find tab's title — *feign.Contract in Project Files*.
    pub fn usages_title(self, name: &str) -> String {
        match self.0 {
            Language::English => format!("{name} in Project Files"),
            Language::Russian => format!("{name} в файлах проекта"),
        }
    }

    /// How many usages are under a group or a row, beside its name.
    pub fn usages_count(self, n: usize) -> String {
        match (self.0, n) {
            (Language::English, 1) => "1 result".to_owned(),
            (Language::English, n) => format!("{n} results"),
            (Language::Russian, n) => {
                russian_count(n as i64, "результат", "результата", "результатов")
            }
        }
    }

    /// The balloon over a name nobody uses — two lines, the name first.
    /// M8.15.
    pub fn usages_nothing_found(self, name: &str) -> (String, String) {
        match self.0 {
            Language::English => (
                format!("Usages of {name}"),
                "Nothing found in 'Project Files'".to_owned(),
            ),
            Language::Russian => (
                format!("Использования {name}"),
                "В файлах проекта ничего не найдено".to_owned(),
            ),
        }
    }

    /// The balloon when the language's server has not agreed to anything
    /// yet — it is starting, or indexing before it answers.
    pub fn usages_server_starting(self, command: &str) -> String {
        match self.0 {
            Language::English => format!("{command} is still starting — try again in a moment"),
            Language::Russian => format!("{command} ещё запускается — попробуйте через мгновение"),
        }
    }

    /// The balloon when the server refused, **in its own words** after
    /// ours: `no handler for request: textDocument/references` is what a
    /// person looks up.
    pub fn usages_refused(self, command: &str, said: &str) -> String {
        match self.0 {
            Language::English => format!("{command} would not search: {said}"),
            Language::Russian => format!("{command} не стал искать: {said}"),
        }
    }

    /// The balloon when the server did not answer in time.
    pub fn usages_timed_out(self, command: &str) -> String {
        match self.0 {
            Language::English => format!("{command} did not answer in time"),
            Language::Russian => format!("{command} не ответил вовремя"),
        }
    }

    /// A definition that is not in this window's projects — the
    /// standard library, a crate under `~/.cargo`, a `jar:` URI. Said
    /// with the path, because the path is what a person does the next
    /// thing with.
    pub fn notice_definition_elsewhere(self, where_: &str) -> String {
        match self.0 {
            Language::English => format!("Defined outside this project: {where_}"),
            Language::Russian => format!("Определено вне проекта: {where_}"),
        }
    }

    /// The close sheet's title for several tabs at once — M5.12's task 1.
    /// One tab's is the menu item's words and its name.
    pub fn close_tabs_title(self, n: usize) -> String {
        match self.0 {
            Language::English => format!("Close {n} tabs with agents?"),
            Language::Russian => format!(
                "Закрыть {} с агентами?",
                russian_count(n as i64, "вкладку", "вкладки", "вкладок")
            ),
        }
    }

    /// The close sheet's title for one tab: the question, with the tab's
    /// name and what its agent is doing — the whole sheet since
    /// 2026-09-28.
    pub fn close_tab_title(self, name: &str, stake: crate::closing::Stake) -> String {
        use crate::closing::Stake;
        match (self.0, stake) {
            (Language::English, Stake::Working) => format!("Close “{name}” mid-turn?"),
            (Language::English, Stake::Waiting) => format!("Close “{name}” without answering?"),
            (Language::English, Stake::Open) => format!("Close “{name}”?"),
            (Language::Russian, Stake::Working) => format!("Закрыть «{name}» посреди хода?"),
            (Language::Russian, Stake::Waiting) => {
                format!("Закрыть «{name}», не ответив агенту?")
            }
            (Language::Russian, Stake::Open) => format!("Закрыть «{name}»?"),
        }
    }

    /// A `cmd`-click on a path outside the project — M5.12's task 12.
    /// Said with the path, section 6.21's rule for `f12`.
    pub fn notice_link_outside(self, path: &str) -> String {
        match self.0 {
            Language::English => format!("Outside this project: {path}"),
            Language::Russian => format!("Вне проекта: {path}"),
        }
    }

    /// A `cmd`-click on a path whose file has gone since the pointer found
    /// it — M5.12's task 12.
    pub fn notice_link_missing(self, path: &str) -> String {
        match self.0 {
            Language::English => format!("No such file: {path}"),
            Language::Russian => format!("Нет такого файла: {path}"),
        }
    }

    /// A `cmd`-click on a program: macOS would run it rather than show it,
    /// so it is not handed over.
    pub fn notice_link_program(self, path: &str) -> String {
        match self.0 {
            Language::English => format!("Not opened, it is a program or an installer: {path}"),
            Language::Russian => format!("Не открыто, это программа или установщик: {path}"),
        }
    }

    /// A `cmd`-click on a file Guild cannot show, which macOS would not
    /// open either — with the system's words.
    pub fn notice_link_not_handed_over(self, path: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!("Could not open {path}: {reason}"),
            Language::Russian => format!("Не удалось открыть {path}: {reason}"),
        }
    }

    /// A tab continued a conversation that is open elsewhere — M13.3.
    pub fn notice_open_elsewhere(self, name: &str, place: &str) -> String {
        match self.0 {
            Language::English => format!(
                "\u{201c}{name}\u{201d} is already open in another tab ({place}) — two processes are now writing one conversation; close one"
            ),
            Language::Russian => format!(
                "«{name}» уже открыта в другой вкладке ({place}) — теперь один разговор пишут два процесса; закройте один"
            ),
        }
    }

    /// A tab continued a conversation that belongs to another root — M13.3.
    pub fn notice_other_seat(self, name: &str, place: &str) -> String {
        match self.0 {
            Language::English => format!(
                "\u{201c}{name}\u{201d} belongs to {place} — continue it there, and that tab becomes its tab"
            ),
            Language::Russian => format!(
                "«{name}» относится к {place} — продолжите её там, и та вкладка станет её вкладкой"
            ),
        }
    }

    /// Why `f12` did nothing: this language has no server configured.
    ///
    /// The language is named because that is the whole fact. *no server*
    /// alone reads as something being broken; *Go has no language server
    /// here* reads as what it is — a row of the table with an empty
    /// `server` field, and nothing wrong with the file.
    pub fn no_server_for(self, language: &str) -> String {
        match self.0 {
            Language::English => format!("{language} has no language server here"),
            Language::Russian => format!("для {language} здесь нет языкового сервера"),
        }
    }

    /// A harness asked for from the start screen whose program the login
    /// shell's `PATH` does not have — said instead of opening a tab that
    /// dies at once. The program is named, as a server's command is,
    /// because it is what a person installs. M14.5.
    pub fn not_on_path(self, program: &str) -> String {
        match self.0 {
            Language::English => format!("{program} is not on the PATH"),
            Language::Russian => format!("{program} нет в PATH"),
        }
    }

    /// Why a language that names a server has none running. M8.5.1.
    ///
    /// The command is named rather than the language, because the command
    /// is what a person installs or looks up.
    pub fn server_fault(self, command: &str, fault: &crate::lsp::progress::Fault) -> String {
        use crate::lsp::progress::Fault;
        match (self.0, fault) {
            (Language::English, Fault::Missing) => format!("{command} not found"),
            (Language::English, Fault::Dying) => format!("{command} keeps stopping"),
            (Language::English, Fault::Refused(why)) => {
                format!("{command} refused to start: {why}")
            }
            (Language::Russian, Fault::Missing) => format!("{command} не найден"),
            (Language::Russian, Fault::Dying) => format!("{command} всё время падает"),
            (Language::Russian, Fault::Refused(why)) => {
                format!("{command} отказался запускаться: {why}")
            }
            (Language::English, Fault::Unprepared(files)) => {
                format!("{command} needs {}", files.join(" or "))
            }
            (Language::Russian, Fault::Unprepared(files)) => {
                format!("{command} нужен {}", files.join(" или "))
            }
        }
    }

    /// Why *Set up for Xcode* wrote no `buildServer.json`, in what the
    /// tool said. 2026-09-28.
    pub fn xcode_setup_failed(self, why: &str) -> String {
        match self.0 {
            Language::English => format!("Not set up: {why}"),
            Language::Russian => format!("Не настроено: {why}"),
        }
    }

    /// Why `f12` into a Kotlin or Java dependency opened nothing: the
    /// class is in the jar and its source was never downloaded. M8.11.
    ///
    /// The archive is named, because that is what a person then adds to
    /// their build — Gradle's `withSources()`, Maven's sources goal —
    /// and because *defined outside this project* sent them looking for a
    /// setting of ours instead.
    pub fn no_downloaded_sources(self, jar: &str) -> String {
        match self.0 {
            Language::English => format!("No sources downloaded for {jar}"),
            Language::Russian => format!("Для {jar} не скачаны исходники"),
        }
    }

    /// A dependency the build system resolved and nothing has downloaded
    /// — a Go module, a gem. M8.14.
    pub fn dependency_not_on_disk(self, dependency: &str) -> String {
        match self.0 {
            Language::English => {
                format!("{dependency} is not on the disk: it has not been downloaded")
            }
            Language::Russian => format!("{dependency} нет на диске: зависимость не скачана"),
        }
    }

    /// A build system that could not be asked what a project depends on,
    /// and the end of what it said. M8.14.
    pub fn build_system_not_asked(self, stack: &str, why: &str) -> String {
        match self.0 {
            Language::English => format!("{stack} could not list the dependencies: {why}"),
            Language::Russian => format!("{stack} не смог перечислить зависимости: {why}"),
        }
    }

    /// A path the disk refused to make, and why.
    ///
    /// One sentence for a file and for a directory: the path says which
    /// it was, and two texts differing only in a noun are two places to
    /// translate the same thing.
    ///
    /// Both halves are in the text rather than only in the log: "could
    /// not create the directory" without saying which one, or why, sends
    /// a person to the log — the thing a notice exists to save them from.
    pub fn notice_not_created(self, path: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!("Could not create {path}: {reason}"),
            Language::Russian => format!("Не удалось создать {path}: {reason}"),
        }
    }

    /// A rename the disk refused, and why.
    pub fn notice_not_renamed(self, name: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!("Could not rename {name}: {reason}"),
            Language::Russian => format!("Не удалось переименовать {name}: {reason}"),
        }
    }

    /// A rename, a move or a Trash refused because the row is, or holds,
    /// a root Guild has open — GLD-057. `root` is that root's path in the
    /// tree, so a folder holding one names the one inside it: the thing to
    /// close.
    pub fn notice_open_root(self, root: &str) -> String {
        match self.0 {
            Language::English => {
                format!("“{root}” is open in Guild as a project — close it first")
            }
            Language::Russian => {
                format!("«{root}» открыт в Guild как проект — сначала закройте его")
            }
        }
    }

    /// A rename refused because the name is already taken. M5.11's task 7.
    ///
    /// Its own sentence rather than [`Ui::notice_not_renamed`] with the
    /// kernel's *File exists*: the field stays up with the name as typed,
    /// and what a person needs to read is that the name is the problem.
    pub fn notice_name_taken(self, name: &str) -> String {
        match self.0 {
            Language::English => format!("“{name}” already exists here"),
            Language::Russian => format!("«{name}» здесь уже есть"),
        }
    }

    /// What the platform said about a file it would not move to the
    /// Trash.
    ///
    /// **AppKit's own sentence, and it already names the file** — *The
    /// file “x” doesn’t exist.* A message of ours would be a worse
    /// version of one the system already wrote, in a language it has
    /// already chosen.
    pub fn notice_not_trashed(self, said: &str) -> String {
        match self.0 {
            Language::English => format!("Not moved to the Trash: {said}"),
            Language::Russian => format!("Не перемещено в Корзину: {said}"),
        }
    }

    // ---- The sheets that ask before a gesture, 2026-09-28 ----
    // **A question with its object in it, and a line under it only where
    // something is lost for good.** Every sheet was a title, the object
    // named again and two sentences; the person found them long. A name is
    // said once, in the question, and cut to `SHEET_NAME` characters.

    /// Before a file with unsaved changes closes.
    pub fn unsaved_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Save changes to “{name}”?"),
            Language::Russian => format!("Сохранить изменения в «{name}»?"),
        }
    }

    /// Before a project with agents at work closes — which is the only
    /// time it asks, so the question says why.
    pub fn close_project_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Close “{name}” while agents are at work?"),
            Language::Russian => format!("Закрыть «{name}», пока работают агенты?"),
        }
    }

    /// Before a project leaves the workspace.
    pub fn remove_project_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Remove “{name}” from the workspace?"),
            Language::Russian => format!("Убрать «{name}» из workspace?"),
        }
    }

    /// Under it, only when its files hold changes nobody saved: its
    /// folders stay and its conversations can be resumed, so nothing else
    /// is lost.
    pub fn remove_project_detail(self, unsaved: bool) -> Option<&'static str> {
        unsaved.then_some(match self.0 {
            Language::English => "Unsaved changes in its files are lost.",
            Language::Russian => "Несохранённые изменения в его файлах пропадут.",
        })
    }

    /// Before a worktree's folder is deleted — decision 33's sheet.
    pub fn remove_worktree_title(self, folder: &str) -> String {
        let folder = sheet_name(folder);
        match self.0 {
            Language::English => format!("Remove worktree “{folder}”?"),
            Language::Russian => format!("Удалить worktree «{folder}»?"),
        }
    }

    /// Under it: what git deletes without a word, and the branch, which
    /// it does not.
    pub fn confirm_remove_worktree(self, branch: Option<&str>) -> String {
        let mut said = match self.0 {
            Language::English => "Files git ignores in it go too: build output, .env.".to_owned(),
            Language::Russian => {
                "Удалятся и файлы, которые git в нём игнорирует: сборка, .env.".to_owned()
            }
        };
        if let Some(branch) = branch {
            said.push_str(&match self.0 {
                Language::English => format!(" Branch {branch} stays."),
                Language::Russian => format!(" Ветка {branch} останется."),
            });
        }
        said
    }

    /// Before a change is rolled back — decision 30's sheet.
    pub fn rollback_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Roll back “{name}”?"),
            Language::Russian => format!("Откатить «{name}»?"),
        }
    }

    /// Under it, for a file git tracks: what it holds now is gone. A file
    /// only added stays on the disk, and gets no line.
    pub fn confirm_rollback(self) -> &'static str {
        match self.0 {
            Language::English => "Its current contents are lost.",
            Language::Russian => "Текущее содержимое пропадёт.",
        }
    }

    /// Before a row of the tree goes to the Trash, which gives it back.
    pub fn delete_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Move “{name}” to the Trash?"),
            Language::Russian => format!("Переместить «{name}» в Корзину?"),
        }
    }

    /// Before a stash entry is dropped.
    pub fn drop_stash_title(self, subject: &str) -> String {
        let subject = sheet_name(subject);
        match self.0 {
            Language::English => format!("Drop “{subject}” from the stash?"),
            Language::Russian => format!("Удалить «{subject}» из stash?"),
        }
    }

    /// Under it: git keeps the commit only until it collects garbage, and
    /// the Console has the hash that finds it.
    pub fn confirm_drop_stash(self) -> &'static str {
        match self.0 {
            Language::English => "Only its hash in the Git console can bring it back.",
            Language::Russian => "Вернуть его можно только по хешу из консоли Git.",
        }
    }

    /// Before a workspace is deleted.
    pub fn delete_workspace_title(self, name: &str) -> String {
        let name = sheet_name(name);
        match self.0 {
            Language::English => format!("Delete “{name}”?"),
            Language::Russian => format!("Удалить «{name}»?"),
        }
    }

    /// Under it: its tabs and sessions, and the projects no other
    /// workspace holds, which leave Guild with it.
    pub fn delete_workspace_detail(self, released: &[String]) -> String {
        let mut said = match self.0 {
            Language::English => "Its tabs and sessions close for good.".to_owned(),
            Language::Russian => "Его вкладки и сессии закроются насовсем.".to_owned(),
        };
        if !released.is_empty() {
            let list = released.join(", ");
            said.push_str(&match self.0 {
                Language::English => format!(" These projects leave Guild: {list}."),
                Language::Russian => format!(" Из Guild уйдут проекты: {list}."),
            });
        }
        said
    }

    /// A commit refused in a folder opened inside its repository, because
    /// git holds staged paths outside the folder that the commit would
    /// take too. M5.11's task 20.
    ///
    /// The first three paths by name and the rest counted: the names are
    /// what a person unstages next, and a list of forty is not a notice.
    pub fn commit_refused_outside(self, paths: &[String]) -> String {
        const NAMED: usize = 3;
        let named = paths
            .iter()
            .take(NAMED)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(", ");
        let rest = paths.len().saturating_sub(NAMED);
        match self.0 {
            Language::English => {
                let list = if rest == 0 {
                    named
                } else {
                    format!("{named} and {rest} more")
                };
                format!(
                    "Not committed: git has {list} staged outside this folder, and a commit \
                     here would take it too"
                )
            }
            Language::Russian => {
                let list = if rest == 0 {
                    named
                } else {
                    format!("{named} и ещё {rest}")
                };
                format!(
                    "Коммита не было: вне этой папки в индексе {list} — коммит отсюда забрал бы \
                     и это"
                )
            }
        }
    }

    /// A project's git thread died and a fresh one was started. M5.11's
    /// task 21.
    pub fn notice_git_restarted(self, project: &str) -> String {
        match self.0 {
            Language::English => {
                format!("git stopped answering for {project} and was started again")
            }
            Language::Russian => format!("git перестал отвечать для {project} и запущен заново"),
        }
    }

    /// A project's git thread died a second time and is left dead.
    pub fn notice_git_stopped(self, project: &str) -> String {
        match self.0 {
            Language::English => format!(
                "git stopped answering for {project} again; reopen the project to try once more"
            ),
            Language::Russian => format!(
                "git снова перестал отвечать для {project}; откройте проект заново, чтобы \
                 попробовать ещё раз"
            ),
        }
    }

    /// A state file that would not read was moved aside rather than
    /// written over. M5.11's task 22.
    ///
    /// Both names are in the text, because the kept name is the thing a
    /// person goes to the disk with, and whether the previous copy was
    /// used is whether their tabs came back.
    pub fn notice_state_unreadable(self, file: &str, kept: &str, from_backup: bool) -> String {
        match (self.0, from_backup) {
            (Language::English, true) => {
                format!(
                    "{file} could not be read and was kept as {kept}; the previous copy was used"
                )
            }
            (Language::English, false) => {
                format!("{file} could not be read and was kept as {kept}; Guild started without it")
            }
            (Language::Russian, true) => {
                format!("{file} не прочитан и сохранён как {kept}; взята предыдущая копия")
            }
            (Language::Russian, false) => {
                format!("{file} не прочитан и сохранён как {kept}; Guild запущен без него")
            }
        }
    }

    /// A state file that could not be read at all, left where it is and
    /// not saved over until Guild restarts. Not [`Ui::notice_state_unreadable`]:
    /// nothing was moved, and nothing is known to be wrong with the file.
    /// The system's reason is the person's way to the cause.
    pub fn notice_state_not_read(self, file: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!(
                "{file} could not be read ({reason}); it was left as it is and will not be \
                 saved over until Guild restarts"
            ),
            Language::Russian => format!(
                "{file} не прочитан ({reason}); он оставлен как есть и не будет \
                 перезаписан до перезапуска Guild"
            ),
        }
    }

    /// Why `f12` did not open a file: this language ships its
    /// dependencies compiled, and Guild decompiles nothing.
    ///
    /// The shape [`Ui::no_server_for`] uses, and for the same reason: the
    /// language named is the whole fact, and *nothing to open* alone
    /// reads as something being broken.
    pub fn no_sources_for(self, language: &str) -> String {
        match self.0 {
            Language::English => format!("a {language} dependency has no sources here"),
            Language::Russian => format!("у зависимости {language} здесь нет исходников"),
        }
    }

    /// How long ago, at a glance: the last minute is *just now*, then
    /// minutes, hours, *yesterday*, days — and a date once a week is
    /// past, because "9 days ago" makes a person count. A time ahead of
    /// ours — a clock that is wrong somewhere — is *just now* too.
    /// A commit's date, the way the log column says it.
    ///
    /// Four shapes, and each earns its place. **Under an hour** stays
    /// relative: that is what a person is reading the top of the log for.
    /// **Today and yesterday** get a clock rather than "3 hours ago",
    /// because three hours and five hours are the same glance while
    /// `11:41` is not. **Older** is a date and nothing else — the hour of
    /// a commit from July answers no question anyone has.
    ///
    /// `offset` is the window's seconds east of UTC, passed in rather
    /// than looked up, so this stays pure and a test can ask about the
    /// far side of midnight. A column that said UTC would put half the
    /// world's commits on the wrong day.
    pub fn commit_date(self, at: i64, now: i64, offset: i64) -> String {
        let ago = now.saturating_sub(at).max(0);
        if ago < 3600 {
            return self.relative_time(at, now);
        }

        let day = |seconds: i64| (seconds + offset).div_euclid(86_400);
        let (today, then) = (day(now), day(at));
        let clock = {
            let local = (at + offset).rem_euclid(86_400);
            format!("{:02}:{:02}", local / 3600, (local % 3600) / 60)
        };

        match (self.0, today - then) {
            (Language::English, 0) => format!("Today {clock}"),
            (Language::English, 1) => format!("Yesterday {clock}"),
            (Language::Russian, 0) => format!("Сегодня {clock}"),
            (Language::Russian, 1) => format!("Вчера {clock}"),
            _ => numeric_date(at + offset),
        }
    }

    /// *In 2 branches: main, m8-3-4-git-tool.*
    ///
    /// The count first, because that is the answer; the names after it,
    /// because the next question is *which*. Both languages count their
    /// own way, which is why this is not a format string.
    pub fn in_branches(self, branches: &[String]) -> String {
        let names = branches.join(", ");
        let n = branches.len();
        match self.0 {
            Language::English => {
                let word = if n == 1 { "branch" } else { "branches" };
                format!("In {n} {word}: {names}")
            }
            Language::Russian => {
                format!(
                    "{}: {names}",
                    russian_count(n as i64, "В ветке", "В ветках", "В ветках")
                )
            }
        }
    }

    pub fn relative_time(self, at: i64, now: i64) -> String {
        let ago = now.saturating_sub(at).max(0);
        let (minutes, hours, days) = (ago / 60, ago / 3600, ago / 86_400);
        match self.0 {
            Language::English => {
                if ago < 60 {
                    "just now".to_owned()
                } else if hours < 1 {
                    format!(
                        "{minutes} minute{} ago",
                        if minutes == 1 { "" } else { "s" }
                    )
                } else if days < 1 {
                    format!("{hours} hour{} ago", if hours == 1 { "" } else { "s" })
                } else if days < 2 {
                    "yesterday".to_owned()
                } else if days < 7 {
                    format!("{days} days ago")
                } else {
                    date(at, &MONTHS_EN)
                }
            }
            Language::Russian => {
                if ago < 60 {
                    "только что".to_owned()
                } else if hours < 1 {
                    format!(
                        "{} назад",
                        russian_count(minutes, "минуту", "минуты", "минут")
                    )
                } else if days < 1 {
                    format!("{} назад", russian_count(hours, "час", "часа", "часов"))
                } else if days < 2 {
                    "вчера".to_owned()
                } else if days < 7 {
                    format!("{} назад", russian_count(days, "день", "дня", "дней"))
                } else {
                    date(at, &MONTHS_RU)
                }
            }
        }
    }

    /// The diff's toolbar: how many hunks one file has.
    pub fn differences(self, hunks: usize) -> String {
        match self.0 {
            Language::English => {
                if hunks == 1 {
                    "1 difference".to_owned()
                } else {
                    format!("{hunks} differences")
                }
            }
            Language::Russian => russian_count(hunks as i64, "отличие", "отличия", "отличий"),
        }
    }

    /// What a search came to: *128 matches in 21 files*. M8.7.
    pub fn matches_in_files(self, matches: usize, files: usize) -> String {
        match self.0 {
            Language::English => {
                let one = if matches == 1 { "match" } else { "matches" };
                let file = if files == 1 { "file" } else { "files" };
                format!("{matches} {one} in {files} {file}")
            }
            Language::Russian => format!(
                "{} в {}",
                russian_count(matches as i64, "совпадение", "совпадения", "совпадений"),
                russian_count(files as i64, "файле", "файлах", "файлах"),
            ),
        }
    }

    /// Find Action's name for a tool window's action — *Git Tool Window*,
    /// *Окно «Git»* — the tool's word from `view::panels::title_of`. For
    /// `ShowExplorerPanel`, `ShowCommitPanel`, `ShowLogPanel`,
    /// `ShowOutputPanel`, `ShowSearchPanel` and `ShowProblems`. M8.9.
    pub fn tool_window(self, tool: Text) -> String {
        let tool = self.text(tool);
        match self.0 {
            Language::English => format!("{tool} Tool Window"),
            Language::Russian => format!("Окно «{tool}»"),
        }
    }

    /// Find Action's name for one of the twelve `Move…To…` actions:
    /// *Move Git to the Bottom*, *Переместить «Git» вниз*. M8.9.
    pub fn move_tool(self, tool: Text, dock: guild_session::Dock) -> String {
        use guild_session::Dock;
        let tool = self.text(tool);
        match (self.0, dock) {
            (Language::English, Dock::Left) => format!("Move {tool} to the Left"),
            (Language::English, Dock::Right) => format!("Move {tool} to the Right"),
            (Language::English, Dock::Bottom) => format!("Move {tool} to the Bottom"),
            (Language::Russian, Dock::Left) => format!("Переместить «{tool}» влево"),
            (Language::Russian, Dock::Right) => format!("Переместить «{tool}» вправо"),
            (Language::Russian, Dock::Bottom) => format!("Переместить «{tool}» вниз"),
        }
    }

    /// Search Everywhere's *More* row under a section of *All* that holds
    /// more than it shows: *12 more*, *ещё 12*. M8.9.
    pub fn more_rows(self, count: usize) -> String {
        match self.0 {
            Language::English => format!("{count} more"),
            Language::Russian => format!("ещё {count}"),
        }
    }

    /// The diff's toolbar on the working-tree tab: which file the top
    /// of the view is in, of how many.
    pub fn files_counter(self, current: usize, total: usize) -> String {
        match self.0 {
            Language::English => format!("{current} / {total} files"),
            Language::Russian => format!("{current} / {total} файлов"),
        }
    }

    /// What stands under a branch's name in the popup: who wrote the
    /// tip, how long ago, and what they called it, separated by the
    /// same middle dot the session rows use. A branch whose tip git
    /// said nothing about — an older capture — says nothing here
    /// either, rather than three empty fields and two dots.
    pub fn branch_tip(self, author: &str, age: &str, subject: &str) -> String {
        let parts: Vec<&str> = [author, age, subject]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect();
        parts.join(" \u{b7} ")
    }

    /// The row that offers to make what was typed.
    pub fn create_branch(self, name: &str) -> String {
        match self.0 {
            Language::English => format!("Create branch \"{name}\""),
            Language::Russian => format!("Создать ветку «{name}»"),
        }
    }

    /// Under the new worktree's field: what it will start from. M12.2.
    pub fn new_worktree_from(self, branch: &str) -> String {
        match self.0 {
            Language::English => format!("from {branch}"),
            Language::Russian => format!("от {branch}"),
        }
    }

    pub fn create_worktree(self, name: &str) -> String {
        match self.0 {
            Language::English => format!("Create worktree \"{name}\""),
            Language::Russian => format!("Создать worktree «{name}»"),
        }
    }

    /// The line above the message box: how many changes are ticked.
    pub fn modified_count(self, ticked: usize) -> String {
        match self.0 {
            Language::English => format!("{ticked} modified"),
            Language::Russian => {
                let word = if ticked % 10 == 1 && ticked % 100 != 11 {
                    "изменён"
                } else {
                    "изменено"
                };
                format!("{ticked} {word}")
            }
        }
    }

    /// A commit that landed: the short hash and the summary, which is
    /// what a person looks for in the log a moment later.
    pub fn notice_committed(self, short: &str, summary: &str) -> String {
        match self.0 {
            Language::English => format!("Committed {short}: {summary}"),
            Language::Russian => format!("Коммит {short}: {summary}"),
        }
    }

    /// A commit refused before git was asked: a tab could not be written,
    /// and a commit of the disk would not be a commit of the screen.
    pub fn commit_refused_by(self, name: &str) -> String {
        match self.0 {
            Language::English => format!("Not committed: {name} could not be saved"),
            Language::Russian => format!("Коммита не было: {name} не удалось сохранить"),
        }
    }

    /// The message file could not be written, in the OS's words.
    pub fn notice_message_not_written(self, reason: &str) -> String {
        match self.0 {
            Language::English => format!("Could not write the commit message: {reason}"),
            Language::Russian => format!("Не удалось записать сообщение коммита: {reason}"),
        }
    }

    /// What became of a run, and how long it took: the line the Output
    /// panel's toolbar ends with.
    ///
    /// **The code is the point.** `just check 2>&1 | tail` exits 0
    /// whatever happened inside it, and this project has already closed
    /// three commits on a check that read as green — a panel that shows
    /// the output and not the code repeats that in a nicer font.
    pub fn run_state(self, state: &crate::run::State, taken: &str) -> String {
        use crate::run::State;
        match (self.0, state) {
            (Language::English, State::Running) => format!("running · {taken}"),
            (Language::Russian, State::Running) => format!("выполняется · {taken}"),
            (Language::English, State::Finished { code }) => format!("exit {code} · {taken}"),
            (Language::Russian, State::Finished { code }) => format!("код {code} · {taken}"),
            (Language::English, State::Stopped) => format!("stopped · {taken}"),
            (Language::Russian, State::Stopped) => format!("остановлено · {taken}"),
            (_, State::Failed { .. }) => self.text(Text::RunFailed).to_owned(),
        }
    }

    /// A command that never started, in the failure's own words: "no such
    /// file" says which half of `just check` was wrong, and "could not
    /// start" says nothing at all.
    pub fn notice_run_failed(self, line: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!("{line} did not start: {reason}"),
            Language::Russian => format!("{line} не запустилась: {reason}"),
        }
    }

    /// Exact selected replacement, including its source and both named builds.
    pub fn update_replacement(self, installed: &str, candidate: &str, source: &str) -> String {
        match self.0 {
            Language::English => format!(
                "Installed: {installed}\nSelected: {candidate}\nSource: {source}\n\nThis may replace the app with an older, rebuilt or incomparable version. Replace it and restart?\n\nGuild checks unsaved files before closing. Restart stops running sessions; conversations are kept, but work in an unfinished turn may be lost. Wait for active work to finish before restarting if you need its result."
            ),
            Language::Russian => format!(
                "Установлено: {installed}\nВыбрано: {candidate}\nИсточник: {source}\n\nВыбранная сборка может быть старее, пересобрана или не иметь сопоставимой версии. Заменить приложение и перезапустить?\n\nПеред закрытием Guild проверит несохранённые файлы. Перезапуск остановит работающие сессии; разговоры сохранятся, но работа незавершённого хода может быть потеряна. Если нужен результат текущей работы, дождитесь её завершения перед перезапуском."
            ),
        }
    }

    /// The Update button's tooltip: the build it would apply — M5.15.
    pub fn update_to(self, label: &str) -> String {
        match self.0 {
            Language::English => format!("Update to {label}"),
            Language::Russian => format!("Обновить до {label}"),
        }
    }

    /// *What's new*'s line under its head: the version run before, and
    /// how many releases came after it — M5.16.
    pub fn whats_new_since(self, since: &str, releases: usize) -> String {
        match self.0 {
            Language::English => {
                let word = if releases == 1 { "release" } else { "releases" };
                format!("You were on {since} · {releases} {word} since")
            }
            Language::Russian => format!(
                "Было: {since} · {} с тех пор",
                russian_count(releases as i64, "релиз", "релиза", "релизов")
            ),
        }
    }

    /// A plan's window by name — M12.10.
    pub fn usage_window(self, name: &crate::usage::WindowName) -> String {
        use crate::usage::WindowName;
        match name {
            WindowName::Session => self.text(Text::UsageSession).to_owned(),
            WindowName::Week => self.text(Text::UsageWeek).to_owned(),
            WindowName::WeekOf(model) => format!("{} · {model}", self.text(Text::UsageWeek)),
            WindowName::Span(minutes) => {
                self.usage_duration(std::time::Duration::from_secs(u64::from(*minutes) * 60))
            }
        }
    }

    /// A window by its length, as *All providers* lists it: *5h*, *7d*,
    /// *7d · Fable*, a span as itself — M12.12.
    pub fn usage_window_short(self, name: &crate::usage::WindowName) -> String {
        use crate::usage::WindowName;
        let span = |minutes: u64| self.usage_duration(std::time::Duration::from_secs(minutes * 60));
        match name {
            WindowName::Session => span(300),
            WindowName::Week => span(10080),
            WindowName::WeekOf(model) => format!("{} · {model}", span(10080)),
            WindowName::Span(minutes) => span(u64::from(*minutes)),
        }
    }

    /// *49% left*.
    pub fn usage_left(self, left: u8) -> String {
        match self.0 {
            Language::English => format!("{left}% left"),
            Language::Russian => format!("осталось {left}%"),
        }
    }

    /// A duration as the usage card says it: *4d 11h*, *4д 11ч*.
    pub fn usage_duration(self, span: std::time::Duration) -> String {
        let said = crate::usage::resets_in(span);
        match self.0 {
            Language::English => said,
            Language::Russian => said.replace('d', "д").replace('h', "ч").replace('m', "м"),
        }
    }

    /// *resets in 4d 11h*.
    pub fn usage_resets_in(self, span: std::time::Duration) -> String {
        let span = self.usage_duration(span);
        match self.0 {
            Language::English => format!("resets in {span}"),
            Language::Russian => format!("сброс через {span}"),
        }
    }

    /// *updated 3m ago*, *updated just now* — the Refresh row's detail.
    pub fn usage_updated(self, ago: std::time::Duration) -> String {
        match (self.0, ago.as_secs() < 60) {
            (Language::English, true) => "updated just now".to_owned(),
            (Language::Russian, true) => "обновлено только что".to_owned(),
            (Language::English, false) => format!("updated {} ago", self.usage_duration(ago)),
            (Language::Russian, false) => format!("обновлено {} назад", self.usage_duration(ago)),
        }
    }

    /// *from a session · 2m ago* — figures a session in a tab carried,
    /// not asked of the network.
    pub fn usage_from_session(self, ago: std::time::Duration) -> String {
        match (self.0, ago.as_secs() < 60) {
            (Language::English, true) => "from a session · just now".to_owned(),
            (Language::Russian, true) => "из сессии · только что".to_owned(),
            (Language::English, false) => {
                format!("from a session · {} ago", self.usage_duration(ago))
            }
            (Language::Russian, false) => format!("из сессии · {} назад", self.usage_duration(ago)),
        }
    }

    /// *outdated · from 40m ago* — figures half an hour old, or with a
    /// window whose reset has passed.
    pub fn usage_outdated(self, ago: std::time::Duration) -> String {
        match self.0 {
            Language::English => format!("outdated · from {} ago", self.usage_duration(ago)),
            Language::Russian => format!("устарело · данные {} назад", self.usage_duration(ago)),
        }
    }

    /// *failed · from 12m ago* — figures older than the last try.
    pub fn usage_failed(self, from: Option<std::time::Duration>) -> String {
        match (self.0, from) {
            (Language::English, Some(from)) => {
                format!("failed · from {} ago", self.usage_duration(from))
            }
            (Language::Russian, Some(from)) => {
                format!("не удалось · данные {} назад", self.usage_duration(from))
            }
            (Language::English, None) => "failed".to_owned(),
            (Language::Russian, None) => "не удалось".to_owned(),
        }
    }

    /// *rate limited · from 12m ago* — the server answered 429, and the
    /// figures kept are this old.
    pub fn usage_rate_limited(self, from: Option<std::time::Duration>) -> String {
        let said = self.text(Text::UsageRateLimited);
        match (self.0, from) {
            (Language::English, Some(from)) => {
                format!("{said} · from {} ago", self.usage_duration(from))
            }
            (Language::Russian, Some(from)) => {
                format!("{said} · данные {} назад", self.usage_duration(from))
            }
            (_, None) => said.to_owned(),
        }
    }

    /// The foot of the card over Update, for the entries it leaves out.
    pub fn and_more(self, n: usize) -> String {
        match self.0 {
            Language::English => format!("and {n} more"),
            Language::Russian => format!("и ещё {n}"),
        }
    }

    /// A changelog group's heading, in the window's language: the file
    /// spells keepachangelog's English names, the card says them.
    pub fn change_group(self, group: guild_session::changelog::Group) -> &'static str {
        use guild_session::changelog::Group;
        match (self.0, group) {
            (Language::English, group) => group.name(),
            (Language::Russian, Group::Added) => "Добавлено",
            (Language::Russian, Group::Changed) => "Изменено",
            (Language::Russian, Group::Deprecated) => "Устарело",
            (Language::Russian, Group::Removed) => "Удалено",
            (Language::Russian, Group::Fixed) => "Исправлено",
            (Language::Russian, Group::Security) => "Безопасность",
        }
    }

    /// Why the staged build was not made ready — M5.15. The window stays.
    pub fn update_failed(self, failure: &crate::update::swap::Failure) -> String {
        use crate::update::swap::Failure;
        let reason = match (self.0, failure) {
            (_, Failure::NoHelper) => return self.text(Text::NoticeUpdateNoHelper).to_owned(),
            (Language::English, Failure::Copy(words)) => format!("the copy failed — {words}"),
            (Language::Russian, Failure::Copy(words)) => {
                format!("копирование не удалось — {words}")
            }
            (Language::English, Failure::Seal(words)) => {
                format!("the copy's signature does not hold — {words}")
            }
            (Language::Russian, Failure::Seal(words)) => {
                format!("подпись копии не сходится — {words}")
            }
            (Language::English, Failure::NotABuild) => {
                "what is staged is not a Guild bundle".to_owned()
            }
            (Language::Russian, Failure::NotABuild) => "подготовленное — не бандл Guild".to_owned(),
            (Language::English, Failure::NotNewer) => {
                "what is staged is no longer newer than this Guild".to_owned()
            }
            (Language::Russian, Failure::NotNewer) => {
                "подготовленная сборка уже не новее этого Guild".to_owned()
            }
        };
        self.update_not_applied(&reason)
    }

    /// The helper that swaps the bundle did not start — M5.15.
    pub fn update_helper_failed(self, reason: &str) -> String {
        let reason = match self.0 {
            Language::English => format!("the helper did not start — {reason}"),
            Language::Russian => format!("помощник не запустился — {reason}"),
        };
        self.update_not_applied(&reason)
    }

    fn update_not_applied(self, reason: &str) -> String {
        match self.0 {
            Language::English => format!("The update was not applied: {reason}"),
            Language::Russian => format!("Обновление не применено: {reason}"),
        }
    }

    /// A write git refused, in git's words: `fatal: pathspec … did not
    /// match` says more than any line of ours could.
    pub fn notice_git_refused(self, words: &str) -> String {
        match self.0 {
            Language::English => format!("git refused: {words}"),
            Language::Russian => format!("git отказал: {words}"),
        }
    }

    /// A file that was not saved, and why — the OS's words, because
    /// "could not save" alone sends a person to the log.
    pub fn save_failed(self, name: &str, reason: &str) -> String {
        match self.0 {
            Language::English => format!("{name} was not saved: {reason}"),
            Language::Russian => format!("{name} не сохранён: {reason}"),
        }
    }

    /// The Log's combined inspection, also used in its diff tab title.
    pub fn log_commit_count(self, count: usize) -> String {
        match self.0 {
            Language::English => format!("{count} commits"),
            Language::Russian => russian_count(count as i64, "коммит", "коммита", "коммитов"),
        }
    }

    /// A file's diff tab: its name, with the base after it when the base
    /// is not the last commit against the working tree — *lib.rs ·
    /// staged*, *lib.rs · a1b2c3d*. The hash is cut to seven, which is
    /// what git itself abbreviates to in a repository of this size.
    pub fn diff_tab_title(self, name: &str, base: &Base) -> String {
        if let Base::Commit(key) = base
            && key.contains(',')
        {
            return format!("{name} · {}", self.log_commit_count(key.split(',').count()));
        }
        let after = match base {
            Base::Head => return name.to_owned(),
            Base::Index => self.text(Text::BaseIndex),
            Base::Unstaged => self.text(Text::BaseUnstaged),
            Base::Commit(sha) => {
                let end = sha.char_indices().nth(7).map_or(sha.len(), |(i, _)| i);
                &sha[..end]
            }
            Base::Proposal => self.text(Text::BaseProposal),
        };
        format!("{name} · {after}")
    }

    /// The working tree's diff tab: *Diff · 3 files*, the mockup's name.
    pub fn diff_title(self, files: usize) -> String {
        // Counting to zero in a tab's name says less than the name alone.
        if files == 0 {
            return self.text(Text::DiffChanges).to_owned();
        }
        format!(
            "{} · {}",
            self.text(Text::DiffChanges),
            self.diff_files(files)
        )
    }

    /// *3 files*, counted in the language's own way.
    pub fn diff_files(self, files: usize) -> String {
        match self.0 {
            Language::English => {
                if files == 1 {
                    "1 file".to_owned()
                } else {
                    format!("{files} files")
                }
            }
            Language::Russian => {
                let last_two = files % 100;
                let last = files % 10;
                let word = if (11..=14).contains(&last_two) {
                    "файлов"
                } else if last == 1 {
                    "файл"
                } else if (2..=4).contains(&last) {
                    "файла"
                } else {
                    "файлов"
                };
                format!("{files} {word}")
            }
        }
    }

    /// What one session spent, for a row in the list.
    ///
    /// Finer than the counter in the title bar: that one adds every
    /// session together, while a single session is often worth fractions
    /// of a cent, and `$0.00` there would read as nothing.
    ///
    /// Empty when there is nothing to say — a shell has no spending and
    /// cannot have any, and a session nobody has spoken to yet has none.
    pub fn session_spent(self, usage: Usage) -> String {
        match (usage.cost_usd, usage.total_tokens()) {
            (Some(cost), _) => format!("${cost:.4}"),
            (None, 0) => String::new(),
            (None, tokens) => match self.0 {
                Language::English => format!("{} tokens", short_tokens(tokens)),
                Language::Russian => format!("{} токенов", short_tokens(tokens)),
            },
        }
    }

    /// How long ago something happened, in as few characters as a column
    /// can spare — M12 task 16.
    ///
    /// **Rounded down and never exact.** A worktree's row answers "when
    /// was I last here", and to that question *2h* and *2h 14m* are the
    /// same answer, while the second costs the width the branch is drawn
    /// in. The units stop at weeks: past that the number is large and the
    /// row is old, and both are the same fact.
    pub fn ago(self, seconds: i64) -> String {
        const MINUTE: i64 = 60;
        const HOUR: i64 = 60 * MINUTE;
        const DAY: i64 = 24 * HOUR;
        const WEEK: i64 = 7 * DAY;

        match (self.0, seconds) {
            (Language::English, s) if s < MINUTE => "just now".to_owned(),
            (Language::English, s) if s < HOUR => format!("{}m ago", s / MINUTE),
            (Language::English, s) if s < DAY => format!("{}h ago", s / HOUR),
            (Language::English, s) if s < WEEK => format!("{}d ago", s / DAY),
            (Language::English, s) => format!("{}w ago", s / WEEK),
            (Language::Russian, s) if s < MINUTE => "только что".to_owned(),
            (Language::Russian, s) if s < HOUR => format!("{} мин назад", s / MINUTE),
            (Language::Russian, s) if s < DAY => format!("{} ч назад", s / HOUR),
            (Language::Russian, s) if s < WEEK => format!("{} дн назад", s / DAY),
            (Language::Russian, s) => format!("{} нед назад", s / WEEK),
        }
    }

    /// Why a worktree cannot be removed: somebody holds a lock on it.
    ///
    /// **git's reason is quoted rather than replaced**, because whoever
    /// took the lock wrote who they are into it — a tool puts its name and
    /// its pid there, and that is the whole of what a person needs to
    /// decide between waiting and unlocking. A reason is not always given,
    /// and then the sentence stands on its own.
    pub fn worktree_locked(self, reason: &str) -> String {
        let reason = reason.trim();
        match (self.0, reason.is_empty()) {
            (Language::English, true) => "That worktree is locked".to_owned(),
            (Language::English, false) => format!("That worktree is locked: {reason}"),
            (Language::Russian, true) => "Этот worktree заблокирован".to_owned(),
            (Language::Russian, false) => format!("Этот worktree заблокирован: {reason}"),
        }
    }

    /// Resuming a conversation.
    pub fn resuming(self, id: &str) -> String {
        match self.0 {
            Language::English => format!("Resuming conversation {id}."),
            Language::Russian => format!("Продолжение разговора {id}."),
        }
    }

    /// The line about the model and the number of tools.
    pub fn model_line(self, model: &str, tools: usize) -> String {
        match self.0 {
            Language::English => format!("model {model} · {tools} tools"),
            Language::Russian => format!("модель {model} · инструментов {tools}"),
        }
    }

    /// The turn's result.
    pub fn turn_result(self, seconds: f64, turns: u32, cost: f64) -> String {
        match self.0 {
            Language::English => {
                format!("done in {seconds:.1} s · {turns} turns · ${cost:.4}")
            }
            Language::Russian => {
                format!("готово за {seconds:.1} с · ходов {turns} · ${cost:.4}")
            }
        }
    }

    /// What Guild is, for the About item.
    ///
    /// Assembled rather than a constant: the version comes from the build
    /// and the log path from the machine, and a person asked to send their
    /// log should be able to read where it is without hunting.
    pub fn about(self, version: &str, log: &str) -> String {
        match self.0 {
            Language::English => format!(
                "An ADE where AI agents do the work and tasks travel a \
                 pipeline with review as a first-class stage.\n\n\
                 Version {version}\nLog: {log}"
            ),
            Language::Russian => format!(
                "ADE, где работу выполняют AI-агенты, а задачи движутся по \
                 конвейеру, в котором ревью — полноправный этап.\n\n\
                 Версия {version}\nЖурнал: {log}"
            ),
        }
    }

    /// The turn failed.
    pub fn turn_failed(self, reason: &str) -> String {
        match self.0 {
            Language::English => format!("Turn failed: {reason}"),
            Language::Russian => format!("Ход оборвался: {reason}"),
        }
    }
}

/// A token count short enough to read at a glance.
///
/// A raw count is six digits by lunchtime, and the digits past the second
/// carry nothing a person acts on.
fn short_tokens(tokens: u64) -> String {
    match tokens {
        0..=999 => tokens.to_string(),
        1_000..=999_999 => format!("{:.1}k", tokens as f64 / 1_000.0),
        _ => format!("{:.1}M", tokens as f64 / 1_000_000.0),
    }
}

const MONTHS_EN: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const MONTHS_RU: [&str; 12] = [
    "янв", "фев", "мар", "апр", "мая", "июн", "июл", "авг", "сен", "окт", "ноя", "дек",
];

/// A day, a month and a year from seconds since the epoch, in UTC: the
/// day a commit was made is the same day in every zone that matters for
/// a log read a week later, and a zone would need the platform.
/// `04.09.2026` — the same digits in both languages.
///
/// A log column is read by the eye rather than the ear, and a month's
/// name in it is three characters that never line up. `at` is already
/// local here: the caller adds the offset.
fn numeric_date(local: i64) -> String {
    let (year, month, day) = civil(local.div_euclid(86_400));
    format!("{day:02}.{month:02}.{year}")
}

fn date(at: i64, months: &[&str; 12]) -> String {
    let (year, month, day) = civil(at.div_euclid(86_400));
    format!("{day} {} {year}", months[(month - 1) as usize])
}

/// Howard Hinnant's civil-from-days, the algorithm every calendar library
/// carries. One copy, because two would drift.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// A Russian count: one form for 1, another for 2–4, a third for the
/// rest — with the teens taking the third, as they do.
fn russian_count(n: i64, one: &str, few: &str, many: &str) -> String {
    let form = match (n % 10, n % 100) {
        (1, teens) if !(11..=14).contains(&teens) => one,
        (2..=4, teens) if !(11..=14).contains(&teens) => few,
        _ => many,
    };
    format!("{n} {form}")
}

impl Ui {
    /// Human-readable state and waiting reason from authoritative telemetry.
    pub fn activity_state(self, status: guild_session::status::Status) -> &'static str {
        use guild_session::status::{Status, Why};
        match (self.0, status) {
            (Language::English, Status::Waiting(Why::Permission)) => "Waiting for permission",
            (Language::English, Status::Waiting(Why::Question)) => "Waiting for an answer",
            (Language::English, Status::Waiting(Why::Auth)) => "Waiting for sign-in",
            (Language::English, Status::Waiting(Why::Unstated)) => "Waiting",
            (Language::English, Status::Failed) => "Observed execution failure",
            (Language::English, Status::Working) => "Working",
            (Language::English, Status::Done) => "Finished unread answer",
            (Language::English, Status::Idle) => "Idle / no current telemetry",
            (Language::Russian, Status::Waiting(Why::Permission)) => "Ждёт разрешения",
            (Language::Russian, Status::Waiting(Why::Question)) => "Ждёт ответа",
            (Language::Russian, Status::Waiting(Why::Auth)) => "Ждёт входа",
            (Language::Russian, Status::Waiting(Why::Unstated)) => "Ждёт",
            (Language::Russian, Status::Failed) => "Ошибка выполнения",
            (Language::Russian, Status::Working) => "Работает",
            (Language::Russian, Status::Done) => "Завершённый непрочитанный ответ",
            (Language::Russian, Status::Idle) => "Покой / нет текущих сигналов",
        }
    }

    /// Separate exact counts, never a claim of verified task success.
    pub fn activity_counts(self, name: &str, a: guild_session::status::Activity) -> String {
        match self.0 {
            Language::English => format!(
                "{name}: waiting {}, failed {}, working {}, unread {}, idle {}",
                a.waiting, a.failed, a.working, a.unread, a.idle
            ),
            Language::Russian => format!(
                "{name}: ждут {}, ошибки {}, работают {}, непрочитано {}, покой {}",
                a.waiting, a.failed, a.working, a.unread, a.idle
            ),
        }
    }
}

impl Ui {
    /// One agent's counts on the status bar's hover: what is non-zero, in
    /// the counters' order.
    pub(crate) fn session_counts(self, counts: crate::session_list::Counts) -> String {
        let parts: [(usize, &str, &str); 5] = [
            (counts.failed, "failed", "с ошибкой"),
            (counts.working, "working", "в работе"),
            (counts.waiting, "waiting", "ждут"),
            (counts.done, "done", "готово"),
            (counts.idle, "idle", "простаивают"),
        ];
        parts
            .into_iter()
            .filter(|(n, _, _)| *n > 0)
            .map(|(n, english, russian)| match self.0 {
                Language::English => format!("{n} {english}"),
                Language::Russian => format!("{russian}: {n}"),
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl Ui {
    pub(crate) fn process_state(self, state: &str) -> &'static str {
        self.text(match state {
            "blocked" => Text::ProcessBlocked,
            "ready" => Text::ProcessReady,
            "active" => Text::ProcessActive,
            "waiting" => Text::ProcessWaiting,
            "finished" => Text::ProcessFinished,
            "skipped" => Text::ProcessSkipped,
            "cancelled" => Text::ProcessCancelled,
            _ => Text::ProcessUnknown,
        })
    }
    pub(crate) fn process_review_usage(
        self,
        used: u32,
        reserved: u32,
        remaining: u32,
        limit: u32,
        corrections: u32,
        correction_limit: u32,
    ) -> String {
        match self.0 {
            Language::English => format!(
                "{used}/{limit} used · {reserved} reserved · {remaining} remaining · corrections {corrections}/{correction_limit}"
            ),
            Language::Russian => format!(
                "{used}/{limit} использовано · {reserved} зарезервировано · {remaining} осталось · исправления {corrections}/{correction_limit}"
            ),
        }
    }
}

impl Ui {
    pub fn harness_failure(self, harness: guild_session::HarnessKind, code: u32) -> String {
        let name = match harness {
            guild_session::HarnessKind::Claude => "Claude Code",
            guild_session::HarnessKind::Codex => "Codex",
            guild_session::HarnessKind::OpenCode => "OpenCode",
        };
        match self.0 {
            Language::Russian => format!("{name} не смог продолжить работу (код {code})."),
            Language::English => format!("{name} could not continue (exit {code})."),
        }
    }
}

impl Ui {
    pub fn commit_selection_count(self, included: usize, selected: usize) -> String {
        match self.0 {
            Language::English => format!("{included} included · {selected} selected"),
            Language::Russian => format!("Включено: {included} · Выбрано: {selected}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test stayed behind when the rest moved out to `tests/labels.rs`,
    // and it stayed for the reason that decides every such question: it
    // reads `short_tokens`, which is private and should be. What the crate
    // promises is `spent`, the label that carries a token figure; how it
    // shortens a number is its own business.
    /// A raw token count is six digits by lunchtime, and the digits past
    /// the second carry nothing a person acts on.
    #[test]
    fn tokens_are_written_short() {
        assert_eq!(short_tokens(0), "0");
        assert_eq!(short_tokens(999), "999");
        assert_eq!(short_tokens(4_231), "4.2k");
        assert_eq!(short_tokens(1_200_000), "1.2M");
    }

    /// **Removing a project asks one question and warns only of what is
    /// lost** — 2026-09-28. The question names the project once and the
    /// workspace it leaves; the only line under it is for unsaved changes,
    /// because its folders stay and its conversations can be resumed.
    #[test]
    fn removing_a_project_warns_only_of_unsaved_changes() {
        for language in Language::ALL {
            let ui = Ui(*language);
            let title = ui.remove_project_title("Guild");
            assert_eq!(title.matches("Guild").count(), 1, "{title}");
            assert!(title.contains("workspace"), "{title}");
            assert_eq!(ui.remove_project_detail(false), None);
            assert!(ui.remove_project_detail(true).is_some());
        }
    }
}
