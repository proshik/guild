---
id: GLD-071
title: A commit menu test's git add meets an index.lock it did not take
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/view/menu/commit_tests.rs
  - crates/desktop/src/git.rs
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-27"
---

## Goal

`view::menu::commit_tests::a_failed_status_read_cancels_the_held_commit_and_keeps_the_ticks`
(run through `isolated_commit_menu_corrections`) failed once in a loaded
`just check` while GLD-068 was checked:

```
panicked at crates/desktop/src/view/menu/commit_tests.rs:1565:65:
called `Result::unwrap()` on an `Err` value: Failed("fatal: Unable to create
'…/T/.tmpZylacA/.git/index.lock': File exists. Another git process seems to
be running in this repository …")
```

Line 1565 is the test's own `git add b.txt` in its fixture repository.
Something else held the index lock of that repository at that moment.
When this task is finished, it is known who, the test does not race it,
and — if the holder is the window's own git reads — it is decided whether
a person's git command in a terminal can meet the same refusal.

## Acceptance

- [ ] WHEN the task is handed over THEN the process that held `index.lock`
      is identified and recorded here with the evidence.
- [ ] WHEN the holder is the fixture's own window (its status walk, its
      watcher or its git worker) THEN the test waits for that work or
      stops it before writing to the repository, and the fix is in the
      test.
- [ ] WHEN the holder is a read the window makes in production THEN the
      finding goes to the person before code: a read that takes the index
      lock can make a person's `git add` or `git commit` in Guild's
      terminal fail the same way, and changing how the window reads git is
      the person's decision, not part of a test fix.
- [ ] WHEN the fix is in THEN `isolated_commit_menu_corrections` passes in
      at least three full `just check` runs beside another session's
      tests, and in a loop of the isolated run beside a synthetic load.

## Checks

- The isolated run in a loop, before and after, alone and beside a
  synthetic load (the failure has not been reproduced on purpose yet).
- `just check`, repeated, with `GUILD_STATE_DIR` in a scratch folder,
  `GUILD_NO_GLOBAL_HOOKS=1` and the `GUILD_*` session variables unset.

## Limits / non-goals

- No new git write and no change to the twenty-one writes or their guards.
- No change to how the window reads git without the person's decision.

## Context

Seen once in six isolated `just check` runs on 2026-11-27, with eight
synthetic busy loops and another session's tests beside it (load median
51, peak 128); the other five passed. Not verified beyond that.

`staged_fixture` (commit_tests.rs:1194) mounts a `Root` on a temporary
repository and then runs `git add` and `git commit` in it through
`git::run`, while the mounted window may already be reading the same
repository. A lead, not checked: `git status` refreshes the index and takes
`index.lock` to do it unless it is run with `--no-optional-locks` or
`GIT_OPTIONAL_LOCKS=0`, and nothing under `crates/` sets either today.
