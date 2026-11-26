---
id: GLD-055
title: "Group sessions: a project group bound to a folder that sessions start in"
kind: feature
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/session/src/project_group.rs
  - crates/session/src/workspace.rs
  - crates/session/src/desk.rs
  - crates/session/src/command.rs
  - crates/session/tests/all/project_group.rs
  - crates/desktop/src/column.rs
  - crates/desktop/src/view/column.rs
  - crates/desktop/src/view/column/project_groups.rs
  - crates/desktop/src/view/menu/column.rs
  - crates/desktop/src/view/menu/mod.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/root/projects.rs
  - crates/desktop/src/root/picker.rs
  - crates/desktop/src/view/picker.rs
  - crates/desktop/src/root/lsp.rs
  - crates/desktop/src/root/watch.rs
  - crates/desktop/src/root/git.rs
  - crates/desktop/src/root/file_index.rs
  - crates/desktop/src/root/symbols.rs
  - crates/desktop/src/root/tree.rs
  - crates/desktop/src/root/files.rs
  - crates/desktop/src/root/command.rs
  - crates/desktop/src/root/ide.rs
  - crates/desktop/src/root/library.rs
  - crates/desktop/src/root/dependencies.rs
  - crates/desktop/src/i18n/mod.rs
  - crates/desktop/src/i18n/english.rs
  - crates/desktop/src/i18n/russian.rs
  - crates/desktop/tests/all/project_groups.rs
  - crates/cli/src/main.rs
  - crates/cli/tests/cli.rs
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-22"
needs_person: true
---

## Goal

A person keeps the repositories of one system in one project group — a
backend, a frontend and a Helm deployment repository, say — and wants an
agent session that sees all of them, mostly to **ask questions across the
group's projects** (GLD-051 records the workflow). Today a session starts in
one root and sees only that root's folder.

When this task is finished, **a project group can be bound to a directory,
and Guild knows nothing more about it.** For Guild the group's folder is
just a directory in which a session can also be started, and **whose files
are shown but not indexed**: the Explorer shows its tree — the member
repositories as ordinary subdirectories, `AGENTS.md` and everything else —
while no file index, no symbol index and no language server runs over it,
and it has no Changes and no worktrees of its own. A file an agent's diff
names inside a member opens in that member's root. Whether the folder holds an
`AGENTS.md`, a `CLAUDE.md`, documents or skills is the person's business:
**Guild does not write, generate, read or check `AGENTS.md` or `CLAUDE.md`
there or anywhere else.**

The intended layout mirrors Guild's column: the group's repositories live
directly inside the group's folder —

```
acme/            the group "acme", bound to this folder
  backend/       a member's main checkout
  backend-fix/   its linked worktree: ../<repo>-<name> lands here too
  frontend/
  helm/
```

— so a session started in `acme/` sees every member as a subdirectory, in
every harness, with no per-harness flags. Where the disk does not match yet,
Guild says so (the match indicator below); making it match is GLD-056.

In the column the folder is **the first row under the group header**, named
by its branch when it is a git repository (`main`) and by its folder name
otherwise, with its session tabs under it as a worktree row has them. Beside
it, the group header gains what a project row already has: `…` (its menu,
which binds the folder) and `+` (which adds projects to the group through a
picker, besides drag and drop).

An agent reaches the same deed through `guild session new --group NAME`,
the door beside the window's `+`.

**Design first.** The header's `…` and `+`, the picker, the folder's row in
the column, the match indicator and the bind question change what a person
sees: they are drawn as a Penpot proposal from the Library's components and
tokens (`docs/penpot.md`) and approved by the person before any code.

## Decided with the person (2026-11-23)

1. **The folder suggestion.** The direct parent of the members' main
   checkouts is suggested only when **every** member has the same one and it
   is neither the home directory nor the filesystem root; otherwise nothing
   is suggested. The person may always pick another existing directory.
2. **Where the group folder's sessions are drawn.** The folder is the first
   row under the group header — named by its branch when it is a git
   repository (`main`), by its folder name otherwise — with its session tabs
   under it as a worktree row has them. The header keeps collapse on its
   click. The alternative of the header itself being the folder is dropped,
   and with it the question of moving collapse to the chevron.
3. **Deleting a group** — by hand, or because its last member left
   (`crates/session/src/project_group.rs:4-8`). The folder stays on disk,
   untouched; Guild closes it as a root of the workspace as it closes any
   root.
4. **Renaming a group.** The folder is not renamed; the group keeps the
   path, not the name.
5. **A repository in groups of two workspaces bound to different folders.**
   Acceptable. Groups are per workspace, so the repository can sit inside at
   most one of the folders; the indicator honestly shows the mismatch in the
   other. No special rule.
