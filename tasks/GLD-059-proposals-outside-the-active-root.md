---
id: GLD-059
title: Edit proposals for a file outside the active root land, show and expire correctly
kind: fix
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/ide.rs
  - crates/desktop/src/root/workspaces.rs
  - crates/desktop/src/root/transfers.rs
  - crates/desktop/src/root/tree.rs
  - crates/desktop/tests/all/ide.rs
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-23"
---

## Goal

An agent's edit proposal (`openDiff`) is placed by the deepest open root
containing its file, but where it is shown and when it is withdrawn are
still worked out from the active workspace and from the tree's project.
For a file under a different root than the one in front — a member of a
group folder, most often — three things go wrong:

1. A session started without `--front` in workspace A, while B is in
   front, proposes an edit to a member file held only by A: the proposal is
   adopted into (B, member), A never shows it, and the agent's call stays
   parked.
2. With the group folder in front, a proposal for a member file fills the
   body while the tab strip stays on the group folder's seat: there is no
   tab for what is shown.
3. Renaming or moving a member file from the group folder's Explorer
   withdraws proposals of the group's project only. A proposal under the
   member's root survives, and Accept can still send `FILE_SAVED` for the
   old path — the CLI would then write a file at a path the person has just
   moved away from.

The third breaks what AGENTS.md promises about the IDE protocol: "The IDE
protocol shows and answers; it never writes the person's files." The CLI
writes after `FILE_SAVED`; a stale path in that answer is a write the
person did not ask for.

When this task is finished, a proposal lands in a workspace that holds its
root, is shown with its tab, and is withdrawn when its file moves, whichever
Explorer moved it.

**Reported by code review of GLD-055, not reproduced yet** — three
findings, one subject.

## Acceptance

- [ ] WHEN each of the three cases above is set up THEN a test reproduces
      it, and it fails for that reason before the fix.
- [ ] WHEN `openDiff` names a file whose owning root is held by the
      requesting session's workspace THEN the proposal is adopted into that
      workspace's seat for the root; WHEN not, into a workspace that holds
      the root; WHEN none does, it is refused as a file outside every
      project is now.
- [ ] WHEN a proposal is adopted into a workspace that is not in front THEN
      the front workspace keeps its place, and the proposal is there when
      the person goes to it; the IDE call is answered when the person
      answers it there.
- [ ] WHEN a proposal is brought forward THEN its owning seat is selected:
      the body and the tab strip show the same tab.
- [ ] WHEN a file or a directory holding it is renamed or moved from any
      Explorer — its own root's or a group folder's — THEN every pending
      proposal whose resolved path is under the source is rejected, across
      roots, both while the rename/move is pending and when it completes.
- [ ] WHEN Accept is pressed on a proposal whose file is being moved or
      renamed from any Explorer THEN no `FILE_SAVED` is sent.
- [ ] WHEN a proposal's file is in the active root THEN nothing changes.

## Checks

- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`).
- The tests drive the IDE handler and the Explorer's rename/move in a
  `Root`, with two workspaces and a group folder with a member, and assert
  on the IDE answer sent, not only on the tabs.
- No live agent: a provider is not launched.

## Limits / non-goals

- The protocol's answers (`FILE_SAVED`, the rejection text) do not change.
- No change to how a proposal is drawn.
- Which workspace a session belongs to is read from its record; no new
  record field.

## Context

Findings in the code review of GLD-055 round 1, all verdict `pre_existing`,
section "Pre-existing":

- `bugs+impl-6` — "A background group session can place a proposal in a
  workspace without its member" (case 1);
- `arch+quality-4` — "A member proposal can fill the body while the group
  tab strip stays selected" (case 2);
- `adversarial-2` — "Renaming a member file from the group Explorer leaves
  its old proposal actionable" (case 3).

The reviewer's claims, not reproduced.

Read from the code on 2026-11-23 (main at c46ac719):

- `crates/desktop/src/root/ide.rs:311` — `open_proposal`; `:340` resolves
  the file by `project_of`, the deepest open root
  (`root/lsp.rs:573`, `column.rs:346`), across every workspace's projects;
  the caller's workspace is not consulted.
- `crates/desktop/src/root/ide.rs:520` — `open_proposal_tab_without_history`;
  `:544` adopts into `self.seat_for_project(project)`; `:553` sets
  `active_tab` without selecting the seat.
- `crates/desktop/src/root/workspaces.rs:274-278` — `seat_for_project` is
  always `self.active_workspace`.
- `crates/desktop/src/view/tabs.rs:229` — the strip lists `area_tabs`, the
  selected seat's.
- `crates/desktop/src/root/transfers.rs:801` — `relocate_project_paths`;
  `:813` picks stale proposals with `t.project == project`, the caller's
  project. Callers: `root/tree.rs:1131` (`follow_rename`, with
  `self.active_project`) and `root/transfers.rs:740` (a completed Move).
- `crates/desktop/src/root/ide.rs:383` — `accept_proposal`; `:394-405` holds
  Accept back only when the pending operation's or the naming tree's
  project equals the proposal tab's project.
- `AGENTS.md:430` — the IDE protocol's guarantee.

Not confirmed: whether a session record's workspace is the right "requesting
session's workspace" for a call that arrives on the IDE socket (the
connection-to-session mapping was not traced); whether a parked call in a
background workspace is otherwise answered on close or quit.

Related: GLD-055 (`tasks/GLD-055-group-sessions.md`), merged in c46ac719;
GLD-057 guards the open-root case of the same Explorer.
