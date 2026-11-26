## Why

`mr-status-cache-per-branch` moved every failure that is not a branch's —
glab missing, not logged in, a 401, glab's own proxy, the network policy, a
batch's deadline — off the worktree rows and into the repository's state,
and since then nothing draws that state: a person whose glab login expired
sees badges that quietly stop changing. Nor does a row say which of the
merge requests in the column are the person's own.

## What Changes

- **The repository header shows where its merge requests stand** — the
  approved Penpot boards *MR header status — column* (Proposals, Guild —
  Screens): an indicator before `⋯ +` that says *queued* (a clock), *checking*
  (turning arrows), *checked* (a faint ✓, only under the pointer on the
  header) or *couldn't refresh* (struck-through arrows in brass, the reasons in
  its hover). Nothing is drawn while the setting is off or before the first
  answer.
- **"You created this merge request"**: a faint person after the MR's number
  on its worktree row, with that hover, when the MR's author is the account
  glab is logged in with on the MR's host — the board *MR author — worktree
  row*. Another's MR has no mark.
- **One account read per host per refresh**: `glab api --hostname <host> user`
  for a host whose batch found an MR, inside the same admission, deadline,
  output cap, scrubbed environment and proxy as every other call. A failed
  account read changes nothing but the mark: the MR is shown, without it.
  Decision 56's "no account read" is revised.

## Capabilities

### Modified Capabilities

- `worktree-merge-requests` (introduced by `mr-status-cache-per-branch`):
  the repository's state is drawn; an MR's author is read and compared with
  glab's account.

## Non-goals

- No click on the indicator: the header's click still folds it.
- No account cache beyond one refresh, no new setting.
- No per-row redraw (deferred by the user in `mr-status-cache-per-branch` 4.4).

## Impact

- `crates/session/src/merge_requests/` — `MergeRequest::author`, the list's
  `author.id`; `Glab::account`.
- `crates/desktop/src/forge.rs` — `Answer::accounts`, `Shown::Mr::mine`,
  `RepoStatus::indicator`, `Forge::indicators`.
- `crates/desktop/src/root/forge.rs` — the account read in `refresh`; a notify
  when an indicator changes.
- `crates/desktop/src/view/column.rs`, `view/forge.rs`, `view/activity.rs` —
  the header's indicator, the row's mark, a turning glyph.
- `assets/ui` + `icons::UI` — Lucide `clock`, `user`, `refresh-cw-off`.
- `i18n`, `docs/decisions.md` decision 56, `CHANGELOG.md`, Penpot.
