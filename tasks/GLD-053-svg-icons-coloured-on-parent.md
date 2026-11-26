---
id: GLD-053
title: "Paint the idle Sessions tick and the terminal-job row's Cancel and info icons"
kind: fix
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/view/agents.rs
  - crates/desktop/src/view/terminal_jobs.rs
  - crates/desktop/src/icons/mod.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-19"
needs_person: true
---

## Goal

Three icons are laid out but not painted in the window, although the code
gives them a colour: the tick inside the footer's Sessions ring when every
session is idle, and the Cancel and info icons of a row in the Terminal Jobs
list. The slot is there — the ring, the button and the row keep their size —
and it is empty.

The likely cause, **not verified**: each of these `gpui::svg()` elements gets
no colour of its own; the colour is set on a parent `div` or comes from the
button's style, while GPUI paints an SVG only with the text colour set on the
svg element itself. Every other use of the same helpers sets it on the svg:
`icons::check().text_color(..)` in `view/changes.rs:102`,
`view/everywhere.rs:652`, `view/find_in_files.rs:570`,
`view/push_dialog.rs:1237`; `icons::toolbar(icon).text_color(..)` in
`view/run.rs` (`render_run_button`) and `view/changes.rs` (the Commit
toolbar). The helpers' own comments in `icons/mod.rs` ("drawn in the current
text colour") read as if the colour were inherited.

## Acceptance

- [x] WHEN every session is idle THEN the footer's Sessions ring shows its
      tick, in `surface.panel` on the `text.muted` disc that
      `render_session_summary` already draws (`view/agents.rs`, the
      `session-summary-clear` element).
- [x] WHEN the Terminal Jobs list shows a running job THEN its Cancel button
      shows the stop square, and every row's details control shows the info
      icon, each in the colour its button or row state gives the text today.
- [x] WHEN the cause is found THEN it is recorded in this file (and in
      `docs/traps.md` if it is a GPUI rule), with the one experiment that
      proved it — the guess above is not to be taken as fact.
- [x] Other `icons::check()` / `icons::toolbar()` call sites that rely on an
      inherited colour are listed, and fixed in the same way or recorded as
      deliberately left.

## Checks

- A window check in an isolated dev window (`just desktop-dev <dir>`, its own
  `GUILD_STATE_DIR`, never the person's Guild.app), before and after:
  1. Open a fixture repository with no agent running: the footer's Sessions
     ring is the idle one (`Status::Idle`), so its tick should show.
  2. On that window's socket only (`$TMPDIR/guild-hooks-<pid>.sock`, with
     `GUILD_SESSION` set to a session of that window), start jobs with
     `guild terminal start --from-file <request.json> --json` — one that
     exits 0, one whose program does not exist, one that keeps running
     (`/bin/sh -c 'echo ready; sleep 600'`) — and hide them with
     `guild terminal hide <id>`.
  3. Open the Terminal Jobs list from the title bar's terminal button: the
     running row should show Cancel's square, and every row its info icon.
  Capture the window by its id (`screencapture -l`) and zoom on the slots.
- `just check`. A unit test is optional: the defect is in painting, which
  tests here do not see.

## Limits / non-goals

- No change to the icons' assets, sizes or the rows' layout.
- No new colours: the colours the code already chooses are the ones to paint.
- Penpot mockups already draw these icons tinted; nothing to redraw there.

## Context

- `crates/desktop/src/view/agents.rs` — `render_session_summary`, the idle
  disc: `.text_color(rgb(palette.surface.panel))` on the `div`, then
  `.child(crate::icons::check().size(theme::activity::RING))`.
- `crates/desktop/src/view/terminal_jobs.rs` — the Cancel button
  (`terminal-cancel`, `.custom(action_style(false, cx))`,
  `.child(crate::icons::toolbar("ui/tool-stop.svg"))`) and the details row
  (`.text_color(..)` on the `div`, `.child(crate::icons::toolbar("ui/job-info.svg"))`).
- `crates/desktop/src/icons/mod.rs` — `check()` and `toolbar()`.
- Seen in the Penpot design passes of 2026-11-19 (stages 1 and 9 of the
  Penpot UI work, branch `penpot-ui`): the slots measured, the glyphs absent
  in window captures.

## Findings (2026-11-21)

**Cause, proven.** `gpui::Svg::paint` (gpui-pre 0.3.8, `elements/svg.rs`)
paints only when its own computed style has `text.color`; it never reads the
inherited text style. A parent `div`'s `text_color` and a gpui-component
`Button`'s foreground reach text children, not an svg. The guess above was
right.

**The experiment.** In an isolated dev window the footer's idle Sessions disc
was empty. The one change — `.text_color(rgb(palette.surface.panel))` added to
the tick's `icons::check()`, the parent's identical `text_color` left in place
— made the tick appear. Nothing else differed between the two builds.
Recorded in `docs/traps.md` as a GPUI rule.

**Call sites.** Every `icons::check()` / `icons::toolbar()` call and every bare
`gpui::svg()` in `crates/desktop/src` was read; all others set `text_color` on
the svg. Those that relied on an inherited colour, all fixed the same way —
the colour set on the svg, the one the code already chose for the parent:

- `view/agents.rs`, the idle Sessions tick — `surface.panel` (moved from the
  disc, where it reached nothing).
- `view/terminal_jobs.rs`, the icons of Show output (`push-preview`), Hide
  (`head-hide`), Cancel (`tool-stop`) and Delete output (`menu-remove`) —
  `text.secondary`, the foreground `action_style(false)` gives these buttons
  in every enabled state.
- `view/terminal_jobs.rs`, the details control (`job-info`) — `text.secondary`,
  and `text.primary` on hover through `group_hover`, as the row's hover gave
  its text; the row's own text colours, which reached nothing, are gone.
- `view/diff/files.rs`, the diff window's file picker — the current file's
  tick and, beside it, the folder chevrons (`icons::chevron`) took the row's
  colour, `accent.brass` for the current file and `text.primary` otherwise.
- `view/menu/agents.rs`, a repository's chosen icon on an agent's tile (a bare
  `svg()`) — `surface.app`, as the tile's letter and the column's tile use.

`icons::tool()`, `icons::crosshair()` and `icons::chevron()` elsewhere all set
their colour. The helpers' doc comments in `icons/mod.rs` no longer say "drawn
in the current text colour".

**How the window check ran.** The base commit, the one-change experiment and
the fix were each run as `target/debug/guild-desktop` from a fixture repository
with a fresh `GUILD_STATE_DIR`, `GUILD_NO_GLOBAL_HOOKS=1` and the inherited
`GUILD_SOCKET`/`GUILD_SESSION`/`GUILD_HOOK_SOCKET` unset; agent launch prefixes
pointed at a stand-in script, so no provider ran. The jobs were started with
that window's `GUILD_SOCKET` and the `GUILD_SESSION` of a stand-in session
`guild session new` returned. The `guild terminal` commands need both
variables; `GUILD_SESSION` alone is refused.

**No paint test.** The footer and Terminal Jobs tests assert bounds, which an
unpainted svg keeps. A test that sees paint is possible — a
`HeadlessAppContext` with a recording renderer, as `tests/all/occurrence_paint.rs`
does, and `Scene::monochrome_sprites` for an svg's colour — but it needs the
window rendered headless with Guild's assets, which no test here does yet. Left
open; the window check is the evidence.

**Not checked in the window:** the diff window's file picker, the agents
menu's tile icon, and the Show output / Hide icons (no job here had an archive
or was shown at the time); they follow from the same rule.
