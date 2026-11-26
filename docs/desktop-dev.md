# Developing Guild beside the Guild you work in

Guild can be developed in Guild. What follows is how, so that the window being
worked in and the window being worked on do not get in each other's way.

## Two windows

**The Guild you work in** is `/Applications/Guild.app`, and it runs
**releases**: nightlies cut from `main`. Two recipes put a build there.

`just release` — **what the Update button offers**:

- refuses anything but `main`, a tree with uncommitted changes to tracked
  files, and a `CHANGELOG.md` whose `## [Unreleased]` has no entry — all
  three before `just check`;
- runs `just check` (its log in `target/release-check.log`), then names
  the release `nightly-<date>` — `nightly-<date>-<short hash>` when a
  tag or a heading of `CHANGELOG.md` has that day already — cuts
  `Unreleased` into it, commits `CHANGELOG.md` alone as
  `Release <name>`, and tags that commit;
- runs `just bundle`, which writes the commit, the tag and the build time
  into `Guild.app/Contents/Resources`, with the swap helper and
  `CHANGELOG.md` beside them;
- **while the installed Guild runs**, stages the bundle in
  `~/.config/guild/update/Guild.app` (the state folder, by the rule
  `GUILD_STATE_DIR` and `XDG_CONFIG_HOME` follow — the recipe prints
  where), and the **Update** pill appears right after Back and Forward
  within a few seconds; **with none running**, swaps it into
  `/Applications`;
- **never pushes**: it prints `git push origin main <name>` for you.

### A nightly built on GitHub

The `bundle.yml` workflow automatically builds a pushed `nightly-*` tag reachable
from `main`. It runs `just bundle` on an Apple Silicon macOS runner, verifies
the ad-hoc signature and embedded commit/tag, and retains a ZIP, SHA-256 checksum
and `build-info.txt` for seven days. `just bundle` uses Cargo.lock unchanged.
The job is limited to ten minutes. Optional manual recovery via
`workflow_dispatch` runs the workflow from main and checks out the selected
immutable tag, so a packaging-only correction does not require moving a release tag.

For a cloud release, run `just check` on clean main, then use the same
`guild-release check/name/cut` commands as `just release`. Commit
CHANGELOG.md alone, tag that commit, push main and the exact tag, and wait for
the automatically triggered build; do not also dispatch it manually. For
optional manual recovery, use
`gh workflow run bundle.yml --ref main -f tag=<nightly-tag>`. After successful
completion, download the artifact, verify its checksum and bundle, and publish
it as a GitHub prerelease using the existing tag. `docs/updates/owner-guide.md`
covers the signed update channel.

`build-info.txt` measures compilation plus app assembly. GitHub's job duration
also includes runner setup, cache handling and artifact upload. Record whether
the release cache was cold; debug/test CI duration does not predict this build.
The ZIP is for arm64, not Intel/universal. It is ad-hoc signed, not Developer ID
signed or notarized. This path does not install on the developer's Mac or stage
Guild's local Update button; `just release` retains that local behavior.

`just install` — **a dev build**, a branch being tried: the clean tree,
`just check`, a `dogfood-<date>` tag (local, never pushed), the bundle,
the swap into `/Applications`. **It refuses while the installed Guild
runs**, first thing, before the check and the tag: a dev build is never
offered to the Update button, so there is nothing to stage it for. Quit
Guild, or release `main`.

**Update** is offered for a `nightly-*` build built later than the one
running. Hovering it shows what that release brings since yours, grouped
as the changelog groups it. A click asks what `cmd-q` asks — unsaved
files, live Claude Code tabs — and *Cancel* leaves everything as it was;
a yes copies the staged bundle aside, starts the helper the running
bundle carries, and quits; the helper swaps the bundle in and opens it,
and the new Guild shows **What's new** once. The window never builds
itself (decision 44). ***Guild → Update Guild…*** does the same,
and says so when nothing newer is staged. Which build is running is in
***Guild → About Guild***. What the helper did is in
`$TMPDIR/guild-update.log`.

