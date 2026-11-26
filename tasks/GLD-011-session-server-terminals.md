---
id: GLD-011
title: Session server owns terminals; the window attaches to them
kind: feature
status: backlog
priority: 3
component: server
depends_on: [GLD-009, GLD-010]
touches:
  - crates/server/**
  - crates/terminal/**
  - crates/desktop/src/terminal/**
  - crates/desktop/src/root/sessions.rs
  - crates/desktop/src/root/tabs.rs
  - crates/session/src/**
  - Cargo.toml
  - Cargo.lock
  - scripts/check-boundaries.sh
  - justfile
  - AGENTS.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

The first step of the session server, in the shape GLD-010 decided: a
headless process that owns each terminal's PTY, its child process, the
authoritative screen (the terminal core from GLD-009) and its OSC 7501
records. The window is a client: a terminal tab draws from the server's
snapshot and change stream and sends input through it. Reattaching — after
the window reloads while the server still runs — restores the exact screen,
OSC 133 marks, hyperlinks and the status record.

This task covers terminals and their records only; harness bookkeeping
(the board, hooks, IDE) and terminal jobs move in GLD-012. GLD-010 may
re-scope it.

## Acceptance

- [ ] WHEN the window opens THEN it starts or attaches to the server for its
      state directory through the versioned handshake, and names the server it
      is attached to in its logs and in `guild` output.
- [ ] WHEN a terminal tab is opened THEN its PTY and process belong to the
      server, and the tab shows and edits it as today (typing, paste, resize,
      selection, find, scrollback, links).
- [ ] WHEN the window detaches and attaches again while the server runs THEN
      the screen, scrollback, OSC 133 marks, hyperlinks and the OSC 7501
      record are exactly as they were, from a snapshot plus the changes after
      its sequence number.
- [ ] WHEN a client is slow or stops reading THEN the PTY reader keeps
      reading and the program never blocks on it; the client catches up by a
      new snapshot.
- [ ] WHEN the last client leaves THEN the server ends after the decided
      grace and its children are reaped (no keep-running yet — GLD-013).
- [ ] The server crate links no GPUI, guarded by `just boundaries`.
- [ ] WHEN the same output stream is measured before and after the move, in
      a release build (a large `cat`, a build's output) THEN both numbers are
      in the commit body, and the extra process and socket between the PTY
      and the screen do not eat the release build's output speed.

## Checks

- Tests first: protocol handshake and versions, snapshot + stream
  equivalence (screen after replay = screen in the server), the slow-client
  case, child reaping on server exit — in crates without GPUI.
- The output-throughput measurement above, release build, one and the same
  stream before and after: today's release build is eight times faster on
  output, and the extra hop must not eat it (large frames, no intermediate
  copy).
- `docs/traps.md`, the terminal section, read before touching the reader loop.
- `just check`; a dev-window check of a shell and of an agent session through
  a stand-in program (no provider), including a window reload.

## Limits / non-goals

- **Not here:** remote connection — the ssh bridge is GLD-065 and the
  remote-machine window GLD-066; this task's protocol carries the generation
  and capabilities they rely on. Out of scope altogether: files, git, search,
  LSP and buffers on the server; a TUI editor; surviving a server crash;
  launch at login.
- No change to how harnesses are launched or resumed beyond where the PTY
  lives.
- If the window has to say that the server failed to start, an existing
  notice needs no Penpot design; a new tab state does, and is designed and
  approved before it is coded (`docs/penpot.md`).
- Up to one spec and two code review rounds, `codex-only` profile.

## Context

- GLD-010's `openspec/changes/guild-session-server/`.
- `docs/runtime-ownership.md` — "Process lifetime", "Bounded transport
  ownership"; `docs/traps.md` — the terminal section.
