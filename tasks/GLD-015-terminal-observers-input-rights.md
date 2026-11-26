---
id: GLD-015
title: Several observers of one terminal, with an explicit handover of input rights
kind: feature
status: backlog
priority: 4
component: server
depends_on: [GLD-014]
touches:
  - crates/server/**
  - crates/session/src/**
  - crates/desktop/src/terminal/view/**
  - crates/desktop/src/root/agent_submit.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

More than one client may show the same terminal — two windows, the window
and a TUI, `guild` watching — but only one holds the keyboard at a time.
Taking input is an explicit act that the previous holder sees; nothing types
into a terminal without holding the right, and programmatic input (`agent
send`, managed live input) goes through the same rule.

## Acceptance

- [ ] WHEN two clients attach to one terminal THEN both see the same screen
      and changes; only the input holder's keys reach the PTY.
- [ ] WHEN another client takes input THEN the previous holder is told and
      its keys stop reaching the PTY, before the new holder's first key is
      written.
- [ ] WHEN a holder disconnects THEN the right is released, not passed on
      silently.
- [ ] WHEN `agent send` or managed live input writes THEN it is refused while
      a person holds input in that terminal, with a reason that says so — or
      the rule GLD-010/GLD-014 decided instead, recorded in the spec.
- [ ] Resize with several observers follows one stated rule, tested.
      Recommended: the input holder's size, the holder leading the others;
      asked when the task starts.
- [ ] The "someone else is typing" state and the take-over are designed in
      Penpot and approved before they are coded (`docs/penpot.md`).

## Checks

- Tests first in the server for the rights state machine and ordering;
  `just check`; a dev-window check with two clients and a stand-in program.

## Limits / non-goals

- **Not here:** remote clients — GLD-065 carries this task's rules through
  the ssh bridge and GLD-066 is the remote-machine window. Out of scope: a TUI
  editor.
- No shared cursor or collaborative editing.

## Context

- GLD-014 (subscriptions, provenance).
- `root/agent_submit.rs` — the one path text takes into an agent's
  terminal; managed-input epochs (`docs/runtime-ownership.md`).
