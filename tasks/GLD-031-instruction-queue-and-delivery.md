---
id: GLD-031
title: "Design a visible instruction queue and audit prompt delivery"
kind: design
status: backlog
priority: 2
component: docs
depends_on: []
touches:
  - openspec/changes/instruction-queue/**
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Messages for a busy agent wait in a visible queue the person can edit, and the
window distinguishes "delivered to the terminal" from "the agent started on
it". First audit today's delivery (send, wait, resume receipts, OSC 7501 turn
start) for gaps such as a stalled prompt, without recreating mechanisms Guild
already has.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (P2 "Visible
instruction queue", the historical "reliable task delivery" note, and the
rows 2026-10-06 (`prompt_stalled`, wait for the launched turn) and
2026-10-05 (`send --steer`)); its scope and behaviour are to be agreed with
the person before any work, and recorded in the task (or an OpenSpec change)
in the same branch. **A visible UI change:** a mockup in Guild's design system
and the person's decision come before code.

## Acceptance

- [ ] WHEN the task is taken THEN a mockup (Guild's components and tokens) and
      the open choices are put to the person, and the approved shape recorded
      before code; nothing below is binding until then.
- [ ] WHEN the audit is done THEN each gap is named with evidence from code,
      and each existing mechanism kept.
- [ ] WHEN the queue is specified THEN pending messages are editable and
      removable, and delivery and execution are separate states.
- [ ] WHEN a provider cannot queue, edit or steer THEN the design does not
      imply it can; steering is provider-specific.
- [ ] The decision is recorded and a feature task written for what is
      approved.

## Checks

- The decision recorded in an OpenSpec change (`openspec validate <change>
  --strict --no-interactive`) or in this file if no change is warranted.
- No production code.

## Limits / non-goals

- Design only.

## Context

- `crates/cli/src/main.rs` (agent send/wait help).
- `root/agent_submit.rs`.
- OSC 7501 decisions 5 and 7.
