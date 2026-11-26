---
id: GLD-018
title: "Scope a Guild Chat surface for agents over ACP"
kind: design
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - openspec/changes/acp-agent-chat/**
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

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/future-improvements.md` ("ACP-backed agent chat",
recorded 2026-10-24); its scope and behaviour are to be agreed with the person
before any work, and recorded in the task (or an OpenSpec change) in the same
branch. **A visible UI change:** a mockup in Guild's design system and the
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
- [ ] WHEN Chat sessions are mapped to tabs, panes, worktrees, status and
      caller identity THEN the existing permission and managed-session
      provenance rules hold.
- [ ] WHEN history and resume are considered THEN only what the chosen agent
      supports is promised: terminal and ACP sessions are not assumed to share
      history or switch surfaces in place.
- [ ] Terminal stays available for provider features ACP lacks.

## Checks

- The decision recorded in an OpenSpec change (`openspec validate <change>
  --strict --no-interactive`) or in this file if no change is warranted.
- No production code.

## Limits / non-goals

- No implementation; agent choice, interaction design and rollout are this
  task's output.
- Out of scope: cross-agent conversation transfer, automatic recovery, remote
  execution.

## Context

- https://agentclientprotocol.com/protocol/v1/overview
- https://github.com/agentclientprotocol/rust-sdk
- GLD-010 (session server) — where a Chat session's process would live.
