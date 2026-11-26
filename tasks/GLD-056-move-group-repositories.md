---
id: GLD-056
title: "Move a group's repositories into its folder, so the disk matches the column"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-055]
touches:
  - docs/decisions.md
  - crates/desktop/src/git.rs
  - crates/desktop/src/root/git.rs
  - crates/desktop/tests/all/git_writes.rs
  - crates/desktop/src/group_move.rs
  - crates/desktop/src/root/group_move.rs
  - crates/desktop/src/view/group_move.rs
  - crates/desktop/src/lib.rs
  - crates/desktop/src/root/mod.rs
  - crates/desktop/src/view/mod.rs
  - crates/desktop/src/view/menu/column.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/root/lsp.rs
  - crates/desktop/src/i18n/mod.rs
  - crates/desktop/src/i18n/english.rs
  - crates/desktop/src/i18n/russian.rs
  - crates/desktop/tests/all/group_move.rs
  - crates/desktop/tests/all/main.rs
  - crates/session/src/project_group.rs
  - crates/session/src/workspace.rs
  - crates/session/src/desk.rs
  - crates/session/src/settings.rs
  - crates/session/tests/all/project_group.rs
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-23"
needs_person: true
---

## Goal

GLD-055 binds a project group to a folder and shows whether the disk matches
the column: every member's main checkout a direct child of the group's
folder. When it does not, the person today has to move the repositories by
hand, fix the linked worktrees git can no longer find, and reopen everything
in Guild.

When this task is finished, the group's `…` menu offers to **make the disk
match Guild**: move the members' repositories — each main checkout and its
linked worktrees — into the group's folder, creating the folder if it does
not exist, so the operating system's layout becomes the column's and the
GLD-055 indicator turns to "matches".

**Two phases, in this order (agreed with the person):**

1. **The plan.** Guild shows what would move where, the commands it would
   run, what blocks the move, and what it cannot see (see Risks). Nothing is
   changed on disk. This phase is useful on its own: a person can run the
   commands by hand.
2. **The move.** From the same plan, Guild performs the move itself, with
   every guard below, and rewrites its own state.

The plan phase can be merged before the move if the person wants it so; the
split is decided when the task is taken.

**Design first.** The menu item, the plan sheet and its blocked and failed
states change what a person sees: they are drawn as a Penpot proposal from
the Library's components and tokens (`docs/penpot.md`) and approved by the
person before any code.

**A decision first.** The move adds git writes (`git worktree move`,
`git worktree repair`). Per decision 21 a new write is a decision in
`docs/decisions.md` first, with its guards, and the test pinning
`git::Write`'s verbs changes with it — before the code.

## Risks found while writing (facts to verify)

Marked **verified** where it was checked on 2026-11-23, otherwise
**unverified**.

- **Harness conversations are kept by path.** Claude Code keeps a folder's
  conversations under `~/.claude/projects/<encoded path>/`, the path with
  every non-alphanumeric character made `-` (**verified**:
  `crates/session/src/history/claude.rs:13-31`, and the directories present
  on this machine). Guild's own history reader computes that directory from
  the folder and confirms each file by its recorded `cwd`, so after a move
  the history list of the new path is empty (**verified by reading**, not
  run). Whether `claude --resume <id>` of an earlier session finds it from
  the new path is **unverified**; `crates/session/src/desk.rs:157-162` keeps
  a session's `cwd` apart from its project's path *because* `--resume` looks
  at the old path. Codex and OpenCode: **unverified**. **Guild never moves
  or edits a harness's own files**; the plan says which sessions may not
  resume.
- **Linked worktrees point at their repository by absolute path, both
  ways.** A linked worktree's `.git` file names
  `<main>/.git/worktrees/<name>`, and that directory's `gitdir` names the
  worktree's `.git` (**verified**, git 2.50.1). After renaming the main
  checkout the worktree fails with *not a git repository* (**verified**);
  after renaming both, `git worktree repair <new worktree paths>` run in the
  main checkout fixes both links (**verified**). `git worktree move` moves a
  linked worktree (**verified**), refuses the main working tree (*is a main
  working tree*, **verified**) and a locked one (*cannot move a locked working
  tree*, **verified**). So a main checkout is moved by `rename(2)` and its
  worktrees by `git worktree move` or by a rename followed by `repair` — the
  order is the implementation's to choose and the decision's to record.
- **These are new git writes.** `git::Write` (`crates/desktop/src/git.rs:170`,
  verbs in `Write::args`, `:426`) has `WorktreeAdd`, `WorktreeRemove`,
  `WorktreeRemoveConfirmed` and `WorktreeUnlock` and no move or repair; the
  list is pinned by
  `every_write_is_named_here_and_restore_appearing_twice_is_two_of_them`
  (`crates/desktop/tests/all/git_writes.rs:842`) and the destructive
  spellings by the substring grep there (`:657-714`). `git worktree move`
  must never be given `-f`; `repair` takes no force.
