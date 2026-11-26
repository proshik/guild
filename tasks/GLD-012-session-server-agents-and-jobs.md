---
id: GLD-012
title: Session server keeps agent statuses, hooks and terminal jobs
kind: feature
status: backlog
priority: 3
component: server
depends_on: [GLD-011]
touches:
  - crates/server/**
  - crates/session/src/**
  - crates/desktop/src/root/status.rs
  - crates/desktop/src/root/terminal_jobs.rs
  - crates/desktop/src/root/resume_handoff.rs
  - crates/desktop/src/root/command.rs
  - crates/desktop/src/hooks.rs
  - crates/desktop/src/hooks/**
  - crates/desktop/src/ide.rs
  - crates/desktop/src/ide/**
  - crates/desktop/src/terminal_job.rs
  - docs/runtime-ownership.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

With terminals on the server (GLD-011), the bookkeeping about the programs in
them moves there too, so it outlives a window reload and every client sees
the same thing: the status board fed by OSC 7501 root records, the hook
socket and identity (conversation binding, Codex durable identity), the
managed-input latch and its epochs counted where bytes are written, and
terminal jobs with their receipts. The window reads all of it from the
server.

Whether Claude's IDE server (`ide/`) moves here is GLD-010's answer:
`CLAUDE_CODE_SSE_PORT` is fixed at launch, so if the window hosts it, IDE
integration breaks across a window restart.

## Acceptance

- [ ] WHEN the window reloads while the server runs THEN every agent's
      status, kind, unread state, turn count and conversation identity are
      as before, and no completion is chimed twice or lost.
- [ ] WHEN a hook posts THEN it reaches the server's socket, and the
      provenance rule (a payload naming no session of the server's is
      dropped) holds there.
- [ ] WHEN managed live input is asked for THEN its gate reads the server's
      latch and the server's input epochs, and is never looser than today —
      the existing managed-input tests pass against it.
- [ ] WHEN a terminal job runs THEN it belongs to the server, its receipts
      are the server's, and `guild` sees the same job from any client.
- [ ] `docs/runtime-ownership.md` says what the server owns and what the
      window still owns.

## Checks

- Tests first; the existing status, chime, managed-input, workflow and
  terminal-job tests moved or pointed at the server and green.
- `just check`; a dev-window check with stand-ins (no provider) across a
  window reload.

## Limits / non-goals

- **Not here:** remote connection — GLD-065 (the ssh bridge) and GLD-066
  (the remote-machine window). Out of scope altogether: files, git, search,
  LSP and buffers on the server; a TUI editor; surviving a server crash;
  launch at login.
- The `guild` CLI keeps talking to the window in this task; the common API
  is GLD-014.
- GLD-010 may split this task (for example jobs apart from statuses).

## Context

- GLD-010, GLD-011.
- `docs/runtime-ownership.md` — "Terminal jobs", "Managed terminal
  recovery".
- `AGENTS.md` — "The hooks reach only what Guild starts"; "`guild` is a
  door".
