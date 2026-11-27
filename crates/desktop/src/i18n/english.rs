//! The interface in English.
//!
//! A file per language, so adding one is a file rather than a diff through
//! the middle of another. The `match` is exhaustive, which is the whole
//! reason the strings are an enum: a language that forgets a label does not
//! compile, where a file of translations would simply have a hole in it
//! that users find first.

use super::Text;

pub(super) fn english(text: Text) -> &'static str {
    match text {
        Text::CommitSelectionChanged => {
            "Selection changed. Reopen the menu to act on the current group."
        }
        Text::CommitRollbackMixed => {
            "Select only tracked files without conflicts. Nothing was changed."
        }
        Text::CommitAddMixed => {
            "Select only unversioned files to add to Git. The selection includes tracked changes."
        }
        Text::CommitTrashMissing => {
            "The selection includes files already deleted. Select existing files to move to Trash."
        }
        Text::CommitDiffDirectories => {
            "The selection includes directories. Expand them to select files."
        }
        Text::CommitDiffUnavailable => "Show diffs unavailable",
        Text::CommitInclusionUnavailable => "Commit inclusion unavailable",
        Text::CommitRollbackUnavailable => "Rollback unavailable",
        Text::CommitAddUnavailable => "Add to Git unavailable",
        Text::CommitTrashUnavailable => "Trash unavailable",
        Text::CommitExpandDirectory => "Expand directory",
        Text::CommitCollapseDirectory => "Collapse directory",
        Text::CommitSpaceHint => "In the list, Space toggles commit inclusion.",
        Text::CommitDirectoryLoading => "Loading…",
        Text::CommitDirectoryEmpty => "Empty; Git does not track empty directories",
        Text::CommitDirectoryNoEligible => {
            "No eligible files; Git does not track empty directories"
        }
        Text::CommitPathNul => "Paths containing NUL are not valid",
        Text::CommitUnsupportedPath => "Paths with non-UTF-8 names are not supported",
        Text::CommitTrashRoot => "The open project folder cannot be moved to Trash",
        Text::CommitDirectoryInvalidName => "Invalid directory name",
        Text::CommitRollbackGroupDetail => {
            "Working-tree changes will be lost. Newly added files will be unstaged and kept on disk."
        }

        Text::ProcessOpenJob => "Open review output",
        Text::ProcessJobUnavailable => {
            "No retained live job surface here. The archive reference remains available; this action cannot relaunch a review."
        }
        Text::ProcessContext => "Process context",
        Text::ProcessSelectedStep => "Selected step",
        Text::ProcessExecuting => "Current execution",
        Text::ProcessWorker => "Worker",
        Text::ProcessEvents => "Recent events",
        Text::ProcessNoEvents => "No durable events recorded",
        Text::ProcessContract => "Contract",
        Text::ProcessEvidence => "Contract and evidence",
        Text::ProcessNoEvidence => "No recorded files for this step",
        Text::ProcessRecordedSource => "Recorded source",
        Text::ProcessResources => "Resources",
        Text::ProcessNoResources => "No skill resources configured for this step",
        Text::ProcessSessionUnavailable => {
            "No eligible existing session view; no session will be started here."
        }
        Text::ProcessFileUnavailable => {
            "File is missing or outside the available original project. Inspect its recorded path in the session."
        }
        Text::ProcessControlsUnavailable => {
            "This view inspects recorded work. Dispatch, retries, corrections and automatic continuation are not available here; use the original session for supported commands."
        }
        Text::ProcessQuestionUnavailable => {
            "The session is waiting for an answer. Question text is not available in Process; open the session to inspect it."
        }
        Text::ProcessWaitingUnavailable => {
            "The session is waiting for the person. What it needs is not available in Process; open the session to see it."
        }
        Text::ProcessPermissionUnavailable => {
            "The session is waiting for tool permission. Request details are not available here; inspect the session. Process cannot approve it."
        }
        Text::ProcessSelectedFallback => {
            "The previously selected step is unavailable; showing the current available step:"
        }
        Text::ProcessStaleSnapshot => {
            "Showing the last recorded snapshot; the latest inspection failed."
        }
        Text::ProcessCloseKeepsRunning => "Closing this view does not stop the work.",
        Text::ProcessSourceDifferent => "Retained evidence · different recorded source",
        Text::ProcessSourceCurrent => "Recorded source matches this snapshot",
        Text::ProcessNoAssignment => "No bound worker session recorded for this step.",
        Text::ProcessRole => "Role",
        Text::ProcessOutcomes => "Required outcomes",
        Text::ProcessDependencies => "Depends on",
        Text::ProcessDefinition => "Configuration",
        Text::ProcessOpenFile => "Open file",
        Text::ProcessUnrecorded => "Not recorded",
        Text::ProcessEventStart => "Run started",
        Text::ProcessEventCheckpoint => "Checked outcome recorded",
        Text::ProcessEventAssignment => "Assignment observed",
        Text::ProcessEventDelivery => "Task delivery observed",
        Text::ProcessEventRecovery => "Origin navigation / recovery observed",
        Text::ProcessEventReport => "Worker report retained",
        Text::ProcessEventReview => "Review observation retained",
        Text::ProcessEventCorrection => "Same-session correction",
        Text::ProcessCorrectionSubmitted => "Input submitted; actual turn start unconfirmed",
        Text::ProcessCorrectionObserved => {
            "Turn start observed; fresh changed-source report required"
        }
        Text::ProcessCorrectionUnknown => {
            "Delivery uncertain; inspect retained receipt, do not replay"
        }
        Text::ProcessCorrectionParent => "Parent contract checkpoint required; no input sent",
        Text::ProcessCorrectionRefused => "Refused before input; explicit retry available",
        Text::ProcessEventCancel => "Cancellation requested",
        Text::ProcessEventReservation => "Effect reserved; result unconfirmed",
        Text::ProcessEventUnknown => "Retained event",
        Text::ProcessLightweight => "Lightweight note",
        Text::ProcessNoReview => "No review receipt for this step",
        Text::ProcessPreviousResumeWait => {
            "An earlier recovery attempt still needs reconciliation; revealing the live session does not clear that wait."
        }

        Text::ProcessRecoveryAreaFallback => {
            "Original area unavailable; restored in the only existing area of the original project and workspace."
        }
        Text::ProcessRecoveryLive => "Original session reused; no new process started",
        Text::ProcessRecoveryLaunched => {
            "Same conversation launch requested; readiness unconfirmed"
        }
        Text::ProcessRecoveryGap => "Origin recovery unavailable",
        Text::ProcessParentAttention => "Parent attention required",
        Text::ProcessReconcileEvent => "Parent reconciliation recorded",
        Text::ProcessReconcileChecklist => "Recovery checklist",
        Text::ProcessReconcileClear => {
            "Facts qualified. The original parent can acknowledge this exact assessment using guild workflow reconcile; no harness input is sent."
        }
        Text::ProcessHandoffReserved => "Eligible prompt qualified; handoff reserved before input.",
        Text::ProcessHandoffReady => {
            "Original conversation resumed; prompt eligibility was independently qualified."
        }
        Text::ProcessHandoffContinued => {
            "A subsequent source-qualified parent workflow action was recorded."
        }
        Text::ProcessHandoffStartup => "Delivery path: native resume startup payload.",
        Text::ProcessHandoffStartupSubmitted => {
            "Recovery summary supplied to the original conversation at native launch; awaiting an authoritative turn-start observation. Trust, authentication and permissions remain separate human requests."
        }
        Text::ProcessHandoffTitle => "Recovery handoff",
        Text::ProcessHandoffWaiting => {
            "Waiting for an independently observed completed turn and an eligible empty prompt in the original session."
        }
        Text::ProcessHandoffBusy => {
            "Original session is working or waiting for a question/permission. No input sent."
        }
        Text::ProcessHandoffDraft => {
            "Automatic continuation is paused because unsent input was observed. Guild did not submit or change it. Finish, send or remove the draft in the original session; other readiness checks still apply."
        }
        Text::ProcessHandoffInputUsed => {
            "Input reached this resumed terminal. Automatic handoff is unavailable; open the original session."
        }
        Text::ProcessHandoffInputUnknown => {
            "This OpenCode version does not provide qualified empty-input readiness. Automatic handoff was not sent; open the original session. Draft presence is unknown."
        }
        Text::ProcessHandoffHolder => {
            "Original runtime generation is unavailable or changed; no handoff sent to a replacement."
        }
        Text::ProcessHandoffCovered => {
            "An overlay or another sender owns the prompt; handoff is waiting."
        }
        Text::ProcessHandoffTransport => {
            "The harness has not qualified prompt submission. Inspect the original session; no startup input is guessed."
        }
        Text::ProcessHandoffSnapshot => {
            "Current source, contract and runtime facts must qualify before handoff."
        }
        Text::ProcessHandoffSubmitted => {
            "Handoff submitted through the owned terminal; awaiting an authoritative turn-start observation. This is not reconciliation or continued work."
        }
        Text::ProcessHandoffAcknowledged => {
            "Handoff turn start and explicit parent reconciliation recorded. Continue through the next eligible configured checkpoint."
        }
        Text::ProcessHandoffObserved => {
            "Handoff turn start observed. The original parent must inspect and explicitly acknowledge a fresh reconciliation assessment."
        }
        Text::ProcessHandoffUnknown => {
            "Handoff delivery is uncertain. Inspect the original session and retained receipt; automatic replay is blocked."
        }
        Text::ProcessReconcileRecorded => {
            "Parent reconciliation recorded. This acknowledgement sends no prompt; handoff delivery is recorded separately."
        }
        Text::ProcessReconcileStale => {
            "The previous acknowledgement is stale; inspect and acknowledge current facts again."
        }
        Text::ProcessReconcileSource => {
            "Source changed: submit source-qualified evidence from the original parent before acknowledgement."
        }
        Text::ProcessReconcileContract => {
            "Pinned contract is missing or changed; inspect its retained evidence."
        }
        Text::ProcessReconcileOrigin => {
            "Exact original conversation/runtime ownership is unqualified; inspect the original session."
        }
        Text::ProcessReconcileHolder => {
            "Assigned runtime identity/holder is unknown; inspect that session without launching a replacement."
        }
        Text::ProcessReconcilePermission => {
            "Tool permission is unanswered. Inspect the indicated session; Process cannot answer it."
        }
        Text::ProcessReconcileQuestion => {
            "A human question is pending. Inspect the indicated session; request details are unavailable here."
        }
        Text::ProcessReconcileEffect => {
            "An effect reservation has no qualified outcome. Reconcile its actual owner; do not replay it."
        }
        Text::ProcessReconcileDelivery => {
            "Child delivery is unqualified; inspect the retained assignment and session."
        }
        Text::ProcessReconcileReport => {
            "The stopped child has no source-qualified report; obtain its retained result before proceeding."
        }
        Text::ProcessReconcileChildSource => {
            "Child source/report is stale or unavailable; recheck that checkout and its evidence."
        }
        Text::ProcessReconcileReview => {
            "Review execution or actual usage remains pending/unknown; reconcile its existing job/archive without another launch."
        }
        Text::ProcessReconcileResource => {
            "A recorded resource/private environment owner is retained, changed or unknown; reconcile that exact owner."
        }
        Text::ProcessReconcileUnknown => {
            "Current assessment is unavailable; inspect the original session and retained history."
        }
        Text::ProcessReconcileStatus => {
            "Native status has not been observed yet; wait for a qualified observation from this exact session."
        }
        Text::ProcessRecover => "Resume parent session and open process",
        Text::ProcessRecoveryPending => "Recovery reserved; launch/readiness unconfirmed",
        Text::ProcessRecoveryWait => {
            "Native launch is separate from readiness and orchestration. Review retained children, source, review usage, pending questions and resources before continuing. No task text or approval was submitted."
        }
        Text::ProcessOpen => "Open process",
        Text::ProcessMoveUnavailable => {
            "Process association is unavailable; inspect its retained run before moving."
        }
        Text::ProcessMoveSelectPane => "Select the origin or Process pane of the group to move.",
        Text::ProcessMoveSeat => {
            "Related views belong to another project/workspace; reveal the original group before moving."
        }
        Text::HarnessFailureNetwork => {
            "Check your internet connection, VPN or proxy, then try again."
        }
        Text::HarnessFailureDetails => "See the terminal output below for details, then try again.",
        Text::HarnessRetry => "Retry",
        Text::HarnessRetryUnknown => {
            "The previous conversation cannot be identified. Start a fresh conversation? The previous prompt will not be sent again."
        }
        Text::HarnessRetryManaged => {
            "Cannot safely identify the task conversation. The assignment and terminal output have been retained."
        }
        Text::HarnessRetryOccupied => {
            "This Claude ID is occupied, but has no resumable conversation. The session and output have been retained."
        }
        Text::HarnessRetryDirectory => {
            "The original working directory is unavailable. Restore it before retrying."
        }
        Text::HarnessRetryIdentity => {
            "Cannot safely determine the conversation identity. The session and terminal output have been retained."
        }
        Text::HarnessRetryHeld => {
            "Cannot safely retry: the conversation is held by another process or its status is unknown."
        }
        Text::Process => "Process",
        Text::ProcessRevision => "revision",
        Text::ProcessUnavailable => "Run inspection unavailable",
        Text::ProcessLoading => "Loading the retained run…",
        Text::ProcessReviewUsage => "Review usage",
        Text::ProcessSteps => "Configured steps",
        Text::ProcessOrigin => "Origin",
        Text::ProcessOpenSession => "Open session",
        Text::ProcessTarget => "Project",
        Text::ProcessAssignments => "Assignments",
        Text::ProcessCheckout => "Worktree",
        Text::ProcessSession => "Session",
        Text::ProcessArchive => "Saved review",
        Text::ProcessNoJob => "No job receipt recorded",
        Text::ProcessBlocked => "Blocked",
        Text::ProcessReady => "Ready",
        Text::ProcessActive => "Active",
        Text::ProcessWaiting => "Waiting",
        Text::ProcessFinished => "Finished",
        Text::ProcessSkipped => "Skipped",
        Text::ProcessCancelled => "Cancelled",
        Text::ProcessUnknown => "Unknown",
        Text::NetworkProxyRoute => "Proxy",
        Text::NetworkDirectRoute => "Direct",
        Text::NetworkSection => "Network",
        Text::NetworkIntro => {
            "Choose how Guild and the processes it starts connect to the internet."
        }
        Text::NetworkConnection => "Connection",
        Text::NetworkApply => "Apply proxy to",
        Text::NetworkHarnesses => "Agent connections",
        Text::NetworkHarnessHint => {
            "For new sessions: Follow global uses Agent sessions above. Proxy uses the saved Custom address and bypass. Direct clears proxy environment. Your explicit launch prefix can override this."
        }
        Text::NetworkFollow => "Follow global",
        Text::NetworkUseProxy => "Proxy",
        Text::NetworkDirect => "Direct",
        Text::NetworkMode => "Connection mode",
        Text::NetworkModeHint => {
            "System keeps existing configuration. Custom applies to selected destinations."
        }
        Text::NetworkAddress => "Proxy address",
        Text::NetworkAddressHint => "HTTP proxy without authentication. Available in Custom mode.",
        Text::NetworkBypass => "Bypass proxy",
        Text::NetworkBypassHint => {
            "Localhost is always bypassed. Add comma-separated hosts or internal domains."
        }
        Text::NetworkGuild => "Guild requests",
        Text::NetworkGuildHint => {
            "Usage, update checks and downloads, external Markdown images, merge request status."
        }
        Text::NetworkAgent => "Agent sessions",
        Text::NetworkAgentHint => "Pass proxy settings to new agents launched by Guild.",
        Text::NetworkTerminal => "Terminal sessions",
        Text::NetworkTerminalHint => "Pass proxy settings to ordinary Guild terminals.",
        Text::NetworkGit => "Git over HTTPS",
        Text::NetworkGitHint => "Clone, fetch and push. SSH is configured separately.",
        Text::NetworkTools => "Project tools",
        Text::NetworkToolsHint => {
            "Pass environment to tools launched by Guild. Tools must support proxy variables."
        }
        Text::NetworkSystem => "System / environment",
        Text::NetworkCustom => "Custom",
        Text::NetworkCheck => "Check connection",
        Text::NetworkChecking => "Checking…",
        Text::NetworkTiming => {
            "Agent and terminal changes apply to new sessions. Off keeps existing configuration."
        }
        Text::NetworkCheckHint => "Check access to Anthropic using this connection.",
        Text::NetworkConnected => "Connected",
        Text::NetworkReachable => "Endpoint reachable",
        Text::NetworkEndpointError => "Endpoint returned an error or redirect",
        Text::NetworkNotUsage => "Authenticated usage access was not tested.",
        Text::NetworkSystemCredentials => {
            "Check cannot send System proxy credentials. Existing System behavior is unchanged. To check here, select a Custom HTTP proxy without authentication."
        }
        Text::NetworkSystemTransport => {
            "Check does not support this System proxy transport. Existing System behavior is unchanged. To check here, select a Custom HTTP proxy without authentication."
        }
        Text::NetworkAuthError => "The proxy requires authentication, which is not supported.",
        Text::NetworkTimeout => "Connection timed out. Check the proxy address and network.",
        Text::NetworkDnsError => "Could not resolve the proxy or destination hostname.",
        Text::NetworkRefused => "Could not connect. Check that the proxy is running.",
        Text::NetworkTlsError => "TLS connection or certificate verification failed.",
        Text::NetworkUnavailable => "Connection check is unavailable: system curl could not start.",
        Text::NetworkInvalid => {
            "Enter a valid HTTP proxy host and port without credentials, and valid bypass hosts/domains."
        }
        Text::NetworkFailed => "Connection failed. Check your proxy and network settings.",

        Text::LspSettings => "Language servers",
        Text::LspDisabled => "Disabled",
        Text::LspEnabled => "Enabled",
        Text::LspInherit => "Inherit",
        Text::LspOn => "On",
        Text::LspOff => "Off",
        Text::LspNotStarted => "Not started",
        Text::LspNotInstalled => "Not installed",
        Text::LspUnprepared => "Unprepared",
        Text::LspFailed => "Failed",
        Text::LspRunning => "Running",
        Text::ScopedRepository => "Repository",
        Text::ScopedGroup => "Group",
        Text::ScopedWorkspaces => "Workspaces",
        Text::ScopedWorkspace => "Workspace",
        Text::ScopedNameHint => "Shown in the workspace title and switcher.",
        Text::ScopedIconHint => "Symbol shown beside this workspace or repository.",
        Text::ScopedBaseHint => {
            "Branch used when creating a new worktree. Empty uses the global default."
        }
        Text::ScopedDirectoryHint => {
            "Directory for future worktrees. Empty uses the automatic location."
        }
        Text::ScopedDeleteHint => "Opens a confirmation before removing the workspace.",
        Text::ScopedSaveHint => "Fields save on Enter or when focus leaves them.",
        Text::ScopedName => "Name",
        Text::ScopedIcon => "Icon",
        Text::ScopedProjects => "Connected projects",
        Text::ScopedMainPath => "Main checkout",
        Text::ScopedBase => "Base for new worktrees",
        Text::ScopedDirectory => "Worktree directory",
        Text::ScopedOpenSettings => "Open settings",
        Text::ScopedShared => "Shared across workspaces and worktrees",
        Text::ScopedDefaults => "Global defaults for inheriting repositories",
        Text::ScopedIdentityPending => "Resolving repository…",
        Text::ScopedUnavailable => "Repository identity unavailable",
        Text::ScopedSave => "Save",
        Text::ScopedRetry => "Retry",
        Text::ScopedChoose => "Choose…",
        Text::ScopedAutomatic => "Automatic",
        Text::ScopedNoMain => "No available main checkout",
        Text::ScopedFutureOnly => "Applies to future worktrees; existing checkouts stay in place",
        Text::ScopedDeleteWorkspace => "Delete workspace…",
        Text::TerminalUnknownCwd => "Program working directory unknown",
        Text::TerminalThisSession => "This session",
        Text::TerminalThisWorktree => "This worktree",
        Text::TerminalThisProject => "This project",
        Text::TerminalAllLaunches => "All launches",
        Text::TerminalOngoing => "Ongoing",
        Text::TerminalCompleted => "Completed",
        Text::TerminalScopeEmpty => "No launches in this area",
        Text::TerminalNoSession => "No focused session",
        Text::TerminalNoRoot => "No selected worktree",
        Text::TerminalUnknownSession => "Original session unavailable",
        Text::TerminalUnknownRoot => "Original worktree unavailable",
        Text::TerminalUnknownRepository => "Original repository unavailable",
        Text::TerminalObservedBranch => "Last observed branch",
        Text::TerminalUnknownBranch => "Branch observation unavailable",
        Text::TerminalProgramCwd => "Program working directory",
        Text::TerminalAllContexts => "All projects and sessions",
        Text::TerminalContextProject => "Project",
        Text::TerminalContextWorktree => "Worktree",
        Text::TerminalContextSession => "Session",
        Text::TerminalHideView => "Hide view — program keeps running",
        Text::TerminalOpeningArchive => "Opening archive",
        Text::TerminalViewingArchive => "Viewing archive",
        Text::TerminalClosingArchive => "Closing viewer",
        Text::TerminalCloseViewer => "Close viewer",

        Text::TerminalJobs => "Terminal Jobs",
        Text::TerminalNewest => "Newest first",
        Text::TerminalNoJobs => "No terminal jobs",
        Text::TerminalUnknownTime => "Launch time unknown",
        Text::TerminalOpenRun => "Open run",
        Text::TerminalShowOutput => "Show output",
        Text::TerminalDeleteOutput => "Delete saved output",
        Text::TerminalJobDetails => "Original launch details",
        Text::ReviewDetails => "Findings details",
        Text::ReviewAnalyze => "Analyzing",
        Text::ReviewConsolidate => "Consolidating",
        Text::ReviewVerify => "Verifying",
        Text::ReviewAnalyzeProgress => "Examining code and collecting candidates",
        Text::ReviewConsolidateProgress => "Reconciling overlapping candidates",
        Text::ReviewVerifyProgress => "Assessing the collected candidates",
        Text::ReviewSerious => "serious",
        Text::ReviewWarnings => "warnings",
        Text::ReviewUnknown => "unclassified",
        Text::ReviewProvisional => "Provisional",
        Text::ReviewAssessed => "Assessed",
        Text::ReviewFinal => "Final",
        Text::ReviewClean => "Completed · no findings",
        Text::ReviewIncomplete => "Incomplete review evidence",
        Text::ReviewUnavailable => "Review evidence unavailable",
        Text::ReviewStale => "Saved observation · source unavailable",
        Text::ReviewLegacy => "Legacy evidence · completeness unknown",
        Text::ReviewOpenQuestion => "Open question",
        Text::ReviewPreExisting => "Pre-existing",
        Text::ReviewImmaterial => "Immaterial",
        Text::ReviewRejected => "Rejected",
        Text::ReviewOther => "Other categories",
        Text::ReviewDropped => "Dropped during consolidation",
        Text::ReviewReported => "Reported activity (verbatim)",
        Text::ReviewNoFindingsYet => "No structured findings yet",
        Text::TerminalNoSavedOutput => "No saved output",
        Text::TerminalOutputDeleted => "Saved output deleted",
        Text::TerminalOutputEmpty => {
            "No saved terminal text. The program may have cleared its screen."
        }
        Text::TerminalOutputUnavailable => "Output unavailable in this Guild session",
        Text::TerminalStarting => "Starting",
        Text::TerminalRunning => "Running",
        Text::TerminalCancelling => "Cancelling",
        Text::TerminalFailed => "Failed to start",
        Text::TerminalInterrupted => "Interrupted",
        Text::TerminalExited => "Exited",
        Text::TerminalSignal => "Signal",
        Text::TerminalSelectPane => "Select an existing pane first",

        Text::GetFromVersionControl => "Get from Version Control…",
        Text::CloneTitle => "Get from Version Control",
        Text::UpdaterCleanupRecovery => {
            "The new build started, but update cleanup was interrupted. Close Guild before finishing cleanup and keeping the current build."
        }
        Text::UpdaterJournalWaiting => "Finishing the previous update…",
        Text::UpdaterRecoveryHelp => {
            "Quit Guild, then run this command in Terminal. Keep these app copies closed until it finishes."
        }
        Text::UpdaterCopyRecovery => "Copy recovery command",
        Text::UpdaterDestination => "Application",
        Text::UpdaterLegacyReplacement => {
            "This older updater supports only Local updates from its default directory and tagged Nightly builds. Your Remote, channel and directory preferences are retained for returning to a newer Guild; this older app will not use them."
        }
        Text::UpdaterPrevious => "Previous app recovery path",
        Text::UpdatesSection => "Updates",
        Text::UpdatesIntro => {
            "Check for new Guild releases. Downloaded updates wait until you choose to restart."
        }
        Text::UpdaterSource => "Update source",
        Text::UpdaterSourceHint => {
            "Remote uses published releases. Local uses completed builds on this Mac."
        }
        Text::UpdaterChannel => "Release channel",
        Text::UpdaterChannelHint => {
            "Nightly includes the latest changes. Stable offers stable releases when available."
        }
        Text::UpdaterDirectory => "Local build directory",
        Text::UpdaterDirectoryHint => {
            "Directory containing a completed Guild.app. Empty uses the default staging directory."
        }
        Text::UpdaterStatus => "Update status",
        Text::UpdaterStatusHint => "Check, download and restart use the selected source.",
        Text::UpdaterAbsolutePath => "Enter an absolute directory path.",
        Text::UpdaterDefaultDirectory => "Default staging directory",
        Text::UpdaterPublicHelp => {
            "GitHub Releases · proshik/guild. Public releases work anonymously. For a private repository, install gh and run: gh auth login --hostname github.com"
        }
        Text::UpdaterLoading => "Loading update preferences…",
        Text::UpdaterSaving => "Saving update preferences…",
        Text::UpdaterSaveFailed => {
            "Could not read or save update preferences. Retry before updating."
        }
        Text::UpdaterPreferenceRecovery => {
            "A previous preference change was interrupted. Choose which saved selection to keep."
        }
        Text::UpdaterKeepRequested => "Keep new selection",
        Text::UpdaterKeepPrevious => "Keep previous selection",
        Text::UpdaterCheck => "Check for updates",
        Text::UpdaterChecking => "Checking selected source…",
        Text::UpdaterDownload => "Download update",
        Text::UpdaterRestart => "Install and restart…",
        Text::UpdaterReinstall => "Reinstall…",
        Text::UpdaterInstalling => "Preparing update and restart…",
        Text::UpdaterVerifying => "Verifying update…",
        Text::UpdaterEmpty => "No eligible build in the selected source/channel.",
        Text::UpdaterCurrentVersion => "Current version",
        Text::UpdaterInstalled => "Installed",
        Text::UpdaterNoNewer => "No newer revision is available.",
        Text::UpdaterLatest => "Latest verified release",
        Text::UpdaterDetails => "Additional settings and details",
        Text::UpdaterCandidate => "Selected build",
        Text::UpdaterHistory => "Available releases",
        Text::UpdaterNotes => "Release notes",
        Text::UpdaterUnverified => {
            "Some published releases have missing, invalid or unsupported signed update metadata and cannot be installed."
        }
        Text::UpdaterCacheWarning => {
            "The verified download is ready, but its restart cache could not be saved."
        }
        Text::UpdaterAccessError => {
            "Repository is private, missing or inaccessible. Check authorized gh access, then retry."
        }
        Text::UpdaterNetworkError => {
            "Could not reach the update source. Check the connection and retry."
        }
        Text::UpdaterRateError => {
            "GitHub request limit reached. Retry after the reset time shown below."
        }
        Text::UpdaterVerificationError => {
            "Update failed validation. The installed app has not been replaced."
        }
        Text::UpdaterStorageError => {
            "Could not prepare update files. Check free space and directory access, then retry."
        }
        Text::UpdaterRecoveryError => {
            "An unfinished update requires manual recovery. Close the affected Guild copies before continuing."
        }
        Text::UpdaterLocalProducer => "Local build",
        Text::UpdaterCiProducer => "GitHub Actions build",
        Text::UpdaterLegacyInfo => "Legacy build; artifact identity and producer are unavailable.",
        Text::CloneDirectoryHelp => "A new folder will be created at this path.",
        Text::CloneFailureHelp => {
            "Git stopped before the repository was cloned. Expand Git output for details."
        }
        Text::CloneUrl => "Repository URL",
        Text::CloneDirectory => "Directory",
        Text::CloneChoose => "Browse…",
        Text::CloneStart => "Clone",
        Text::CloneRunning => "Cloning repository…",
        Text::CloneCancelling => "Cancelling…",
        Text::CloneCancelled => "Clone cancelled. You can retry.",
        Text::CloneFailed => "Couldn’t clone repository",
        Text::CloneHelp => "HTTPS or SSH",
        Text::CloneDetails => "Git output",
        Text::CloneHide => "Continue in background",
        Text::CloneRetained => "Retained directory:",
        Text::CloneOpen => "Open project",
        Text::CloneOpenMissing => {
            "Repository is ready. The original workspace or destination is unavailable. Open it in the current workspace or use File → Open Project."
        }
        Text::CloneComplete => "Repository cloned.",

        Text::SettingsPreview => "Font preview",
        Text::SettingsLanguage => "Language",
        Text::SettingsLocalFiles => "Local files",
        Text::SettingsColourTheme => "Colour theme",
        Text::SettingsActionBar => "Action Bar",
        Text::ActionBarItemOrder => "Items and order",
        Text::ActionBarHint => {
            "Drag to reorder; use × to remove and Add item to restore. Tab focuses an item; Option+←/→ moves it. Scroll sideways when needed. Changes apply immediately."
        }
        Text::ActionBarUsage => "Usage",
        Text::ActionBarReset => "Restore defaults",
        Text::ActionBarAdd => "Add item",
        Text::ActionBarRemove => "Remove",
        Text::ActionBarEmpty => "No items. Add an item or restore defaults.",
        Text::ActionBarAllShown => "All supported items are already shown.",
        Text::ActionBarAddHint => "Add an excluded item to the end of the Action Bar.",
        Text::ActionBarKeyboardHint => "Option+←/→ moves this item.",
        Text::SettingsTypography => "Typography",
        Text::SettingsSelection => "Selection",
        Text::LaunchPrefixes => "Launch command",
        Text::LaunchPrefixLabel => "Prefix (before program)",
        Text::LaunchArgumentsLabel => "Arguments (after program)",
        Text::LaunchPrefixHint => {
            "Prefix and arguments apply to future launches. Shell quoting is supported; variables are not expanded. Guild flags follow."
        }
        Text::SettingsNewSessions => "New tabs",
        Text::SettingsSessionList => "Session list",
        Text::SettleAfterTitle => "Settle sessions after",
        Text::SettleAfterHint => {
            "In the column's Sessions mode, open tabs idle this long go under Settled, beside closed conversations their agent last wrote to within it."
        }
        Text::SettleAfterUnit => "days",
        Text::SettingsClaudePermissions => "Claude Code permissions",
        Text::SettingsStartup => "Startup",
        Text::SettingsClosing => "Closing projects and tabs",
        Text::SettingsSaving => "Saving",
        Text::SettingsDiffs => "Diffs",
        Text::SettingsSearch => "Search settings…",
        Text::SettingsResults => "Search results",
        Text::SettingsNoResults => "No matching settings",
        Text::SettingsTrySearch => "Try a different word or clear the search.",
        Text::SettingsClear => "Clear search",
        Text::SettingsScope => "Personal preferences · All projects",
        Text::SettingsImmediate => "Changes are saved automatically.",
        Text::SettingsModified => "Modified only",
        Text::SettingsShortcuts => "Keyboard shortcuts",
        Text::SettingsGeneralIntro => "Language and local files.",
        Text::SettingsAppearanceIntro => "A workspace made comfortable to read.",
        Text::SettingsTerminalIntro => "Text and selection in your terminal tabs.",
        Text::SettingsSessionsIntro => "Choose what new tabs open and how each agent starts.",
        Text::SettingsProjectsIntro => "What happens when you open and close your work.",
        Text::SettingsEditorIntro => "Saving files and reading changes.",
        Text::SettingsKeymapIntro => "Select a shortcut to record new keys. Escape cancels.",
        Text::SettingsResultsIntro => "Matching settings across all categories.",

        Text::NewSession => "+ session",
        Text::NewTerminal => "+ terminal",
        Text::Session => "Session",
        Text::Terminal => "terminal",
        Text::Shell => "shell",
        Text::Settings => "Settings",
        Text::Open => "open",
        Text::Reopen => "resume",
        Text::NoConversation => "no conversation",
        Text::NoProjects => "No project open.",
        Text::HintNewTerminal => "New terminal",
        Text::HintRecentProjects => "Recent projects",
        Text::HintOpenProject => "Open project",
        Text::HintNewProject => "New project",
        Text::HintSettings => "Settings",
        Text::StartChoose => "choose what to start",
        Text::StartAllTools => "All tools",
        Text::StartRecent => "Recent",
        Text::StartAll => "All",
        Text::StartDefault => "Default",
        Text::StartTerminalName => "Terminal",
        Text::StartNotOnPath => "not on PATH",
        Text::StartResume => "Resume",
        Text::StartNoSessions => "No sessions in this worktree yet.",
        Text::StartSettings => "Settings",
        Text::StartNewSession => "New session",
        Text::NoProject => "No project",
        Text::LanguagePlain => "Plain text",
        Text::ChooseConversation => "Choose conversation",
        Text::ClaudeWithoutConversation => {
            "Conversation identity was not saved. Resume the existing conversation from the start screen."
        }
        Text::QuitTitle => "Quit while sessions are running?",
        Text::QuitAnyway => "Quit",
        Text::Cancel => "Cancel",
        Text::SettingsTitle => "Settings",
        Text::SectionGeneral => "General",
        Text::SectionTerminal => "Terminal",
        Text::SectionSessions => "Tabs & agents",
        Text::SectionEditor => "Editor",
        Text::AutosaveTitle => "Autosave",
        Text::AutosaveHint => {
            "Written 300 ms after the last key, on losing focus, and before a commit; never over a file that changed on disk"
        }
        Text::FileNotSaved => "Not saved",
        Text::SectionExperimental => "Experimental",
        Text::SettingsExperimentalIntro => "Try new features at your own pace.",
        Text::DevelopmentWorkflowsTitle => "Development workflows",
        Text::DevelopmentWorkflowsHint => {
            "Opt in to new development processes. Existing processes continue when this is off."
        }
        Text::SectionKeymap => "Keymap",
        Text::SettingsDone => "Done",
        Text::KeymapRecording => "press the keys…",
        Text::KeymapReset => "Reset",
        Text::KeymapTaken => "already taken by",
        Text::KeymapHint => {
            "Click a shortcut to change it. What differs from the defaults is \
             kept in a file, and a broken one changes nothing: the defaults \
             stay in effect."
        }
        Text::KeymapFileTitle => "Keymap file",
        Text::RevealFile => "Reveal",
        Text::TabLauncherOrderTitle => "New-tab launcher order",
        Text::TabLauncherOrderHint => {
            "Hold ⌘⌃, choose with ↑/↓, then release to open. Enter or a row’s number opens immediately; Escape cancels. Reorder items here; the dropdown uses the same order."
        }
        Text::DefaultTabTitle => "Default tab",
        Text::DefaultTabHint => {
            "What the strip's + opens: a harness, or a plain terminal. The arrow beside it still offers every tab."
        }
        Text::SettingsRendering => "Rendering",
        Text::AdaptiveRefreshTitle => "Adaptive rendering up to 120 Hz",
        Text::AdaptiveRefreshHint => {
            "Experimental. Up to 120 Hz on supported displays; may increase energy use while active."
        }
        Text::CopyOnSelectTitle => "Copy on select",
        Text::CopyOnSelectHint => {
            "Selected text goes straight to the clipboard. Applies to new terminals."
        }
        Text::FontSizeTitle => "Terminal font size",
        Text::FontSizeHint => "Cmd+= and Cmd+− change it on the fly.",
        Text::PermissionsTitle => "Permissions",
        Text::NativePermissionsValue => "In the agent",
        Text::CodexSandboxTitle => "Restrict access through sandbox",
        Text::CodexSandboxHint => {
            "Workspace-write when enabled; full filesystem access when disabled. Applies to new, resumed and retried Codex processes; running processes are unchanged."
        }
        Text::CodexCommandApprovalsTitle => "Request command approval",
        Text::CodexCommandApprovalsHint => {
            "On-request when enabled; never when disabled. Turning this off does not bypass sandbox restrictions. These choices override permission options in Arguments for new, resumed and retried processes."
        }
        Text::OpenCodePermissionsHint => {
            "Set permissions in OpenCode configuration or explicit launch arguments. Guild does not override them."
        }
        Text::PermissionsHint => "For new Claude Code sessions only.",
        Text::LanguageTitle => "Language",
        Text::LanguageHint => "Applies to the whole interface, including the menu bar.",
        Text::Yes => "yes",
        Text::No => "no",
        Text::MenuFile => "File",
        Text::MenuEdit => "Edit",
        Text::MenuView => "View",
        Text::MenuGo => "Navigate",
        Text::MenuHelp => "Help",
        Text::MenuSettings => "Settings…",
        Text::MenuServices => "Services",
        Text::MenuHide => "Hide Guild",
        Text::MenuHideOthers => "Hide Others",
        Text::MenuShowAll => "Show All",
        Text::MenuQuit => "Quit Guild",
        Text::MenuOpen => "Open",
        Text::MenuShowDiff => "Show Diff",
        Text::MenuExpand => "Expand",
        Text::MenuCollapse => "Collapse",
        Text::MenuCopyPath => "Copy Path",
        Text::MenuCopyFullPath => "Copy Full Path",
        Text::MenuReveal => "Reveal in Finder",
        Text::MenuCloseOthers => "Close Others",
        Text::MenuCloseAll => "Close All",
        Text::TabsHidden => "Show Hidden Tabs",
        Text::TabActions => "Recent Files, Tab Actions, and More",
        Text::NewTabKinds => "New Tab…",
        Text::MenuRecentFiles => "Recent Files",
        Text::MenuCloseAllTabs => "Close All Tabs",
        Text::MenuCloseOtherTabs => "Close Other Tabs",
        Text::MenuCloseLeft => "Close Tabs to the Left",
        Text::MenuCloseRight => "Close Tabs to the Right",
        Text::MenuCopyFileName => "Copy File Name",
        Text::MenuCopyPathWithLine => "Copy Path with Line Number",
        Text::BranchesTab => "Branches",
        Text::StashesTab => "Stashes",
        Text::BranchSearch => "Switch or type to create a branch…",
        Text::BranchHint => "enter chooses",
        Text::BranchesLocal => "Local Branches",
        Text::BranchesNone => "No branch by that name",
        Text::StashesNone => "Nothing is stashed",
        Text::MenuDiffStaged => "Show What Is Staged",
        Text::MenuDiffUnstaged => "Show What Is Not Staged",
        Text::MenuInclude => "Include in the Commit",
        Text::MenuExclude => "Leave Out of the Commit",
        Text::MenuShowBranchLog => "Show This Branch",
        Text::MenuShowHeadLog => "Show the Current Branch",
        Text::MenuCopyBranchName => "Copy Branch Name",
        Text::MenuCopyRevisionShort => "Copy Revision",
        Text::MenuCopyRevision => "Copy Full Revision",
        Text::MenuCopySubject => "Copy Message",
        Text::MenuRenameTab => "Rename Tab…",
        Text::MenuCloseTab => "Close Tab",
        Text::MenuReopenTab => "Reopen Closed Tab",
        Text::MenuCopy => "Copy",
        Text::MenuPaste => "Paste",
        Text::MenuLarger => "Larger",
        Text::MenuSmaller => "Smaller",
        Text::MenuActualSize => "Actual Size",
        Text::MenuSessionList => "Session List",
        Text::MenuGoToWaitingSession => "Go to Waiting Session",
        // The main worktree of a repository. A word rather than `main`,
        // which is a branch's name as often as not. M12.
        Text::MenuToggleColumn => "Workspaces",
        Text::MenuRenameProject => "Rename Project…",
        Text::MenuMoveToWorkspace => "Move to workspace",
        Text::MenuMoveToGroup => "Move to Group",
        Text::WorktreeTabsExpand => "Show tabs",
        Text::WorktreeTabsCollapse => "Hide tabs",
        Text::MenuNewProjectGroup => "New Group…",
        Text::MenuRemoveFromProjectGroup => "Remove from Group",
        Text::MenuRenameProjectGroup => "Rename Group…",
        Text::MenuDeleteProjectGroup => "Delete Group",
        Text::ProjectGroupNew => "New Group",
        Text::ProjectGroupRename => "Rename Group",
        Text::ProjectGroupNamePlaceholder => "Group name",
        Text::ProjectGroupMerge => "Merge projects into a group?",
        Text::ProjectGroupMergeAnd => "and",
        Text::ProjectGroupMergeConfirm => "Merge",
        Text::MenuGroupNewSession => "New Session",
        Text::MenuGroupNewSessionAsk => "New Session…",
        Text::MenuGroupBindFolder => "Bind to Folder…",
        Text::MenuGroupChangeFolder => "Change Folder…",
        Text::GroupFolderLine => "Its sessions will start in this folder.",
        Text::GroupFolderPlaceholder => "Folder path",
        Text::GroupFolderNoSuggestion => "The projects are in different folders.",
        Text::GroupFolderChoose => "Choose Folder…",
        Text::GroupFolderBind => "Bind",
        Text::GroupFolderOwnRoot => "That folder is a project of a group",
        Text::GroupFolderOtherGroup => "Another group is bound to that folder",
        Text::GroupFolderNewSession => "New Session in the Group's Folder",
        Text::GroupAddProjects => "Add Projects to Group",
        Text::GroupMemberOutside => "outside",
        Text::GroupNothingToAdd => "Every project of this workspace is in the group",
        Text::GroupFolderLinked => {
            "That folder is a linked worktree or submodule; pick the folder that holds the projects"
        }
        Text::GroupFolderGitBusy => {
            "Git is still writing for a command; bind the folder in a moment"
        }
        Text::MenuRemoveProject => "Remove Project",
        Text::MenuRemoveWorktree => "Remove Worktree…",
        Text::ActionCopyWorktreePath => "Copy Worktree Path",
        // Guild forgets the checkout; git keeps it, and so does the disk.
        Text::MenuUnlockWorktree => "Unlock Worktree",
        Text::ConfirmRemoveWorktree => "Remove",
        Text::NoticeWorktreeBusy => "An agent is working in that worktree — stop it first",
        Text::NoticeWorktreeChanged => "That worktree has changes git would not throw away",
        Text::ColumnNewWorktree => "New Worktree…",
        Text::AgentClaudeCode => "Claude Code",
        Text::AgentOpenCode => "OpenCode",
        Text::AgentCodex => "Codex",
        Text::OpenCodeNeedsV2 => "OpenCode v2 is required — please update OpenCode.",
        Text::ResumeStillClosing => {
            "This conversation's last process is still closing — try again in a moment."
        }
        Text::SessionsClear => "All clear",
        Text::SessionStateWaiting => "Waiting",
        Text::SessionStateFailed => "Error",
        Text::SessionStateWorking => "Working",
        Text::SessionStateDone => "Done",
        Text::SessionStateIdle => "Idle",
        Text::SessionsWaitingForYou => "Waiting for you",
        Text::SessionsWorking => "Working",
        Text::SessionsDone => "Done",
        Text::SessionsIdle => "Idle",
        Text::SessionsSettled => "Settled",
        Text::SessionFilter => "Filter sessions",
        Text::SessionFilterAgent => "Agent",
        Text::SessionFilterState => "State",
        Text::SessionFilterProject => "Project",
        Text::SessionShowInProjects => "Show in Projects",
        Text::SessionContinue => "Continue",
        Text::SessionsKeys => "↑ ↓ Navigate   Enter Open   Esc Close",
        Text::AgentsNone => "No agents open",
        Text::AgentsTooltip => "Agents open in the window",
        Text::NewWorktreePlaceholder => "Branch for the new worktree",
        Text::ColumnOpen => "Open…",
        Text::WorktreeDeleteTooltip => "Delete worktree",
        Text::WorktreeDeleteTitle => "Delete this worktree?",
        Text::WorktreeDeleteDescription => "Its folder and ignored files are deleted.",
        Text::WorktreeDeleteBranch => "Delete branch too",
        Text::WorktreeDeleteBranchHint => {
            "Only the confirmed branch is deleted, and only if Git safely allows it."
        }
        Text::WorktreeBranchProtected => {
            "Main/master and detached or unavailable branches are kept."
        }
        Text::WorktreeBranchUnavailable => "Unavailable branch",
        Text::WorktreeDetached => "Detached HEAD",
        Text::WorktreeDelete => "Delete",
        Text::WorktreeRemovalUnavailable => {
            "The repository is unavailable. Try again after its status is ready."
        }
        Text::SidebarPinned => "Pinned",
        Text::WorktreePin => "Pin worktree",
        Text::WorktreeUnpin => "Unpin worktree",
        Text::SidebarProjects => "Projects",
        Text::SidebarSessions => "Sessions",
        Text::ColumnShowSessions => "Show Sessions",
        Text::ColumnShowProjects => "Show Projects",
        Text::SidebarProjectsAndGroups => "Projects & Groups",
        Text::ProjectSort => "Sort projects",
        Text::ProjectSortRecent => "Recent activity",
        Text::ProjectSortMost => "Most activity",
        Text::ProjectSortUser => "Last user message",
        Text::ProjectSortManual => "Manual",
        Text::NoticeNobodyWaiting => "No session is waiting for an answer",
        // git still lists it, and its folder is not there: `git worktree
        // prune` is what forgets it, and that is git's to be asked for.
        Text::NoticeWorktreeGone => "That worktree's folder is gone",
        Text::MenuFullScreen => "Full Screen",
        Text::TerminalPreviousCommand => "Previous Command",
        Text::TerminalNextCommand => "Next Command",
        Text::TerminalSelectOutput => "Select Command Output",
        Text::TerminalCopyOutput => "Copy Command Output",
        Text::TerminalExitStatus => "Exit status",
        Text::MenuClearTerminal => "Clear Terminal",
        Text::MenuNextTab => "Next Tab",
        Text::MenuPreviousTab => "Previous Tab",
        Text::MenuNextProject => "Next Project",
        Text::MenuNextWorkspace => "Next Workspace",
        Text::MenuPreviousWorkspace => "Previous Workspace",
        Text::MenuSplitAreaRight => "Split Right",
        Text::MenuSplitAreaDown => "Split Down",
        Text::MenuSplitPaneRight => "Split Tab Right",
        Text::MenuSplitPaneDown => "Split Tab Down",
        Text::MenuCloseArea => "Close Area",
        Text::ActionFocusLeft => "Focus Left",
        Text::ActionFocusRight => "Focus Right",
        Text::ActionFocusUp => "Focus Up",
        Text::ActionFocusDown => "Focus Down",
        Text::LastAreaStays => "the last area stays",
        Text::NoticeLastAreaStays => "The last area stays",
        Text::NoticeNoRoomToSplit => "No room to split this area",
        Text::NoticeNoRoomToSplitTab => "No room to split this tab",
        Text::MenuMoveToArea => "Move to Area",
        Text::MenuAreaLeft => "Left Area",
        Text::MenuAreaRight => "Right Area",
        Text::MenuAreaAbove => "Area Above",
        Text::MenuAreaBelow => "Area Below",
        Text::MenuNewAreaRight => "New Area to the Right",
        Text::MenuNewAreaBelow => "New Area Below",
        Text::WorkspaceNew => "New Workspace",
        Text::WorkspaceNamePlaceholder => "Workspace name",
        Text::WorkspaceRenameAction => "Rename",
        Text::WorkspaceRename => "Rename Workspace",
        Text::WorkspaceChangeIcon => "Change Icon",
        Text::WorkspaceChooseIcon => "Choose icon",
        Text::WorkspaceDelete => "Delete",
        Text::WorkspaceNoIcon => "No icon",
        Text::WorkspaceCapacity => "At most five workspaces can be created.",
        Text::WorkspaceLastCannotDelete => "The last workspace can't be deleted.",
        Text::WorkspaceOnlyOne => "There is only one workspace.",
        Text::MenuRecentProjects => "Recent Projects…",
        Text::MenuShowLog => "Show Log",
        Text::MenuNew => "New",
        Text::MenuExistingSources => "Project from Existing Sources…",
        Text::MenuNewProject => "Project…",
        Text::MenuOpenProject => "Open…",
        Text::MenuCloseProject => "Close Project",
        Text::MenuAbout => "About Guild",
        Text::MenuUpdateGuild => "Update Guild…",
        Text::UpdateSourceSearch => "Search source branches…",
        Text::UpdateTerminal => "Open Guild Terminal",
        Text::UpdateTerminalUnavailable => {
            "Open the originating worktree in this workspace to use its terminal."
        }
        Text::UpdateSourceLocal => "local only",
        Text::UpdateBranch => "Update branch…",
        Text::UpdateBranchConfirm => "Update",
        Text::UpdateBranchRunning => "Updating branch…",
        Text::UpdateBranchChoose => "Choose source…",
        Text::UpdateBranchUnsaved => "Save unsaved files before updating the branch.",
        Text::UpdaterReplaceTitle => "Replace installed build?",
        Text::UpdaterReplaceAction => "Replace and Restart",
        Text::UpdaterUpToDate => "This build is already installed.",
        Text::UpdateButton => "Update",
        Text::WhatsNewTitle => "What's new in Guild",
        Text::WhatsNewContinue => "Continue",
        Text::UsageSession => "Session",
        Text::UsageWeek => "Week",
        Text::UsageAllProviders => "All providers",
        Text::UsageNotInstalled => "not installed",
        Text::UsageCouldNotRead => "couldn't read",
        Text::UsageRateLimited => "rate limited",
        Text::UsageNotSignedIn => "not signed in",
        Text::UsageNothingToRead => "no limits to read",
        Text::UsageRefresh => "Refresh",
        Text::UsageAsking => "asking…",
        Text::OpenSelectedApp => "Open",
        Text::OpenInAppsTitle => "Open In Apps",
        Text::OpenInAppsHint => "Choose installed apps, visibility and picker order.",
        Text::OpenInAdd => "Add app",
        Text::OpenInName => "Display name",
        Text::OpenInChoose => "Choose app…",
        Text::OpenInSearch => "Search applications…",
        Text::OpenInRemove => "Remove",
        Text::OpenInUnavailable => "Not installed or not connected",
        Text::OpenInLoading => "Loading applications…",
        Text::OpenInNoResults => "No applications found",
        Text::OpenInMoveUp => "Move up",
        Text::OpenInMoveDown => "Move down",
        Text::OpenInButton => "Open In…",
        Text::NoticeRestartingToUpdate => "Restarting to apply update...",
        Text::NoticeUpdateNothingStaged => {
            "No newer release is waiting. just release on main in Guild's checkout stages one, and Update applies it."
        }
        Text::NoticeUpdateNotABundle => {
            "This Guild is not an installed bundle — there is nothing to replace."
        }
        Text::NoticeUpdateNoHelper => {
            "This Guild's bundle has no swap-guild.sh — install it once with just install."
        }
        Text::ConfirmClose => "Close",
        Text::MenuOpenRecent => "Open Recent",
        Text::MenuClearRecent => "Clear Menu",
        Text::MenuNoRecent => "Nothing yet",
        Text::NoticeWorktreeOrderUnsupportedPath => {
            "Worktree order was not changed: a checkout path is not valid UTF-8"
        }
        Text::NoticeStateNotSaved => "State was not saved",
        Text::NoticeSettingsNotSaved => "Settings were not saved",
        Text::NoticeKeymapNotSaved => "The keymap was not saved",
        Text::NoticeKeymapNotRead => {
            "The keymap file could not be read — the defaults are in effect"
        }
        Text::ThemeTitle => "Theme",
        Text::ThemeHint => {
            "System follows macOS's appearance; the terminal and the editors repaint with the window"
        }
        Text::ThemeSystem => "System",
        Text::ThemeLight => "Light",
        Text::ThemeDark => "Dark",
        Text::CustomizeThemeTitle => "Customize theme",
        Text::CustomizeThemeHint => {
            "Tint the Light or Dark theme's background, accent and highlight. Changes show at once; Save keeps them"
        }
        Text::CustomizeThemeClose => "Close without saving",
        Text::ThemeBackground => "Background",
        Text::ThemeAccent => "Accent",
        Text::ThemeHighlight => "Highlight",
        Text::ThemeHue => "Hue",
        Text::ThemeSaturation => "Saturation",
        Text::ThemeIntensity => "Intensity",
        Text::ThemeCopyFromLight => "Copy from Light",
        Text::ThemeCopyFromDark => "Copy from Dark",
        Text::ThemeSaveLight => "Save Light Theme",
        Text::ThemeSaveDark => "Save Dark Theme",
        Text::ThemeReset => "Reset",
        Text::ThemeEyedropper => "Pick a colour from the screen",
        Text::ThemeHexInvalid => "Not a colour — type #rrggbb or #rgb",
        Text::ThemeDefaultSwatch => "Default",
        Text::AppearanceSection => "Appearance",
        Text::UiFontSizeTitle => "Interface font size",
        Text::UiFontSizeHint => {
            "Everything but the terminal: tabs, the project row, the menus and this screen"
        }
        Text::UiFontTitle => "Interface font",
        Text::TerminalFontTitle => "Terminal font",
        Text::TerminalFontHint => "Font used for terminal tabs.",
        Text::CodeFontTitle => "Monospace font",
        Text::CodeFontHint => "Font used for code and diffs.",
        Text::FontCustom => "Custom",
        Text::FontBeta => "Beta",
        Text::FontFromSystem => "From the system",
        Text::LogDirectoryTitle => "Log",
        Text::PanelProject => "Explorer",
        Text::PanelSessions => "Sessions",
        Text::PanelTerminal => "Terminal",
        Text::NewPanelTerminal => "New terminal",
        Text::ClosePanelTerminal => "Close terminal and end its processes",
        Text::PanelTerminalEmpty => "Open a terminal in this project",
        Text::PanelOutput => "Run",
        Text::RecentSearch => "Search recent projects",
        Text::RecentNoMatch => "No recent project matches that",
        Text::RecentNever => "Nothing has been opened yet — File ▸ Open",
        Text::PanelProjectEmpty => "The project's files will be shown here",
        Text::TreeEmpty => "Nothing here yet",
        Text::TreeCannotRead => "This directory could not be read",
        Text::ImageSource => "Source",
        Text::ImageFit => "Fit",
        Text::ImageLoading => "Loading image…",
        Text::ImageCannotRead => "Image could not be loaded",
        Text::FileNotText => "Not a text file",
        Text::FileTooLarge => "Too large to open here",
        Text::FileGone => "This file is no longer on disk",
        Text::FileCannotRead => "This file could not be read",
        Text::SettingsMarkdown => "Markdown",
        Text::MarkdownDefaultViewTitle => "Markdown default view",
        Text::MarkdownDefaultViewHint => {
            "Applies to newly opened Markdown files in all projects. Open panes keep their current view."
        }
        Text::MarkdownEditor => "Editor",
        Text::MarkdownSplit => "Editor and Preview",
        Text::MarkdownPreview => "Preview",
        Text::DiagramRendering => "Rendering diagram…",
        Text::DiagramResourcesUnavailable => {
            "Diagram resources are missing or corrupt. Reinstall Guild; source remains editable."
        }
        Text::MarkdownPreviewLimit => {
            "Preview is unavailable for this large document. Continue in Editor."
        }
        Text::FileChangedOnDisk => "Changed on disk while edited here",
        Text::MenuSave => "Save",
        Text::MenuSaveAll => "Save All",
        Text::MenuReloadFile => "Reload from Disk",
        Text::UnsavedDetail => "They're lost if you don't save.",
        Text::Save => "Save",
        Text::DontSave => "Don't Save",
        Text::RevealActiveFile => "Reveal Active File",
        Text::NoFileInFront => "no file is open in this tab",
        Text::ShowDiff => "Show Diff",
        Text::ShowChanges => "Show Changes",
        Text::DiffChanges => "Diff",
        Text::DiffLoading => "Reading the last commit\u{2026}",
        Text::DiffNoChanges => "No changes since the last commit",
        Text::DiffBinary => "Binary file, no diff to show",
        Text::DiffTooLarge => "Too large to diff here",
        Text::DiffNoRepository => "Not in a git repository",
        Text::DiffNoGit => "git is not installed on this machine",
        Text::Changes => "Changes",
        Text::Log => "Log",
        Text::PanelChangesEmpty => "No changes since the last commit",
        Text::Unversioned => "Unversioned files",
        Text::NextChange => "Next change",
        Text::RollbackLines => "Rollback Lines",
        Text::PreviousChange => "Previous change",
        Text::BaseStaged => "Staged",
        Text::BaseCurrent => "Current version",
        Text::BaseIndex => "staged",
        Text::BaseUnstaged => "unstaged",
        Text::Refresh => "Refresh",
        Text::ShowSelectedDiff => "Show diff",
        Text::TickAll => "Tick all",
        Text::UntickAll => "Untick all",
        Text::CollapseAll => "Collapse all",
        Text::ExpandAll => "Expand all",
        Text::CommitPlaceholder => "Commit message",
        Text::CommitButton => "Commit",
        Text::CommitNothingStaged => "Nothing is ticked",
        Text::CommitNoMessage => "Write a message first",
        Text::MenuCommit => "Commit…",
        Text::LogLoading => "Reading\u{2026}",
        Text::LogNoMatches => "No commits match the filter",
        Text::LogLocal => "Local",
        Text::LogSelectCommit => "Select a commit to see it",
        Text::DiffViewTitle => "Diff view",
        Text::DiffViewHint => {
            "Two columns with changed lines facing each other, or one as git prints it"
        }
        Text::DiffSideBySide => "side by side",
        Text::DiffUnified => "unified",
        Text::ToolCommit => "Commit",
        Text::ToolSearch => "Search",
        Text::ToolProblems => "Problems",
        Text::ToolFind => "Find",
        Text::PanelFindEmpty => "Press ⌥F7 on a name to find its usages",
        Text::UsagesInProjectFiles => "Usages in Project Files",
        Text::UsagesInLibraries => "Usages in Libraries",
        Text::UsagesSearching => "Searching…",
        Text::UsagesCannotSearch => "Cannot search for usages from this location.",
        Text::UsagesCannotSearchHint => {
            "Place the caret on the element to find usages for and try again."
        }
        Text::UsagesNoPreview => "No preview for a library's file",
        Text::FindRerun => "Rerun",
        Text::FindExpandAll => "Expand All",
        Text::FindCollapseAll => "Collapse All",
        Text::ActionFindUsages => "Find Usages",
        Text::ActionNextUsage => "Next Occurrence",
        Text::ActionPreviousUsage => "Previous Occurrence",
        Text::ToolGit => "Git",
        Text::ConsoleTab => "Console",
        Text::LogSearch => "Text or hash",
        Text::LogBranchFilter => "Branch",
        Text::LogUserFilter => "User",
        Text::LogDateFilter => "Date",
        Text::LogAnyone => "Anyone",
        Text::LogAnyTime => "All",
        Text::LogLastDay => "Last 24 hours",
        Text::LogLastWeek => "Last week",
        Text::LogLastMonth => "Last month",
        Text::LogHead => "HEAD",
        Text::LogAllBranches => "All",
        Text::LogAllBranchesRow => "All Branches",
        Text::LogHeadRow => "HEAD (Current Branch)",
        Text::LogBranchSearch => "Branch or tag",
        Text::LogLocalBranches => "Local",
        Text::LogRecentBranches => "Recent",
        Text::LogClearHint => "Press \u{232b} to clear the filter",
        Text::ConsoleEmpty => "Nothing has been done to this repository yet",
        Text::MoveTo => "Move to",
        Text::PanelOptions => "Options",
        Text::PanelHide => "Hide",
        Text::DockLeft => "Left",
        Text::DockRight => "Right",
        Text::DockBottom => "Bottom",
        Text::DockEmpty => "Nothing here",
        Text::DockEmptyHint => {
            "Drag a tool here from the status bar, or move one from the View menu"
        }
        Text::Annotate => "Annotate",
        Text::AnnotateTab => "annotated",
        Text::AnnotateNotCommitted => "Not committed yet",
        Text::PanelLogEmpty => "No commits yet",
        Text::PanelSessionsEmpty => "This project's sessions and tasks will be shown here",
        Text::PanelOutputEmpty => "Builds, tests and logs will be shown here",
        Text::PanelSearchEmpty => "Type to search this project",
        Text::PanelProblemsEmpty => "Nothing is wrong here",
        Text::SearchPlaceholder => "Find in files…",
        Text::SearchMask => "File mask",
        Text::SearchMatchCase => "Match case",
        Text::SearchWholeWord => "Words",
        Text::SearchRegex => "Regular expression",
        Text::SearchNothingFound => "Nothing matches",
        Text::SearchRunning => "searching…",
        Text::SearchCapped => "and more",
        Text::RunNothingYet => "Nothing has been run here yet",
        Text::RunStart => "Run",
        Text::RunAgain => "Run again",
        Text::RunClosePrompt => "Stop and close this run?",
        Text::RunStopAndClose => "Stop and close",
        Text::RunStop => "Stop",
        Text::RunNotRunning => "Nothing is running",
        Text::RunClear => "Clear",
        Text::RunChoose => "Choose a command to run",
        Text::RunTaskSettings => "Run arguments",
        Text::RunConfigure => "Run with arguments: Option-click",
        Text::RunArgumentsRequired => "Required argument",
        Text::RunVariables => "Optional NAME=value overrides (quote spaces)",
        Text::RunDefault => "Default task",
        Text::BlameUnavailable => "Blame unavailable",
        Text::BlameCommitUnavailable => "Historical commit is unavailable",
        Text::BlameFileUnavailable => "Historical file is unavailable in this commit",
        Text::RunSourceChanged => "Task source changed; choose the task again",
        Text::RunFailed => "could not start",
        Text::RunPlaceholder => "Run a command…",
        Text::RunRecent => "recent",
        Text::RunDeclared => "declared",
        Text::RunNothingToOffer => "Type a command to run it",
        Text::MenuRun => "Run",
        Text::FontBundled => "ships with Guild",
        Text::FontSearch => "Find a face…",
        Text::FontNoneFound => "No family of that name",
        Text::ServerNone => "no server",
        Text::ServerStarting => "server starting…",
        Text::ServerReady => "ready",
        Text::ServersNone => "No language server has started in this project",
        Text::ServersTooltip => "Language servers",
        Text::XcodeSetUp => "Set up for Xcode",
        Text::XcodeSettingUp => "Setting up for Xcode…",
        Text::GoToDefinition => "Go to definition",
        Text::BaseProposal => "proposed",
        Text::AcceptProposal => "Apply",
        Text::AcceptProposalHint => {
            "The agent is waiting: it makes this edit, and its terminal does not ask again"
        }
        Text::RejectProposal => "Refuse",
        Text::RejectProposalHint => "The agent is waiting: this writes nothing",
        Text::ProjectsSection => "Projects",
        Text::ReopenLastTitle => "Reopen the last project on launch",
        Text::ReopenLastHint => "Its tabs come back with it",
        Text::AutoFastForwardTitle => "Auto fast-forward default branch",
        Text::AutoFastForwardHint => {
            "After each background fetch — when a project opens and every 5 minutes — the clean default branch moves to its upstream; a branch that has diverged is left alone"
        }
        Text::GitLabMrTitle => "GitLab merge requests in the column",
        Text::GitLabMrHint => {
            "Show each worktree's merge request and what blocks it, read with your glab login. Off, Guild starts no new request; one already running finishes and its answer is dropped."
        }
        Text::MenuViewMr => "View MR",
        Text::MenuCopyMrUrl => "Copy MR URL",
        Text::ConfirmCloseTitle => "Ask before closing an agent at work",
        Text::ConfirmCloseHint => {
            "A tab, a project or its removal — when an agent is working or waiting for you"
        }
        Text::ConfirmCloseAgentTitle => "Ask before closing an agent's tab",
        Text::ConfirmCloseAgentHint => {
            "Claude Code, Codex or opencode — even when it is idle or finished"
        }
        Text::ForgetOnCloseTitle => "Forget a project's sessions when it closes",
        Text::ForgetOnCloseHint => "Off, a reopened project comes back with its conversations",

        // ---- M8.8.5 — the status bar and Problems ----
        Text::GoToLine => "Go to Line:Column",
        Text::SoftWrapOn => "Soft line wrap: on (click to disable)",
        Text::SoftWrapOff => "Soft line wrap: off (click to enable)",
        Text::LineEndings => "Line endings",
        Text::ProblemsNoServer => "No language server has looked here",
        Text::ProblemsLooking => "Still looking…",
        Text::ProblemsCapped => "and more",
        // The file tree — M8.8.3.
        Text::MenuNewFile => "File…",
        Text::MenuNewDirectory => "Directory…",
        Text::MenuOpenIn => "Open In",
        Text::MenuOpenTerminal => "Terminal",
        Text::MenuFindInFiles => "Find in Files…",
        Text::MenuReplaceInFiles => "Replace in Files…",
        Text::MenuMove => "Move…",
        Text::MenuCutFiles => "Cut",
        Text::MenuCopyFiles => "Copy",
        Text::MenuPasteFiles => "Paste",
        Text::MenuOpenAsProject => "Open as a project",
        Text::MenuHoldsOpenProject => "Holds an open project",
        Text::FileTransferCompleted => "Transferred",
        Text::FileTransferRemaining => "Not transferred",
        Text::FileClipboardChanged => {
            "Clipboard sources changed. Select and copy or cut them again."
        }
        Text::FileTransferBusy => "Transferring files…",
        Text::TrashUnsaved => {
            "Unsaved edits will remain open. Saving them later can recreate the deleted files."
        }
        Text::FileTransferDestination => "Destination relative to project (empty = root)",
        Text::FileTransferCollision => "These names already exist",
        Text::FileTransferSkip => "Skip",
        Text::FileTransferKeepBoth => "Keep Both",
        Text::MenuRename => "Rename…",
        Text::MenuDelete => "Delete",
        Text::MenuAnnotate => "Annotate",
        Text::MenuAnnotateWithBlame => "Annotate with Git Blame",
        Text::MenuAddToVcs => "Add to VCS",
        Text::TreeNewFile => "New file",
        Text::TreeNewDirectory => "New directory",
        Text::TreeRename => "Rename",
        Text::Delete => "Move to Trash",
        Text::SearchScope => "In",
        Text::ExternalLibraries => "External Libraries",
        // Navigating
        Text::MenuBack => "Back",
        Text::MenuForward => "Forward",
        Text::MenuLastEdit => "Last Edit Location",
        Text::MenuRecentLocations => "Recent Locations",
        Text::MenuGoToLine => "Go to Line:Column…",
        Text::MenuSelectInExplorer => "Select in Explorer",
        Text::RecentFilesSearch => "Search recent files",
        Text::RecentFilesNever => "No file of this project has been opened yet",
        Text::RecentFilesNoMatch => "No recent file matches that",
        Text::RecentFilesNoEdits => "Nothing here has been edited yet",
        Text::RecentFilesEdited => "Show edited only",
        Text::RecentFilesCloseHint => "⌫ closes",
        Text::RecentLocationsSearch => "Search recent locations",
        Text::RecentLocationsNever => "Nowhere has been visited yet",
        Text::GoToLinePlaceholder => "line:column",
        // git: the Commit panel's menu, and Push — M8.8.4.
        Text::MenuRollback => "Rollback…",
        Text::Rollback => "Roll back",
        Text::PushTab => "Push",
        Text::PushDialogTitle => "Push Commits",
        Text::PushTagsLabel => "Push tags:",
        Text::PushTagsAll => "All",
        Text::PushTagsCurrent => "Current Branch",
        Text::PushHooks => "Run Git hooks",
        Text::PushDetails => "Show Details",
        Text::PushPreview => "Diff Preview",
        Text::PushTarget => "Target branch",
        Text::PushTargetApply => "Apply",
        Text::PushEmpty => "No commits selected",
        Text::PushHelp => {
            "Select the branch to inspect all changes, or a commit for its files and details. Command-click selects several commits; Shift-click selects a range. Push always sends the whole branch. Run Git hooks runs the repository's pre-push hook."
        }
        Text::PushNew => "New",
        Text::DiffEditSource => "Edit Source",
        Text::DiffWindowNextFile => "Compare Next File",
        Text::DiffWindowPreviousFile => "Compare Previous File",
        Text::PushButton => "Push",
        Text::PushRunning => "Pushing…",
        Text::PushSetUpstream => "Set upstream",
        Text::PushNoRemote => "This repository has no remote",
        Text::PushDetached => "HEAD is detached: there is no branch to push",
        Text::ChangesSelectRow => "Select a change first",
        Text::RollbackNothingToUndo => "git has never seen this file: there is nothing to put back",
        Text::AlreadyInVcs => "git already has this file",
        // The audit's sharp edges — M5.11.
        Text::FileReadOnly => "this is a dependency's source, read here and not written",
        Text::KeymapNeedsModifier => "add cmd, ctrl or alt",
        Text::DropStash => "Drop",
        Text::NoticeStashGone => {
            "That stash entry is no longer in the list, so nothing was dropped"
        }
        Text::NoticeHunkStale => {
            "The file changed since its changes were shown; they are recomputed — try again"
        }
        Text::NoticeHunkConverted => {
            "Git converts this file when it stores it — stage the whole file"
        }
        Text::NoticeHunkHeadMoved => {
            "A commit landed while the change was unstaged — check the Staged diff"
        }
        Text::NoticeHunkUnverified => {
            "Git wrote the change, but the index could not be read back — check the Staged diff"
        }
        Text::NoticeHunkUnconfirmed => {
            "Guild could not verify whether Git wrote the change — check the Staged diff"
        }
        Text::StageHunk => "Stage change",
        Text::UnstageHunk => "Unstage change",
        Text::HunkWider => {
            "The index holds a change wider than this one — use the Staged or Unstaged diff"
        }
        Text::HunkMixed => {
            "Part of this change is already staged — stage or unstage it in the Unstaged or Staged diff"
        }
        Text::HunkWholeFile => "Only the whole file can be staged",
        Text::HunkLineEndings => {
            "This change differs from the index only in line endings — stage the whole file"
        }
        Text::NoticeHunkCommitCancelled => {
            "Commit not made: check the index and press Commit again"
        }
        Text::NoticeLoginPathFailed => {
            "The login shell did not say what PATH is in time; sessions and language servers \
             use the one Guild was started with"
        }
        // The terminal for agents — M5.12.
        Text::MenuSelectAll => "Select All",
        Text::MenuFind => "Find…",
        Text::FindPlaceholder => "Find in the output",
        Text::FindOlder => "Next match, further back",
        Text::FindNewer => "Previous match, further down",
        Text::FindClose => "Close the find bar",
        Text::FindScreenOnly => {
            "This program draws on the screen alone: only what is on it is searched"
        }
        // Search and navigation — M8.9.
        Text::MenuSearchEverywhere => "Search Everywhere",
        Text::MenuGoToFile => "Go to File…",
        Text::MenuGoToSymbol => "Go to Symbol…",
        Text::MenuFindAction => "Find Action…",
        Text::EverywhereAll => "All",
        Text::EverywhereFiles => "Files",
        Text::EverywhereSymbols => "Symbols",
        Text::EverywhereActions => "Actions",
        Text::EverywhereText => "Text",
        Text::EverywhereNonProject => "Include non-project items",
        Text::EverywherePreview => "Preview",
        Text::EverywhereIndexing => "Indexing…",
        Text::EverywhereTypeToSearch => "Type to search",
        Text::ReplaceInFilesTitle => "Replace in Files",
        Text::FindInFilesTitle => "Find in Files",
        Text::FindInFilesPin => "Keep the window open",
        Text::FindInFilesUnpin => "Close the window after opening a result",
        Text::FindInFilesOpenInWindow => "Open in Find Window",
        Text::FindInFilesInProject => "In Project",
        Text::FindInFilesDirectory => "Directory",
        Text::FindInFilesChooseDirectory => "Choose a directory…",
        Text::FindInFilesUseMask => "Use the file mask",
        Text::FindInFilesHistory => "Recent searches",
        Text::FindInFilesMaskHistory => "Recent masks",
        Text::FindInFilesClose => "Close",
        Text::ActionNewSession => "New Session",
        Text::ActionNewTerminal => "New Terminal",
        Text::ActionCloseOtherTabs => "Close Other Tabs",
        Text::ActionCloseAllTabs => "Close All Tabs",
        Text::ActionSaveFile => "Save File",
        Text::ActionSaveAllFiles => "Save All Files",
        Text::ActionRun => "Run Command",
        Text::ActionStopRun => "Stop Command",
        Text::ActionClearRun => "Clear Output",
        Text::ActionAddProject => "Open Project…",
        Text::ActionNewProject => "New Project…",
        Text::ActionIncreaseFontSize => "Increase Terminal Font Size",
        Text::ActionDecreaseFontSize => "Decrease Terminal Font Size",
        Text::ActionResetFontSize => "Reset Terminal Font Size",
        Text::ActionToggleSidebar => "Toggle Workspaces",
        Text::ActionGoToDefinition => "Go to Definition",
        Text::ActionFind => "Find…",
        Text::ActionReplace => "Replace…",
        Text::ActionNextChange => "Next Change",
        Text::ActionPreviousChange => "Previous Change",
        Text::ActionStageHunk => "Stage Hunk",
        Text::ActionUnstageHunk => "Unstage Hunk",
        Text::ActionRefreshChanges => "Refresh Changes",
        Text::ActionShowSelectedDiff => "Show Diff of the Selected Change",
        Text::ActionIncludeSelectedChanges => "Include Selected Changes in Commit",
        Text::ActionExcludeSelectedChanges => "Exclude Selected Changes from Commit",
        Text::ActionTickAllChanges => "Tick All Changes",
        Text::ActionUntickAllChanges => "Untick All Changes",
        Text::ActionCollapseChanges => "Collapse All Changes",
        Text::ActionExpandChanges => "Expand All Changes",
        Text::ActionPreviousProject => "Previous Project",
        Text::ActionShowTabList => "Show Tab List",
        Text::ActionPush => "Push Commits…",
        Text::ActionNewFile => "New File…",
        Text::ActionNewDirectory => "New Directory…",
        Text::ActionRenameFile => "Rename File…",
        Text::ActionJumpToSource => "Jump to Source",
        Text::ActionDeleteFile => "Move File to Trash…",
        Text::ActionToggleLeftPanel => "Toggle Left Panel",
        Text::ActionToggleRightPanel => "Toggle Right Panel",
        Text::ActionToggleBottomPanel => "Toggle Bottom Panel",
        Text::ActionWidenPanel => "Widen Panel",
        Text::ActionNarrowPanel => "Narrow Panel",
        Text::ActionStretchToLeft => "Stretch to Left",
        Text::ActionStretchToRight => "Stretch to Right",
        Text::ActionStretchToTop => "Stretch to Top",
        Text::ActionStretchToBottom => "Stretch to Bottom",
        Text::ActionToggleSessions => "Toggle Session List",
        Text::ActionToggleFullScreen => "Toggle Full Screen",
        Text::ActionShowLog => "Show Guild's Log",
    }
}
