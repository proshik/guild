# Decisions

The decisions still in force for Guild, under the numbers code and commits cite.
Each is the rule and the reason that keeps it from being broken. A number whose
decision no longer applies keeps a one-line stub, so a comment that cites it
still finds something.

Code comments also cite decisions **of a milestone's plan** — "M8.9, decision 6",
"decision 1 of M12's plan". Those are local to that piece of work, not entries
here; the comment beside the citation carries the rule it names.

1. *Retired* — the name's rationale; it described a task tracker that is not part of this repository.
2. *Retired* — the tracker's positioning.
3. *Retired* — the order of milestones across a server and a desktop.
4. *Retired* — the server's layering.
5. *Retired* — review as a task's lifecycle stage in the tracker's data model.
6. *Retired* — the tracker's task schema.
7. *Retired* — no message broker for the server.
8. *Retired* — users inside the server's task service.
9. *Retired* — runners connecting outbound to a control plane.
10. *Retired* — credentials held by the runner, not a control plane.

11. **The interface is GPUI with `gpui-component`.** The window is a code
    editor's workload — a file tree, highlighting, diffs — and GPUI was built for
    exactly that load; `gpui-component` gives an editor with LSP slots and
    virtual lists. GPUI is pre-1.0, so versions are pinned exactly, there is one
    GPUI line in the binary (`just boundaries` holds it), and an upgrade is planned
    work, not a bump.

12. *Retired* — `stream-json` sessions; interactive sessions are terminal-only, and the provider event parser remains for reviews.
13. *Retired* — the permission rules of stream sessions.
14. *Retired* — the removal of two server crates.

15. **A tab is not a session.** A tab has its own `TabId`; what it shows is a
    `Body` in each of its panes, and only the bodies that show a session carry a
    `SessionId`. Asking a tab for its session is a question with an honest `None`,
    two views of one session are representable, and a new kind of tab is a new
    variant rather than a change to every caller. Open tabs are part of a seat's
    state and come back after a restart; a file whose path is gone is dropped
    and logged.

16. *Retired* — the order of a server track and a desktop track.

17. **State is written on a thread of its own.** Every save stays atomic and
    synced — write beside, `sync_all`, rename, sync the directory, keep the
    previous copy — but goes through one queue on one thread, in order, the
    latest snapshot of a path winning; a load answers from the queue; the queue
    is flushed before quitting; a failed write is kept for the window to report.
    A synced write on the window's thread made every tab switch wait for the disk.

18. *Retired* — "three writes and no more"; replaced by 21, which keeps its two lasting rules.

19. **Tools live in docks, and their buttons in the status bar.** Explorer,
    Commit, Git, Run, Search and the rest are tools; each has a dock — left,
    right or bottom — and a dock shows one tool at a time. A tool moves by
    dragging its button or from the View menu. **The Commit panel takes the
    ticked model**: a change is ticked the moment it appears, the ticks say what the
    commit takes, and the index is brought to them only when the commit is made.
    A conflict is never ticked.

20. **The right button opens one menu per surface, and every item is a deed
    aimed at its target.** A menu belongs to a surface, not to a row: the row
    records what the pointer was on and the surface's builder reads it. An item
    acts on the thing under the pointer — a tab's *Close* closes that tab, not
    the one in front — and the keymap keeps actions of its own for what a stroke
    means. Items that write follow 30.

