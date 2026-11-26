---
id: GLD-076
title: A first release marked as a preview, with notes for a newcomer
kind: chore
status: backlog
priority: 2
component: tooling
depends_on: [GLD-074, GLD-075, GLD-057, GLD-058, GLD-060]
touches:
  - .claude/skills/release/**
  - README.md
  - site/**
  - CHANGELOG.md
harness: any
reviews: {spec: 1, code: 1}
created: "2026-11-27"
needs_person: true
---

## Goal

The build Guild is offered with is one a newcomer can tell apart from a
daily nightly. Today the Releases page holds one `nightly-*` marked
*Pre-release*, and every crate is `0.1.0`. When this task is finished, one
release is marked as the preview to try, with short notes: what Guild does,
what works, what does not yet, for whom it is, and how to report a problem.
The nightlies and the Update button go on as they are.

It waits for the clean first run (GLD-074), a place to report (GLD-075) and
the data-loss fixes GLD-057, GLD-058 and GLD-060. It does not wait for
notarization (GLD-073): the person decided on 2026-11-27 that the preview
ships without it while there is no Apple Developer account.

## Acceptance

- [ ] WHEN the task is taken THEN the scheme is put to the person and the
      choice recorded here. Options: a nightly marked as GitHub's *Latest*
      release with a preview note, or a tag of its own (`v0.1.0`) that the
      update path learns to read. The update path offers only `nightly-*`
      today (`crates/desktop/src/update.rs:13`, `:156`), so the second one
      is a code change with its own tests.
- [ ] WHEN a person opens the Releases page THEN the preview is the release
      shown first, and its notes say what is known not to work.
- [ ] WHEN Guild from the preview runs THEN its Update button still offers
      the later nightlies, unless the person decided otherwise above.
- [ ] WHEN the preview is not notarized THEN its notes and README say so
      plainly, with the first-launch steps for macOS 13–14 and for 15 and
      later, as checked by GLD-074.
- [ ] README and the site point at the preview rather than at "the
      releases".

## Checks

- The release skill's own checks (sign, draft, verify), and publication
  only on the person's word.
- An update from the preview to the next nightly, on a Mac other than the
  author's (with GLD-074's account).

## Limits / non-goals

- No stable channel with its own update cadence, no Homebrew cask, no Intel
  or universal build.

## Context

- `.claude/skills/release/SKILL.md` — the release route.
- `crates/desktop/src/update.rs` — which staged build is offered.