6. **A git group folder holding the members as untracked nested
   repositories.** Guild does nothing and writes nothing; a `.gitignore` is
   the person's.
7. **What runs for a group root — shown, not indexed.** *Revised by
   decision 9 the same day:* the Explorer **does** show the group folder's
   tree, lazily, as it shows any root. What stays off is indexing and
   language: no file index, no symbol index (so Search Everywhere, Go to
   File and Go to Symbol have no index of the group root), no language
   servers. This **narrows** the earlier line "if the folder is a git
   repository it behaves like any repository root (branch, worktrees,
   Changes)": what remains of git is the row's label from answer 2, read
   with the one cheap read it needs — `git::branch`, a file read of `HEAD`
   and no process (see Context). No Changes panel and no worktrees of the
   group folder in this task. What keeps the shown tree fresh — the watcher
   yes, the git status worker no — is a proposal below, from the code.
8. **`guild session new --group NAME`** — in this task: the agent's door to
   start a session in the group's folder, beside the window's `+`. A door to
   the same deed as the window's gesture (AGENTS.md: "`guild` is a door,
   never a second way"), with the same provenance rules as `session new`.
   How NAME resolves is written as acceptance below, from the code.
9. **What the panels show with the group folder's row in focus.** The
   person: "show the nested directories, AGENTS.md and all the other files;
   just don't index." The Explorer shows the group folder's whole tree —
   every file and directory, the members as ordinary subdirectories —
   lazily, as for any root. The Changes panel stays out (decision 7).
   Search Everywhere with the group row in focus has no results from the
   group root's own index, because there is none; nothing else about it is
   open.
   *Proposal, from the code, on what keeps the tree fresh:* the tree is
   refreshed only from the root's watcher batches
   (`root/watch.rs:265-280`), so the group root **keeps its file watcher**;
   it **gets no git status worker**, which feeds only the tree's git marks,
   the Changes panel and the worktree list (`root/git.rs:1019-1039`,
   `view/tree.rs:165`), so the group tree is drawn without git marks. The
   watcher's cost is one more recursive FSEvents stream over every member's
   subtree, filtered only by the group folder's own `.gitignore`
   (`watch.rs:163-174`) — so a member's `target/` or `node_modules/` churn
   reaches the window as batches, coalesced to 100 ms (`watch.rs:28`); a
   batch reloads only directories already loaded in the tree
   (`tree.rs:367-372`). **Not measured.**
10. **`openDiff` (and `openFile`) from a group session.** The tab opens in
    the root of the member repository the file belongs to, as if opened
    from that repository's Explorer, so its language server and git markers
    work there. A file inside the group folder but outside every member —
    its `AGENTS.md`, say — opens in the group folder's root. Guild offers
    no `openFile` tool today (`ide.rs:145-210` lists `openDiff` and no
    `openFile`); this task does not add one, and the rule is written for
    `openDiff` and for whatever opens a file by path the same way.
11. **A member's file clicked in the group folder's Explorer** opens in that
    member repository's root — the same rule as decision 10 — so its
    language server, git markers and go-to-definition work there. A file
    outside every member (`AGENTS.md` and the like) opens in the group
    root. The rule reaches `tree_file_click` (`root/tree.rs:1499-1541`) and
    `open_file_from` (`root/files.rs:711-724`), which today opens the path
    relative to `active_project` (`root/files.rs:733`), i.e. the group root.
12. **No library or dependency asking for a group root.** Its tree is read
    without `ensure_libraries` and `ask_dependencies`
    (`root/tree.rs:410-456`): they serve language tooling, which is off for
    a group root.

## The approved design (Penpot, 2026-11-23)

The proposal is on Guild — Screens' *Proposals* page, boards named
`Group sessions — …`; the person approved it with these answers:

13. **The folder's row** wears a folder icon always — git or not — like
    the header's; its label is the branch for a git folder, the folder's
    name otherwise. Its second line is the match line (14). On hover it
    shows *Copy Path* and `+` (a new session in the folder), as a worktree
    row shows its actions; its session tabs hang under it as a worktree's do.
14. **The match indicator is in the folder's row**: its second line says
    `3 projects inside` in `text.faint` when every member's main checkout is
    a direct child of the folder, and `2 of 3 inside` in `accent.brass`
    when not. A member outside the folder wears `outside` in `accent.brass`
    after its name on its own head. The header is unchanged.
15. **The header** gains the project row's `…` and `+`, same glyphs and
    measures. Its menu: *New Session* (*New Session…* without a folder,
    which asks for the folder first), *Bind to Folder…* / *Change Folder…*,
    then today's *Rename Group…* and *Delete Group*.
16. **The header's `+`** opens a menu in the manner of *Move to Group ▸*:
    the workspace's projects not in the group, each with its letter tile,
    a member of another group with `in <group>` beside it; a click adds
    that one project and closes the menu.
