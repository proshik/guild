---
id: GLD-044
title: "Filter the sidebar's project groups"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: []
touches:
  - crates/desktop/src/column.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/view/column.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Narrow the column to the groups that matter now — e.g. only worktrees with a
working or waiting agent — without changing the saved order.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (the note
on filtered sidebar groups); its scope and behaviour are to be agreed with the
person before any work, and recorded in this file in
the same branch. **A visible UI change:** a mockup in Guild's design system
and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a filter is on THEN hidden groups are named as hidden and the
      filter is one click from off.
- [ ] WHEN it is turned off THEN order and expansion are exactly as before.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Nothing beyond the agreed scope.

## Context

- `crates/desktop/src/column.rs`.
