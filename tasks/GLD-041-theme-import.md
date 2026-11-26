---
id: GLD-041
title: "Import a colour theme"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: []
touches:
  - crates/desktop/src/theme/**
  - crates/desktop/src/view/settings/**
  - crates/desktop/src/i18n/**
  - docs/design-system.md
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Bring a theme from another editor (format to be chosen with the person) into
Guild's palettes, mapped onto `theme/`'s tokens rather than added as raw
colours.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3
"Windows and developer tools" — theme import); its scope and behaviour are to
be agreed with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in
Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the source format is chosen THEN the mapping to Guild's tokens is
      written down, including what has no counterpart.
- [ ] WHEN a theme is imported THEN it appears beside the eight palettes and
      every surface reads it through `theme/`.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No colour literal in widget code (`AGENTS.md`).

## Context

- `crates/desktop/src/theme/palettes.rs`.
