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
  - tasks/GLD-018-acp-agent-chat-scope.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Guild becomes a client of a local session server that runs terminals,
harnesses, terminal jobs, ACP agents and the language servers' processes, and
keeps their statuses; the window, `guild`, a
TUI and the windows of other machines attach to it — locally through a Unix
socket, from another machine through ssh. Before any server code, decide its
shape and protocol and record them in an OpenSpec change
`guild-session-server`. The output is the decision and the protocol outline,
not code; GLD-011–016 are edited to match, and GLD-065–067 (the remote
layer) are brought to the same decision.

**Decided by the person** (2026-11-22, recorded here as decided, not to be
asked again):

- **The server owns terminals and agents**: statuses, hooks, terminal
  jobs, input. Files, git, the editor and search stay in the window and
  never move to the server.
- **(2026-11-27) The language servers' processes move to the server too**,
  revising the 2026-11-22 line that kept LSP in the window. A language
  server's index then outlives a window restart and is shared by every
  window of the state directory. Files, buffers, the editor and git stay in
  the window. How a window speaks LSP through the server is decided here
  (*Open questions*).
- **(2026-11-27) An ACP agent is the server's agent.** A conversation over
  the Agent Client Protocol (GLD-018) is a kind of session beside a terminal.
  The server runs the agent's process and is its ACP client. The window, the
  TUI and `guild` see the conversation through Guild's own protocol. ACP is
  not that protocol: it joins one client to one agent, with no snapshot, no
  sequence numbers and no second observer. GLD-018 designs the Chat surface
  on top of this.
- **(2026-11-27) A session's overlays are the server's.** The overlay board
  (`guild_session::overlay::Board`, window-local today, `docs/overlays.md`)
  moves to the server with the terminal jobs its terminal overlays show
  (GLD-012). `guild overlay` from a session reaches its own machine's
  server, and every client, a remote-machine window included, draws that
  session's overlays. This narrows "only through the terminal emulator":
  an overlay is text or a job's terminal, not files or git.
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
- **The encoding and the prior art.** No standard the author knows of covers
  a server that owns terminals and agent conversations, has several
  observers and is reached through ssh. So Guild writes its own
  specification, and the design compares it with what exists before choosing.
  Each entry below is from memory and is to be checked against its source
  *(ext, not verified)*:
  - **tmux control mode** (`tmux -CC`, what iTerm2 uses): a server that owns
    PTYs, several clients, a documented text protocol with `%output`
    notifications.
  - **WezTerm's mux** (`wezterm connect`, SSH and Unix domains): a mux server
    owns the PTYs, and clients attach locally or through ssh, where `wezterm
    cli proxy` joins stdio to the socket as `guild server bridge` would. Its
    codec is internal, not a specification.
  - **Neovim's UI protocol** (`--listen`, `nvim_ui_attach`): state on the
    server, several UIs drawing it from documented events.
  - **Jupyter's messaging protocol**: a kernel holds the state, several
    front ends share an iopub broadcast, and the specification is
    documented. It is the nearest to a conversation with several observers.
  - **JetBrains Rd** (Rider's and Gateway's frontend/backend link): a
    reactive model kept in sync. Heavy, and made for their stack.
  - **herdr and agterm**, studied on 2026-11-22: ssh only, no network port.
    herdr's endpoint-generation rules are already above.
- **A conversation session (ACP).** Its frames carry the agent's structured
  updates (messages, tool calls, plans) beside a terminal's PTY bytes, with
  the same snapshot and sequence rules. A permission request is answered by
  the client that holds input (GLD-015). An agent that waits is a
  `Waiting(Why)` session like any other (GLD-067). ACP's client-side
  capabilities are answered by the server or not offered: `terminal/*`
  becomes a terminal of the server's, and `fs/*` is an open question below.
- **Language servers.** One process per project root and language, owned by
  the server's process groups (`ProcessGroup`) rather than the window's, and
  shared by every window that has the project open. Its lifetime is
  decided here: when it starts, when it stops, and what a window sees while
  it starts. What the window keeps: the buffers, `didOpen`/`didChange`, and
  the providers in `lsp/providers.rs`.
- **The ssh bridge** (GLD-065). The approach planned on 2026-11-26, to
  be confirmed or changed here: `guild server bridge` joins its stdio to the
  server's socket and does nothing else; the client runs the system `ssh`
  (hosts from `~/.ssh/config`, keys and the agent, `BatchMode=yes`,
  `ControlMaster`, compression). Disconnected ≠ ended; reconnect catches up
  by sequence number or a new snapshot.
- **The remote-machine window** (GLD-066): one per machine, terminal-only —
  no file links, no Claude IDE integration, no git, Run, Find in Files,
  Explorer or editor, no moving tabs between it and local windows; `guild`
  inside a remote session talks only to its own machine's server. Its
  sessions' overlays are shown (decided 2026-11-27).
- **Overlays on the wire.** The board's records (id, owner, target, title,
  content, visible) travel as a snapshot plus changes, like a session's
  status. A terminal overlay names a job, and its bytes come through that
  job's terminal stream, not as a copy. Whether showing, hiding and focus
  are one state shared by all clients or each client's own is decided
  here. Recommendation: whether a surface is open is shared, and focus is
  each client's own.
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
| Who speaks LSP to a language server | The window, through the server, which passes its frames to the process it owns. The window keeps its buffers and providers, and the server owns only the process, its lifetime and its group |
| When a language server stops | When no window has its project open, after the same kind of grace as the server's own |
| Whether the server offers ACP's `fs` capability | No: the agent reads and writes the disk itself, as a terminal agent does. Answering from the window's buffers would fail with no window open, and files stay in the window |
| The protocol's encoding | JSON-RPC 2.0 with LSP's `Content-Length` framing: ACP, LSP and MCP share that base, Guild already frames LSP (`crates/desktop/src/lsp.rs`) *(code)*, and ACP's events go inside with little translation. Guild's own methods on top: snapshot, subscribe, sequence numbers, capabilities. PTY bytes need their own answer, binary frames or base64, since JSON is wasteful for them. Against: a binary encoding of Guild's own (or msgpack-RPC, as Neovim does), which is smaller over ssh but needs tooling of its own |
| Whether a remote-machine window shows an ACP conversation | Not yet: the window stays terminal-only (2026-11-22) until GLD-018 is designed, and a remote agent that waits still reaches the shared waiting list |

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
- [ ] The conversation (ACP) session and the language servers' hosting are
      in the protocol outline, each with the task that builds it (created or
      re-scoped if none fits), and GLD-018 is edited to match.
- [ ] Overlays are in the protocol outline: the board's snapshot and
      changes, how a terminal overlay reaches its job's stream, and whether
      visibility is shared or each client's own.
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
- **Out of scope, recorded as such:** files, git, search, the editor and
  buffers on the server (they stay in the window, decided 2026-11-22; the
  language servers' processes are in scope since 2026-11-27); a TUI editor;
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
- `crates/desktop/src/lsp/client.rs` (a server in a process, a group to
  kill), `lsp/providers.rs`, `root/lsp.rs` — today's language servers, owned
  by the window *(code)*.
- GLD-018 (ACP Chat) and the protocol: https://agentclientprotocol.com/protocol/v1/overview
  *(ext)*.
- GLD-009 (the terminal core the server and its clients will share) —
  useful, not required, to read first.
