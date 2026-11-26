---
id: GLD-074
title: Try Guild as a new person would, from a clean macOS account, and file what breaks
kind: research
status: backlog
priority: 1
component: docs
depends_on: []
touches:
  - tasks/**
  - README.md
  - docs/guide.md
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-27"
needs_person: true
---

## Goal

Before Guild is offered to other people, someone walks README's path on a
Mac account that has never seen Guild or the author's setup: download, open,
pick a repository, press **+**, start an agent, answer it, commit. Every
place where it fails, stalls or needs knowledge only the author has becomes
a task or a line in README. Hidden dependencies on the author's machine (a
login `PATH`, Homebrew tools, hooks, CLI versions, a proxy) show up only
there.

## Acceptance

- [ ] WHEN the check runs THEN it uses a separate macOS user account (or a
      clean VM) with no `~/.config/guild`, no `~/Library/Logs/Guild`, no
      `glab`, no `gh` and only one agent CLI installed and logged in. The
      person's own account and state are not touched.
- [ ] WHEN the path is walked THEN each step is recorded with what was seen:
      download and the first launch (with and without GLD-073), the start
      screen, opening a repository, a worktree, starting the agent,
      its status in the column, an answer, the diff and a commit, the Update
      button, quitting and reopening.
- [ ] Also walked: no agent CLI installed at all; an agent CLI older than
      README's minimum; a repository without a remote; a second language
      (Russian).
- [ ] WHEN something fails or needs unstated knowledge THEN it is filed as a
      task (`tasks/README.md`) or fixed in README or the guide in the same
      branch, and the record lists which.

## Checks

- The record of the walk is kept in the task's branch (in this file, under
  *Findings*), with what was and was not checked.
- No provider is called without the person's permission. Starting an agent
  on the clean account is the person's act, with their account.

## Limits / non-goals

- Fixing what is found is the tasks filed, not this one, except README and
  guide lines.
- Not a performance or load test.

## Context

- README, *Install* and *Requirements*.
- GLD-073 (the first launch's warning).
