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

## Findings

**Scope, decided by the person on 2026-11-27:** no VM and no second macOS
account for now — a simple check on the author's account instead. The
release build from GitHub (`nightly-2026-11-27`, commit `f65258f`) run
from a scratch folder with an empty `HOME`, `PATH=/usr/bin:/bin:/usr/sbin:/sbin`,
`SHELL=/bin/zsh`, started from `/` as Finder would. This is *not* the clean
account the acceptance asks for: `/etc/paths.d/homebrew` puts
`/opt/homebrew/bin` on every account's login `PATH` here, so `codex`,
`opencode`, `gh` and `glab` were visible (none logged in); `claude` lives
in the author's home and was not. The person's `~/.config/guild` was not
touched (its mtime unchanged).

Run A — "a new account on this Mac":

1. **Download.** `gh release download` timed out once on the TLS handshake,
   then worked; the zip's SHA-256 matched. The bundle is ad-hoc signed and
   `spctl --assess` says `rejected`, as expected without GLD-073.
   README's first-launch advice (Control-click ▸ Open) is the macOS 13–14
   route; macOS 15 removed it — there the person opens the app once and
   allows it in System Settings ▸ Privacy & Security ▸ *Open Anyway*. This
   Mac runs macOS 26.5. *Not walked:* the app was not quarantined (gh does
   not set the flag), so the dialog itself was not seen. → README and the
   site's landing page fixed in this branch.
2. **First window.** It opens with *What's new in Guild* — the nightly's
   changelog about merge requests and glab — before the person has seen
   Guild at all. → GLD-079.
3. **Before any agent is started**, Guild's startup ran the agent CLIs it
   found: `codex` created `~/.codex` (sqlite databases, `config.toml` with
   a `# >>> guild hooks >>>` block written by Guild), and `opencode`
   created `~/.config/opencode`, `~/.local/{share,state}/opencode`,
   `~/.cache/opencode` and downloaded about 109 MB into `~/.npm`. A
   newcomer is not told, and it happens over the network. → GLD-078; input
   for GLD-077.
4. **No project open.** The start screen offers *New project* (a Save
   panel for a new folder), *Recent projects* and *Settings*; there is no
   *Open…* there, although README's step is *File ▸ Open…* (which exists
   in the menu bar and worked). Clicking the *New project* row twice did
   nothing; its shortcut ⌘⇧N opened the panel. → GLD-080 (the row's click
   to be reproduced first).
5. **Repository open** (a local repository without a remote, through
   *File ▸ Open…*): the worktree's start screen. Its default button says
   *Claude Code — Default* while the Claude Code tile below says *not on
   PATH*; Codex and OpenCode, which are installed, are not offered as the
   default. Pressing the default raises a Warning, "claude is not on the
   PATH", with no word on what to install or where. → GLD-081.
6. README's step 3 says to press **+** beside the tabs; the control there
   is the harness mark with a chevron, not a plus — README to name it in
   GLD-080. *Not checked* what it
   offers: the screen went to the login window (the display locked or
   changed) and the walk stopped here.

Not checked: starting an agent and answering it, the column status, a diff
and a commit, the Update button without `gh` logged in, quitting and
reopening; run B (no Homebrew and no agent CLI, prepared with a login
shell that skips `/etc/zprofile`); an agent CLI older than README's
minimum; Russian.

