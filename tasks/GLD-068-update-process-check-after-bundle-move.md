---
id: GLD-068
title: The update recovery's process check misses a live Guild after its bundle moves
kind: fix
status: backlog
priority: 1
component: desktop
depends_on: []
touches:
  - crates/desktop/src/system/update_processes.rs
  - crates/desktop/src/update/recovery.rs
harness: any
reviews: {spec: 0, code: 2}
created: "2026-11-26"
---

## Goal

`system::update_processes::in_use` is the guard manual update recovery
(`crates/desktop/src/update/recovery.rs:30`) uses to refuse restoring or
keeping a bundle while an affected Guild still runs. Its test
`live_executable_is_refused_even_after_its_bundle_directory_moves`
(`crates/desktop/src/system/update_processes.rs`) failed twice in a row in
the full workspace `just check`, and was ignored to unblock a release. The
failure says `in_use` answered "not in use" for a process that was alive,
after its bundle directory was renamed — exactly the situation a recovery
meets, since an update moves `Guild.app` aside while the old one may still
run. In production that is a recovery that overwrites or moves a bundle
under a live Guild.

When this task is finished, the cause is known and written down, `in_use`
(or the recovery guard by another means) is reliable after a bundle rename,
and the test runs again in the full workspace run.

## Acceptance

- [ ] WHEN the task is handed over THEN the root cause of the false
      `Ok(false)` is recorded in this file (or in `docs/traps.md` if it is a
      trap of the platform), with the evidence that established it — not
      only a fix that makes the symptom go away.
- [ ] WHEN a process runs an executable whose bundle directory has been
      renamed THEN the recovery guard reports it as in use, under load as
      well as alone; or, if `proc_pidpath` cannot be made reliable, the guard
      identifies the live executable another way (for example by vnode or
      by device and inode taken before the move) and the recovery refuses
      while it runs.
- [ ] WHEN the fix is in THEN `#[ignore = "flaky in the full workspace run:
      GLD-068"]` is removed and the test passes in at least three repeated
      full `just check` runs under load comparable to the one that failed
      (load average around 10, another session's tests running).
- [ ] WHEN the cause is in production code THEN a regression test pins it,
      failing before the fix for the right reason.
- [ ] WHEN an error is the cause (a path that cannot be read, a process
      list that cannot be taken) THEN the guard answers `Err` — which the
      recovery turns into `Failure::Conflict` — rather than "not in use".

## Checks

- The test alone, in a loop, before and after:
  `cargo test -p guild-desktop --lib live_executable_is_refused_even_after_its_bundle_directory_moves -- --include-ignored`
  (currently 10/10 green alone, which is why a loop alone proves nothing).
- A reproduction under load: the test in a loop while the workspace test
  suite or a synthetic load runs beside it; report how many runs failed
  before the fix and after.
- `just check`, repeated, with `GUILD_STATE_DIR` in a scratch folder and the
  `GUILD_*` session variables unset.
- Not run: a real update recovery against `/Applications`; the guard is
  checked through its test and the reproduction.

## Limits / non-goals

- The guard never signals or kills a process; it only refuses. That stays.
- No change to the recovery's journal, its choices or its command line.
- Not a redesign of the update transaction; only what the guard sees.
- A user-visible changelog line only if the recovery's behaviour as a person
  sees it changes (for example, a new refusal message).

## Context

The failure, both times, in the full workspace `just check`:

```
panicked at crates/desktop/src/system/update_processes.rs:75:13:
assertion failed: in_use(&[executable.clone(), moved.join("sleep")]).unwrap()
```

The test copies `/bin/sleep` into a temporary `Guild.app`, starts it,
checks `in_use` (green), renames `Guild.app` to `previous.app`, checks that
the child is still alive (`try_wait` green), and then `in_use` over the old
and the new path returned `Ok(false)`.

Conditions: `just check` with `GUILD_STATE_DIR` in a temporary directory,
the `GUILD_*` session variables unset, load average about 10–12, another
session running its own tests concurrently. Run alone it passed 10 times
out of 10. The root cause has not been established; nothing below is
verified.

How `in_use` works (`update_processes.rs`): it canonicalizes each target,
falling back to the path as given when canonicalization fails; lists the
owner's pids with `proc_listpids(PROC_UID_ONLY)` into a 4096-entry buffer;
for each pid takes `proc_pidpath`, skips it on `ESRCH` or `ENOENT`,
canonicalizes the answer (falling back to it as given) and compares it with
the targets.

Hypotheses to check:

- **`proc_pidpath` fails with `ENOENT` for the moved executable** and the
  pid is skipped as if it had exited. The kernel builds the path from the
  vnode's name cache; after a rename under memory pressure the entry may be
  purged or not yet rebuilt, and `vn_getpath` can fail. The skip on
  `ENOENT` would then turn a live process into "not in use". This is the
  first one to check: log the errno per pid in a reproduction.
- **`proc_pidpath` returns a stale or differently formed path** — the old
  `Guild.app/sleep`, a path through `/private/var` versus `/var`, or a
  firmlink form — which then fails to match because the old path no longer
  canonicalizes and the new one does.
- **The canonicalize fallback** for a target that no longer exists: the old
  `executable` cannot be canonicalized after the move and is compared as
  given; the kernel's answer for the old path (if stale) is also compared
  as given. Check that both sides fall back to the same form.
- **The process list is incomplete**: `proc_listpids` answered with fewer
  pids than exist (a race with processes starting and ending under load)
  and the child was not among them; or the list filled the buffer.
- **An interaction with a concurrently running test** — another test (in
  this binary or another session's) touching the same temporary directory
  naming, `/bin/sleep` copies, or the process table — so the pid or path
  seen was not the fixture's.

Callers: `crates/desktop/src/update/recovery.rs:30` is the only one; it maps
`Err` to `Failure::Conflict` and `Ok(true)` to a refusal.
