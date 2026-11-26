---
id: GLD-050
title: "Separate stopping an agent from closing its tab; send named keys to a terminal"
kind: feature
status: backlog
priority: 3
component: cli
depends_on: []
touches:
  - crates/cli/src/**
  - crates/session/src/command.rs
  - crates/desktop/src/root/command.rs
  - crates/desktop/src/root/agent_submit.rs
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

`guild agent stop` ends a session; whether it also closes the tab should be
the caller's explicit choice. And a caller sometimes needs to press a key —
Escape, Enter, Ctrl-C — in an agent's terminal, which today means sending raw
text.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (row
2026-10-06 (`agent stop --close`, named terminal input keys)); its scope and
behaviour are to be agreed with the person before any work, and recorded in
the task (or an OpenSpec change) in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN the current behaviour of `agent stop` is checked THEN it is
      recorded, and `--close` (or the inverse) is added so both outcomes are
      explicit.
- [ ] WHEN a named key is sent THEN it goes through the same path and
      caller/target guards as `agent send`.
- [ ] `--json` words of existing commands unchanged.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No answering for the person (GLD-049).

## Context

- `crates/cli/src/main.rs` (agent stop).
- `root/agent_submit.rs`.
