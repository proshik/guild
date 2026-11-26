---
id: GLD-066
title: A window per remote machine — its sessions through the terminal emulator only
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-065, GLD-015]
touches:
  - crates/desktop/src/remote/**
  - crates/desktop/src/windows.rs
  - crates/desktop/src/root/satellites.rs
  - crates/desktop/src/view/satellite.rs
  - crates/desktop/src/view/titlebar.rs
  - crates/desktop/src/column.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/view/column.rs
  - crates/desktop/src/menu.rs
  - crates/desktop/src/root/menu.rs
  - crates/desktop/src/terminal/links.rs
  - crates/desktop/src/root/links.rs
  - crates/desktop/src/i18n/**
  - crates/desktop/tests/all/**
  - crates/session/src/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-26"
---

## Goal

From the laptop, the person opens a window onto the home Mac, sees its
sessions and answers an agent right in its terminal. Each connected remote
machine gets **a window of its own**, apart from every local window: its
title names the host and the connection's state, and its column lists that
machine's projects, worktrees and sessions as its server's snapshot reports
them. The only way to work in it is the terminal emulator.

**Decided by the person** (2026-11-22): remote machines are fully separate
from local sessions — one window per remote machine, interaction only
through the terminal emulator — no file links, no Claude IDE integration, no
git, Run, Find in Files, Explorer or editor, no moving tabs between the
remote window and local windows; `guild` inside a remote session talks only
to its own machine's server.

## Acceptance

- [ ] WHEN the person chooses "Connect to machine…" and a saved machine
      (GLD-065) THEN a window for that machine opens, attached through the
      ssh transport; WHEN that machine's window is already open THEN it is
      raised instead of a second one.
- [ ] WHEN the window is attached THEN its title shows the host and one of
      the states connected, disconnected (reconnecting) or incompatible
      ("update Guild on <machine>"), as drawn in Penpot.
- [ ] WHEN the remote server's snapshot changes THEN the column shows that
      machine's projects, worktrees and sessions with their live statuses,
      built from the optional project and worktree path GLD-014 adds; a
      session without one is still listed.
- [ ] WHEN the person opens a session THEN it is a terminal tab drawn from
      the remote server's snapshot and stream, taking input through GLD-015's
      handover, with resize following GLD-015's rule.
- [ ] WHEN the window is a remote machine's THEN file links in output,
      Claude IDE integration, git, Run, Find in Files, the Explorer and the
      editor are absent, and a tab cannot be moved to or from a local window
      — each checked by a test on the window's model, not only by eye.
- [ ] WHEN `guild` runs inside a remote session THEN it talks to that
      machine's server only, never to the local window.
- [ ] WHEN the connection drops THEN the window and its tabs stay, marked
      disconnected, never ended, and come back as GLD-065 reconnects.
- [ ] The window, its title, the connected / disconnected / incompatible
      states and the "Connect to machine…" item are designed in Penpot and
      approved before they are coded, and Penpot is brought to what was
      merged (`docs/penpot.md`).
- [ ] `CHANGELOG.md` has its line under `## [Unreleased]`.

## Checks

- Tests first: the window model's disabled features, the column built from a
  snapshot (with and without paths), the state shown for each connection
  state.
- `just check`, with `GUILD_STATE_DIR` set for anything that touches state.
- A check in an isolated dev window against `ssh localhost`, with a stand-in
  agent (no provider): connect, answer in the terminal, drop the
  connection, reconnect.

## Limits / non-goals

- No files, git, LSP, search or editor of the remote machine, and no
  file links into it; nothing of the local window's tools reaches it.
- The shared "who is waiting" list across machines is GLD-067.
- The machine list itself and the transport are GLD-065's; this task only
  offers them in the window.
- No phone access; it waits for a separate task tool's first version.

## Context

- GLD-010's `openspec/changes/guild-session-server/` — the remote window's
  boundaries.
- GLD-065 (the ssh transport and saved machines), GLD-015 (input rights),
  GLD-014 (the optional project and worktree path).
- `crates/desktop/src/windows.rs`, `root/satellites.rs` — the windows beside
  the main one today *(code)*. `crates/desktop/src/remote/**` in `touches` is
  where the author expects the new module, not a decision.
- `docs/penpot.md`; `AGENTS.md` — the Penpot and changelog rules.
