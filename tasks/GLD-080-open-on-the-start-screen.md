---
id: GLD-080
title: Open an existing folder from the empty start screen, and make its rows clickable
kind: feature
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/start.rs
  - crates/desktop/src/root/start.rs
  - crates/desktop/src/view/start.rs
  - crates/desktop/src/i18n/**
  - README.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
needs_person: true
---

## Goal

GLD-074's check: with no project open, the start screen offers *New
project* (a Save panel for a new folder), *Recent projects* and
*Settings*. A newcomer with a repository on disk has nothing to press:
*File ▸ Open…* exists only in the menu bar. And clicking the *New project*
row did nothing, twice; only its shortcut opened the panel.

When this task is finished, the empty start screen's first row is
*Open…*, the same deed as *File ▸ Open…*, and every row there answers a
click.

**The person's decision (2026-11-27):** an *Open…* row, first.

## Acceptance

- [x] WHEN no project is open THEN the start screen lists *Open…* first,
      with *File ▸ Open…*'s shortcut, then *New project*, *Recent
      projects*, *Settings*.
- [x] WHEN the person clicks any of those rows, or chooses it with the
      keys, THEN its deed runs (the click reproduced and fixed first).
- [x] The look follows the Penpot proposal the person approves.
- [x] README's *Download* steps name what the screen and the menu say.

## Design

Approved by the person on 2026-11-27 (board `GLD-080 — Window — no project ·
Open first, rows clickable` on Screens › Proposals). The rows were never
broken: they were drawn as hints for the keys, with no click, and *Open
project* was filtered out because `cmd-o` went to Open In and the list kept
only actions with a stroke. Now every row is a 32 px row (`MENU_ROW`,
`CARD_GAP` inside, `radius::ROW`) lit `surface.raised` with `text.primary`
under the pointer, as the start screen's history rows are; a click
dispatches the row's action, the one its menu item and stroke reach; a row
without a stroke shows none. *Open…* (`Named::AddProject`) is first.

## Checks

- A test for the start model's rows and a window test for the click.
- `just check`, isolated; the dev window compared with the approved board.

## Limits / non-goals

- No change to *New project*, the Open panel or Recent projects.

## Context

- GLD-074, *Findings* 4. `start.rs` (the model), `root/start.rs`,
  `view/start.rs`.
