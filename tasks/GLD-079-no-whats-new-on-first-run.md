---
id: GLD-079
title: Do not greet a first launch with What's new
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/update.rs
  - crates/desktop/src/root/update.rs
  - crates/desktop/src/view/release_notes.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

GLD-074's check: Guild's very first launch on a new account opens *What's
new in Guild* with the nightly's changelog — merge requests, glab — before
the person has seen Guild at all. `update::greet` treats "no version seen"
as "everything is new".

When this task is finished, a launch with no `release-seen` in the state
folder and no earlier state records the running version as seen and shows
nothing; *What's new* appears from the next updated build on, as today.

**The person's decision (2026-11-27):** do not show it on the first launch.

## Acceptance

- [x] WHEN Guild starts with a state folder that has never held a Guild
      (no `release-seen`, no settings or workspace file) THEN *What's new*
      is not shown and `release-seen` records the running version.
- [x] WHEN `release-seen` is missing but the state folder holds an earlier
      Guild's files (an install from before `release-seen` existed) THEN
      the behaviour is today's: the greeting shows.
- [x] WHEN a later nightly starts THEN *What's new* shows the releases since
      the recorded version, as today.
- [x] Dev and debug builds still write and show nothing.

## Checks

- Unit tests on `greet` for the three states above, failing before the fix
  for the first one.
- `just check`, isolated.

## Plan

One step: `update::greet` tells a first launch from an install older than
`release-seen` by the desk — `workspace.json`, or the legacy
`sessions.json` — and records the version without a greeting when neither
is there; unit tests in `tests/all/update.rs` for all three states.

**A disagreement with the Acceptance's wording, recorded here:** the first
criterion names "no settings or workspace file", but `settings.json` is no
sign of an earlier Guild. `Root::from_state` saves the panels it adopts
(`Settings::adopt_panels` is true whenever no panels are stored) *before*
the `Update` field — and with it `greet` — is built, so on a brand-new
folder the file already exists when the greeting is asked. Only the desk
counts: it is written by `Root::write`, which needs a built `Root`. An
install with settings but never a desk is left ungreeted, the harmless
direction. `a_first_launch_is_not_fooled_by_its_own_settings` pins it.

## Limits / non-goals

- No welcome card or other new interface.

## Context

- GLD-074, *Findings* 2. `update.rs`, `greet` and `seen_file`.
