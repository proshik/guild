---
id: GLD-075
title: SECURITY.md, CONTRIBUTING.md, issue templates, and where the logs are
kind: chore
status: backlog
priority: 2
component: docs
depends_on: []
touches:
  - SECURITY.md
  - CONTRIBUTING.md
  - .github/ISSUE_TEMPLATE/**
  - README.md
  - docs/guide.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
needs_person: true
---

## Goal

A person who meets a bug or a security problem in Guild knows where to send
it and what to attach. Today the repository has only `.github/workflows`: no
`SECURITY.md`, no `CONTRIBUTING.md` and no issue template. The logs are
written to `~/Library/Logs/Guild` (`crates/desktop/src/logging.rs:18-24`),
but neither README nor the guide says so. Guild runs agents and terminals
and keeps local sockets, so a private way to report a vulnerability matters
more than usual.

## Acceptance

- [x] WHEN the task is taken THEN the person decides the private reporting
      channel for `SECURITY.md`: GitHub's private vulnerability reporting, or
      an address they choose. No personal address is written without their
      word.
- [x] `SECURITY.md` says which versions are supported (the latest nightly),
      how to report privately and what to expect back.
- [x] `CONTRIBUTING.md` is for people, beside `AGENTS.md` for agents. It
      covers building (`just`, rustup, not Homebrew's rust), `just check`
      before a pull request, the changelog line, the Penpot step for a
      visible change, and the task files. It points at `AGENTS.md` and does
      not repeat it.
- [x] `.github/ISSUE_TEMPLATE/` has a bug report asking for the Guild
      version (the build's commit), the macOS version, the agent CLI and its
      version, the steps, and the log from `~/Library/Logs/Guild`, with a
      word on removing anything private from it. It also has a feature
      request.
- [x] README and the guide say where the logs are.

## Decisions

- **The person's decision (2026-11-28):** private reports go through
  GitHub's private vulnerability reporting (Security ▸ Report a
  vulnerability). It was off; on the person's word it was switched on
  on 2026-11-28 (`gh api -X PUT repos/proshik/guild/private-vulnerability-reporting`,
  checked `{"enabled":true}`).
- **No code review**, the person's call (2026-11-28): documents and forms
  only, checked by links, YAML and the code they describe.
- `SECURITY.md` promises no response time: none was given.
- The issue forms are GitHub issue forms (YAML); blank issues are off, and
  the chooser links to private reporting.
- No changelog line: documents only.

## Checks

- The templates render on GitHub (checked on the pushed branch, or in a
  fork, by the person).
- Links in the new files resolve (grep, and a look at the rendered page).

## Limits / non-goals

- No code of conduct beyond what the person asks for.
- No bot, label automation or CI change.

## Context

- `AGENTS.md`, *How work is done* and *Commands*.
- `crates/desktop/src/logging.rs` (the log directory).
