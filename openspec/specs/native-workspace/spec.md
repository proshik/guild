# Native Workspace

## Purpose

Preserve the native desktop, session and source-editing boundaries while review and task workflows evolve.

## Requirements

### Requirement: Native ownership

Guild SHALL use its native GPUI desktop and SHALL keep guild-session and guild-review independent of GPUI.

#### Scenario: Headless verification

- **WHEN** session or review domain tests run without a window
- **THEN** they can exercise domain behavior without linking the desktop.

### Requirement: Tabs and panes

Guild SHALL associate content with pane bodies and create tabs through the existing adoption path.

#### Scenario: Review beside a terminal

- **WHEN** a Review tab is opened and then closed
- **THEN** the terminal session remains reachable and closing the view does not cancel its review.

### Requirement: Existing project tools

Guild SHALL preserve project navigation, source editing, search, language services and guarded git operations while adding workflow features.

#### Scenario: Review alongside editing

- **WHEN** a person inspects review activity
- **THEN** the task source checkout and ordinary editor/git tools remain available.

### Requirement: Guarded writes

Git mutations SHALL use named native actions and retain their existing refusal and confirmation behavior. Worktree creation SHALL reject a trimmed branch name beginning with `-` before dispatching a Git write, with a concise error shared by UI and CLI callers.

#### Scenario: Agent worktree creation

- **WHEN** a CLI caller requests a worktree
- **THEN** the same native creation path and guards used by the UI apply.

#### Scenario: Option-like branch name

- **WHEN** UI or CLI worktree creation supplies `--help`, `-h`, `-topic` or whitespace surrounding such a name
- **THEN** Guild rejects it without invoking Git or creating a branch, folder or task session
- **AND** the error does not contain a Git manual.

#### Scenario: Ordinary branch names

- **WHEN** a caller supplies `topic`, `feature/topic` or `topic-name`
- **THEN** the name continues through the existing creation path and Git retains its own remaining validation.

#### Scenario: Existing guards

- **WHEN** the name is empty or the base reference begins with `-`
- **THEN** the existing refusal remains in force.

### Requirement: Available harness usage
Guild SHALL always display the titlebar harness usage chip, with `-` when no figure is available. Transient Keychain command failures SHALL remain retryable without manual Refresh; access denial SHALL NOT trigger repeated automatic permission prompts.

#### Scenario: No figures yet
- **WHEN** neither provider has supplied usage, including after an error
- **THEN** the chip remains visible and its usage card and Refresh are accessible.

#### Scenario: Temporary startup failure
- **WHEN** launching the Keychain reader fails temporarily
- **THEN** normal scheduled polling can recover without Refresh.

#### Scenario: Keychain access refused
- **WHEN** the Keychain command returns a failure status
- **THEN** automatic reads remain stopped until manual Refresh.

### Requirement: Codex session restoration

Guild SHALL preserve Codex's saved permission choice when resuming a conversation
and SHALL show local conversation content while the terminal restores history.

#### Scenario: Saved permission choice

- **WHEN** a Codex conversation with workspace or full access resumes after restart
- **THEN** Guild does not override its saved policy with a sandbox launch flag.

#### Scenario: History replay

- **WHEN** a restored Codex terminal has not replayed its conversation yet
- **THEN** its saved screen or exact local conversation is visible immediately
- **AND** the live screen replaces it after conversation text appears.

#### Scenario: Startup requires attention

- **WHEN** the process exits, the person interacts, or startup exceeds five seconds
- **THEN** the live terminal becomes visible so dialogs and errors remain reachable.

### Requirement: Visible background activity animation
Guild SHALL continue animating visible working indicators after application deactivation while retaining reduced-motion and hidden-surface repaint bounds. Existing steady inactive caret behavior SHALL remain unchanged.

#### Scenario: Another application takes focus
- **WHEN** Guild remains visible and a person clicks Chrome on another monitor
- **THEN** visible working worktree, project, workspace and session-menu activity indicators continue rotating smoothly.

