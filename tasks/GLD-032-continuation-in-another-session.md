---
id: GLD-032
title: "Design handing a task over to another session"
kind: design
status: backlog
priority: 2
component: docs
depends_on: []
touches:
  - tasks/GLD-032-continuation-in-another-session.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Hand a task to a new session — possibly another harness — with its
constraints, decisions and references to results, as an explicit, reviewable
handoff. This is not the provider's native resume and does not move hidden
state; Guild's existing startup continuation (`session resume --from-file
--request-id`) resumes the same conversation and stays as it is.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Continuation in
another session"); its scope and behaviour are to be agreed with the person
before any work, and recorded in this file in the same
branch. **A visible UI change:** a mockup in Guild's design system and the
person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the handoff is specified THEN what it carries (task, constraints,
      decisions, result references) is visible and editable before it is sent.
- [ ] WHEN it is sent THEN it uses the existing at-most-once request receipts,
      not a new delivery path.
- [ ] The decision is recorded and a feature task written for what is
      approved.

## Checks

- The decision recorded in this file: its Design and Plan sections written
  and agreed with the person.
- No production code.

## Limits / non-goals

- Design only; no promise of native conversation transfer.

## Context

- CHANGELOG: startup continuation.
