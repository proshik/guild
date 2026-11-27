---
id: GLD-078
title: Leave an agent CLI the person has never used alone at startup
kind: fix
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/status.rs
  - crates/desktop/src/hooks.rs
  - crates/desktop/src/root/usage.rs
  - crates/desktop/src/root/tabs.rs
  - crates/session/src/history.rs
  - crates/session/src/history/opencode.rs
  - crates/session/src/harness/**
  - crates/desktop/tests/all/**
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

GLD-074's check: on a fresh account where `codex` and `opencode` were on the
`PATH` but had never been run, Guild's startup alone created `~/.codex`
(sqlite databases, and `config.toml` with Guild's hook block), and
`~/.config/opencode`, `~/.local/{share,state}/opencode`, `~/.cache/opencode`
and about 109 MB in `~/.npm` — before the person started any agent. The
causes: `start_hooks` (`root/status.rs`) installs the opencode plugin and the
Codex hook block at every start; `root/usage.rs` runs `codex app-server`
for the limits card; the start screen's history runs `opencode session
list`, and opencode, finding Guild's plugin, installs its dependencies.

When this task is finished, Guild touches an agent's files and runs it in
the background only when that agent is already known on the machine, or
when the person starts it from Guild.

**The person's decision (2026-11-27):** "only the agents already known".

## Acceptance

- [ ] WHEN Guild starts and Codex's home (`$CODEX_HOME`, else `~/.codex`)
      does not exist THEN Guild writes no Codex hook block, creates no
      Codex directory and runs no `codex app-server`.
- [ ] WHEN Guild starts and opencode's config directory
      (`$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`) does not
      exist THEN Guild writes no plugin there, and it runs no `opencode`
      for history while opencode's data directory does not exist.
- [ ] WHEN the agent's directory already exists THEN everything behaves as
      today (the author's machine sees no change).
- [ ] WHEN the person starts Codex or OpenCode from Guild for the first time
      THEN its hook block or plugin is installed before the process starts,
      so its status moves from the first session.
- [ ] `<program> --version` probes stay as they are (they decide *not on
      PATH*); a probe that is found to create the agent's files is reported
      in the plan rather than changed silently.
- [ ] `GUILD_NO_GLOBAL_HOOKS` keeps its meaning.

## Checks

- A test per agent with a scratch home: absent directory → nothing written
  and nothing run; present directory → installed; first launch from Guild
  → installed before spawn.
- `just check`, isolated (`GUILD_STATE_DIR` scratch).
- Real behaviour: the GitHub-style run of GLD-074 (empty `HOME`, minimal
  `PATH`, `codex` and `opencode` visible) leaves no `~/.codex`,
  `~/.config/opencode` or `~/.npm` after the window opens and a repository
  is opened.

## Limits / non-goals

- Not a change to what Guild writes once an agent is in use.
- No new setting, notice or interface.

## Context

- GLD-074, *Findings* 3. GLD-077 (what Guild touches on the Mac) cites the
  rule this task leaves.
- `hooks.rs` explains why the two writes exist at all.
