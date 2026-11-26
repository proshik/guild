---
id: GLD-072
title: The Sessions page test sees its two working sessions in the wrong order
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/view/column/sessions_tests.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

`view::column::sessions_tests::the_sessions_page_lists_by_urgency_settles_and_takes_keys`
failed once (in its isolated child process, so both it and the wrapper
report it) in a `just check` beside another session's tests while GLD-068
was checked:

```
panicked at crates/desktop/src/view/column/sessions_tests.rs:130:5:
waiting, then the newer working, the older, then done: [116px, 280px, 210px, 374px]
```

The rows are listed as waiting, new working, old working, done; the
newer working session (`Add a search index`, active a minute ago) was drawn
below the older one (`Review the parser`, six minutes ago). When this task
is finished, the cause is known and written down, and the order the test
checks does not depend on timing.

## Acceptance

- [ ] WHEN the task is handed over THEN the cause is recorded here with the
      evidence that established it — whether the test's times or the
      page's ordering rule is at fault.
- [ ] WHEN two working sessions differ in their last activity THEN the
      Sessions page lists the more recent first, however slowly the
      fixture runs.
- [ ] WHEN the cause is in production code THEN a regression test pins it,
      failing before the fix for the right reason; WHEN it is in the test,
      the fix is in the test.
- [ ] WHEN the fix is in THEN the test passes in at least three full
      `just check` runs beside another session's tests, and in a loop of
      the test alone beside a synthetic load.

## Checks

- The test in a loop, before and after, alone and beside a synthetic load
  (the failure has not been reproduced on purpose yet).
- `just check`, repeated, with `GUILD_STATE_DIR` in a scratch folder,
  `GUILD_NO_GLOBAL_HOOKS=1` and the `GUILD_*` session variables unset.

## Limits / non-goals

- No change to the Sessions page's sections or their order as designed.

## Context

Seen in one of six isolated `just check` runs on 2026-11-27, with no
synthetic load and another session's tests beside it (load median 17,
peak 34); the other five passed. Not verified beyond that.

The fixture (sessions_tests.rs:18) gives each session a last activity of
`now_ms() - ago` through `Desk::observe_reported_activity` while the desk
is built, and only afterwards, inside the window, calls
`r.board.report(…, Reported::Working)` for `old_work` and then `new_work`.
Leads, not checked: whether a board report or anything the window does on
its first frames replaces those activity times with the real clock, and in
what order; and what the page compares when two times tie.