- **Absolute paths outside Guild's reach break**, and Guild cannot detect
  them all: Python virtual environments (their scripts' shebangs), `direnv
  allow` and `mise trust` approvals, docker-compose bind mounts, IDE recent
  projects, shell history and scripts. The plan says so in words rather than
  pretending to a complete list (**unverified** per tool).
- **Guild's own state keyed by path** has to be rewritten in one atomic
  save. The repository key is the common git directory
  (`crates/session/src/project_group.rs:70-72`), which for a main checkout is
  `<main>/.git`, so moving the main checkout changes the key of every map
  keyed by it (**verified by reading**):
  - `Project.path` and `Project.repository` (`crates/session/src/desk.rs:58-77`);
  - the group's members (`ProjectGroup.repositories`,
    `crates/session/src/project_group.rs:61`);
  - the desk's `names`, `folded` and `project_activity` (`desk.rs:316`,
    `:350`, `:357`) and `recent` (`:343`);
  - `Workspace.worktree_order` (keys and values), `pinned_worktrees`,
    `unfolded_worktree_tabs` (`crates/session/src/workspace.rs:113-123`);
  - `Settings.repository_settings` (`crates/session/src/settings.rs:158`),
    including a `worktree_directory` that pointed inside the old place;
  - `SessionRecord.cwd` (`desk.rs:162`) — **deliberately not** rewritten
    blindly: it is where the session ran. What happens to it is open
    question 3.
  `desk.rs:1170-1185` restores a root from a stray session's `cwd`, so a
  half-rewritten state can bring a root back at a path that no longer
  exists.
- **One repository, one parent.** A repository in groups of two workspaces
  bound to different folders can live under only one of them.
- **A group rename does not rename the folder** (GLD-055), unless decided
  here.

## Acceptance

- [ ] WHEN the person approves the Penpot proposal of the menu item, the plan
      sheet and its blocked and failed states THEN the code follows it, and
      the proposal is promoted into the Library and Screens in the same
      branch.
- [ ] WHEN the decision for the new writes is recorded in `docs/decisions.md`
      under decision 21 with its guards THEN, and only then, the writes are
      added to `git::Write`, and the verb-pinning test lists them.
- [ ] WHEN the person opens the group's `…` menu and the group has a folder
      and does not match THEN *Move repositories into the folder…* is
      offered; WHEN it matches THEN it is not.
- [ ] WHEN the person chooses it THEN Guild shows the plan without changing
      anything: per member, the main checkout's current and new path, each
      linked worktree's current and new path, whether the folder will be
      created, the commands it would run in order, every guard that blocks,
      the sessions that may not resume, and a sentence naming what Guild
      cannot see (virtual environments, `direnv`, `mise`, bind mounts, IDE
      projects).
- [ ] WHEN the plan is shown THEN the commands can be copied, so a person can
      run them by hand.
- [ ] WHEN an agent in any tab of a root that would move is working THEN the
      move is refused, naming the tab.
- [ ] WHEN a session or terminal is open in a root that would move THEN the
      move is refused, naming it (whether Guild may close them itself is open
      question 4).
- [ ] WHEN a file of a root that would move has unsaved text THEN the move is
      refused, naming the file.
- [ ] WHEN a language server runs in a root that would move THEN the move is
      refused, or the server is stopped first — open question 5 decides.
- [ ] WHEN a linked worktree that would move is locked THEN the move is
      refused, naming it; Guild never passes `-f`.
- [ ] WHEN a member and the group's folder are on different volumes THEN the
      move is refused; Guild only renames (`rename(2)`), never copies and
      deletes.
- [ ] WHEN a target path already exists THEN the move is refused for that
      member, naming the path; nothing is overwritten or merged.
- [ ] WHEN a repository is a member of groups in two workspaces bound to
      different folders THEN the plan says so and the move of that
      repository is refused until the person resolves it.
- [ ] WHEN no guard blocks and the person confirms in the sheet THEN, on one
      volume, Guild creates the folder if needed, moves each member's main
      checkout and its linked worktrees, and git answers every moved
      worktree (`git worktree list`) at its new path with its branch.
- [ ] WHEN the move succeeds THEN Guild's state is rewritten in one atomic
      save — every path-keyed entry listed in Risks — and after a restart the
      roots, the group, the names, folds, order, pins and repository settings
      are as they were, at the new paths.
- [ ] WHEN a step fails midway THEN Guild stops at that step, changes nothing
      further, keeps its state consistent with what is on disk, and reports
      what moved, what did not, and the commands that finish or undo it. "Recoverable"
      means: every member is either wholly at its old path or wholly at its
      new one with git's links repaired, and the report says which; a member
      caught between (main moved, worktree not yet) is named with the exact
      commands to finish it.
- [ ] WHEN the move has finished THEN the GLD-055 indicator shows the group
      as matching.
- [ ] WHEN the open questions below are put to the person THEN each answer is
      recorded in this file before the code that depends on it.
- [ ] The implementing branch adds a `### Added` line to `CHANGELOG.md`
      under `## [Unreleased]`.

## Checks

- Tests first. A pure plan (`group_move.rs`, new) tested without a window:
  targets for main checkouts and worktrees, each guard, the two-workspace
  case, the command list.
- Tests on scratch repositories in `tests/all/group_move.rs` (new, declared
  in `tests/all/main.rs`): a main checkout with two linked worktrees moved on
  one volume and `git worktree list` read back; a locked worktree refused; an
  existing target refused; a failure injected after the main checkout moved,
  and the report and the state checked against the disk.
- The session crate: the state rewrite of every path-keyed field, and an old
  path left nowhere (`tests/all/project_group.rs` or a new module).
- The verb-pinning and destructive-spelling tests in
  `tests/all/git_writes.rs` extended for the new writes.
- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`).
- A window check in an isolated dev window with a group of scratch
  repositories beside a folder: the plan, each blocked state, the move, a
  restart, the indicator — compared with the approved proposal. Needs a
  person at the machine.
- Not run: anything on the person's real repositories; a cross-volume move
  (the refusal is tested by its check, not by a second volume); `--resume`
  of a real conversation after a move, which is a provider call and only
  with the person's permission.

## Limits / non-goals

- **Never moves or edits a harness's own files** (`~/.claude/projects`,
  Codex's or OpenCode's stores).
- **Never copies and deletes**, never crosses volumes, never overwrites a
  target, never passes a force flag.
- **Never touches the person's files inside a repository** — no
  `.gitignore`, no `AGENTS.md`, no virtual environment repaired.
- No moving a repository out of a group's folder, or between groups' folders.
- No renaming the folder when the group is renamed (unless open question 7
  decides otherwise).
- No `git init` in the group's folder.

## Open questions for the person

Not decided. Each is put to the person as a separate question when the task
is taken; the proposals are proposals.

1. **One branch or two.** Merge the plan phase first and the move after it,
   or both in this task's branch?
2. **Partial moves.** When one member is blocked, may the others move?
   *Proposal:* no — all or nothing at the plan's level, so the group either
   matches afterwards or nothing changed.
3. **Sessions that ran at the old path.** *Proposal:* keep their `cwd` as is
   and mark them as not resumable here, since `--resume` looks at the old
   path (`desk.rs:157-162`); the plan names them. Alternative: rewrite the
   `cwd` and accept that `--resume` may not find the conversation.
4. **Open sessions and terminals.** Refuse until the person closes them
   (proposal), or close idle ones as part of the move?
5. **Language servers.** Refuse while one runs (proposal), or stop them as
   part of the move and let them start again lazily?
6. **A `guild` door for the plan** (e.g. `guild group move --plan --json`)
   — in this task, or later? The move itself stays a window gesture.
7. **Group rename and the folder.** Still not renamed (GLD-055), or offered
   here as a second move?
8. **A worktree outside the repository's default place** (a
   `worktree_directory` setting, or one made by hand elsewhere): moved into
   the group's folder too, or left where it is and named in the plan?

## Context

Verified in the code on 2026-11-23 (path:line as cited above), plus:

- `crates/desktop/src/column.rs:375-385` — `folder_for`: a new worktree is
  `../<repo>-<name>`, a sibling of the main checkout, so after the move new
  worktrees land in the group's folder too, unless a `worktree_directory`
  setting says otherwise (`crates/desktop/src/repository_settings.rs:54-75`).
- `crates/desktop/src/closing.rs:20-125` — what closing a tab would interrupt
  (`Stake`), the source for the "agent working" and "session open" guards.
- `docs/decisions.md:68-` — decision 21; decision 30 (`:188`) — a menu item
  may write, and what guards it is a sheet.
- `crates/session/src/transcript.rs:60-75` — Guild finds a transcript by id
  across every directory of `~/.claude/projects`, so Guild's own reading of
  an earlier transcript survives the move even where the history list does
  not.

New files this task adds (in `touches`, not yet in the tree):
`crates/desktop/src/group_move.rs`, `crates/desktop/src/root/group_move.rs`,
`crates/desktop/src/view/group_move.rs`,
`crates/desktop/tests/all/group_move.rs`. Their names are a proposal.

Verified with git 2.50.1 on scratch repositories on 2026-11-23: the
worktree link behaviour in Risks.

Not verified: `claude --resume` from a moved folder; Codex's and OpenCode's
session stores after a move; how each tool in the "cannot see" list fails.

Related: GLD-055 (the group's folder and the match indicator); GLD-051 (the
workflow); `AGENTS.md` "Git is written by named actions"; `docs/penpot.md`.
`CHANGELOG.md` is not in `touches` on purpose: `tasks/README.md` keeps a
changelog out of it.
