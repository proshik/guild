# Guild

**An ADE — the window a person works in all day while AI agents do the work: a file tree, an editor, diffs, git, and agent sessions in tabs.**

Agents run in the next tab, in the worktree they were given; the person reads what they did — the files, the diff, the commit — in the same window, and answers them there. Nothing needs a server: sessions, tabs and settings are local.

> **Status: in daily use, and released as nightlies through its own Update button.** The workspace holds **3,811 tests** — 3,035 in the desktop, 706 in the session crate and 70 in the command line, counted as the `#[test]`, `#[gpui::test]` and `#[tokio::test]` functions in the source. 359 of them are `#[ignore]`d on purpose: most are run by a wrapper test inside an isolated subprocess, so `just test` runs them anyway; the session crate's eight are what `just test-live` runs; and a handful — measurements, checks of an installed package — are run by hand.

---

For resume behavior and saved-instruction limitations, see [Sessions](docs/sessions.md).

## What makes Guild different

| | |
|---|---|
| Unit of work | **an agent session in a tab**, beside the files it changes, the diff it made and the commit it becomes |
| Agents | Claude Code, Codex and OpenCode, each running its own CLI in **a terminal of ours**; what each is doing it reports itself, in the terminal — working, waiting for the person, finished — and a finished turn stays unread until it is looked at |
| Worktrees | **a column of repositories and their worktrees**, made and removed by a person or by an agent through `guild` — or, switched to Sessions, of the agents, by who needs you first |
| Driven by agents | `guild`, a command line the open window answers: an agent makes a worktree, starts a session in it with a task, waits for it and reads what it said |
| An agent's edit | arrives as a diff to apply or refuse, through `claude --ide`; review runs outside, through revust, as a terminal job |
| State | local, in `~/.config/guild` — no server, no account |
| Stack | Rust throughout |

## Layout

A Cargo workspace of three crates. The boundaries between them are hard — that is discipline, not bureaucracy.

```
crates/
  session/        what can be checked without a window: sessions, transcripts,
                  the harnesses and what each remembers, the status board,
                  settings, the workspace and the division of the window into
                  areas and panes, the command vocabulary `guild` and the
                  window share, the changelog, terminal jobs, the workflow and
                  the update's persistence. No GPUI
  desktop/        the GPUI window: title bar, the column, tabs, our own
                  terminal, the editor and everything around it
  cli/            `guild`, the command line the open window answers, and
                  `guild-release`, which `just release` asks. No GPUI
```

[`AGENTS.md`](AGENTS.md) maps the desktop crate module by module.

Three boundaries are held not by the compiler but by the `just boundaries` recipe, which is part of `just check`: `session` and `cli` have no GPUI — otherwise transcript parsing could not be checked without an open window, and `guild` could not start in milliseconds — and the `desktop` tree contains neither the `bezel-gpui` fork nor `gpui-terminal`. The recipe looks at `cargo tree`, not `grep` over the manifest, because `grep` cannot see a transitive dependency; each guard has been made to fail and prints the path. A guard whose subject has gone passes silently and looks alive, so a guard is removed with the crate it guards.

The `check` workflow on GitHub Actions **does not run on a push**. It has two jobs, both started by hand from the Actions tab or with `gh workflow run check.yml` before a merge to `main`: `headless`, clippy and the session and CLI tests on Linux, and `desktop`, everything `just check` runs, on macOS — `guild-desktop` links `objc` and builds only there, and macOS minutes bill at ten times the Linux rate. What stands in for it every day is `just check` before each commit. A second workflow, `bundle`, builds an unsigned release bundle when a `nightly-*` or `v*` tag is pushed, or by hand for such a tag on `main`.

## Terminal sessions in the desktop

Claude Code, Codex and OpenCode run their own CLI in a pseudo-terminal, including
permission dialogues and slash commands. What a session is doing is what its CLI
reports in the terminal (OSC 7501); the hooks only name the conversation and say
when the person submitted a prompt. Transcript cost takes precedence over a legacy
saved cost.

The rendering is ours, on top of `alacritty_terminal`. The off-the-shelf component
was tried and dropped because it lost the dim-text flag used by CLI hints.
Earlier records saved as `stream` load as terminal sessions with the same
conversation identity. Settings no longer offer a session mode.

## The window

Along the top is the title bar: Workspaces and Back and Forward at its left, and at the column's right edge the button that switches it between Projects and Sessions; the branch of the project in front with a ↓ beside it that updates the branch from a source picked in a list, after a question; *Update* while a nightly is offered; and at its right the controls Settings lets you order — the plans' usage, Open In, and the terminal jobs to recover. A double click on it does what the system is set to do — that is a person's own setting, and its default is zoom rather than full screen, which is `ctrl-cmd-f`.

