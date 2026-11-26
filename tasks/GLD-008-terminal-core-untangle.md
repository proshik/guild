---
id: GLD-008
title: Cut the terminal core's ties to the desktop crate, in place
kind: chore
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/terminal/**
  - crates/desktop/src/transport.rs
  - crates/desktop/src/discovery.rs
  - crates/desktop/src/network.rs
  - crates/desktop/src/system.rs
  - crates/desktop/src/native_preview.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-07"
---

## Goal

The terminal emulator and its OSC parsing should be able to live in a crate
without GPUI (GLD-009), so that a headless session server can own them
later. Today the modules that would move use GPUI only in tests, but reach
into the desktop crate for other things. This task removes those reaches
**without moving any file**, so the move itself is mechanical and reviewable
on its own.

Found in code (OSC 7501 design, "Later stages", stage 3; re-check before
starting): `emulator.rs` and `emulator/events.rs` use GPUI only in tests;
`semantic`, `shell_integration`, `shell_state`, `xtversion`, `reply`,
`ownership`, `emulator/job`, `program_status` use none. The blockers are
`crate::transport`, `crate::discovery`, `crate::system::local_hostname`,
`crate::network` (through `launch_prefix`), the mixed `refresh::Demand`, and
`native_preview`.

## Acceptance

- [ ] WHEN the set of modules to move is listed (in the commit body) THEN
      none of them names `crate::` outside that set, except through a type or
      trait the set itself defines and the desktop implements or passes in.
- [ ] `refresh::Demand` is split so the part the emulator needs carries no
      view concern.
- [ ] The hostname, the network/proxy decision and the discovery environment
      reach the emulator as values or a small trait given at launch, not as
      calls into `system`/`network`/`discovery`.
- [ ] Behaviour is unchanged: every terminal test passes unmodified except
      for constructor arguments.

## Checks

- `grep -n "crate::"` over the listed modules, its output in the commit
  body, shows only references inside the set.
- `just check`.

## Limits / non-goals

- No file moves, no new crate (GLD-009).
- No behaviour change, no new dependency.
- `transport` may move with the core in GLD-009 if it is only the
  terminal's; decide here and say which.

## Context

- `AGENTS.md` — the module map (`terminal/`), boundaries.
- `docs/traps.md` — the terminal section, before touching the reader loop.
