---
id: GLD-045
title: "An OpenSpec panel: changes, specs and task checklists"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: []
touches:
  - crates/desktop/src/openspec.rs
  - crates/desktop/src/root/openspec.rs
  - crates/desktop/src/view/openspec.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Browse a repository's OpenSpec changes and specs and see each change's task
checklist progress, read-only, opening the Markdown in a tab.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P2
"OpenSpec UI and editor navigation" — the OpenSpec part); its scope and
behaviour are to be agreed with the person before any work, and recorded in
the task (or an OpenSpec change) in the same branch. **A visible UI change:**
a mockup in Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN a repository has `openspec/` THEN the panel lists active changes
      with checked/total tasks, and specs.
- [ ] WHEN it has none THEN the panel is absent, not empty.
- [ ] Reading uses the files, not the `openspec` CLI, unless the person
      decides otherwise.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Read-only.

## Context

- The former backlog entry quoted in Goal.
