---
id: GLD-047
title: "An outline of the current file"
kind: feature
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/panels.rs
  - crates/desktop/src/view/panels.rs
  - crates/desktop/src/symbols.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

A Structure-like view of the file in front: its declarations as a tree,
following the caret, from the language server's document symbols or
tree-sitter tags when there is no server.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P2
"OpenSpec UI and editor navigation" — outline); its scope and behaviour are to
be agreed with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in
Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a file is in front THEN its outline is shown and a click moves the
      caret.
- [ ] WHEN the caret moves THEN the outline highlights the enclosing
      declaration.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Nothing beyond the agreed scope.

## Context

- `crates/desktop/src/symbols.rs` (tags and server symbols).
