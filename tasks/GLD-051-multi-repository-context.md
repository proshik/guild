---
id: GLD-051
title: "Decide on shared context for multi-repository tasks"
kind: research
status: backlog
priority: 4
component: docs
depends_on: []
touches:
  - tasks/GLD-051-multi-repository-context.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
---

## Goal

A task spanning several repositories might need a shared context its sessions
all read. Only worth doing if a concrete workflow needs it.

**Not yet agreed with the person.** This is a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (the note
on shared context for multi-repository tasks); its scope and behaviour are to
be agreed with the person before any work, and recorded in the task (or an
OpenSpec change) in the same branch.

## Acceptance

- [x] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [x] WHEN the person names a concrete workflow THEN the need is recorded here
      and a design task written; otherwise this task is cancelled.

## Answer (2026-11-22)

**The workflow the person named.** The person keeps several repositories of
one system in one project group — for example a backend, a frontend and a
Helm deployment repository — and wants to start an agent session that sees
all of them, mostly to **ask questions across the group's projects**.

**What was considered and rejected.** A cross-repository "feature" — a set of
worktrees with one branch in every repository — is too heavy for this: not
every such activity is a feature.

**The answer is GLD-055** (`tasks/GLD-055-group-sessions.md`): a project
group is bound to an existing directory, and Guild knows nothing more about
it — it writes, generates, reads and checks no `AGENTS.md` or `CLAUDE.md`,
and passes no `--add-dir` or other per-harness flag. The group's repositories
are meant to live directly inside that folder, as the column shows them, so a
session started there sees every member as a subdirectory in every harness;
Guild shows whether the disk matches. Moving the repositories into the folder
is GLD-056 (`tasks/GLD-056-move-group-repositories.md`). The decisions, the
rejected alternatives and the questions still open are recorded in those
two tasks.

The first answer — a Guild-generated `AGENTS.md` naming the members, a
`CLAUDE.md` of `@AGENTS.md`, and `--add-dir` per member — was replaced on
2026-11-23, because the person wants Guild to treat a group as just a
directory.

## Checks

- The answer recorded in this file; no code.

## Limits / non-goals

- Decision only.

## Context

- GLD-055, the feature task this answer produced.
