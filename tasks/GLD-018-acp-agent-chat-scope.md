---
id: GLD-018
title: "Scope a Guild Chat surface for agents over ACP"
kind: design
status: backlog
priority: 3
component: docs
depends_on: [GLD-010]
touches:
  - tasks/GLD-018-acp-agent-chat-scope.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

A native Chat surface beside Terminal: the person opens a compatible agent
either in its own terminal interface or in a Guild-rendered conversation with
readable messages, streamed tool activity, links to changed files and diffs,
permission requests and cancellation. Agent Client Protocol (ACP) would give
one transport for compatible agents; it supplies neither the UI nor better
models. This task scopes it and records the decision; it builds nothing.

**Where it runs** (decided 2026-11-27, recorded in GLD-010): the ACP agent is
a session of the session server, not a process of the window. The server
starts the agent, is its ACP client and keeps the conversation. Chat in the
window is one client of that session, as a terminal tab is of a terminal. So
a conversation outlives a window restart, the TUI can see it, and an agent
that waits reaches the shared waiting list. Chat is therefore designed on
GLD-010's protocol, after it.

**Agreed with the person** (2026-11-27):

- **A client never speaks ACP.** ACP runs only between the server and the
  agent. The window and the TUI speak Guild's own protocol (GLD-010), for a
  terminal session and a conversation alike.
- **Terminal sessions are used as today.** The server holds the PTY and the
  screen; a client draws them with its own emulator and sends input through
  the server, one keyboard at a time (GLD-015).
- **A conversation's interface is Guild's own.** The server sends the
  conversation as events (messages, tool calls, plans, permission requests)
  under the same snapshot and sequence rules as a terminal. The window or the
  TUI draws them, and answers to permissions and cancellation go back
  through the server. The server keeps the conversation itself, so a new
  client gets all of it without relying on the agent's `session/load`.

**Still open:**

- **The events' form** is GLD-010's to decide. ACP's own types can go almost
  as they are inside Guild's envelope: less work, and a client draws straight
  from ACP's schema. Or they can be translated into Guild's types: clients
  then do not depend on ACP's versions, at the cost of keeping a translation.
  Recommendation: ACP's types inside the envelope, since ACP carries versions
  and capabilities.
- **No switch in place.** An agent started in a terminal and the same agent
  over ACP are two processes with no shared history (Acceptance, *history and
  resume*).
- **Which agents speak ACP.** opencode is believed to support it natively,
  and Claude Code and Codex through adapters. This is not verified, and the
  first agent's check below verifies it.
- **A remote machine's conversation.** ACP would reach a remote server
  through ssh without anything new, but the remote-machine window is
  terminal-only (2026-11-22). The question is open in GLD-010.

**Not yet agreed with the person.** Beyond the points above, this is a
candidate carried over from the former `docs/backlog/future-improvements.md`
("ACP-backed agent chat", recorded 2026-10-24); its scope and behaviour are to
be agreed with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in Guild's design system and the
person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the official Rust `agent-client-protocol` SDK is evaluated THEN its
      fit with Guild's process ownership, runtime and dependency boundaries is
      recorded, with the stable protocol surface preferred.
- [ ] WHEN the first agent is chosen THEN its native ACP support or adapter,
      authentication, configuration and capability limits are verified and
      recorded.
- [ ] WHEN the surface is specified THEN messages, streamed tool activity,
      file/diff links, permission requests and cancellation are mapped to what
      the chosen agent actually reports; unsupported capabilities are handled
      explicitly.
- [ ] WHEN the surface is specified THEN Chat is a client of the server's
      conversation session (GLD-010): what it draws comes from the server's
      events, a permission request is answered by the client that holds
      input (GLD-015), and ACP's `fs` and `terminal` capabilities follow
      GLD-010's answer.
- [ ] WHEN Chat sessions are mapped to tabs, panes, worktrees, status and
      caller identity THEN the existing permission and managed-session
      provenance rules hold.
- [ ] WHEN history and resume are considered THEN only what the chosen agent
      supports is promised: terminal and ACP sessions are not assumed to share
      history or switch surfaces in place.
- [ ] Terminal stays available for provider features ACP lacks.

## Checks

- The decision recorded in this file: its Design and Plan sections written
  and agreed with the person.
- No production code.

## Limits / non-goals

- No implementation; agent choice, interaction design and rollout are this
  task's output.
- Out of scope: cross-agent conversation transfer, automatic recovery, and a
  remote machine's conversation (GLD-066's window is terminal-only unless
  GLD-010 changes it).

## Context

- https://agentclientprotocol.com/protocol/v1/overview
- https://github.com/agentclientprotocol/rust-sdk
- GLD-010 — the server that runs the agent's process and keeps its
  conversation; GLD-015, input handover; GLD-067, the waiting list.
