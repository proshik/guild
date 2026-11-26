# agent-turn-chime Specification

## Purpose
Guild tells the person by sound that an agent session finished a turn they have
not seen, for every harness Guild runs, so a person working elsewhere learns
about it without watching the window.

## Requirements

### Requirement: A finished turn nobody is looking at chimes

When an agent session's status becomes *done* — its turn ended and the person
has not seen it — Guild SHALL play the macOS system sound "Glass" once. This
SHALL hold for Claude Code, Codex and OpenCode sessions in terminal tabs and for
stream sessions, whichever witness reported the end of the turn (a hook, the
Codex rollout, OpenCode's plugin or a stream result).

#### Scenario: Codex finishes while the person is in another application
- **WHEN** a Codex session in a terminal tab completes a turn and the Guild
  window is not focused
- **THEN** Guild plays the Glass sound once

#### Scenario: OpenCode finishes in a tab that is not in front
- **WHEN** an OpenCode session reports `idle` after a prompt, and its tab is not
  the tab in front of any area of the focused window
- **THEN** Guild plays the Glass sound once

#### Scenario: Claude Code finishes in a background tab
- **WHEN** a Claude Code session's `Stop` hook arrives for a tab that is not in
  front
- **THEN** Guild plays the Glass sound once, at the end of the turn rather than
  a minute later

#### Scenario: A stream session finishes unseen
- **WHEN** a stream session's turn ends with a result while its tab is not in
  front of a focused window
- **THEN** Guild plays the Glass sound once

### Requirement: No chime for what the person is already looking at

A session whose turn ends while its tab is in front in an area of the focused
Guild window SHALL NOT chime: it is seen in the same pass. A Guild window that
is not focused SHALL NOT count as looking, even for the tab in front.

#### Scenario: The tab in front of the focused window finishes
- **WHEN** the agent in the tab in front of the focused Guild window finishes
  its turn
- **THEN** no sound is played

#### Scenario: The tab in front of an unfocused window finishes
- **WHEN** the agent in the tab in front finishes while the Guild window is not
  focused
- **THEN** Guild plays the Glass sound once

### Requirement: One finished turn, one chime

Guild SHALL chime only on the change into *done*. A session that stays *done*
SHALL NOT chime again, and several sessions becoming *done* in the same pass
SHALL produce a single sound.

#### Scenario: Two agents finish together
- **WHEN** two unseen sessions become *done* in the same status pass
- **THEN** Guild plays the Glass sound once

#### Scenario: A late signal for a turn already done
- **WHEN** a session is already *done* and another completion signal for the
  same turn arrives (an asynchronous `PostToolUse`, a Codex rollout confirming
  the hook)
- **THEN** no further sound is played

#### Scenario: A repeated completion after the person looked
- **WHEN** an unseen session finishes and chimes, the person then looks at its
  tab, and a delayed second completion signal for that same turn arrives after
  they switched away
- **THEN** no further sound is played

#### Scenario: The next turn finishes
- **WHEN** a *done* session receives a new prompt and that turn also ends unseen
- **THEN** Guild plays the Glass sound again

### Requirement: Only a finished turn chimes

Guild SHALL NOT play the sound when a session starts waiting for permission or
for an answer, fails, is interrupted, ends, or is restored on startup.

#### Scenario: A permission prompt
- **WHEN** an unseen session starts waiting for permission or asks a question
- **THEN** no sound is played

#### Scenario: Failure and interrupt
- **WHEN** an unseen session reports an execution failure, or the person
  interrupts it with Esc or Ctrl-C
- **THEN** no sound is played

#### Scenario: Reopening a window
- **WHEN** Guild starts and restores sessions whose last turns ended before it
  started
- **THEN** no sound is played
