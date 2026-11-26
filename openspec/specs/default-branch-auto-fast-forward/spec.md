# default-branch-auto-fast-forward Specification

## Purpose
Keep each opened repository's default branch (main/master) current with its
upstream without a click, by a bounded background fetch and a
fast-forward-only update that leaves the branch alone whenever it is not
safe, silently, under one Settings toggle.

## Requirements

### Requirement: Setting
Settings → Projects SHALL offer a toggle titled "Auto fast-forward default
branch" in English and localized in Russian, with a hint saying that the clean
default branch is fast-forwarded to its upstream after each background fetch.
The toggle SHALL be persisted in the settings file and SHALL be on when the
key is absent, including for an existing settings file written before this
change. While it is off, Guild SHALL start no background fetch and no
automatic fast-forward. Turning it on SHALL make every opened repository due
at once.

#### Scenario: Existing settings file without the key
- **WHEN** Guild starts with a settings file that has no auto fast-forward key
- **THEN** the toggle reads on and the background fetch runs for opened repositories

#### Scenario: Setting off starts no fetch
- **WHEN** the toggle is off and a repository is opened, and 5 minutes pass
- **THEN** no `fetch` and no fast-forward is run for it, and the Console shows no such command

#### Scenario: Turning the setting off mid-run
- **WHEN** the toggle is turned off while a background fetch is running
- **THEN** that fetch finishes or times out, no fast-forward follows it, and no further fetch is started

#### Scenario: Turning the setting off after the hand-off
- **WHEN** the fetch has finished and the fast-forward of the checked-out default branch is waiting behind other git work of that worktree, and the toggle is turned off
- **THEN** the fast-forward is not run, and nothing is written to the branch or the checkout

### Requirement: Which branch and which upstream
The automatic update SHALL apply only to the repository's default branch as
Guild already resolves it for new worktrees: the local branch named by
`origin/HEAD` when it exists locally, otherwise a local `main`, otherwise a
local `master`. Its upstream SHALL be that local branch's configured upstream,
and only when the upstream is a remote-tracking branch of a configured remote.
Feature and worktree branches SHALL NOT be updated automatically.

#### Scenario: No default branch
- **WHEN** a repository has no `origin/HEAD` local branch, no `main` and no `master`
- **THEN** nothing is fetched or written for it

#### Scenario: Default branch without an upstream
- **WHEN** the default branch has no configured upstream, or its upstream is a local branch
- **THEN** nothing is fetched or written and the reason "no upstream" is kept

#### Scenario: Feature branch behind its upstream
- **WHEN** a feature branch checked out in a worktree is behind its upstream
- **THEN** it is not fetched for or changed by the automatic update

### Requirement: When it runs
For each repository with at least one opened root, Guild SHALL run the
automatic update once when the repository's first root is opened (including
roots restored at launch) and then again 5 minutes after the previous run
finished, while the window is open, whether or not it is focused. At most one
run per repository SHALL be in flight. The fetch SHALL be the same bounded
fetch as the manual Update branch: only the upstream branch into its
remote-tracking ref, with no tags, no pruning and no submodules, through the
configured network proxy policy, under a 120-second deadline, and never on
the window's thread. Closing the repository's last root SHALL stop scheduling
it; a run already in flight SHALL NOT write after that, and SHALL still count
as the repository's run in flight — for the one-run-per-repository rule, the
window-wide limit and the manual Update branch rule — until it has finished.
The fetch SHALL write no ref but the upstream's remote-tracking ref, whatever
other fetch refspecs the remote has configured.

#### Scenario: Project opened
- **WHEN** a repository's root is opened with the setting on
- **THEN** one background fetch of its default branch's upstream starts without a click

#### Scenario: Every five minutes
- **WHEN** a repository stays open
- **THEN** a new run starts 5 minutes after the previous one finished, and never while one is in flight

#### Scenario: Closed and reopened during a run
- **WHEN** a repository's last root is closed while its background fetch runs, and the repository is opened again before that fetch has finished
- **THEN** no second fetch and no manual update of it runs alongside the first; the first writes nothing; a new run starts only after the first has finished

#### Scenario: Extra configured fetch refspec
- **WHEN** the remote's configured fetch refspecs include an extra forced mapping such as `+refs/heads/main:refs/heads/feature`
- **THEN** the background fetch moves only `refs/remotes/origin/main`, and `feature` keeps its commit

#### Scenario: Symbolic tracking ref
- **WHEN** the upstream's remote-tracking ref is a symbolic ref, for example `refs/remotes/origin/main` pointing at `refs/heads/feature`
- **THEN** nothing is fetched or written, `feature` keeps its commit, and the reason "symbolic tracking ref" is kept

