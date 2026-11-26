---
id: GLD-043
title: "Declarative layout presets with a dry run"
kind: feature
status: backlog
priority: 4
component: cli
depends_on: []
touches:
  - crates/cli/src/**
  - crates/session/src/command.rs
  - crates/session/src/layout.rs
  - crates/desktop/src/root/command.rs
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Describe an arrangement — areas, panes, which sessions and files in them — in
a file, apply it through `guild` with a dry run first, and save the current
one.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3 "CLI
layout presets" and the note on saved/declarative layouts); its
scope and behaviour are to be agreed with the person before any work, and
recorded in the task (or an OpenSpec change) in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN `--dry-run` is given THEN the command prints what would change and
      changes nothing.
- [ ] WHEN applied THEN targets are stable (by worktree and session identity)
      and the focus does not move unless asked.
- [ ] It goes through the deeds the gestures use (`guild` is a door).

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No new window-local layout format beyond `guild_session::layout`.

## Context

- `crates/session/src/layout.rs`.
