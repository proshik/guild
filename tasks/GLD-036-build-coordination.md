---
id: GLD-036
title: "Decide on limits for heavy commands and shared local resources"
kind: design
status: backlog
priority: 4
component: docs
depends_on: []
touches:
  - tasks/GLD-036-build-coordination.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Several agents running heavy builds at once can starve the machine. Decide
whether Guild limits explicitly managed heavy commands and how shared
resources are owned — which needs cancellation and recovery of a stale owner;
compare-and-set alone is not a lock or a lease.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3 "Build
coordination" and the note on versioned coordination state
(`--if-version`, watch)); its scope and behaviour are to be agreed with the
person before any work, and recorded in this file in
the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN the design is written THEN it covers ownership, cancellation,
      stale-owner recovery and how a person overrides a limit.
- [ ] The decision is recorded; a feature task follows only if approved.

## Checks

- The decision recorded in this file: its Design and Plan sections written
  and agreed with the person.
- No production code.

## Limits / non-goals

- Separate from model/provider limits (GLD-042).

## Context

- GLD-035 (measurement first).