#### Scenario: Symbolic default branch
- **WHEN** the default branch is itself a symbolic ref, for example `refs/heads/main` pointing at `refs/heads/feature`
- **THEN** nothing is fetched or written, `feature`, `HEAD`, the index and the files are unchanged, and the reason "symbolic ref" is kept

#### Scenario: Two roots of one repository
- **WHEN** the primary checkout and a linked worktree of the same repository are both opened
- **THEN** a single run serves the repository, not one per root

### Requirement: Fast-forward only
After a successful fetch the default branch SHALL be moved to the fetched
upstream commit only when its current commit is an ancestor of that commit.
When the two are equal, nothing SHALL be written. When the default branch has
commits its upstream lacks (ahead or diverged), the branch SHALL be left
alone. No merge commit, squash, rebase, reset, stash, force, `+` refspec or
destructive spelling SHALL be used. A failed or timed-out fetch SHALL NOT be
followed by any write. These guarantees SHALL hold whatever the person's git
configuration says — in particular `fetch.showForcedUpdates`,
`branch.<name>.mergeOptions`, `merge.ff` and `pull.twohead` SHALL NOT turn
the update into anything but a fast-forward.

#### Scenario: Clean main behind, checked out
- **WHEN** the primary checkout is on `main`, clean, and `origin/main` has new commits that descend from it
- **THEN** after the fetch `main` and the checkout's HEAD are at the fetched `origin/main` commit, no merge commit exists, and the Console shows the fetch and the fast-forward

#### Scenario: Diverged
- **WHEN** local `main` has a commit that `origin/main` does not, and `origin/main` has new commits too
- **THEN** after the fetch `main` is unchanged and the reason "diverged" is kept

#### Scenario: Merge options configured for the branch
- **WHEN** the checkout on `main` is clean and behind, and `branch.main.mergeOptions` is `--squash`, `--no-commit`, `--no-ff` or `-s ours`, or `pull.twohead` is `ours`
- **THEN** after the run HEAD is at the fetched `origin/main` commit with one parent chain (no merge commit), and the index and worktree are clean

#### Scenario: Ahead only
- **WHEN** local `main` has commits `origin/main` lacks and nothing new upstream
- **THEN** `main` is unchanged and the reason "diverged" is kept

#### Scenario: Already up to date
- **WHEN** the fetch brings nothing new for `main`
- **THEN** no fast-forward command is run and no reason is kept

#### Scenario: Fetch fails
- **WHEN** the remote is unreachable or refuses the fetch
- **THEN** no fast-forward command is run, `main` is unchanged, and the reason "fetch failed" is kept with git's words in the Console

#### Scenario: Fetch times out
- **WHEN** the fetch does not finish within its deadline
- **THEN** its process group is stopped, no fast-forward command is run, and the reason "fetch failed" is kept

### Requirement: Checked-out default branch is updated only when clean
When the default branch is checked out in a worktree, the fast-forward SHALL
run in that worktree only if, immediately before it, the worktree is still on
that branch at the commit read before the fetch, has no index, worktree or
untracked changes, has no merge, rebase, cherry-pick, revert or sequencer in
progress, and no Guild tab of that worktree holds unsaved changes. It SHALL
be serialized with every other git write Guild makes in that worktree.

#### Scenario: Dirty checkout
- **WHEN** the primary checkout is on `main` with a modified tracked file (or an untracked file, or a staged change) and `origin/main` moved
- **THEN** after the fetch `main` and the files are unchanged and the reason "uncommitted changes" is kept

#### Scenario: Unsaved buffer
- **WHEN** a file of the checkout on `main` has unsaved edits in a Guild tab and `origin/main` moved
- **THEN** `main` is unchanged and the reason "unsaved files" is kept

#### Scenario: Operation in progress
- **WHEN** the checkout on `main` is in the middle of a merge or rebase
- **THEN** `main` is unchanged and the reason "operation in progress" is kept

#### Scenario: Checkout changed during the fetch
- **WHEN** the checkout switches branch or commits on `main` while the fetch runs
- **THEN** no fast-forward is run and the reason "branch changed" is kept

#### Scenario: Checkout opened during the fetch
- **WHEN** `main` is checked out in a worktree no root has open when the run starts, and that worktree is opened in Guild while the fetch runs
- **THEN** the fast-forward is run by that root's git work, after its unsaved buffers are asked, and not from the list of roots the run started with

