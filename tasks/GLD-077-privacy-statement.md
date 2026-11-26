---
id: GLD-077
title: Say plainly what Guild reads on the Mac and what it sends over the network
kind: chore
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - README.md
  - site/**
  - docs/guide.md
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-27"
---

## Goal

A person deciding whether to let Guild run agents in their code can read,
in one short section, what Guild reads and what leaves the Mac. README says
"Nothing needs a server or an account" but lists neither. The statement is
written from the code, not from memory, and says that Guild has no
telemetry, if the inventory confirms it.

## Acceptance

- [ ] WHEN the inventory is made THEN every place the desktop reaches the
      network or reads another program's data is listed, with its path in
      the code, what it sends or reads, and whether a setting turns it off.
      Starting list, to be checked and completed, not trusted:
      - updates through GitHub's releases, anonymous HTTPS or `gh`
        (`update/remote.rs`, `update/remote/gh.rs`);
      - plan limits: the Keychain item's token sent to
        `api.anthropic.com/api/oauth/usage` (`usage.rs`, `root/usage.rs`),
        and `codex app-server` (`usage/codex.rs`);
      - merge requests through `glab api` (`root/forge.rs`, decision 56);
      - remote images in Markdown (`markdown/remote.rs`);
      - git's own fetch, push and clone, and the build systems asked for
        dependencies (`deps/ask.rs`);
      - the agents' transcripts and histories read from their folders
        (`guild_session::history`, `transcript`).
- [ ] WHEN the inventory is done THEN README (and the site's matching page)
      has a short *Privacy* section in plain words: what is read, what is
      sent and to whom, what is never sent, and which settings turn each off.
- [ ] WHEN something is found that the person would not expect to be sent
      THEN it is raised with the person before the text is written, not
      described away.

## Checks

- Each line of the section names the code it rests on in the task's
  record, so a later change can be checked against it.
- A grep for HTTP clients, `network::` and launched network programs, kept
  in the record.

## Limits / non-goals

- Describes the behaviour; it changes none of it. A change found worth
  making becomes its own task.
- What the agents themselves send to their providers is theirs to state,
  not Guild's; the section says so.

## Context

- `crates/desktop/src/network.rs` — the proxy and where it is applied.
- README, *What is Guild* and *Configuration*.
