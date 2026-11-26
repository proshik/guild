## Why

A worktree's GitLab MR badge (GLD-063, decision 56) is lost and re-asked
every time a workspace is switched away and back, one failed branch blanks
the answers of every other branch of the same project, the default branch is
asked although it never has an MR of its own, and every landed answer redraws
the whole column even when nothing on it changed — the jolt seen when badges
appear on a large column.

## What Changes

- A repository that leaves the column keeps its last known MRs in memory
  (never on disk). It is still not asked while hidden and its refresh is still
  stopped; an answer that lands after it left still publishes nothing. When it
  comes back, its rows show the last known badges at once and it is asked
  again in the background. Turning the setting off still forgets everything.
- A refresh's answer is kept **per branch**: one branch's failure or timeout
  no longer discards the other branches' answers of the same host and
  project, and a branch whose refresh failed keeps its own last known badge
  with the failure beside it. Whole-or-nothing now holds per branch.
- Failures that concern a whole host or project — glab missing, not logged
  in, a 401, glab's own proxy, the network policy, the refresh's deadline
  spent before the batch started, a host with a port — are the
  **repository's state**, not a row's: rows no longer show them. The model
  exposes this state (queued / checking / checked / problems) for the
  repository header indicator (task B), which draws it.
- **BREAKING (visible)**: a worktree row no longer explains a missing badge
  with "glab not found" / "glab is not logged in to …" / "glab cannot address
  a host with a port"; until task B lands, these reasons are held in the model
  only.
- The repository's default branch — `git::main_branch`'s rule, the branch
  `origin/HEAD` names, else `main`/`master` — is not asked: no glab call, no
  badge and no failure on its row.
- A landed answer redraws the window only when what is drawn changed: the
  rows' badges and failures, and the repository's state.

## Capabilities

### New Capabilities

- `worktree-merge-requests`: when a worktree's MR is asked, what an answer
  may change, and what the rows and the repository state show.

### Modified Capabilities

None — GLD-063 has no baseline spec yet; decision 56 is its record and is
updated with this change.

## Non-goals

- No new visible surface: the repository header indicator and "You created
  this MR" are task B, designed in Penpot first.
- No disk cache, no new setting, no retry policy.
- Updating only the affected column elements (instead of a root notify) is
  only measured and proposed, not done, without the user's agreement.

## Impact

- `crates/session/src/merge_requests/glab.rs` — `read_until` answers per
  branch; host- and project-wide failures stay the read's error.
- `crates/desktop/src/forge.rs` — cache on hide, per-branch failures,
  repository state API, visible-change comparison.
- `crates/desktop/src/root/forge.rs` — the refresh's batches, the notify.
- `crates/desktop/src/git/worktrees.rs`, `root/git.rs` — the default
  branch marked on the worktree snapshot, off the window's thread.
- `crates/desktop/src/view/column.rs` — the row's reason arm goes.
- `docs/decisions.md` decision 56, `CHANGELOG.md`.
- Authorization unchanged: reads only through the person's glab login; no
  push, MR or publication.
