# terminal-jobs-usability Specification

## Purpose
Make terminal job recovery understandable through compact presentation, reliable ordering and explicit saved-output states without changing process ownership.

## Requirements

### Requirement: Consistent compact popup
The Terminal Jobs list SHALL use the existing Guild popup appearance and dismissal conventions, with bounded scrolling, readable localized labels and no redundant close button. It SHALL include a fixed scope header and scrollable ongoing/completed groups within the existing 400 × 420 viewport-clamped bounds. Scope changes SHALL start the selected list at the top.

#### Scenario: Dismissal and focus
- **WHEN** the list or its scope menu is open and the user clicks outside, presses Escape, toggles its trigger, or opens another popup
- **THEN** dismissal follows neighboring popup/menu rules, Escape restores the appropriate prior focus, and no scope/dismissal gesture activates or types into the terminal behind it.

#### Scenario: Small viewport and long history
- **WHEN** the viewport is small or many jobs have long titles/context labels
- **THEN** the list remains within the viewport, its fixed scope header remains reachable, rows scroll, labels truncate with accessible full context and actions remain reachable.

#### Scenario: Wider scope context
- **WHEN** the user selects a scope broader than This session
- **THEN** rows show their original worktree/session context, except that This worktree omits a saved worktree field only when its known original path matches the known selected worktree identified in the header; All launches also identifies the project/repository, and unknown legacy context retains its existing explicit text; branch is information rather than a filter.

#### Scenario: Scope menu keyboard and localization
- **WHEN** the user opens the scope menu using mouse or keyboard in English or Russian and changes its selection
- **THEN** all available scopes are reachable through established menu navigation, the selected label/counts agree with the visible groups, and closing the menu preserves popup focus.

### Requirement: Honest launch chronology
Terminal Jobs SHALL group starting/running/cancelling jobs before completed jobs, list known launch times newest first within each group, show a readable date/time and status, and retain stable in-group order when a launch is reattached or its state changes. Completion SHALL move the job into the completed group according to its original time.

#### Scenario: Multiple launches
- **WHEN** jobs start at different times and the application later reopens their saved receipts
- **THEN** ongoing jobs appear above completed launches; each group orders known original times descending with a deterministic tie-breaker, and duplicate request identities do not reset launch time.

#### Scenario: Local calendar day
- **WHEN** a launch belongs to the current local calendar day
- **THEN** its row shows only the time; launches on other days show the date and time, with the comparison using the same local offset as formatting.

#### Scenario: Legacy receipts
- **WHEN** saved receipts contain no launch time
- **THEN** they load successfully, appear after dated jobs within their state group with an explicit unknown-time label, and never receive invented historical timestamps.

#### Scenario: New scope and completion
- **WHEN** a person widens scope after scrolling or a running job completes while its list is open
- **THEN** scope selection starts at the newest ongoing entries and completion moves the job to its chronological position in Completed without restarting it.

### Requirement: Useful completed output
Showing output SHALL reveal retained terminal content when available and clearly distinguish retained-but-empty content from deleted or unavailable output. Default geometry SHALL be moderate and clamped to the target; explicit requested geometry and user resizing SHALL remain respected.

#### Scenario: Completed ordinary output
- **WHEN** a fixture writes diagnostic text and exits, and the user later shows its output
- **THEN** that diagnostic remains readable with exit status, without restarting the job or changing its owner.

#### Scenario: Fullscreen program teardown
- **WHEN** a terminal job leaves its alternate screen or clears it while exiting
- **THEN** reopening does not misleadingly present an unexplained blank result; the application shows retained useful content where supported or explicitly explains that the terminal has no saved text.

#### Scenario: Empty or unavailable output
- **WHEN** a job emitted no displayable content or its buffer was deleted
- **THEN** the list/view describes that condition and does not offer an action that opens an unexplained empty surface.

#### Scenario: Unavailable output in history
- **WHEN** a receipt has the unavailable output state in this Guild session, excluding explicit deleted output and retained-but-empty output states
- **THEN** its history row remains visible with metadata and any independently available Open run action, omits the unavailable-output explanation and offers no inaccessible output action; explicit deleted and retained-but-empty states retain their corresponding explanations.

#### Scenario: Ongoing program
- **WHEN** a running job is hidden, shown, resized or receives terminal input
- **THEN** normal terminal rendering, Escape forwarding, process lifetime and caller ownership remain unchanged.

### Requirement: Explicit saved-output deletion
A completed job SHALL offer an action whose label describes deleting saved output. After deletion its launch metadata and exit status SHALL remain with a visible output-deleted state; running jobs SHALL NOT offer deletion.

#### Scenario: Delete saved output
- **WHEN** the user deletes a completed job's saved output
- **THEN** the buffer and its presentation are released, the row explicitly reports deletion, and other jobs and stored review report files remain untouched.

#### Scenario: Restart after deletion
- **WHEN** Guild reloads a receipt after explicit output deletion
- **THEN** the row still explains deletion rather than implying that output can be recovered.

### Requirement: Labelled original launch fields
For launches with saved context, broader scopes SHALL identify original worktree, last-observed branch and session as separate localized labelled fields; All launches SHALL additionally label the project/repository. This worktree SHALL omit only the redundant worktree field when saved context has a known original worktree path matching the known selected worktree identified in the header; labelled branch/session and full-path access SHALL remain. Wider scopes, differing original paths and unknown paths SHALL retain their original context presentation. Values SHALL truncate independently within the existing bounded popup, with full original paths accessible in its bounded hoverable tooltip. Legacy messages SHALL retain their existing text and no historical data SHALL be invented.

#### Scenario: Distinguish project worktree branch and session
- **WHEN** All launches shows a launch with complete saved context and long names
- **THEN** four separately labelled fields distinguish project, worktree, last observed branch and session; the popup remains bounded and full paths remain reachable by hover.

#### Scenario: Missing legacy metadata
- **WHEN** a saved receipt lacks launch context
- **THEN** its existing explicit unavailable-context messages remain, with no replacement summary or guessed historical labels.

### Requirement: Archive viewer is distinguishable from review work
A running saved-review viewer SHALL use a localized viewing-archive status and a Close viewer action instead of presenting review-running and generic Cancel labels. Viewer startup and shutdown SHALL likewise describe archive viewing. Failures and final process status SHALL remain observable.

#### Scenario: Close a read-only archive viewer
- **WHEN** a person opens a saved review and closes its viewer from Terminal Jobs
- **THEN** only the viewer stops, the original receipt and report files remain unchanged, and no new review starts.

#### Scenario: An ordinary job has an archive-like title
- **WHEN** an ordinary terminal job uses the same title as an archive viewer
- **THEN** it retains ordinary process status and cancellation controls; titles alone cannot identify archive viewing.

### Requirement: Usage-aligned job card
Terminal Jobs SHALL use the neighboring usage popup's text hierarchy, shared spacing tokens and inset separators, keeping title/status together and time/context compact. The fixed header and bounded scroll body SHALL remain usable.

#### Scenario: Selected worktree context
- **WHEN** This worktree is selected and a receipt's known original worktree matches the selected worktree identified in the header
- **THEN** the row omits that redundant worktree field, retains labelled branch/session and full-path access; broader scopes, differing origins and unknown legacy context remain explicit.

#### Scenario: Popup interaction
- **WHEN** a person changes scope, scrolls a long history or dismisses with Escape in English or Russian
- **THEN** all existing navigation, viewport limits, recovery actions and focus restoration remain intact.