#### Scenario: Hidden or inactive scopes
- **WHEN** an indicator is clipped, hidden, or belongs to a workspace not currently shown
- **THEN** it does not sustain animation frame requests.

#### Scenario: No work or reduced motion
- **WHEN** work stops or reduced motion is enabled
- **THEN** activity indicators do not sustain rotation frame requests.

#### Scenario: Return to Guild
- **WHEN** application focus returns
- **THEN** activity remains correct without duplicate animation clocks or altered status/attention counts.

#### Scenario: Related visual activity
- **WHEN** loading or progress animation is visible while Guild is inactive
- **THEN** application-focus gating alone does not freeze it.

### Requirement: Navigation history spans tabs, worktrees and workspaces

Guild SHALL keep one window-wide, in-memory Back/Forward history of what was in front — a place in a project file, any other tab body (harness or terminal session, Diff, Annotate, Settings, Review) and a seat showing no tab — record every switch of tab, pane, worktree and workspace as a movement, step across worktree and workspace boundaries, and skip entries whose target no longer exists.

#### Scenario: Back from a terminal returns to the file

- **WHEN** a person stands at line 120 of a project file and then brings a terminal or harness tab forward
- **AND** presses Back (title-bar arrow, `cmd-[` or `cmd-alt-←`) with the terminal focused
- **THEN** the file's tab is in front with the caret at line 120
- **AND** Forward brings the same terminal tab back.

#### Scenario: Back between sessions

- **WHEN** a person switches from harness session A to terminal session B by tab click, `ctrl-tab` or the column
- **AND** presses Back
- **THEN** session A's tab is in front and focused.

#### Scenario: Back across a worktree

- **WHEN** a person moves from a tab of one worktree to another worktree's row in the column
- **AND** presses Back
- **THEN** the first worktree is the active project again with the tab it showed in front.

#### Scenario: Back and Forward across a workspace

- **WHEN** a person switches workspace by mark, keyboard or swipe, including to a workspace that shows no tab
- **AND** presses Back, then Forward
- **THEN** Back selects the previous workspace with the tab it showed in front
- **AND** Forward returns to the second workspace and its front, leaving the history as it was before Back.

#### Scenario: Back lands on panes and empty places with the keyboard

- **WHEN** a person moves from an empty pane, or from a Diff or Settings pane, to a terminal in the neighbouring visible pane of the same seat
- **AND** presses Back
- **THEN** the previous pane is focused and takes the keyboard, and the terminal no longer receives keys.

#### Scenario: Every switch is a step

- **WHEN** several switches happen within one update — two workspace selections from `guild` handled in one batch, or two IDE proposals for the same file opened one after the other
- **THEN** each is its own step, and Back walks them in reverse order.

#### Scenario: Back after a jump into an unopened file

- **WHEN** a person jumps (`f12`, a search hit) into a file whose restored tab has not been drawn yet, and presses Back once
- **THEN** they return to where the jump was made, not to the top of the jumped-to file.

#### Scenario: Closed targets are skipped

- **WHEN** the previous entry is a session tab that has since been closed, a closed project, a removed worktree, a deleted workspace, or an empty pane that has since been filled or closed
- **THEN** Back skips it silently and lands on the nearest live entry
- **AND** a project file whose tab was closed still reopens by path at its place.

#### Scenario: Arrows reflect live entries only

- **WHEN** every entry behind the person is dead or is the place already in front
- **THEN** the Back arrow is drawn faint and pressing it changes nothing.

#### Scenario: File-only readers stay file-only

- **WHEN** a person opens Recent Locations, uses Last Edit Location or filters Recent Files to edited files
- **THEN** they list or walk only places in the active project's files, never sessions, other tabs, other worktrees or other workspaces
- **AND** Back after Last Edit Location returns to where it was pressed.

#### Scenario: History is not persisted

- **WHEN** Guild restarts
- **THEN** Back and Forward start empty.
