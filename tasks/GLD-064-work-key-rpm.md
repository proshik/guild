---
id: GLD-064
title: Requests per minute of a work API key, seen in the usage card
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/usage.rs
  - crates/desktop/src/usage/**
  - crates/desktop/src/root/usage.rs
  - crates/desktop/src/view/usage.rs
  - crates/desktop/src/view/settings/**
  - crates/desktop/src/root/settings.rs
  - crates/session/src/settings.rs
  - crates/desktop/src/i18n/**
  - crates/desktop/Cargo.toml
  - Cargo.lock
  - crates/desktop/tests/all/main.rs
  - crates/desktop/tests/all/work_key_rpm.rs
  - crates/desktop/tests/fixtures/usage/**
  - docs/decisions.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-26"
needs_person: true
---

## Goal

At work, the models that are not on a subscription (for example qwen-36-27b
and deepseek-flash on `*.acme.example`) have a limit of **30 requests per
minute per key**. OpenCode talks to them most, Claude Code and Codex sometimes. The person
needs to see in Guild's limits card how many requests went out on that key in
the last minute, so as not to run into the limit. Today the card says "no
limits to read" under OpenCode.

When this task is finished:

- The limits card has a row for every work key found: how many requests went
  out in the last 60 seconds and, when a limit is set, how many of it are left.
- The count covers the whole machine and all three programs, including sessions
  not started from Guild: the limit is shared by the key.
- The limit is set in Settings.
- When the tab in front is an agent on a model of a work key, the chip in the
  title bar shows the requests in the last minute instead of percentages.

**Decided by the person (2026-11-26):**

- Approach 1 of *How it works*: read the programs' own logs.
- The limit is **per key**: one row per key, not per model.
- Claude Code and Codex belong to a key **by the model name** written in their
  logs.
- The limit is set by **a field in Settings**.
- The **chip** in the title bar shows the requests in the last minute when the
  agent in front is on a model of a work key; in every other case it is as
  today.
- OpenCode's database is read through **`rusqlite` with `bundled`** (a new
  dependency of `guild-desktop`; opened with `SQLITE_OPEN_READONLY`).
- JSONC is parsed by **our own mini-parser**: strip `//` and `/* */` outside
  strings and trailing commas, then `serde_json`. No new dependencies.

Everything marked **(proposal, the author's call; open to the person's veto)**
below was decided by the author of this file. It is open to the person's veto
at the Penpot step or before.

### Approaches considered

1. **Read the programs' own logs: chosen.** Every few seconds Guild counts the
   records of requests to the key's models in the last 60 seconds: OpenCode's
   database, Claude Code's transcripts, Codex's rollout files. Passive: it sees
   the whole machine, changes nothing in how agents are launched, and a Guild
   crash touches no request. The cost is a lower estimate (see *Limits*).
2. A local proxy in Guild. An exact count, but only Guild's own sessions are
   seen, the addresses of all three programs would have to be rewritten at every
   launch, and a point of failure appears for work requests. Rejected: too
   heavy for a counter.
3. Ask the gateway (`x-ratelimit-*` or a usage-by-key endpoint). The most
   exact, but it is not known what stands behind `acme.example`. Deferred, not
   rejected: if the gateway can do it, it can be added later as one more source
   of the same row.

Hooks do not fit: Claude Code calls them on tools, not on every request to the
model.

## What the person sees

### The card

Under OpenCode, instead of "no limits to read", for every group:

```
OpenCode
  Work · acme.example         12 / 30 per minute
  [██████████░░░░░░░░░░]      <- the bar shows how much is left, as Week's does
