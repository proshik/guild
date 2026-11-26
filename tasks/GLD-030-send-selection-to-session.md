---
id: GLD-030
title: "Send selected code, a diff or an error to a chosen session"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/agent_submit.rs
  - crates/desktop/src/view/menu/**
  - crates/desktop/src/actions/**
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

From an editor selection, a diff or an error in a terminal, put that text into
a chosen agent session — through the one path text takes into an agent's
terminal (`root/agent_submit.rs`), with the same refusals.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Selected context
transfer"); its scope and behaviour are to be agreed with the person before
any work, and recorded in the task (or an OpenSpec change) in the same branch.
**A visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the person picks a session THEN the text arrives in its input as a
      quoted block with its source (file and lines, commit, terminal), not
      submitted unless the person submits it.
- [ ] WHEN the target is working, blocked or has no OSC 7501 record THEN the
      existing send refusals apply and are shown.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Not a continuation (GLD-032) and not a queue (GLD-031).

## Context

- `crates/desktop/src/root/agent_submit.rs`.
- `AGENTS.md` — module map, `agent_submit`.
