---
id: GLD-028
title: "Decide whether changelists and a shelf are worth having"
kind: research
status: backlog
priority: 4
component: docs
depends_on: [GLD-027]
touches:
  - tasks/GLD-028-changelists-and-shelf.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
---

## Goal

Changelists and a shelf, evaluated only after partial staging
exists. Grouping by the index is intentional today; this task decides whether
anything beyond it earns its place.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3
"Changelists and shelf", not promoted); its scope and behaviour are to be
agreed with the person before any work, and recorded in the task (or an
OpenSpec change) in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN partial staging has been in use THEN the person is asked whether
      changelists or a shelf would replace a real workaround, with examples.
- [ ] The answer is recorded here; a yes becomes its own feature task, a no
      cancels this one.

## Checks

- The answer recorded in this file; no code.

## Limits / non-goals

- Decision only.

## Context

- GLD-026, GLD-027.
