---
id: GLD-069
title: "Open a failed pipeline job's log in the window"
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: [GLD-063]
touches:
  - crates/session/src/merge_requests/**
  - crates/session/tests/all/merge_requests_glab.rs
  - crates/session/tests/all/merge_requests.rs
  - crates/desktop/src/forge.rs
  - crates/desktop/src/root/forge.rs
  - crates/desktop/src/view/forge.rs
  - crates/desktop/src/i18n/**
  - crates/desktop/tests/all/forge.rs
  - crates/desktop/tests/fixtures/forge/**
  - docs/decisions.md
  - CHANGELOG.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-27"
needs_person: true
---

## Goal

GLD-063 shows when a worktree's MR has a failed pipeline. The icon turns
rust, the badge shows ✗ and the path-to-merge card has a pipeline line. Then
the person still opens the browser to find out *why* it failed. When this
task is finished, the person can open the log of the failed job from the
window, without a browser. This is the part GLD-021 left undone when it was
cancelled.

Reads only, as decision 56 requires. Guild retries nothing, cancels nothing
and writes nothing to GitLab.

**Not yet agreed with the person.** The scope and behaviour below are a
proposal, to be agreed before any work and recorded here in the same branch.
**A visible UI change:** it is designed in Penpot first, from the Library's
components and tokens, and the person approves the design before code
(`docs/penpot.md`).

## Acceptance

- [ ] WHEN the task is taken THEN a Penpot proposal and the open questions
      (below) are put to the person, and the approved shape is recorded here
      before code. Nothing below is binding until then.
- [ ] WHEN a worktree's MR has a failed pipeline THEN the person can reach
      its failed jobs from the place GLD-063 already shows the failure (the
      card's pipeline line, proposal), and each job is named by its stage and
      name.
- [ ] WHEN the person opens a failed job THEN its log is shown in the window.
      The log is bounded, and when it is cut the person sees the tail, where
      the error is, with a word saying that it was cut.
- [ ] WHEN the log is shown THEN GitLab's ANSI colours and section markers do
      not appear as raw escape bytes.
- [ ] WHEN glab is missing, signed out for the host, the network fails or
      the read runs out of time THEN the window says which of these happened,
      and nothing is written anywhere.
- [ ] Every new read goes through the same `glab api --hostname <host>` path
      as decision 56's reads: the same admission, caps, scrubbed environment,
      deadline and proxy. No token of Guild's own and no second HTTP client.
- [ ] Decision 56 is extended in `docs/decisions.md` with the new reads.
- [ ] A line under `## [Unreleased]` in `CHANGELOG.md`.

## Open questions for the person

- Where the log opens: a read-only tab of its own, a card over the column,
  or the Run/Terminal dock. The terminal already renders ANSI.
- Which jobs are offered: every failed job of the head pipeline, or only the
  first one. And whether a job that is allowed to fail counts.
- How much of the log is kept (the cap), and whether a "load more" is
  needed.
- Whether an open log refreshes while a retried job runs, or is a snapshot.

## Checks

- A failing test first for each acceptance item. Glab is replaced by the
  fake `glab` the GLD-063 tests already use (`tests/fixtures/forge/`).
- `just check`.
- A check in an isolated dev window (`just desktop-dev`) for what is drawn.
  `needs_person`: this needs the person's glab login against a real failed
  pipeline, or their permission to use it.

## Limits / non-goals

- Reads only: no retry, no cancel, no play of a manual job.
- GitLab only, as in GLD-063. GitHub Actions is a later task.
- Only the head pipeline of a worktree's MR. Pipelines of branches without an
  MR, and the pipelines list, are not part of this task.
- Credentials are never logged or shown. A log is shown as GitLab returns
  it; Guild does not try to mask secrets inside it (not verified: what
  GitLab's own masking covers).

## Context

- GLD-063: the MR reader and the card. `crates/session/src/merge_requests/glab.rs`.
  `PipelineWire` (`:1063`) keeps only `status`, so the pipeline's id is not
  read yet. The caps are at `:38-47`.
- The pipeline's state: `merge_requests/status.rs:22` (`Pipeline`) and
  `:287` (the card's `Check::Pipeline` line). Its words are in
  `crates/desktop/src/i18n/forge.rs:70`.
- Decision 56, `docs/decisions.md:571`.
- GitLab's endpoints, not verified against the person's GitLab version:
  `projects/:id/pipelines/:pipeline_id/jobs?scope[]=failed` and
  `projects/:id/jobs/:job_id/trace`.
- GLD-021, cancelled. It carried this item.
