# session-launch-mode Specification

## Purpose
How an agent session is launched in the window: Claude Code, Codex and
OpenCode sessions run in a terminal, and state saved by earlier builds that
offered a stream mode keeps loading.

## Requirements

### Requirement: Agent sessions run only in a terminal
The window SHALL launch every Claude Code, Codex and OpenCode session as the
harness's own CLI in a terminal tab. It SHALL NOT offer, render or start a
structured stream-mode session, and no surface SHALL let a person choose a
session mode.

#### Scenario: A new Claude Code session from any entry point
- **WHEN** a person opens a Claude Code session from the strip's +, its
  dropdown, the start screen or a window command, or an agent runs
  `guild session new`
- **THEN** the tab runs `claude` in a terminal with Guild's hooks and IDE
  connection, as a terminal session does today

#### Scenario: Holding ⌘ on the start screen
- **WHEN** a person holds ⌘ while choosing a start-screen card or a row to
  open or resume a Claude Code conversation
- **THEN** the session opens in a terminal, the same as without ⌘
- **AND** the start screen shows no mode word and no hint about holding ⌘ to
  change the mode

#### Scenario: Settings offer no session mode
- **WHEN** a person opens Settings → Sessions
- **THEN** there is no "Claude Code mode" row, and the Permissions hint does
  not describe stream-mode behaviour

### Requirement: Stream-mode state saved earlier still loads
The window SHALL load session records and settings written by builds that
had stream mode, without discarding them, and SHALL treat every saved
stream-mode Claude session as a terminal Claude session with the same
conversation. It SHALL NOT write a stream mode to new state.

#### Scenario: A saved stream session with a conversation is restored at startup
- **WHEN** the session list contains a Claude Code record saved in stream
  mode that was open as a tab, and its conversation transcript exists
- **THEN** the window starts, the tab is restored as a terminal session
  resuming the same conversation id, and the other records are intact
- **AND** the record is written back as a terminal session the next time the
  list is saved

#### Scenario: A saved stream session that never had a conversation
- **WHEN** the session list contains an open Claude Code record saved in
  stream mode whose conversation transcript does not exist
- **THEN** the window treats it exactly as it treats a terminal record in the
  same situation today: a new conversation under that id when the id is free,
  or the record is forgotten when the id is taken without a conversation

#### Scenario: A closed stream session is reopened
- **WHEN** a person reopens or resumes a closed Claude Code conversation whose
  record was saved in stream mode and whose transcript exists
- **THEN** it opens in a terminal with the same conversation

#### Scenario: A former stream session's spend is counted once
- **WHEN** a record saved in stream mode carries a saved cost, and after it
  reopens as a terminal its transcript reports a cumulative cost
- **THEN** the Sessions list and the window's total show the transcript's
  cost for that session, not the sum of the two
- **AND** a record whose transcript reports no cost still shows its saved cost

#### Scenario: Settings saved with stream as the default mode
- **WHEN** the settings file contains `"default_mode": "stream"` beside other
  settings
- **THEN** the settings load without falling back to defaults, every other
  setting is kept, and new Claude Code sessions open in a terminal
- **AND** the settings file written afterwards does not contain a session mode
