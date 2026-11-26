# terminal-job-context Specification

## Purpose
Make terminal launch history follow the person's current session and worktree while retaining deliberate access to wider history and honest original launch provenance.

## Requirements

### Requirement: Contextual history scope
Terminal Jobs SHALL offer This session, This worktree, This project and All launches scopes. This project SHALL include worktrees of the same repository. Scope membership SHALL use original root/repository and immutable resolved conversation identity, independent of process-owner rebinding, current output presentation and branch name.

#### Scenario: Session default and widening
- **WHEN** a person opens history from a focused session that launched revdiff while other sessions/projects launched revmux
- **THEN** This session is selected and only that session's jobs appear; selecting wider scopes exposes only their matching jobs, and All launches includes all.

#### Scenario: Worktree and repository separation
- **WHEN** one repository has two worktrees, another repository has identically named roots, and each has jobs
- **THEN** This worktree excludes the sibling root, This project includes both roots of the same repository and excludes the other repository, without comparing display names or branch names.

#### Scenario: Reattachment and resumed session
- **WHEN** process A launches in conversation A, resumes conversation B and launches again, then output is reattached or those conversations close/reopen before restart
- **THEN** the first receipt belongs only to conversation A and the second only to B; both retain original process owner A for existing authorization, and mutable resume bindings cannot rewrite membership.

#### Scenario: No session or project
- **WHEN** history opens from a file/empty pane with a selected root, or without a selected root
- **THEN** its default is This worktree or All launches respectively, and unavailable scopes are disabled rather than silently widening.

#### Scenario: Empty scoped history
- **WHEN** the selected scope has no matching jobs
- **THEN** the list explains the empty scope and offers explicit access to wider history without changing scope automatically.

#### Scenario: Launch before repository discovery
- **WHEN** a launch is reserved before the original root receives its first asynchronous repository answer and the worker later identifies a repository shared with another worktree
- **THEN** This project includes the launch through its extant exact original ProjectId, without rewriting saved display labels or branch; until discovery only exact-root membership is known, and missing identity is not treated as confirmed non-git.

### Requirement: Immutable honest launch context
New launches SHALL retain sanitized original worktree/repository context, immutable resolved conversation identity and label, program working directory and the last original-worktree branch observation available at reservation. The UI SHALL explicitly describe branch information as last observed rather than freshly verified HEAD. The window SHALL resolve trusted ownership context. Retries, presentation changes, renames, branch switches and restarts SHALL NOT rewrite that snapshot; original argv and environment SHALL NOT be persisted as history metadata.

#### Scenario: Different cwd and later branch change
- **WHEN** a program starts with a cwd different from its originating worktree and the worktree later changes branches
- **THEN** history preserves the two distinct paths and the original last-observed branch information, without attributing the worktree branch to the program's different cwd.

#### Scenario: External branch switch before worker refresh
- **WHEN** an external command switches branch A to B immediately before a launch, while the asynchronous worker still reports A
- **THEN** the saved A value is explicitly presented as last observed, not freshly verified branch-at-launch; unavailable observations remain unknown and the distinct program cwd is not assigned that branch.

#### Scenario: Duplicate launch request
- **WHEN** an identical request ID is retried after a label or branch changes
- **THEN** no duplicate process/history record is created and original metadata/time remain unchanged.

#### Scenario: Legacy restart and deleted owner
- **WHEN** receipts lack new metadata or original session/root records have been removed before restart
- **THEN** old receipts load without relaunch, known original IDs still support literal exact matching without guessing conversation membership through today's resume binding, unknown labels/branch/context are explicit, and All launches retains recovery without invented historical data.

### Requirement: Scope is presentation only
Selecting a scope SHALL NOT start, restart, cancel, modify or reassign a terminal job. Existing show/hide/cancel/delete-output and archive-view rules SHALL remain in force.

#### Scenario: Broader history recovery
- **WHEN** the person selects All launches and shows another context's retained output or archived review
- **THEN** the existing recovery action runs under its existing guards and original job ownership/history remain unchanged.
