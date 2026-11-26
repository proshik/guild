# Review Session Launch

## Purpose

Describe requesting a review from an already open agent session, the compact
recorded review, and how finder stages are dispatched and skipped.

## Requirements

### Requirement: Request review in an existing agent session
Guild SHALL offer Run review in the current eligible session and a choice of
other open agent terminal sessions in the same checkout. A UI request SHALL
reuse shared terminal submission, preserve CLI provenance restrictions, capture
the source checkout before switching focus, and refuse busy, waiting, closed or
exited targets and duplicate submissions. No new interactive session is created.

#### Scenario: Start from the focused session
- **WHEN** a person presses Run review in an eligible current agent session
- **THEN** one instruction to review through Guild CLI is submitted to that session

#### Scenario: First review request in a fresh prompt
- **WHEN** a person requests review in a fresh agent session that accepts bracketed prompt submission and is neither working nor waiting
- **THEN** the UI can submit the first request without requiring a completed previous turn, while unknown status after input to the current process and duplicate pending requests remain refused

#### Scenario: Review after restoring a conversation
- **WHEN** Guild restarts and restores an existing conversation in a new terminal process that accepts bracketed submission before receiving any input
- **THEN** Run review can send the first request despite persisted previous turns and missing current-process telemetry
- **AND** working, waiting, exited and pending targets remain refused, and unknown status after input to that process remains refused

#### Scenario: Choose another harness
- **WHEN** a person chooses another eligible open agent session in the checkout
- **THEN** the instruction is submitted there and its actual harness remains unchanged

### Requirement: Compact recorded review remains readable
The initial compact observer SHALL show the standard four participant rows in full,
with profile/run identity and launch actions each occupying one row. Empty and
expanded observers SHALL retain their existing layout. Larger rosters or smaller
resized viewports SHALL scroll rather than paint over persistent controls.

#### Scenario: Inspect a recorded focused review
- **WHEN** a person opens compact observation of a four-participant focused review
- **THEN** all four rows, stage progress, snapshot metadata and launch actions are visible without overlapping

### Requirement: Bounded parallel finder dispatch
The native engine SHALL run independent finders concurrently up to max_parallel,
reserve every invocation against the shared task limit before dispatch, serialize
archive updates through the coordinator, and finish or cancel owned children
before returning. Retries SHALL count against the same limit.

#### Scenario: Four finders with a limit of two
- **WHEN** a four-finder profile runs with max_parallel equal to two
- **THEN** two finders may execute together and no third executes simultaneously

#### Scenario: Limited calls
- **WHEN** remaining task calls cannot cover pending finder invocations
- **THEN** no invocation beyond that limit is dispatched and acceptance is UNVERIFIED

### Requirement: Explicit stage skips
Launch options and pinned review defaults SHALL support no_synthesis and
no_verify. Without synthesis, findings SHALL retain source attribution. Without
verification, findings and scenario acceptance SHALL remain unverified and no
verifier is dispatched. Stage skips SHALL be recorded in the archive.

#### Scenario: Skip both later stages
- **WHEN** no_synthesis and no_verify are true
- **THEN** only finders run and the final acceptance is UNVERIFIED
