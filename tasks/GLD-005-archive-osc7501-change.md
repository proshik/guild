---
id: GLD-005
title: Archive the osc7501-agent-status OpenSpec change once it has shipped
kind: chore
status: cancelled
priority: 4
component: docs
depends_on: [GLD-002, GLD-003]
touches:
  - openspec/changes/osc7501-agent-status/**
  - openspec/changes/archive/**
  - openspec/specs/**
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
---

## Goal

**Cancelled:** the `osc7501-agent-status` change was removed with the other
draft OpenSpec changes before the repository was published, so there is
nothing left to archive.

The OSC 7501 stage was merged into `main` by `0068f472`. When a nightly
release contains that merge, its OpenSpec change has done its job: the
behaviour it describes becomes the baseline in `openspec/specs/`, and the
change moves to the archive so the list of active changes shows only work in
flight. The live checks GLD-002 and GLD-003 write into the change's
`design.md` first, so they are recorded before it moves.

## Acceptance

- [ ] WHEN a `nightly-*` tag contains `0068f472` (`git tag --contains
      0068f472 --list 'nightly-*'` is not empty) THEN the task may start; not
      before.
- [ ] WHEN `openspec archive osc7501-agent-status` runs THEN the change's
      spec deltas are applied to `openspec/specs/` and the change sits under
      `openspec/changes/archive/` with its date.
- [ ] WHEN the archive is done THEN `openspec validate --specs --strict
      --no-interactive` passes, and no document outside `openspec/` points at
      the change's old path (grep).
- [ ] The baseline specs read as current behaviour: requirements marked
      "proposed" in the deltas are worded as decided (open decision 2 was
      approved with the plan, 2026-11-06).

## Checks

- `openspec validate` as above; `git diff --stat` shows only `openspec/`
  (and fixed links, if any).

## Limits / non-goals

- Only this change. Other active changes under `openspec/changes/` are not
  archived here, even if they look finished.
- No code change.

## Context

- `openspec/README.md` — OpenSpec is followed only where selected; this
  change selected it.
