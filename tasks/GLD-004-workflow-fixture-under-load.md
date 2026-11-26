---
id: GLD-004
title: Native workflow fixture tests that fail under load
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/workflow/native.rs
  - crates/desktop/src/root/workflow/native/tests/**
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-07"
---

## Goal

A heavy native-workflow test failed during the OSC 7501 stage at a load
average of 11–18, on its 5-second deadline, and passed on rerun. A red
`just check` that is green on rerun teaches everyone to rerun. Find the test,
reproduce the failure under load, and make the test wait for the event it
means rather than for wall-clock time.

The record of the failure did not keep the test's name. The candidates are
the polling loops with a fixed `Duration::from_secs(5)` in
`root/workflow/native.rs` (`:529`, `:1039`, `:1196`, `:1506`, `:1583`,
`:1834`, `:1931`) and `root/workflow/native/tests/{correction,reconciliation}.rs`.

## Acceptance

- [ ] WHEN the workflow tests run under a reproducible load (e.g. `just test`
      beside a CPU-bound load of twice the core count) THEN the failing test
      is identified by name and the failure reproduced, with the run recorded
      in the commit body.
- [ ] WHEN the cause is a wait on time rather than on the fixture's event
      THEN the test waits on the event (a channel, a state the fixture reports)
      and keeps a generous outer limit only as a hang guard.
- [ ] WHEN the cause is a real ordering bug in `native.rs` THEN it is fixed
      with a test that fails for that reason first.
- [ ] WHEN the identified tests run 20 times in a row under the same load
      THEN they pass every time.
- [ ] If the failure cannot be reproduced, the task ends with what was tried
      recorded and no blind deadline increase.

## Checks

- The load reproduction above, before and after.
- `just check`.

## Limits / non-goals

- Raising the deadline alone is not a fix.
- No change to workflow behaviour beyond what a found bug requires.
- `docs/traps.md` gets a trap only if the cause is one others will meet.

## Context

- The same pattern fixed once: `cf66f5aa` (an exit test that waited on
  `exit_status` and raced under load now waits for the reader's `Exited`).
- `docs/traps.md` — tests that fail only under load (`set_current_dir`).
