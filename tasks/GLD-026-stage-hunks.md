---
id: GLD-026
title: "Stage and unstage whole hunks"
kind: feature
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/git.rs
  - crates/desktop/src/git/**
  - crates/desktop/src/changes.rs
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/root/git.rs
  - crates/desktop/src/view/changes.rs
  - crates/desktop/src/diff_tab.rs
  - crates/desktop/src/diff_tab/**
  - crates/desktop/src/view/diff.rs
  - crates/desktop/src/view/diff/**
  - crates/desktop/src/line_changes.rs
  - crates/desktop/src/file_tab/git_markers.rs
  - crates/desktop/src/file_tab/git_markers/**
  - crates/desktop/src/file_tab/gutter.rs
  - crates/desktop/src/actions/**
  - crates/desktop/src/icons/**
  - crates/desktop/assets/ui/check-partial.svg
  - crates/desktop/src/i18n/**
  - CHANGELOG.md
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Put part of a file's changes into the index: one hunk at a time, from a diff
or the editor's change markers. Today `git::Write::Stage` takes whole paths
only.

Agreed with the person on 2026-11-23, answer by answer (below), and the
Penpot proposal approved on 2026-11-24 (*Design*, below). The new write is
decision 55 in `docs/decisions.md`. Code waits for the spec review.

## Agreed behaviour

The person's answers are marked *(agreed)*; the rest are the worker's calls,
stated to the person and open to correction, marked *(proposed)*.

### Where the actions are

- *(agreed)* **The diff tab and the editor's change-marker popup** both carry
  the actions. The Changes panel carries no hunk action of its own; its rows
  change only through the tick (below).
- *(agreed)* **Stage Hunk and Unstage Hunk ship together**, symmetric.
- *(agreed)* **In the diff tab, at the change and in the toolbar.** Side by
  side, an icon at each change in the ribbon between the sides; unified, an
  icon in the gutter at the change's first row. In the toolbar, beside
  ↑/↓ and the change counter, one button acting on the current change (the
  one ↑/↓ selects) — the same in both modes.
- *(agreed)* **In all three working-tree diffs, by what the change is:**
  - **HEAD ↔ working** (what a click on a Commit row opens): a change with
    any part not in the index offers **Stage**; a change wholly in the index
    offers **Unstage**.
  - **Unstaged** (index ↔ working): **Stage** only.
  - **Staged** (HEAD ↔ index): **Unstage** only.
  - *(agreed after code review r1, replacing a proposal)* A change partly
    in the index in the HEAD diff — line endings included — offers nothing:
    its icon is disabled with the hint *Part of this change is already
    staged — stage or unstage it in the Unstaged or Staged diff*. Staging
    "the rest" could take out an index line the screen does not show.
  - No action in a commit's diff or an agent's proposal.
- *(agreed)* **The marker popup** gets Stage or Unstage beside Rollback, by
  the same rule as the HEAD diff, for the block it is open on.
- *(agreed)* **Keyboard:** two actions, *Stage Hunk* and *Unstage Hunk*, in
  the keymap and **unbound by default**: the current change in a diff tab,
  the block under the caret in an editor.

### What the editor's markers show

- *(agreed)* The markers stay HEAD ↔ buffer. **A block wholly in the index
  is drawn as an outline** of its colour instead of a fill; a block with any
  unstaged part stays filled. The popup then offers Unstage for an outlined
  block and Stage for a filled one.

### What is written

- *(agreed)* **The text on screen.** The working side of a hunk is the open
  tab's buffer when there is one — unsaved edits included — and the disk
  otherwise, which is what the diff and the markers already show. The disk is
  never written; Stage does not save the file.
- *(agreed; revised after spec review 1)* **The window computes the entry's
  new content and git swaps it in** (decision 55): `apply --cached
  --no-3way --whitespace=nowarn -` of a binary patch with a full `index
  <old>..<new> <mode>` line, so git refuses under its own
  `index.lock` unless the entry is still `<old>` and of that type. No line numbers travel.
- *(settled by the reviews)* **Which index bytes a write changes** — only
  those of the shown block:
  - Unstaged diff: Stage puts the block's working lines in place of its
    index lines.
  - Staged diff: Unstage puts HEAD's lines in place of its index lines.
  - HEAD diff and editor: the block's HEAD lines are found in the index
    through HEAD ↔ index — a staged insertion exactly at an edge lies
    outside — and the index bytes there decide, byte for byte: HEAD's bytes
    are Stage (the working bytes go in), the working bytes are Unstage
    (HEAD's go back), anything else is partly staged (above).
  - *(agreed after code review r2)* A block that differs from the index
    only in line endings is disabled with its own hint: *This change differs
    from the index only in line endings — stage the whole file*.
  - A write never joins its bytes to a kept index line; an empty HEAD range
    at a staged insertion is that insertion (so it can be unstaged).
  - Each control acts only on the offer it was drawn for; a different
    current offer, or a buffer that changed under it, makes the click stale.
  - *(agreed)* Where a staged change crosses the shown block's edge, **the
    action is disabled** with the hint *The index holds a change wider than
    this one — use the Staged or Unstaged diff*.
- *(agreed)* **Hunks are offered only where git stores the bytes as they
  are**: a regular tracked file (`100644`/`100755`) in HEAD and the index,
  one stage-0 entry, a regular working file (not a symlink, not gone), text
  within the size limit, `filter` unspecified, no `ident` and no
  `working-tree-encoding` attribute, and no line-ending conversion of the
  working text or the postimage (decided from `text`, `eol` and
  `core.autocrlf`, running nothing; previously `hash-object --path` against
  `--no-filters`, checked when the diff is computed and again before the
  write). Untracked, added, deleted, renamed, binary, symlinked, too large,
  conflicted and converted files keep whole-file Stage/Unstage; the hunk
  controls are **disabled with a hint** — for a converted file *git converts
  this file when it stores it — stage the whole file*.

### Design (Penpot, approved 2026-11-24)

Guild — Screens, page *Proposals*; every part a Library copy with its tokens:

- **Stage hunks — diff (HEAD)**: `body / diff` with a *stage change* button
  after the change counter in `diff / toolbar` (`head-plus`, the toolbar's
  button anatomy), and in the ribbon lane between the line numbers, at each
  change's first row, a 16 px icon in `text.secondary`: `head-plus` for
  Stage, `head-hide` (Lucide minus) for Unstage. No new icon for either.
- **Stage hunks — commit (partial tick)**: `dock / commit` with `index.rs`
  partial.
- **Stage hunks — editor marker popup**: `body / file` with line 13's marker
  drawn as a 1 px inner outline in its colour, and the marker popup on the
  block at rows 19–20: ↑ ↓ Rollback **Stage**, HEAD's old text under them as
  today.
- **Stage hunks — marker popup (staged block)**: the same popup for an
  outlined block: ↑ ↓ Rollback **Unstage**.
- **Stage hunks — drafts**: what the Library lacks — `checkbox
  State=Partial (draft)` and `icon / tool-revert (draft)` (the popup's
  Rollback icon, in code since `a913e43c` and never drawn). Both are promoted
  into the Library with the branch's Penpot step.
- Unified mode is not in Penpot at all; there the icon sits in the number
  gutter at the change's first row, by the same rule.
- *(agreed)* **The partial tick's dash is drawn as heavy as the tick** — a
  new `ui/check-partial.svg` (Lucide's minus at `check.svg`'s weight) on an
  empty box, `text.primary`, for files **and** for the directories that
  already show a partial tick with the thin `head-hide` today.

### The commit's ticks

- *(agreed)* **A third, partial tick.** A tracked row with changes both in
  the index and outside it shows a partial tick. Committing with a row
  partial commits **its index as it is** — `commit_plan` neither adds nor
  restores it. A click on a partial tick makes it full (the whole file, as
  today, `git add` at commit); a click on a full tick clears it (unstaged at
  commit, as today).
- *(agreed; narrowed after code review r3)* **The tick follows the index
  after a hunk write**, not the buffer: a hunk staged from an unsaved buffer
  leaves the row partial (`MM`) until the file is saved, when the partial
  tick turns full. The commit result is the same either way, since a
  commit writes dirty buffers first and a partial row commits the index.
- *(agreed after code review r2)* **After a hunk write the row's tick follows
  the index**: partial when part is staged, full when all of it, cleared when
  none — whatever the tick was. A click on the tick and edits without a hunk
  write keep the rules above.
- *(agreed after code review r3)* **A status transition alone never adds a
  path to the commit.** An excluded row stays excluded; a full row edited
  on disk or staged in part from a terminal stays full and is committed
  whole, as before GLD-026 (the person's choice); a partial tick turns full
  when nothing of the row is unstaged and clears when nothing is staged. A
  failed status read keeps every tick. A cleared or full tick does not return to
  partial by clicking — only a write that changes the index does.
- *(agreed)* **Commit waits for a hunk write in flight.** Pressed (or
  `cmd-enter`) while a hunk write of the project has no status answer yet, the
  commit is remembered, planned from the status that write produces, and then
  runs — in its own project, whichever is in front; nothing is shown but the
  delay.
- *(agreed after code review r1)* **A refused hunk write cancels the commit
  waiting for it**: no commit, the message and ticks stay, and a notice says
  *Commit not made: check the index and press Commit again* — after code
  review r3 for any cause (a refusal, a commit landing during an Unstage, a
  status read that failed), as the person chose.
- *(after code review r3)* While a hunk write is pending the Commit button
  asks only for a message; a partial row's menu offers Exclude and Include.

### When the write refuses

- *(agreed)* **Nothing is written, a notice says so, and the changes are
  recomputed** — *The file changed since its changes were shown; they are
  recomputed, try again* — whenever an identity is stale:
  - the index entry: git's own refusal under its lock (*does not match the
    current contents*);
  - the working side: the buffer snapshot (checked in the window before
    sending) or the disk bytes' hash (checked on the git thread);
  - for Unstage, HEAD's blob for the path, checked on the git thread just
    before the write.
  git's words reach the Console.
- *(agreed)* **A commit landing between the HEAD check and the write** (from
  a process outside Guild, in milliseconds) cannot be excluded by git; a
  check after the write sees it, and a warning says *A commit landed while
  the change was unstaged — check the Staged diff*, then the changes are
  recomputed.

## Acceptance

- [x] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code. *(Recorded above; Penpot approved 2026-11-24.)*
- [x] WHEN the person stages a hunk THEN only that hunk enters the index; the
      working file is unchanged.
- [x] WHEN other changes of the same file are already in the index THEN they
      are preserved.
- [x] WHEN the person unstages a hunk THEN only that hunk leaves the index;
      the working file is unchanged and other staged hunks stay.
- [x] WHEN the buffer has unsaved edits THEN the staged lines are the
      buffer's, in the file's own line endings, and the disk is not written.
- [x] WHEN the file or its index entry changed since the diff was computed
      THEN the write refuses rather than applying to the wrong lines, says so
      in a notice and recomputes.
- [x] WHEN another process changes the index entry after the window read it
      THEN git refuses the write (test: an outside `git add` between the
      window's read and the write).
- [x] WHEN identical blocks repeat in the file and lines were inserted above
      THEN the change lands on the selected block, in both directions.
- [x] WHEN HEAD's blob for the path changed since the diff THEN Unstage
      refuses; WHEN it changes during the write THEN the warning is shown.
- [x] WHEN the shown block and the index's blocks differ (a staged change
      wider than the shown one) THEN the action is disabled with its hint and
      no index line outside the shown block ever changes.
- [x] WHEN Commit is pressed while a hunk write has no status yet THEN the
      commit is planned from that write's status and takes only what was
      staged.
- [x] WHEN a file is partly staged THEN its Commit row shows a partial tick,
      and a commit with it partial commits exactly the index for that file.
- [x] WHEN a block is wholly in the index THEN its editor marker is an
      outline and its popup offers Unstage; otherwise a fill and Stage.
- [x] WHEN the file is untracked, added, deleted, renamed, binary, a
      symlink, too large or conflicted, or git converts it (`filter`,
      `ident`, `working-tree-encoding`, line endings) THEN no hunk action is
      enabled, and the write refuses a postimage git would store differently.
- [x] WHEN a hunk is staged from an unsaved buffer THEN the row is partial
      until the file is saved.
- [x] *Stage Hunk* and *Unstage Hunk* are keymap actions, unbound by default.
- [x] The new writes (`apply --cached` of a full-index binary patch) are
      decision 55 under decision 21, with its guards; the test pinning `git::Write`'s
      verbs and the destructive-spelling guard pass with it.
- [x] A partial tick, a directory's included, shows a dash as heavy as the
      tick (`ui/check-partial.svg`, in `icons::UI`'s table).
- [x] `CHANGELOG.md` has its `### Added` line under `## [Unreleased]`, and a
      `### Changed` line for the heavier directory partial tick.
- [x] Penpot brought to what was merged (`docs/penpot.md` step 6): the
      drafts promoted into the Library, the boards onto Screens, stale copies
      relinked, the proposal boards removed.

## Checks

- Tests first for each acceptance item, against a real git in a temporary
  repository for the writes; `just check` with `GUILD_STATE_DIR` isolated,
  `GUILD_NO_GLOBAL_HOOKS=1`, `GUILD_USER_STATUSLINE` unset.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn,
  against the approved Penpot boards.

## Limits / non-goals

- Selected lines are GLD-027; a full index preview is not promoted.
- No hunk staging for the files listed above; no split of a hunk.
- The Changes panel does not become git's Staged/Unstaged groups.

## Implementation notes (2026-11-24)

- Built on the agreed design; every acceptance item above is covered by a
  test against a real git where it is a write (`tests/all/git_hunk.rs`,
  `tests/all/hunk.rs`, `diff_tab/hunks/tests.rs`,
  `file_tab/git_markers/tests.rs`, the commit barrier in
  `view/menu/commit_tests.rs`, which reproduces `lenses-5` through `Root`
  with the barrier taken out).
- Checked in an isolated dev window, clicks only: the HEAD diff's icons and
  toolbar button, Stage and Unstage there and from the marker popup, the
  outlined markers, the partial tick turning full and back.
- *(proposed, matching the approved board)* A group's head counts a partial
  row as included: ticked when every row is ticked or partial; its click
  clears the group, partial rows with it.
- Covered by tests only, not in the dev window (driven without typing): the
  stale-buffer refusal; a commit landing between an Unstage's checks and
  its write — `git_hunk::head_moving_during_an_unstage_is_seen_after_the_write`
  for the write and its post-check, `an_unstage_racing_a_commit_is_said` for
  the window's notice; another root's thread committing in that instant is
  excluded by holding the checkout's turn, reasoned from the lock and not
  raced in a test.
- Penpot step 6, done on 2026-11-24 after the diff was accepted.
  - **Library**: section *Stage hunks* with `icon / check-partial`,
    `icon / tool-revert`, `diff / hunk action` (Action Stage|Unstage ×
    State Rest|Hover|Disabled) and `editor / change popup` (Action ×
    Available Yes|No). New variants in existing sets: `checkbox
    State=Partial`, `changes / row Ticked=Partial`, `editor / gutter
    Marker=Added (staged)|Modified (staged)`. `diff / toolbar` gained *stage
    change* after the counter, and `diff / body` the controls in the ribbon
    lane.
  - **Screens**: the five screens holding `body / diff` were relinked — *Main
    window — diff*, *Menu — tab launcher (+)*, *Popup — Find in Files*, *Go
    to File* and *Search Everywhere*. *Main window — commit* shows `index.rs`
    partial and line 13 outlined. Three screens are new: *Main window —
    change popup (stage)*, *— change popup (unstage)* and *— diff (a change
    partly staged)*, the last with the disabled control and its hint.
  - The proposals were removed. Tokens are 534 / 74 / 74 in both files,
    hashes `249cdbe7` / `c8726182` / `56a32fc0`, equal to the script's.
  - The Commit row's context menu (Include / Exclude on a partial row) has
    never had a Penpot screen; it is a gap that predates GLD-026, not drawn
    here.

## Code review r5 (2026-11-24)

revust, profile `codex-only`, four sources, a correction review of
`cd740d14..7b66ab19`. **The person narrowed decision 55's promise** to four
things: a hunk write changes exactly the chosen hunk or nothing; every hunk
call and the settling status walk are bounded in time; nothing is fetched;
a converted file takes only whole-file Stage and Unstage. Hooks and other
files' clean filters during status behave as everywhere in Guild — "no
configured program runs" is no longer claimed.

- Fixed here: the settling walk detects no renames (`bugs+impl-2`); a
  filter-only promisor is a partial clone (`docs+tests-3`); every walk stays
  bounded until a usable map lands (`arch+quality-2`); a dropped path's
  revision is pruned (`bugs+impl-5`); a timed-out write that cannot be read
  back says Guild could not verify it (`docs+tests-8`); a worker test of the
  unverified outcome (`docs+tests-6`); a local-only fixture probe
  (`bugs+impl-7`); the traps recipe's empty `core.fsmonitor`
  (`docs+tests-9`).
- To GLD-062: `bugs+impl-4`, `bugs+impl-3`, `bugs+impl-1`, `docs+tests-1`,
  `bugs+impl-6`.
- The parent's diff review of r5 found a regression in the first fix: with
  `--no-renames` everywhere, an excluded staged rename came back as `D old`
  and `A new`, `old` was ticked as a row never seen, and a commit held for a
  hunk took the deletion — confirmed by a test through the barrier. The
  settling walk now finds renames as the ordinary one does, and detects
  none only in a partial clone, where the split source takes the rename
  row's tick (`changes::ticks_for`); both paths are tested.

## Code review r4 (2026-11-24)

revust, profile `codex-only`, four sources. 6 major, 4 minor
and 1 pre-existing finding, all accepted and fixed; every new test was seen
failing first or with its guard removed.

- The barrier (`bugs+impl-1`, `-2`, `-3`): the take keeps every outcome and
  map in order; the settling walk is bounded, runs no monitor, fetches
  nothing, and answers a failed map past its deadline; an applied write
  that cannot be read back is unverified — the commit is cancelled, the
  tick follows the index, and the notice says so (the person's choice).
- No fetch, no monitor on any git (`bugs+impl-5`, `docs+tests-6`):
  `GIT_NO_LAZY_FETCH=1` (git 2.45.0+; an older git takes no hunks in a
  partial clone) and an empty `core.fsmonitor`, which git reads as off in
  every version from git's source — no minimum git version is needed.
- Folder scope (`bugs+impl-4`): `--ignore-submodules=none`.
- The diff's working side (`adversarial-6`, `bugs+impl-7`, pre-existing
  `bugs+impl-6`): the re-read is bounded and dropped when superseded; a
  Staged diff reads none on restore or from the watcher.
- Minor: the marker outline from what is staged (`bugs+impl-8`); the
  changelog ties a staged-only commit to the partial tick (`docs+tests-5`).

## Code review r3 (2026-11-24)

revust, profile `codex-only`, four sources. 8 major, 4 minor
and 2 pre-existing findings, all accepted and fixed; every new test was seen
failing first or with its guard removed.

- What a commit takes (`bugs+impl-1`, `-2`, `arch+quality-3`): only the
  person and a settled hunk write change ticks (the person's choice: a full
  row edited on disk stays full); a failed status read keeps them and
  cancels the held commit; `HeadMoved` cancels it too. The cancel notice is
  one text for every cause (the person's choice).
- No program, a deadline (`adversarial-5`, `-6`): every hunk git call runs
  with `core.fsmonitor=false` (an empty value after r4) and a ten-second deadline ending its process
  group with `SIGTERM`, then `SIGKILL`.
- The write carries what it was decided on (`bugs+impl-4`, `-6`): Stage
  carries `HEAD`'s bytes; a refetch reads the working side again.
- Admission (`arch+quality-5`): one write per file while it is unanswered,
  refused before the file is copied.
- Minor: offers check their own written bytes for conversion (`bugs+impl-5`);
  the button while a write is pending (`bugs+impl-3`); Exclude for partial
  rows (`bugs+impl-7`); fixtures isolated from the person's git config
  (`docs+tests-7`).
- Pre-existing: a folder's commit scope asked again in the checkout's turn
  (`arch+quality-2`); a diff's working text read without waiting on a FIFO,
  and none for the Staged diff (`docs+tests-4`).

## Code review r2 (2026-11-24)

revust, profile `codex-only`, four sources. 16 major and 6
minor findings, all accepted, fixed by class; each fix's test fails with the
guard removed (checked by mutation).

- Eligibility (`bugs+impl-5`, `-6`, `-7`, `-8`, `-11`, `adversarial-6`,
  `adversarial-11`, `bugs+impl-13`): an allow-list read from
  `check-attr --all` and `core.autocrlf`; the working file opened beneath the
  root with no link followed and no FIFO blocking, and text in every view;
  the index text held to the editor's line budget; literal `ls-tree`.
- The held commit (`arch+quality-7`, `adversarial-3`, `bugs+impl-1`,
  `arch+quality-9`): one barrier — settle, tick from the index (the person's
  choice), then plan or cancel; flights bound to their git thread.
- Mapping (`bugs+impl-2`, `-3`, `-4`, `-9`, `docs+tests-7`): no joined lines;
  staged insertions unstage; controls carry their offer; clicks check the
  buffer as serialized now; an endings-only block has its own hint (the
  person's choice).
- Minor (`bugs+impl-10`, `-12`, `docs+tests-9`, `-10`, `-11`): superseded
  facts batches skip; a refusal refreshes the sources; the mode, size and
  worker-path tests now fail without their guards.

## Code review r1 (2026-11-24)

revust, profile `codex-only`, four sources with synthesis and
verification. 17 major and 2 minor findings, all accepted; each real behaviour
got a failing test first.

- Written wider than shown (`bugs+impl-2`, `arch+quality-1`,
  `bugs+impl-8`, `adversarial-9`): one rule maps a shown block to exactly
  its index bytes (above); a partly staged block is disabled, as the person
  chose; a followed buffer reads the file's line endings at each edit.
- Stale computations (`arch+quality-3`, `arch+quality-9`,
  `adversarial-10`): offers carry the document and the facts generation;
  a superseded job stops; a file leaving a tree diff drops its state; the
  mapping is a binary search.
- Commit and ticks (`docs+tests-4`, `bugs+impl-3`, `bugs+impl-9`): staged-
  only after a change is full; a refusal cancels the waiting commit, as the
  person chose; the waiting commit runs in its own project.
- Target and preflight (`bugs+impl-1`, `bugs+impl-4`, `bugs+impl-5`,
  `adversarial-5`): the patch path from the repository's top and a read-back
  after the write; the mode on the index line; regular files everywhere,
  read without following links; the checkout's turn held across checks,
  write and post-check.
- Filters and limits (`bugs+impl-6`, `arch+quality-7`, `bugs+impl-7`,
  `arch+quality-8`): no check runs a program — `--no-filters` only, byte
  preservation from attributes and config, `filter` must be unspecified;
  sizes asked before reading; raw hashing under the reader permit.
- `docs+tests-7`: the notes no longer claim the post-write case without a
  test; both halves now have one.

## Spec review 1 (2026-11-24)

revust, profile `codex-only`, one architecture reviewer. Every finding
was checked against the code and against git 2.50.1 in scratch repositories;
all seven are real.

- `lenses-1` (index check and apply see different entries) — real: the
  checkout's turn serialises Guild's writers only. Closed by the binary
  full-index patch: git's compare-and-swap under `index.lock`.
- `lenses-2` (Unstage after a terminal commit stages an inverse) —
  reproduced. HEAD's blob joins the identity; the residual window is
  reported after the write.
- `lenses-3` (HEAD-based Unstage removes a hidden change) — reproduced: two
  replacements one line apart are one `-U3` hunk. The unit is the block;
  writes never leave it; the action is disabled where they would.
- `lenses-4` (coordinates) — reproduced with display coordinates: the
  identical block twenty lines away was changed, *Applied cleanly*. A gap in
  the spec rather than in git; the binary patch carries no coordinates.
- `lenses-5` (commit planned before a hunk write lands) — confirmed in
  `root/git.rs`: a batch is drained before its status walk. Commit waits.
- `lenses-7` (`ident`, `working-tree-encoding`) — reproduced both.
  Attributes excluded, line endings checked against git's own storing.
- `lenses-6` (dirty buffer tick) — `MM` confirmed. The tick follows status.

## Context

- `crates/desktop/src/git.rs` (`Write::Stage`, `Write::args`, `write_one`'s
  stdin file, `turn`); `root/git.rs` the worker's batch and its status walk.
- `crates/desktop/src/changes.rs` `commit_plan`: today a ticked row with an
  unstaged part is `git add`ed whole and an unticked staged row restored
  whole — which is why the partial tick is part of this task.
- `crates/desktop/src/diff_tab.rs` bases `Head`, `Index`, `Unstaged`;
  `view/diff/surface.rs` the toolbar's ↑/↓; `view/diff/center.rs` the ribbon.
- `crates/desktop/src/line_changes.rs` and `file_tab/git_markers/popup.rs`:
  HEAD ↔ buffer blocks, the popup's ↑/↓ and Rollback (`a913e43c`), which
  edits the buffer and never git.