Down the left is the column: **a project is a repository and its rows are its worktrees**, read from `git worktree list` — a worktree made by hand or by an agent in a terminal is there as surely as one made here — each with its agents' marks and git's count. Under a worktree are its agent and terminal tabs, each with its agent's mark and status, a click away. A worktree is made from a project's `+` and removed from its row, after a sheet that names what git would delete without a word. Projects can be gathered into named, collapsible groups, dragged into one order, and a group can be bound to the folder that holds its repositories, where a session sees them all. The column belongs to a workspace, a named set of roots, and the foot of it switches between them. Switched to **Sessions**, the column lists the workspace's agents instead, by who needs you first — waiting for you, working, done, idle — each with its project, branch and how long since it last did something; agents idle for days and conversations closed lately wait under Settled, where a closed one can be continued.

Beside it, the tabs of the worktree in front. **A tab is a view onto something, not a session:** agent sessions and terminals, Guild's own settings, files — Markdown drawn as a document — diffs, Annotate, and a workflow's Process page. A mark says which — its shape agent, shell or settings, its colour the harness — and a dot appears only when a tab is waiting for an answer. With no tab open, the start screen shows the default harness, a tile per kind of tab and what was said in this folder before.

A session tab is named by the program running in it. `claude` writes the conversation's topic into the terminal title and a shell writes where it is; that is what the tab shows. Beside the name is what the session has spent — money when the transcript reports it, tokens when it only reports those.

**A seat divides, and so does a tab.** Beside the work you can split the screen into areas — `⌃⌘⇧→` puts a new one to the right, `⌃⌘⇧↓` below — each a card of its own with its own row of tabs and its own tab in front; switching a tab in one area moves nothing in another. Inside a tab, `⌃⌘⌥→` and `⌃⌘⌥↓` split it into panes, a hairline apart, the focused one outlined. A new area or pane is empty and shows the start screen, whose Recent and foot drop away when it is short. `⌃⌘` and an arrow moves the focus to the neighbouring pane or area, `⌘W` closes a pane before it closes the tab, and a tab dragged onto another area's strip, or sent there from its menu's *Move to Area*, moves with its process untouched. The last area stays; its panes hide with their tab; the side panels are cards of their own beside the areas; and all of it comes back after a restart.

Everything that starts something is in the File menu — a project, which creates its directory, a clone from Get from Version Control, a session, a terminal — together with Open Recent and Reopen Closed Tab. Every shortcut is editable in Settings ▸ Keymap: click the keys, press the new ones, and it is in force at once.

Three panels frame the work — left on `cmd-b`, right on `cmd-l`, bottom on `cmd-j` — resized by dragging a border or by `cmd-shift-←` and `cmd-shift-→`, and a double click on a border puts it back to the width the design system gives it. The left one holds the project's files: a tree read a directory at a time, `.gitignore` respected and ignored entries dimmed, each file with the icon its kind has everywhere else — the Material Icon Theme's, a Kotlin or Java file drawn by what it declares. One click opens the file in a tab, highlighted by tree-sitter and edited in `gpui-component`'s editor: a brass dot while there is unsaved text, `cmd-s` writing beside the file and renaming over it, and a question — Save, Don't Save, Cancel — before a dirty tab is closed, quit past or reloaded. The disk is watched, so a file the agent writes appears in the tree within a second and an open tab follows it: reloaded in place when clean, marked in rust and left alone when dirty. The tree says what git says: `M`, `A`, `D` and `?` at the row's edge, in the accents that already mean those things, and on the directories above a change; the marks move without a click when a file is written, staged or committed from any terminal. A click on a mark, or `cmd-d` on a file, opens its diff against the last commit in a tab — two number columns, `+` and `−` in their accents, the changed lines on the design system's two backgrounds — and the diff follows the buffer as it is typed, before any save. Its lines carry the same colours the tab does: a language and its grammar are separate things, so that `Cargo.lock` is read as TOML, `package-lock.json` as JSON and `mix.lock` as Elixir, and the status bar names the language so a file taken for the wrong one can be seen; thirty-four grammars stand behind them. `cmd-shift-d` opens *Diff · N files*: every changed file with what it gained and lost, and under the list every hunk; a commit empties it within a second. A double click on a line opens the file there. The diff is side by side by default — the old text on the left, the new on the right, a changed line facing what replaced it — with a switch to the unified view at the header's right, and it can compare against the index or a commit, not only `HEAD`. A toolbar above the rows steps between hunks on `f7` and `shift-f7` and counts them, and a bar at the right carries a mark for every hunk in its colour, each a click to it. The `+` beside a change stages that one change and the `−` unstages it — the editor's change markers offer the same beside Rollback — and git takes the write only over the index it was computed from, so a file changed since is shown again rather than staged wrong. The right button opens a menu wherever it means something: on a tree row — open it, show its diff, copy its path, reveal it in the Finder; on a tab — close it, close the others, close them all; on a change — its diff against the last commit, against the index, and the index against the disk; on a commit — copy the revision or the message; on a branch — list it. The tree's menu also makes, renames and trashes a file and the Commit panel's rolls a change back; what guards them is a sheet that names what it is about.

