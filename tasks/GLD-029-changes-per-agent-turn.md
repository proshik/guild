---
id: GLD-029
title: "Show what changed after the last instruction to an agent"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/status.rs
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/view/changes.rs
  - crates/desktop/src/diff_tab/**
  - crates/session/src/desk.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Answer "what changed since I gave that instruction" for a session: a diff from
a baseline taken when the turn began (OSC 7501 `working`) to now — while being
honest that the person or another session may have edited the same files in
between.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Changes per
agent turn"); its scope and behaviour are to be agreed with the person before
any work, and recorded in the task (or an OpenSpec change) in the same branch.
**A visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a turn begins THEN a baseline of the session's worktree is recorded
      without writing to the repository.
- [ ] WHEN the person asks for the turn's changes THEN the diff from that
      baseline is shown.
- [ ] WHEN another session or the person edited the worktree during the turn
      THEN the view says attribution is uncertain, never presents those edits
      as the agent's.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No commit, stash or index change to take the baseline.

## Context

- `crates/desktop/src/diff_tab.rs`.
