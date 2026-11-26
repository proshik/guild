---
id: GLD-006
title: Measure what starts when the window opens, and propose the off-by-default list
kind: research
status: backlog
priority: 2
component: docs
depends_on: []
touches:
  - openspec/changes/light-start/**
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

Guild should feel light when opened: heavy parts run only when the person has
turned them on, globally or per project — not lazily auto-started. Which parts
are heavy is to be **measured**, not assumed. This task measures what the
window starts on open and proposes, from the measurement, the list of
features to put behind settings that are off by default. The person approves
the list; GLD-007 implements it.

Candidates found in code, **not measured**: a file watcher and a git status
worker for every project in every workspace at the first watch tick
(`root/watch.rs:199-209`, `root/git.rs:960-992`); usage polling running
`security`, an HTTPS request and `codex app-server`
(`root/usage.rs:389-409`); the updater's `gh api` every 30 minutes
(`root/update/sources.rs:324`); dependency discovery running gradle/mvn/cargo
when the Explorer is visible (`root/dependencies.rs:90-131`); language
servers for restored file tabs, the only feature with settings today, where
an absent setting means **enabled** (`session/settings.rs:155-156`,
`repository_settings.rs:10-49`); run discovery.

## Acceptance

- [ ] WHEN the dev window opens on a fixed fixture — one workspace with one
      small project, and one with several projects and workspaces, restored
      file tabs included — THEN the measurement lists, per feature: child
      processes started (name, count, lifetime), threads, file watchers,
      network requests, CPU time and memory in the first 60 s and at idle
      after it.
- [ ] Every number says how it was taken (tool, command, run count) and the
      runs are repeatable from the change's text.
- [ ] WHEN the measurement is done THEN an OpenSpec change `light-start`
      holds it in `design.md` and proposes the list of features off by
      default, each with its setting's scope (global, per project or both),
      what the person sees while it is off, and how existing settings migrate
      (an explicit setting is kept; an absent LSP setting changes meaning).
- [ ] The proposal is put to the person as a decision (one question per open
      choice) and their answer recorded in the change before it is merged.
- [ ] WHEN the approved list is more than one merge can carry THEN this task
      writes one task file per feature (new ids, `depends_on: [GLD-006]`)
      and narrows GLD-007 to the first, in the same branch.

## Checks

- `just desktop-dev` with its own state directory; the installed Guild and
  the person's own state are never measured.
- `ps`/`lsof`/`nettop`-style sampling or `scripts/resource-probe.py` where it
  fits (it measures a preview binary, `native-preview`); `tracing` lines for
  timing.
- `openspec validate light-start --strict --no-interactive`.
- One spec review round, `codex-only` profile, if the person wants it before
  approving.

## Limits / non-goals

- No production code. The measurement may add nothing to the crate; a probe
  script beside the change is fine if it is needed and kept small.
- No harness launched, no provider called — usage polling is measured with
  the network request it makes, not with a signed-in harness session.
- Not lazy auto-start: the proposal is switches, off by default.
- Independent of the session server (GLD-010 onward).

## Context

- Session-server plan, decision 2 ("Light by default").