21. **Git is done as an environment does it, and the guards are the rule, not
    the count.** Every write is a named variant of `git::Write`, its verbs
    spelled in `Write::args` and nowhere else, and none runs on the window's
    thread: it goes to the project's git thread and is followed by a status walk.
    Every write is one of three kinds:
    - **what git itself refuses** needs nothing more — git's refusal, in git's
      words, is the notice;
    - **what can be walked back** names the way back where the gesture is;
    - **what cannot be walked back** asks first, in a sheet that says what will
      be lost, never on one click.
    No fourth kind is offered. The destructive spellings — `-D`, `--force`,
    `pop`, `clear`, `--hard` — are spelled nowhere, and a test greps every write
    for them by substring, which is what bars `--force-with-lease`. A new write
    is a decision here, not one more arm.

    - **Clone** (Get from Version Control) runs only through the window's clone
      gateway: its own process group, a private mode-0700 staging directory, an
      atomic no-replace rename to publish, and a 120-second silence deadline.
      An existing destination is never a cleanup target.
    - **Branch update** is `FetchSource` then `MergeSource`. A source choice is
      a per-worktree preference and never a checkout. The update is bound to its
      worktree, symbolic `HEAD`, commit and source; it refuses a dirty index,
      untracked files, an operation in progress or unsaved buffers; fetches one
      source without tags, pruning or submodules, and with `--refmap=` into its
      tracking ref alone, so a configured refspec moves no other ref; rechecks
      identity and
      cleanliness before merging the resolved commit. A failed or timed-out
      fetch never falls through to a merge; a conflict keeps git's state and
      names `git add` + `git commit` or `git merge --abort`. No force, rebase,
      automatic stash or reset.
    - **Auto fast-forward** keeps the default branch — the one new worktrees
      start from — current, and nothing else. When a repository's first root
      opens and 5 minutes after each run, while the setting is on, one run per
      repository fetches that branch's upstream with the branch update's
      bounded `FetchSource`, refusing a forced upstream
      (`fetch.showForcedUpdates=true` on the argv) and a symbolic tracking ref
      or default branch (git would write the ref it points at), off every
      root's git thread
      and never beside
      a manual Branch update. A failed or timed-out fetch writes nothing. The
      branch moves only as a fast-forward to the fetched commit: checked out
      nowhere, `FastForwardBranch` (`fetch . <oid>:refs/heads/<branch>`, no
      `+`, `fetch.showForcedUpdates=true` forced on the argv), which git
      refuses for a non-fast-forward or a checked-out branch; checked out,
      `FastForwardCheckout` (`merge --ff-only --no-squash --commit
      --no-overwrite-ignore --strategy=ort` with the branch's `mergeOptions` emptied) on that
      worktree's git thread, in the checkout's turn every write of Guild's in
      it takes (`git::turn`), after rechecking its branch and commit, a clean
      index, worktree and untracked set, no operation in progress, no unsaved
      buffers and no cancellation, and checked afterwards for HEAD at the
      commit on a clean checkout. The person's git config is not trusted to
      keep a fast-forward a fast-forward: the argv says so. Diverged, ahead or
      any refusal leaves it alone; the reason is shown where Branch update
      explains itself, and the writes are in the Console. No merge commit,
      squash, rebase, stash, reset or force. `--ff-only` is the one exact word
      the destructive-spelling guard allows besides the stdin options.
    - **Confirmed worktree removal** (`WorktreeRemoveConfirmed`) binds the
      sheet's repository, path, branch and commit, removes the worktree, then
      — only if asked — deletes the branch through the same `branch -d`. The
      primary checkout is never removable; `main`/`master` and detached
      contexts keep their branch. A branch failure after the folder is gone is
      reported as partial, never as success.
    - **Hunk staging** (`StageHunk`, `UnstageHunk`) is `apply --cached` of a
      binary patch with a full index line — the entry's new content, computed
      by the window, swapped in by git only over the entry it was computed
      from — decision 55.

22. **The Console records the writes and nothing else, and a persisted enum is
    a wire format.** The window reads git constantly; a console of reads is one
    nobody reads, so it shows each write with its directory, argv and both
    streams — stderr on success included. A variant written to disk keeps its
    name after it leaves the interface (`Tool::Output`, `Tool::Git` as `"log"`):
    an unknown variant makes serde fail the whole file, and the project loses its
    tabs and sessions.

23. **The editor's colours are the component's; ours reach everything else, from
    the same table.** The component paints a file in a tab and cannot be handed
    spans, so the diff, Annotate and a commit's files are painted by
    `desktop::syntax` from the same grammars, queries and capture table — one
    table, two engines, a test holding them together. A language is a row of
    data, and its grammar a separate field (`Cargo.lock` is TOML). An
    unregistered language name must never resolve silently to another grammar;
    a test checks every name round-trips.

24. **Run executes a line in a pty, and the exit code is the point.** The
    console is our terminal, so tools colour and redraw as they mean to; the
    line runs in the person's login shell, because a bundled app's `PATH` lacks
    Homebrew. The panel shows how the run ended, because a green-looking log with
    a failed exit is the failure it exists to prevent. Nothing the window starts
    outlives it: quitting stops every process group it owns. What a project can
    run is discovered as rows of data, never run by discovery.

25. **The editor speaks to language servers; the component draws what they
    say.** One server per project and language, a row of data, nothing
    downloaded; a command not on the `PATH` means no server, said quietly in the
    status bar. The server is told the buffer, not the disk, and `didSave` is
    sent because diagnostics depend on it; nothing is sent before `initialize`
    is answered. Diagnostics are dropped on every edit. Jumps into another file
    are ours. The hover slot stays empty: documentation opens on a stroke, not
    on a resting pointer.

26. **The faces ship with the window, and a face is chosen from a list.** The
    bundled faces are registered before the first frame. The terminal and code
    are separate settings because a prompt needs Nerd Font glyphs and a file does
    not. The chooser is a list drawn in the faces it offers, read once on open.

27. **The window is an IDE `claude` connects to, and the agent's edit arrives
    as a diff.** The tool list names only what the window can do. `openDiff` is
    parked until a person applies or refuses; a tab closed any other way is a
    refusal. **The window never writes the agent's file** — the CLI writes it in
    its own turn after its own question; writing first makes the agent's edit
    fail. The session is pointed at this window by its port, and stale locks of
    dead windows are swept at start.

28. **Search is ripgrep's libraries on a thread, and what it searches is what
    the tree shows.** Not `rg` on the `PATH` and not `git grep`, which would miss
    the file the agent just wrote. The walk streams, a new query stops the old
    one, and the list never blanks while it waits; limits are said when they
    stop a walk, and a long line is cut around its match. The query is state and
    comes back with the project; the hits are not saved. Replacing across the
    project goes through each file's buffer, never behind an open tab.

29. *Retired* — how one milestone was built in parallel branches.

30. **A menu item may write, and what guards it is a sheet.** Where git can
    refuse, its refusal is the guard; where it will not argue — a rollback, a
    worktree removal that deletes ignored files — the window asks first, in a
    sheet naming what it is about, one question at a time. A file is deleted by
    moving it to the Trash, never by `unlink`.

31. **The window pushes, and force is spelled nowhere.** `Push` is
    `push [--set-upstream] <remote> <refspec>` from names git printed, the pair
    spelled only where they differ. `Rollback` is
    `restore --source=HEAD --staged --worktree --`, never `checkout`; an
    unversioned row gets no rollback, an added one is unstaged instead.

32. **A tab says what its agent is doing only as the agent reports it, through
    OSC 7501.** Every terminal answers the Program Status Protocol's probe
    before the DA1 that follows it, and the session's status is its root
    record: `idle` rest, `working` working, `blocked` the question mark (with
    `permission`, `question` or `auth` only when reported), `done` finished,
    `error` failed, `clear` nothing. A program that reports nothing shows
    nothing — no mark, no unread, no sound — and `guild agent send` refuses it;
    no status is derived from hooks, rollout files, transcripts, titles or
    keystrokes. Hooks Guild hands the sessions it starts carry only what a
    program cannot say: which conversation it is, and that the person
    submitted a prompt; Codex's also feed managed live input's private latch
    (`docs/runtime-ownership.md`). The hooks reach only what Guild starts; the
    person's own settings are never written. The dot means one thing —
    someone is waiting on you: rust while blocked, green while a reported
    `done` is unread; *working* pulses instead. Changed 2026-11-06.

33. **A worktree is a root, and a project in the column is a group of roots.**
    Everything bound to a directory — tree, git worker, watcher, servers,
    search, Run, state — is a worktree's. What git lists is what there is.
    `worktree add -b`, never `-B`; `worktree remove` without `--force`, and the
    ignored files git would delete silently are named in a sheet first.

34. **The window may unlock a worktree.** A lock outlives the process that
    took it; `worktree unlock` loses nothing, so it is offered where there is a
    lock, after the lock's reason is shown whole. `-f` stays barred.

35. **The window has a command line, and an agent drives the window through
    it.** `guild` has no GPUI and starts in milliseconds; it finds the window by
    `GUILD_SOCKET`, given only to what Guild starts, and refuses without one.
    Every command is a door to a deed the window already has, so 21's and 33's
    guards hold for an agent unchanged. An answer is sent when it can be read
    back. **Provenance is checked by the window, not the command**: a session
    started with a task may not start another task, a task may not remove a
    worktree, and `agent send` types only into a session reading a prompt —
    never into a permission or a question. Every agent the window starts is told
    `guild` is on its path.

36. **The work is a card on the window's colour, in every palette.** The title
    strip, the column and the status bar lie on `surface.panel`; docks and tabs
    sit in rounded cards of `surface.app`; the step in tone is the separation.
    The measures are section 4.1 of the design system.

37. **Guild has windows beside its own.** A satellite — Push Commits, the diff
    window — exists at most once per kind, remembers its place, closes on
    `cmd-w` and Escape, and never decides whether the application lives: it
    quits when the main window closes.

38. **The window says what a language server is doing.** Work is kept by
    token, its message and percentage shown; a server that is missing, dying or
    refusing is a state drawn in rust, not silence. One process per project and
    language, started when a file of that language opens.

39. *Retired* — the vendored `gpui-component` patch; the component is an unmodified registry dependency, and `tests/all/editor_runs.rs` guards the viewport clipping it fixed.

40. **A file's icon is the Material Icon Theme's own table, and a Kotlin or
    Java file's is what it declares.** The set is the published extension,
    pinned; Guild's language table stands in for the editor's language registry the theme expects. A
    declaration is read by the grammar, never scanned for, and the artwork is
    IntelliJ's own. Nothing is read during a frame.

41. **External Libraries is what the build system resolved.** The build system
    is asked, not its manifest parsed — Gradle, Maven, Cargo, Go — or its
    lockfile read where asking is unreliable. On a thread per project with a
    deadline, asked again when a manifest changes, the last answer kept. A jar
    opens as its sources; nothing is decompiled.

42. **Find Usages is the language server's answer, asked only about a name.**
    The grammar decides first whether the caret is on a name; nothing gates on
    a server's advertised capability; a language without a server says so
    rather than falling back to text. Warnings are a balloon over the name; one
    usage is a jump.

43. **A workspace is a named set of seats over shared roots.** A root's
    watcher, git worker, indexes, servers and buffers exist once; its tabs,
    sessions, tree state and closed-tab stack belong to the seat. Switching
    workspaces stops no hidden work; a root's workers are released only when no
    workspace holds it.

44. **An update is a bundle already built, swapped in by a button.** The ready
    bundle lives in Guild's state folder, never in a checkout, and is copied
    beside and renamed in. The click asks the quit's questions first, then a
    helper from the bundle swaps it after the window quits. The window never
    builds itself.

45. **Update offers a release, and says what it changed.** A staged dev build
    is not offered; `just release` makes a `nightly-*` from a clean `main` after
    `just check`, renaming `Unreleased` in `CHANGELOG.md`, tagging, bundling and
    staging — never pushing. A change a person would notice adds its changelog
    line in its own branch. The changelog parser is ours, in `guild-session`,
    and the release writes through the same module.

46. **The title bar carries the plans' limits and Open In.** Claude Code's
    figures are read with its own Keychain token, which Guild never writes;
    Open In is a menu of installed applications. The look is Guild's.

47. **Codex's limits come from `codex app-server`, and the chip follows the
    agent in front.** Guild never holds Codex's token; the child's group is
    killed after a bounded exchange. The chip shows the active tab's agent and
    keeps the last one while a file or a shell is in front.

48. **A worktree with no tab open is a start screen.** The default harness as
    a card `enter` starts, a tile per kind of tab, and the recent sessions of
    every harness read from each harness's own history for this folder, newest
    first. A row resumes into a new session, or brings an open one forward.

49–51. *Unused* — reserved for designs that were not built.

52. **A continued conversation is the session it continues, and a task stays a
    task.** Provenance belongs to the conversation, not to the process holding
    it: a tab that continues a conversation Guild knows becomes that session's
    tab and keeps what it is. `guild session resume` refuses when the
    conversation is open or held by a live process. A resume typed into a
    terminal is handed over to the window's own resume, because a harness Guild
    did not start has no hooks.

53. **Two palettes, tinted by the person.** Light (Cloud's values) and Dark
    (Midnight's) are the only palettes; Theme is System, Light or Dark, and
    System follows macOS live. A person customizes either palette's
    background, accent and highlight; only the choices are stored and the
    colours are computed from the built-in palette, guarded so nothing reads
    worse than built-in. One pair for the whole application. A settings
    file's old `theme` is kept as read and only migrated. The rule is design
    system §2.9.

54. **A file list's rows share one look.** The Explorer and its libraries, the
    Commit panel, a commit's files in the Log, Push's files, Find and Find
    Usages draw a row through one look (`view::tree::ground`): the selection
    is one solid fill and no outline — `accent.patina_bg` while the list or
    its own menu has the keys, primary ink at `alpha::SELECTION_INACTIVE`
    while they are elsewhere — and it replaces the Git tint; an unselected
    row under the pointer takes primary ink at `alpha::ROW_HOVER` over what
    it shows, Git tint included; on an untinted row that is softer than
    either selection, and on any row it is not the highlight's colour, so
    the M5.8 trap of a hover in the highlight's colour does not return; the
    pointer is the arrow; names are in the interface face, code lines in
    the code face. The keyboard's cursor has no mark of its own: the arrows
    move the selection, Command with an arrow is the plain arrow (no cursor
    walks apart from the selection, so Command- and Control-Space toggle
    nothing), and a speed-search match becomes the selection. The
    right button selects its row before the menu, or keeps a group the row
    is already in — replacing M8.3.2's rule that it selects nothing. A
    folder a drag will land in takes `accent.brass_bg`, the docks' drop
    colour, apart from the selection's.

55. **A hunk is staged by the window computing the index entry's new
    content and git swapping it in only over the entry it was computed from;
    a partial tick commits the index.** Stage Hunk and Unstage Hunk are two
    writes under 21, of its second kind: each is the other's way back,
    offered at the same place.
    - **The write is git's compare-and-swap.** The window takes the index
      entry's blob (`<old>`, the preimage, read once and pinned by its id),
      replaces in it exactly the lines of the selected block, and hands the
      result to `apply --cached --no-3way --whitespace=nowarn -` as a
      **binary patch with a full index line** — `index <old>..<new> <mode>`,
      a `literal` forward and reverse, on an owned stdin file as the
      pathspec writes read theirs, the path inside it quoted as git quotes
      it and never on the argv. There are no line numbers in it to
      misplace. git 2.50.1 takes `index.lock` before it reads the index
      (`apply.c` 4828–4836) and, for a binary patch, hashes the entry it read
      and refuses unless it is `<old>` — *the patch applies to 'f' (…), which
      does not match the current contents* (3182–3192). So a `git add` from a
      terminal between the window's reading and the write is refused under
      git's own lock, not raced past. The mode is the entry's as read in the
      checkout's turn, so git also refuses an entry that became another type
      with the same blob — a symlink — under its lock (3790–3797); an
      executable bit changed from outside in that instant is the residue, and
      the shown entry's bit is kept. The path is spelled from the
      repository's top: in a folder of its repository git applies only
      patches under that folder spelled so, and skips the rest *with a
      success* — so the write reads the entry back and calls a write that
      changed nothing a refusal (review r1). Measured: a hand-built literal applies with the
      postimage object absent, a re-applied patch is refused, an entry
      changed by an outside `add` is refused, a held `index.lock` is refused.
    - **Why not a text patch.** A text hunk is matched by context: git
      applied one *at an offset* without a word, and one carrying the
      display's line numbers to the identical block twenty lines away with
      the index entry exactly the expected one — *Applied patch f cleanly*.
      Coordinates relative to the right preimage would have worked; the
      binary form needs none.
    - **The unit is the block on screen, not git's context hunk.** A block is
      one contiguous non-equal run of the line diff, what the ribbon and a
      marker show. Two changes one line apart are one `-U3` hunk to git and
      two blocks here. Unstaged diff: Stage replaces the block's index lines
      with its working lines. Staged diff: Unstage replaces its index lines
      with HEAD's. HEAD diff and the editor (HEAD ↔ working or buffer): **one
      rule maps the block to exactly its index bytes** (review r1) — the
      block's HEAD lines are found in the index through HEAD ↔ index, a
      staged insertion exactly at an edge outside them and a staged change
      across an edge disabling the action; the index bytes there decide,
      byte for byte, terminators included — HEAD's are Stage, the working
      text's are Unstage, anything else is a block partly staged and the
      action is disabled with a hint naming the Unstaged or Staged diff, as
      the person chose. A write only ever puts one side's shown bytes in
      place of that region, so an index-only line at the edge and a line
      ending staged differently cannot be taken out or misread. **A write
      never joins its bytes to a kept index line** (review r2): an
      unterminated replacement before one, or new bytes after an
      unterminated one, make the block partly staged. An empty HEAD range
      exactly at a staged insertion is that insertion, so a fully staged
      insertion offers Unstage. A block that differs from the index only in
      line endings has its own hint — no diff shows that difference, and the
      whole file is the way — as the person chose. The mapping is a binary
      search over prefix sums, not a scan per block.
    - **The text staged is the text on screen**: the open tab's buffer when
      there is one, unsaved edits included, else the disk — in the file's own
      line endings, read at each edit (a reload keeps the editor and can
      change them). The disk is never written and nothing is saved. The
      working side's identity is checked before sending: the buffer snapshot
      in the window, the disk bytes on the git thread. A diff's offers are
      bound to the document **and** the facts' fetch generation, from the
      job to the click — a refresh can bring the same document back over
      another index — and a superseded job stops. **Each control carries the
      exact offer it was drawn for** — path, document, facts generation,
      block, action — and a click whose offer is not the current one is
      stale: GPUI dispatches a click through the last frame (review r2). A
      followed buffer is a function serializing it as a save would, and a
      click compares the shown bytes with it: a reload replaces the text
      without a Change event.
    - **HEAD is part of every hunk write's identity.** Unstage writes HEAD's
      lines back, so after a commit from a terminal — HEAD moved, the index
      and the worktree unchanged — the old write would stage the inverse of
      that commit (measured). A Stage was offered over a regular HEAD entry,
      and HEAD turning the path into a symlink or dropping it would make it a
      staged type change or addition (review r3). Both writes carry HEAD's
      blob for the path — the shown side, or the facts' bytes in the
      Unstaged diff — which must still be a regular file there, and the git
      thread checks it just before applying, and an Unstage again after —
      all in the checkout's turn, so another root's thread in the same
      checkout cannot commit between them. Before: a mismatch writes
      nothing. After: git has no command that holds HEAD and
      the index at once, so a commit landing in between is reported — *a
      commit landed while the change was unstaged; check the Staged diff* —
      as auto fast-forward reports what a process outside Guild did.
    - **Refusals.** Every stale identity — the entry (git's refusal), the
      working side, HEAD before the write — writes nothing, says *the file
      changed since its changes were shown*, and recomputes — the diff's
      working side included, read again off the window's thread (review r3:
      an edit under an ignored folder sends no watcher event); git's words go
      to the Console. **One write per file while it is unanswered**: a
      second activation is ignored before the file is copied (review r3).
      The window's thread reads a diff's working text only from a regular
      file opened without waiting, so a FIFO in its place answers at once,
      and the Staged diff reads none.
    - **Hunks only where git says the bytes stay as they are** — an
      allow-list read from git, refusing what it cannot tell (review r2): a
      regular file (`100644`/`100755`) in `HEAD` and the index, one stage-0
      entry, a regular **text** working file in every view, the Staged
      diff's included, opened beneath the root with no link followed at any
      component and without blocking on a FIFO; sizes asked with
      `cat-file -s` before anything is read, and the editor's line budget
      held for the index text too. `check-attr --all` lists every attribute
      set, unset or valued — so a filter literally named `unspecified` or
      `unset` is listed — and `filter`, `ident` or `working-tree-encoding`
      listed in any state is refused; `text`, `eol`, the legacy `crlf`, or a
      `core.autocrlf` that is present and not false (a valueless key is
      true) allow only content with no carriage return, which line-ending
      conversion leaves alone — checked for the file and, per offer, for the
      bytes that offer would write (review r3: a Staged-diff Unstage putting
      `HEAD`'s CRLF back is disabled as converted).
    - **What a hunk promises — four things, and no more** (the person's
      decision after review r5):
      1. **A hunk write changes exactly the chosen hunk, or nothing**: git
         swaps the entry in only over the preimage it was computed from, and
         every stale input — the entry, the working side, `HEAD` — is a
         refusal that writes nothing.
      2. **Every hunk call, and the status walk that settles one, is
         bounded in time**: ten seconds each (reviews r3–r5), past which the
         call's process group is sent `SIGTERM` — git removes its lock files
         on it — then killed. A FIFO where `.gitattributes` should be blocks
         `check-attr`, `apply` and `status` on its open (measured). A refused
         write releases the checkout's turn and the reader permit; a walk
         past the deadline answers a failed map, the thread answers what
         comes next, and every walk stays bounded until a usable map lands.
         The settling walk finds renames as the ordinary walk does — the
         barrier ticks and plans from it, and a rename split into its two
         paths reached the commit as a deletion of an excluded rename's
         source (measured, the person's diff review of r5) — **except in a
         partial clone**, where it detects none: an inexact rename needs its
         source's bytes, an unrelated promisor source failed the whole walk
         (measured), and 2.45.0's rename prefetch fetches past
         `GIT_NO_LAZY_FETCH`. There the split `D old` takes the rename
         row's tick, so an excluded rename stays out of the commit
         (`changes::ticks_for`). `core.fsmonitor`
         is passed empty to these calls because a monitor can hold them —
         empty rather than `false`, which git through 2.35.1 reads as a
         hook's path; an empty one is unset there and false from 2.36 (git's
         source, review r4).
      3. **Nothing is fetched from the network**: `GIT_NO_LAZY_FETCH=1` on
         every hunk call and the settling walk, so a partial clone's missing
         object makes the file take no hunks. git reads the variable from
         2.45.0; with an older git a partial clone — a promisor remote, one
         with only `partialCloneFilter`, or `extensions.partialClone` —
         takes no hunks at all, with the whole-file hint.
      4. **A file git would convert takes no hunk actions**, only
         whole-file Stage and Unstage: `filter`, `ident`,
         `working-tree-encoding`, and line-ending conversion of content with
         a carriage return, as above. `hash-object` runs only with
         `--no-filters`, never `--path`, which runs a clean driver (review
         r1).

      **Nothing more is promised.** Hooks such as `post-index-change`, and
      the clean filters of *other* files that a repository-wide status may
      run, behave as they do everywhere else in Guild: whole-file Stage, the
      commit and the ordinary status walk never promised otherwise, and
      whether Guild's status reads should avoid them is an open question
      (GLD-062). An `apply` that answered 0 and whose entry then cannot be
      read back is **unverified**, not refused — the index has most likely
      changed — so the held commit is cancelled, the row's tick follows the
      next usable map, and the notice says the change was written but the
      index could not be read back, as the person chose. One that ran past
      the deadline and cannot be read back either is **unconfirmed**: the
      same, and the notice says Guild could not verify whether it was
      written (review r5). Raw hashing and facts batches wait for the shared reader
      permit, and a superseded batch skips its remaining paths. Measured: with `ident` the expanded `$Id: … $` and an edit
      beside it are one block and the bytes differ from what git would
      store; with `working-tree-encoding` every byte differs. Elsewhere the
      actions are disabled with a hint; whole-file Stage and Unstage stay.
    - **The argv pins what config could change**: `--whitespace=nowarn`
      (`apply.whitespace=fix` rewrote staged bytes of a text patch, measured)
      and `--no-3way`; no `--index`, `--unsafe-paths`, `--allow-empty`,
      `--recount`, `--reverse`. The oids are git's reads (`hash-object`
      without `-w`), so a repository's hash function is git's to choose.
    - **Unstage loses what only the index held**, as whole-file Unstage
      already does; it is the same kind as `restore --staged` and asks
      nothing.
    - **The Commit panel's tick has a third state.** A partial row's index is
      committed as it is; full still means `add`, clear `restore --staged`.
      **Only the person and a settled hunk write change what a commit
      takes** (review r3): a click, or a hunk write's row taking its tick
      from the index once its status has landed — partial when part is
      staged, full when all of it, cleared when none, as the person chose
      in review r2. A status transition alone never adds a path: an
      excluded row stays excluded whichever way its sides turn, and a full
      one edited on disk stays full — committed whole, as before GLD-026, as
      the person chose. Only a partial tick follows git — full once nothing
      of the row is unstaged, which commits the same bytes; cleared once
      nothing is staged. A row arriving is ticked whole by decision 16 of
      M5.11, as always. A status read that fails changes no tick and no row
      behind its error, so the next good map does not tick every row as
      new. A partial row is in the commit, so its menu offers Exclude, and
      Include takes it whole.
    - **One barrier holds a commit.** Pressed while any hunk write of the
      project is unanswered, or answered with no map after it, the commit
      waits; when the map after the last answer lands, the written rows take
      their ticks from the index, and only then is the commit planned — in
      its own project, whichever is in front — or cancelled and said when
      any of those writes refused since the last settled map. The worker
      drains a batch before its status walk, so a plan from the map before
      would `add` the whole file. A flight belongs to the git thread it was
      sent to and is counted only when that thread took the write; a
      replaced thread drops it and cancels the commit waiting on it. A
      refusal asks the diffs and markers showing its path again. **Anything
      but a clean write cancels the waiting commit** (review r3): a refusal,
      a `HeadMoved` — after which the index may hold the inverse of the
      commit that landed — and a status read after the writes that fails,
      which leaves their ticks for the next usable map; the notice says to
      check the index, whatever the cause, as the person chose. **The
      barrier reads every outcome and every map in the order they came**
      (review r4): a failed map while a write awaits its status cancels the
      commit even when a usable map follows in the same poll. While a
      write is pending the Commit button asks only for a message, like
      `cmd-enter`. **A folder project's commit asks its scope again** in
      one turn of the checkout held through the preparation and the commit:
      a change staged outside the folder meanwhile — by the checkout's own
      hunk write — refuses it, as decision 18 of M5.11 does from the map;
      no `diff.ignoreSubmodules` can hide a staged gitlink from it (r4).

56. **A worktree's merge request is read through glab, and only read.** The
    column shows each worktree's GitLab MR, its verdict, its corner badge and
    its path to merge; the status bar repeats the active one. Guild writes
    nothing to GitLab and holds no token (decisions 46 and 47): every read
    is `glab api --hostname <host> <endpoint>`, the person's own glab login
    for that host, GLD-063.
    - **The adapter is the removed MR panel's reader** (`69c459cf`) cut down
      to reads: no writes, no `projects/:id` discovery, and one account read
      (below). Host
      and project come from the worktree's push remote — its push URL, a
      `pushurl` included — with the host's glab `subfolder` cut from the
      front of the path, since glab puts it in its API base; the MR by the source
      project and the upstream's **remote** branch name (`Ahead`'s rule), so
      a local `bar` tracking `origin/foo` is looked up as `foo`. A fork's MR
      into its parent is not looked up. **The repository's main branch is
      not asked** — `git::main_branch`'s rule, the one a new worktree
      starts from: `origin/HEAD`'s local branch, else `main` or `master` —
      read with the worktree snapshot off the window's thread: no glab call,
      no MR and no failure on its row.
    - **The person's own MR is marked, by one account read per host**: the
      list's `author.id` against `glab api --hostname <host> user`'s `id`,
      read once per host per refresh, only for a host whose batches found an
      MR and that no 401 signed out in that refresh, under what is left of the refresh's deadline and through the same
      admission, caps, scrubbed environment and proxy as every read, and
      kept for the window's life until the next read on the host replaces
      it. A failed account read is nobody's problem: the last account read
      for the host still marks; with none read yet, the MR is shown without
      the mark. A person
      after the MR's number says "You created this merge request",
      GLD-063.
    - **REST, per branch**: every page of its MRs in every state, an open
      one over a merged or closed one and the newest of a state; for an
      open one its detail, approvals and every page of its discussions.
      **Each branch is whole or nothing**: a branch whose read failed — a
      call timed out, a record changed or incomplete, any other refusal —
      keeps its own last known MR beside its own failure and is never shown
      as "no MR", while the other branches of the same batch publish what
      they read. A deadline passing mid-batch times out each branch left,
      without a call.
    - **What is not a branch's is the repository's state, not a row's**:
      glab missing, a host not logged in — a 401 included — or with a port,
      and a batch failed as a whole — glab's own proxy, the network policy,
      the refresh's deadline spent before the batch started. The rows keep
      their last known MRs and draw no reason; `Forge::status` says the
      repository's progress (queued, checking), when it was last answered
      and these problems, and **the repository's header draws it** — one
      indicator before `⋯ +`, the approved Penpot board *MR header status —
      column*: checking (turning arrows) over queued (a clock) over a
      problem of the last answer (struck-through arrows in brass, every
      problem in the hover) over checked (a faint ✓, only under the pointer
      on the header). No ✓ before the first answer — the first refresh is
      queued and checking like any other — and nothing at all for a
      repository with nothing left to ask, whose problem no answer would
      clear, or off GitLab: one whose worktrees were all found to push
      elsewhere, or, before its first answer, push to a host without
      GitLab in its name. Progress moves
      outside an answer's landing, so the window compares every header's
      indicator with the one it last drew, GLD-063.
      Only an absolute
      http or https `web_url` is an MR's link; anything else makes the
      record incomplete, so nothing but a web page is opened from GitLab's
      answer.
    - **Bounded**: a per-call deadline and **one deadline for a
      repository's whole refresh**, each of its batches given an equal share
      of what is left — so an unresponsive host cannot spend it all and leave
      the next project none — looked at before a permit is granted and
      again right before a child starts; output read from pipes as it comes,
      no byte past the cap kept, a stream past it ending the child; and
      **at most two glab children across every repository**. Each child is a `ProcessGroup`, with
      the proxy from `crate::network` under `Scope::Guild` and its
      environment scrubbed of endpoint, base-path, proxy, CI, credential and
      debug overrides (`merge_requests::glab::SCRUBBED`).
    - **Ended in two ways, never mid-save by a stop.** glab may be writing
      a refreshed OAuth login to its config when it is interrupted, so the
      setting going off or a repository leaving the column only *stops* a
      refresh: no child more starts — checked under the lock a child is
      started under — a running one finishes, and its answer is dropped. A
      deadline and an output cap *end* a child: `SIGTERM`, a 2 s grace, then
      `SIGKILL` and reaping. A refresh is owned until its thread ends; none
      starts beside it, and one whose repository left the column has lost
      its right to publish.
    - **The quit fits GPUI's 200 ms.** GPUI 0.3.8 waits `SHUTDOWN_TIMEOUT`
      (200 ms) for a quit's cleanup and has no way to ask for more, so the
      quit aborts every refresh — `SIGTERM`, a 100 ms grace — and 150 ms in,
      from a background task, `SIGKILL`s every glab group still alive
      (`glab::kill_every_child`, through `ProcessGroup::killer`, which
      signals only while the group's owner has not reaped it). A glab deaf
      to `SIGTERM` dies with the quit rather than outliving it; one cut off
      mid-save loses that save, which is the price of not leaving it behind.
    - **glab's own proxy is not overridden.** glab prefers its per-host
      `proxy` to the environment's, so with Guild's network set to Custom a
      host glab keeps a proxy for is not asked, and the row says the read
      failed; in System mode glab's configuration is the person's, its proxy
      included.
    - **Logged in or not is read locally**: glab's `config.yml` — the file
      glab 1.118's `SearchConfigFile` picks: `GLAB_CONFIG_DIR` alone when
      set, else the legacy `~/.config/glab-cli`, else XDG's config home —
      lists its hosts, keyed with an https port; a host with a token or
      `use_keyring: true` is signed in. Values are read as YAML scalars, an
      empty block scalar empty. A relative `GLAB_CONFIG_DIR` is made
      absolute once, for this read and for every child, which runs in `/`.
      A host's `ssh_host` names it only for an ssh or scp-style remote. A
      401 signs a host out for the rest of the refresh: one probe. A
      signed-out host, a missing glab and a host with an explicit port —
      which glab 1.118's `--hostname` refuses — cost no request; the
      repository's state says why only for those three, and a missing glab
      only for a remote that looks like GitLab. The port stays in the
      host's identity.
    - **What is not known is not "no MR".** A record without its project or
      its source branch, an MR closed between the list and its detail, an
      approvals answer that states nothing: unknown, never a ✓. A branch read
      with no MR whose next read failed is unknown again. GitLab's
      `not_approved` with no failing approvals line is GitLab's own block,
      said without a count; the detail's `draft` wins over the list's. A branch asked and never answered
      draws the plain glyph with its hover saying the MR is unknown and why.
      `ci_must_pass` with no pipeline is a missing pipeline, a ✗.
    - **One logical refresh per repository**, batched by host and project:
      at start, on coming back (30 s debounce), after a push or a switch made
      from Guild, and every two minutes while the window is active — for the
      repositories the active workspace's column shows, and no other. The
      column's candidates are taken in before any reply lands. A
      trigger while one is out is one pending refresh; a reply is dropped
      when the branches or the setting's generation moved while it was out.
      After a push or a switch from Guild no reply publishes and nothing is
      asked until the worktree list after that write has landed. Each
      repository keeps its own answers and failures, and an answer belongs
      to the whole candidate — worktree, URL and branch — so a moved
      destination keeps nothing of the old one.
    - **A repository the column stops showing keeps its last known MRs in
      memory**, never on disk: a workspace switch hides it — not asked, its
      refresh stopped and that refresh's answer dropped — and shown again
      its rows draw what was known at once while it is asked in the
      background. Only the setting going off forgets.
    - **A landed answer notifies the window only when what it draws
      changed** — each row's MR and its own failure, and the repository's
      problems: a notify of the root rebuilds the whole column
      (`docs/traps.md`), and a refresh every two minutes that read the same
      MRs used to be one such frame per repository each time.
    - **A setting, on by default** (Projects → Git), starts no new glab
      child when off; one already running finishes and its answer is
      dropped. `↑N`, the commits a push would send, is git's — refs only,
      Push Commits' rule without an upstream — and shows either way.

57. **A file operation never takes away a root Guild has open.** The Explorer
    refuses Rename, Move, Cut and the Trash for its own root (`writable_row`)
    and, since GLD-057, for any row that is, or holds, a project the desk
    has open in any workspace — a member in its group folder's Explorer, a
    worktree opened from inside its checkout. Each would leave that root's
    tabs and sessions on a path that no longer exists, and the Trash's
    question about unsaved buffers counts only the tree's own project.
    Copy and Paste into it stay: neither writes the directory itself.
    - **Visible before the gesture, refused in the handler.** The row's menu
      draws the four disabled with a one-line footnote, and the row is no
      drag source; the handlers ask again on the resolved source paths
      (`Root::refuse_open_root`) and say why in a Warning, because a key
      binding reaches the handler without the menu and a drop reaches the
      transfer without either. Every move meets in one place, the transfer's
      start, so the refusal is there too; the Trash, a rename and a move's
      collision retry ask once more at the write for a root opened while
      their question was up.
    - **The menu compares spellings, the handlers the disk.** Drawing reads
      no files, so the rows and the menu match paths as written; the
      handlers also resolve symlinks (`open_root_resolved`), so a tree
      opened through a symlinked ancestor still finds a member opened by
      its real path. A symlink row is the link, not its target.
    - **Closing the root first is the way.** The window offers no shortcut
      that closes and then renames.
