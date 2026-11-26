---
id: GLD-024
title: "Resolve a text merge conflict in the window and mark it resolved"
kind: feature
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/changes.rs
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/view/changes.rs
  - crates/desktop/src/diff_tab.rs
  - crates/desktop/src/diff_tab/**
  - crates/desktop/src/git.rs
  - crates/desktop/src/i18n/**
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Open a conflicted file, combine the versions, save, and explicitly mark it
resolved. Today an unmerged file is shown and refused for commit
(`changes.rs`, `EligibilityError::Conflict`) but cannot be resolved in Guild.
Both earlier mockup iterations were rejected; no layout is approved (the
three-pane, editable-result proposal is history, not a requirement).

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P1 "Text conflict
resolution"); its scope and behaviour are to be agreed with the person before
any work, and recorded in this file in the same branch.
**A visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the person opens an unmerged file THEN its versions are shown and a
      result can be edited and saved.
- [ ] WHEN the person marks it resolved THEN that is an explicit act; staging
      an unmerged file is a git write decided under decision 21, not a silent
      reuse of `Stage`.
- [ ] WHEN conflict markers remain in the result THEN marking it resolved asks
      first.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Text files and a merge only; continue/abort, rebase/cherry-pick,
  rename/delete and binary are GLD-025.

## Context

- `crates/desktop/src/changes.rs:154-192, 245-282` (unmerged rows).
- `docs/decisions.md` — decisions 21 and 30.
