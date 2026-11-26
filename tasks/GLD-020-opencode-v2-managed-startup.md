---
id: GLD-020
title: "Qualify managed startup for OpenCode V2"
kind: research
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - docs/runtime-ownership.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
needs_person: true
---

## Goal

Guild's managed OpenCode startup is pinned to a positively inspected 2.0.21
service and fails closed otherwise (`docs/runtime-ownership.md`, managed
terminal recovery). On 2.0.21 the initial marker was accepted but execution
failed without an assistant response; the saved model (big-pickle/default)
differed from the TUI's (DeepSeek V4 Flash/high), and the real request model
and failure cause are unknown. Guild's current-runtime start, its status
plugin, same-conversation resumed delivery and receipt retry were never
qualified. The person now runs OpenCode 2.2.x.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/future-improvements.md` ("OpenCode V2 managed-startup
qualification", recorded 2026-10-26, deferred by the person); its scope and
behaviour are to be agreed with the person before any work, and recorded in
this file in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN narrowly scoped own metadata is read THEN the effective request
      model and the failure cause are identified, or recorded as still unknown
      with what was tried.
- [ ] WHEN Guild starts OpenCode for a managed continuation THEN the supported
      current-runtime plugin and the exact conversation binding are verified.
- [ ] WHEN a same-conversation startup succeeds in a targeted native check
      THEN a duplicate-request retry is shown to deliver at most once.
- [ ] WHEN the installed version is not 2.0.21 THEN what the pinned check does
      is recorded, and whether to re-pin, widen or keep it is put to the
      person.
- [ ] Results recorded in `docs/runtime-ownership.md`; no secret or raw dump
      kept.

## Checks

- Isolated dev window; provider calls only with the person's explicit
  permission for this run.

## Limits / non-goals

- No code change here; a fix found becomes its own task.
- GLD-003 checks OpenCode's OSC 7501 status; this task is about managed
  startup and may share the person's session with it.

## Context

- `docs/runtime-ownership.md` — "Managed terminal recovery".
- GLD-003.