#### Scenario: Root closed after the hand-off
- **WHEN** the root that holds the checked-out `main` is closed after the run handed it the fast-forward
- **THEN** the run still finishes — written or not — and frees its place, so the repository runs again when reopened and a manual Update branch there is not held

#### Scenario: Nested checkout
- **WHEN** the only open root is a checkout nested inside the default checkout's folder (a linked worktree, a repository or a submodule there)
- **THEN** that root is not taken for the default checkout: the default checkout is written as an unopened one, and the nested checkout is untouched

#### Scenario: Last root closed before the handed-off write
- **WHEN** the last root of the repository closes after the hand-off but before its git work reaches the write
- **THEN** the write is cancelled at once, nothing is merged, and the run frees its place when the git work answers

#### Scenario: Merge whose root closed
- **WHEN** the root that ran the fast-forward closes before the window has heard the merge
- **THEN** the merge still appears in the Console, under the run's root

#### Scenario: Checked out in an unopened worktree
- **WHEN** `main` is checked out in a linked worktree that is not opened in Guild, that worktree is clean, and `origin/main` moved
- **THEN** that worktree's `main` is fast-forwarded under the same rules, without unsaved-buffer checks since Guild holds none there, and the Console names that worktree as the directory of the write

### Requirement: Default branch checked out nowhere
When the default branch is checked out in no worktree, its ref SHALL be moved
only through a command that git itself refuses for a non-fast-forward and for
a branch checked out in any worktree.

#### Scenario: Ref fast-forwarded
- **WHEN** the primary checkout is on a feature branch, no worktree has `main`, and `origin/main` has new commits descending from `main`
- **THEN** after the fetch `refs/heads/main` equals the fetched `origin/main` commit and no checkout's files changed

#### Scenario: Local commit during the fetch, forced updates hidden
- **WHEN** `main` is checked out nowhere at A, `fetch.showForcedUpdates` is `false`, and while the background fetch runs a commit C is added to `main`, and the fetched upstream B descends from A but not from C
- **THEN** `main` stays at C, and the reason "branch changed" (or git's non-fast-forward refusal, in the Console) is what is recorded

#### Scenario: Branch checked out between read and write
- **WHEN** `main` becomes checked out somewhere after Guild read that it was not
- **THEN** git refuses the ref update, `main` is unchanged, and the refusal is in the Console

### Requirement: Never overlapping a manual Update branch
An automatic run SHALL NOT start for a repository while a manual Update branch
of any of its roots is in flight; it SHALL start on the first tick after that
update finishes. A manual Update branch requested for a root of a repository
whose automatic run is in flight SHALL be held, shown as running, and carried
out once the automatic run has finished, from the branch's state at that
moment — not refused and not run alongside.

#### Scenario: Manual update running when the timer is due
- **WHEN** a manual Update branch is running in a root of the repository and the 5 minutes elapse
- **THEN** no background fetch starts until the manual update has finished

#### Scenario: Manual update outliving its root
- **WHEN** a manual Update branch is still fetching when its last root closes, and the repository is opened again
- **THEN** no automatic run starts until that manual update has finished

#### Scenario: Click during an automatic run
- **WHEN** a person clicks Update branch while that repository's automatic run is fetching
- **THEN** the button shows running, no second fetch starts alongside, and the manual update runs after the automatic run finishes, without a "branch changed" refusal caused by the automatic fast-forward

### Requirement: Silent outcomes
An automatic run SHALL raise no notice, toast, sound or badge, on success or
skip. Every write it runs — the fetch and the fast-forward — SHALL appear in
the Console with its directory, argv and output, success or failure. Reads
SHALL NOT appear there. The latest skip reason SHALL be kept per repository
in memory only, cleared by the next successful or up-to-date run, and shown
as one extra line naming the default branch and the reason: in the Update
branch hover hint of a root on the default branch, and in the Update branch
source popup's details of every root of that repository. The automatic run
SHALL NOT write or change the manual Update branch's persisted receipts or
source choice.

#### Scenario: Success is silent
- **WHEN** a background run fast-forwards `main`
- **THEN** no notice appears, and the Console shows the fetch and the fast-forward

#### Scenario: Skip reason in the source popup
- **WHEN** the last run left `main` alone because the checkout was dirty, and the person opens the Update branch source popup in any root of that repository
- **THEN** its details include a line saying the automatic fast-forward of `main` was skipped because of uncommitted changes

#### Scenario: Reason cleared
- **WHEN** a later run fast-forwards `main` or finds it up to date
- **THEN** the skip line no longer appears

#### Scenario: Restart
- **WHEN** Guild restarts
- **THEN** no skip reason is shown until the first run after opening has finished
