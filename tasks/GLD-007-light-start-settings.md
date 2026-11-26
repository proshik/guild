---
id: GLD-007
title: Heavy features off by default behind settings
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: [GLD-006]
touches:
  - crates/session/src/settings.rs
  - crates/desktop/src/repository_settings.rs
  - crates/desktop/src/root/lsp.rs
  - crates/desktop/src/root/watch.rs
  - crates/desktop/src/root/git.rs
  - crates/desktop/src/root/usage.rs
  - crates/desktop/src/root/update/**
  - crates/desktop/src/root/dependencies.rs
  - crates/desktop/src/view/settings/**
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

Implement the list of heavy features GLD-006 measured and the person
approved: each one behind a setting that is **off by default**, enabled
globally or per project as the approved list says. A window opened on a fresh
state starts none of them; turning one on starts it without a restart;
turning it off stops what it started.

`touches` lists every candidate; GLD-006 narrows this task to the approved
list (or to its first feature, with the rest as their own tasks), and the
narrowed `touches` replaces this one before the task starts.

## Acceptance

- [ ] WHEN the window opens on a fresh state THEN none of the approved
      features starts: no process, watcher, thread or request of theirs is
      seen by the same measurement GLD-006 used.
- [ ] WHEN the person turns a feature on, globally or for a project THEN it
      starts for that scope without a restart, and the setting survives one.
- [ ] WHEN the person turns it off THEN what it started is stopped — its
      children reaped through `ProcessGroup` (`docs/runtime-ownership.md`).
- [ ] WHEN an earlier state holds an explicit setting THEN it is kept; an
      absent setting follows the approved migration (for LSP an absent value
      means enabled today — `session/settings.rs:155-156`).
- [ ] WHEN a feature is off and the person reaches for it (a language-server
      action, the dependencies view) THEN the window says it is off and where
      to turn it on — no silent nothing.
- [ ] `CHANGELOG.md` under `## [Unreleased]` says what is now off and where
      the switch is; the settings rows are designed in Penpot and approved
      before they are coded (`docs/penpot.md`).

## Checks

- Tests first: the settings model (defaults, scopes, migration) in
  `guild-session`; the start/stop gate per feature where it is decided.
- The GLD-006 measurement repeated on the same fixtures, before and after,
  numbers in the commit body.
- `just check`; a dev-window check of the settings rows and of turning one
  feature on and off.

## Limits / non-goals

- Not lazy auto-start; not a redesign of any feature itself.
- Nothing about the session server.
- Up to two code review rounds, `codex-only` profile.

## Context

- GLD-006's measurement and the approved list, in its task file.
- `AGENTS.md` — "Where Guild takes text" for any new field; design system
  for the rows.
