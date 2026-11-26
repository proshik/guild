# Review Lifecycle

## Purpose

Record the native fixed review pipeline, exact scope and durable state contracts; observation is optional.

## Requirements

### Requirement: Registration and immutable scope

Review SHALL register its baseline before task edits and freeze only explicitly selected task paths across committed, staged, unstaged and untracked layers.

#### Scenario: Ambiguous initial dirt

- **WHEN** a ready request includes a path dirty before registration
- **THEN** the engine refuses ambiguous ownership rather than reviewing unrelated changes.

### Requirement: Pinned configuration

Each task SHALL pin roles, profile, budget and effective configuration independently of later edits.

#### Scenario: Defaults edited

- **WHEN** project defaults change during a run
- **THEN** the existing task retains its pinned configuration.

### Requirement: Fixed orchestration

Review SHALL use find, synthesis and verification with every actual invocation counted against the task budget.

#### Scenario: Budget exhausted

- **WHEN** the next required invocation exceeds the remaining task budget
- **THEN** the engine records incomplete coverage without declaring PASS.

### Requirement: Idempotent submission

A repeated ready request identity SHALL NOT duplicate a paid invocation.

#### Scenario: Lost response

- **WHEN** the caller repeats the same task/request after losing the reply
- **THEN** the engine reconciles the existing request rather than dispatching a duplicate.

### Requirement: Honest outcomes

Review SHALL preserve completed, failed, cancelled, interrupted, waiting and degraded states separately from PASS, FAIL and UNVERIFIED acceptance.

#### Scenario: Interrupted process

- **WHEN** Guild restarts after uncertain execution
- **THEN** persisted process/output state is reconciled without inferring success from silence or blindly replaying calls.

### Requirement: Independent storage

Guild SHALL store review history in the main checkout archive and SHALL keep source identity and scope digests.

#### Scenario: Disposable worktree removed

- **WHEN** a reviewed source worktree no longer exists
- **THEN** the main checkout review archive remains available.

### Requirement: Read-only execution

Review executors SHALL enforce tool isolation; unsupported adapters SHALL NOT run merely because their terminal harness is installed.

#### Scenario: OpenCode review requested

- **WHEN** equivalent isolation has not been verified
- **THEN** the review executor refuses it while ordinary OpenCode task sessions remain a separate capability.

### Requirement: Optional observation

Hiding or closing a review surface SHALL NOT cancel the review.

#### Scenario: Observer closed

- **WHEN** the user closes activity while execution continues
- **THEN** the task progresses in the background and requires a distinct cancel action to stop.