17. **The bind question** is Guild's own popup, in the merge popup's
    anatomy: `Bind <group> to a folder`, `Its sessions will start in this
    folder.`, a path field holding the suggestion (decision 1) or empty
    with `The projects are in different folders.` under it when there is
    none, a folder button in the field that opens the system's folder
    panel, and *Cancel* / *Bind*. *Bind* is off while the field is empty.

Found in the code while implementing, decided here before the code that
depends on it:

18. **A folder root closes as any root closes.** Deleting a group from its
    menu, or changing its folder, asks to close the old folder's root as
    *Close Project* asks. When a group goes because its last member left —
    a deed with no sheet of its own — its folder's root is closed at once
    if nothing in it would be lost (no live session, no unsaved file), and
    is otherwise left open as an ordinary root for the person to close.
19. **A folder that is one of the group's own roots is refused** — a
    member's checkout or worktree cannot be the group's folder, or the
    group root would be a member of its own group.
20. **The folder's row is drawn from the binding, open or not.** Bound and
    not open — after the person closed it some other way — it is drawn as
    an unopened worktree is, and opened on its click or its `+`.

21. **The folder's `.guild/` is Guild's own state, as in every root** — its
    sessions, tabs and history, which the restart acceptance needs. It is
    the one thing Guild writes in the folder; nothing of git, no
    `.gitignore`, no `AGENTS.md` or `CLAUDE.md`. Found in the window check.
22. **A member outside the folder gives its chevron's room to `outside`**,
    open or folded: with `…` and `+` on the head the note left a short name
    truncated — folded, to nothing but `…`. A folded member's missing rows
    say it is folded. Found in the window checks.

23. **Being a group's folder is a directory's role, not a seat's.** A root
    is one directory with one status worker, index and server set, shared by
    every workspace that holds it; a directory bound as a group's folder is
    shown, not indexed, wherever it is open. To keep one directory from
    being both, binding refuses a root that is a member of any group in any
    workspace, a group never admits its folder's own repository, and the
    folder's root never counts as holding a member. Found in code review.
24. **A root bound while open drops what it ran as an ordinary root** — its
    indexes and their rankings, language servers and their problems, what
    its build systems said, its git status — and is watched afterwards
    without git being asked where its directories are. Its file tabs ask
    git for no line markers; auto fast-forward and the settings' repository
    identity leave it alone. Found in code review.
25. **The branch label stays a file read on the title bar's clock**, as
    decision 7 chose: `git::branch`, no process. A named pipe planted at a
    folder's `.git` or `HEAD` would block it, as it would the title bar's
    own read of the same file today; that is not this task's to change.
    Raised in code review and left as is.

26. **A group's folder is never a checkout git keeps elsewhere** — a folder
    whose `.git` is a file (a linked worktree, a submodule) is refused, as
    worktree removal, auto fast-forward and the worktree list would treat it
    as theirs. Binding also waits while a command's git write is still
    reporting through the root's worker, which binding stops. Auto
    fast-forward never writes a checkout that is, or holds, a bound folder,
    open or closed (`Reason::GroupFolder`). Found in code review, round 2.
27. **A released folder closes only with nothing in it**: no tab of the
    seat at all, no Run, no Terminal-tool shell, no unsaved file; its
    `.guild/` is kept whatever *forget on close* says. A root that stops
    being a folder and stays open is watched with git again and its file
    tabs get their markers back. Every `+` in a group's folder pins a new
    shell to the folder. Found in code review, round 2.
28. **Left as limits, from code review round 2:** an `openDiff` for a file in
    an unrelated project nested inside a group's folder lands in that
    project, the deepest root, because the IDE connection does not say
    which session asks — one token serves every `claude`; a member whose
    separate git directory is not named `.git` reports git's first worktree
    row as its checkout; no window test starts a successful group session
    (the window check did, with a stand-in agent).

29. **The header's `+` with nothing to add is disabled**: when every
    project of the workspace is already in the group it is drawn as the
    title bar's unavailable controls are — a `text.faint` glyph at the
    faint-mark opacity (`kind::FAINT`), since in Dark `text.faint` alone is
    the live glyph's `text.muted` to the eye — with no hover and no press, and its tooltip says *Every project
    of this workspace is in the group*. A project outside the group — opened,
    or a member that left — makes it live again, with its tooltip *Add
    Projects to Group*. Found in the person's window check after
    implementation: the button did nothing and said nothing.

## Acceptance

- [ ] WHEN the person has approved a Penpot proposal of the group header with
      `…` and `+`, the add-projects picker, the group folder's row as the
      first row under the header (decision 2), the match indicator and the
      bind question THEN the code follows it, and the proposal is promoted
      into the Library and Screens in the same branch.
