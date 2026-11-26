---
id: GLD-023
title: "Open or create a worktree from a GitLab MR"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-063]
touches:
  - crates/desktop/src/root/gitlab.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/view/new_worktree.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Start work on someone's MR in its own worktree in one step, after its identity
and checkout behaviour are specified.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (2026-09-12
"Open/create worktree from PR/MR"); its scope and behaviour are to be agreed
with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in Guild's
design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the person picks an MR THEN a worktree on its source branch is
      opened if one exists, or created through the existing `worktree add`
      write (decision 21), never by a new git verb.
- [ ] WHEN the MR comes from a fork THEN the behaviour is specified first
      (fetch refspec, remote naming) and refused until it is.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No force, no reset; the existing worktree guards hold.

## Context

- `AGENTS.md` — git writes and their guards.
- GLD-063 (decision 56); GLD-021, cancelled.