```

- "N / L per minute", the bar and the colour follow `usage::level` of the
  share left (the same thresholds as the percentages).
- The row belongs to the key, not to OpenCode: a group with a limit is shown
  even when the requests came only from Claude Code or Codex. It sits in the
  OpenCode section because the key is found in OpenCode's config. **The final
  place and look are decided in Penpot.**
- Before the first count the row shows "–", not 0.

### The chip

If the agent in the tab in front is now on a model of a group with a limit,
the chip shows "12/30" with a level disc. The tab's model is taken by the same
counting thread, not by the window's thread:

- OpenCode: by the tab's `oc_session` (the plugin already posts it; see
  `assets/hooks/opencode-status.js`), the model of that session's last
  assistant message in the database;
- Claude Code / Codex: the model of the last record of the transcript or
  rollout file of the session Guild already ties to the tab (`binding.rs`,
  hooks). The builder reuses the existing link of a tab to its session and
  does not make a new one.

When the model is not known (a fresh tab, no first answer yet) the chip behaves
as it does now. This is intentional.

### Settings

In the section where the limits settings live now (or a new "Usage"
subsection), a list of all groups found: a label and a field "Requests per
minute". It is saved by the group's identity. The field is a `TextField`,
integers above zero only. The place of the section is decided in Penpot.

**The card and the chip show only groups with a limit set.** A group without a
limit is visible only in Settings, so personal keys without limits do not clog
the card, and hiding a group means clearing its field. The person types 30
once. (proposal, the author's call; open to the person's veto)

## How it works

### Which keys are work keys

The source is OpenCode's config (`~/.config/opencode/opencode.jsonc`, JSONC;
also `opencode.json` if it exists). A work key is a provider with its own
`baseURL` and `apiKey` in `options`. Providers with the same `apiKey` are
merged into one group. The key's value is compared only in memory while the
config is parsed and is dropped at once: neither the key nor its hash is
saved, logged, or put into `Debug` or error text anywhere. If `apiKey` is a
reference `{env:NAME}` or `{file:PATH}`, the group is decided by the reference
string, without reading the variable or the file.

**The group's identity** (the limit is saved by it) is the sorted `providerID`s
of its providers. It holds nothing of the key; when the key changes, the limit
stays. (proposal, the author's call; open to the person's veto)

A group knows:

- the set of `(providerID, modelID)` pairs for OpenCode: all the models of
  these providers from the config's `models`;
- the set of model names for Claude Code and Codex: the same `modelID`s;
- a label: the common domain of its providers' `baseURL` (`acme.example`); if
  the domains differ, the provider names separated by commas.

The config is re-read when its mtime changes. No config, no groups, or no
group with a limit: no row, and OpenCode keeps "no limits to read".

### What counts as a request

The window is the sliding 60 seconds before "now" by Guild's system clock. The
time of a record is the one the program itself wrote (OpenCode: epoch ms;
Claude and Codex: ISO 8601). No margin for clock disagreement is added: the
gateway is not on the same machine, and a one-second shift at 30 rpm changes
nothing. (proposal, the author's call; open to the person's veto) The number is
a lower estimate; in the UI it is written without "≥", and the caveat is
stated in the decision in `docs/decisions.md`. (proposal, the author's call;
open to the person's veto)

One record is one HTTP request to a model. For Codex the builder picks the
event that is written exactly once per model answer, and confirms it on a real
rollout file where the number of requests is known.

- **OpenCode:** one record of a model step starting (`step-start` in the
  current `part`/`message` tables) on an assistant message whose
  `providerID`/`modelID` is in the group. The database is opened read-only
  (`SQLITE_OPEN_READONLY`, no write to the WAL), and the query is bounded by
  `time_created`. **The builder checks which tables the installed OpenCode 2.x
  writes to** (`session_v2`, `session_message` and `event` are next to them)
  and pins this with a fixture.
- **Claude Code:** one unique assistant `message.id` in a transcript
  (`~/.claude/projects/*/*.jsonl`) whose `message.model` is in the group. Only
  files with an mtime within the last 60 seconds are read, and only the tail of
  a file, from where the previous read stopped (the offset is remembered per
  file). Repeated lines of one `message.id` (streaming) count once.
- **Codex:** one model answer in a rollout file (`~/.codex/sessions/**.jsonl`)
  while the active model (from `turn_context`) is in the group. The builder
  picks the concrete event that matches one request and pins it with a
  fixture. It is read the same way: files with a fresh mtime, only the tail.

Unreadable records are skipped silently, as in `usage.rs`.

### Polling

- Counting runs on a separate Guild thread, every **5 seconds**, while at
  least one group has a limit and either the card is open or the tab in front
  holds an agent (OpenCode, Claude Code or Codex) — the chip can only learn
  that the agent in front is on a group's model from this thread, so the
  thread cannot wait for the chip to show requests first. Otherwise the
  counter sleeps. (proposal, the author's call; open to the person's veto)
- The result is `{group -> (count, when counted)}`; only this reaches the
  window's thread. Before the first count the row shows "–", not 0.
- Tail offsets live only in memory. After a start, the first read of each file
  goes from the end backwards until it passes 60 seconds (bounded by volume).
  (proposal, the author's call; open to the person's veto)
- Claude Code subagents' transcripts count if they lie under
  `~/.claude/projects/**`; the builder checks where they lie in the current
  version.
- If a source cannot be read (no database, a file is busy), the row shows the
  last number marked with its age, as stale figures are shown now.

## Fit with the rules in force

- `crates/desktop`: reading is pure functions in `usage/` (in `usage.rs`'s
  manner), polling is in `root/usage.rs`, drawing is in `view/usage.rs`. The
  disk is never read on the window's thread.
- Colours and sizes come from `theme/`; levels come from the existing
  `usage::level`.
- A change visible in the window is drawn in Penpot first and agreed before
  any code (`docs/penpot.md`). A line in `CHANGELOG.md` under `[Unreleased]` is
  needed. A new data source (other programs' files) is recorded as a decision
  in `docs/decisions.md`, beside decisions 46 and 47.
- Tests do not touch the real `~/.config/guild`, `~/.local/share/opencode`,
  `~/.claude` or `~/.codex`: only fixtures and `GUILD_STATE_DIR`.

## Acceptance

- [ ] WHEN a fixture OpenCode config has providers with their own `baseURL`
      and `apiKey`, THEN providers with equal `apiKey` form one group, the
      group's identity is its sorted `providerID`s, and a provider without its
      own `baseURL` and `apiKey` is in no group. A `{env:NAME}` or `{file:PATH}`
      key is grouped by its reference string, and neither the variable nor the
      file is read.
- [ ] WHEN a config is grouped, THEN no key value and no hash of one is stored,
      logged, shown in `Debug` or put into an error message; a test checks the
      `Debug` and error text of every type that touches the config, and the
      saved state and the log of a run.
- [ ] WHEN a key changes but the providers stay, THEN the saved limit still
      applies to the group.
- [ ] WHEN the config holds `//` and `/* */` comments, also inside and next to
      strings, and trailing commas, THEN the mini-parser strips them outside
      strings only and `serde_json` reads the result; `opencode.json` is read
      when it exists; a changed mtime makes the config read again.
- [ ] WHEN there is no config, no group, or no group with a limit, THEN there
      is no row and OpenCode shows "no limits to read".
- [ ] WHEN a fixture OpenCode database holds `step-start` records of
      assistant messages, THEN each in-group record in the window counts one
      request; records of other models, of messages that are not the
      assistant's, and older than 60 seconds do not count. The fixture pins the
      tables the installed OpenCode 2.x writes to.
- [ ] WHEN a fixture Claude transcript holds assistant lines, THEN one unique
      `message.id` whose `message.model` is in the group counts one request;
      repeated lines of one `message.id` count once.
- [ ] WHEN a fixture Codex rollout file holds model answers, THEN each answer
      while the `turn_context` model is in the group counts one request, by
      the one event the fixture pins; the event is confirmed on a real rollout
      file where the number of requests is known.
- [ ] WHEN the clock is fixed, THEN only records from the sliding 60 seconds
      before "now" are counted, by the time the program itself wrote (epoch ms
      for OpenCode, ISO 8601 for Claude and Codex), with no margin added.
- [ ] WHEN Claude or Codex files are polled, THEN only files with an mtime
      within the last 60 seconds are read, only from the remembered offset, and
      the first read after a start goes from the end backwards until it passes
      60 seconds, bounded by volume; offsets are kept in memory only.
- [ ] WHEN unreadable records are met, THEN they are skipped silently and the
      rest are counted.
- [ ] WHEN OpenCode's database is opened, THEN it is opened with
      `SQLITE_OPEN_READONLY`, the query is bounded by `time_created`, and the
      database and its WAL are the same, byte for byte, after a count.
- [ ] WHEN no group has a limit, or the card is closed and the tab in front
      holds no agent, THEN the counter does not poll; otherwise it polls every 5
      seconds on its own thread, and neither the window's thread nor paint
      reads a disk.
- [ ] WHEN no count has finished yet, THEN the row shows "–", not 0.
- [ ] WHEN a source cannot be read, THEN the row shows the last number with
      its age.
- [ ] WHEN a group has no limit set, THEN it is absent from the card and the
      chip and present in Settings; WHEN its field is cleared, THEN it
      disappears from the card and the chip again.
- [ ] WHEN a group has a limit L and N requests in the window, THEN the card
      row reads "N / L per minute" under OpenCode, with a bar of the share left
      and a colour by `usage::level` with the same thresholds as the
      percentages; the row is shown also when the requests came only from
      Claude Code or Codex.
- [ ] WHEN the agent in front is on a model of a group with a limit, THEN the
      chip shows "N/L" with a level disc; the model comes from `oc_session` for
      OpenCode and from the already linked session's last record for Claude
      Code and Codex (no new link of a tab to a session); WHEN the model is not
      known, THEN the chip is as today.
- [ ] WHEN a value is typed into the Settings field, THEN integers above zero
      are accepted and saved by the group's identity; zero, a negative number,
      a fraction and non-digits are refused; the field is a
      `desktop::text_field::TextField`.
- [ ] WHEN the interface is Russian, THEN the new strings are in
      `i18n/russian`.
- [ ] WHEN `guild-desktop` is built, THEN `rusqlite` with `bundled` is its new
      dependency (no other new dependency), `just boundaries` still passes, and
      `Cargo.lock` carries the change.
- [ ] WHEN the work starts, THEN a Penpot proposal of the card row, the chip and
      the Settings field, drawn from the Library's components, is approved by
      the person before any code is written; WHEN the work ends, THEN Penpot
      holds what was merged (`docs/penpot.md`).
- [ ] WHEN the change is merged, THEN `docs/decisions.md` holds a decision for
      the new data source and for the lower estimate, and `CHANGELOG.md` has a
      line under `## [Unreleased]`, in this branch.

