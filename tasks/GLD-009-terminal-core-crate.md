---
id: GLD-009
title: Move the terminal emulator and OSC parsing into a crate without GPUI
kind: chore
status: backlog
priority: 3
component: terminal
depends_on: [GLD-008]
touches:
  - crates/terminal/**
  - crates/desktop/src/terminal/**
  - crates/desktop/Cargo.toml
  - Cargo.toml
  - Cargo.lock
  - scripts/check-boundaries.sh
  - AGENTS.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-07"
---

## Goal

The emulator state, the PTY reader, the OSC parsing (OSC 7, 133, 7501,
XTVERSION and the semantic queue) and the OSC 7501 record table live in a new
workspace crate that does not link GPUI — so a headless process can own a
terminal and a test can check it without a window. The desktop's terminal
view keeps drawing from it. `just boundaries` guards the crate.

## Acceptance

- [ ] WHEN `cargo tree -p <the new crate>` runs THEN no `gpui` appears, and
      `scripts/check-boundaries.sh` has a `check <crate> gpui` line that
      fails `just boundaries` when gpui is added (verified once by adding it).
- [ ] WHEN the desktop runs THEN terminals behave as before: the moved tests
      run in the new crate and pass unchanged in substance; the view's tests
      stay in the desktop.
- [ ] `AGENTS.md`'s crate list and module map name the new crate and what
      stayed in `terminal/` (view, paint, font, keys, mouse, selection).
- [ ] The crate's name and path match `tasks/tasks.toml`'s `terminal`
      component, or the component is renamed in the same branch.

## Checks

- `just check` (it includes `just boundaries`); the guard broken once on
  purpose and the failure shown in the commit body.
- `just desktop-dev` smoke check: a shell, a Claude or Codex session's status
  mark through OSC 7501 with a stand-in (no provider).

## Limits / non-goals

- A move, not a redesign: no API change beyond what crossing a crate boundary
  requires (visibility, re-exports).
- No server, no process split.
- The C helper `process_generation.c` and `build.rs` follow the code that
  calls them; say which crate builds it.

## Context

- GLD-008 (the untangling this move relies on).
- `AGENTS.md` — "Boundaries that are easy to break unnoticed"; "a guard whose
  subject no longer exists passes silently".
- `scripts/check-boundaries.sh`.
- Session-server plan, stage 3.
