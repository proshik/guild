---
id: GLD-037
title: "Amend, tag, cherry-pick and revert as named git writes"
kind: feature
status: backlog
priority: 4
component: desktop
depends_on: []
touches:
  - crates/desktop/src/git.rs
  - crates/desktop/src/root/log.rs
  - crates/desktop/src/view/menu/log.rs
  - crates/desktop/src/root/changes.rs
  - crates/desktop/src/i18n/**
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

Amend the last commit, create a tag, cherry-pick and revert a commit from the
Log — each a new variant of `git::Write`, each a decision under decision 21
with its guard.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3
"Extended Git actions", not promoted); its scope and behaviour are to be
agreed with the person before any work, and recorded in the task (or an
OpenSpec change) in the same branch. **A visible UI change:** a mockup in
Guild's design system and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN each write is added THEN `Write::args` spells it, the test pinning
      the list is updated, and its guard is stated in decisions.
- [ ] WHEN amend would rewrite a pushed commit THEN it refuses or asks
      (decision 30's rule for writes git does not argue about).
- [ ] WHEN cherry-pick or revert conflicts THEN the conflict flow
      (GLD-024/025) takes over, or the action says it is not available yet.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- No reset, rebase or force-with-lease (GLD-038).

## Context

- `AGENTS.md` — "Git is written by named actions".
- `docs/decisions.md` — 21, 30.
