---
id: GLD-019
title: "Verify native IME composition in Guild's search fields"
kind: research
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - docs/search.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
needs_person: true
---

## Goal

Search shipped with IME composition UNVERIFIED: plain Unicode insertion was
tested, real marked-text composition was not, because no composition-capable
input source worked on the machine (Kotoeri disabled, `TISSelectInputSource`
returned -50). Verify it, or record precisely why it still cannot be.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/future-improvements.md` ("Native IME search
verification", recorded 2026-10-26, deferred by the person); its scope and
behaviour are to be agreed with the person before any work, and recorded in
the task (or an OpenSpec change) in the same branch.

## Acceptance

- [ ] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [ ] WHEN a composition-capable input source is available (enabled by the
      person, with explicit permission for any user-global input-source
      change) THEN marked text, candidates, commit, cancel, and Enter/Escape
      are checked in local Find/Replace, the global single-line and multiline
      fields, and Search Everywhere.
- [ ] WHEN composing THEN no text leaks into the editor or the terminal behind
      the field.
- [ ] WHEN the check ends THEN the person's original input source is restored
      exactly.
- [ ] The result (pass, fail with a new fix task, or still blocked with the
      reason) is recorded in `docs/search.md`.

## Checks

- Isolated dev window and fixture; the person present. Unicode insertion alone
  is not IME evidence.

## Limits / non-goals

- No product change; a failure found becomes its own fix task.
- No new dependency.

## Context

- `docs/search.md`.
- `AGENTS.md` — "Where Guild takes text" (`TextField` over `InputState`).
