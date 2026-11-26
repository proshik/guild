---
id: GLD-073
title: Sign Guild with a Developer ID and notarize it, so it opens on a first launch
kind: chore
status: backlog
priority: 1
component: tooling
depends_on: []
touches:
  - scripts/bundle.sh
  - scripts/write-build-info.sh
  - .claude/skills/release/**
  - .github/workflows/bundle.yml
  - crates/cli/src/release.rs
  - crates/cli/src/update_release.rs
  - docs/updates/**
  - README.md
  - site/**
harness: any
reviews: {spec: 1, code: 1}
created: "2026-11-27"
needs_person: true
---

## Goal

A person who downloads Guild from the Releases page opens it with a double
click and without a warning. Today the bundle is signed ad hoc
(`scripts/bundle.sh:59`, `:82`), not with a Developer ID, and it is not
notarized. README asks for a Control-click → Open on the first launch. On
macOS 15 and later that path is believed to be gone: the person has to find
*Open Anyway* in System Settings ▸ Privacy & Security (not verified by this
task's author). For someone trying Guild for the first time, this is the
first screen they see, and it reads as "this may be malware".

## Acceptance

- [ ] WHEN the task is taken THEN the person has decided on the Apple
      Developer account (its cost and owner). Nothing below starts before
      that. Its credentials are the person's to provide, through their
      approved secret storage, never in the repository or a log.
- [ ] WHEN a release is cut THEN `Guild.app` and every executable inside it
      (`guild-desktop`, `guild`, the helpers) are signed with the Developer
      ID with the hardened runtime, and the bundle is notarized and stapled.
      `spctl --assess --type execute` accepts it.
- [ ] WHEN the hardened runtime is on THEN everything Guild does today still
      works: the terminal's child processes, the hooks script, the language
      servers, the update helper and the swap. Each entitlement that is
      needed is named in the task with the reason, and nothing broader is
      asked for.
- [ ] WHEN Guild updates itself to a notarized build THEN the swapped bundle
      still passes `spctl` and opens without a warning, and the update
      manifest's own signature (`guild-release`) is checked as today.
- [ ] WHEN the release runs without the signing credentials (a fork, a dev
      build) THEN it falls back to the ad-hoc signature and says so. It does
      not fail half-way, and `just install` and `just bundle` work as today.
- [ ] README and the site no longer ask for Control-click → Open.

## Checks

- A notarized build downloaded through a browser, so it carries the
  quarantine attribute, opens on a Mac that has never run Guild.
- `codesign --verify --deep --strict` and `spctl --assess` in the release
  path. `just check`.
- No secret is printed, committed or passed on a command line visible in
  `ps`.

## Limits / non-goals

- The Mac App Store, its sandbox and an Intel or universal build are not
  part of this task.
- The release stays the release skill's route (`.claude/skills/release`),
  and publication stays the person's act.

## Context

- `scripts/bundle.sh` (the ad-hoc signature), `.github/workflows/bundle.yml:90`
  (the CI's verification).
- `docs/desktop-dev.md:55` says the ZIP is ad-hoc signed and not notarized.
- `docs/updates/owner-guide.md`: the update path and its manifest signature.
- README, *Download*: "Guild is not notarized yet".
