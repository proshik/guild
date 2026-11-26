---
id: GLD-014
title: One API for the window and the guild CLI, served by the session server
kind: feature
status: backlog
priority: 3
component: server
depends_on: [GLD-012]
touches:
  - crates/server/**
  - crates/session/src/**
  - crates/cli/src/**
  - crates/desktop/src/root/command.rs
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

The window and `guild` talk to the session server through one API: a
snapshot plus subscriptions with sequence numbers, reconnect from a sequence
number, and protection against a command executing twice. `guild` keeps its
commands and their `--json` words; what changes is who answers. The
provenance rule — `GUILD_SESSION` looked up by the owner, never trusted from
the caller — becomes the server's.

## Acceptance

- [ ] WHEN `guild agent list|wait|send`, `session new`, `terminal` job
      commands and `worktree create` run THEN the server answers, with the
      same `--json` words as today (`session/command.rs`), whether a window
      is attached or not — except commands that are window deeds by nature
      (opening a tab in a window), which say plainly that no window is
      attached.
- [ ] WHEN a client subscribes THEN it receives a snapshot with a sequence
      number and every change after it, in order; WHEN it reconnects with its
      last sequence number THEN it receives what it missed, or a new snapshot
      when that is gone.
- [ ] WHEN a command carrying a request id is sent twice (a retry after a
      lost connection) THEN it executes once and both calls get its result —
      as terminal jobs and `session resume --request-id` already do.
- [ ] WHEN a caller names a session THEN the server checks provenance
      (decision 35) against its own records; the 0600 socket stays the outer
      gate.
- [ ] WHEN a client and the server meet THEN the handshake carries the
      protocol generation and capabilities GLD-010 decided, because the same
      API later goes through the ssh bridge (GLD-065) between two builds:
      an unknown field is skipped, an unknown enum value does not break the
      client, a method missing on one side disables only its own action, and
      an incompatible generation is refused with a plain "update Guild on
      machine X" — each tested.
- [ ] WHEN the server describes a session THEN it may carry the session's
      project and worktree path, an optional field (GLD-066's column is built
      from it); a client that does not know it ignores it.
- [ ] A command still calls the deed a person's gesture calls: nothing new
      spells a git verb or parses git's output (`AGENTS.md`, "`guild` is a
      door").

## Checks

- Tests first: the CLI tests that run the binary against a socket playing
  the owner, now playing the server; idempotency and reconnect tests in
  crates without GPUI.
- `just check` (`guild-cli` still links only `guild-session` and
  `serde_json`, or `just boundaries` is updated deliberately, with the reason
  in decisions).

## Limits / non-goals

- Several observers of one terminal and input rights: GLD-015.
- Resumable lifecycle subscriptions (2026-09-23, from
  the former `docs/backlog/2026-10-17-product-improvements.md`) are this
  task's subscriptions with a sequence number; durable message inboxes are
  GLD-049. A message that survives does not mean a process survived.
- **Not here:** remote connection — the ssh bridge is GLD-065, the
  remote-machine window GLD-066; this task only makes the API fit for them.
  Out of scope altogether: files and git on the server (language servers:
  GLD-010); a TUI editor;
  surviving a server crash.

## Context

- Today provenance is the `GUILD_SESSION` value looked up by the window with
  the 0600 socket as the only gate (`root/command.rs:1327-1365`).
- `docs/decisions.md` — decision 35 (provenance).
- GLD-010's protocol.
