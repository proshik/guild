---
id: GLD-034
title: "Named run and stop targets per worktree"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-033]
touches:
  - crates/desktop/src/run.rs
  - crates/desktop/src/run/**
  - crates/desktop/src/root/run.rs
  - crates/desktop/src/repository_settings.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Beside what a project declares it can run (`run/discover.rs`), the person can
name run and stop commands for a repository, shared by its worktrees,
extending Run and Terminal Jobs rather than adding a third runner.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical extension
of "Worktree preparation" and the note on named run/stop scripts);
its scope and behaviour are to be agreed with the person before any work, and
recorded in the task (or an OpenSpec change) in the same branch. **A visible
UI change:** a mockup in Guild's design system and the person's decision come
before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a named target is configured THEN it appears in Run for every
      worktree of the repository and runs in that worktree.
- [ ] WHEN its stop command is configured THEN Stop runs it before signalling
      the process group.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No automatic start.

## Context

- `crates/desktop/src/run/discover.rs`.
- GLD-033.
