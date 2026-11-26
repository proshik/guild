---
id: GLD-042
title: "Design model and reasoning choice for launched sessions"
kind: design
status: backlog
priority: 4
component: docs
depends_on: []
touches:
  - openspec/changes/session-model-choice/**
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

`guild session new`/`worktree create` choose a harness but not a model or
reasoning level. Decide whether they should: discovering what a provider
supports before launch, validating and applying an override (never silently
ignoring it), keeping the choice on follow-ups, and — later — pools of models
with explicit limits and fallback rules.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (historical P3 "Model
pools" and the rows 2026-10-09 (follow-ups keep model and
reasoning), 2026-10-06 (provider model discovery) and 2026-10-01 (validate and
apply overrides)); its scope and behaviour are to be agreed with the person
before any work, and recorded in the task (or an OpenSpec change) in the same
branch. **A visible UI change:** a mockup in Guild's design system and the
person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the design is written THEN each harness's real way to set model and
      reasoning is recorded with its version.
- [ ] WHEN an override cannot be applied THEN the launch refuses rather than
      starting with another model.
- [ ] The decision is recorded; feature tasks follow only for what is
      approved.

## Checks

- The decision recorded in an OpenSpec change (`openspec validate <change>
  --strict --no-interactive`) or in this file if no change is warranted.
- No production code.

## Limits / non-goals

- Separate from build concurrency (GLD-036).

## Context

- `crates/cli/src/main.rs` (session new, worktree create).
- `guild_session::harness`.