- [ ] WHEN the person opens a group's `…` menu THEN it offers binding the
      group to a folder, or changing the folder when one is bound, besides
      Rename and Delete.
- [ ] WHEN the person binds a group whose members' main checkouts all have
      the same direct parent directory, and that parent is neither the home
      directory nor the filesystem root, THEN that parent is suggested, and
      the person may pick any other existing directory instead; the group
      keeps the chosen path.
- [ ] WHEN the members share no direct parent, or the shared parent is the
      home directory or the filesystem root, THEN nothing is suggested and
      the person picks an existing directory.
- [ ] WHEN the person picks a path that does not exist, or is not a directory
      THEN the binding is refused with a notice; **Guild creates no
      directory in this task**.
- [ ] WHEN the person asks for the first session of a group that has no
      folder THEN the bind question is asked first, and the session opens in
      the folder once it is bound.
- [ ] WHEN a group has a folder THEN that folder is a project root of the
      workspace, held apart from the group's members: it is never a member
      of its own group, does not keep the group alive, and is not drawn as an
      ungrouped repository row.
- [ ] WHEN a group has a folder THEN the column draws it as the first row
      under the group header, labelled with the folder's branch when it is a
      git repository and with the folder's name otherwise, its session tabs
      under it as under a worktree row; a click on the header still
      collapses the group.
- [ ] WHEN the group folder's row is focused and the person presses `+` (the
      tab launcher) or opens a session there THEN the session runs in the
      group's folder, with the permission mode any new session gets; nothing
      in this task adds a mode or a launch flag.
- [ ] WHEN a group session starts THEN its command line carries no
      `--add-dir`, no `--append-system-prompt` and no other host text, and
      `a_session_is_told_of_guild_only_when_guild_can_answer` still passes.
- [ ] WHEN the group folder's row is in focus THEN the Explorer shows the
      folder's tree lazily, as for any root: every file and directory in it,
      the member repositories as ordinary subdirectories, `AGENTS.md` and
      the rest; a change on disk shows up as in any root's tree (through
      the group root's watcher); the rows carry no git marks.
- [ ] WHEN a group root is open THEN no file index and no symbol index is
      started over it, no git status worker is started for it, no language
      server is started for it, and its libraries and dependencies are not
      asked (decision 12); its file watcher runs, as the shown
      tree needs — while each member root keeps all of these as
      before.
- [ ] WHEN Search Everywhere, Go to File or Go to Symbol is used with the
      group folder's row in focus THEN it has no results from the group
      root's own index, because there is none, and it starts none.
- [ ] WHEN a group session's `openDiff` names a file inside a member
      repository THEN the proposal's tab opens in that member's root — the
      deepest open root containing the file — as if opened from that
      repository's Explorer, with its language server and git markers.
      WHEN the file is inside the group folder but outside every member
      (its `AGENTS.md`, say) THEN it opens in the group folder's root.
      Today the owning root is `Root::project_of`, which takes the **first**
      root whose path is a prefix, in `desk.projects()` order, not the
      deepest (`root/lsp.rs:572-581`): with `acme/` and `acme/backend` both
      open, a member's file can land in the group root. The fix is in that
      one helper, and its other caller — a language server's definition
      jump, `root/lsp.rs:994` — is checked with it (a linked worktree
      nested inside its main checkout is the same shape).
- [ ] WHEN the person opens a file in the group folder's Explorer that lies
      inside a member repository THEN its tab opens in that member's root —
      the same deepest-root rule as `openDiff` — with the member's language
      server, git markers and go-to-definition. WHEN the file is outside
      every member (`AGENTS.md`, say) THEN it opens in the group root. The
      rule is applied where the tree's click lands: `tree_file_click`
      (`root/tree.rs:1499-1541`) and `open_file_from`
      (`root/files.rs:711-724`), which today takes the path relative to
      `active_project` (`root/files.rs:733`).
- [ ] WHEN the group's folder is a git repository THEN its row's label is
      the branch `git::branch` reads, and nothing else of git runs for it: no
      status walk, no Changes, no worktrees. Guild never runs `git init`
      there and writes nothing there — a `.gitignore` is the person's.
- [ ] WHEN the group's folder is not a git repository THEN it behaves as any
      non-git root: its sessions, history and tab restore work, and its row
      is labelled with the folder's name.
- [ ] WHEN Guild is restarted THEN the binding, the folder's sessions, their
      history and tab restore come back as for any other project root.
- [ ] WHEN the person deletes a group, or its last member leaves THEN the
      group's folder root is closed from the workspace as any root is
      closed, and the folder on disk is left untouched.
- [ ] WHEN the person renames a group THEN the binding keeps the same path
      and nothing is renamed on disk.
