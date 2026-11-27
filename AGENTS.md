# Guild

An ADE where AI agents do the work: the window a person works in all day — a file tree, an editor, diffs, git, and Claude sessions in tabs.

- **Decisions:** `docs/decisions.md` — the rules in force, under the numbers code cites as "decision N"
- **Design system:** `docs/design-system.md` — tokens, anatomy and measures; `theme/` and its tests read it
- **Traps:** `docs/traps.md` — read before working in the area a trap names
- **Penpot:** `docs/penpot.md` — where the window is designed, and the process for a change a person would see
- **Tasks:** `tasks/README.md` — one file per task, holding its agreed behaviour and, when it needs one, its design and plan

Documentation, comments and commit messages are in English.

---

## How the desktop crate is laid out

The split is by responsibility: what the window does, what it draws, and the
pure models beside them.

```
crates/desktop/
  src/             a module's directory holds its children and its tests; they are
                   named here only where they are a subject of their own
    main.rs        the executable: logging, the panic hook, one window
    lib.rs         Root, Tab, TabId, Body, Kind — the crate's vocabulary
    stage.rs       what is in front of each seat: its areas, the one with the keys, the tab
                   in front of each, where a new tab lands and where an arrow goes — pure,
                   in fronts.rs's manner
    fronts.rs      which tab was in front of each seat, a project's or a worktree's — pure
    root/          what the window DOES, by subject
      tabs projects sessions settings menu body panels recent render
      files tree watch git diff changes log annotate docks run fonts lsp ide search
      branches history recent_files library problems push status column command
      declarations dependencies xcode
      appearance customizer  the palette in effect — mode, macOS appearance, tints —
                   and the one path that hands it to everything; the customizer's card
      areas        where a tab lands and which area has the keys: the window asking
                   stage.rs, and the one `Root::adopt_panes` every tab is made by
      availability which agent CLIs the login PATH has: one shared answer, unknown
                   until the first probe, refreshed on the clock and asked again
                   at a press whose default nobody chose
      picker everywhere file_index symbols find_in_files preview speed_search
                   the one popup slot and what fills it: Search Everywhere,
                   the file and symbol indexes on threads, Find in Files, the
                   preview's asking; a list's speed search
      usages       Find Usages: `alt-f7`, the Find tool's searches, the balloon
      start        the start screen's focus and keys, its history read on a thread
      quit links satellites  a quit from outside asking `cmd-q`'s questions; a link
                   clicked in a terminal, opened; the windows beside the main one
      update usage  the window's halves of Update Guild — where a build comes from
                   is update/sources — and of the plans' limits
      forge        a worktree's merge request asked: a thread per refresh, owned
                   until it ends and waited for at the quit, the proxy from
                   `crate::network`
      workspaces repository_settings  switching a workspace's arrangement without
                   restarting its roots; scoped preferences and repository identities
      clone_project  the one clone the window owns, apart from the projects' workers
      transfers tree_drag  the Explorer's file operations and drags, each owned by
                   the project it started in
      tab_launcher  the + menu: momentary, never takes the focus or binds a key
      terminal_panel terminal_visibility  the Terminal tool's dock shells, apart from
                   Run and the jobs; which retained terminals are shown
      terminal_jobs agent_submit  caller-checked terminal jobs, and the one path text
                   takes into an agent's terminal, from a gesture or from `guild`
      hunks        a hunk write counted until its status lands, and a commit held for it
      harness_retry resume_handoff  a failed runtime replaced when asked; a conversation
                   continued in a managed session — no blind input, no replay
      workflow/    the one serialized workflow queue: its store — runs, assignments,
                   reports, recovery, revust review admission — and its native
                   effects, disk IO never on the window's thread
      process      the Process page's inspection of workflow snapshots
      overlay isolation  a generic overlay's presentation; the diagnostic policy,
                   explicit and never read off the environment
    view/          what the window DRAWS
      tabs titlebar sessions notices body kind
      panels status recent tooltip tree diff changes log log_filters annotate console drag menu run fonts search column
      branches dock_menu goto_line problems recent_files tab_list
      areas panes  the areas as cards of their own, six apart; a tab's panes under a
                   hairline, the focused one outlined
      popup agents new_worktree  a menu's card and rows; the agents list; the + field
      region       a region drawn as a cached view of its own — the pinned column
                   and the three docks' panels — reused until the root notifies;
                   the title bar, the strip and the status bar stay inline:
                   traps.md says why
      column/spacers  the column's items far from its viewport, drawn as spacers of
                   their last height while nothing that decides it changed
      servers      the list of the project's language servers the status bar opens
      release_notes  what an update brings: the card over Update, and What's new
      usage        the title bar's usage chip and its card
      forge        a merge request's glyph and badge in a row, and its path-to-merge card
      satellite push_dialog diff_window  a second window's frame; Push Commits; the diff window
      picker preview everywhere find_in_files usages speed_search  the picker card
                   every popup is drawn in, the code under its list, Search
                   Everywhere's and Find in Files' cards, the Find tool, speed search
      start        the start screen, drawn
      clone_project workspaces worktree_removal  the URL clone form; a workspace's
                   name field and icon picker; the one Remove Worktree sheet
      terminal_panel terminal_jobs  the Terminal tool's shells; hidden jobs recovered
      process      the Process page, drawn from the workflow's graph
      activity controls overlay overlay_viewport resize  activity marks animated only
                   while seen; the navigation controls' states; the overlay host
                   and its measured bounds; a resize that outlives its hitbox
      tree_drag    the Explorer's drag sources and row targets
      menu/        one module per surface: tree, tabs, changes, log, column, gutter,
                   terminal, workspace — and the engine's other menus: agents,
                   dock, history, log_filter, open_in, strip
      settings/    sections, the keymap editor, and catalog choices action_bar
                   network open_in scopes updates
    tree.rs        the file tree as a model: lazy, .gitignore-aware, pure
    highlight.rs   the language table, the capture table, line endings, text or not
    grammars.rs    the grammars, and nothing about who is asking
    syntax.rs      the spans of a text, by line: what we paint with, injections and all
    editor.rs      gpui-component's editor wired in: grammars, theme, unbound keys
    editor/        occurrences, and reveal: centred jumps through the public layout API
    file_tab.rs    a file in a tab: read, dirt, save, reload, and the record of a save
    file_tab/      read off the window's thread, blame, git_markers and their popup,
                   the gutter, images, and launching what the file declares
    gutter.rs file_scroll.rs  the editor's gutter hit-tested from its layout; a file's
                   scroll gestures filtered
    markdown.rs    Markdown's images, through GPUI's public image cache
    markdown/      pane: a file tab's buffer shown as a document; cached: the parsed
                   view kept; inline_image, html_image, remote
    diff.rs        the hunks between two texts, as rows — pure, on similar
    diff/          document: a read-only comparison mapped back to its sources
    pick.rs        moving a selection with the arrows: one function, two lists
    diff_tab.rs    a diff in a tab: the two texts by its base, the buffer or the disk, the rows
    diff_tab/      compute: one owner of the work per diff; authors; interaction; hunks:
                   what each shown block offers to stage, and the write a click makes
    fonts.rs       the faces that ship, and the list a face is chosen from
    lsp.rs         the framing, the three shapes of message, and a URI
    lsp/client.rs  a server in a process: one owned, bounded connection, a group to kill
    lsp/providers.rs the component's five slots, and the counting the protocol asks for
    lsp/progress.rs what a server says it is doing, by token, and why one is not running — pure
    run.rs         a command as a line, its four states, and how long it took — pure
    run/discover.rs what a project declares it can run: a runner is a row
    run/source.rs run/tabs.rs  source declarations shared with the editor; a run's
                   identity apart from its tab
    annotate_tab.rs a file annotated in a tab: the runs, and a click that reaches the commit
    autosave.rs    when a buffer is written without cmd-s: the truth table, pure
    agents.rs      which tabs hold an agent, grouped by kind under one total, and what a
                   command line running Claude Code looks like — pure
    binding.rs     which session a tab holds when its program continues another's
                   conversation — pure
    closing.rs     what closing a tab would interrupt, and the one sheet that asks
    changes.rs     the Changes panel's model: git's three groups from the status map, pure
    column.rs      the column as a model: a group per repository, its rows, what a row
                   is called, where a new worktree goes and whether one may be removed
    git.rs         what git says and the twenty-one it is told: run, status, a blob,
                   the log, a scope, a write
    git/status.rs  porcelain v2 parsed; the map with a mark for every row above a change
    git/worktrees.rs what `git worktree list --porcelain -z` says, parsed; which are shown;
                   what git ignores in one, which a removal deletes without a word
    git/worktree_removal.rs the sheet's confirmed identity, then two guarded writes
    git/hunk.rs    Stage Hunk and Unstage Hunk: the facts a file's hunks need, the checks
                   before the write, and git's compare-and-swap over the index entry
    git/log.rs     the marked log format and --name-status parsed
    git/blame.rs   porcelain blame parsed into runs
    git/branches.rs the local branches with the current one marked, parsed
    git/stashes.rs the stash list in a marked format, parsed
    git/branch_update.rs a source chosen by a read; the update a bounded fetch, then a merge
                   pinned to what was fetched
    git/clone.rs   a clone in a process group, into private staging, published at the end
    git/inspection.rs Push's selected commits as one set of changes, oldest to newest
    git/read_budget.rs headroom for state writes while every project's git worker wakes
    branch_list.rs what the branch popup shows for what has been typed — pure
    log_layout.rs  three columns or a stack: one comparison with the design system's width
    log_graph.rs   a loaded history's ancestry, laid out once, apart from painting
    line_changes.rs the HEAD-to-buffer line markers, bounded, apart from drawing and git
    hunk.rs        which index lines staging one shown block replaces, and with what — pure
    files_tree.rs  a commit's flat file list as a tree, single-child chains collapsed
    console.rs     what the window told git, and what git said back
    declaration.rs what a Kotlin or Java file declares, by its grammar — pure
    jars.rs        `f12` into a Kotlin or Java dependency: a `jar:` URI, the source beside it
    sourcekit.rs   where sourcekit-lsp's answers outside a project land — an
                   interface, a framework's header — and what binds an Xcode
                   project for it; the running is root/xcode.rs
    deps.rs        what a project depends on: a stack is a row, a reader per stack's answer
    deps/ask.rs    asking a build system: the commands, the login PATH, a deadline, the kept answer
    deps/go_vendor.rs Go's validated vendor list, read; the mode is Go's to choose
    update.rs      a bundle's commit, source and build time; how far main is past it;
                   which staged build is offered — only a nightly — what
                   it brings, and the greeting after a restart — pure
    update/swap.rs the Update click's copy of the staged bundle and its helper
    update/        remote: release discovery, its cache and downloads, through gh;
                   runtime: the one updater worker; control, selection, preparation,
                   recovery, helper — the selectable sources' moving parts
    usage.rs       a plan's limits: the Keychain item's token, oauth/usage's windows,
                   a level, a reset said, the agent in front — pure; the asking
                   is root/usage.rs
    usage/codex.rs Codex's: its answer read, and `codex app-server` asked
    forge.rs       when a worktree's merge request is asked and what a reply may
                   change — pure; the asking is root/forge.rs, the reading
                   guild_session::merge_requests
    open_in.rs     what the title bar's Open In offers: sixteen apps, the installed rows
    windows.rs     the windows beside the main one: which kind is open, and when the
                   application quits
    split.rs       where `Split` is read from: the fraction is the session crate's
    push_dialog.rs Push Commits: what it offers, which commit it shows, a refusal said short
    diff_window.rs where a step through a commit's files lands — pure
    ide.rs         Guild as claude's IDE: the lock file, the tools, the two words
    ide/server.rs  the socket claude connects to, and the threads behind it
    hooks.rs       the hook a terminal session is given: the script, its settings, the socket's path
    hooks/server.rs the socket the hooks post to, and `guild` asks on: a request parsed, a channel, a sweep
    hooks/identity.rs what a Codex hook proves durably, apart from the socket
    discovery.rs   the CLI discovery a terminal launch is given, prepared and verified per launch
    search.rs      what a search of a project is: a query, a walk, a hit — pure
    search_tree.rs a search's hits as a tree of files, with what is folded
    document_search.rs local_search.rs project_search.rs  matching over snapshots;
                   a document's own Find bar; whole documents for a project's replace
    source_occurrences.rs source_snippet.rs  an identifier's matches, shared by
                   editors and read-only views; source text that leaves the focus with its host
    everywhere.rs  Search Everywhere's tabs and their order — pure
    file_index.rs  Go to File's index: what a search walks, and the ignored files beside it
    symbols.rs     Go to Symbol's index: tree-sitter's tags, and a server's symbols over them
    find_in_files.rs Find in Files without a window: the toggles' strokes, the history, where it looks
    usages.rs      Find Usages: what can be searched for, and what a server's answer becomes
    picker.rs fuzzy.rs preview.rs  every popup's list: one picker, one fuzzy match, the
                   code under a lit row painted by syntax.rs
    speed_search.rs a match over a host's expanded rows: no filtering, no I/O
    double_tap.rs  a double Shift, detected rather than bound
    start.rs       the start screen as a model: its rows, its highlight, what a key does
    customizer.rs  the theme customizer as a model: both palettes' drafts, Save, Reset,
                   Copy from — pure; the card is root/customizer and view/settings/customizer
    file_operations.rs file_selection.rs  the Explorer's copies and moves, by keyboard,
                   clipboard or drag; the selection file lists share
    clone_project.rs a clone's URL and destination, checked when submitted, never in paint
    repository_settings.rs workspaces.rs  a local repository's identity; which
                   workspace a command means — pure, the window's and `guild`'s
    terminal_job.rs terminal_panel.rs  the terminal jobs the window owns, with their
                   sanitized receipts; the Terminal tool's stable tabs
    review_activity.rs process_view.rs  a review's evidence, read-only and bounded;
                   the Process page's projection, read-only
    network.rs     the proxy, applied only where a launch asks for it — git, a pty, HTTP
    transport.rs   byte and item permits, and Unix IO a stop can interrupt
    frame.rs panel_corners.rs observer.rs swipe.rs  the footer line and the room a
                   window keeps; the areas' frame geometry; an overlay's geometry; a
                   two-finger swipe classified
    watch.rs       the disk watched: batches of the directories touched
    terminal/      our own terminal on alacritty_terminal
      emulator font keys mouse paint palette scroll shell
      select find find_worker links reply resize sprite xtversion program_status
      facts geometry
      refresh failure ownership launch_prefix shell_integration shell_state
      semantic semantic_forward
                   what it selects and finds, what it links, answers and draws
                   itself; demand, failures, cleanup, wrappers, and the shell's metadata
      emulator/    events and job: wakeups coalesced, a job's leader kept until cleanup
      process_generation.c  a process's kernel metadata, zombies included; build.rs compiles it
      view/        mod, drawing, input, pointer, hover, find_bar, platform_input,
                   prepared_rows, restore, adaptive, scheduling, commands
    i18n/          mod, english, russian — a language is a file; commit and branch_update
                   words formatted when shown
    theme/         mod, palettes — Light and Dark; custom: a person's tint over them, tint:
                   its OKLab arithmetic; card_outline diff_center push settings
    actions/       mod, keymap, labels — and how a stroke is printed
    icons/         mod: which icon a file gets; map and embed are generated
    edit.rs        what a key does to a line, wherever Guild takes text
    text_field.rs  the single-line field: the component's text, caret and IME, the host's keys
    blink.rs       one caret phase, shared so two cannot blink out of step
    system.rs      what the platform does that GPUI does not expose
    system/        browser close_prompt window_probe terminal_power terminal_deadline
                   update_processes open_files — the platform's other doors; the
                   open-file limit main.rs raises at the start
    closed.rs notify.rs paths.rs scale.rs title.rs logging.rs menu.rs
    native_preview.rs native_preview/ notice_preview.rs  development-only fixtures behind
                   the native-preview and notice-preview features, for examples/
  tests/all/       what a caller can reach, one binary, one module per topic
  tests/fixtures/  what they read: git's output, fake language servers, images, Markdown
  examples/        isolated native trials and probes over the production Root, most
                   behind the native-preview features
  build.rs         the vendored grammars and the terminal's C helper, compiled with cc
  vendor/          grammars whose crates pin an incompatible tree-sitter
  assets/queries/  queries of grammars that ship a parser without one, and of
                   two that ship no query at all — each with the licence of
                   whoever wrote it and a PROVENANCE
    icons/         the Material Icon Theme's set
    ui/ component/ Lucide's icons: ours, and the ones gpui-component asks for
    fonts/         Inter, JetBrains Mono and Lilex, vendored, with their licences
    nodes/         IntelliJ's declaration, library and JDK icons, Apache-2.0, with a PROVENANCE
    gradle/        guild-deps.gradle, the init script Gradle is asked through
    hooks/         guild-hook.sh, what a terminal session's hooks run: one POST; the
                   status line, the usage post, OpenCode's plugin, CLI discovery's,
                   and the branch-delete confirmation
    shell/         the shell integration scripts for zsh, bash and fish
```

