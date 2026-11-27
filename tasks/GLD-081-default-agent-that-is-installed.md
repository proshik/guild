---
id: GLD-081
title: Default to an agent that is installed, and say how to install a missing one
kind: fix
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/session/src/settings.rs
  - crates/session/src/harness/**
  - crates/desktop/src/root/start.rs
  - crates/desktop/src/root/tab_launcher.rs
  - crates/desktop/src/view/start.rs
  - crates/desktop/src/i18n/**
  - README.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

GLD-074's check: on a Mac with Codex and OpenCode but no Claude Code, the
worktree's start screen offered *Claude Code — Default* above a Claude Code
tile marked *not on PATH*; pressing it raised "claude is not on the PATH",
which does not say what to do.

When this task is finished, until the person picks a default agent, the
default is the first one installed, and a missing agent's notice says how
to install it.

**The person's decision (2026-11-27):** automatic choice plus the hint.

## Acceptance

- [ ] WHEN the person has never chosen a default agent THEN the default is
      the first on the `PATH` of Claude Code, Codex, OpenCode, else the
      Terminal, and the start screen, the tab launcher and the new-tab
      gesture all use it.
- [ ] WHEN the person has chosen one (Settings) THEN it stays theirs even
      when it is missing, as today. A settings file that already names a
      default keeps it (the author's machine sees no change).
- [ ] WHEN an agent that is not on the `PATH` is started THEN the notice
      says it is not installed and where to get it — the agent's own
      install page, checked to resolve — in English and in Russian.
- [ ] README's *Requirements* names where each agent comes from.

## Checks

- Tests for the choice (none chosen with each combination installed;
  chosen and missing) and for the notice text.
- `just check`, isolated; a dev-window check with `claude` absent.

## Limits / non-goals

- No new screen; the default's button and the tiles keep their look.

## Context

- GLD-074, *Findings* 5. `Settings::default_tab`, `HarnessKind::available_on`,
  `Ui` text for `{program} is not on the PATH` (`i18n/mod.rs`).
