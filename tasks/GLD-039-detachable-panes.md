---
id: GLD-039
title: "Detach a pane into a window of its own"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: []
touches:
  - crates/desktop/src/windows.rs
  - crates/desktop/src/root/satellites.rs
  - crates/desktop/src/root/areas.rs
  - crates/desktop/src/view/satellite.rs
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Move a pane — a terminal, a file, a diff — into its own window and back. Today
a second window is a satellite of fixed kinds (Push, Diff; `windows.rs`).
Detaching a pane is not process persistence (that is GLD-010–013).

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3
"Windows and developer tools" — detachable panes); its scope and behaviour are
to be agreed with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in
Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a pane is detached THEN it keeps its body, state and focus rules in
      the new window; closing the window returns or closes it as decided with
      the person.
- [ ] WHEN the main window quits THEN detached windows follow `windows.rs`'s
      rule that the main window alone decides whether the application lives,
      or the rule is changed by decision.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No process moves between windows.

## Context

- `crates/desktop/src/windows.rs`.
- `AGENTS.md` — "Every tab is made by `Root::adopt_panes`".
