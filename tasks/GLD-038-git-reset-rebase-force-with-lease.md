---
id: GLD-038
title: "Decide reset, rebase and force-with-lease against the existing guards"
kind: design
status: backlog
priority: 4
component: docs
depends_on: []
touches:
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Today the destructive spellings (`--hard`, `--force`, `-D`, `pop`, `clear`)
are spelled nowhere and a test greps for them, which also bars
`--force-with-lease`. Decide whether any of reset, rebase or force-with-lease
joins Guild, and with what guard — or that they stay out.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3
"Extended Git actions" — the separately decided part); its scope and behaviour
are to be agreed with the person before any work, and recorded in this file
in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN the decision is made THEN it is a numbered decision in
      `docs/decisions.md`, with the guard test's change stated if any.
- [ ] A yes becomes its own feature task.

## Checks

- The decision recorded in this file: its Design and Plan sections written
  and agreed with the person.
- No production code.

## Limits / non-goals

- Decision only.

## Context

- `AGENTS.md` — the destructive-spellings test.
- GLD-037.
