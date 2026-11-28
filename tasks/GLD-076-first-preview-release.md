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

- [x] WHEN the task is taken THEN the scheme is put to the person and the
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

## Decisions

- **The person's decision (2026-11-28): `v0.1.0` as a Stable release**,
  titled as the preview. The update path already reads it: `bundle.yml`
  builds `v*` tags, `guild-release` names a `vMAJOR.MINOR.PATCH` tag
  Stable and signs it like a nightly, and `Channel::Stable` accepts a
  non-prerelease `v` tag. No code change. GitHub shows it as *Latest*. A
  person who installs it keeps the default Nightly channel, so Update offers
  the later nightlies. The other scheme — a nightly marked *Latest* — breaks
  the rule that a nightly is a prerelease (`docs/updates/owner-guide.md`),
  and the Nightly channel would no longer take it.
- **This step goes as far as a verified draft** (the person's word,
  2026-11-28): the changelog cut and `v0.1.0` tag on main, CI's bundle,
  signing with `owner-2026`, a draft non-prerelease release with its assets
  re-downloaded and verified. Publishing is a separate word.
- **README and the site point at `releases/tag/v0.1.0` only when the release
  is published**: the site deploys on a push to main that touches `site/`,
  and a draft's page is a 404 to everyone else.
- `Release v0.1.0` cuts `## [Unreleased]` into `## [v0.1.0]`; the changelog
  reader is positional (`changelog::between`), so What's new for later
  nightlies is unaffected.

## Release notes (draft)

**Guild 0.1.0 is a preview** — the build to try if you have not used Guild before. Guild also ships as nightlies; this preview is one of them, checked and marked for newcomers.

## What Guild is

A native macOS window for running many AI coding agents at once. It makes git worktrees, starts **Claude Code**, **Codex** or **OpenCode** in each, shows which agent needs you first, and lets you read the diff, commit and push in the same window — beside a file tree, an editor and a terminal of its own. Everything stays on your Mac: no server, no account.

## Who it is for

Developers on a Mac with Apple silicon who already use an agent CLI and want several of them working in parallel worktrees without losing track of them.

## What works

- Projects, worktrees and workspaces in one column; a new worktree with an agent in it in one step.
- Claude Code, Codex and OpenCode in tabs, each with its own CLI, prompts and slash commands, and their status (working, waiting for you, done) in the column.
- The Explorer, an editor with language servers, diffs, Stage Hunk, Commit and Push.
- `guild`, the command line your agents can use to open worktrees and sessions.
- Updates: **Update** appears in the title bar when a newer nightly is out.

## What does not work yet, or is not checked

- **Not notarized.** macOS refuses Guild the first time: on macOS 15 and later, open it once, then choose **Open Anyway** in System Settings ▸ Privacy & Security; on macOS 13 and 14, Control-click it in Applications and choose **Open**.
- **Apple silicon only**, macOS 13 or later. No Intel or universal build.
- **A first run on a clean Mac has been checked only in part**: download, the first window, opening a repository and the start screen were walked; starting an agent, committing and updating from a fresh account were not.
- **You need an agent CLI** — Claude Code 2.1.295, Codex 0.163.0 or OpenCode 2.2 or later. Guild does not install one.
- Nightlies change daily; this preview offers you the next nightly as an update.

## Reporting a problem

- A bug or an idea: [open an issue](https://github.com/proshik/guild/issues/new/choose). **Guild ▸ About Guild** shows the version; the log is `~/Library/Logs/Guild/guild.log` (**Help ▸ Show Log**) — remove anything private from it.
- A security problem: report it privately, as [SECURITY.md](https://github.com/proshik/guild/blob/main/SECURITY.md) says.

## What changed since the last nightly

See the `[v0.1.0]` section of the [changelog](https://github.com/proshik/guild/blob/main/CHANGELOG.md).

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
