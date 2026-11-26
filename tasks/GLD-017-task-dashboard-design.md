---
id: GLD-017
title: Design the task dashboard and launcher that read this format
kind: design
status: backlog
priority: 2
component: docs
depends_on: []
touches:
  - openspec/changes/task-dashboard/**
  - tasks/README.md
  - tasks/task.schema.json
  - tasks/state.schema.json
harness: any
reviews: {spec: 1, code: 0}
created: "2026-11-07"
---

## Goal

`tasks/` defines tasks, where their status comes from and how one is taken
and merged, but nothing reads it yet. Design the tool that does: a board of
the project's tasks with their live lifecycle and harness status; launching a
task through a harness in its own worktree; merging with the `Task:` trailer;
and per-component release rules that turn `merged` into `done` — for
projects with several deliverables (backend, frontend, desktop, mobile, CLI).
The output is the decision recorded in an OpenSpec change `task-dashboard`;
no code.

**Open decision of this task:** whether the dashboard is part of Guild (a
page in the window, the launcher as `guild` commands) or a separate CLI/TUI
that runs in any terminal. Put both to the person with their costs and a
recommendation.

## Acceptance

- [ ] WHEN the change is written THEN it decides, with the person's answer
      recorded: Guild page versus separate tool, and if separate, its relation
      to `guild` and to the session server (GLD-010–014).
- [ ] The board: columns by lifecycle, the harness status per running task,
      blocked tasks with their reason, takeable tasks (dependencies merged,
      `after_release` met, no `touches` overlap with running work),
      `needs_person` marked.
- [ ] Launch: worktree and branch naming, writing the state file (atomic
      rename, one writer), handing the task file to the harness, and carrying
      the harness's OSC 7501 root record into the state file — never the
      agent reporting itself.
- [ ] Merge: who merges, how the `Task:` trailer is added, what happens to
      the state file and the worktree after.
- [ ] Validation: rejecting front matter that fails `task.schema.json`, an
      `id` that differs from its file name, an unknown component, a
      `depends_on` cycle or unknown id, and a second `Task:` commit for one id.
- [ ] WHEN a task is in `review` THEN one view shows its acceptance
      readiness — diff, checks, review result, integration and cleanup
      readiness of its worktree — reusing the existing tools and guards, and
      the same summary is available machine-readably from the CLI.
      (From the former `docs/backlog/2026-10-17-product-improvements.md`:
      "Task acceptance and cleanup" and the note on a worktree
      status/checks/diff summary.)
- [ ] Any change the design needs in the format is made in the same branch
      in `tasks/README.md` and the schemas, with the reason.
- [ ] `openspec validate task-dashboard --strict --no-interactive`.

## Checks

- One spec review round, `codex-only` profile.
- Compared with Backlog.md (https://github.com/MrLesk/Backlog.md) for what
  to borrow and what not, without depending on it.

## Limits / non-goals

- Design only: no tool, no code, no Cargo change.
- The format stays portable: nothing Guild-specific in `tasks/README.md` or
  the schemas.
- Remote dashboards and multi-machine state are out of scope.

## Context

- `tasks/README.md`, `tasks/task.schema.json`, `tasks/state.schema.json`,
  `tasks/tasks.toml`.
- `.claude/skills/development-workflow/SKILL.md`, the `orchestration` skill —
  how work is assigned and supervised today.