The branch in the title bar is a button. The popup behind it lists the local branches — each with the tip's author, its age and its subject — and the stash beside them; typing narrows the list, or offers to create the name that matched nothing. A branch is switched to, made, or deleted where git agrees it can be; a stash is applied onto the working tree, or dropped. **The window works nothing out about what is safe:** it says `switch` rather than `checkout`, `branch -d` rather than `-D`, `stash apply` rather than `pop`, and when git refuses — a checkout that would overwrite work, a branch whose commits are on no other — its own words become the notice.

A file is written as it is edited: 300 ms after the last keystroke, when the editor loses focus, when the tab goes behind another, and before a commit — never over a change something else made to the disk, which is the one case the brass dot and the question before closing still cover. The panels are tool windows: the Explorer, the Commit panel, Git and the rest are tools, each in a dock — left, right or bottom — and the buttons that open them sit in the status bar, lit while their dock is open. A tool moves to another dock by dragging its button onto the panel it should live in — or to the window's edge, where a zone lights up for a dock that is closed — or from the View menu. A dock holding more than one tool names them as tabs in its head. At every head's right sit two controls: a ⋮ that opens the panel's own menu — *Move to* the two docks it is not in, and *Hide* — and a bar that puts the panel away, which is a bar rather than a cross because a dock is hidden and not closed. **Explorer** is the tree. **Commit** is the Commit window: one list of changes with every file ticked the moment it appears, unversioned files below it unticked, a toolbar to refresh, show a diff, tick or untick all, fold the sections; the ticks are the panel's own and say what the commit takes — nothing is written to git until you commit, when the index is brought to the ticks and the commit made, except that a file only partly staged hunk by hunk wears a partial tick and commits just what is staged; the message box under the list holding a second paragraph or a line starting with `#` intact; the notice names the hash, a failing hook is git's words with the message still in the box. **Git** lives at the bottom and holds two faces, named as tabs in the dock's own head. *Log* is the local branches at the left, the commits newest first two hundred at a time, and the opened commit at the right; moved to a side dock it draws the list with the commit under it. A commit is one line — the author, a dot that is a ring for a merge, the message, its refs as chips and its date at the right — and a toolbar above narrows the list: a word searches the messages, four or more hex characters mean *that commit*, and *Branch*, *User* and *Date* are menus whose value the button shows and whose cross takes off. Every one of them is an argument of `git log` rather than a sieve over the two hundred loaded, so the answer is about the whole repository. The opened commit shows its files as a tree above its message, single-child directory chains folded into one row. *Console* is the second face: the writes the window makes, each with the working directory, git's own argv and what git answered — reads stay invisible. A commit from anywhere is the top row within a second. *Annotate* opens the file in front with a column that says who wrote each run of lines and how long ago, dimmer the older; a hover gives the hash and the summary, a click opens the commit in the Log.

The desktop writes to the repository through twenty-one named writes and nothing else: `git add`, `git restore --staged`, `git commit`, `switch`, `switch -c`, `branch -d`, `stash apply`, `stash drop`; `git restore --source=HEAD --staged --worktree` for Rollback and `git push`; `worktree add -b`, `worktree remove`, `worktree unlock`, and a removal confirmed in its sheet that may take the branch with it through the same `branch -d`; Get from Version Control's `clone` through a private staging directory; a bounded `fetch` of one source and a `merge` pinned to what it fetched; the two fast-forwards that keep a clean default branch up to its upstream without a click — a `fetch` into the branch with no `+`, and a `merge --ff-only` where it is checked out; and Stage Hunk and Unstage Hunk, one `apply --cached` each. `cmd-k` toggles the Commit panel, `cmd-enter` in the commit message box makes the commit, and `cmd-shift-k` opens Push Commits, a window of its own. The count is not the rule and the guards are; the destructive spellings — `-D`, `--force`, `pop`, `clear`, `--hard` — are spelled nowhere, and a test greps every write for all of them **by substring**, which is what bars `--force-with-lease`.

