## Context

`mr-status-cache-per-branch` gave the model `Forge::status(repo) ->
RepoStatus { progress, checked, problems }` and stopped rows from saying a
repository's problem. The approved Penpot boards (Guild — Screens,
Proposals: *MR header status — column*, *MR author — worktree row*) say what
the header and the row draw.

## Decisions

- **The indicator is the model's, the look the view's.** `RepoStatus::
  indicator()` folds progress, problems and `checked` into one `Indicator`
  (`Checking > Queued > Failed > Checked > none`), pure and tested without a
  window; `view::column` draws it. Checking wins over a problem of the last
  answer: the refresh out may clear it.
- **A notify for an indicator, whichever path moved it.** Progress changes
  outside `land` — `trigger`, `take_due`, `wrote`, `abandon`, `inputs` — so
  `land`'s picture comparison cannot see it. `poll_forge` compares
  `Forge::indicators()` with the ones last drawn and notifies on a
  difference; *checked*'s time is left out (its hover reads the clock when it
  opens), problems are in. The author mark is in `Repo::picture()`, since it
  changes only when an answer lands.
- **The account rides the answer.** `refresh` reads `Glab::account(host,
  deadline)` once per host, after that host's batches, only when one of them
  found an MR, under the refresh's deadline — through `Glab::call`, so
  admission, caps, the scrubbed environment and the proxy are the reads'.
  `Answer::accounts` carries what was read; the model keeps the last account
  per host for the window's life, so one failed read does not blink the
  mark off. With no account ever read for a host, no MR on it is "mine".
- **`author` is provider-neutral**: `MergeRequest::author: Option<u64>`, the
  account id on the MR's host, from the list's `author.id`.
- **Checking turns with the activity ring's clock.** `view::activity::ring`
  becomes a turning glyph of any asset and size; it asks for frames only
  while visible and not under reduced motion.
- **Checked under the pointer only**: the ✓ wears the same `group_hover`
  as the head's chevron, so it costs no notify; its slot stays visible, so
  the move that reveals it starts its hover (code review r1).
- **Off GitLab, no indicator**: `RepoStatus::gitlab` — a worktree asked or
  with a reason, or, before any answer, a host that says GitLab — keeps a
  GitHub repository's header quiet through its refreshes (code review r1).
- **A failure's reasons stay on one line**, the approved board's tooltip:
  code review r1 proposed a wrapping card for long reasons, and the person
  kept the one line (2026-11-26).

## Risks

- A login changed to another account marks by the old one until a refresh
  reads the new one; a failed read keeps the old one meanwhile. Acceptable:
  every refresh that finds an MR asks again.
- A host a 401 signed out is asked nothing more in that refresh, its
  account included (code review r2).
- A problem is drawn only while the repository has something to ask: with
  its last worktree gone, no answer would clear it (code review r2).
- One more glab call per host per refresh, only where an MR was found.