**What a release says comes from the branches.** A change you would
notice adds its line under `## [Unreleased]` in `CHANGELOG.md` in its own
branch, before it is merged (`AGENTS.md`); a release only renames the
section.

To go back: `git switch --detach nightly-<date> && just install` — a dev
build of the old release, installed while Guild is quit.

## The Guild being developed

**The Guild being developed** is `just desktop-dev <dir>`: a debug build,
with its state under `~/.config/guild-dev` (`GUILD_STATE_DIR`), started in
`<dir>`. Two things keep it apart from the one you work in:

- **Its own state directory.** Not `XDG_CONFIG_HOME`, which every session
  the window starts inherits, and git reads its global ignore file from
  under it.
- **Never the same project root.** A project's state is
  `<root>/.guild/state.json`, and two windows on one root write each
  other's tabs away. The recipe refuses this checkout; a worktree the
  installed Guild has open is the same mistake and the recipe cannot see
  it. Open a fixture, or a worktree nothing else has open.

For a dev window that must leave global harness integrations untouched, set
`GUILD_NO_GLOBAL_HOOKS=1`. Guild then skips registering Codex hooks and
installing the OpenCode status plugin. Its local socket and per-session Claude
hooks still work. Keep the real login shell and PATH so installed harnesses
remain available; do not isolate the window by replacing `XDG_CONFIG_HOME`.

Dev windows launched from a harness subprocess should remove its inherited
`NO_COLOR` from the child launch environment. Keep `GUILD_SESSION` and
`GUILD_SOCKET` intact; Guild assigns its own socket and identity to the sessions
it starts. Terminal color capability is still set by the native emulator.

`guild-desktop` opens the window; it refuses command arguments before startup.
Use its sibling `target/debug/guild` for `instructions`, `status` and `review`.
The target window comes from `GUILD_SOCKET`, even when a different `guild` binary
is used. Ordinary Guild sessions can launch explicitly authorized manual reviews
of prepared changes in their own checkout, retaining verified observation
ownership. Assigned builders continue to use their registered begin/ready task;
manual review does not refresh their allowance or create a late baseline.

## Codex provenance

Guild pins its session and socket variables in the launched Codex process’s `shell_environment_policy.set`, so resumed shell tools receive the same provenance as the TUI. This is a per-run CLI override, not a global config edit. An explicit user include-only filter can still exclude those variables; do not bypass that policy or remove the session variable to launch as an external operator.

## Isolated native trials

### Compact controls

The `tooltip-preview` example (feature `native-preview`) uses the production
Root with Diagnostic integration policy. It never polls providers or Keychain,
installs hooks or starts language servers. Use task-owned absolute state with
`.owned-tooltip-preview` containing `Guild isolated tooltip preview\n` and a
`fixture/` project containing `.owned-tooltip-fixture`. Run:

```sh
env -u NO_COLOR GUILD_NO_GLOBAL_HOOKS=1 GUILD_STATE_DIR=/absolute/task/state \
  GIT_CEILING_DIRECTORIES=/absolute/task/state \
  cargo run -p guild-desktop --features native-preview --example tooltip-preview
```

The title is **Guild UI Tooltip Trial**. Hover Open In (label only), Workspaces
(current shortcut), footer tools and panel header buttons. Compare heights,
outline, delay and content widths; move between adjacent controls, open/dismiss
a menu, toggle panels and resize the window. This is an isolated manual preview,
not visual acceptance evidence. Do not launch harnesses or browse user projects
from this fixture.

### Interface scale

`cargo run -p guild-desktop --features native-preview --example scale-audit-preview`
uses the same absolute, marked state and fixture directories as `tooltip-preview`
(`.owned-tooltip-preview` and `fixture/.owned-tooltip-fixture`), with
`GUILD_NO_GLOBAL_HOOKS=1`. It opens `fixture/example.rs`, searches the fixture for
`needle`, and supplies a missing-server status without starting an LSP. Keep all
files task-owned. If the fixture is under an ignored task directory, give its
own `.ignore` the explicit `!**` rule, or the real Search walker correctly finds
nothing. Put interface size, theme and language in that state's `settings.json`.