- [ ] WHEN the person presses `+` on a group header THEN a picker lists the
      workspace's projects that are not in this group (members of another
      group included, marked with that group), and taking one moves it into
      this group through the same deed as *Move to Group*. Drag and drop onto
      the header keeps working unchanged.
- [ ] WHEN a group has a folder THEN the column shows whether the group
      matches the filesystem — every member's main checkout is a direct child
      of the group's folder — and, when it does not, which members do not.
      The indicator only reads; it never changes anything on disk. A
      repository grouped in two workspaces bound to different folders shows
      the mismatch where it is not inside the folder; there is no special
      rule.
- [ ] WHEN `guild group check [GROUP] --json` is run (shape is a proposal)
      THEN it answers the group's folder and, per member, its main checkout
      and whether it matches — the same read the window's indicator does,
      through `root/command.rs`, with no second implementation in the CLI.
- [ ] WHEN `guild session new --group NAME` is run THEN the window starts a
      session in that group's folder through the same deed the window's `+`
      on the folder's row calls — the folder opened as the group's root,
      never as an ungrouped repository — and answers as `session new` does
      (`session started`, the session and the folder). `--prompt` /
      `--from-file`, `--front` and `--harness` mean what they mean on
      `session new`, and the provenance rule is the same: the caller is
      `GUILD_SESSION`, a task may not start a task
      (`guild_session::command::refuses_session`, the window's
      `TASK_FROM_A_TASK`), and a session opens behind the person's work
      unless `--front`.
- [ ] WHEN NAME is resolved THEN the window looks among the groups of each
      workspace by name (trimmed, exactly as shown): with `--workspace` only
      that workspace's groups; otherwise, the one workspace holding a group
      of that name; among several such workspaces, the caller's; and
      otherwise it refuses naming the workspaces and saying to pass
      `--workspace` — the rule `workspaces::workspace_for` already applies to
      a repository. A name no workspace holds is refused by name.
- [ ] WHEN two groups of the resolved workspace share NAME (names are not
      unique: neither `create_project_group` nor `rename_project_group`
      checks) THEN the command is refused as ambiguous and starts nothing.
- [ ] WHEN the resolved group has no folder THEN the command is refused
      saying the group has no folder and that it is bound from the group's
      `…` menu; **the door never asks the bind question and never binds**.
      WHEN the bound folder is not a directory any more THEN it is refused as
      `session new` refuses a missing folder.
- [ ] WHEN `--group` is given together with `--worktree` THEN the CLI
      refuses locally with the usage, sending nothing.
- [ ] WHEN an older window receives the group request THEN it refuses it by
      name rather than starting a session somewhere else — which is why the
      request is a variant of its own (`SessionNewInGroup`, name a proposal)
      and not an optional field on `SessionNew`: `Command` does not deny
      unknown fields, so an old window would drop the field and start the
      session in `worktree`.
- [ ] WHEN the decisions above are implemented THEN anything found to
      contradict them is written into this file before the code that
      depends on it.
- [ ] The implementing branch adds a `### Added` line to `CHANGELOG.md`
      under `## [Unreleased]`.

## Checks

- Tests first, in the session crate: the folder kept by `ProjectGroup` and
  restored from state (an old state file without it still loads); the
  suggestion worked out (one direct parent, different parents, the home
  directory, the filesystem root); the folder root held apart from the
  members — not a member, not keeping the group alive when the last member
  leaves; rename keeps the path; the match per member (inside, outside, a
  nested deeper path, a symlinked path); the wire shape of the group
  `session new` request and its round trip.
- Desktop tests in `tests/all/project_groups.rs`: the group's menu entries
  with and without a folder; `+`'s picker lists exactly the projects not in
  the group; taking a row moves it as *Move to Group* does; deleting a group
  closes its folder root and leaves the directory on disk.
- **No indexer and no language server for a group root.** A test that opens
  a group root beside a member root and asserts the group root has no file
  index, no symbol index, no git status worker and no language server —
  even after a Go to File or Go to Symbol asked with it in front — while
  its Explorer tree is read and its watcher runs; the member root has its
  watcher and status worker as before. These maps are `Root`'s own, so the
  test most likely lives in `root`'s module tests rather than in
  `tests/all/` — the implementer's call, with a note why. This replaces the
  earlier "measure the file index and the watcher": the index does not
  run.
- The group tree stays fresh: a file written into a member's subdirectory
  and into the group folder itself appears in the expanded group tree.
- `openDiff`'s owning root, in a test of `project_of`'s rule: a member's
  file lands in the member with the group root open first in
  `desk.projects()`; the group's `AGENTS.md` lands in the group root; a
  linked worktree nested in its main checkout lands in the worktree.
- A tree click in the group root's Explorer: a member's file opens in the
  member's root, the group's `AGENTS.md` in the group root.
