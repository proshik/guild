---
id: GLD-001
title: Claude's turn witness from OSC 7501 only, not from its transcript
kind: fix
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/desktop/src/root/sessions.rs
  - crates/desktop/src/root/status.rs
  - crates/session/src/desk.rs
harness: any
reviews: {spec: 0, code: 1}
created: "2026-11-07"
---

## Goal

`guild agent list` reports `turned: true` for a Claude session only once the
program has reported a finished turn through OSC 7501. Today a pre-existing
transcript witness sets the same flag as soon as Claude's conversation file
holds the person's message — before the turn has ended — so
`settled("idle", turned)` and `agent send`'s refusal read a turn that has not
happened. This was seen live in task 11.3 of the OSC 7501 stage.

The flag carries two meanings, and only one of them is about turns: for
Claude, `had_turn` is also "there is a conversation on disk to reopen"
(`SessionRecord::has_conversation`). The fix separates them rather than
deleting the transcript probe.

## Acceptance

- [ ] WHEN a Claude session's transcript holds the person's first message and
      the program has not reported `working` → `done`/`idle`/`error` THEN
      `guild agent list --json` shows `turned: false` and `agent send`
      refuses with "it has not finished its first turn yet".
- [ ] WHEN the program then reports the turn's end through OSC 7501 THEN
      `turned` becomes `true`, exactly as for Codex and OpenCode.
- [ ] WHEN a Claude session whose transcript holds a conversation is closed,
      or the window restarts THEN the Sessions panel still offers Reopen and a
      restart still resumes it — the conversation-on-disk witness is kept.
- [ ] WHEN a state file written by an earlier build holds `had_turn: true`
      for a Claude session THEN it loads, and the session neither loses its
      Reopen nor is treated as unturned by a gate that the earlier build let
      through (decide and test which; record the choice in the commit body).
- [ ] No other reader of `had_turn` changes behaviour without a test saying
      so: `root/command.rs:361`, `root/agent_submit.rs:56`,
      `root/workflow/handoff.rs:33`, `harness_retry`, workflow recovery.

## Checks

- A failing test first for the first criterion (a transcript fixture with the
  message and no OSC 7501 report), then the fix.
- The existing `root/status.rs` turn tests (`had_turn` assertions around
  `:739`, `:901`, `:973`) and the restore/reopen tests stay green.
- `just check`. No harness launched, no provider called.

## Limits / non-goals

- Do not remove the transcript reader: it also supplies the conversation's
  name and its spending (`root/sessions.rs:96-123`).
- Do not change the wire words or `settled`'s rule
  (`session/command.rs:458-463`).

## Context

- The witness: `root/sessions.rs:95` (`!record.had_turn &&
  transcript::has_conversation(...)` → `desk.mark_turn`), and `:152` on
  retire.
- The OSC 7501 witness: `root/status.rs:541-545` (`outcome.ended` →
  `mark_turn`).
- The flag: `session/desk.rs:210-212` (persisted, `#[serde(default)]`),
  `:295-299` `has_conversation`, `:923-925` `mark_turn`.
- Observed in the OSC 7501 stage's live check
  ("An early `turned: true` in `agent list` comes from the pre-existing
  transcript witness").
