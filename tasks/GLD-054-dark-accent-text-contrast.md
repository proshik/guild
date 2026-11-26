---
id: GLD-054
title: "Bring Dark's accent text on raised and on the card up to 4.5:1"
kind: fix
status: backlog
priority: 3
component: desktop
depends_on: []
touches:
  - crates/desktop/src/view/controls.rs
  - crates/desktop/src/view/settings/sections.rs
  - crates/desktop/src/view/settings/updates.rs
  - crates/desktop/src/theme/palettes.rs
  - crates/desktop/src/theme/mod.rs
  - crates/desktop/tests/all/palettes.rs
  - docs/design-system.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-21"
needs_person: true
---

## Goal

In the Dark palette, words drawn in `accent.patina` (#548af7) on
`surface.raised` or on `surface.card` read below WCAG's 4.5:1 for text
under 18 points. The person accepted this when Dark's `surface.raised` went
from #27292d to #313337 (branch `dark-raised-appearance`, 2026-11-21) so
that a raised fill shows on a card; this task brings the text back above
the floor without losing that step.

Measured with `theme::contrast` (WCAG's formula) on the built-in Dark
palette, no customization:

| Ink on ground | Before 2026-11-21 | Now |
| --- | --- | --- |
| `accent.patina` on `surface.raised` | 4.41:1 (#27292d) | 3.83:1 (#313337) |
| `accent.patina` on `surface.card` (#26282c) | 4.47:1 | 4.47:1 |
| `accent.patina` on `surface.app` (#191a1c) | 5.27:1 | 5.27:1 |

Both rows were already under 4.5 before the change; raising `raised`
widened the first gap. A customized Dark accent is held only to the built-in
accent's contrast on the app and the card (`theme/custom.rs`, `accent()`),
so a tint can fall lower on `raised` still: code review r1 of that branch
measured #65a065 at 100% as 4.70:1 on the old raised and 4.08:1 on the new.

**Design first.** The fix changes what a person sees: draw it as a Penpot
proposal from the Library's components and tokens (`docs/penpot.md`) —
a lighter Dark `accent.patina`, a different ink for the selected label, or
another treatment — and get the person's approval before any code.

## Acceptance

- [ ] WHEN the person has approved a Penpot proposal THEN the code follows
      it, and the proposal is promoted into the Library and Screens after the
      merge.
- [ ] WHEN Dark is in effect without customization THEN every word the
      window draws in the accent on `surface.raised` or `surface.card` reads
      at least 4.5:1 against it — at least the call sites below.
- [ ] WHEN a Dark accent tint is chosen THEN the selected label on `raised`
      is held to the same floor as the built-in one (or the ink falls back
      to text that is), not only the app and the card.
- [ ] WHEN Light is in effect THEN nothing changes.
- [ ] A test in `tests/all/palettes.rs` pins the floor for the built-in
      palette and for a tinted accent.

## Checks

- `just check`, isolated (`GUILD_STATE_DIR` scratch, `GUILD_NO_GLOBAL_HOOKS=1`).
- A window check in an isolated dev window in Dark: Settings' navigation
  (the selected section), a Theme segment, the Updates source buttons, and a
  selected ghost button — compared with the approved proposal.

## Limits / non-goals

- `surface.raised` stays #313337 — the step on a card is the point of the
  earlier change.
- The brass Update word is out of scope: in Dark it is `text.primary`
  already, because brass is 4.10:1 on the hover over the card.

## Context

Call sites that draw `accent.patina` text on `surface.raised` (read from the
code on 2026-11-21):

- `crates/desktop/src/view/controls.rs:60-67` — `value_content`: a selected
  value control (the Theme segments System / Light / Dark, the monospace
  font choice) is `raised` with an accent label.
- `crates/desktop/src/view/controls.rs:6-24` — `ghost`: a selected ghost
  button's label is the accent, its hover and press `raised`.
- `crates/desktop/src/view/settings/sections.rs:826-847` —
  `navigation_button`: Settings' selected section (Appearance, General, …).
- `crates/desktop/src/view/settings/updates.rs:807-815` — the selected
  update source button.

Accent text on `surface.card` at 4.47:1: **not enumerated** — the call
sites drawing the accent as a word on a card are to be found as the first
step (a search for `text_color(rgb(…accent.patina))` and the grounds under
each).

Related: `crates/desktop/src/theme/custom.rs` (`accent()`, its floor on
`[app, card]`); `docs/design-system.md` section 2.1 for the Dark surfaces.