**Run** is what a person reaches for after an agent turn: the Run action runs the project's check and the panel shows it running. It has no stroke by default — `cmd-r` is Replace — and Settings ▸ Keymap gives it one. The console is a terminal of ours rather than lines we print, so `cargo`, `clippy` and `just` colour their own output and redraw it as they mean to; the command runs in the shell the person logs in with, because a bundled `Guild.app` inherits a `PATH` from `launchd` that has no `/opt/homebrew/bin` in it and `just` would be *not found* in the bundle and nowhere else. At the toolbar's right is **what became of it** — *running · 12s* while it goes, then *exit 0 · 1m 04s* in green or *exit 101 · 8s* in rust, and *stopped* for a run the Stop button ended, which keeps everything it had printed. The command itself is a button, and the list under it is what this project can run: what was run here lately, then what its `justfile` and its `package.json` declare, then whatever is typed that is not in the list yet. A run belongs to the project, and nothing outlives the window: quitting stops every process the window started. Under everything a status bar that does not hide says where you are and holds the tools' buttons.

The window finds things the way an IDE does. A double Shift opens Search Everywhere, whose tabs are Go to File, Go to Symbol on `cmd-alt-o` and Find Action on `cmd-shift-a`; `cmd-shift-f` opens Find in Files, a popup with the hits above a preview of the code around the lit one, and `cmd-shift-r` replaces across the project. Underneath is ripgrep's own libraries rather than `rg` on the `PATH` or `git grep`, which searches tracked files and would be blind to what the agent has just written — on a thread, so a tree with no `.gitignore` in it does not freeze the window for the twelve seconds it takes. *Open in Find Window* puts the hits in the Search tool as a **tree of the files they are in**, and `alt-f7` on a name finds its usages, asked of the language server, in the Find tool.

The window is an IDE the agent in the next tab can talk to. A `claude` session started here connects to it, and when it wants to change a file the change arrives as a tab — the file as it is beside what the agent proposes, with **Apply** and **Refuse** in the toolbar and the halo on the tab while it waits, because the agent is waiting too. Refusing tells it so and it moves on without writing; applying closes the diff, and the agent then writes the file in its own turn, with its own question in the terminal. It can also ask the window what it has open, whether a file has unsaved text, and what the language servers say is wrong — the same map the underlines are drawn from.

An agent can drive the window too. Every session the window starts has `guild` on its `PATH` and is told so: `guild worktree create`, `guild session new --worktree … --prompt …`, `guild agent wait` and `guild agent read` make a worktree, start a session in it with a task and wait for what it says — in the window's own column and tabs, not beside them. `guild help` lists the rest.

`cmd-alt-shift-o` opens **Recent Projects**: type to narrow the list, arrows and the page keys to move through it, Enter to open. It matches the whole path rather than the last component, because two checkouts of one repository differ by the directory above them. The popup is dragged by its header, resized from its edges and corners, and remembers where it was left.

Every field Guild owns is the editor component's single-line input: a real caret and selection, `cmd-a`, copy, cut and paste, undo, and the shell's deletions on top — `⌥⌫` and `ctrl-w` take a word, `⌘⌫` and `ctrl-u` take the line, and a word ends where readline says it ends, so a path comes back one component at a time. The window's main occupant is a terminal, and a field that deleted differently would be the one that was wrong.

The terminal handles what a terminal is expected to handle: selection and copy-on-select, bracketed paste, a blinking cursor whose shape the program chooses, a scrollbar over the history, find in the history, links followed with `cmd`, and a wheel that reaches a full-screen program — `claude` reads the mouse itself, so it receives the wheel rather than having the terminal act on it.

## Stack

Rust stable (1.98 on the machine it is written on; the workspace asks for 1.88 at least) · tokio · tracing · GPUI (`gpui-pre` 0.3.8, with `gpui-kit` and `gpui-component` 0.7.1) + alacritty_terminal + portable-pty for the window, `gpui-component` for the editor, tree-sitter and thirty-four grammars for the colours, ripgrep's `grep-regex` and `grep-searcher` for search, `lsp-types` and a client of ours for the language servers, `tungstenite` for the socket `claude --ide` connects on, `ureq` for the plans' usage and the update's downloads, `notify` for the disk, and `objc` reaching AppKit for the window details GPUI does not expose.

## Documents

