# Contributing to Guild

This page is for people. The rules every change follows — how work is done,
the desktop crate's module map, the boundaries the compiler does not check —
are in [`AGENTS.md`](AGENTS.md), which agents read too; read it first rather
than this page twice. Before working in an area, read its entries in
[`docs/traps.md`](docs/traps.md).

## Reporting a problem

- **A bug or an idea:** open an issue with the
  [bug report or feature request form](https://github.com/proshik/guild/issues/new/choose).
- **A security problem:** report it privately, as [`SECURITY.md`](SECURITY.md)
  says — not in an issue.

For a bug, the form asks for the version (**Guild ▸ About Guild**, which
shows the build's commit) and the log, `~/Library/Logs/Guild/guild.log`
(**Help ▸ Show Log** reveals it in Finder).

## Building

You need macOS on Apple silicon, the Rust toolchain from
[rustup](https://rustup.rs) — not Homebrew's `rust`, whose `cargo` ignores
`rust-toolchain.toml` — [`just`](https://github.com/casey/just) and the Xcode
Command Line Tools.

```bash
just toolchain   # must print "overridden by '<repository>/rust-toolchain.toml'"
just desktop     # run a development build
just --list      # everything else
```

[`docs/desktop-dev.md`](docs/desktop-dev.md) shows how to run the build you
are working on beside an installed Guild without either touching the
other's state.

## Making a change

- **Work in a branch**, one change per branch.
- **Run `just check` before each commit and before a pull request:**
  formatting, clippy with no warnings, crate boundaries, tests — in that
  order. Tests that touch Guild's state should run with `GUILD_STATE_DIR`
  set to a scratch folder, as [`AGENTS.md`](AGENTS.md) explains.
- **A bug fix starts with a test** that fails for that bug.
- **A change a person would notice** adds one line to
  [`CHANGELOG.md`](CHANGELOG.md) under `## [Unreleased]`, written for the
  person reading it — what they will see, not which module moved.
- **A change a person would see in the window** is designed in Penpot and
  agreed before it is coded — [`docs/penpot.md`](docs/penpot.md). Ask in the
  issue first; a pull request that changes the window's look without that
  step will be asked to take it.
- **A larger or unclear change** is agreed in a task file first:
  [`tasks/README.md`](tasks/README.md) describes the format, and a rule that
  outlives the change goes to [`docs/decisions.md`](docs/decisions.md).

## License

By contributing you agree that your contribution is licensed under the
same terms as Guild: Apache-2.0 or MIT, at the user's option.
