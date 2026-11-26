## ADDED Requirements

### Requirement: The repository header shows its merge requests' state
A repository's header in the column SHALL draw one indicator of its merge
requests' state, from the repository's state alone, before its `⋯` and `+`:
- while a refresh is out, *checking*, turning arrows;
- while one is due and not out, *queued*, a clock;
- otherwise, when the last answer could not ask something — glab missing, not
  logged in or a 401, a host with a port, a batch that failed as a whole —
  *couldn't refresh*, struck-through arrows in brass, whose hover names every
  such problem;
- otherwise, after an answer has landed, *checked*, a faint ✓ drawn only
  while the pointer is on the header, whose hover says how long ago;
- otherwise nothing.
Nothing SHALL be drawn while the setting is off, the repository has no
state or nothing left to ask, or the repository is off GitLab: every worktree resolved as pushing
elsewhere, or — before its first answer — none pushing to a host with GitLab
in its name. A problem of the repository SHALL NOT be drawn on a worktree's row.

#### Scenario: Not logged in
- **WHEN** the last answer for a repository lists the host as not logged in, and nothing is due
- **THEN** its header draws *couldn't refresh* with "glab is not logged in to <host>" in its hover, and no row says it

#### Scenario: Checked at rest
- **WHEN** the last answer had no problem and nothing is due
- **THEN** the header draws nothing until the pointer is on it, then a ✓ saying when it was checked

#### Scenario: Checking
- **WHEN** a refresh is out for the repository
- **THEN** its header draws the turning arrows, whatever the last answer was

#### Scenario: A repository on GitHub
- **WHEN** a repository's worktrees push to GitHub and its refresh resolves nothing to ask
- **THEN** its header draws nothing, before, during and after the refresh

#### Scenario: A change of state is drawn
- **WHEN** the indicator a repository would draw changes — a refresh starts, an answer lands with a new problem
- **THEN** the window is notified, and only then

### Requirement: The person's own merge request is marked
A worktree row's MR SHALL carry a faint person after its number, with the
hover "You created this merge request", when GitLab's `author.id` of the MR
equals the `id` of the account glab is logged in with on the MR's host. The
account SHALL be read once per host per refresh, only for a host whose batch
found an MR and that no 401 signed out in that refresh, through the same admission, deadline, output cap, scrubbed
environment and proxy as every other call, and the last account read for a
host SHALL be kept until another read replaces it. A failed or unreadable
account read SHALL NOT be a problem of the repository: the last account read
for the host SHALL still mark, and with none the MR SHALL be shown without
the mark.

#### Scenario: The author is the account
- **WHEN** a branch's MR has `author.id` 42 and `glab api --hostname <host> user` answers `id` 42
- **THEN** its row shows the person after `!<iid>`

#### Scenario: Someone else's MR
- **WHEN** the MR's author is 7 and the account is 42
- **THEN** its row shows the MR with no person

#### Scenario: The account read fails
- **WHEN** the MR is read, no account was read for its host before, and the account read times out or answers garbage
- **THEN** the row shows the MR without the person, and the header shows no problem for it

#### Scenario: A later account read fails
- **WHEN** account 42 was read for the host and the next refresh's account read fails
- **THEN** an MR by 42 keeps the person
