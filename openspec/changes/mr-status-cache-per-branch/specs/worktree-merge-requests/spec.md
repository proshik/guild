## Purpose

When a worktree's GitLab merge request is asked through glab, what an answer
may change, and what the column's rows and the repository's state show.

## ADDED Requirements

### Requirement: Last known MRs outlive a workspace switch
A repository that leaves the column SHALL keep its last known MRs in memory
and SHALL NOT write them to disk. While it is not in the column it SHALL NOT
be asked, a refresh out for it SHALL be stopped, and an answer that lands
after it left SHALL NOT publish. When it is shown again, its rows SHALL show
the last known MRs at once and it SHALL be asked again in the background.
Turning the setting off SHALL forget every MR.

#### Scenario: Back to a workspace
- **WHEN** a repository whose worktree showed !5 leaves the column and later returns
- **THEN** the row shows !5 before any new answer, and a refresh is started for it

#### Scenario: Answer after leaving
- **WHEN** a refresh is out, the repository leaves the column, and the answer lands
- **THEN** nothing of that answer is shown, on return or ever

#### Scenario: Off forgets the cache
- **WHEN** the setting is turned off and on again while a repository is hidden
- **THEN** none of its earlier MRs is shown on its return until a new answer lands

### Requirement: Each branch's answer stands on its own
A refresh's answer SHALL be kept per branch. A failure or timeout of one
branch SHALL NOT discard the answers of the other branches of the same host
and project. A branch whose refresh failed SHALL keep its last known MR with
the failure beside it; a branch never answered SHALL be unknown, not "no MR".

#### Scenario: One branch fails
- **WHEN** a project's batch reads branch `a` with !1 and branch `b` fails
- **THEN** `a`'s row shows !1 with no failure, and `b`'s row keeps its last known MR with the failure, or is unknown with it

#### Scenario: Deadline mid-batch
- **WHEN** the refresh's deadline passes after branch `a` was read and before `b`
- **THEN** `a` is published and `b` alone is timed out, without a glab call for it

### Requirement: Project-wide failures are the repository's state
A failure that concerns a whole host or project — glab missing, not logged in
or a 401, glab's own proxy over Guild's network setting, the network policy,
the refresh's deadline spent before the batch started, a host with an
explicit port — SHALL be kept as the repository's state and SHALL NOT be
shown on a worktree's row. The repository's state SHALL say whether a
refresh is queued or checking, when the last answer landed, and these
problems with their text.

#### Scenario: Not logged in
- **WHEN** glab is not logged in to the host
- **THEN** no row shows a reason, and the repository's state lists "glab is not logged in to <host>"

#### Scenario: A 401 keeps the badges
- **WHEN** a row showed !5 and the next refresh answers 401
- **THEN** the row still shows !5, and the repository's state lists the host as not logged in

### Requirement: The default branch is not asked
The worktree on the repository's default branch — the local branch
`origin/HEAD` names, else `main` or `master` — SHALL NOT be asked: no glab call
SHALL be made for it and its row SHALL show neither an MR nor a failure.

#### Scenario: Main and a feature branch
- **WHEN** a repository shows worktrees on `main` and on `feat`
- **THEN** glab is asked for `feat` only

### Requirement: A frame only for a visible change
A landed answer SHALL redraw the window only when what the rows or the
repository's state draw changed.

#### Scenario: The same answer twice
- **WHEN** a refresh brings exactly the MRs already shown
- **THEN** the window is not notified