`GUILD_SCALE_SCENE=terminal` opens the real Terminal settings section.
`GUILD_SCALE_PHYSICAL=1` adds a 100-pixel pane using the real terminal renderer
and a short `/usr/bin/printf` process: no shell startup files or provider. Use
12.5 as the terminal setting for the sample's labelled physical comparison.
Only Search is explicitly polled in this diagnostic Root; its normal external
integration polling remains disabled. Capture and input must target only the
process returned by this launch.

### Git rows and identifier occurrences

`cargo build -p guild-desktop --features native-preview --example tree-occurrences-preview`
builds a diagnostic production Root for a marked task-owned fixture, using the
same `.owned-tooltip-preview`, `fixture/.owned-tooltip-fixture`, absolute state
and disabled-global-hooks requirements above. The fixture must have at least one
Git commit and `example.rs`. Git status, log and commit files are read before the
event loop. Explorer, Commit, Git Log and the Push window then use those snapshots;
`GUILD_TREE_SCENE=log` opens a taller Git Log; `GUILD_TREE_SCENE=push` seeds
the Push window after its normal read reset. Push has no remote/destination. Normal file editing and local Find remain live,
without providers, global hooks or language servers. Never point it at user data.
The title is **Guild Tree Occurrences Trial**; target its launch process only.

The same Root trial supports `GUILD_TREE_SCENE=commit`, `diff`, `unified`,
`diff-window`, `proposal`, `restored-annotate`, `search`, `find`, `find-popup`,
`everywhere`, `readonly`, `inline` and `markdown`. These use the production hosts,
including the real Find Usages list and its side preview. `find` supplies a local
references answer for `total`; it starts no language server. `proposal` uses
`fixture/proposed.rs` against `example.rs`, without an IDE connection or parked
apply request; leave Apply/Refuse untouched. `readonly` registers `fixture/library`
and opens `library.rs`; `markdown` opens `notes.md` in Split. Annotate and inline
blame use the Git blame captured before startup. Only local diff/search/preview
publication is polled. Popups still accept their normal query and gestures.
Capture the Push and DiffWindow satellites by both launch process and native title,
since the larger parent window belongs to that same process. A rendered screenshot
is evidence only after inspecting the actual source, selection and backgrounds;
a library editor must accept caret placement, selection and Copy while rejecting
typing, paste and deletion; sharing FileTab's occurrence owner alone is not
evidence of the actual gesture.

`GUILD_TREE_SCENE=git-markers` uses the ordinary project FileTab and adds F8 only
as a diagnostic alias for the existing production SaveFile action. This makes a
save capture independent of the machine's native keyboard layout. It adds no
product action or shortcut. Edit the owned source normally, inspect the unsaved
markers, press F8, and verify both disk bytes and the unchanged HEAD markers.
The existing `inline` and `markdown` scenes exercise the same gutter with blame
and wrapped Markdown Source; the rich Preview must show no Git marker lane.
The same scene also aliases F6/F7 to the existing component Undo/Redo actions.
These diagnostic-only keys avoid unreliable external Cmd-modifier injection when
proving a single rollback transaction. They add no product shortcut or alternate
edit implementation.

For the same marked occurrence trial, `GUILD_TREE_SCENE=annotate` reads
`fixture/dense.js` before the event loop and shows it through the production
standalone AnnotateTab. The supplied file is one blame line in this diagnostic
fixture; use a dense single JavaScript line to exercise full-line syntax
preparation before clicking any identifier. It creates no production Root,
provider, hook or language server. The same title and process isolation rules
apply.

Set `GUILD_PREVIEW_BACKGROUND=1` for that example to show its window without
activating it or taking the keyboard focus. A background capture alone
is not proof of gesture delivery: process-directed CG events can be ignored
without a key native window. Verify a driver targets the owned native window;
if it requires frontmost activation, pause concurrent typing rather than
posting global events while someone is working.
