---
id: GLD-070
title: The footer glyph test loses its title-bar branch controls under load
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/render/footer_tests.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

`root::render::footer_tests::fixed_footer_glyph_corner_alignment` (run
through `isolated_fixed_footer_regressions`) failed once in a loaded
`just check` while GLD-068 was checked:

```
panicked at crates/desktop/src/root/render/footer_tests.rs:1202:75:
called `Option::unwrap()` on a `None` value
```

Line 1202 is `cx.debug_bounds("update-branch").unwrap()`: the title bar's
Update branch control was not drawn. When this task is finished, the cause
is known and written down, the test no longer depends on how long the run
takes, and it passes repeatedly under load.

## Acceptance

- [ ] WHEN the task is handed over THEN the cause is recorded here with the
      evidence that established it.
- [ ] WHEN the glyph scenario runs under load THEN the branch controls it
      measures are drawn on every iteration, or the test fails with a
      message naming what is missing instead of an `unwrap` on `None`.
- [ ] WHEN the cause is in the test THEN the fix is in the test; WHEN it is
      in production code THEN a regression test pins it, failing before
      the fix for the right reason.
- [ ] WHEN the fix is in THEN `isolated_fixed_footer_regressions` passes
      in at least three full `just check` runs beside another session's
      tests, and in a loop of the isolated run beside a synthetic load.

## Checks

- The isolated run in a loop, before and after, alone and beside a
  synthetic load (the failure has not been reproduced on purpose yet).
- `just check`, repeated, with `GUILD_STATE_DIR` in a scratch folder,
  `GUILD_NO_GLOBAL_HOOKS=1` and the `GUILD_*` session variables unset.

## Limits / non-goals

- No change to what the footer or the title bar draws, as a person sees it.
- Not a rework of the footer fixture beyond what the cause needs.

## Context

Seen once in six isolated `just check` runs on 2026-11-27, with eight
synthetic busy loops and another session's tests beside it (load median
51, peak 128); the other five passed. Not verified beyond that.

The `glyph` scenario of `workspace_controls_fixture` (footer_tests.rs:832)
sets `r.branch = Some("fixture")` and `r.branch_read = Some(Instant::now())`
once, then renders 64 combinations (fullscreen, four font sizes, four
column widths, folded or not) and unwraps the bounds of `column-button`,
`go-back`, `go-forward`, `branch-button` and `update-branch` on each.

A lead, not checked: `Root::refresh_branch` (lib.rs:1966) reads the branch
from disk again once `branch_read` is older than `BRANCH_PROBE` (two
seconds) and the integrations are external. If that applies in this
fixture, a slow run replaces the fixture's branch with what git says of a
folder that is not a repository, and the branch controls disappear midway.
