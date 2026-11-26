---
id: GLD-046
title: "Go to Implementation and Format Document through language servers"
kind: feature
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/lsp/providers.rs
  - crates/desktop/src/root/lsp.rs
  - crates/desktop/src/actions/**
  - crates/desktop/src/editor.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Two editor actions the language servers already offer: jump to implementations
of an interface or method, and format the document (or selection). Today there
are Go to Definition (`f12`) and Go to Symbol.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P2
"OpenSpec UI and editor navigation" — Go to Implementation and formatting);
its scope and behaviour are to be agreed with the person before any work, and
recorded in the task (or an OpenSpec change) in the same branch. **A visible
UI change:** a mockup in Guild's design system and the person's decision come
before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the server supports `textDocument/implementation` THEN the action
      jumps (one result) or lists (several); WHEN it does not THEN it says so.
- [ ] WHEN formatting is applied THEN it is one undoable edit, and a server
      error leaves the buffer unchanged.
- [ ] Key bindings are chosen with the person (common IDE bindings are the
      reference).

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Format on save is not part of this task.

## Context

- `crates/desktop/src/lsp/providers.rs`.
- `crates/desktop/src/actions/mod.rs`.
