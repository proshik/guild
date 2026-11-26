---
id: GLD-062
title: Status reads bounded in memory and time, and a decision on hooks and filters they run
kind: fix
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/diff.rs
  - crates/desktop/src/file_operations.rs
  - crates/desktop/src/git/read_budget.rs
  - crates/desktop/src/root/git.rs
  - crates/desktop/src/git/hunk.rs
  - crates/desktop/src/file_tab/git_markers.rs
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-24"
needs_person: true
---

## Goal

Five findings reported by code review of GLD-026, round 5, and **not
reproduced yet**. They were left out of GLD-026: they predate its last
round, or they go beyond decision 55's narrowed promise. That promise:
a hunk write changes exactly the chosen hunk or nothing, every hunk call
and the settling status walk are bounded in time, nothing is fetched, and a
converted file takes only whole-file Stage and Unstage.

When this task is finished:

- the window's thread never reads more of a working file than a diff can
  show;
- a settling map is never held back by permits other repositories hold;
- the person has decided whether Guild's status reads should avoid hooks and
  other files' clean filters, and that decision is recorded and carried out;
- a wholly staged block of a file with a pass-through filter is outlined.

Each finding is reproduced first. A finding that does not reproduce is
written down as not reproduced, with how it was tried, and is not fixed.

## Acceptance

- [ ] **Oversized working file, whole-tree refresh** (`bugs+impl-4`).
      Setup: a working-tree diff is open with a small modified file, and the
      file is replaced by a sparse file far past `DIFF_LIMIT`.
      - First, reproduce through the production root poll: the refresh reads
        the whole file on the window's thread.
      - WHEN fixed THEN every working-side read of a whole-tree status
        refresh or a watcher refresh takes at most `DIFF_LIMIT` + 1 bytes,
        and the file shows as too large.
- [ ] **Shared read permits** (`bugs+impl-3`).
      Setup: ordinary git reads hold all four shared read permits, in this
      or another repository.
      - First, reproduce: a held commit waits on a settlement map that never
        comes, and later asks of that git thread queue behind it.
      - WHEN fixed THEN taking a permit for a settlement read has a deadline,
        so do the repository-root lookup and every other read the
        settlement needs, and the settlement map is published apart from the
        branch-source refresh. A settlement past its deadline answers a
        failed map, as GLD-026's barrier expects.
- [ ] **Hooks and clean filters during status** (`bugs+impl-1`,
      `docs+tests-1`). This is an open question for the person, not a
      settled fix.
      - First, reproduce two things:
        - a `post-index-change` hook run by `apply --cached` and by the
          settling status walk;
        - another tracked file's clean or process filter run by the settling
          status when that file's stat changed and its size did not.
      - Then put the choice to the person: should Guild's status reads, and
        hunk writes, avoid them?
        - If yes: disable hooks for those calls, disable optional index
          writes during the walk, and use a read that cannot invoke filters.
          A walk whose ticks cannot be decided under that constraint answers
          a failed map.
        - If no: record that they run, as for every git command Guild
          starts.
      - WHEN the person has decided THEN decision 55 or a new decision says
        which, and a test pins the chosen behaviour with an observable hook
        and filter fixture.
- [ ] **Marker outline under a pass-through filter** (`bugs+impl-6`).
      Setup: a tracked file with a pass-through clean filter whose staged and
      working texts are the same and differ from `HEAD`.
      - First, reproduce: the gutter draws a filled marker.
      - WHEN fixed THEN the block is outlined, because its staged state is
        computed from the `HEAD`, index and working texts. The file still
        takes only whole-file Stage and Unstage.
- [ ] WHEN the task's diff is read THEN no GLD-026 promise is weakened.

## Checks

- A failing test first for each finding that reproduces, and the same test
  passing after the fix.
- Isolated `just check`: a scratch `GUILD_STATE_DIR`,
  `GUILD_NO_GLOBAL_HOOKS=1`, and `GUILD_SOCKET`, `GUILD_SESSION`,
  `GUILD_HOOK_SOCKET` and `GUILD_USER_STATUSLINE` unset.
- A dev-window check of the marker outline only, while the person is away,
  with clicks only.
- Not run: anything needing a git older than 2.45 or a remote over the
  network.

## Limits / non-goals

- No change to GLD-026's hunk contract beyond what the person decides for
  hooks and filters.
- The window's whole-file Stage, the commit and the ordinary status walk keep
  running hooks and filters as git does, unless the person decides
  otherwise.
- No new UI without the person's decision.

## Context

Reported by the code review of GLD-026, round 5 (revust, `codex-only`).
None of it is reproduced yet.

Read from the code on 2026-11-24, branch `gld-026` at `57c22cb5`:

- `crates/desktop/src/root/diff.rs:400-415`: `working_text` reads through
  `file_operations::read_regular_file`.
- `crates/desktop/src/root/diff.rs:225`, `:304` and `:498`: the same uncapped
  wrapper, at restore, watcher refresh and unfollow.
- `crates/desktop/src/file_operations.rs:494`: the wrapper.
  `read_regular_file_up_to` beside it is the bounded form GLD-026 added for
  a refusal's re-read.
- `crates/desktop/src/git/read_budget.rs:8` and `:15-22`: four permits, taken
  with an unbounded `wait_while`.
- `crates/desktop/src/root/git.rs:819-846`: the walk.
  - `git::toplevel` comes before the bounded `git::hunk::status`.
  - The branch-source `Snapshot::read` is sent before the map.
- `crates/desktop/src/git/hunk.rs:442`: `apply --cached`.
- `crates/desktop/src/git/hunk.rs:518`: the settling `status`.
- Both run with `core.fsmonitor` empty and `GIT_NO_LAZY_FETCH=1`, and with
  hooks and filters as configured.
- `crates/desktop/src/file_tab/git_markers.rs:70`: `Offers::of` keeps the
  comparisons only when the file's eligibility is `Yes`.
- Decision 55 in `docs/decisions.md` and `tasks/GLD-026-stage-hunks.md`
  ("Code review r5") record the narrowed promise.
