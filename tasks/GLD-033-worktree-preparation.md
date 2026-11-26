---
id: GLD-033
title: "Prepare a new worktree: configured setup steps and selected ignored-file copies"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/view/new_worktree.rs
  - crates/desktop/src/repository_settings.rs
  - crates/session/src/settings.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

A new worktree comes up ready: configured preparation steps run visibly, and
ignored files the person chose (e.g. a local `.env`, `values-local.yaml`) are
copied from the main checkout.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Worktree
preparation" and the note on setup scripts and explicit ignored-file
copies); its scope and behaviour are to be agreed with the person before any
work, and recorded in this file in the same branch. **A
visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a repository has preparation steps configured THEN they run after
      `worktree add`, visibly, with their output kept and a failure reported,
      never blocking the worktree's creation.
- [ ] WHEN files are selected for copying THEN exactly those are copied;
      nothing is copied implicitly, and secrets are never copied without being
      named.
- [ ] WHEN a repository's scripts are new or changed THEN they are not trusted
      automatically: the person confirms them once.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Named run/stop targets are GLD-034.

## Context

- `crates/desktop/src/repository_settings.rs`.
