# Task Session Provenance

## Purpose

Describe the implemented Guild command boundary and visible task session handoff without promising a workflow scheduler.

## Requirements

### Requirement: Visible task creation

Guild SHALL accept a task file and a supported harness when creating a worktree task and SHALL preserve originating caller identity.

#### Scenario: Initiator creates a task

- **WHEN** an eligible initiating session requests a task worktree
- **THEN** Guild returns actual worktree/session outcomes and opens a visible terminal session.

### Requirement: No nested task bypass

Guild SHALL refuse task callers creating another task or steering another session.

#### Scenario: Builder attempts delegation

- **WHEN** a task session requests another prompted worktree/session
- **THEN** the request is refused without stripping caller identity.

### Requirement: Observable waiting

Session status SHALL distinguish waiting or missing telemetry from completion.

#### Scenario: Permission prompt

- **WHEN** a builder waits for permission
- **THEN** the initiator does not infer success or answer the prompt on the user’s behalf.

### Requirement: Window transport

The current CLI SHALL address its owning Guild process through its socket and SHALL report connection failures.

#### Scenario: Window unavailable

- **WHEN** the owning process is no longer reachable
- **THEN** commands fail visibly rather than claiming a task or review started.
