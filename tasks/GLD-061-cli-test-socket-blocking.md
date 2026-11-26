---
id: GLD-061
title: A CLI test reads from a socket accepted in non-blocking mode
kind: fix
status: backlog
priority: 2
component: cli
depends_on: []
touches:
  - crates/cli/tests/cli.rs
harness: any
reviews: {spec: 0, code: 0}
created: "2026-11-24"
---

## Goal

`just check` fails now and then in `crates/cli/tests/cli.rs`, and the
fault is in the test, not in the `guild` binary. Three tests make their
`UnixListener` non-blocking so they can poll `accept()` against a deadline.
On macOS an accepted socket inherits `O_NONBLOCK` from its listener, so the
shared `request()` helper, written for a blocking stream, can read before
the client's bytes arrive. It then gets `WouldBlock` and panics on
`.unwrap()`.

When this task is finished, every stream these tests accept is read in
blocking mode, the tests pass 50 runs in a row in isolation, and nothing
the `guild` binary does has changed.

## Acceptance

- [ ] WHEN a test accepts a stream from a listener it made non-blocking
      THEN the stream is put back into blocking mode before `request()`
      reads it (`stream.set_nonblocking(false)`). Alternatively, `request()`
      copes with `WouldBlock` by waiting and retrying, bounded by a
      deadline. One of the two, applied to every such test.
- [ ] WHEN the three tests below run 50 times in a row, isolated THEN all
      50 pass:
      `terminal_wait_timeout_does_not_cancel`,
      `workflow_start_forwards_project_dependent_overrides_to_the_owning_window`,
      `workflow_review_forwards_the_explicit_parent_request_without_local_launch`.
- [ ] WHEN the fix is in THEN the tests that already accept on a blocking
      listener (`cli.rs:57-58`, `:921-922`) still read through the same
      `request()`, unchanged in behaviour.
- [ ] WHEN the diff is read THEN it touches only `crates/cli/tests/cli.rs`:
      no change under `crates/cli/src/` and no `CHANGELOG.md` line, since
      this is a test.

## Checks

- A loop of 50 isolated runs of the three tests, e.g.
  `for i in $(seq 50); do cargo test -p guild-cli --test cli -- <name> || break; done`
  for each name, with `GUILD_STATE_DIR` in a scratch folder and
  `GUILD_NO_GLOBAL_HOOKS=1`. Report how many runs passed, not only the
  last one.
- `just check`, isolated the same way, with `GUILD_USER_STATUSLINE` unset.
- No reproduction is needed before the fix: the failure is intermittent.
  It counts as proven by the inheritance check below together with the
  observed panic.

## Limits / non-goals

- No change to the binary, its socket client or its timeouts.
- No new test harness or helper crate. The fix stays in the test file.

## Context

Seen by the parent session on 2026-11-24. An isolated `just check` failed
once in `workflow_review_forwards_the_explicit_parent_request_without_local_launch`,
which panicked at `cli.rs:34` with `Os { code: 35, kind: WouldBlock }`.
Run alone, it failed 1 run in 5. That observation was the parent's and
is not repeated here.

Read from the code on 2026-11-24 (main at `dd1851e2`):

- `crates/cli/tests/cli.rs:29` is `request()`. `:34` is
  `stream.read(&mut chunk).unwrap()`, in a loop that expects a blocking
  read: it treats `0` as end of stream and has no `WouldBlock` arm.
- The three tests that share the pattern (`set_nonblocking(true)` on the
  listener, then `request(&mut stream)` on what `accept()` returns) are
  the only `set_nonblocking` calls in `crates/cli/tests/`:
  - `cli.rs:1707` is `terminal_wait_timeout_does_not_cancel`, with the listener
    at `:1711` and the read at `:1717-1718`. Added in `1ffce297`. Its
    server thread uses `if let Ok(..) = listener.accept()`, so a panic in
    `request()` kills that thread. It was not checked how the test then
    fails.
  - `cli.rs:2024` is
    `workflow_start_forwards_project_dependent_overrides_to_the_owning_window`,
    with the listener at `:2028` and the read at `:2032-2034`. Added in
    `04581f84`.
  - `cli.rs:2104` is
    `workflow_review_forwards_the_explicit_parent_request_without_local_launch`,
    with the listener at `:2109` and the read at `:2113-2115`. Added in
    `bbb61f8e`.
- `cli.rs:57` and `:921` accept on listeners left blocking. They are not
  affected.

**The cause was confirmed on this machine (Darwin 25.5), outside Rust.** A
C program made a Unix listener non-blocking, connected, accepted, and read
before the client wrote. The accepted socket had `O_NONBLOCK` set, and
`read` returned `EAGAIN` ("Resource temporarily unavailable"; errno 35 on
macOS, which is `WouldBlock`). macOS `accept(2)` says the new socket has
"the same properties of socket". Not checked: that Rust's
`UnixListener::accept` on macOS calls plain `accept` rather than setting
the flags itself. It cannot use `accept4`, which macOS lacks, and the
observed panic matches. Also not checked: whether Linux CI can hit the
same failure. Linux `accept` does not inherit `O_NONBLOCK`, so it probably
cannot.