- Not measured, and worth a look in the window check: the group watcher's
  batch traffic while a member builds (`target/`, `node_modules/` are not
  ignored by the group folder's own `.gitignore`).
- The group folder's row label: the branch for a git folder, the folder
  name for a non-git one, read by `git::branch` with no git process.
- `guild session new --group` against the window's command handling: one
  workspace, several workspaces with `--workspace` and with a caller, a
  name in several workspaces and no caller (refused), two groups of one
  name (refused), a group with no folder (refused), a task asking for a
  task (refused).
- CLI tests in `crates/cli/tests/cli.rs` against the socket stand-in: the
  `group check` request and its JSON answer; `session new --group NAME`
  sends the group variant with `GUILD_SESSION` as the caller; `--group`
  with `--worktree` is refused locally.
- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`):
  the root tests can overwrite `~/.config/guild` otherwise.
- A window check in an isolated dev window with a group of two or three
  scratch repositories laid out both ways (inside the folder and beside it):
  `…`, the bind question and its suggestion, `+`'s picker, the folder's row
  and its label (git and non-git folder), its Explorer tree with the
  members and `AGENTS.md`, a session started there listing the members,
  the indicator, deleting and renaming a group, a restart —
  compared with the approved proposal. Needs a person at the machine.
- Not run: a live provider session. That a session in the folder can read a
  member's file is a property of the harness and the directory layout, not
  of Guild; a live check is a provider call and only with the person's
  permission.

## Limits / non-goals

- **Nothing about `AGENTS.md` or `CLAUDE.md`** — no file written, generated,
  read or checked, in the group's folder or in any repository.
- **No moving of repositories on disk** — GLD-056 does it, from the group's
  `…` menu.
- **No creating the folder** — it comes with GLD-056's move.
- **No deleting or renaming the folder** — deleting a group closes the root
  and leaves the folder; renaming a group leaves the path.
- **No `git init`** in the group's folder; making it a repository is the
  person's choice. **No `.gitignore` or other write** there either.
- **A group root is shown, not indexed:** its Explorer tree and its
  watcher run; no file index, no symbol index, no Search Everywhere index
  over it, no language servers, no libraries or dependencies asked, no git
  status worker (so no git marks in its tree).
- **No `openFile` tool** is added; decisions 10 and 11 cover `openDiff`, the
  Explorer's click and whatever opens a file by path the same way.
- **No Changes panel and no worktrees of the group folder** in this task,
  even when it is a git repository; only its branch is read, for the row's
  label. Either is a task of its own if it is ever wanted.
- **The door never binds.** `guild session new --group` refuses a group with
  no folder; binding is the window's `…` menu.
- **No special rule for a repository in groups of two workspaces** bound to
  different folders; the indicator shows the mismatch.
- **No cross-repository "feature"** — a set of worktrees with one branch in
  every repository. Considered in GLD-051 and rejected as too heavy.
- **No symlinks** to the repositories inside the group's folder: search and
  git behave differently through them, and a file gets two paths.
- **Not in Guild's state directory** (`~/.config/guild`): a hidden path the
  person could not use as their own place.
- **No special launch mode**, no `--add-dir` or other per-harness flags, no
  host text in the system prompt.

## Context

Verified in the code on 2026-11-23:

- `crates/session/src/project_group.rs:56-66` — `ProjectGroup` keeps `id`,
  `name`, `repositories` (keys) and `collapsed`; the folder is a new field
  here, `#[serde(default)]` so an old state file loads. A group is a
  workspace's and exists only while it holds a repository (`:1-19`, `forget`
  at `:89-98`).
- `crates/session/src/project_group.rs:70-72` — `repository_key`: the common
  git directory, or the directory itself outside git. A group folder opened
  as a root gets a key too, which is why it must be held apart from the
  members.
- `crates/session/src/project_group.rs:116-135`, `:365-388` — creating and
  renaming a group check only that the name is not blank: **two groups of a
  workspace may share a name**, which is why the door refuses an ambiguous
  NAME.