**`guild-desktop` is a library with a thin binary.** Not a preference: a
binary crate has no `tests/` directory that works, because Cargo builds an
integration test against a library. Every test living inside its module was
that constraint, not a choice.

**A test has two homes and they mean different things.** `#[cfg(test)] mod
tests` inside a module is for what only that module can see; `tests/*.rs` is
for behaviour a caller can reach, and out there a test can only use what is
public — which makes it a check on the public surface too. A test that
reads a private table stays in its module with a note saying why: the crate
promises the function that wraps the table, not the table.

**The integration tests are one binary.** Cargo builds every `tests/*.rs`
as its own crate and links it separately, and every link pulls the whole
graph with gpui in it, so the files are modules of `tests/all/main.rs`. Each
file is still a topic and still reaches the crate only through its public
door. A new test file is added to `tests/all/` **and declared in
`main.rs`**, or it does not run — and nothing will say so.

**`include_bytes!` resolves against the file it is written in**, so a
fixture is `../fixtures/…` from inside `tests/all/`.

**A child module sees its parent's private fields; a sibling does not.**
That is why `terminal/view/drawing.rs` is a child rather than a neighbour —
the alternative was opening up a type for
the sake of the file layout, which is the wrong way round.

**Beside the desktop, two crates, and neither links GPUI.**
`guild-session` is what can be checked without a window — sessions,
transcripts, the status board, the workspace and settings; the harnesses,
`guild_session::harness` (which agent CLI a tab runs and what it
can do) and `history` (what each one remembers of a folder); the division of a surface, `guild_session::layout` (one tree for both), `areas` (a
seat's areas, the last one stays), `panes` (a tab's panes, each showing one body
or nothing) and `split` (a fraction that is dragged); and
the vocabulary the window and the command line share,
`guild_session::command`: `Command`, `Answer`, the provenance rule and
the sentence every session is told. It also holds
`guild_session::changelog`, `CHANGELOG.md` as the window reads it and
the release writes it. What the window and the command line both need
past that is here too, without a window or a process of its own:
`process` (the process groups), `terminal_job` (the jobs' protocol and
lifecycle), `workflow` (the portable development workflow and its revust
reviews), `update` (the update's vocabulary and persistence),
`merge_requests` (a merge request's provider-neutral status and the glab
reader, decision 56), and the
network, repository and resume-handoff settings. `crates/cli` is `guild`
— `main.rs`, with `terminal.rs` and `workflow.rs` beside it, doors to the
window and never local work — and tests that run the binary against a
socket playing the window; and `guild-release`, `release.rs` with
`update_release.rs`, which `just release` asks to name a nightly and cut
the changelog, and to write, sign, verify and stage an update's manifest
and bundle — none of it a CI run or a publication — and which no session
calls. **The window's end of a command
is `root/command.rs`**, and it calls what the column, the tabs and the
Commit panel already call.

The workflow's reviews run in revust, an external review tool: the code
qualifies the installed package and starts each review as an ordinary
terminal job through the generic Terminal Jobs API, whose `archive_viewer`
opens what the review saved. Existing native review archives and settings are
kept as they are; nothing initialises or migrates them.
`guild-session::process::ProcessGroup` owns synchronous children used by LSP,
usage, version probing and dependency discovery. Its leader is not reaped until
its group has been stopped. See `docs/runtime-ownership.md`.

---

## How work is done

Small, settled fixes need the request, the expected result, the change and
targeted checks — not a separate specification or plan. A substantial or
ambiguous change records its agreed behaviour, scenarios, design decisions
and one implementation plan in its own `tasks/GLD-*.md` — Acceptance,
Design and Plan, as `tasks/README.md` says. Decision evidence is
`docs/decisions.md`: a rule that outlives the task goes there, and the
plan's last step records it there or says none arose. Architecture-only work
stops at a reviewed design.

Work follows the task file's plan in dependency order, each step to green
tests and a commit; a disagreement with the plan is recorded in the plan.

- **Test first, implementation second.** The test pins down an acceptance criterion from the request or the plan; first make sure it fails for the right reason, not because of a typo.
- **A task is closed by a commit.** The body explains why it was done this way, when the decision is not obvious.
- **A disagreement with the plan is recorded in the plan.** If the plan turns out to be wrong, fix the plan — not the code silently.
- **Report what actually happened.** What was checked and with what; if something was not checked, say so.
- **A change a person would notice adds its line to `CHANGELOG.md`, under `## [Unreleased]`, in its own branch** — before it is merged, so a merge carries its line and a release is only a rename (decision 45). The form is keepachangelog 1.1.0 in the part `guild_session::changelog` reads: `### Added`, `Changed`, `Deprecated`, `Removed`, `Fixed` or `Security`, and one `- ` line per change, written for the person reading the card over Update — what they will see, not which module moved. A refactor, a test, a document: no line.
- **A change a person would see in the window is designed in Penpot first.** It is drawn as a proposal from the Library's components and tokens, and the user approves the Penpot design before any code is written; the same branch's work ends with Penpot brought to what was merged — the proposal promoted into the Library and Screens, stale copies relinked, the token script's output checked against both files. `docs/penpot.md` says where the files are and how. The same line as the changelog's: a refactor, a test, a document — nothing to design.

---

## Where Guild takes text

**Every Guild-owned single-line field uses `desktop::text_field::TextField`,
backed by the existing `gpui-component::InputState`.** A real selection and caret,
Cmd+A/C/X/V, undo/redo, arrow/word/line movement, mouse selection, IME and
horizontal scrolling belong to the input component. Do not draw a text label
and a synthetic caret or append raw keystrokes to a String to make a field.

Hosts retain their Enter/Escape/Tab and list-navigation behavior. The adapter
keeps `desktop::edit`'s readline word/line deletion and single-line clipboard
flattening. Changing a model value is distinct from typing: equal values must
not reset selection or undo history, and a programmatic suggestion must not
mark a clone destination as manually edited. Handle input focus without
redirecting ordinary text into the panel or terminal behind it.

The file editor and multiline commit/review fields already use `InputState`
and retain their existing editing rules. The commit box uses Cmd+Enter to
commit and Enter for a newline. The terminal forwards keys to its child program;
its Find field, however, is a Guild-owned field and uses the shared adapter.

---

## Boundaries that are easy to break unnoticed

The compiler does not check them. `just boundaries` guards some; the rest is only caught by eyes at review.

- `guild-session` **has no GPUI.** This is a testability requirement: transcript parsing and session management must be checkable without an open window.
- The `guild-desktop` tree contains **neither `bezel-gpui` nor `gpui-terminal`.** The first is a GPUI fork — two incompatible versions in one binary. The second loses the `DIM` flag agent CLIs draw their hints with; the terminal is ours.
- **A pane is a view onto *something*, and `Body` is where that something is named.** A tab is one or more panes, and the body belongs to the pane, not the tab: an empty pane is the absence of a `Body` (it draws the start screen), never a variant of it. `Tab::session()` answers for the focused pane and `Tab::sessions()` for all of them; `Tab.id` is a `TabId`, never a `SessionId`, and the two variants that show a session carry its identity themselves — `Body::session()` answers `None` for the ones that do not. Reaching for a tab's session without asking which pane — or giving a non-session body a `SessionId` "just for the id" — puts back the assumption decision 15 removed. **Every tab is made by `Root::adopt_panes`** (`root/areas.rs`), which holds the one `Tab` literal and asks `Stage::landing` for its area; `Root::adopt` is that call with a single pane. A path that builds a `Tab` by hand, bypassing `adopt_panes`, skips the rule that a tab is in exactly one area. A new kind of body is still a pure addition.
- **Everything that touches AppKit lives in `desktop::system`.** `objc` implements native services GPUI does not expose: system gesture/preferences, Dock badges and sounds, Trash, application discovery and quit coordination. `raw-window-handle` reaches the native window for sheet detection, deminiaturization and main-window content minimums. Pointers are checked before use; native full-screen title-bar reveal is left to AppKit. Reaching for these APIs elsewhere spreads unsafe platform code through the window. **It is `system` and not `titlebar` on purpose:** the module that draws Guild's title bar is `desktop::view::titlebar`.
- **The grammar table is one table, and the capture table is one table.** `grammars.rs` knows nothing about who is asking: the editor component registers from it and `syntax.rs` parses from it. `highlight::CAPTURES` is where a capture becomes a colour, and both the component's theme and our own spans are built from it, through `Theme::of_paint`. Two of each is how one file comes to look different in a tab and in a diff.
- **Git is written by named actions and read by everything else.** Every `git` command the desktop **reads** with goes through `git::run`; the ones that write go through `git::write`, which builds its own command because it keeps what git said on both streams and `run` does not. The writes are the twenty-one variants of `git::Write` — `add`, `restore --staged`, `commit -F`, `switch`, `switch -c`, `branch -d`, `stash apply`, `stash drop`, `restore --source=HEAD --staged --worktree` (Rollback), `push`, `worktree add -b`, `worktree remove`, `worktree unlock`, `WorktreeRemoveConfirmed` (the sheet's confirmed folder, branch and commit: `worktree remove --`, then, if asked, the same guarded `branch -d`), `clone` through its private staging gateway, the bounded `FetchSource` (`fetch --no-tags --no-prune --no-recurse-submodules --refmap=`, so no configured refspec moves another ref; auto fast-forward's adds `fetch.showForcedUpdates=true`) and identity-pinned `MergeSource` (`merge --no-edit --no-autostash`), auto fast-forward's `FastForwardBranch` (`fetch . <oid>:refs/heads/<branch>`, no `+`) and `FastForwardCheckout` (`merge --ff-only --no-squash --commit --no-overwrite-ignore --strategy=ort` with the branch's `mergeOptions` emptied), and `StageHunk` and `UnstageHunk` (`apply --cached --no-3way --whitespace=nowarn -` of a binary patch with a full index line, so git swaps the entry only over the one it was computed from — decision 55) — whose verbs are spelled in `Write::args` and nowhere else; a test pins the list, and the only other occurrence is the worker's own test making its fixture commit. **The count is not the rule; the guards are** — decision 21. A new write is a decision there, not one more arm, and `restore` appearing **twice** in that list is two writes rather than one written twice: `Unstage` takes the index back and `Rollback` takes the disk with it. **The destructive spellings are spelled nowhere:** `-D` deletes an unmerged branch, `--force` overwrites one that exists, `pop` drops what it applied, `clear` empties the stash, `--hard` throws the worktree away — a test greps for all of them **by substring rather than by whole word**, which is the only form that bars `--force-with-lease`; its only allowances are exact words — the two stdin options and `--ff-only` — and no argv word may begin with `+`, a forced refspec. Where git can refuse, its refusal *is* the guard: the window works nothing out about what is merged, git does, and its words become the notice. **Where git will not argue, the window must** — decision 30: a rollback asks first in a sheet naming the files, because git restores a worktree without a word and there is nothing behind it to undo with. None of it runs on the window's thread: a write goes to the project's git thread and is followed by a status walk, and what git answers comes back on the watch loop's clock like everything else git says.
- **A row in `CAPTURES` must be a name the component can reach, and a capture it cannot reach belongs in `RENAMED`.** The table feeds two engines and only ours reads it as a table: the component's is `SyntaxColors::style`, a `match` over forty fixed names ending in `_ => None`, deserialised with no `deny_unknown_fields` — so a key outside those forty is dropped without a word. A row for `field` would be a colour in the diff and no colour in the tab, which is the drift the one-table rule exists to prevent, arriving through the table itself. `every_row_of_the_table_is_a_name_the_component_can_reach` is the guard, and the cure for a name with no slot is to rewrite the *query text* as it is registered, which both engines then read. A row of `RENAMED` matches as a **prefix of the dots**, so `@module.builtin` follows `@module`.
- **The rules for an overlap are the component's two, not the one that reads like the rule.** `match_styles` keeps the **earlier** capture when two cover exactly the same bytes — nvim-treesitter's queries write `(comment) @comment @spell`, and `@spell` is an annotation and not a second colour — and only `unique_styles` after it lets a strictly-inner capture win. Copying only the second greys the comments of sql, scss, swift and graphql. And `flatten` paints bytes in order rather than asking each piece who covers it: the literal form is quadratic in the captures, on a path rebuilt on every keystroke.
- **`syntax.rs` follows injections, and only a static `#set! injection.language`.** HTML's own highlights query is thirteen lines; everything that makes a page look coloured is `<script>` handed to javascript and `<style>` to css, and the component follows those in a tab — so a module that did not would colour a page in a tab and grey in a diff, in Annotate and in a commit's files. A region is parsed **in place** with `set_included_ranges`, so its nodes carry offsets into the original text and there is no shift to get wrong. A `@injection.language` capture, which names the language with a word in the file — svelte's `lang="ts"`, php's heredoc tag — is **not** followed, because the component does not resolve one either. And a grammar reaches only what it declares in `injects`, the same list the component resolves against; a test insists every name on it is registered.
- **A `@` inside a string literal in a query is a token of the language, not a capture.** css matches its at-rules as `"@namespace" @keyword` and scss matches fourteen of them; `renamed_captures` skips string literals, because rewriting one leaves a query hunting for a token no stylesheet contains — and the query still compiles, and the file still looks coloured, and the at-rule is simply grey.
- **A vendored query travels with the licence of whoever wrote it, and that is not always the parser's.** `include_str!` is the guard that the `.scm` is there; a test walks `assets/queries/` for the `LICENSE` and the `PROVENANCE` beside it, which are the two files a person adding a grammar by hand leaves out. graphql's and hcl's queries are nvim-treesitter's, Apache-2.0, while the parser crates are MIT and Apache-2.0 separately — and `scripts/vendor-grammars.py` pins nvim-treesitter to a **commit**, because a branch is not a version.
- **Every icon of ours is one table per directory, and a test walks each directory against its table** — `icons::UI` for `assets/ui` (Lucide, ISC), `icons::COMPONENT` for `assets/component`, and `icons::NODES` for `assets/nodes` (IntelliJ's, Apache-2.0); one directory per licence keeps a licence with the files it covers. An icon file that nothing loads draws **nothing at all** — `AssetSource::load` answers `None` for a name it does not know, GPUI paints emptiness, and there is no error, no log line and no gap in the layout to notice.
- **The IDE protocol shows and answers; it never writes the person's files.** `openDiff` opens a tab and parks the call; `FILE_SAVED` closes it and **the CLI writes the file itself**, in its own turn, after its own permission question. A window that writes first makes that edit fail on a file that has changed since the agent read it, and the agent spends a long turn working out that its change is already there. The tool list is a promise for the same reason: `ide::tools()` names only what `root::ide::answer_tool` can actually do, because the agent plans around what it is told it has.
- **The hooks reach only what Guild starts, and never the person's settings.** `~/.claude/settings.json` is the person's: merging hooks into it reorders their file, and a changed hook table never reaches an install that exists. Guild hands `--settings <state_dir>/hooks/claude-settings.json` and `GUILD_HOOK_SOCKET` to the **terminal** sessions it launches and to nothing else, and that is the whole of the guard against a worker agent's hooks being read as its parent's: a nested `claude -p` inherits the variable but not the flag, and a payload naming no session of the window's is dropped. The script's text is `include_str!`'d and written out at every start, so a fix is made in `assets/hooks/guild-hook.sh` and never in the copy under `.config`, which the next start overwrites.
- **`guild` is a door, never a second way to do a thing.** A command the window answers calls the deed a person's gesture calls — `write_worktree` for `worktree create`, `open_tab_with` for `session new` — so decision 21's and 33's guards hold for an agent without being written twice, and nothing in `root/command.rs` spells a git verb or parses git's output. **The provenance rule is the window's, not the command's**: `guild` sends `GUILD_SESSION` as the caller and the window looks the record up, because a binary on somebody's `PATH` can be made to say anything. And `guild-cli` links `guild-session` and `serde_json` and nothing else, which `just boundaries` holds.
- **`gpui-component` draws its own icons from *our* asset source, under `icons/`.** It ships none and asks the application for `icons/<lucide-name>.svg`; `icons::COMPONENT` is that table and `assets/component/` is where the files live — a directory of their own, so the walk over `assets/ui` and the walk over this one are two tests that cannot be confused. **The component's names are answered before the file-type set**, because the Material Icon Theme is eleven hundred names from somebody else's repository and the day one of them is `close.svg` a file icon would be handed to a close button. Nothing of ours can see a rename upstream: that is a button silently going blank again, which is what this table exists to stop.
- **`crates/desktop/src/icons/map.rs` and `embed.rs` are generated by `scripts/vendor-icons.py`, never edited.** The set is the Material Icon Theme's published extension, pinned by version, with its licence beside it. A rule that is wrong is fixed in the script and regenerated; a fix made in the generated file is undone by the next vendoring, silently.
- **Interface numbers come from the design system.** Colour, size, spacing and radius come from `theme/`, which a test checks against the document. A colour literal in widget code is a reason to open the document, not to eyeball a shade.

- **GPUI Kit and gpui-component are unmodified registry dependencies.** The desktop pins Kit/component 0.7.1 and the matching `gpui-pre` 0.3.8 runtime; do not introduce a second GPUI runtime or patch a dependency to implement Guild behavior. Upstream clips composed editor highlights to visible ranges; `tests/all/editor_runs.rs` reproduces viewport-straddling diagnostics and a scrolled-off definition hover with Cyrillic text. `editor/reveal.rs` implements Guild's centered jumps through public layout/scroll APIs, tested before first layout, at document edges and after a superseding movement. `FileTab` measures gutter and range geometry from the editor instead of copying its private padding. See `docs/gpui-kit-migration.md` before raising these versions.

Three of them are guarded by `just boundaries` through `cargo tree`, and it is part of `just check`. Verified by breaking them: gpui slipped into `guild-session` fails the recipe with exit code 1 and prints the path, and so does `guild-desktop` added to `guild-cli`'s dependencies. **A guard whose subject no longer exists passes silently and looks alive**, so a guard goes with the crate it guards.

---

## Traps that have already cost us

**They are in `docs/traps.md`** — a hundred and more, each one a bug or an hour this repository has already paid for: the terminal, focus and key dispatch, popups and hitboxes, git, state files, language servers, driving the window from outside. **Read it before working on any of those**, and add a trap there, not here, when one is found.

---

## Commands

The entry point is `just`; the recipe list is `just --list`. The toolchain comes from `rust-toolchain.toml`.

```bash
just check                 # fmt → lint → boundaries → test
just test                  # cargo test --workspace -- --test-threads=4
just test-headless         # session and CLI on Unix CI, locked dependencies
just test-live             # opt-in local transcript and isolated resume tests
just test-crate guild-session
just lint                  # clippy --workspace --all-targets -- -D warnings
just boundaries            # boundaries the compiler cannot see
just build                 # cargo check --workspace
just fmt / just fmt-check

just desktop               # the Guild window
just desktop-release       # the same, optimised — eight times faster on output
just bundle                # assemble Guild.app with the icon and version
just app                   # bundle and open it
just release               # main only: check, cut Unreleased into nightly-<date>, commit, tag,
                           # bundle; staged for Guild's Update button while it runs, swapped
                           # into /Applications otherwise; the push is printed, never run
just install               # a dev build: check, tag dogfood-<date>, bundle, swap into
                           # /Applications; refused while the installed Guild runs
just desktop-dev <dir>     # the build being developed, its state apart — docs/desktop-dev.md
just site / just site-serve # the landing page in site/, Hugo + Hextra — docs/site.md
just toolchain             # what actually builds this
```

**`test-live` is not part of `check`.** Its transcript test reads existing local Claude sessions, while ordinary checks use isolated fixtures. It launches no provider.

Recipes are deliberately thin: `just` composes multi-step commands rather than renaming one-liners. If you need bare `cargo`, it is exactly what the recipe says.

**`just quick` is the loop between tasks**, `just check` before a commit —
the two differ by the boundaries guard.

**`just build-all` and `just build-desktop`** draw a status row per crate
while cargo works. One cargo, not one per crate: cargo locks the target
directory, so a process per row would queue and the table would show a
parallelism that is not happening.

**Do not reorder `check`.** `fmt` changes the files that `clippy` then reads; the other order gives a false "clean".

**The toolchain is installed only through rustup.** Homebrew's `rust` conflicts: `/opt/homebrew/bin` comes before `~/.cargo/bin` in `PATH`, and brew's `cargo` is not rustup-aware and ignores `rust-toolchain.toml` — the build silently uses a different version. The check is `just toolchain`; it must print `overridden by '<repository>/rust-toolchain.toml'`.

`just check` includes `just boundaries` — a check of the boundaries the compiler cannot see (listed above), and it holds **three**: no gpui in `guild-session` or `guild-cli`, and no GPUI fork in the `guild-desktop` tree. It looks at `cargo tree`, not just `grep` over the manifest: put a crate behind something the desktop already depends on and it arrives transitively while the desktop's manifest does not change by a single line. Verified both ways: in that situation `grep` stays silent and `cargo tree` prints the path.

---

## Claude Code settings in this repository

**There is no `CLAUDE.md`, and that is deliberate.** Claude Code reads
`AGENTS.md` when a project has no `CLAUDE.md` of its own, so this file is
the one set of rules every harness reads.

`.claude/settings.json` (in git) holds two things:

- **An allow-list** for `cargo`, safe `just` recipes and read-only `git` commands. It names only commands that exist: a dead rule in an allow-list is one nobody re-reads before the day something makes it live again. The point is to drop confirmation for commands that run dozens of times per session. Writing commands are deliberately absent: `git push`, `git reset` and `cargo add` are confirmed by hand. The `just` recipes are listed **by name rather than as `just:*`**: a blanket rule would be a hole — a recipe is edited in the repository, and tomorrow anything could be behind the same permission. A new recipe requires confirmation by default until it is added to the list deliberately.
- **A `PostToolUse` hook** on `Write|Edit`: it runs `rustfmt --edition 2024` over every changed `.rs`. Files with other extensions are skipped.

Personal overrides go into `.claude/settings.local.json`, which is in `.gitignore`.
