# sessions-column Specification

## Purpose
The column can list the agent sessions of a workspace by who needs the person
first, and the status bar and its popup say the same thing for the whole window.

## Requirements

### Requirement: Column mode button
The title bar SHALL show one square icon button at the column's right edge, in
the row of the traffic lights and history arrows, that switches the column
between Projects and Sessions. The button SHALL show the mode's icon and look
pressed in Sessions mode. It SHALL be hidden while the column is hidden. The
`Toggle Session List` action SHALL switch the same mode. The mode SHALL persist
across launches. The column heading SHALL read "Projects" or "Sessions".

#### Scenario: Switching and back
- **WHEN** the person clicks the mode button in Projects mode
- **THEN** the column lists sessions, the heading reads "Sessions" and the button looks pressed
- **WHEN** the person clicks it again
- **THEN** the column lists projects and the heading reads "Projects"

#### Scenario: Keyboard
- **WHEN** the `Toggle Session List` action runs
- **THEN** the column mode switches exactly as a click does

#### Scenario: Hidden column
- **WHEN** the column is hidden
- **THEN** the mode button is not drawn

#### Scenario: Remembered
- **WHEN** the window is restarted in Sessions mode
- **THEN** the column opens in Sessions mode

### Requirement: Sessions by urgency
In Sessions mode the column SHALL list the open agent sessions of the shown
workspace in sections Waiting for you (waiting or failed), Working, Done (an
unread finished turn) and Idle, in that order, each newest activity first,
omitting empty sections. A row SHALL show the tab's state mark, the project's
tile and name, the time since last activity, the session's name, its worktree
branch (starred for the primary checkout) and lines added and removed when
non-zero, and the agent's logo. The session in front SHALL be marked as the
selected worktree row is.

#### Scenario: Order
- **WHEN** one session waits, one failed, two work and one finished unread
- **THEN** the waiting and failed ones are under Waiting for you, the two working under Working, the finished one under Done, each section newest first

#### Scenario: Last activity survives a restart
- **WHEN** a session reported activity and the window restarts
- **THEN** its row shows the age of that activity, not "—"

### Requirement: Settled
Sessions mode SHALL end with a collapsed Settled section counting open agent
tabs idle for longer than the configured number of days (default 3) and agent
sessions of the workspace's folders closed within that period, as the harness's
own history reports them. Expanded, an open one SHALL open its tab and a closed
one SHALL continue its conversation through the start screen's resume path. A
closed session without a name SHALL be labelled by the harness history's title.

#### Scenario: An old idle tab
- **WHEN** an open idle tab's last activity is four days old with the default threshold
- **THEN** it is counted under Settled, not Idle

#### Scenario: Continue a closed session
- **WHEN** the person activates a closed session in Settled
- **THEN** a tab resumes that conversation in its project

#### Scenario: Threshold setting
- **WHEN** the person sets "Settle sessions after" to 1 day
- **THEN** idle tabs older than one day and sessions closed within one day are Settled

### Requirement: Navigation in Sessions mode
Sessions mode SHALL support the column's speed search over session and project
names, ↑/↓ to move the selection, Enter to open the selected session, a context
menu on a row (Open, Rename Tab…, Show in Projects, Close Tab; Continue and Show
in Projects for a closed one), and the heading's filter menu by agent, state and
project with Show All.

#### Scenario: Keys
- **WHEN** the column has the keys and the person presses ↓ then Enter
- **THEN** the next session's tab comes to the front

#### Scenario: Show in Projects
- **WHEN** the person chooses Show in Projects on a row
- **THEN** the column switches to Projects mode with that session's worktree selected

#### Scenario: Filter
- **WHEN** the person unticks Codex in the filter menu
- **THEN** Codex sessions are not listed until Codex is ticked again or Show All is chosen

### Requirement: Status bar counters
The status bar's session control SHALL show, for all open agents together, a
failure mark and count only when a session failed, then working, waiting and
done counts, and last an idle count in the idle mark's muted colour, each hidden
at zero, and nothing at all when no agent is open.
Its look SHALL not depend on the column mode. Hovering SHALL show a per-agent
breakdown. A click SHALL open the sessions popup.

#### Scenario: Counts
- **WHEN** three agents work, one waits and one finished unread
- **THEN** the control reads ◐ 3, ? 1, ● 1 and no failure mark

#### Scenario: All idle
- **WHEN** four agents are open and none has reported a state
- **THEN** the control reads ○ 4

#### Scenario: Nothing open
- **WHEN** no agent tab is open
- **THEN** the status bar shows no sessions control

### Requirement: Sessions popup
The popup SHALL list every open agent session in one scrolling column with no
section tabs or headings, ordered waiting and failed, working, done, idle, newest
first within each. Each row SHALL show the avatar with the agent's logo, the
name, "project · branch", and on the right the state mark with a word of the same
colour (Waiting, Error, Working, Done, Idle) over the time. ↑/↓, Enter and Esc
SHALL work and their hint SHALL stay at the foot. The popup's left edge SHALL
line up with its control's.

#### Scenario: Open from the popup
- **WHEN** the person presses ↓ and Enter in the popup
- **THEN** the selected session's tab comes to the front and the popup closes