- `crates/session/src/project_group.rs:145-178`, `:365-388`, `:390-398`,
  `:401-411` — move to a group (the deed `+`'s picker reuses), rename,
  delete (returns repositories, closes nothing — the folder root's closing
  is new), toggle.
- `crates/session/src/desk.rs:56-77` — `Project`: `path`, `name`,
  `repository: Option<PathBuf>` — `None` outside git, so non-git roots are a
  supported shape. `crates/desktop/src/repository_settings.rs:10-52` —
  `resolve_repository` answers `git: false` with the root as key for a
  directory with no `.git` up its ancestors.
- `crates/session/src/workspace.rs:97-137` — `Workspace`: `projects`,
  `worktree_order`, `pinned_worktrees`, `unfolded_worktree_tabs`,
  `project_groups` (`:127`), `front`.
- `crates/desktop/src/view/column/project_groups.rs:20` —
  `render_project_group_head`; its click collapses (`:191-194`), the right
  button notes the menu target (`:195-200`); it has a count and a chevron but
  no `…` or `+`.
- `crates/desktop/src/view/column.rs:1300-1331` — a project row's `…`
  (`project-header-menu`, opens the same menu as the right button) and
  `:1333-1360` its `+` (`project-header-new`, `new_worktree_here`). On a
  project row `+` makes a worktree; on a group header it adds projects — the
  same glyph meaning a different deed, which the Penpot proposal should look
  at.
- `crates/desktop/src/view/menu/column.rs:51-74` — the group's menu: Rename,
  Delete (`ProjectGroupDeed`); dispatched at `:315-327`; their icons in
  `crates/desktop/src/view/menu/mod.rs:915-916`.
- `crates/desktop/src/root/column.rs:540-588` — the window's group deeds:
  toggle, move a repository (`move_repository_to_group`), delete.
- `crates/desktop/src/root/picker.rs:66-92` — `Holds`, the one popup slot's
  lists; `+`'s picker is a new variant there (or a menu in the manner of
  `move_to_group_entries`, `view/menu/column.rs:79`) — the proposal decides.
- `crates/desktop/src/root/projects.rs:17`, `:168` — opening a root in a
  workspace and selecting it; `crates/desktop/src/root/tab_launcher.rs:286`
  — `+` opens in `self.active_project`, so a focused group root needs no
  change there (not traced further).
- **Worktrees land in the group's folder.** `crates/desktop/src/column.rs:375-385`
  — `folder_for(main, name)` is `main.with_file_name("<repo>-<name>")`, a
  sibling of the main checkout, so with `acme/backend` a new worktree is
  `acme/backend-<name>`. **Except** where the repository's settings name a
  `worktree_directory`: `crates/desktop/src/repository_settings.rs:54-75`
  then puts it under that directory instead
  (`crates/session/src/repository_settings.rs:27-28`).

**What starts for a root, and what a group root keeps** (decisions 7
and 9):

- File watcher: `crates/desktop/src/root/watch.rs:208-222` starts a
  `Watcher` for **every** root in `desk.projects()`, on the batch clock.
  It is the tree's only source of freshness: `apply_batch`
  (`root/watch.rs:265-280`) calls `tree.refresh(dir)` for each directory a
  batch names, and `Tree::refresh` reloads a directory only if it is
  already loaded (`crates/desktop/src/tree.rs:367-372`). The watcher is
  `notify` in `RecursiveMode::Recursive` (`crates/desktop/src/watch.rs:293`),
  dropping events by the root's **own** `.gitignore` only
  (`watch.rs:163-174`) and coalescing for `LATENCY`, 100 ms (`watch.rs:28`).
  **A group root keeps it**; its cost over the members' subtrees is not
  measured.
- git status worker: `crates/desktop/src/root/git.rs:1019-1039`
  (`poll_git`) starts a `StatusWorker` for every root likewise — the
  source of the tree's git marks (`git_status`, read at
  `crates/desktop/src/view/tree.rs:165`), the Changes panel and the
  worktree list. **A group root gets none**; its tree has no marks.
- File index: `crates/desktop/src/root/file_index.rs:291-295` starts an
  `Indexer` on demand, when a project's files are ranked; symbols:
  `crates/desktop/src/root/symbols.rs:528-533` a `Tagger` likewise. **A
  group root gets neither**, however it is asked.
- Explorer tree: `crates/desktop/src/root/tree.rs:410-456`
  (`ensure_active_tree`) reads the active project's tree the first time the
  panel shows it, and asks its libraries (`root/library.rs:76`) and its
  dependencies (`root/dependencies.rs:90`, build systems asked on a
  thread) too. **A group root's tree is read; its libraries and
  dependencies are not asked** (decision 12): they serve language tooling,
  which is off for it; `Stack::of` would usually find no build file at a
  group folder's top anyway (`root/dependencies.rs:100`).
- Language servers: `crates/desktop/src/root/lsp.rs:64-80` —
  `ensure_server`, gated by `lsp_permitted`. The group root's exclusion is
  decided there, not by writing the person's `LspPolicy`
  (`crates/session/src/repository_settings.rs:9-18`, `:38-52`).
