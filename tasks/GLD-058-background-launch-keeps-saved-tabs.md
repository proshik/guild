---
id: GLD-058
title: A background session launch must not overwrite an unrestored workspace's saved tabs
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/command.rs
  - crates/desktop/src/root/tabs.rs
  - crates/desktop/src/root/workspaces.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-23"
---

## Goal

**Data loss.** After a restart in workspace A, a `guild` command that opens
a session in workspace B without `--front`, before the person has visited
B, can wipe B's saved tabs. The command makes B active in the model without
restoring B's tabs, the session's opening saves, and the save writes every
project of B from what is in memory — the new tab, or nothing. A group
folder can keep only the new tab, and B's other projects can lose their
whole saved layout.

When this task is finished, a background launch into a workspace that has
not been restored restores it first, as resuming a session already does,
and the person's place in A stays as it was.

**Reported by code review of GLD-055, not reproduced yet.**

## Acceptance

- [ ] WHEN Guild has restarted in workspace A and `guild session new
      --group <name>` without `--front` targets workspace B, not yet
      visited THEN a test reproduces B's saved layouts being overwritten,
      and it fails for that reason before the fix.
- [ ] WHEN the same is done with plain `guild session new <folder>` and with
      `guild worktree create … --prompt` into B THEN the test covers them
      too (all three go through `command_session_start`).
- [ ] WHEN any of the three launches into an unrestored B without `--front`
      THEN every project of B keeps its saved tabs and areas, and the new
      session is added to its project's strip.
- [ ] WHEN it does THEN A stays in front: the active workspace, project and
      tab, and the focus, are what they were before the command.
- [ ] WHEN the person then visits B THEN its tabs are shown once each — none
      restored twice, no second process on one transcript.
- [ ] WHEN the launch has `--front`, or B was already restored THEN the
      behaviour is as now.

## Checks

- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`).
- The tests read the projects' saved state from disk after the launch, not
  only the in-memory stage.
- No window check: nothing a person sees changes when it works.

## Limits / non-goals

- The restore path is reused, not written a second time.
- No change to what `store_tabs` writes for a restored workspace.

## Context

Finding `bugs+impl-1`, verdict `pre_existing`, in the code review of GLD-055
round 2, section "Pre-existing". The reviewer's claim, not reproduced.

Read from the code on 2026-11-23 (main at c46ac719):

- `crates/desktop/src/root/command.rs:767` — `command_session_start`;
  `:791-792` sets `desk.select_workspace(target)` and
  `self.active_workspace = target` without a restore; `:795`
  `open_tab_with_selection`; `:800-807` puts the person's place back and
  saves.
- Its callers: `root/command.rs:733` (`command_session_new`, from
  `session new`), `:873` (`command_session_new_in_group`, `--group`), and
  `:1225` (`worktree create --prompt`, through `command_session_new`).
- `crates/desktop/src/root/tabs.rs:485` — `open_record_without_history`
  calls `save()`; reached from `open_tab_with_selection` (`:354`) through
  `open_tab_from` (`:379`) — the chain was read, not run.
- `crates/desktop/src/lib.rs:1590` — `save()` ends in `store_tabs`;
  `root/tabs.rs:542-549` — `store_tabs` writes every project of
  `self.active_workspace`.
- `crates/desktop/src/root/tabs.rs:1121-1126` — the resume path,
  `reopen_prompt_without_history` (`:1104`), restores an unseen workspace
  (`select_workspace`) before a tab goes into it, and `put_back` (`:1461`)
  returns the person's place. `restore_tabs_without_history` (`:694-697`)
  is guarded by `restored_workspaces`.

Not confirmed: that `select_workspace` restores without taking the focus
visibly for a frame; whether a background launch should go through
`put_back` or keep its own restore at `command.rs:800-807`.

Related: GLD-055 (`tasks/GLD-055-group-sessions.md`), merged in c46ac719,
made `--group` a third route into this function.
