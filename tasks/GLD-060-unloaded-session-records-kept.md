---
id: GLD-060
title: Saving sessions must not erase the records of a project whose records were never loaded
kind: fix
status: backlog
priority: 2
component: session
depends_on: []
touches:
  - crates/session/src/persist.rs
  - crates/session/src/desk.rs
  - crates/session/tests/all/workspace.rs
  - crates/desktop/src/root/command.rs
  - crates/desktop/src/root/projects.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-24"
needs_person: false
---

## Goal

**Data loss.** `persist::save` replaces a project's session records
wholesale with what is in memory, without knowing whether that project's
records were ever loaded into memory. When they were not, the save writes
nothing over them: the conversations a workspace held in that folder are
gone from its `.guild/state.json`. The saved tab strip still names them, so
the loss shows only on the first visit, when `restore_tab` finds no record
and skips the tab, and that visit then saves the shorter strip, which makes
the loss permanent.

Two routes into it were reported (below). Both have the same cause, so
they get one fix: remember which projects' records were loaded (or load the
retained ones before the first save that would replace them), and never
replace records that were not loaded.

When this task is finished, a save keeps every record it did not load, and
both scenarios end with every tab back on the first visit.

**Reported by code review of GLD-058. Neither scenario has been reproduced
yet.** The first finding's verdict is `unverified`, the second's
`pre_existing`.

## Acceptance

- [ ] WHEN a project is closed in workspace B with the default
      `forget_sessions_on_close: false`, Guild restarts in workspace A, and
      `guild session new <that folder>` without `--front` targets B THEN a
      test reproduces B's retained records for that folder being dropped
      from its `.guild/state.json`, and the test fails for that reason
      before the fix. (Finding `docs+tests-2`.)
- [ ] WHEN Guild starts while a folder held by an unvisited workspace B is
      not mounted, and the folder is back before the next save THEN a test
      reproduces that save dropping B's records for the folder, and the
      test fails for that reason before the fix. (Finding `bugs+impl-1`.)
- [ ] WHEN either test runs after the fix THEN the folder's file still holds
      every record it held before, and a session the command added is there
      too.
- [ ] WHEN the person then visits B THEN every tab the saved strip names is
      restored, and the strip saved after the visit is no shorter than the
      one before it.
- [ ] WHEN a project's records were loaded, and a session is closed,
      removed or forgotten in memory THEN the save still removes it from
      the file as it does now. The fix must not keep deleted records.
- [ ] WHEN `forget_sessions_on_close` is on THEN closing a project still
      clears its records as now (`root/projects.rs:427-444`).
- [ ] WHEN a folder is still unmounted at a save THEN its sessions still go
      to the global file as now (`persist.rs:81-88`).

## Checks

- Tests first: the two reproductions above, each failing before the fix.
  They read the folder's `.guild/state.json` from disk, not only the
  in-memory desk.
- A session-crate test of the rule itself: `persist::save` over a desk that
  did not load a project's records leaves that project's records alone.
- `just check`, isolated: `GUILD_STATE_DIR` in a scratch folder,
  `GUILD_NO_GLOBAL_HOOKS=1`, `GUILD_USER_STATUSLINE` unset. Bare tests can
  write to `~/.config/guild`.
- No window check: when the fix works, nothing changes on screen.

## Limits / non-goals

- The other two pre-existing findings of the same review are left for
  separate tasks: a stopped background task restarted on the first visit
  (`bugs+impl-2`) and legacy open sessions hidden by an appended strip
  (`bugs+impl-3`). So are its active findings (`bugs+impl-4`,
  `docs+tests-4`..`7`).
- No change to the file format beyond what tracking "loaded" needs. Ideally
  it is tracked in memory only.
- `project_state::load_for_change` (`project_state.rs:1173`) and the
  `NotRead` refusal (`store.rs:613-619`) stay as they are. They guard a
  different case: a read that failed or a folder that went away.

## Context

Source: GLD-058's code review r2 (revust, `codex-only`), section
"Pre-existing", findings `docs+tests-2` and `bugs+impl-1`.
These are the reviewer's claims, not reproduced. GLD-058, merged in
`dd1851e2`, closed the background launch's own layout writes
(`store_behind`) and deliberately left this one.

Read from the code on 2026-11-24 (main at `dd1851e2`):

- `crates/session/src/persist.rs:74` is `save`. `:79-104`: for each folder
  that is there, it loads the file (`:96`), drops every record whose
  workspace is a current holder (`:100-103`), and adds the holders' records
  from memory (`:104`). Nothing tells it whether those records were ever
  in memory.
- `crates/session/src/persist.rs:29-66` is `load`. `:47-48` read each
  folder's file through `project_state::load`. That function
  (`project_state.rs:1157-1161`) answers an empty state for a folder that is
  not there, so an unmounted folder at startup loads no records, and the
  `persist::save` after it is remounted finds them on disk and drops them.
  This is route 2. Read from the code: any `Root::save`
  (`crates/desktop/src/lib.rs:1590`) after the remount seems to reach it,
  not only a background launch. Not confirmed.
- `crates/session/src/desk.rs:546` is `Desk::open_in`. It adds the
  workspace as a holder (`:567-574`) and loads no records.
- `crates/desktop/src/root/command.rs:703` is `command_session_new`. It calls
  `open_in` (`:731`) and then `command_session_start` (`:767`) without
  `adopt_project`. The save comes from `open_record_without_history`
  (`root/tabs.rs:463`, `save()` at `:513`). This is route 1.
- `crates/desktop/src/root/projects.rs:139` is `adopt_project`, the import of
  retained records that the window's own open path runs (`:40-42`) and the
  command path does not.
- `crates/desktop/src/root/projects.rs:401` closes a seat, and
  `persist.rs:97-101` keeps records of a workspace that no longer holds the
  folder on disk. That is what keeps a closed project's records, by default
  (`settings.rs:431`, `forget_sessions_on_close`, default `false` at
  `:916`).
- `crates/desktop/src/root/tabs.rs:523-538` is `restore_tab`. It returns
  without a tab when `desk.session(id)` finds no record (`:530-532`).

Seen while reading, not reported by the review and not reproduced:

- `crates/desktop/src/root/command.rs:1325`: `command_open` (`guild open`)
  has the same `open_in` (`:1341`) without `adopt_project`, followed by
  `save()`.
- `crates/desktop/src/root/projects.rs:140` skips the import when
  `sessions_of(project)` finds any record. Through
  `From<ProjectId> for Seat` (`session/src/workspace.rs:157`) that means any
  workspace's record (`desk.rs:924-930`). If A still holds the folder, it
  seems that reopening it in B imports nothing of B's and the next save
  drops B's retained records.

The fix probably belongs to `guild-session`. It needs a fact about what was
loaded that `persist::save` can read, with the window marking a folder
adopted. Whether that fact lives on `Desk` or is passed to `save` is the
implementer's decision. Not confirmed: whether the import in
`adopt_project` is enough for route 1, or whether its per-root check
(above) must become per-seat too.