## Checks

- `just check` (fmt, lint, boundaries, test) with `GUILD_STATE_DIR` pointing
  at a temporary directory: tests in this repository can otherwise touch
  `~/.config/guild`.
- Fixture tests in `crates/desktop/tests/all/work_key_rpm.rs`, declared in
  `tests/all/main.rs`, over `crates/desktop/tests/fixtures/usage/**`: the
  config, OpenCode's database, a Claude transcript and a Codex rollout file,
  from the current versions of the programs.
- A live check in an isolated dev window (`just desktop-dev <dir>`,
  `docs/desktop-dev.md`) with fixture directories in place of the real
  `~/.config/opencode`, `~/.local/share/opencode`, `~/.claude` and `~/.codex`:
  the card row, the bar and its colour, the chip, the Settings field and the
  "–" before the first count. A person at the machine is needed.

Not run: anything against the real work gateway or the real keys.

## Limits / non-goals

- **No exact count up to the request.** Retries after a 429, title
  generation, context compaction and other service calls may not reach the
  programs' logs, so the number is a lower estimate.
- Guild does not limit or delay requests. It only shows.
- No proxy, no rewriting of addresses, no change to how agents are launched.
- No tokens, cost, or limits in days or hours.
- The key goes nowhere but the process's memory: not a log, a state file, the
  UI or a thread message. This is a failure even if everything else works.
