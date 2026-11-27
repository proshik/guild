---
id: GLD-057
title: An Explorer must not rename or trash a folder that is an open root
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/tree.rs
  - crates/desktop/src/root/transfers.rs
  - crates/desktop/src/root/tree_drag.rs
  - crates/desktop/src/file_operations.rs
  - crates/desktop/src/view/tree.rs
  - crates/desktop/src/view/tree_drag.rs
  - crates/desktop/src/view/menu/tree.rs
  - crates/desktop/src/i18n/english.rs
  - crates/desktop/src/i18n/russian.rs
  - crates/desktop/tests/all/tree.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-23"
needs_person: true
---

## Goal

An Explorer protects only its own root from Rename and Trash. A directory
below it that is itself an open root — a member checkout in a group
folder's Explorer, or a worktree opened from inside a checkout — is an
ordinary row, so it can be renamed, moved or sent to the Trash while Guild
holds it open. A rename leaves that root's tabs and sessions on a path that
no longer exists. Trash asks about unsaved buffers only for the tree's own
project, so the member's dirty buffers can be missing from the question,
and the person loses them.

When this task is finished, every Explorer file operation refuses a source
that is an open root or contains one, and tells the person why in Guild's
words.

**Reported by code review of GLD-055, not reproduced yet.** GLD-055's group
folder Explorer made the case easy to reach: before it, the only way was a
worktree opened from inside another checkout.

**Open question — answered by the person (2026-11-27):** the refusal is
visible *before* the gesture — Rename, Move to Trash, Cut and Move are
disabled in the row's menu, and the row is not draggable — and the guard
still lives in each handler (a key binding reaches the same handler), so an
attempt through a binding is refused with a notice too. The menu and drag
surfaces read the same guard as the handlers. To be drawn in Penpot from
the Library's components and approved before code (`docs/penpot.md`); the
person's answer here is the decision, the Penpot approval is the gate.

## Proposal (Penpot) — approved 2026-11-27

What the code draws today (read 2026-11-27, main f65258f):
- A row's right-button menu is `view/menu/tree.rs`: a directory gets the
  transform group (`MenuMove`, `MenuCutFiles`, `MenuCopyFiles`,
  `MenuPasteFiles`, `MenuRename`, `MenuDelete` — `entries_for_directory`,
  `transform_group()`); the tree's own row passes `root: true` and gets only
  *Paste* (the two writes are withheld). The refusal today is
  `writable_row` (tree.rs:231) — only the tree's own empty path.
- `view/tree_drag.rs:70` — a row is a drag source only when
  `project_row && writable_row(&path)`. A member/worktree row is `writable`
  (its relative path is non-empty), so it is draggable today.
- `root/transfers.rs:106` `operation_sources` and `:73` `select_operation_target`
  filter by `writable_row` only.

Drawn on 2026-11-27 on Guild — Screens › Proposals and approved by the
person the same day. The unchanged parts of each screen are an image of
`Main window — group folder Explorer`; what changes is built from Library
copies and tokens:

1. `GLD-057 — group folder Explorer · open member row`: `backend` (an open
   project in the group folder `acme`) right-clicked. The directory menu
   (`menu / row` copies): *Move…*, *Cut*, *Rename…* and *Delete* are the
   Library's `Kind=Disabled shortcut` — drawn faint, their strokes kept, not
   taken; *Copy* and *Paste* stay enabled (Copy does not write the source,
   Paste writes into the folder, not over it). A footnote at the menu's foot,
   the `Entry::Note` Open In already uses (`popup::note`: one line, eyebrow,
   `text.faint`, truncated, never widens the menu), says why: **Open as a
   project** / **Открыт как проект**.
2. `GLD-057 — group folder Explorer · refused by a key`: the same row
   selected and ⇧F6, ⌘Backspace, ⌘X or F6 pressed: nothing on disk changes
   and a Warning notice (`Root::warn`, the Library's `notice`
   `Level=Warning`) says **“backend” is open in Guild as a project — close
   it first** / **«backend» открыт в Guild как проект — сначала закройте
   его**. In the containing case the name is the open root's path relative
   to the tree, e.g. “services/backend”.
3. `GLD-057 — menu of a folder that holds an open project (part)`: the same
   menu for a folder that contains an open root; the footnote reads **Holds
   an open project** / **Внутри открытый проект**.

Not drawn, because nothing shows: the row is not a drag source (no drag
starts), and a drop of other rows onto it is unchanged.

## Plan

