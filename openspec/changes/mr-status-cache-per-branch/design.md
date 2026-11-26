## Context

Implemented today (decision 56): `crate::forge::Forge` is the pure model, one
`Repo` per common git directory; `root/forge.rs` runs one thread per refresh;
`guild_session::merge_requests::glab::Glab::read_until` reads one host and
project's branches and answers `Err` for the whole batch at the first failed
branch. `Forge::retain` drops a repository that left the column, `land`
answers `true` for any accepted answer and `poll_forge` then notifies `Root`,
which rebuilds the cached column region whole (`docs/traps.md`, `view::region`).

## Goals / Non-Goals

Goals: the four agreed behaviours in proposal.md, a repository-state API task
B can draw from, and a measurement of the column's frame before and after.
Non-goals: any new drawing; per-element column updates (proposed only).

## Decisions

1. **Hidden, not forgotten.** `Repo` gains a `hidden` flag instead of being
   removed. `retain` hides a repository the column no longer shows: its write
   epoch is bumped when a refresh is out (that answer is dropped, as today),
   nothing pending survives, `trigger` and `take_due` skip it and `shown`
   answers `Nothing` for it. Its candidates, resolutions and reads stay.
   `inputs` for a hidden repository unhides it and makes it due, so the last
   known badges draw at once and a background refresh follows; a refresh
   still out is not doubled — the pending one runs when it ends. `set_enabled`
   still clears every read and drops hidden repositories with nothing out.
   *Alternative*: a separate cache map keyed by repository — rejected, two
   places to keep consistent for the same facts.

2. **Per-branch results, project-wide errors apart.** `read_until` answers
   `Result<Vec<(String, Result<Option<MergeRequest>, Error>)>, Error>`. The
   outer error is the read's as a whole: `Stopped`, `NotInstalled`,
   `SignedOut` (a 401 on any branch: the rest is not asked), and `TimedOut`
   only when the deadline passed before the first branch. Inside the loop a
   branch's own failure (`TimedOut`, `Changed`, `Failed`) is that branch's;
   a deadline that passes mid-batch makes each remaining branch `TimedOut`
   without a call. The desktop's `Batch::result` takes the same shape. `Repo`
   keeps `failures` per `(host, project, branch)` and `problems` — the last
   landed answer's project-wide state: each `Resolution::Why` it resolved and
   each batch whose outer result failed. `Shown::Why` is removed; a row draws
   only its branch's MR, its branch's failure, or nothing.
   *Alternative*: keep the outer error per batch and copy it to each branch —
   rejected, that is the behaviour the user asked to remove.

3. **The default branch from the existing rule.** `git::main_branch(root)`
   (M13.1: `origin/HEAD`'s local branch, else `main`/`master`) is read on the
   git thread with the worktree snapshot, in `root/git.rs::counted`, and
   marked on each `Worktree` as `main_branch: bool` (not filled by the parser,
   like `lines` and `push`). `forge_take_in_the_column` leaves marked
   worktrees out of the candidates, so no glab call, badge or failure.
   *Alternative*: compute it in the refresh thread — rejected, the candidate
   would still exist and could still carry a stale badge.

4. **Notify on a visible change.** `Forge` builds an owned, comparable
   picture of a repository — each candidate's drawn row (`Drawn`: nothing,
   unknown with its error, or the MR with its failure and, only when a
   failure is drawn, the read time its text ages from) and its problems —
   before and after `land`; `land` answers `true` only when they differ.
   Progress (queued/checking) is not part of the picture while nothing draws
   it; task B adds it there if its indicator does.

### API for task B (`crate::forge`)

```rust
pub enum Progress { Idle, Queued, Checking }
pub enum Problem { Why(Why), Batch { host: String, project: String, error: Error } }
pub struct RepoStatus<'a> {
    pub progress: Progress,
    pub checked: Option<Instant>,   // the last landed answer
    pub problems: &'a [Problem],    // that answer's project-wide failures
}
impl Forge { pub fn status(&self, repo: &Path) -> Option<RepoStatus<'_>> }
impl Ui { pub fn mr_problem(self, problem: &Problem) -> String }
```

`Root::forge_status(repo)` forwards it. `None`: the setting is off, the
repository is hidden, or it was never taken in.

## Risks / Trade-offs

- [A row loses its "glab not found" hint before B ships] → agreed; the reason
  is in `status()` and B draws it.
- [Hidden repositories accumulate for the session] → bounded by the
  projects ever opened; off forgets them; nothing is on disk.
- [Two more git reads per changed worktree snapshot] → only when the list or
  config changed, off the window's thread.
- [A notify still rebuilds the whole column when badges do change] →
  measured; a per-element update is proposed with its cost, not done.

## Migration Plan

None: in-memory state only. Rollback is reverting the branch.
