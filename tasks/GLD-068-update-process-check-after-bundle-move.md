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

- [x] WHEN the task is handed over THEN the root cause of the false
      `Ok(false)` is recorded in this file (or in `docs/traps.md` if it is a
      trap of the platform), with the evidence that established it — not
      only a fix that makes the symptom go away.
- [x] WHEN a process runs an executable whose bundle directory has been
      renamed THEN the recovery guard reports it as in use, under load as
      well as alone; or, if `proc_pidpath` cannot be made reliable, the guard
      identifies the live executable another way (for example by vnode or
      by device and inode taken before the move) and the recovery refuses
      while it runs.
- [x] WHEN the fix is in THEN `#[ignore = "flaky in the full workspace run:
      GLD-068"]` is removed and the test passes in at least three repeated
      full `just check` runs under load comparable to the one that failed
      (load average around 10, another session's tests running).
- [x] WHEN the cause is in production code THEN a regression test pins it,
      failing before the fix for the right reason.
- [x] WHEN an error is the cause (a path that cannot be read, a process
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

## Cause (established 2026-11-27)

The guard was right; the fixture died. The test's "live Guild" was a copy
of `/bin/sleep`, and a copy of a system binary keeps Apple's platform
signature: macOS 26.5 kills such a copy with `SIGKILL (Code Signature
Invalid)` some hundred milliseconds after it starts, whether or not anything
is renamed. Alone the test finished before that; under a loaded `just check`
the kill landed after `try_wait` had said "running" and before `in_use`
looked at the pid, which by then was a zombie — `proc_pidpath` answers
`ESRCH` for a zombie, and skipping it is correct.

Evidence, gathered beside the other session's `just check` (load 7–23):

- `~/Library/Logs/DiagnosticReports` held 131 `sleep-*.ips` since
  2026-11-20, every one `procPath …/previous.app/sleep`, `SIGKILL (Code
  Signature Invalid)`, either with no sender or killed later by the test
  binary's own cleanup.
- The test itself, instrumented to report the child when `in_use` said
  `false`: 5 failures of 40 under load, and in all 5 the child had already
  exited by signal 9, before the test's own `kill`. Unchanged test: 3 of 30
  with no synthetic load, 4 of 30 beside 16 busy loops.
- A C reproduction (copy, `posix_spawn`, rename, `proc_listpids`,
  `proc_pidpath`, `ps`), run side by side under the same load:
  - copy of `/bin/sleep`, nothing renamed, 500 ms after the start:
    40 of 40 dead (34 of 40 in another run); 200 ms: 9 of 40;
  - original `/bin/sleep`, the same timing: 0 of 40; a copy re-signed ad hoc
    with `codesign --force --sign -`: 0 of 40 with and without the rename;
  - every false answer (61 of 61 in one run) was a process `ps` showed as
    `Z`, `PROC_PIDTBSDINFO` also refused, and `waitpid` reaped it at once;
    no `ENOENT`, no stale or differently formed path and no live process
    missing from `proc_listpids` was seen in any of about three thousand
    runs, so the hypotheses above about the kernel's path, the canonicalize
    fallback and the process list are not what failed.
- A second, rarer effect: renaming the directory within the first
  milliseconds of the process's life (while it is still being loaded) killed
  even an ad-hoc copy — 3 of 40 renamed at once, 0 of 40 two milliseconds
  later. The mechanism of either kill is not established; both are the
  system's, not Guild's.

The fix is in the test only: the fixture copies `/bin/cat`, re-signs it ad
hoc, and renames the bundle only after the child has echoed a line back.
With it, 0 of 200 runs failed (100 with no synthetic load, 100 beside 16
busy loops at load 20). Without the re-signing the same test fails 40 of 40,
at the echo, with `cat-*.ips` reports of the same `Code Signature Invalid`
kill. `docs/traps.md` records the trap for other fixtures.

`in_use` is unchanged, by the owner's decision: `ESRCH` is a process that is
gone or a zombie, and `ENOENT` is a live process whose executable no longer
exists at any path — such a process holds none of the recovery's slots, and
refusing on it would block a recovery whenever any of the owner's programs
outlived its own update (a `claude --chrome-native-host` did, on this
machine, during the investigation).

## Result

- Acceptance 4 and 5 are met by their condition: neither the cause nor an
  error is in production code, so `in_use` keeps no new test or `Err`. The
  fixture's regression is pinned by the test itself: unsigned, it fails 40
  of 40 at the echo, under load.
- Six isolated `just check` runs (`GUILD_STATE_DIR` in a scratch folder,
  `GUILD_NO_GLOBAL_HOOKS=1`, the session variables unset), the other
  session's tests running beside most of them. The guard's test passed in
  all six. Three runs were green. Three failed once each, in tests this
  branch does not touch: `fixed_footer_glyph_corner_alignment` (an
  `unwrap` on `None`, footer_tests.rs:1202) and
  `a_failed_status_read_cancels_the_held_commit_and_keeps_the_ticks` (git's
  `index.lock` exists, commit_tests.rs:1565), both with 8 synthetic busy
  loops beside them (load median 51, peak 128); and
  `the_sessions_page_lists_by_urgency_settles_and_takes_keys` (two Working
  rows in the wrong order, sessions_tests.rs:130) with no synthetic load
  (median 17, peak 34). They are left to their own tasks.