- [x] Draw the refusal in Penpot (menu items disabled, row not draggable)
      on the Screens file's Proposals page and get the person's approval
      before any code (`docs/penpot.md`).
- [ ] Reproduce: a test in `crates/desktop/tests/all/tree.rs` that renames
      / trashes / moves an open-root member row today and fails for that
      reason before the fix (acceptance 1).
- [ ] Add the guard: a source that is an open root, or contains one, is
      refused at `file_operations`/`tree`/`transfers`/`tree_drag` handlers
      on the resolved source path, and the menu (`view/menu/tree.rs`) and
      drag (`view/tree_drag.rs`) stop offering it (acceptances 3-7).
- [ ] Tell the person why in English and Russian (`i18n/english.rs`,
      `i18n/russian.rs`).
- [ ] `just check`, isolated; the window check against the approved
      proposal; then the Penpot file brought to what was merged.

## Acceptance

- [ ] WHEN a group folder's Explorer shows an open member checkout and the
      person renames it, trashes it, or moves it by Cut/Paste or drag THEN a
      test reproduces the move happening today, and it fails for that reason
      before the fix.
- [ ] WHEN the person has answered the open question and approved the
      Penpot proposal THEN the code follows it.
- [ ] WHEN the source of Rename, Trash, Cut/Paste Move or a drag out of the
      tree is an open root THEN nothing on disk changes and the person is
      told, in English and in Russian, why.
- [ ] WHEN the source is a directory that contains an open root (a
      worktree inside a checkout, a folder holding a member) THEN the same.
- [ ] WHEN several rows are selected and one of them is or contains an
      open root THEN the whole operation is refused, not the rest done.
- [ ] WHEN a source neither is nor contains an open root THEN Rename,
      Trash and Move behave as they do now; the tree's own root stays
      refused as now.
- [ ] The guard is on the resolved source path in each handler, not only
      in the menu: a key binding reaches the same handler.

## Checks

- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`).
- A window check in an isolated dev window: a group folder with an open
  member, the member's row in the group's Explorer — Rename, Move to Trash,
  Cut and Paste, drag onto another folder — and the same for a worktree
  opened from inside a checkout, against the approved proposal.

## Limits / non-goals

- No change to what an open root itself offers on its own row: the tree's
  root is already refused (`writable_row`).
- Copy is not a write to the source and stays allowed.
- The Commit panel shares the Trash service (`trash_paths_asking`); it
  gets the same guard for free, and nothing else in Changes is in scope.
- Closing the root and then renaming it is not offered as a shortcut.

## Context

Finding `adversarial-1`, verdict `pre_existing`, in the code review of
GLD-055 round 1, section "Pre-existing". The reviewer's claim, not reproduced.

Read from the code on 2026-11-23 (main at c46ac719):

- `crates/desktop/src/root/tree.rs:232` — `writable_row` is
  `!relative.as_os_str().is_empty()`: only the tree's own row is protected.
  Its callers: `root/tree.rs:111`, `:132`, `:303` (`Naming::rename`),
  `:487`, `:495`, `:781`, `:1146` (`delete_asking`), `:1172`
  (`trash_paths_asking`); `root/transfers.rs:73`, `:114`, `:120`
  (`select_operation_target`, `operation_sources`); `view/tree.rs:106`;
  `view/tree_drag.rs:70`; `view/menu/tree.rs:251` (Rename, Move, Cut, …);
  `view/menu/changes.rs:293` and `root/changes.rs:713`.
- `crates/desktop/src/root/tree.rs:1066-1080` — Rename's
  `rename_without_replacing`; `:1117` and `:1130-1131` — `follow_rename`
  relocates tabs of `self.active_project` only, the tree's project.
- `crates/desktop/src/root/tree.rs:1178-1193` — `trash_paths_asking`
  collects views with `t.project == project`, the tree's ProjectId, so a
  member's dirty buffer is not counted in `dirty`.
- `crates/desktop/src/root/transfers.rs:740` — a completed Move relocates
  by the tree's project only.
- `crates/desktop/src/column.rs:346` — `owning_root`, the deepest open root
  containing a path; `root/lsp.rs:573` `project_of` uses it. A guard can ask
  whether any open root lies at or under a source the same way.

Not confirmed: whether Guild's own `worktree create` can place a worktree
inside a checkout, or only a person opening one there by hand; and whether
`root/tree_drag.rs` and `file_operations.rs` have a path that does not pass
through `operation_sources`.

Related: GLD-055 (`tasks/GLD-055-group-sessions.md`), merged in c46ac719.
