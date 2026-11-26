---
id: GLD-067
title: One list of who is waiting for the person, across this machine and every connected one
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-066]
touches:
  - crates/desktop/src/remote/**
  - crates/desktop/src/root/status.rs
  - crates/desktop/src/view/status.rs
  - crates/desktop/src/view/titlebar.rs
  - crates/desktop/src/view/column.rs
  - crates/desktop/src/system.rs
  - crates/desktop/src/system/**
  - crates/desktop/src/i18n/**
  - crates/desktop/tests/all/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-26"
---

## Goal

With several machines connected, the person should not have to visit every
window to learn which agent needs them. The local window gets **one place**
that gathers the waiting sessions of this machine and of every connected
remote machine: each row names the machine, the project and why it waits. A
click raises that machine's window on that session; the answer is given
there, not from the list.

**Decided by the person** (2026-11-22): one shared "who is waiting" list in
the local window; a click raises the right machine's window; it does not
answer from the list. Where in the window it lives — the title bar, the
status bar or the column — is decided on the Penpot mockup.

## Acceptance

- [ ] WHEN a session on this machine or on a connected one is
      `Status::Waiting(Why)` (`guild_session::status`, `Why` = `Permission |
      Question | Auth | Unstated`) THEN the list shows it with its machine,
      its project and its reason; WHEN it stops waiting THEN its row goes.
- [ ] WHEN the person clicks a row THEN that machine's window (the local one
      or GLD-066's) is raised with that session in front; nothing is typed
      or answered from the list.
- [ ] WHEN a remote session starts waiting THEN the chime and the Dock badge
      behave as for a local one, and WHEN one machine is connected twice
      THEN one wait is chimed and counted once, never twice.
- [ ] WHEN a machine disconnects THEN its rows are marked as from a
      disconnected machine or dropped, as the Penpot design says — never
      shown as live.
- [ ] Where the list lives (title bar, status bar or column) and how it
      looks are designed in Penpot and approved before they are coded, and
      Penpot is brought to what was merged (`docs/penpot.md`).
- [ ] `CHANGELOG.md` has its line under `## [Unreleased]`.

## Checks

- Tests first: the list's model merging local and remote waits, the
  once-per-wait chime and badge count with a machine connected twice, a
  disconnect.
- `just check`, with `GUILD_STATE_DIR` set for anything that touches state.
- A check in an isolated dev window with a stand-in agent waiting locally
  and one through `ssh localhost` (no provider); the chime confirmed by its
  audio assertion, not by ear.

## Limits / non-goals

- No answering from the list, no quick replies, no permission buttons.
- No phone or web inbox; that layer waits for a separate task tool's first
  version and builds on this list later, in a plan of its own.
- No new status values: the list shows what `Status::Waiting(Why)` already
  says.

## Context

- GLD-010's Design section; GLD-066 (the
  remote-machine window this list raises).
- `crates/session/src/status.rs` — `Status::Waiting(Why)` *(code)*.
- `crates/desktop/src/root/status.rs`, `crates/desktop/src/system.rs` — the
  chime and the Dock badge today *(code)*.
