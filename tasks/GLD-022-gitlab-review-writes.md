---
id: GLD-022
title: "GitLab review writes: drafts, selected publication, replies, approvals"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-063]
touches:
  - crates/desktop/src/gitlab/**
  - crates/desktop/src/root/gitlab.rs
  - crates/desktop/src/view/gitlab.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Write review comments locally as drafts, publish only the ones chosen, reply
in threads and approve — from the window.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P2
"GitLab review writes", not promoted); its scope and behaviour are to be
agreed with the person before any work, and recorded in the task (or an
OpenSpec change) in the same branch. **A visible UI change:** a mockup in
Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a draft is written THEN it stays local until the person publishes
      it; nothing is published implicitly.
- [ ] WHEN the person publishes a selection THEN exactly those drafts are
      posted, and failures are reported per draft.
- [ ] WHEN replying or approving THEN the action names the MR revision it
      applies to.
- [ ] Sending a selected MR revision or job log to a session goes through
      GLD-030, not a second path.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Every publication is the person's explicit act.

## Context

- GLD-063 (decision 56), GLD-030; GLD-021, cancelled.
