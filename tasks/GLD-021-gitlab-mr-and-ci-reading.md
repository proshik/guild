---
id: GLD-021
title: "Read the current branch's GitLab MR, checks and failing job log"
kind: feature
status: cancelled
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/gitlab.rs
  - crates/desktop/src/gitlab/**
  - crates/desktop/src/root/gitlab.rs
  - crates/desktop/src/view/gitlab.rs
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-07"
---

## Goal

**Cancelled:** GLD-063 delivered what this task was for and settled its
open choice. Decision 56 reads each worktree's merge request through `glab`,
only reads, and holds no token of Guild's own. The column and the status bar
show the MR, its state and its pipeline, and they say when there is none.
The failing job's log was not delivered and is dropped here. If it is wanted,
it becomes a task of its own with a new id.

From the window, see the merge request of the branch in front, its pipeline's
checks and the log of a failing job — without opening a browser. Read-only.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P1 "GitLab MR and CI
reading"); its scope and behaviour are to be agreed with the person before any
work, and recorded in the task (or an OpenSpec change) in the same branch. **A
visible UI change:** a mockup in Guild's design system and the person's
decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the branch in front has an MR on its GitLab remote THEN its title,
      state and pipeline status are shown; WHEN it has none THEN that is said.
- [ ] WHEN a job failed THEN its log (bounded, tail first) opens in the
      window.
- [ ] WHEN the token or network is missing THEN the window says which, and
      makes no write of any kind.
- [ ] The library/CLI choice (GitLab's API directly, `glab`, or another) is
      put to the person with pros, cons and a recommendation before code.

## Checks

- Tests first for each acceptance item; `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.

## Limits / non-goals

- Read-only: review writes are GLD-022, worktree from an MR GLD-023.
- The wider earlier scope — an inbox, description, version-pinned diff,
  discussions — is not part of this task.
- Credentials never logged or shown; network through `desktop::network`.

## Context

- `desktop::network` (proxy).
- GLD-022, GLD-023.