- [Project rules](AGENTS.md) — how work is done, the desktop crate's module map, and the boundaries that are easy to break unnoticed
- [Decisions](docs/decisions.md) — the rules in force, under the numbers code cites
- [Design system](docs/design-system.md) — tokens, component anatomy and measures; the theme's tests read it
- [Traps](docs/traps.md) — bugs and hours this repository has already paid for, by area
- [Runtime ownership](docs/runtime-ownership.md) — who owns the processes the window starts
- [Search](docs/search.md), [Sessions](docs/sessions.md), [Overlays](docs/overlays.md) — how those behave
- [Development workflows](docs/workflows/README.md) — the configured process definitions
- [Developing Guild in Guild](docs/desktop-dev.md) — releases, dev builds and the dev window beside the installed one
- [GPUI Kit](docs/gpui-kit-migration.md) — what to know before raising the GPUI and component versions
- [Update sources](docs/updates/owner-guide.md) — the owner's runbook for distribution
- [OpenSpec](openspec/README.md) — how a change's behaviour is agreed before it is built; `openspec/specs/` is the baseline

The window is designed in Penpot before it is coded — [docs/penpot.md](docs/penpot.md) says where and how.

## Running it

Mermaid and PlantUML files (.mmd/.mermaid/.puml/.plantuml) and Markdown diagram
fences render offline through Source/Split/Preview. Official browser scripts and
a small system WebKit helper ship inside Guild.app; no separate installation,
Node, Java or bundled browser is required. For development, `just desktop` and
`just desktop-dev` explicitly prepare the helper from the installed Apple
toolchain and committed JS, without downloads. Bare Cargo builds can use
`just prepare-diagrams` and `GUILD_DIAGRAM_RESOURCES=$PWD/target/diagram-runtime`.
See [renderer resources](tools/diagrams/renderer/README.md).

```bash
just desktop            # development build
just desktop-release    # optimised; eight times faster on heavy output
just app                # assemble Guild.app and open it
just desktop-dev <dir>  # the build being developed, its state apart from yours
just install            # a dev build swapped into /Applications
just release            # main only: cut a nightly and hand it to the Guild you work in
```

Settings live in `~/.config/guild/settings.json` (or under `$XDG_CONFIG_HOME/guild`), projects and sessions next to them in `workspace.json`, and any shortcuts you have changed in `keymap.json` — only what differs from the defaults, so a changed default still reaches you. A file in the previous format is read and migrated; a corrupted one leaves the defaults in force rather than stopping the window from opening.

Settings → **Tabs & agents** separates **New tabs** (what + opens, including a plain Terminal) from **Claude Code**, **Codex** and **OpenCode**. Each agent has its own **Launch command**, with **Prefix (before program)** and **Arguments (after program)** fields. Prefix `pai` plus arguments `--model glm` launches `pai codex --model glm <Guild arguments>`; these argument words follow the program, so a wrapper can consume them. Arguments support shell quoting (e.g. `--model 'two words'`) and escaping without variable expansion, substitutions or script evaluation; unmatched quotes refuse the launch visibly. With an empty prefix, arguments still precede Guild's flags in a direct harness launch; both fields empty preserve the original direct launch. Settings values are `launch_prefixes` and `launch_arguments`, each with `claude`, `codex` and `opencode` keys; older settings remain valid.

The permission selector applies only to Claude Code. Codex uses its native `/permissions` menu and configuration; OpenCode uses its own configuration or explicit launch arguments. Guild does not translate Claude permission choices into Codex sandbox flags or OpenCode automatic approvals.

A nonempty prefix loads your interactive login shell (zsh/bash/sh) so shell functions work; other shells, including fish, visibly refuse a nonempty prefix. Guild restores cwd, PATH, hooks, provenance and configuration/history directories after startup, preserves literal prompts and flags, and avoids repeated alias/function wrapping. Restoration values stay out of argv. Changes affect future new, restored, resumed and managed terminal sessions; running sessions stay intact. Missing or broken wrappers never fall back to an unwrapped launch. Background reviews, usage and version probes do not use these preferences. Guild does not edit shell configuration or copy proxy credentials.

The pre-commit check is `just check`: formatting, clippy with no warnings, crate boundaries, tests — in that order, because `fmt` changes the files `clippy` then reads. `just test-live` keeps opt-in checks for existing local transcripts and isolated resume persistence; it launches no provider.

Everything else is `just --list`. The toolchain comes from `rust-toolchain.toml` and is installed through rustup.

Native review is removed; external revust uses the generic Terminal Jobs and
archive viewer APIs. Existing native review archives and settings remain untouched.

## License

Guild is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Vendored assets — fonts, icons, grammars and their queries — keep their own licences, which sit beside them with a `PROVENANCE` where one is needed.
