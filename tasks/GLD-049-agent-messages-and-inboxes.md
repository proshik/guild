---
id: GLD-049
title: "Design agent messages: sender identity, answers, results delivered once"
kind: design
status: backlog
priority: 3
component: docs
depends_on: []
touches:
  - tasks/GLD-049-agent-messages-and-inboxes.md
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

When agents work for a parent, their results and questions should arrive
attributed to the session that sent them and be consumed once, and a parent
may answer a delegated technical question — never a decision or approval that
belongs to the person. Today task sessions may not steer other sessions; a
durable report inbox would route to the initiator first, without lifting that
rule.

**Design drafted; implementation not approved.** On 2026-11-21 the person
approved a narrow direction (queued sends, waiting/stall events for the
initiator, child-to-initiator reports). On 2026-11-22 they added mail between
any sessions (answer 8). The draft OpenSpec change `agent-messages` is no
longer in the repository; the person's answers of 2026-11-22 are recorded
below, and the design is written again from them, in this file's Design and
Plan sections, before implementation.
Feature tasks follow only once the person approves implementation.

Originally this was a candidate carried over from the
former `docs/backlog/2026-10-17-product-improvements.md` (rows
2026-10-16 (`agent answer`, sender identity), 2026-10-11 (attributed messages,
no duplicate result delivery) and 2026-09-23 (durable inboxes with
acknowledgements), and the note on peer messaging); its scope and behaviour
are to be agreed with the person before any work, and recorded in this file
in the same branch.

## Acceptance

- [x] WHEN the task is taken THEN its open choices are put to the person, one
      question each, and the answers recorded here before code; nothing below
      is binding until then.
- [x] WHEN the design is written THEN it separates delegated technical
      questions from the person's decisions and approvals, and says who may
      answer each.
- [x] WHEN a result is delivered THEN it carries sender provenance and is
      consumed once; surviving messages do not imply surviving processes.
- [x] General peer messaging is put to the person as its own decision.
- [ ] The decision is recorded; feature tasks follow only for what is
      approved. (Recorded; feature tasks wait for the person's approval of
      implementation.)

## Checks

- The decision recorded in this file: its Design and Plan sections written
  and agreed with the person.
- No production code.

## Limits / non-goals

- Design only.

## Context

- `AGENTS.md` — provenance, "`guild` is a door".
- GLD-014 (subscriptions).

## Decision record (2026-11-22)

The person's answers, one question each.

1. A send to a busy task is **queued by the window** and delivered after its
   turn ends, not refused, and not held by a waiting CLI.
2. A queued message is **superseded, never typed**, if the person sent input
   to that program, or a prompt not delivered by Guild started there, since it
   was queued. Answering the child's own question does not count. No TTL.
3. **Inboxes persist** until acknowledged. **Queued text does not survive a
   restart**: the sender gets `expired: window restarted`.
4. A parent is woken by **`guild inbox wait`** run in the background. No
   `asyncRewake`, and nothing typed into the parent's session. (Replaced by
   answer 9 for Claude Code and Codex.)
5. **Who may send is unchanged**: any non-task session, to a session the
   command line started.
6. A child may report a **result, a problem or a delegated technical
   question** to its initiator. Decisions and approvals stay the person's.
7. **2 minutes** of `unknown` makes a task session possibly stalled. The
   threshold is fixed, not a setting.
8. **General peer messaging.** The first answer was "not in
   this change". After comparing four options (initiator only; the initiator
   writing to all its children; a family notice board; general mail between
   any sessions) the person chose the last the same day: any session, a task
   included, may mail any session or all of them (`guild mail send --to |
   --all [--workspace]`) into their inboxes. Mail is never typed, carries no
   authority, and each inbox is still readable only by its owner. A task still
   may not type into, start or prompt another session.
9. **Inbox items are delivered between turns, one to one.**
   The same day the person first chose a turn-start read, then replaced it
   with delivery between turns: Claude Code gets an `asyncRewake` waiter (in Guild's
   per-launch settings only) that starts a new turn carrying the items, and
   Codex gets them from the window's `codex queue`. An idle agent is woken, a
   running turn is not interrupted, and a broadcast starts a turn in every
   idle Claude Code and Codex agent. This replaces answer 4's background
   `inbox wait` for those two harnesses. OpenCode has no receiver, and reads
   its inbox at the start of each turn.

## Spec review (2026-11-22)

One round, as `reviews.spec` allows: revust, `codex-only` profile,
architecture lens only, one call. It ran on `2d8170bf` and completed with 8
major findings. All 8 held against the code and were folded into the draft.
Two of them add mechanisms without anything a person sees: a peer-process
check, so that only the owner opens its inbox, and an initiator file of its
own, so that a rollback cannot erase it.

Mail between sessions (answer 8, changed after the review) was added after
that round, and no independent review has read it. The budget allows no
second spec review; the code review of the implementation is the first that
will.

