---
id: GLD-025
title: "Conflict lifecycle: continue/abort, rebase and cherry-pick, rename/delete and binary cases"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: [GLD-024]
touches:
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/view/changes.rs
  - crates/desktop/src/git.rs
  - crates/desktop/src/i18n/**
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Carry a conflicted operation to its end from the window: continue or abort it,
for rebase and cherry-pick as well as merge, then rename/delete and binary
conflicts — and notice when the files changed underneath.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P2
"Conflict lifecycle", not promoted); its scope and behaviour are to be agreed
with the person before any work, and recorded in this file
in the same branch. **A visible UI change:** a mockup in Guild's
design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN an operation is in progress THEN the window names it and offers
      continue and abort, each a decided git write.
- [ ] WHEN a file changed on disk since it was shown THEN resolving it refuses
      and says so.
- [ ] Rename/delete and binary conflicts are offered only as choices of a
      whole side.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Abort never discards work without a sheet naming what is lost (decision 30).

## Context

- GLD-024.
