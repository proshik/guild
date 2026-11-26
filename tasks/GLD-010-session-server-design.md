---
id: GLD-010
title: Decide the session server's shape and protocol, the ssh bridge to other machines included
kind: design
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - openspec/changes/guild-session-server/**
  - tasks/GLD-011-session-server-terminals.md
  - tasks/GLD-012-session-server-agents-and-jobs.md
  - tasks/GLD-013-session-server-keep-running.md
  - tasks/GLD-014-common-api-gui-and-cli.md
  - tasks/GLD-015-terminal-observers-input-rights.md
  - tasks/GLD-016-tui-client.md
  - tasks/GLD-065-ssh-bridge.md
  - tasks/GLD-066-remote-machine-window.md
  - tasks/GLD-067-shared-waiting-list.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Guild becomes a client of a local session server that runs terminals,
harnesses and terminal jobs and keeps their statuses; the window, `guild`, a
TUI and the windows of other machines attach to it — locally through a Unix
socket, from another machine through ssh. Before any server code, decide its
shape and protocol and record them in an OpenSpec change
`guild-session-server`. The output is the decision and the protocol outline,
not code; GLD-011–016 are edited to match, and GLD-065–067 (the remote
layer) are brought to the same decision.

**Decided by the person** (2026-11-22, recorded here as decided, not to be
asked again):

- **The server owns terminals and agents only**: statuses, hooks, terminal
  jobs, input. Files, git, the editor, LSP and search stay in the window and
  never move to the server.
- **Remote is ssh only.** The server has no TCP listener. Locally it is a
  0600 Unix socket; remotely the same byte stream goes through `ssh host
  guild server bridge`. A dropped connection is *disconnected*, never
  *ended*.
- **The first extra client is the TUI** (GLD-016), before the remote window,
  because it is simpler and checks the protocol without GPUI.
- **Input is handed over explicitly** (GLD-015).
- **A remote machine is fully separate from local sessions**: one window per
  remote machine (GLD-066), working only through the terminal emulator.
- **One shared "who is waiting" list** in the local window gathers the
  Waiting sessions of this machine and every connected one (GLD-067).
- **The phone layer waits for a separate task tool's first version** and is
  not planned here.

## What the design must cover

- **Shape.** One server process for all sessions, or one host process per
  session. Recommendation: one server per state directory — statuses, the
  terminal-job book, records and provenance are cross-session state, and
  per-session hosts are where OSC 133 marks, hyperlinks and images are lost
  on reattach.
- **What goes over the wire for a terminal.** Raw PTY bytes with a sequence
  number, plus a grid snapshot on attach, with the client keeping its own
  emulator from the same crate (GLD-009); or rendered cell deltas.
  Recommendation: bytes plus snapshot — compact over ssh, and the
  window needs its own `Term` for selection and find anyway.
- **The protocol.** One ordered stream of frames; a handshake carrying a
  **protocol generation and capabilities** from the start, with
  stable-endpoint rules: fields are optional, an unknown enum value does not
  break a client, a method missing on one side disables only its own action,
  and an incompatible generation is a plain "update Guild on machine X".
  Snapshot plus a sequence-numbered change stream; retried requests
  deduplicated by id; no fd passing or shared memory.
- **The ssh bridge** (GLD-065). The approach planned on 2026-11-26, to
  be confirmed or changed here: `guild server bridge` joins its stdio to the
  server's socket and does nothing else; the client runs the system `ssh`
  (hosts from `~/.ssh/config`, keys and the agent, `BatchMode=yes`,
  `ControlMaster`, compression). Disconnected ≠ ended; reconnect catches up
  by sequence number or a new snapshot.
- **The remote-machine window** (GLD-066): one per machine, terminal-only —
  no file links, no Claude IDE integration, no git, Run, Find in Files,
  Explorer or editor, no moving tabs between it and local windows; `guild`
  inside a remote session talks only to its own machine's server.
- **The shared waiting list** (GLD-067): local and remote `Waiting(Why)`
  sessions in one place; a click raises that machine's window on the
  session; the chime and the Dock badge counted once per session.
- **Version skew between two machines**: what the generation and
  capabilities rules above mean when the window and the remote server are
  different builds.

## Open questions

Each person decision is asked as its own question, with the
recommendation, and recorded with its date — never assumed.

| Question | Recommendation |
| --- | --- |
| One server, or one process per session | One server per state directory |
| PTY bytes plus snapshot, or cell deltas | Bytes plus snapshot, one emulator crate on both sides |
| Where Claude's IDE endpoint lives | On the server, forwarding calls to the attached **local** window; with no window it says plainly there is no IDE; never forwarded to a remote-machine window. `CLAUDE_CODE_SSE_PORT` is set at launch (`crates/desktop/src/terminal/view/mod.rs:1664`) *(code)*, so a window-hosted endpoint breaks across a window restart |
| Who shows the menu bar item | The server through a small AppKit module, or a windowless app — decided with the cost of both |
| The grace period after the last client leaves | Short, on the order of a minute |

Settled by the design itself, recorded in `design.md`:

- The PTY reader never waits on a client; a lagging client catches up with a
  new snapshot. Today it blocks on a bounded channel
  (`terminal/emulator.rs`, the reader loop) *(code)*.
- Managed-input epochs are counted where bytes are written.
- `GUILD_SOCKET` and `GUILD_HOOK_SOCKET` move to the server, and so does the
  provenance rule (decision 35).

## Probe before any code

A small experiment in scratch, not production code: an alacritty grid
snapshot restored into another `Term` yields the same screen, OSC 133 marks
and hyperlinks. If it does not, the wire format is revisited here, in
GLD-010, and not discovered in GLD-011. The result goes into `design.md`.

## Acceptance

- [ ] WHEN the change is written THEN `design.md` presents both shapes and
      both wire formats with their costs against Guild's code and the
      recommendations, and each of the person's decisions in *Open
      questions* is recorded with its date — each asked as its own question,
      not assumed.
- [ ] WHEN the probe has run THEN `design.md` records whether a restored
      grid snapshot gives the same screen, OSC 133 marks and hyperlinks, and
      the wire format is confirmed or changed accordingly.
- [ ] The protocol outline: one ordered stream of frames over a Unix socket;
      a handshake in which the client learns which server (machine, state
      directory, version) it is attached to, with a protocol generation and
      capabilities and the stable-endpoint rules above; snapshot +
      sequence-numbered change stream for reattach; request ids for retries;
      no fd passing or shared memory — so the same stream goes through the
      ssh bridge unchanged.
- [ ] The remote layer: the bridge, the client's ssh transport, the
      remote-machine window's boundaries, the shared waiting list and the
      version-skew rules are described, each with the task that builds it
      (GLD-065–067).
- [ ] Lifetime: the server always owns sessions; by default it ends when the
      last client leaves (after the decided grace); outliving the window is a
      setting (GLD-013).
- [ ] Each of the open points above has an answer or is named as open with
      the task that will decide it.
- [ ] GLD-011–016 are edited in the same branch to match the decision
      (split, merged or re-scoped), each still one merge, and GLD-065–067
      are created or edited to match.
- [ ] `openspec validate guild-session-server --strict --no-interactive`.

## Checks

- One spec review round, `codex-only` profile, architectural lens.
- Facts about today's code cited with paths and marked *(code)*; external
  facts marked *(ext)* with sources.
- The probe's code and output kept in scratch and summarised in
  `design.md`; it is not committed as production code.

## Limits / non-goals

- Design only: no production code, no Cargo change.
- **Out of scope, recorded as such:** files, git, search, LSP and buffers on
  the server (they stay in the window, decided 2026-11-22); a TUI editor;
  surviving a server crash; launch at login; the phone layer (a web inbox or
  Telegram, a VDS with Caddy and passkeys, `ssh -R` from the Mac) — it waits
  for a separate task tool's first version and gets a plan of its own, so nothing in
  GLD-011–016 or GLD-065–067 is built for it ahead of time.
- From the former `docs/backlog/2026-10-17-product-improvements.md`
  ("Local process service", later and conditional): keeping commands and
  agents alive across a UI restart must preserve session identity, status
  and the `guild` CLI's contracts. That note deferred remote hosts until a
  concrete need existed; the person's decision of 2026-11-22 is that need,
  and it is bounded the same way — remote access is to terminals and agents
  through ssh, not a split of the whole backend.

## Context

- `docs/runtime-ownership.md` — process lifetime, terminal jobs, managed
  terminal recovery, bounded transport ownership.
- `crates/session/src/status.rs` — `Status::Waiting(Why)`, `Why` =
  `Permission | Question | Auth | Unstated` *(code)*, what GLD-067 lists.
- GLD-009 (the terminal core the server and its clients will share) —
  useful, not required, to read first.
