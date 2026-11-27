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

- [x] WHEN the person has never chosen a default agent THEN the default is
      the first on the `PATH` of Claude Code, Codex, OpenCode, else the
      Terminal, and the start screen, the tab launcher and the new-tab
      gesture all use it.
- [x] WHEN the person has chosen one (Settings) THEN it stays theirs even
      when it is missing, as today. A settings file that already names a
      default keeps it (the author's machine sees no change).
- [x] WHEN an agent that is not on the `PATH` is started THEN the notice
      says it is not installed and where to get it — the agent's own
      install page, checked to resolve — in English and in Russian.
- [x] README's *Requirements* names where each agent comes from.

## Checks

- Tests for the choice (none chosen with each combination installed;
  chosen and missing) and for the notice text.
- `just check`, isolated; a dev-window check with `claude` absent.

## Design

- `Settings::default_tab` is `Option<DefaultTab>`, `None` meaning "not
  chosen", read as before and skipped when written: a file that names a
  default keeps it, a fresh one is never given one by a save.
- `Settings::default_tab_among(available)` is the one answer: the choice,
  or `DefaultTab::first_installed` — Claude Code, Codex, OpenCode, else the
  Terminal. The window asks it through `Root::default_tab()` with
  `Root::available` (the same list the tiles and the chevron grey by) for
  the start screen's card, the **+**'s mark and tooltip, Settings' row and
  `cmd-t`. Choosing in Settings writes `Some`.
- `HarnessKind::install_page()` names each agent's page; checked with
  `curl -sIL` on 2026-11-27 (all 200): Claude Code
  `https://code.claude.com/docs/en/setup` (the docs.claude.com page
  redirects there), Codex `https://github.com/openai/codex`, OpenCode
  `https://opencode.ai/docs`. `Ui::not_installed` replaces `not_on_path`.
- The missing-agent refusal moved into `open_harness_tab`, where every
  new-tab gesture meets: before, `cmd-t` and the **+** opened a tab that
  died at once for a missing default, with no notice at all.

## Review round 1 fixes

- **bugs+impl-1** — `Root::available` seeded every agent as installed and
  was refreshed only by the start screen's read, so an early `cmd-t`
  chose a missing Claude and an agent installed with tabs open was never
  seen. `root::availability::Availability` keeps "not asked yet" apart
  (`None`): the tiles are still drawn enabled by it, the default drawn
  meanwhile is the Terminal. The watch loop probes on a thread at the
  start and every ten seconds while the window is in front, whatever
  fills the middle, and a press whose default nobody chose
  (`Root::default_tab_now`, through `runs_for_default_tab`) asks again on
  the window's thread — the stat calls `harness_missing` already makes at
  every press, and the one login-shell wait a process allows — and
  publishes the answer, so the mark, tooltip, card and Settings row agree
  with what opened. An older thread's answer never overwrites a newer
  press's.
- **adversarial-2 / bugs+impl-3** — the person's decision (2026-11-27),
  "as the window + an honest refusal": `guild session new`, `--group` and
  `worktree create --prompt` without `--harness` send none, and the window
  runs its own default (`command_harness`). A default that is the Terminal
  is refused, not replaced: a chosen one with `TERMINAL_IS_THE_DEFAULT`
  (name `--harness`), an automatic one with `no_agent_installed` (the
  three install pages). A requested or default agent that is not
  installed is `Answer::Refused` with `command::not_installed` — the
  agent and its install page — before any record, tab or worktree is
  made; `session resume` and the continuation path say the same words. A
  window older than this reads a missing `harness` as Claude Code, as the
  old wire meant.
- **docs+tests-3** — the landing page's third install step says what
  README says.

## Limits / non-goals

- No new screen; the default's button and the tiles keep their look.

## Context

- GLD-074, *Findings* 5. `Settings::default_tab`, `HarnessKind::available_on`,
  `Ui` text for `{program} is not on the PATH` (`i18n/mod.rs`).
