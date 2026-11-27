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

- [x] WHEN Guild starts and Codex's home (`$CODEX_HOME`, else `~/.codex`)
      does not exist THEN Guild writes no Codex hook block, creates no
      Codex directory and runs no `codex app-server`.
- [x] WHEN Guild starts and opencode's config directory
      (`$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`) does not
      exist THEN Guild writes no plugin there, and it runs no `opencode`
      for history while opencode's data directory does not exist.
- [x] WHEN the agent's directory already exists THEN everything behaves as
      today (the author's machine sees no change).
- [x] WHEN the person starts Codex or OpenCode from Guild for the first time
      THEN its hook block or plugin is installed before the process starts,
      so its status moves from the first session.
- [x] `<program> --version` probes stay as they are (they decide *not on
      PATH*); a probe that is found to create the agent's files is reported
      in the plan rather than changed silently.
- [x] `GUILD_NO_GLOBAL_HOOKS` keeps its meaning.

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

## Plan

One rule, `guild_session::harness::known(dir)` — the directory exists —
over each agent's own directory: `CodexHarness::home()`,
`OpenCodeHarness::config_dir()` (the plugin) and `OpenCodeHarness::data_dir()`
(its sessions). `AgentDirs` names the first two for the doors.

1. History: `Stores` carries opencode's data directory; `opencode::recent`
   runs nothing while it does not exist.
2. Start: `start_hooks` writes each door through `hooks::install_door_if_known`;
   Codex still also needs `codex` on the login `PATH`, as before.
3. Limits card: `ask_codex` asks nothing while Codex's home does not exist;
   the card shows the agent in front, and a Codex tab in front has its home
   by the next tick.
4. Launch: `guild_session::Hooks` carries `doors`, set by `hook_launch` only
   for a production window with `GUILD_NO_GLOBAL_HOOKS` unset, and
   `terminal::view::open_with_selection` — every agent tab's one launch path
   — writes the harness's door through `write_door` before the process is
   built and spawned. A diagnostic window's launches carry `None`.

**Review round 1 fixes.** (1) A launch's door is done once per window:
`hooks::prepare_door` remembers every door `install_door` wrote — the
start's, for a known agent, or an earlier launch's — and does no disk work
for it again, so a restored or new tab of a known agent costs the window's
thread nothing. The spawn stays synchronous on that thread (a terminal tab
is built around its running process), so the door's one install for an
agent never run here — once per machine — is still done there, before the
spawn; deferring the spawn to a background install was judged a larger
change than the finding. (2) `install_codex_with` writes nothing when the
block is already there, and renames a changed file in only over the bytes
it read, else reads again (three tries). (3) A launch writes no door for
an agent not on its own `PATH`, so a saved tab restored after the agent was
uninstalled makes no directory. (4) `root::body`'s
`isolated_launch_writes_the_door_before_the_agent_starts` runs the
production `Body::open` against compiled stand-ins that compare the door's
mtime with their session leader's start; a door write removed, or moved
after `Emulator::spawn_harness`, fails it (both mutations checked).

**Probe finding (not changed):** measured with an empty scratch `HOME`,
`opencode --version` (v2.4.2) itself creates `~/.config/opencode`,
`~/.local/{share,state}/opencode` and `~/.cache/opencode`; `codex --version`
(0.172.0) creates nothing. The probe (`Root::opencode_floor_missed`) runs
only when the person opens or resumes an OpenCode tab, never at the start,
so it does not break this task's start rule — but after that first probe
opencode is "known" by its directories.

## Context

- GLD-074, *Findings* 3. GLD-077 (what Guild touches on the Mac) cites the
  rule this task leaves.
- `hooks.rs` explains why the two writes exist at all.
