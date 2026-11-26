---
id: GLD-002
title: Live check of blocked permission, blocked auth and error on real harnesses
kind: research
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - docs/sessions.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-07"
needs_person: true
---

## Goal

The OSC 7501 stage's live check (task 11.3) saw `working`, `blocked` with
`kind: question`, `done` and rest against real Claude Code and Codex, but
never `blocked` with `kind: permission` or `kind: auth`, nor `error`. Those
paths are proven only against a stand-in program (task 11.2). See them from
real harnesses in an isolated dev window, and record what each harness
actually sends — including where it sends nothing.

## Acceptance

- [ ] WHEN Claude Code asks for a tool permission THEN Guild shows `waiting`
      with `kind: permission` (`guild agent list --json`) and the question
      mark with the permission reason on the tab; after the answer it returns
      to `working`.
- [ ] WHEN Codex runs with an approval policy that asks THEN the same is
      observed for its approval prompt.
- [ ] WHEN a harness needs a sign-in (a signed-out profile in the isolated
      environment) THEN what it reports is recorded: `blocked` with
      `kind: auth`, `blocked` without a kind, or nothing.
- [ ] WHEN a turn ends in an error the harness can be made to produce
      without harm (an invalid model name, the network cut for the dev window)
      THEN what it reports is recorded: `error` → `failed`, or something else.
- [ ] Each observation names the harness version, the raw report where it
      was captured, `agent list --json`, and what the person saw drawn.
- [ ] The results are a dated paragraph in `docs/sessions.md`; anything not
      seen is said to be not seen.

## Checks

- Isolated dev window (`just desktop-dev`, own `GUILD_STATE_DIR`,
  `GUILD_NO_GLOBAL_HOOKS=1`), a fixture repository, the person at the machine.
- Before running Codex, copy `~/.codex/config.toml` and restore it byte for
  byte afterwards: its trust prompt writes into it (task 11.3).
- No code change. If a check finds a bug in Guild, it becomes a new task.

## Limits / non-goals

- **Provider calls only with the person's explicit permission for this run.**
- Not a fix for a harness that does not report; a missing report is a
  finding, not something Guild works around.
- OpenCode is GLD-003.

## Context

- `docs/desktop-dev.md` — the dev window beside the installed Guild
  (`just desktop-dev`, its own state).
