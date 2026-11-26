---
id: GLD-035
title: "Implement local memory observation per project and session"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - openspec/changes/memory-observability/**
  - crates/desktop/src/root/usage.rs
  - crates/desktop/src/view/usage.rs
  - crates/desktop/src/view/titlebar.rs
  - crates/desktop/src/system/**
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

See how much memory Guild and the processes it owns use, and which project or
session accounts for it. A plan was drafted as the OpenSpec change
`memory-observability` (titlebar memory chip, card with ten-minute history,
grouping by project or process type), not implemented; that draft is no
longer in the repository and is written again when the task is taken. The
backlog also named CPU; the draft covered memory only.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Local resource
observation"); its scope and behaviour are to be agreed with the person before
any work, and recorded in the task (or an OpenSpec change) in the same branch.
**A visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the task is taken THEN the person confirms the change's plan, written again, its
      backend choice (native versus `sysinfo`) and whether CPU joins it or
      stays out.
- [ ] The change's tasks are carried out as written there, each to green
      tests.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Observation only: no automatic scheduling or concurrency limits (GLD-036).

## Context

- `docs/runtime-ownership.md` — the processes the window owns.
