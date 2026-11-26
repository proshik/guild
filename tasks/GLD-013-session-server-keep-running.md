---
id: GLD-013
title: Keep sessions running after the window closes — a setting and a menu bar item
kind: feature
status: backlog
priority: 3
component: desktop
depends_on: [GLD-012]
touches:
  - crates/server/**
  - crates/session/src/settings.rs
  - crates/desktop/src/root/quit.rs
  - crates/desktop/src/root/settings.rs
  - crates/desktop/src/view/settings/**
  - crates/desktop/src/system.rs
  - crates/desktop/src/system/**
  - crates/desktop/src/i18n/**
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-07"
---

## Goal

The server always owns sessions; a setting decides whether it outlives the
last window. With the setting on, closing or
quitting the window leaves agents and terminals running, a menu bar item
shows that the server is alive, opens the window, and stops the server with
an explicit confirmation that lists the work it would end. Off by default.

For remote access (GLD-065, GLD-066) this is what lets another machine reach
this Mac's sessions with no window open here: without it the Mac needs Guild's
window open, with it the Mac needs only to be awake — so while any agent is
working, the server also keeps the Mac from idle sleep.

## Acceptance

- [ ] WHEN the setting is off (the default) and the last window closes THEN
      the server ends as in GLD-011, after the same questions `cmd-q` asks
      today about interrupted work.
- [ ] WHEN the setting is on and the window quits THEN the server and its
      sessions keep running, and a menu bar item is shown.
- [ ] WHEN the person opens Guild again THEN it attaches to the running
      server and every tab comes back with its exact screen and status.
- [ ] WHEN the person chooses Stop in the menu bar item THEN a confirmation
      lists the sessions and jobs that would end (working agents first), and
      only a confirmed stop ends them.
- [ ] WHEN any agent is working THEN the server holds a no-idle-sleep power
      assertion (an `IOPMAssertion`, as `caffeinate -i` takes), and WHEN
      every agent rests THEN it is released; a closed lid on battery is not
      covered (*Limits*).
- [ ] Where the menu bar item lives (the server or a windowless app) follows
      GLD-010's decision; AppKit code stays in `desktop::system` or the
      server's own equivalent module.
- [ ] `CHANGELOG.md`; the settings row, the menu bar item and the stop sheet
      designed in Penpot and approved before they are coded (`docs/penpot.md`).

## Checks

- Tests first for the lifetime rule (setting × last client), the stop list
  and when the power assertion is held and released; `just check`.
- The assertion seen held by the server's PID (`pmset -g assertions`) while
  a stand-in agent works, and gone once it rests.
- A dev-window check: quit with the setting on, a stand-in agent still
  reporting, reopen, stop from the menu bar.

## Limits / non-goals

- **Launch at login is not part of this task** — an optional later item.
- **Not here:** remote connection — GLD-065 and GLD-066. Out of scope
  altogether: surviving a server crash (a crash ends the sessions); files,
  git, LSP on the server; a TUI editor.
- Keeping a Mac awake with its lid closed on battery, and any change to the
  person's energy settings.

## Context

- GLD-010 (lifetime and menu bar decisions), GLD-011, GLD-012.
- `root/quit.rs`, `closing.rs` — what `cmd-q` asks today.
- Session-server plan, decision 3.
