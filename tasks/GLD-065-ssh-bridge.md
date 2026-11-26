---
id: GLD-065
title: The ssh bridge — guild server bridge, and clients that reach another machine's server through ssh
kind: feature
status: backlog
priority: 4
component: server
depends_on: [GLD-014]
touches:
  - crates/cli/src/**
  - crates/cli/tests/**
  - crates/server/**
  - crates/session/src/**
  - crates/tui/**
  - scripts/check-boundaries.sh
  - docs/runtime-ownership.md
  - docs/remote-machines.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-26"
---

## Goal

A client on one machine reaches the session server of another through ssh,
and through nothing else. On the remote machine `guild server bridge` joins
its stdio to that machine's server socket and does nothing more; on the
client's side an **ssh transport** runs the system `ssh` and speaks the same
byte stream over it that a local client speaks over the Unix socket. The TUI
gets `--remote host`. The window's use of it — a window per machine — is
GLD-066.

**Decided by the person** (2026-11-22): ssh only, no TCP listener; locally a
0600 Unix socket, remotely the same stream through `ssh host guild server
bridge`; a dropped connection is *disconnected*, never *ended*.

**The approach planned on 2026-11-26** (as GLD-010 confirms or changes it):
the bridge joins its stdio to the server's socket and does nothing else; the
client runs the system `ssh`, so hosts come from `~/.ssh/config` and keys
from the person's agent, with `BatchMode=yes`, `ControlMaster` and
compression.

## Acceptance

- [ ] WHEN `guild server bridge` runs THEN it connects to the server socket
      of its own state directory and copies bytes both ways between that
      socket and its stdio until either side closes; it starts no server,
      runs no command and opens no listener of its own. WHEN no server is
      running THEN it exits non-zero with a plain message.
- [ ] WHEN a client uses the ssh transport for a host THEN it runs the
      system `ssh` with `BatchMode=yes`, `ControlMaster` and compression,
      the host resolved by `~/.ssh/config`, and runs `guild server bridge`
      there. WHEN ssh cannot sign in without asking (a password, no key)
      THEN the client fails with "set up a key for <host>" instead of
      hanging, since a window cannot ask for a password.
- [ ] WHEN the client runs the bridge on a host THEN the remote command
      does not depend on the `PATH` of a non-interactive ssh shell, which
      does not reach the binary inside `Guild.app`. By default it is the
      bundle's own `/Applications/Guild.app/Contents/MacOS/guild`, and a
      saved machine may name another path. WHEN the command is not found
      there (ssh's exit 127) THEN the client says "Guild is not installed on
      <host>" with the path it tried, not a generic failure.
- [ ] WHEN ssh cannot reach the host at all (connection refused or timed
      out) THEN the client says so in a plain line, and notes that Remote
      Login may be off on that Mac. It neither retries in a tight loop nor
      changes anything there.
- [ ] `docs/remote-machines.md` says how to prepare a machine: Remote Login
      turned on in macOS's settings, a key the person's ssh agent offers, the
      host in `~/.ssh/config`, Guild installed there, and its server running
      (a window open or GLD-013's setting on).
- [ ] WHEN the TUI is started with `--remote host` THEN it lists and attaches
      to that machine's sessions exactly as it does locally — one more
      transport behind GLD-016's abstraction, no change to the list or attach
      code.
- [ ] Saved machines are the hosts the person names, kept as `~/.ssh/config`
      host names plus a small settings file of Guild's that lists which of
      them to offer; Guild never writes `~/.ssh/config`.
- [ ] WHEN client and server meet over the bridge THEN the handshake names
      the machine, the server and its version. WHEN the generation matches
      THEN different builds work together, and a method one side lacks
      disables only its own action. WHEN the generation is incompatible THEN
      the client says "update Guild on <machine>" and attaches nothing.
- [ ] WHEN the connection drops THEN every remote session shows as
      disconnected, never ended; the client reconnects with backoff and
      catches up by sequence number, or by a new snapshot when the missed
      changes are gone.
- [ ] Security, stated in the spec and checked by tests: the server still
      listens on no TCP port; its socket stays 0600; the bridge only connects
      to that socket; the provenance rule (decision 35) is not loosened by
      the bridge.

## Checks

- Tests first, without a network: the ssh transport's command is
  replaceable, so tests run the bridge through a command transport
  (`sh -c` in place of `ssh`) against a test server — handshake, version
  skew, disconnect and reconnect with catch-up.
- `just check`, with `guild-cli` still linking only `guild-session` and
  `serde_json`, or `just boundaries` changed deliberately with the reason in
  `docs/decisions.md`.
- A manual check against `ssh localhost` with a stand-in agent (no
  provider): the TUI with `--remote localhost`, a dropped connection, the
  reconnect.

## Limits / non-goals

- **No window UI here**: choosing a machine in the window, its window and
  its states are GLD-066. Nothing in this task is a change a person sees in
  the window, so there is no Penpot step.
- No TCP, no listener, no port forwarding, no own ssh implementation or
  library — the system `ssh` only.
- No phone access (a web inbox or Telegram through a VDS and `ssh -R`): it
  waits for a separate task tool's first version and gets a plan of its own.
- A remote machine's files, git, language servers and search are not
  reached.
- **The person's own setup, not Guild's work:**
  - turning on Remote Login on the remote Mac;
  - creating and installing the first ssh key;
  - reaching a machine outside the local network (Tailscale, a VPN or a
    forwarded port).

  Guild builds no relay, no NAT traversal and no key management. It reports
  what is missing and points at `docs/remote-machines.md`.

## Context

- GLD-010's `openspec/changes/guild-session-server/` — the protocol, its
  generation and capabilities, and the remote layer.
- GLD-014 (the API this carries), GLD-016 (the transport abstraction).
- `docs/decisions.md` — decision 35 (provenance); `AGENTS.md` — "`guild` is a
  door", the `guild-cli` boundary.
- The stable-endpoint rules, as GLD-010 records them.
