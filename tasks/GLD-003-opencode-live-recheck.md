---
id: GLD-003
title: OpenCode 2.2.x live re-check after signing in to its console
kind: research
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - docs/sessions.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
needs_person: true
---

## Goal

Task 11.3 of the OSC 7501 stage stalled on OpenCode: it reported `idle` and
then `working`, but its own log said `failed to load OpenCode provider config
… OpencodeConsole.SignedOut`, so it never reached a model and never finished
a turn. Repeat the check with OpenCode signed in, to see the rest of its
lifecycle through Guild: a turn end (`idle` after `working`, since 2.2.x sends
no `done`), `blocked`, and what is left after exit.

## Acceptance

- [ ] WHEN OpenCode (2.2.x, version recorded) is signed in and given a prompt
      in a Guild session THEN `working` and then `idle` are observed, and
      `guild agent wait` ends with the finished turn and `agent send` accepts
      the next prompt (design decision 7).
- [ ] WHEN OpenCode asks for a permission THEN what it reports is recorded
      (`blocked` with `kind: permission` is expected from its source, task
      9.0).
- [ ] WHEN OpenCode exits THEN what Guild shows is recorded — its root record
      is not cleared on exit (upstream issue #60466); Guild should show no
      `working`/`blocked` after the process exit.
- [ ] As expected for a harness without `done`: no unread mark and no chime at
      the turn end — confirmed or contradicted.
- [ ] The results are a dated paragraph in `docs/sessions.md`.

## Checks

- Isolated dev window, fixture repository, the person at the machine and
  signed in to OpenCode's console themselves.
- `guild agent list --json` polled, the tab watched, `pmset` audio
  assertions for the chime.

## Limits / non-goals

- **Provider calls and the sign-in only with the person's explicit
  permission; the person signs in, not the agent.**
- No code change; a bug found becomes a new task.
- Guild's managed OpenCode startup check is pinned to 2.0.21 and fails closed
  (`docs/runtime-ownership.md` "Managed terminal recovery"); that is existing
  behaviour and not this task's subject.

## Context

- https://github.com/anomalyco/opencode/issues/60466 (root record not
  cleared on exit), #60411 (no done/error).
