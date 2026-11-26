---
id: GLD-016
title: A terminal (TUI) client — sessions, statuses, attach
kind: feature
status: backlog
priority: 4
component: tui
depends_on: [GLD-014, GLD-015]
touches:
  - crates/tui/**
  - Cargo.toml
  - Cargo.lock
  - scripts/check-boundaries.sh
  - justfile
  - tasks/tasks.toml
  - AGENTS.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

A separate binary that attaches to the session server from any terminal:
it lists sessions grouped by project and worktree with their live statuses
(rest, working, waiting with its reason, done, error, unknown), and attaches
to one terminal full-screen, taking input through GLD-015's handover and
detaching back to the list. It is a client of the API from GLD-014, nothing
more. `guild` stays small: the TUI is its own binary.

## Acceptance

- [ ] WHEN the person picks the rendering library THEN the choice is
      recorded — a library (e.g. ratatui) versus Guild's own drawing, with
      pros, cons and a recommendation put to the person before code.
- [ ] WHEN the TUI starts THEN it attaches to the server for the state
      directory (or says none is running) and shows the session list with
      statuses updating live.
- [ ] WHEN the person attaches to a session THEN its screen is drawn from
      the server's snapshot and changes, keys go to it while the TUI holds
      input, and a detach key returns to the list.
- [ ] WHEN a session is waiting for the person THEN the list shows it and
      its reason, as the window's column does.
- [ ] WHEN the TUI connects THEN it does so through a transport
      abstraction, not a socket path, with the local Unix socket as its one
      transport here — so GLD-065 adds `--remote host` as one more transport
      without reworking the client.
- [ ] The binary links no GPUI (`just boundaries`); whether it ships inside
      Guild.app is decided and `tasks/tasks.toml`'s `tui` rule matches.

## Checks

- Tests for the list model and the attach/detach state without a terminal;
  `just check`; a manual run against a dev server with a stand-in agent.

## Limits / non-goals

- **No TUI editor**, no file tree, git or LSP — those stay in the window.
- **Not here:** remote connection — the TUI attaches to a local socket
  only, and `--remote host` is GLD-065's. **Deferred:** launch at login.

## Context

- GLD-014, GLD-015; GLD-010's protocol.
- Session-server plan, stage 6.