- **Which root owns a path.** `crates/desktop/src/root/lsp.rs:572-581` —
  `project_of` answers the **first** root in `desk.projects()` whose path
  is a prefix, not the deepest. Its callers: `openDiff`
  (`crates/desktop/src/root/ide.rs:340`, `open_proposal`, which then
  adopts the tab in that root's seat, `:520-560`) and a language server's
  definition jump (`root/lsp.rs:994`). With a group root and its members
  both open, the first match can be the group root, so decision 10 needs
  the deepest match. `ide::tools()` (`crates/desktop/src/ide.rs:145-210`)
  offers `openDiff` and no `openFile`.
- **The label's read.** `crates/desktop/src/git.rs:1499-1502` —
  `git::branch(dir)` reads `HEAD` from the git directory as a file, with no
  git process; it is what the title bar's branch uses
  (`crates/desktop/src/lib.rs:1909-1928`, re-read every `BRANCH_PROBE` and
  when the project changes). It walks `dir`'s ancestors as git's own
  discovery does (`git.rs:1504-1515`, decision 18 of M5.11), so a group
  folder inside an outer repository is labelled with that repository's
  branch — as any root is in the title bar. Not verified: on what clock the
  row's label is re-read; the title bar's two seconds is the obvious one.

**The `session new` door, which `--group` follows.**

- `crates/session/src/command.rs:139-160` — `Command::SessionNew`:
  `worktree` (required), `prompt`, `caller`, `front`, `workspace`,
  `harness`. `crates/session/src/command.rs:12-14` — a command the window
  does not know is refused by name; `Command` is tagged with no
  `deny_unknown_fields` (`:20-22`), so a new optional field would be dropped
  silently by an old window.
- `crates/session/src/command.rs:556-561` — `refuses_session(prompt,
  caller_is_task)`: a task may not start a task.
- `crates/cli/src/main.rs:335-358` — the CLI's `session new`: the caller from
  `GUILD_SESSION` (`guild_session::SESSION_VAR`), the worktree from
  `--worktree` or the current directory; flags parsed at `:60-90`; usage at
  `:1127`.
- `crates/desktop/src/root/command.rs:112-128` dispatches it to
  `command_session_new` (`:680-771`): provenance first, then the launch
  selection, the folder's existence (`there is no folder at …`), the
  OpenCode floor, the workspace (`command_workspace_for`, `:1183-1202`), the
  root opened with `desk.open_in`, the tab opened with
  `open_tab_with_selection` behind the person's work unless `front`, and a
  task marked. **The group door must not reuse `desk.open_in` as is**: that
  would open the folder as an ordinary root. It opens the folder through the
  same function the window's bind and first group session use, and then
  shares the rest.
- `crates/desktop/src/workspaces.rs:28-72` — `workspace_for`: asked; the
  one holder; among several the caller's; otherwise refused with "pass
  --workspace". The NAME rule above is this rule with "workspaces holding a
  group of that name" as the holders; for no holder it refuses instead of
  taking the caller's or the front workspace.
- `crates/cli/tests/cli.rs:632-662` — `session_new_sends_the_task_and_who_is_asking`,
  the shape the new CLI test follows.

**Guild injects nothing into the system prompt.**
`crates/desktop/src/terminal/view/mod.rs:2406-2437` —
`a_session_is_told_of_guild_only_when_guild_can_answer` pins that no
`--append-system-prompt` is passed; this task does not touch
`harness_command` (`:1546`).

`crates/desktop/src/i18n/mod.rs:662-668` — the group's texts.

Verified with git 2.50.1 on scratch repositories on 2026-11-23: in a folder
that is itself a repository, a nested member repository and its linked
worktree are listed by `git status --porcelain=v2` as `? backend/` and
`? backend-feat/`; `git add .` warns *adding embedded git repository* and
stages both as mode `160000` gitlinks. Guild shows none of it (no status
worker for a group root) and writes nothing (decision 6).

**Why `--add-dir` was dropped.** The earlier version of this task gave
Claude Code each member with `--add-dir` and noted Codex's `--add-dir`
(writable, not read-only) and OpenCode's lack of one. With the members inside
the folder, every harness sees them as subdirectories, so no per-harness flag
is needed and none is passed.

Not verified: how a worktree row's click puts a root in focus (the path from
the row to `active_project`); how the Changes panel and Search Everywhere
behave today when the active root has no status worker or index; the group
watcher's cost while a member builds; whether a tab opened in a member's
root from the group (decisions 10 and 11) brings the member's row to the
front or stays behind the group row — `open_proposal_tab` adopts the tab in
the owner's seat and sets `active_tab` without selecting the project
(`root/ide.rs:520-560`), not traced further; the clock of the row label's
re-read.

Related: GLD-051 (the workflow); GLD-056 (moving the repositories into the
folder); `docs/penpot.md`; `AGENTS.md` "How work is done" (Penpot first, the
changelog line) and "`guild` is a door, never a second way". `CHANGELOG.md`
is not in `touches` on purpose: `tasks/README.md` keeps a changelog out of
it.
