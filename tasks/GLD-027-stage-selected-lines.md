---
id: GLD-027
title: "Stage and unstage selected lines"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: [GLD-026]
touches:
  - crates/desktop/src/git.rs
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/diff_tab/**
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Stage only the selected lines of a hunk, with the same guarantees as GLD-026.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P1 "Partial staging"
— selected lines next); its scope and behaviour are to be agreed with the
person before any work, and recorded in the task (or an OpenSpec change) in
the same branch. **A visible UI change:** a mockup in Guild's design system
and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN lines are selected in a diff THEN exactly those lines are staged or
      unstaged; the working file and unrelated index changes are preserved.
- [ ] WHEN a selection cannot be expressed as a valid patch THEN the action
      refuses and says why.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No index preview.

## Context

- GLD-026.