- Reading the logs must not slow the window or load the disk noticeably: huge
  Claude transcripts are not re-read whole every few seconds.
- Guild does not write to OpenCode's database or disturb it (locks, WAL).
- The gateway as a source (approach 3) is deferred, not part of this task.

## Left to the builder

- The exact tables and events of OpenCode and Codex, pinned on fixtures from
  the current versions.
- How to keep the tail offsets and when to forget them.
- The Russian text of the strings in `i18n/russian`.

## Order of work

1. Penpot: a proposal of the card row, the chip and the Settings field from the
   Library's components; the person's approval.
2. A decision in `docs/decisions.md`, reading tests on fixtures, then the
   reading, the polling, Settings, the card and the chip.
3. A line in `CHANGELOG.md`; a check in an isolated dev window with fixture
   directories; the design carried from the proposal into the Library and
   Screens.

## Context

- `docs/penpot.md`: where the window is designed, and the process for a change
  a person would see.
- `docs/decisions.md`, decisions 46 and 47: the plans' limits (the title bar's
  chip, and Codex's limits from `codex app-server`).
- `crates/desktop/src/usage.rs`: the plans' limits model, `usage::level`, and
  the style the new readers follow.
- `crates/desktop/assets/hooks/opencode-status.js`: the OpenCode plugin; it
  already posts `oc_session`.
- `crates/desktop/src/binding.rs`: which session a tab holds.
