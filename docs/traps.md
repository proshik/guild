# Traps that have already cost us

Do not step on them again. Read this before working on the area a trap names.

- **A `claude` session id is assigned, not guessed.** The `--session-id <uuid>` flag works in both modes — `--print` and interactive — and `--resume` then brings the conversation back (verified end to end with a code word). Before that was discovered, a terminal session's id was looked up as "the newest transcript for this directory" — and a tab attached itself to the session of the agent writing this very code: in one directory every session writes at once. `--resume` would have led into someone else's conversation silently. There is nothing to guess: the record's `SessionId` **is** the conversation id.
- **Do not reproduce the `~/.claude/projects` directory naming scheme.** It is promised nowhere: `/` and `.` become `-`, and so does every non-ASCII character, one by one. What we need is the file `<session-id>.jsonl` — look for it across all directories at once (`transcript::has_conversation`) instead of computing a path. A computed path drifts from the real one silently: it returns "no session" instead of an error.
- **`--permission-prompt-tool` does not exist in CLI 2.1.251.** The M5 plan was written assuming it. What does exist is `--permission-mode` with the values `acceptEdits`, `auto`, `bypassPermissions`, `manual`, `dontAsk`, `plan`. Check that a flag exists before building a task on it.
- **In an unfamiliar directory `claude` asks about trust first.** The "Is this a project you created or one you trust?" dialogue appears before anything else and waits for an answer. The terminal shows that dialogue and the person answers it there.
- **`bezel-terminal` must not be used.** It pulls the `bezel-gpui` fork, putting two incompatible GPUI versions into one binary. `just boundaries` guards this.
- **The off-the-shelf `gpui-terminal` did not fit, and it cannot be fixed from outside.** It ignores `Flags::DIM` — `grep -rn DIM` across the crate finds nothing — while `claude` sends that flag dozens of times: that is how it draws hints. `INVERSE`, `HIDDEN` and `STRIKEOUT` were lost too, and `shape_line` was called per character. The terminal is now ours, on top of `alacritty_terminal`.
- **Frame capture holds the grid mutex; shaping and painting must not.** An accessor on `Emulator` that locks again during capture deadlocks, because a std mutex is not reentrant. It does not look like a deadlock from outside: the window never appears, while its icon remains in Cmd+Tab. `sample <pid>` identifies the main thread in `Window::draw` and the PTY reader waiting on the lock. This is why cursor state is read from the existing guard. Before the 2026-10-15 resource correction, the guard covered the entire paint: cold glyph preparation could block incoming PTY output. Capture owned viewport runs, cursor and scrollbar under one guard, then release it before font shaping, rasterization and scene construction. Never acknowledge newer parser damage while using that older snapshot.
- **Scrolling a full-screen program does nothing, and that is correct.** `claude`, `vim` and `less` draw in the alternate screen, which has no history: `scroll_display` moves an offset that cannot move. What terminals do instead is send cursor keys — xterm's alternate-scroll — unless the program is reading the mouse itself, in which case they send nothing. `terminal::scroll::action` decides; the encoding is shared with the arrow keys so the application-cursor mode cannot be handled in one place and forgotten in the other.
- **A test may not assume this checkout has been built.** `opening_this_repository_does_not_read_target` asserted that `target/` is there and dimmed — the design decision that an ignored entry is shown rather than hidden — and a worktree with its own `CARGO_TARGET_DIR` has no `target/` at all. It reddened a branch that had never touched a tree, and an hour went into reading it as a bug in the pass. A claim about a directory that may not exist is made only when it does, and says so when it does not.
- **A test's scratch folder was named by hand and never removed.** Every filesystem test made `std::env::temp_dir().join(format!("guild-<topic>-<name>-{pid}"))`; many never removed it, and the rest did so on their last line, which does not run when an assertion above it fails. On 2026-09-24 the person's `$TMPDIR` held 28,562 entries, nearly all of them ours, and one green `cargo test --workspace` into an empty `TMPDIR` left 94 more. A test's folder is `scratch::Scratch` in `tests/all/` and `tempfile::TempDir` in a `#[cfg(test)]` module now, and `tests/all/leftovers.rs` refuses the hand-made form outside the four production uses it names. Two ways a `TempDir` still leaks, both met on the way: **a path taken from a temporary** — `scratch(name).join("Guild.app")` compiles, drops the folder at the end of the statement, and the next `create_dir_all` makes it again where nothing will remove it; the update tests did that, and three of their `!dir.exists()` assertions held of a folder that could never have existed. `Scratch::within` is the form that holds on, and `leftovers.rs` looks for the other. And **a write that outlives its test**: `project_state::save` queues on the store's thread, so a test that ended first had its folder made again after tempfile removed it — the session's `Scratch` flushes that queue before it lets go.
- **A synthetic click needs about a tenth of a second between down and up.** With 30 ms GPUI does not reliably see a click at all: the row lights up under the pointer, `on_click` never runs, and it reads exactly like a handler that was never wired. Found in M8.8.5, and proved to be the sender rather than the code because the file tree — shipped and used since M8.1 — failed the same way in the same session. 150 ms works every time. Every "the click does nothing" of this milestone is worth re-testing at that delay before it is believed.
- **Three Guild windows sit at the same centred place on one display.** Yours, another branch's, and the person's own: raising one by pid and clicking is a race that another driver wins often enough to matter, and a right-click landing in somebody else's window looks like a handler that did nothing. The rule that works is to verify the frontmost process **is** the intended pid immediately before every gesture, and refuse otherwise — activating and then acting in two separate shell commands is exactly the gap.
- **A test that opens a pty spends a system-wide resource.** `kern.tty.ptmx_max` is 511 and shared with every terminal on the machine; a workspace run next to a few of your own sessions exhausts it, and then *every* pty test fails with `failed to openpty: Os { code: -6 }` — which reads as a broken test rather than a full machine. Assert several things per pty test rather than adding one test per assertion.
- **Waiting for a real process needs a clock, not an iteration count.** A hundred 20 ms sleeps is two seconds on an idle machine and much less than that under `just check`, which runs the whole workspace at once. It shows up as tests that fail in a full run and pass on their own.
- **A copy of a system binary is killed by macOS soon after it starts.** `/bin/sleep` or `/bin/cat` copied anywhere keeps Apple's platform signature, and on macOS 26.5 the copy dies of `SIGKILL (Code Signature Invalid)` — the crash report names no sender — some hundred milliseconds into its life, with nothing moved or renamed. The update guard's test ran such a copy as "a live Guild": alone it finished in time, under a loaded `just check` the kill landed between its liveness check and `in_use`, and `in_use` rightly skipped a zombie (GLD-068 has the measurements). A fixture process copied from the system is re-signed ad hoc (`codesign --force --sign -`) before it runs, and a fixture that renames its own executable's directory waits until the process has answered something first: a rename during its first milliseconds can kill even an ad-hoc copy.
- **The terminal size must be told to the pty, not only to the grid.** `Term::resize` changes the grid inside, but the program keeps drawing at the old width until `MasterPty::resize` receives a `PtySize`. It looks like "full screen does not stretch the terminal". Checked with `stty size` from inside the process — from outside it is invisible.
- **A tab is spawned before it is drawn, and its program reads the size at once.** The first fit happens in the tab's paint, and a tab restored behind another is not painted until it is chosen. So every such `claude` started at the 80 × 24 the spawn made up, printed its header and first screens narrow, and stretched only when it was first shown. What it printed before then stayed narrow. Measured with `stty -f /dev/ttysNNN size` from outside: 24 80 behind, 29 122 in front. `terminal::view::StartSize` is the last fit of any body tab, kept in the settings as `tab_grid`, and a tab is spawned at it.
- **We own the pty process.** Neither the component nor `alacritty_terminal` holds it. Without an explicit owner with a `Drop`, closing a tab leaves `claude` orphaned: the same trap as a zombie after `kill` without `wait`.
- **A `claude` session id can be dead in both directions.** `claude` creates the transcript file as soon as a session starts, before the first message: it contains only `ai-title` and `agent-name`. It considers such an id taken (`Error: Session ID … is already in use`), yet there is nothing to continue (`No conversation found with session ID`). So there are three cases, not two — `guild_session::continuation`. The **presence of a file** is not enough to decide: a conversation begins where `user` and `assistant` records appear.
- **`just check` does not rebuild the binary.** It runs `clippy --all-targets` and `cargo test`; the executable in `target/debug/` stays as it was. Checking the window after an edit without `cargo build -p guild-desktop` looks at the old build — an hour went into "why does the fix not work" when it simply had not been compiled.
- **Nothing in the window takes focus by itself, and then no shortcut works.** GPUI dispatches a shortcut along the path from whatever holds focus; with nothing focused that path is the dispatch root alone, and the window's own `on_action` handlers are not on it. It looks like the keymap is broken rather than like a focus problem. The root holds a focus handle and takes focus when nothing below can; a terminal tab takes it on render.
- **A shortcut being recorded must not also fire.** Pressing `cmd-w` to bind it would close the tab. Every binding carries a `!recording` predicate and the root's key context gains `recording` while a stroke is being read — the stroke is an answer to a question, not a command.
- **`claude` reads the mouse itself.** It turns on `SGR_MOUSE | MOUSE_MOTION` at startup, so the wheel belongs to it and is forwarded rather than acted on. In the alternate screen without mouse reporting the wheel becomes cursor keys, as xterm does; only ordinary output moves the history. All three live in `terminal::scroll::action`, and the cursor-key encoding is shared with the arrow keys so the application-cursor mode cannot be handled in one place and forgotten in the other.
- **A trackpad reports pixels, a wheel reports notches.** Two or three pixels per event, rounded on its own, is zero every time — the trackpad did not scroll at all while the wheel worked. The remainder is carried between events in `terminal::scroll::Scroll`.
- **The terminal is painted cell by cell, not row by row.** A row shaped into a single `shape_line` rests on the assumption that every glyph is exactly one cell wide. One icon from a fallback font with a different advance, and the rest of the row slides right, away from its own background. It shows up immediately in a `powerlevel10k` prompt. Each run is painted at its own place: `origin.x + cell_width * column`.
- **A run's `len` is measured on the text that is shaped, never on the text it came from.** M5.14's task 10 gave the font a space in place of every block and box line it draws itself, and kept measuring the original run: a `▛` is three bytes and a space one, so `shape_line` sliced past the end — *end byte index 71 is out of bounds for string of length 69* — and the main thread aborted. The tab with `claude`'s frame came back with the session, so every start died on its first frame and Update Guild could not be reached; the build had to be replaced from a shell with `just install`. `paint::typed` makes the substituted text a `Run`, so the text and its length are one value. Any rewrite of a run's text goes through a `Run`, not a `String` beside one.
- **The terminal font is chosen from what is installed, not named in code.** Modern shell prompts are drawn with Nerd Font glyphs; an ordinary monospace font does not have them, and they come out as boxes with a question mark. The `Nerd Font Mono` variant is preferred: the plain one makes glyphs double width and the grid falls apart.
- **The state file format changed: the old key was `records`, the new one is `sessions`.** Both are read via `serde(alias)`. The first version of the migration test checked a key the old file never had — the migration "worked" on invented data while a real file would have been discarded silently. Take the shape of an old file from disk, not from memory.
- **A uniform list's top row is an offset, not an index.** `UniformListScrollHandle::logical_scroll_top_index` is behind gpui's `test-support` feature, and the base handle's `logical_scroll_top` answers zero for a uniform list, which keeps an offset and never fills the child bounds that method reads. The row at the top is the offset over a row's height, the height read from the window's rem size at each render. And `scroll_to_item` does nothing for a row already in view — the diff's second `f7` stood still until `scroll_to_item_strict` was used.
- **A drag that begins while the application is not active does not start.** The first press activates the window and the drag events that follow it are not a drag; the driver activates the window and drags in the same breath. In a person's hands the window is active already, which is why nobody sees it.
- **A dock a move leaves with no tool stayed open on *Nothing here*.** A hole where the tool was, not a place; `move_tool` closes it, and the state closes such a dock at load for files written before it did.
- **An M8.3 state file has a face and no docks, and a project left on the Log was left with the left panel open.** The Log is a bottom tool now; `ProjectState::normalise` makes the docks from the face and opens the bottom dock instead, so the next save writes what the window shows.
- **Reading an entity from inside its own render panics.** The diff header's button read the tab's mode with `cx.entity().read(cx)` while the rows were being built, and GPUI refused with *cannot read while it is already being updated* — the window went down on the first diff drawn. Read what the rows need at the top of `render` and pass it down.
- **A tab restored before the project's git thread exists asks nobody.** The Log face and the annotate tab asked for their first page at their first render, and the thread that answers is made by `poll_git` a tick later; the ask was dropped and nothing asked again, so the Log said *No commits yet* of this very repository. A thread's start makes the first asks for whatever was waiting on it. **M12.3 met it a third time**: Push in a dock asked at its first render, the ask made the project's entry and was dropped, and the entry then said the ask had been made — so it stood on *Reading…* for good. Whatever marks "asked" before the thread can hear it has to be asked again by the thread's start.
- **The autosave table reads "no editor" as "not text".** A tab not yet shown has no editor, and its facts say the buffer is not text; a commit that asked every tab of the project whether it could be written was stopped by a clean `.gitignore` behind the front tab. Ask dirty tabs only — a tab without an editor cannot be dirty.
- **Under `-z` with a record mark, the NUL after the mark belongs to the record before.** `git log` with `%x01` at the end of the format and `-z` puts a NUL after each commit's mark, which lands at the start of the next chunk; the NUL at the end of a chunk is the last field's own terminator. Stripping both made an empty body a missing field. Strip leading NULs only.
- **`git restore --staged` resolves `HEAD`, and refuses on a branch with no commit yet.** The worker's test found it on a fresh `git init`; the fixture makes one commit first. In the window the refusal is a notice in git's words, and `git rm --cached` in a terminal is the way there.
- **Opening a tab focuses it, so focus the panel after, not before.** A click on a Changes row opens its diff and is meant to leave the keyboard in the panel; the first version focused the panel and then opened the diff, and the space bar went to the tab. Enter is the gesture that goes there.
- **The component's `set_value` wants a window, and a commit lands on the watch loop, which has none.** The box is marked for clearing and cleared at the next render, which is where a window is in hand; the same reason the box itself is made at the first render of the Changes face rather than when the map lands.
- **`std::fs::write` truncates before it writes.** The state file was saved with it, and `load` treated an unparseable file as "no state" and started clean. Separately both are right; together they are a silent data-loss path — a crash between the truncation and the last byte took every project, tab, session and the cost history, leaving one warning in a log nobody opens. The cure is the usual one and its two details matter: `sync_all` on the file **before** the rename, or the rename can reach the disk first, and `sync_all` on the **directory** after, which is what survives a power cut rather than only a dead process. The old file is kept as `.bak` and `load` falls back to it — that fallback is what makes the two-rename form safe, not a spare wheel.
- **A transcript's `usage` repeats itself in an `iterations` array.** Summing both doubles every turn, and the file reads as though it wants to be walked recursively. Only the top-level counts.
- **A state file that could not be read is not a broken one.** `Store::read` took every IO error other than `NotFound` for corruption: on 2026-11-07 the installed Guild ran out of descriptors (`EMFILE`, the Dock's soft limit of 256), and loads moved 44 `state.json` files and their `.bak` aside as `.unreadable-…` — all 44 valid JSON — then answered a default that the next save wrote over the person's state. Only bytes that were read and do not parse are kept aside now; an IO failure on either copy moves nothing, answers `StoreError::Read`, and the path refuses saves (`StoreError::NotRead`) **until restart, not until the next good read**: the settings and the desk are loaded once and held, and a later read by anyone else does not make the default a holder kept any less wrong. The executable also raises its open-file limit at start (`system::open_files`).
- **`cost-state` is rare and `totalCostUSD` is a running total.** The record carrying money appears in about one transcript in thirty-five — 21 of 755 measured — so tokens are the number that can always be shown and dollars are a bonus. And within a session the figure is what has been spent so far, not what a turn cost: absorb it by replacing, not by adding, or a person is billed once per reading.
- **A key that belongs to a popup has to be caught where every key passes, not where the popup is.** The Log's filter menus handled backspace in the Log's own `on_key_down` — which runs only while the Log holds the focus, and a person who opened a menu with the pointer has no reason to have arranged that. Nothing happened, and nothing said why. The root's `capture_key_down` sees every key whatever is focused; that is where a popup's own keys belong, next to the Recent Projects popup's.
- **A popup drawn over a focused view still lets that view have the keys.** A key event is dispatched from the *focused* node upwards, so a `on_key_down` on the root runs **after** the terminal's — by which point the letter is already in a live `claude`. Typing into the Recent Projects popup typed into the session behind it. Drawing on top changes nothing: the dispatch path is focus, not paint order. The cure is `capture_key_down` on the root plus `stop_propagation`, which runs on the way *down* and never reaches the bubble phase at all. Bindings are dispatched before both phases (`dispatch_key_event` matches them first), so capturing does not take a shortcut away — the popup's own `cmd-shift-o` still closes it.
- **A persisted enum is a wire format, and a variant's name is data.** `Tool` is `snake_case`, so a variant's name is what lands in `docks.placed` in every project's `state.json` — and the variant that proves it is `Tool::Git`, which is the string `"log"` on disk because it was `Tool::Log` until M8.3.4 renamed it. Renaming or deleting a variant makes serde fail the **whole file**, and `Store::load` then answers `T::default()` — the project loses its sessions, its open tabs, its expanded directories and its panel sizes because a tool was renamed. `#[serde(rename = "log")]` keeps the name on disk; a retired tool keeps its variant, leaves `Tool::ALL`, and is stripped from the docks by `normalise` until no file names it.
- **A record with a trailing separator has one field more than it looks.** The log format ends `%b%x00%x01`, so a correct record splits into `FIELDS + 1` parts with the last empty — and a capture one field short splits into `FIELDS` and **destructures happily**, reading the summary as the refs and the body as the summary. Counting the parts exactly is what catches it; a `let [a, b, .., ] = ` pattern does not. The first version of the test asserted the error and passed on the shift.
- **An absolutely placed popup is still painted in its own place in the tree.** A menu drawn as a child of a button in a strip that comes *before* the list it hangs over is painted before that list, so the list paints over it and the menu reads as transparent — its background is there and the rows are simply on top. `deferred` is what GPUI has for a popup that must outrank its place in the tree, and `anchored().snap_to_window_with_margin` puts it where it hangs and keeps it inside the window. The branch popup had already learned this; the Log's filter menus reinvented half of it and hit the same wall.
- **A hitbox does not hide what is behind it by covering it.** A popup drawn over a full-window backdrop still lets that backdrop record the press: by default `Hitbox::is_hovered` is true for every hitbox under the pointer, not only the front one. So a mouse-down inside the popup became a *pending click* on the backdrop as well, and letting go counted it — the popup was closed by its own backdrop, from inside itself. It showed up only when the popup was dragged, because the resize edges call `stop_propagation` and the header did not. The cure is `.occlude()` on the card, which is what GPUI has for exactly this; `stop_propagation` on each handler is patching one hole at a time in a surface that has many.
- **…and occluding takes the mouse away from the window underneath, including the handlers that were driving the drag.** `on_mouse_move` and `on_mouse_up` both fire only while the element's own hitbox `is_hovered`, and an occluding popup makes the root's hitbox stop being hovered wherever the popup is. So the window stopped receiving moves the instant the popup caught up with the pointer — the drag advanced only in the moments the pointer slipped off — and letting go over the popup never ended the drag at all, leaving it stuck to the pointer with the place never written down. Whatever occludes has to carry the handlers for the gesture it is part of. It reads as "the drag lags", which sends you to look at frame times: those were 8.3 ms, pinned to the display's 120 Hz, and the cause was not there. Text fields also intentionally stop bubble move/up events: carrying a bubble listener on the card is insufficient once its header contains a real input. A floating picker follows only its active grab through window capture, consumes that gesture before child selection, and applies the release coordinate before dropping it (2026-10-26).
- **`on_drag_move` is heard by every listener, wherever the pointer is.** GPUI runs it in the capture phase with no hit test — the listener gets its element's `bounds` and is left to ask itself. The strip's drag (M14) never asked, so on every move each tab moved the dragged one beside itself in turn and the last tab drawn won — by that arithmetic, read from GPUI's source rather than seen, a short drag of the first of three tabs puts it second. With one strip it read as a jumpy reorder; with M12.8's areas the last tab drawn is another area's, and every drag would have carried the tab away. A listener that acts on a place checks the pointer against its bounds first — **and those bounds are not clipped by the scroll container**: `event.bounds` is the raw hitbox, where GPUI's own hit test uses `hitbox.bounds.intersect(&hitbox.content_mask.bounds)`. A strip's tabs scrolled out past its end keep live bounds over whatever lies there — with two areas side by side, the next area's strip — so the check is against the tab's bounds **intersected with the strip's visible box** (`view::tabs::answers_drag`). 2026-09-29.
- **A drop has its own position.** A tab's no-op `on_drop` assumed the last drag move had already put it there. A fast gesture can release over a different tab: with five tabs, the first stayed second when released at the fifth. Apply placement on release too, through the same reorder action. `ScrollHandle::bounds_for_item` is unscrolled; add its offset before comparing it with `window.mouse_position()`. A GPUI event-dispatch regression reproduces this without a native window or a live agent. 2026-10-02.
- **`overflow_y_scroll` does nothing without `min_h_0` on the same item.** A flex item's automatic minimum size is its own *content*, so a list taller than its box pushes the box open from the inside rather than scrolling. With a card that clips, the result looks like a list missing its last rows — and the wheel does nothing, because as far as the layout is concerned there is nothing to scroll. The scroll properties are not the fix; the minimum size is.
- **A scrollbar is drawn, not provided.** GPUI scrolls but ships no bar. The thumb comes from the `ScrollHandle`: `bounds()` is the view, `max_offset()` is what is left over, and `offset().y` runs **negative** as the list goes down. It is worth wrapping the scrolling element in a box of its own — then the bar's track is that box's ends, and nobody has to measure the header above it.
- **MouseExited leaves GPUI's last in-window coordinates intact on macOS.** `is_window_hovered()` means window activity there, so polling those two values alone can keep an Info notification paused after the pointer leaves. Remember exit until movement returns; retain the listeners even when the stack is empty, or a returning pointer cannot release that remembered exit before the next notice appears. GPUI's test window also reports `(0,0)` at frame start, so event tests must account for that platform limitation without silently weakening the hover assertion. The notification regressions cover both exit and empty-stack reentry. 2026-10-10.
- **A resize cursor is only set while the window is active, and only when the hit test changes.** On macOS GPUI's `is_window_hovered()` *is* `is_window_active()`, and `reset_cursor_style` runs from `dispatch_mouse_event` only when the set of hovered hitboxes differs from the last one. So a pointer resting on a handle over an inactive window shows a plain arrow, and it stays an arrow after the window is activated until the pointer moves off the handle and back. Nothing in our code can set it instead — `set_cursor_style` only records a request for the frame. When a handle's cursor is reported missing, that is the first thing to rule out, and the second is the size of the target: three points is what the eye needs and not what a hand needs, which is why every grip now draws three and takes nine.
- **An element that tracks focus takes the focus on mouse-down by itself, and the window's root tracks focus too.** GPUI registers an automatic bubble-phase listener on every focusable element: it focuses *itself* and calls `prevent_default` so no ancestor does. Bubble is innermost-first, so the root — which holds the window's own handle — runs **last** and wins whenever the press was not inside a focusable descendant. The Log's toolbar therefore focused the panel by hand from `on_mouse_down`, and the root took it back in the same event: the click landed, the field lit up, and not one letter ever reached `log_key`. Focusing from `on_click` happens on mouse-*up*, after all of it, which is why the tree and the Commit panel were fine. The cure is not another `window.focus` call — it is putting `track_focus` on the element that is actually pressed. Measured with a probe in the running window after two blind fixes: the counter said `log_focused=false root_focused=true` on every key.
- **A filter that re-asks git must not empty the list while it waits.** `restart_log` cleared the commits and closed the opened commit before asking, so every letter blanked the panel and filled it again — one blink per keystroke. What is on screen is the same log one narrowing behind; it stays until the new page lands and replaces it whole. And the note for an empty result belongs **in the column the field narrows**, not in place of all three: a word that matches nothing emptied the branch tree and the opened commit as well, which reads as the tool breaking rather than as a search finding nothing.
- **A cross inside a row runs the row's handler too, and it runs second.** GPUI's bubble phase walks the frame's listeners in reverse paint order, so a child fires before its parent: a tab's ✕ closed the tab and *then* the row underneath made the closed tab the one in front — an empty body, no tab lit, and the state saved that way. The project tab's ✕ had the same shape. It is not only ancestors: every hitbox under the pointer is hovered, so a notice drawn over the Git panel dismissed itself *and* opened the commit beneath it. A gesture aimed at something inside a row says so — `stop_propagation` on the cross, `.occlude()` on anything drawn over a surface with handlers.
- **A note that replaces a list must not take the list's controls with it.** The Log returned early when it had no commits, and the toolbar was built after that line — so *Date: last day* in a repository with nothing committed today emptied the panel and removed the cross that would have undone it. Escape rescued the text filter and nothing rescued the others. It is the `Branch: HEAD` bug in another shape: a state that can be reached and not left. The controls belong outside whatever the emptiness replaces, and the note says *which* emptiness it is — "no commits match the filter" is not "no commits yet".
- **A list with rows keeps one of them lit, or `enter` answers nothing.** The branch popup cleared its highlight after every keystroke, so a person who typed a new name, saw *Create branch …* offered and pressed the obvious key got silence — and the same silence when a narrowed list left the highlight past its end. `pick::settle` is that rule beside `pick::step`, and the Recent Projects list had it from the start.
- **A list that scrolls needs the keyboard to scroll it too.** Arrows that move a highlight past the fold move something nobody can see, and Enter then opens it — worse than a list that does not scroll at all. `ScrollHandle::scroll_to_item` is applied during prepaint, so it can be called straight from the key handler, before the row heights of the new list exist.
- **A drag's release can go missing, so the drag has to be able to end without one.** Let go outside the window and no hitbox is hovered: neither the window nor the popup hears the button come up, the drag stays live for ever, and the popup then follows the pointer with nothing pressed — through being closed and opened again, because the state is in memory rather than in the pointer. Every `MouseMoveEvent` carries `pressed_button`, so the answer is to correct the state on every move rather than trust a release to arrive: no button held means the drag is over. On macOS `NSLeftMouseDragged` fills that field and a plain `NSMouseMoved` leaves it `None`, which is exactly the distinction needed.
- **A global action listener runs before the window's, in the capture phase.** `Quit` was handled with `cx.on_action` at the application level and quit outright — the window with the unsaved file never got to ask, while the close button did. What has to ask needs the window, and gets it by handle from the binary. And a window cannot be updated from inside its own update: an action arrives during one, so the handle's `update` answers "not found" there. `cx.defer` first.
- **A program that reads the mouse gets all of it, not just the wheel; option-drag is ours.** `claude` turns on 1000, 1002, 1003 and 1006 and draws on the alternate screen, which has no history: it selects, autoscrolls at the edge and copies on select by itself, and *Jump to bottom (click)* is a button. Guild forwarded only the wheel, so a drag past the top edge scrolled nothing and the button did nothing. **Two fixes were made from the code alone** — an edge clock, then a window listener for moves outside the element — and both were right about our grid and useless in `claude`, which has no grid history to scroll. It took a review that read `claude`'s binary to see the layer. A press, a drag and a release go to the program now, and option keeps them ours.
- **A focused element that nobody draws is a dispatch path with nothing on it.** Switch projects with the editor focused and the editor keeps the focus handle behind the other project's tabs; every shortcut then silently does nothing until a click. It reads as "the terminal ignores `cmd-3`". `Window::on_focus_lost` fires when the focus path goes empty, and the root's answer is to focus the body in front.
- **…and the focus-lost rule does not cover a closed tab.** `on_focus_lost` fires only when the focus path goes *empty*; a closed tab's editor kept the focus with a path that was not empty but reached only the component's `Root`, which has no handler of ours, and a new tab takes the focus only when *nothing* has it. So every second or third `cmd-w` did nothing, on 2026-09-24. A probe on `observe_keystrokes` showed it at once — `action=None contexts=[] focused=true` — after a morning of reading the code for it. Closing the tab in front refocuses the new one explicitly; whatever else takes a focused thing off the screen has to do the same.
- **The platform binds Escape to the button it knows as Cancel, and from a string it knows the English word only.** `window.prompt` with `"Отмена"` as a `&str` gave a sheet Escape could not dismiss; `PromptButton::cancel(label)` names it in any language.
- **The menu stays live under a sheet.** A second `cmd-w` while the unsaved-changes question was up queued a second sheet for the same tab, answered after the tab had gone. Ask one question at a time and refuse the rest while it is up.
- **A synced write is twenty milliseconds, and the window made two per tab switch.** M5.3's atomic save — write beside, `sync_all`, rename, sync the directory — ran on the main thread on every change; `sample` during forty tab switches put 719 of 1,036 busy samples in `fcntl`. Saves go through `Store::save_soon` now: one thread, one queue, a load answering from the queue, an awaited Root quit hook flushing off the UI thread — every `cx.quit()` is `guild_desktop::quit(cx)` for that reason. `Store::save` waits for the writer; live scoped settings call it only in their owned background worker, and tests use it when reading disk next.
- **A row in a `uniform_list` is as wide as its content unless told otherwise.** The tree's rows were as wide as their names for a whole pass, and nothing showed it: the selected background ran to the end of the text and passed for a highlight. The first element placed after the name — the git mark — sat right there instead of at the panel's edge. `w_full()` on the row; and a background that stops at the text is the sign to look for.
- **`all_font_names()` enumerates and sorts every installed font, through CoreText, on every call.** Two views resolved the code face from it per frame, and typing in a file spent ten times more on the font list than on the keystroke. The terminal had resolved its face once since M5.1; the lesson was in its code and not here. Resolve a face once, when the setting changes, and hand the name down.
- **Starting an FSEvents stream takes seconds, and the watcher's root has to be canonical.** `notify`'s `watch()` took 2.4 s on this machine; called from the window's loop it would have frozen the window per project, so the platform's watcher is made on the watcher's own thread and `Watcher::start` returns at once. And FSEvents reports real paths: a root under `/var` — every temporary directory — is really under `/private/var`, and a watcher given the symlink matched nothing it was told.
- **`just check 2>&1 | tail` always exits 0.** A pipeline's exit code is its last command's, and `tail` has nothing to complain about. Three commits of M8.1 closed on a red check that read as green until CI said otherwise. Run it as `just check > out 2>&1; echo $?`, and read the code, not the tail.
- **`gpui-component`'s `Root` has to be the window's root view, and it binds `tab` there.** `Root::read` is `window.root::<Root>().expect(…)`, so ours is its child, not the other way round; it writes the window's rem size from its theme's `font_size` on every render, which is kept equal to `ui_font_size`. Its root context binds `tab` and `shift-tab`, and a debug build binds `cmd-alt-i` and `ctrl-shift-i` for an inspector — a focused terminal never saw Tab until `KeyBinding::new("tab", gpui::NoAction {}, Some("terminal"))` unbound it in the terminal's own context. Every context of ours that reads keys itself carries those four unbinds. And its editor ignores the theme's `mono_font_family`: the face, the size and the leading go on the `Input` element, where `refine_style` lets them win. Found in M8.1's task 0.
- **A hover tint in the highlight's colour is a second highlight.** The Recent Projects rows took `surface.raised` both for the row the keyboard had lit and for the row under the pointer, so two rows were lit at once and Enter opened one of them. A list with a keyboard highlight gets no hover tint: the pointer moves the one highlight, on `on_mouse_move` rather than `on_hover`, because hover is recomputed when the arrows scroll the list under a still pointer and would drag the highlight straight back. The same pair sits on the project popup's rows; there the raised row means "current", which is a different claim, and it has not been touched. **A file list's hover is not this trap** (decision 54): its selection is `accent.patina_bg` or the unfocused grey at `alpha::SELECTION_INACTIVE`, and its hover is primary ink at `alpha::ROW_HOVER` — a third, softer colour, so no row under the pointer reads as selected. A list whose hover shares its highlight's colour is still the trap.
- **A marker painted in the background's colour is not hidden, it is waiting.** The brass dot's place was filled with `surface.card` when there was nothing to mark — invisible on a plain row, plainly visible on a lit one. Paint nothing, not the background.
- **Clicks bubble in GPUI, and they fire in the bubble phase.** A handler on a row runs for a click on any button inside it, unless the button calls `cx.stop_propagation()`. It shows up as one gesture doing two things — double-clicking the project popup opened it, closed it, *and* zoomed the window — which reads as a broken button rather than as a handler one level up.
- **A window that draws its own title bar gets none of the title bar's gestures.** Double-clicking it is a system behaviour that comes with an `NSWindow`'s own bar, and what it does is a setting the person chooses (`AppleActionOnDoubleClick`, System Settings → Desktop & Dock). Its default is **Zoom, not full screen** — those are different things, and full screen is `ctrl-cmd-f`. Read the setting rather than picking one: guessing turns a person's deliberate "do nothing" into a resized window.
- **A trailing `flex_1` spacer competes with `flex_1` children.** The project row ended with `div().flex_1()`, which was right while the tabs were sized to their contents — it pushed them left. Give the tabs `flex_1` too and the spacer becomes a fourth tab: with three projects it took a quarter of the row, and the symptom was "the tabs do not fill the width" rather than "there is an invisible element among them". Measured rather than reasoned about — `on_children_prepainted` prints every child's bounds, and it named the cause in one run after the reasoning had said the layout was correct.
- **A default and a choice that share one value give a control nothing to clear.** The Log's branch filter was `Option<String>`: `None` meant both "the log as it opens" and "the person picked HEAD", so *Branch: HEAD* wore a cross, the cross ran, `show_branch_log(None)` found nothing had changed and returned — and the screen did not move. It was reported twice as "backspace does not work", and the key was fine both times; the second fix chased the dispatch path of a key that was already arriving. Three states are three values — `git::Scope` is `All`, `Head`, `Named` — and the giveaway is a state that can be *reached* by choosing but not *left*.
- **A gesture with no tooltip has to speak for itself.** `Commit` returned silently whenever something stopped it — nothing ticked, no message, no panel yet — which was invisible while the only route was a button that greys itself and explains in a tooltip. The moment it got a stroke, "cmd-k does not work" was the report. Whatever refuses a keyboard gesture says why, in the same words the button's tooltip uses. **Since 2026-09-18 `cmd-k` does not commit at all** — it opens the Commit panel with the caret in the message — and the notices stay for the button and `cmd-enter`, which is a stroke with no tooltip too.
- **A context menu belongs to a surface, not to a row.** `gpui-component`'s `ContextMenuExt::context_menu` hardcodes the element's id and its builder-taking constructor is private, so a menu per row is many elements sharing one element state — every row would draw the last row's menu. The row records what the pointer was on with `on_mouse_down(MouseButton::Right, …)`; the surface's builder reads it, which is safe because the component builds the menu in a `window.defer` a frame after the press.
- **A menu item that must act on the row under the pointer cannot be an action.** An action reaches the window's *current* thing: dispatch `CloseTab` from a background tab's menu and the tab in front closes. Items are deeds carrying their target; the keymap keeps actions for what a stroke would mean, which is always the thing in front.
- **An application that is not frontmost eats the click that activates it**, and the gesture that follows a swallowed press is not a drag either. Driving the window, every click needs the app activated first; in a person's hands it is the same rule, which is why a tool dragged from the status bar "does not drag" the first time after reading something in another window. It reads as a handler that was never wired — half an hour went into a branch row that was fine.
- **Editing a file while `just check` runs makes the check test something else.** The run compiles from the disk, not from a snapshot: a task-5 edit landed mid-run and the task-4 check failed on code that was not what the commit would hold. Start the check when the tree is the tree you mean to commit, and do documentation between runs rather than code.
- **The editor component's registry answers *something* for every name, and that something is JSON.** Its grammar feature is off, which reduces its own language list to the single variant `Json` whose `from_str` answers unconditionally, and the lookup falls back through it — so an unregistered name resolves to the JSON grammar rather than to `None` or to `text`. A language whose grammar was forgotten is not plain: it is parsed as JSON and coloured with confident nonsense, silently. `editor::grammar_or_plain` asks the only question that can be asked of a lookup that never fails — whether the name that comes back is the name that went in — and a test walks every row of the language table through it.
- **A diff is rebuilt on every keystroke.** `InputEvent::Change` → `set_working` → `rebuild`, with no debounce, for every file in the tab. Nothing expensive belongs in that path: a parse with its query is 9 ms for three thousand lines, and putting it there would have made typing worse in exactly the file being typed in. The spans are computed on a thread and arrive late.
- **A row coloured from a text that is not its own is a plausible lie.** The rows of a diff come from `git diff` and the spans from parsing the blobs; they disagree over CRLF, over a rev that moved under an open tab, over an off-by-one anywhere. Colouring such a row paints a keyword-red `if` on a line with no `if` in it — no gap, no artefact, nothing in any log. `SidePaint::of_line` compares the row with the line the spans were taken from, one string compare per drawn row. The rule earns its keep twice: it is also what makes spans that arrive late safe.
- **A span is a byte range, and a run must start on a character boundary.** GPUI refuses one that does not, so a Cyrillic comment in a Rust file would take the window down on the first such file drawn. There is a test with one.
- **A test whose example is a real name becomes a tautology when that name arrives.** `an_unregistered_grammar_is_plain_text` used `kotlin` as its example of a name nobody registers, and stopped testing anything the moment kotlin was registered two tasks later. It uses a name no grammar can take.
- **A grammar that extends another publishes half a query, and the other half is a crate away.** `tree-sitter-typescript`'s `HIGHLIGHTS_QUERY` is thirty-five lines — the types, the parameters and the eleven keywords TypeScript adds — and the two hundred it inherits are `tree-sitter-javascript`'s; C++'s seventy inherit C's eighty-one. Nothing says so, and `Query::new` answers `Ok` for the half alone, so a TypeScript file was drawn from M8.1 to M8.8 with its comments, its strings, its numbers and its function names grey, and no line in any log. `Grammar.highlights` is a **list of parts** for that reason, and the test counts painted runs rather than naming four roles.
- **A grammar crate can be unusable in four different ways, and the fourth is not about `tree-sitter` at all.** kotlin's and dockerfile's manifests pin `tree-sitter` 0.20 and 0.21, and cargo refuses two versions of a crate that declares `links` — the grammar is version-agnostic and it is the manifest that is stale, so the parser is vendored and built by our own `build.rs`. just hides its query constant behind a `cfg` its published package cannot satisfy, and proto has the constants commented out — for those only the query is vendored. And two of the four ship no licence text at all, so `scripts/vendor-grammars.py` fetches one from the repository the package's own manifest names; it caught `tree-sitter-just` declaring MIT while that repository carries Apache-2.0. **graphql and hcl ship no query at all**, so theirs come from nvim-treesitter. And **`tree-sitter-sequel` asks for `cc = "~1.2.1"`** — `cc` is a build dependency cargo resolves once for the whole graph, `^1.4` and `~1.2.1` have no version in common, and the resolution **fails outright** rather than degrading; the workspace's `cc` is 1.2 for that reason and section 11 says which crate holds it there. Read every manifest before adding a row.
- **On macOS an accepted socket inherits the listener's non-blocking flag**, so a WebSocket handshake on it reads what has arrived and returns `HandshakeError::Interrupted` for the rest. Treating that as a failure dropped about one `claude --ide` connection in four — and what a person saw was a session drawing its diff in the terminal instead of in a tab, with nothing anywhere saying why. It works often enough to look like it works. The handshake is resumed until it finishes or ten seconds pass.
- **A lock file outlives the window that was killed.** `Drop` does not run for a process that is gone, and `claude --ide` connects only when it sees *exactly one* valid IDE — so a pile of dead locks is a live window nobody can find. `ide::sweep` removes ours whose pid does not answer, at start-up, before ours is written; another editor's lock is never touched, dead or alive.
- **A flag is read once, when the process begins.** The IDE server is started in `Root::new` rather than at the first render because `restore_tabs` runs in the constructor: a window that announced itself a tick later would leave every restored `claude` without an IDE for the rest of its life, and nothing would ever try again.
- **A tool that blocks an agent must be answered by every path that can end it.** `openDiff` waits for a person, so the tab is the answer — and `cmd-w`, the cross and the project closing are answers too. A parked call that is simply forgotten is a session that never moves again, with no error anywhere.
- **`ignore` obeys `.gitignore` only inside a git repository.** `require_git` defaults to true, which is ripgrep's behaviour and not the file tree's: the tree dims whatever an ignore file names, wherever it finds one. A search left at the default would walk `node_modules` in a project nobody had run `git init` in, while the tree showed it greyed out — two rules where there should be one. Found by a test whose first fixture happened to contain a `.git/config` and so passed by accident.
- **A row coloured from a second search of the same text is a lie waiting.** `search::Hit` carries the ranges it matched at, through the cut, so the panel paints from them rather than searching the line again — the rule `SidePaint::of_line` keeps for a diff, met from the other side.
- **A toolbar that cannot wrap loses its controls in a side dock.** Search can be moved to a 300-pixel dock, and five controls do not fit one: a person who turned *match case* on at the bottom would have no way to turn it off. It is the *Branch: HEAD* shape again — a state that can be reached and not left — and the answer is `flex_wrap` and a toolbar that grows.
- **A popup drawn over a focused editor never sees Return, and `capture_key_down` is not the answer.** The capture handler on the root is what makes a popup's letters and arrows work over a terminal, and it is not enough over a **file**: GPUI dispatches a *binding* before both key phases (`dispatch_key_event` matches bindings first), and `gpui-component`'s editor binds `enter` in its own context — so Return became a newline in the file being edited and the capture handler was never asked. Typing and the arrows worked throughout, which is what makes it read as one dead key rather than as a focus problem. **The popup takes the focus, and gives it back when it closes**; a window left focused on its own root is a window whose editor has quietly stopped taking letters. M8.8.2 met it in Recent Files, and it had been latent in the Recent Projects popup since M5.8 — which had never shown it, being opened over a terminal.
- **An element that reads keys has to be the focused one, and there is no second way to arrange it.** The naming field of *New File…* has no editor of its own — its keys arrive through the panel's `tree_key`, which runs only while the panel is focused — and an item chosen from a **context menu** leaves the focus wherever the pointer found it, so the field drew, stayed empty, and the letters went to the terminal behind it. `cmd-n` was fine because it focused the panel by hand. The focus call belongs where all the routes meet. Third time in this crate: the Log's toolbar that lit up and never saw a key, and the tree row that opened its diff before the panel was focused.
- **A grep for a flag that compares whole words is not a grep for that flag.** `["--force", …].contains(&word.as_str())` is equality, and `--force-with-lease` is a different word — so the one spelling of a force push a person coming from another IDE reaches for went straight through the guard written to stop it, from M8.3.3 until M8.8.4 while the documentation said it was covered. Nothing had spelled it, so nothing failed. The match is by substring now, and the lesson is the older one: **a guard is only known to work when it has been made to fail.**
- **`git restore --source=HEAD --worktree` on a path `HEAD` does not have deletes it from the disk** — exit 0, no warning, and the file is gone. So a rollback aimed at "everything ticked" cannot simply be handed every path: an unversioned row gets no item at all and an added one is unstaged instead. And **a rename rolled back by its new path alone leaves the old one deleted in the index**, so both paths go in. Measured against a real git rather than reasoned about, and pinned by tests that run one.
- **A test that drives the git worker may not run git of its own until the walk has landed.** A write answers in three pieces and `index.lock` is still held after the second; a fixture that shells out to git in between fails on a lock, which reads as a broken test rather than as a race the window does not have.
- **A number drawn in the error colour is read as a number of errors.** The Problems tree showed `117` in `accent.rust` beside a status-bar button saying `58`; both were right — the row counts everything beneath it, the button counts errors and warnings — and together they said a third thing that was false. A count and a severity are **two tokens**: a disc in the colour, then the number in `text.faint`.
- **A type that changes what an existing pattern means has to be handed out before the branches are cut.** `Origin` arrived in M8.8.3 and made `Body::File { path, .. }` a question rather than an answer — the path is relative to the project's root *or* to a dependency's. M8.8.2 was written in a worktree where the type did not exist and added six readers of that path; the merge compiled, the tests passed, and every one of them read a crate's `src/lib.rs` as this repository's. Recent Files persisted it, `Back` replayed it, `cmd-shift-t` would have reopened the wrong file. `tests/all/origin.rs` is the guard — a walk of the source for patterns that bind `path` without naming `origin` — and the lesson beyond it is decision 29's: what six branches share is handed out in the seams commit, and a widely-matched enum gaining a field is exactly that.
- **A menu sees rows, and one row is not a row.** The Explorer draws the project's own directory as an ordinary row with the tree's empty path, so its right-button menu offered *Delete* — and `root.join("")` is the project. Confirming would have moved the open project to the Trash. The keyboard route had refused it since it was written; the menu was a second route to the same gesture and carried no such check. **Where a gesture has two routes, the refusal goes where they meet** — `writable_row`, read by `delete_asking` and by `Naming::rename` — and the menu stops offering the item as well, because an item that is merely not drawn is one keymap entry away from being reachable again.
- **`git restore` on a conflicted path succeeds, and that is the danger.** Every other refusal in the Commit panel exists because git refuses; `restore --source=HEAD --staged --worktree` on an unmerged path writes `HEAD`'s side into the index and the worktree, resolving the conflict one way with the merge still in progress and the incoming side gone. *Rollback* does not say that. Where git will not argue, the window must — decision 30, met from the side that is easy to miss.
- **`git apply` places a text hunk by its context, not by its line numbers, and says nothing when it moves it.** Measured on git 2.50.1 for GLD-026: a hunk whose index changed above it applied *at an offset* (`Hunk #1 succeeded at 14`), and a hunk carrying the display's line numbers landed on the identical block twenty lines away with *Applied patch f cleanly* — the index entry exactly the expected one. A check of the entry before `apply` does not close it either: a `git add` from a terminal can land between the check and git's lock. Hunk staging therefore sends the entry's whole new content as a binary patch with a full `index <old>..<new>` line, which git refuses under `index.lock` unless the entry is `<old>` (decision 55). And `apply.whitespace=fix` in a person's config rewrites the staged bytes, so the argv says `--whitespace=nowarn`.
- **`git apply --cached` in a folder of its repository applies only patches spelled from the top under that folder, and skips the rest with exit 0.** A hunk patch naming `f` from `repo/sub` wrote nothing and succeeded; one naming `sub/f` was applied — and a project-relative `sub/f` from `repo/sub` would have hit `repo/sub/f`'s twin at the top. Spell the path with `rev-parse --show-prefix` and read the entry back after the write. Found by GLD-026's code review.
- **`check-attr` prints `filter=unset` exactly like `-filter`, and `hash-object --path` runs whatever clean driver the attribute names.** A check that reads "unset" as safe and then hashes with `--path` can run a person's driver of that name — on an unsaved buffer. Hunk staging asks `hash-object` only with `--no-filters` and accepts no `filter` value but unspecified; line endings are decided from `text`, `eol` and `core.autocrlf`, since content without a carriage return is never changed by them.
- **`-c core.fsmonitor=false` runs a program called `false` on git through 2.35.1**, which reads the key as a hook's path; an empty value is off in every version (unset path before 2.36, a false bool after). And **`GIT_NO_LAZY_FETCH` exists only from git 2.45.0**: before it, reading a missing object in a partial clone fetches it from the promisor remote, so a reader that must stay local checks for a partial clone first. GLD-026, review r4.
- **`StatusWorker::take` keeps only the newest map.** Anything that must see a failed map between an answer and a usable one reads the ordered `hunk_events`, not `taken.map`. Review r4.
- **`git apply --cached` runs a configured `core.fsmonitor`, and blocks on a FIFO where `.gitattributes` should be.** Measured on git 2.50.1: the monitor ran from `apply`, not from the plain reads; `check-attr` and `apply` both hang on the FIFO's open before `index.lock` is taken. A call that must not wait on the monitor passes `-c core.fsmonitor=` — empty, never `false`; see the compatibility note above; a call made while holding the checkout's turn needs a deadline that ends its process group (`ProcessGroup::terminate`: `SIGTERM` first, on which git removes its lock files). GLD-026, review r3.
- **A status read that fails must not empty what the Commit panel remembers.** Replacing the list with nothing cleared the ticks, and the next good map ticked every row as new (`to_tick`). Keep the list and the ticks behind the error note. Review r3.
- **`check-attr <name>` prints `unspecified` both for an absent attribute and for `filter=unspecified`; `check-attr --all` lists only what is set, unset or valued, so it tells them apart.** And a valueless `core.autocrlf` reads back as an empty string from `git config --get` while git takes it as true. GLD-026's eligibility reads `--all` and treats any present, non-false `autocrlf` as on. Review r2.
- **`O_NOFOLLOW` guards only the last component, and opening a FIFO for reading blocks until a writer comes.** A parent directory that became a symlink is followed silently. Open beneath the root component by component (`file_operations::open_beneath`) with `O_NONBLOCK`, and check the descriptor is a regular file before reading — above all while holding the checkout's turn.
- **`InputState::set_value` emits no `InputEvent::Change`.** A reload replaces a buffer that way; anything following an editor by its Change events keeps the old text. Re-read on the file tab's `BufferChanged`, and check the shown bytes against the buffer at the moment they matter.
- **A gpui test's debounce timer does not run by itself.** The editor's marker job waits 75 ms on the background executor's timer, and `run_until_parked` never advances the test clock: a test that waits for markers or hunk offers loops `advance_clock` with `run_until_parked`, or it times out with nothing computed.
- **Guild has three fields that take the keyboard, and only one may be up.** Recent Projects, Recent Files and Go to Line are offered a key in a fixed order by the root's `capture_key_down`, first-come; two open at once is one field and one *drawing* of a field, with a caret blinking in it and every key going elsewhere. Each of the three used to close some of the others and none closed them all. `put_fields_away` is the rule in one place, and every opener calls it.
- **A language server can need something from the project that the client cannot give.** Measured on 2026-09-17:
  - kotlin-lsp answers every definition with `null` in a directory that neither Gradle nor Maven describes;
  - kotlin-lsp exits at once, with an `UnsatisfiedLinkError` on `libfilewatcher_jni.dylib`, while Homebrew's cask is quarantined — `xattr -dr com.apple.quarantine` is the person's decision;
  - ruby-lsp exits with 78 beside a `Gemfile` that has no `Gemfile.lock`;
  - sourcekit-lsp outside a SwiftPM package compiles each file alone: no definition, not even in the same file, and dozens of false *cannot find … in scope* errors per file. An Xcode project needs `xcode-build-server config` to write `buildServer.json`; the row now starts the server only beside that or a `Package.swift` (2026-09-28).

  Each reads as a client that does not work. Ask the server over stdio before blaming the window: a two-file project and a script that sends `initialize` and `textDocument/definition` settles it in a minute.
- **A lock file written by one Ruby breaks another.** A `Gemfile.lock` made by macOS's Ruby 2.6 says `BUNDLED WITH 1.17.2`, and ruby-lsp's Ruby 4 then loads that bundler and dies on `untaint`. It is a fixture's fault, not a server's.
- **A server with nothing to say is not a ready one, and the difference has to be drawn.** Until M8.5.1 a missing command, a server that kept dying and a server that refused `initialize` all drew nothing, exactly as a server that was listening did. And a refusal was not marked as tried, so the next render started the server again, once a frame.
- **Servers run several tasks at once.** expert begins two `$/progress` tokens in one breath. A single "what it is doing" slot is emptied by the first `end` while the other task still runs, and the bar goes quiet as though the server were ready. Keep tasks by token, and remember that `7` and `"7"` are two tokens.
- **A truncated line in a flex column is measured at nothing.** A task row in the servers list drew as a bare `…` until the row and each line inside it had `w_full`. The same `truncate` in a flex *row* was fine, which is what makes this one easy to miss.
- **A tooltip on a button that opens a popup is drawn over the popup.** Drop the tooltip while the popup is up.
- **A style the editor is given outside what it draws shifts every colour after it, and then kills the window.** `gpui-component`'s editor concatenates the styles it gathers for the viewport into `TextRun`s and consumes them **positionally** from the first byte it draws — so a diagnostic straddling the top of the viewport, or the `cmd`-hover underline left on a symbol scrolled off the screen, adds a run for text nobody draws and moves everything after it. Measured on 2026-09-18: `cmd`-hover a three-byte name, scroll past it with the keyboard, and every string literal is coloured three bytes late; with a two-byte name a boundary lands inside a two-byte Cyrillic letter and the process aborts in `shape_line` — *end byte index 8 is not a char boundary; it is inside 'п'*. **The wrong colour and the crash are one bug**, which is why it lived so long: without a multi-byte character on the screen it looks like an imperfect grammar. `clip_styles` in the old vendored crate was the cure. The 0.7.1 migration removes that patch because upstream clips the composed styles; `editor_runs.rs` now reproduces both cases against the actual editor. The lesson beyond it: **anything gathered for a viewport is clipped to it where it is gathered**, because whoever concatenates lengths cannot tell a span from a lie.
- **An icon theme's table is not the whole theme.** The Material Icon Theme's `fileExtensions` has no `ts`, `js`, `yml` or `php`, and its `fileNames` no `.zshrc`: the editor it was made for resolves those by **language id** and the theme answers the language. A client that reads only the two file tables draws the generic page for the most common files there are — and one that took a vendor's subset of those tables drew it for eleven of Guild's languages until M8.13, with nothing to notice. `icons::for_file` asks Guild's language table last. And **an icon's key is not always its file name**: a clone is `<key>.clone.svg`, so `scripts/vendor-icons.py` resolves `iconDefinitions` and copies the file under the key. `every_language_guild_knows_draws_something_of_its_own` is the guard; a language added to `highlight` meets it.
- **`deferred` is above everything, the popups included.** GPUI paints a deferred element after the whole frame, and the popups — Find in Files, the menus, the pickers — are deferred at the same priority, so a grip made deferred to rise above the editor rose above them too: its line cut Find in Files in two. What must be above one sibling and below the popups is **a later child of the same parent**, not a deferred one.
- **Maven colours its answer even into a file.** `dependency:list -DoutputFile=…` wrote `…commons-io-2.16.1.jar\x1b[36m -- module org.apache.commons.io\x1b[m`, and a reader splitting on ` -- ` took the colour code as part of the jar's path — a path to no file, and a row that could not open. `deps::maven` strips ANSI codes and `ask` runs Maven with `-B`; the test holds the measured line. **A tool's output is data only once the colour is out of it**, whatever the flag that was supposed to stop it.
- **A process a check looks for by name may have been renamed by the tool.** Gradle's wrapper is `java -jar gradle-wrapper.jar` now, not `…GradleWrapperMain`; a `pgrep` for the old name found nothing after an edit and read as a re-ask that never came. A probe in the log settled it in a minute. Look for what the tool *is* this version, or ask the window, before believing a feature is dead.
- **`pgrep` on macOS leaves out its own ancestors, so a check run from Guild's terminal cannot see Guild.** `-a` is the flag that takes them back, and the default is off. A terminal tab of Guild is a descendant of Guild, so `just install` run from one asked `pgrep -f /Applications/Guild.app/Contents/MacOS/guild-desktop`, heard nothing, and swapped the bundle under the running window instead of staging it for the Update button — measured on 2026-09-24 with pid 3175 up: `pgrep -f` rc 1, `ps` naming 3175 in the same shell. The refusal before M5.15 had the same hole. `pgrep -a -f` is not the cure either: it matches every process whose *arguments* mention the path, the calling shell among them. `install` asks `ps -axo comm=` and compares whole lines — `comm` is argv[0], which LaunchServices sets to the executable's full path — and pipes into `grep -Fx` without `-q`, because under `pipefail` the SIGPIPE `ps` dies of when grep leaves early reads as "not running". **A check for a process is only as good as the process it is run from**; try it from the place it will actually be run.
- **Saving a project's state made its folder.** `project_state::save` creates `<root>/.guild/`, and `create_dir_all` makes the root above it — so a worktree removed outside the window came back as a folder holding nothing but Guild's state, a folder test said it was still there, and `git worktree remove` failed on it. A project whose folder is not a directory is not written. Anything that writes *beside* a root must first ask whether the root is there.
- **Dependency versions are declared once, in `[workspace.dependencies]`.** Do not substitute `*` and do not "update by eye".
- **The toolchain must be at least 1.88**, which is `[workspace.package]`'s own `rust-version`. It was 1.85 — set by the `config` crate, then kept because `edition = "2024"` needs that much — and **`zip` 8.6 raised it on 2026-09-18**, M8.11. The floor is whatever the highest dependency asks for, and it is declared rather than discovered: a build that works here and not on a colleague's machine is what the field is for.
- **A face registered after the first frame is a first frame in the wrong font**, and the jump is visible. `desktop::fonts::register` runs before anything is measured or drawn. And **the editor and the terminal resolve the code face differently on purpose**: `terminal::font::choose` chases an installed Nerd Font because a shell prompt is drawn with those glyphs, `choose_code` does not, because a file has none — for a while the editor used the first and drew Rust in somebody's patched Meslo.
- **A language server exits if you speak before it has answered `initialize`** — rust-analyzer says *expected initialized notification, got …* and stops. It says it **on stderr**, which is the only place it can: discard that stream and a server that will not run looks like a client that does not work. Nothing is sent to a server that is not `ready`, everything already open is announced the moment it is, and its stderr is logged at debug.
- **A client that never says `didSave` gets diagnostics once.** rust-analyzer's type errors come from `cargo check`, which it runs on save; without the notification the squiggles from startup stay and nothing new ever arrives. This nearly shipped: the first diagnostics look exactly like working diagnostics.
- **Clean is not proof of a write.** Undo and external reload also clear dirt. LSP didSave follows FileTab's successful atomic-write event with its exact Rope, never a dirty-to-clean timer observation. A failed directory fsync must not announce success. A didOpen sent before debounce also becomes the comparison snapshot: returning to earlier text still needs didChange after that open.
- **A receipt revision is not an archive revision.** Cache ordered metadata for unchanged render/idle reads, but admit periodic Activity disk work independently and drain worker replies before its clock check. Otherwise a cheap metadata cache can freeze findings while the archive keeps changing.
- **A provider called from the window's own update panics.** The component may call one because it is a different entity; `Root` asking through its own provider is `cannot update Root while it is already being updated` — the window has to ask the server directly. Same shape as the diff header reading its own tab during a render.
- **A restored tab has no editor until it is first drawn**, so nothing that needs the buffer can be wired where the tab is made — the autosave table met this as "no editor" meaning "not text", and M8.5 met it again as a file the server was never told about. `ensure_documents` on the render is where that belongs.
- **GPUI's quit does not drop what the window owns.** `cx.quit()` is `NSApplication terminate:`, and nothing on that path promises an entity's `Drop`, so a pty owned by a view outlives the application: after `cmd-q` the window was gone and the command the Output panel had started was still running. Anything that must be let go of is let go of in `cx.on_app_quit`, which is registered at the root's first render and does run. Every terminal is stopped there, the tabs' included — an orphaned `claude` mid-turn goes on spending money with nothing to read it.
- **A pty's child is a session leader, and killing it is not killing the command.** `zsh -lc "just check"` **execs** `just` rather than forking it, so the child is `just`, and under it `sh`, and under that `cargo`. `Child::kill` reaches the leader alone and leaves the tree running. `killpg` on the leader's pid is what ends the run — and only while `try_wait` says the child is unreaped, because a recycled pid would take a stranger's process group with it.
- **Stopping is not closing.** The first Stop button dropped the terminal, which does kill the process — the emulator owns the pty — and took the output away in the same breath. A person presses Stop *while reading*: the process is killed through the terminal now, and the grid stays. The comment on that code said the output was kept; the window said otherwise inside a minute.
- **`serde_json`'s map order is a feature flag somebody else turned on.** A `package.json`'s scripts came back in the file's own order, which is `preserve_order` — enabled by something in the graph, and by nothing of ours. An order that changes when a dependency three crates away changes a feature is not an order: sort it and own the answer.
- **A test must never call `std::env::set_current_dir`.** The working directory is process-wide and cargo runs tests in threads: a test that moves the process out from under its neighbours fails them at random, and only under load. It cost one red `just check` that passed on every rerun in isolation. Where the decision depends on the cwd, split it into a pure function taking the path and test that — `desktop::paths::offerable`. The same reasoning already bars `std::env::set_var`, which Rust 2024 makes `unsafe` for exactly this reason.
- **An interrupt fires no hook.** Esc at a permission prompt and Esc in the middle of a turn were both recorded against a live `claude` 2.1.268: no `Stop`, no `PostToolUse`, nothing until `SessionEnd` when the session closes. A status driven by hooks alone stays *working* or *blocked* for the rest of the session's life once the person has stopped it; the tab's own keystroke is the only witness — `TerminalTab::take_interrupt`. *History since 2026-11-06:* the displayed status is the program's OSC 7501 report, which says what it does after Esc; the keystroke now reaches only Codex managed input's latch.
- **`AskUserQuestion` asks for permission too.** It fires `PreToolUse` and then `PermissionRequest` with the same `tool_name`; read by the event's name alone, the permission overwrites the question a moment after it was asked. And `PermissionRequest` carries **no `tool_use_id`**, while `PreToolUse` and `PostToolUse` share one — so a waiting permission cannot be tied to the tool that finishes, and a parallel tool finishing clears the wait. Known, and written into M5.10's plan.
- **curl waits a second before it sends a body over a kilobyte.** It asks `Expect: 100-continue` and waits for an answer, which is exactly the script's `--max-time 1` — and a `PostToolUse` carries the tool's whole output. `-H 'Expect:'` clears it in `guild-hook.sh`, and the receiver answers `100 Continue` to a client that sends it anyway. The test posts 200 KB for that reason.
- **A Codex command hook that is a script is refused unless it is executable.** Codex runs the configured command as a subprocess (in its sandbox), so a `644` script — a plain `cp` of the repo's `guild-hook.sh` — fails with `Permission denied` and the session logs `hook: <Event> Failed`, **even though the script would `exit 0` in your hand**. Guild's own `install()` writes the script `755`, which is why the window never saw it; the trap is for reproducing a live Codex run by hand, where a `cp` keeps the repo's `644`. Use the executable copy, or go through the install path. Same shape as the pty mode: the file system's permission is the gate, not the shell's. Found while live-checking M14.4.
- **Codex hands the event's JSON to the hook's stdin, and it names the harness's own `session_id`.** Measured live on 0.155.1: `SessionStart`'s payload is `{"session_id":"<uuid>","transcript_path":…,"cwd":…,"hook_event_name":…,"model":…}`. That id is Codex's (not Guild's `GUILD_SESSION`, which the script still posts as `session_id`), and lifting it out as `codex_session` is what lets a reopened Codex resume by `codex resume <id>`. Task 0 had said the hook carried the event name only — the JSON is there too, and the script reads it.
- **The `claude` on the `PATH` may be somebody else's.** Another tool can put its own wrapper, such as `~/.superconductor/bin/claude`, first — a wrapper that adds its own `--settings` and posts to its own port whenever its variables are in the environment — so a Guild started from such a shell runs every session through another tool's hooks, and moves that tool's sidebar. A window started for a check runs under `env -i` with a `PATH` that does not have it, which is also as bare as Finder's.
- **A session that cannot capture the screen cannot check the window — say so rather than retry.** `screencapture -l` answering *could not create image from window*, and an activation that leaves another application in front, are a process without the rights, not a broken window; M5.10's window checks went into its plan as a person's for that reason. **And the two rights are separate, which M12 measured**: capturing by window id worked throughout that pass while nothing sent arrived, because `AXIsProcessTrusted()` was false — `CGEvent.post` and `postToPid` report success and deliver nothing, keys as well as clicks, and `CGWarpMouseCursorPosition` still moves the cursor, so a dead click looks exactly like a handler that was never wired. Ask the probe before believing the code: a window that can be photographed is not a window that can be driven. And a window killed with its session leaves `guild-hooks-<pid>.sock` in `$TMPDIR` — `hooks::sweep` takes it at the next start.
- **A window that is not visible does not paint, and a window that is asked answers.** An isolated window lying under others is not redrawn, so two captures of it were byte-identical and showed the old root in front while `guild status` and the saved workspace both named the new one. Since M13 the command is the witness of a check and the picture is corroboration — and a check that cannot drive the window by input can still drive it by `guild`.
- **A window ended by a signal leaves its `claude` children running.** The ptys are let go of in `on_app_quit`, which `SIGTERM` does not run, so every session outlived the window by a moment — or for good, mid-turn. A check that kills a window finds what it left by the socket in the children's environment (`ps eww | grep guild-hooks-<pid>.sock`) and ends them too.
- **A command on the `PATH` is not a command an agent knows of.** `guild` sat on every session's path and a session asked for a worktree would still have run `git worktree add` — measured: the same question answered `guild worktree create demo` inside the window and `git worktree add -b demo ../demo` outside it. What made the difference is `command::SESSION_PROMPT` on `--append-system-prompt`, and it goes **before** the positional task, which `claude` reads as the prompt.
- **A worktree of a trusted repository is trusted.** `claude` 2.1.269 keys `hasTrustDialogAccepted` by the git root canonicalised, and a linked worktree canonicalises through `.git`'s `gitdir:` and `commondir` to the main repository's root — so a session started in a new worktree does not stop at the dialog. A fixture repository nobody trusted *does* stop there, which makes a check of the rules free and a check of a turn impossible; the turn is checked in this repository.
- **A refusal that carries advice is an instruction to an agent.** git refuses to remove a changed worktree with *use --force to delete it*, and the next thing an agent runs is that — beside the window, throwing away what git was protecting. Where git's words go to an agent, the window keeps the words and replaces the advice: `column::refused_removal`.
- **An answer sent before the thing can be read back is a lie to an agent.** `guild worktree create` first answered on git's own line, and a `guild worktree list` straight after did not have it — the list is re-read every three seconds. The answer waits in `pending_settle` until the column's own list agrees, and the list is asked at once rather than on its clock.
- **A cadence justified by measuring one call is wrong once the round grows and the projects multiply.** M12's task 0 timed one `git worktree list` at 7 ms and called asking every three seconds free; M12.4 put a `show`, and a `merge-base` and a `diff` per worktree, into the same ask, and every open root made it. With eighteen roots a focused window with every agent idle cost 10–20 % of a core and as much again in git — about thirty forks a second, none of them heavy. What is priced is the whole round times the roots, and it is priced again when either grows; the measurement belongs beside the constant, where the next person to add a call to the round will read it. The cadence now asks for the list alone and counts only when it moved — `root::git::WORKTREES_EVERY`.
- **State read from the window in `render` is only as fresh as the last paint, and a covered window does not paint.** `window_focused` was set at the top of `Root::render`. GPUI calls the activation observers and then asks for a frame, but a window that is hidden, minimised, on another Space or under a full-screen application has its display link stopped — no frame comes, and the window went on believing it was in front: measured on 2026-09-24, a hidden scratch window with twenty roots kept running `git worktree list` twenty times every three seconds with macOS reporting it inactive, and would have marked the tab in front seen. The flag is written by the observer now (`Root::watch_activation`), and a source test holds it there. Anything a timer acts on belongs to the event that changes it, not to the next paint — and the fewer idle repaints the window makes, the longer such a flag stays wrong.
- **A terminal says what it is; nothing inherited says it for it.** Until 2026-09-12 every tab took `TERM` and `COLORTERM` from the window, and the window from whatever started it — so the person's window, launched from a terminal, looked right, and the same window restarted with a bare environment drew `claude` in **monochrome**, which is also what a `Guild.app` opened from Finder would have done. And it handed on the name of the terminal it came from: `claude` in Guild was told `TERM_PROGRAM=Superconductor`. `Emulator::spawn`, the one door every pty goes through, sets `TERM=xterm-256color` and `COLORTERM=truecolor` and drops `TERM_PROGRAM` and its version.
- **`gpui-component`'s menu fixes its measures in its own code.** A row is `px(26.)` and its text `text_sm()` inside `PopupMenu::render_item`, where no theme reaches — so a menu of 32-point rows could not be had from it, and the right button's menu is Guild's own since M12.2 (`view::popup`, `view::menu`). Read a component's source before promising a look it would have to give.
- **A place written at prepaint is read a frame late, and one cell for many writers keeps the last.** The context menu's submenu hangs from its row's bounds, recorded by a `canvas` — every submenu row wrote into the one cell, so *New*'s items hung beside *Git*, and a submenu opened by the pointer was placed from the frame before and stayed there until something else redrew. Only the row that owns the submenu writes, and a write that moves the place calls `window.refresh()`.
- **A fixture window opens over the person's, and their clicks land in it.** The check windows start at the same centred place as the person's own Guild, on top: during M12.2 a click of theirs closed the name field a check had opened and opened a project's menu instead. Keep a check window up for seconds, not left open, and read a capture that disagrees with the state the check set as the person's hand before reading it as a bug.
- **A state a gesture would reach can be set for a photograph, and the hook must not be committed.** With `AXIsProcessTrusted()` false nothing can be clicked; M12.2 opened its menus and fields for the camera through a hook behind an environment variable in `render`, built, captured and removed before the commit — the file restored from a copy taken before the hook, and a grep for its marker run before `just check`.
- **A view that can live in a dock measures the dock, not the window.** The Push tab decided whether its files column fit against the window's width, which was right while it could only be a tab; put in a 620-point side dock it drew 320 points of files beside a list of 300, and the author and date columns squeezed every summary out of it — rows with no text and nothing in any log. The Log has asked `dock_of` for its width since M8.3.1; a view that becomes a tool asks the same question.
- **A linked worktree's `.git` is a file, and a watcher of the root hears no commit.** `watch::Watcher` watched the project root and read a change under `.git` as a commit or a stage — which in a linked worktree never comes: its `HEAD` and index are under `<common>/worktrees/<name>` and its refs in the common dir, both outside the root. From M12 on, every worktree's marks, Log and Push stood still after a commit or a push from a shell, and nothing said so. The watcher asks git for both directories and watches them too, reading what it hears there as the same file under `.git`. Found after M12.3, when a push from a terminal left Push offering what it had just sent — which was a second bug in front of this one: nothing asked Push again when a ref moved.
- **With no tab open, the window took the focus back on every frame.** `render` focused its own handle whenever there was no body, which was right while the panels were empty and wrong from M8.1 on: a tree clicked, a Log field or a panel pressed held the focus for one frame, and its keys went to the window. It hid behind the fact that a window almost always has a tab. Found by M12.3's stretch, whose strokes reach only the panel the focus is in; focus inside a dock is left alone now.
- **A stroke that acts on the focused panel needs opening a panel to focus it.** M12.3's stretch worked for a hand that had clicked into a panel and for nobody else: a panel opened by its button or `cmd-1` left the focus in the tab, and the stroke went to the terminal. Checked through a hook, it had looked right, because the hook focused the panel itself. A check of a keyboard gesture starts where a person starts — from the tab, through the gesture that opens the thing — and `window.dispatch_keystroke`, deferred out of `render`, is how a check presses a stroke through the keymap when nothing can be sent to the window.
- **A window opened during a frame frees the frame's elements.** `cx.open_window` draws the new window's first frame at once, and a frame clears the element arena that every window shares. So a window opened from inside the main window's render segfaulted GPUI while it dropped one of the main window's elements (`EXC_BAD_ACCESS` in `Rc<Cell<bool>>::drop` under `request_layout`). This was the first try of M8.3.6. `Root::open_satellite` defers the open with `cx.spawn`, and remembers what it is opening so that a second ask meanwhile opens nothing.
- **A satellite is built and drawn outside `Root`'s update, and told of changes by id.** Two panics met on M8.3.6's first window checks:
  - `open_window` builds the new view and draws it at once. A builder called inside `root.update` that reads `Root` is a double lease (*cannot read … while it is already being updated*).
  - Push Commits' button calls into `Root`, and `Root` then called `dialog.update` to tell it something changed. That is an update of the dialog inside its own update.

  `open_satellite` opens from a spawned task, outside the update. `notify_push_dialog` is `cx.notify(entity_id)`, which leases nothing.
- **A satellite's size is its content's.** It has the system's title bar, and `open_window` takes the size as the content's. Saving `window_bounds()` saved the frame, so each reopen made the window 32 taller. `remember_satellite` saves `viewport_size()`.
- **The application quits when the main window closes, not when the last one does.** Before M8.3.6 the rule was `cx.windows().is_empty()`. A Push window left open would then have kept alive an application whose main window was gone. `windows::quits` compares window ids.
- **Two writers of one state file rename each other's scratch file.** The window saved its settings both ways: a dragged dock queued a save (`save_soon`), and every frame's terminal fit wrote one at once (`save`). Both wrote `settings.json.tmp`, one renamed it away, and the other failed with *No such file or directory*, which the person saw as *Settings were not saved* while dragging. `Store::save` now goes through the one writer thread and waits for it, and a test runs the two side by side 300 times. **What changes every frame is queued**: `remember_window` uses `save_soon`. **The queue is process-local.** On 2026-11-04 four isolated processes saving one workspace reproduced the same ENOENT despite that queue. Each save now owns a unique scratch in the destination directory, synced before publication and removed on failure; it never consumes a fixed `.tmp` another build may still own. Publication errors name the operation and scratch path before the queue reduces them to a reason for the window. This prevents scratch collisions, not stale whole-snapshot overwrites between processes; no cross-process merge is promised. Historical workspace ENOENT logs alone do not identify the competing process or prove this was their cause.
- **A chain's row is renamed by the fold that makes it, and a selection by path is left behind.** M8.10 draws `cfg` and its only child as one row called `cfg/guild`, by the deeper path; expanding `cfg` renames the row and collapsing it renames it back. The selection is a path — so that it survives the list changing shape — and after a click on `cfg` it named no row: nothing was lit, and `down`, finding no current row, went to the top. The person reported it as *the arrows do not work in directories*. `Tree::row_covering` finds the row whose chain runs through a path, and `TreeState::settle_selection` moves the selection onto it — or onto its nearest drawn ancestor — after every fold, every batch from the disk, and before every key.
- **A handle that takes room beside a panel moves the panel's middle.** The side grip was a strip of nine points beside the panel with its line at the far edge, so a panel's content was centred nine points left of where the eye put its centre — the person found it under the commit box, measured at 6 against 15. A side grip hangs over its edge now, **the last child of the row that holds the work** (`view::panels::side_grips`), so it is drawn after the work and occludes it — laid under the editor it would hand a press to both, and the editor would begin a selection. And nothing lights under any handle any more — the cursor alone, by the person's word.
- **A rounded card clips its children to its rectangle.** GPUI's content mask is a rectangle, so a child that paints a ground to the card's edge paints the card's corner square — and the colour it leaks is the card's own, which reads as a crooked corner rather than as a child. It was met three times on 2026-09-18: the body's ground, the bottom dock's panel, then a file tab's root after both of those had been fixed. Taking the ground off each child is a rule the next child forgets; **the card closes its corners itself**: `view::panels::card_corners` draws a band of `surface.panel` outside the arc and the card's border, **last, over everything**. The card has no border of its own and a point of padding instead, because an absolute child is placed inside a border, and the two outlines stood a point apart.
- **An svg is painted only in a colour set on the svg itself.** GPUI's `Svg::paint` reads `style.text.color` of its own style and paints nothing when it is `None`; a parent's `text_color` and a component button's foreground reach text, never an svg child. The slot keeps its size, so the defect is an empty space with no error: GLD-053 found the idle Sessions tick, five Terminal Jobs icons, the diff window's file-list chevrons and tick, and a repository icon in the agents menu all blank. Proven by one change: adding `.text_color(..)` to the tick's svg, the parent's colour left in place, made the tick appear. Set the colour on the `icons::*` element itself; a hover colour is its own `group_hover`, which works for an svg because its colour is read at paint, unlike text's.
- **`group_hover` cannot recolour text.** A text colour is fixed when the text is laid out, and a group registers its hitbox at paint, which comes later. So `group_hover(|s| s.text_color(…))` on a tab's name never showed, while `group_hover(|s| s.visible())` on the project head's chevron did: visibility is read at paint. M12.6's tab strip keeps the hovered tab in `Root.hovered_tab`, set by `on_hover`. Test hover-dependent text in a window rather than assuming it.
- **GPUI 0.2.2 has no `opacity` on an element**, only on a colour. A fade or a pulse is the colour's alpha — `view::kind::mark_colour`. **Nor has it letter spacing**: M12.4's design tracked a dock head's capitals by 0.06 em, and the capitals go untracked — read the styled trait before a design promises what the text system cannot draw.
- **An absolute box with no inset sits where it would have flowed.** A `canvas` given `.absolute().size_full()` as the last child of a column recorded bounds whose origin was the column's *bottom* — after the body — and M8.15's first balloon hung at the foot of the window rather than over the name. `.top_0().left_0()` pins it; a measuring canvas needs both.
- **A check window started from a directory opens that directory.** With an empty workspace the window offers the folder it was started in, so every fixture photograph of M12.4 carried a stray group — the repository checkout, then the fixture's parent — until the script started the window from `/`.
- **A menu opened from inside a popup must not put the popup away.** `open_menu` calls `put_fields_away()` by default, which clears `Root.picker` — so Find in Files' own chevron, opening its history menu over the Find in Files popup, closed that popup in the same click that opened the menu meant to sit over it, M12.7 task 7. `Target::FindHistory` is the one exception: opening it clears only `context_menu`, leaving `self.picker` alone, and `refocus_after_menu` gives the keyboard back to the popup's own card rather than to the tab in front when the history menu closes.
- **`open` hands the caller's environment on, not only what `--env` names.** M5.15's end-to-end check restarted a scratch Guild through the helper's `open -n --env GUILD_STATE_DIR=…`, and the new process carried the check hook's own variables as well — which on macOS 26.5 is what `open` does from a shell. A hook that acts once per process acts again in the process it restarted, and a check that clicks Update would loop if the build were still offered. Name every variable a check sets, and expect it in whatever that check makes `open` start.
- **A view's `cx.notify()` redraws the whole window, so a clock that notifies must ask whether its picture changed.** The root view is not cached: a terminal tab's notify is `Root::render` again — the column, every group, the workspace page. Each terminal tab's 16 ms poll notified on every caret phase, twice a second, whether or not a blinking caret was drawn — an unfocused tab's hollow block, a tab behind another, and `claude`, which hides the cursor. On 2026-09-24 two idle `claude` tabs in a focused dev window were 246 notifies and 161 frames a minute, 5.6 s of CPU against 2.0 s once the paint says what it drew (`blink::Repaint`). A clock's notify is gated on what the last paint showed, not on time passing — and since the person's decision of the same day an inactive window's caret is steady, as other terminals' are, so a focused shell in a window behind another app asks for nothing either (`blink::Blink`; GPUI refreshes the window on every activation change, which is what restarts the phase from the visible half). **And an occluded window does not draw at all** — GPUI stops the display link — so a count of frames is taken with the window visible, and a count of notifies is the one that does not depend on what covers it.
- **A root that draws every region inline rebuilds all of them for a change in one.** A view's notify marks it and every view above it dirty, and `Root` is the window's root, never cached — so a terminal printing in the tab in front was `Root::render` on each of its frames, and the column, the docks, the strip and the status bar were built, laid out by taffy, prepainted and painted again for a picture in which only the terminal moved. Measured on 2026-09-24 in a release build with twelve projects and two docks open: **2.7 ms a frame, of which the terminal was 0.24 ms, the column 0.7 and the bottom dock 0.34**. GPUI's own remedy: a region is a view of its own handed over as `AnyView::cached`, reused until it is notified — `view::region`. Three things come with it. **The cached style is the region's layout node**, so it must size the region exactly as its drawing does — one accessor for both (`Root::column_width`). **A region observes the root**, and so a `Root` handler that changes what a region shows and forgets `cx.notify()` is now a stale region rather than something the next terminal frame papered over. **A region may read only `Root` and the views drawn inside it** — another entity's notify reaches no region, which is why the strip and the status bar, which read file and editor entities, stay inline.
- **Reading an entity while the window draws ties the window to that entity's every notify.** GPUI clears its set of accessed entities at the start of `Window::draw`, records every `read` and `update` until the draw ends, and from then on a `notify` of any of them invalidates the window — and the root is not cached, so that is the whole window again. `Root::render` called `sync_from_views`, which updated every terminal to ask what it ran in front and read every stream view for its cost, and the strip and the agents counter read every terminal's title and agent. Measured on 2026-09-24 in a release build with twelve projects: two shells printing in tabs nobody could see kept a window with nothing changing on it at **20–35 frames a second and 10.3 % of a core, against 2.8 % idle**. What a frame needs from a terminal is published beside it — `terminal::facts`: the title, the agent in front, the exit — and the terminal tells the root **by id** only when a fact changes what would be drawn; `title::of` decides that, so `claude`'s animated marker is not news. `session_tab_name` and `open_agents` take no `App`, so the compiler holds them, and `a_frame_binds_no_session_view` greps the rest. **A read outside a draw ties nothing** — the watch loop may read every tab, which is where `sync_from_views` runs now. The tab in front is the exception by nature: it is on screen, and its output must redraw.
- **A notify anywhere inside a region rebuilds the whole region, because GPUI dirties every view above the one that notified.** `Window::mark_view_dirty` walks the notifying view's ancestry, so a cached region is reused only while nothing drawn inside it notifies either. Scrolling a `div` notifies the view it is drawn in: a wheel step over the column was the column region and the root dirty — every group built, laid out by taffy and painted again, linear in all the rows and not in the ones on screen — and so was every tick of a working ring's clock. The tab's terminal, an uncached child of the root, repainted its whole grid with each of those frames. Measured on 2026-11-23 with forty repositories, two hundred rows and 480 session tabs on the test platform: **a scroll frame 111 ms and a notify of the root 56 ms, three quarters of it taffy; with the far items as spacers, 21 and 10.** Two cures: the column draws what is far from its viewport as spacers (`view::column::spacers`, the next trap), and the terminal body is a cached view laid out by `terminal::view::fill`, so a notify of the root or the column replays its grid — GPUI's `reuse_paint` carries the input handler, the window's mouse listeners and the cursor styles over, and `window.refresh()` (a focus change, a font size) bypasses every cache. **A cached view is dirtied only by its own notify or a view drawn inside it**: an entity it merely reads notifies the window, the root renders, and the cached view is replayed unchanged — `InputState` is safe because the component draws it as a view, a model read in a terminal's render would not be.
- **Keys handed over while the window draws do not repaint a cached view.** `window.focus` refreshes the window only outside a draw, and `Root::render` moves the keys there — `focusing_front`, a closed pane's, an empty path's. A cached terminal not otherwise dirty then replayed the paint it made without the keys: no platform text client was registered, so text or a composition the platform delivered before the next repaint had nowhere to go. A render that moved the keys draws the terminal bodies whole for that frame (`Root::focus_moved_in_draw`). The same family: **whatever clears what a terminal shows must notify it** — a refused IME commit cleared the composition and notified only the root's warning, and the cache replayed the underlined text; a terminal hidden by its host or by a covered window clears `painted`, so its output asked for no frame, and shown again it must ask for one itself (`refresh_policy_and_repaint`, from the host's report and the window's visibility, activation and bounds). Every way a composition is dropped outside a paint goes through `clear_composition`, which repaints when one was on the screen — a refused commit, the commit before a paste, an unmark.
- **A spacer's height is trusted only while the item's shape holds.** `view::column::spacers` keeps one viewport of margin above and below; an item further out is a `div` of the height the scroll handle laid it out at last frame, at its own index, so `bounds_for_item`, `scroll_to_top_of_item`, `sidebar_speed_rows` and the drags keep counting along the same children and `max_offset` does not change. Its limits are the rules: a `Shape` must name everything that decides an item's height — a test build panics when a drawn item moved under an unchanged shape, production logs and draws the next frame whole; from the first item whose shape changed, everything after is drawn, since every position after it may have moved; another column width, interface size, font, language, display scale or window height draws everything — a fractional line snaps to another height at another scale, and a window grown past twice its height shows rows no plan covered; an item being renamed is never a spacer. **A box's bounds leave out its margins**: an expanded group header with no folder reaches six points into the repository under it with a negative bottom margin, and a spacer that copied only the height moved everything after it down — `head_overlap` is the one rule for both. **The plan reads last frame's offset and viewport; the prepaint decides this frame's.** A reveal moves the offset, GPUI clamps it into the list — a wheel that overshot either end, a list that became shorter (Escape out of a search that opened a long collapsed group) — and the viewport grows when what is above it goes (unpinning against Pinned's 40%). So the plan draws whatever any offset the clamp can reach and any viewport up to the window's height can bring into view, and a reveal into items that changed — a search opening a collapsed group at the end — draws everything, having no bounds to aim by. **A scroll made inside the list's prepaint is one the plan did not foresee**: `scroll_to_top_of_item` without `Spacers::reveal` paints one frame with a spacer in view and asks for the next — so whoever scrolls the handle that way tells the spacers first, as the speed search does. And a spacer drops its children's element state — a hover, a tooltip's timer — as an item leaves the margin, as any virtual list does: a row never hears the pointer leave it, so what it owned is let go of when the plan spaces it: the worktree actions' shared tooltip if this row's action asked for it (`worktree_tooltip_owner`; another row's spacer leaves it alone), and Root's `hovered_worktree` or `hovered_column_tab` if they name the row — kept, they came back with the row as stuck actions over hidden diff counts. Heights are counted, not named: `column_tab_counts` walks the tabs once with the rule `column_tabs` lists by, because a `Shape` is asked for every item on every frame.
- **A GPUI test window draws itself.** In `GpuiMode::Test` the end of `flush_effects` draws every dirty window, so an `update` that notifies returns after a whole event-to-frame cycle — what `view::column::perf_bench` times — and a notify deferred from a prepaint with `cx.defer` is drawn within the same `run_until_parked`. `window.on_next_frame` is not: its callbacks wait for `simulate_next_frame`. A count of renders cannot tell a blank frame and the one that repaired it from one good frame; count what was painted instead (`spacers::BLANK`, `TerminalTab::paints`). And `simulate_keystrokes` falls back to inserting a key's text through the input handler when nothing handles the key: a test of a key the terminal sends itself dispatches the `KeyDownEvent` and has the program answer only the byte it expects, with echo off. **The first key-down after the mouse repaints everything**: GPUI switches to keyboard modality and refreshes, and `dispatch_key_event` draws a dirty window before dispatching — so a test that means to type into a replayed frame switches modality first, then checks in one update that the input arrived and nothing was painted. And a test that reaches `adaptive-terminal-refresh` code is gated by the feature: `cargo test --no-default-features` builds the same binary without it.
- **Unchanged state is not written, and the disk is what is compared.** `persist::save` hands every open project's state to the writer on any change; `Store`'s writer leaves a file alone when it already holds the bytes and a `.bak` is beside it. Comparing with what this process last wrote instead would miss a file another build or a person changed; skipping before a `.bak` exists would leave a truncated file nothing to fall back to.
- **A copy of a system binary is killed the moment it starts.** M5.16 faked an installed Guild for `_running` by copying `/bin/sleep` to `<scratch>/Guild.app/Contents/MacOS/guild-desktop`: it died with 137 before `ps` could see it, since a platform binary's signature does not hold at another path. A two-line C program built with `cc` into that path runs, and `ps -axo comm=` names it by its full path, which is the whole of what the check reads.
- **GPUI's `traffic_light_position` is the top of the buttons' frame, and the frame's height is the system's.** `main.rs` put it at 11 for 12-point buttons, to centre them in the 34-point bar; on macOS 26.5 the frame is 14 (read through AppKit: `standardWindowButton(.closeButton).frame`), so the lights sat a point under everything else in the bar and the person saw the bar's buttons as riding high. Measure the frame on the system in hand before trusting a comment's number. (The next day the whole bar, lights included, went a point lower on purpose — optical centring, section 6.9 — which is a decision, not this trap.)
- **A line wraps only against a width its element has.** The card over Update had `min_w` and `max_w` and no width: GPUI laid each entry out as one line and painted the wrapped rest over the entry below. A notice, which wraps correctly, has `.w(…)` on the card and `flex_1().min_w_0()` on the text — that is the shape.
- **A Keychain item read through `security` is granted to `/usr/bin/security`, not to Guild.** M12.10 reads Claude Code's OAuth token with `security find-generic-password -w`, the person's choice over a crate. macOS's prompt then names *security*, and *Always Allow* adds that tool to the item's access list — so any program that runs `security` reads the item without asking from then on. *Allow* asks again at the next read, which is why Guild holds the token in memory between requests instead of reading it every five minutes. A crate calling the Keychain API would be asked for as *Guild*; that trade is decision 46's.
- **A way into a project picks its tab through `Root::come_back`, never `visible_tabs().last()`.** Eight paths — the column, `ctrl-tab`, `cmd-1`, Recent Projects, a workspace switch, a seat brought forward, a project closed, a root moved — each set the tab in front to the strip's last, so four tabs with the second in front came back with the fourth. Under it, `store_tabs` wrote the front only for the project in front and `None` for every other one it saved, so a restart lost them too. `desktop::fronts` kept each seat's front, and since M12.8 it is one per area, through `desktop::stage`; `save` writes it down by the tab's own seat, and a new path that writes `last()` by hand brings the bug back. 2026-09-26.
- **A save while a workspace this run has not restored is active writes its projects back empty.** `store_tabs` writes every project of `active_workspace` from `Root.tabs`, and tabs are restored lazily, on the first visit (`restored_workspaces`). `command_session_start` made a workspace active in the model alone to record a background session's tab, the record's save ran, and every project of an unvisited workspace lost its saved tabs and areas on disk — the one the session went into kept only the new tab (GLD-058). Restoring that workspace in the background instead was the first fix and the wrong one: it is a workspace switch the person did not make — it puts their open fields away, restores at a moment they did not choose (a volume not mounted reads as a project with no tabs, a file on another branch drops its tab, and the workspace is then marked restored), and the switch's save hands the front workspace's project to the other's `Desk::look_at`. A tab for another workspace goes into its named seat (`open_tab_in_seat`) and is written by `store_behind`: from memory when that workspace is restored, else added to its own project's file alone; the visit's restore finds the session already running and gives that tab its place rather than starting a second process. The file is changed only from a read of a folder that is there on both sides of it (`project_state::load_for_change`), a seat from before tab lists gets its open records written first, and a session closed meanwhile — `guild agent stop` — leaves the file again (`forget_behind`); a restore never reopens a closed record a strip still names. A test reads the projects' files back after `guild_session::flush()`, because the stage in memory — and `project_state::load`, which answers the writer's pending snapshot — look right either way. 2026-11-24.
- **A seat held is not a seat read, and a save that cannot tell them apart writes an empty one.** `persist::save` replaced a root's records on disk, workspace by holding workspace, with what memory held — and two ways leave a seat held with none of its records in memory: a root opened again in a workspace that had closed it (`Desk::open_in`, which reads nothing; `guild session new` and `guild open` never ran `adopt_project`, and `adopt_project` skips a root any *other* workspace has records of), and a folder away when the desk was read (`project_state::load` answers an empty state for it). Either way the first save after it dropped that workspace's conversations from `.guild/state.json` while its strip still named them, and the visit then saved the shorter strip — measured: in the second route the very save that a restart makes once the folder is back did it, before any command ran. `Desk.loaded` is the in-memory fact of which seats this run has read; `persist::save` first reads the unread ones whose folder is there (`persist::read_unread`, also asked before a workspace's restore) and replaces only read seats' records, so one still away keeps its file. Two edges of that rule: a late read skips a record this run removed (`Desk.removed`) — the desk held it from the global file while its folder was away, and the file's older copy is still there; across a restart that fact is `Desk.read_away` in the global file, the seats a save sent to the global file — folder away or its file refusing the write — and had read, whose records there are whole and win over the folder's copy; and *read* means a file that read, not a folder that was there, or a file that failed with `EMFILE` would be written down as a whole empty seat — and a move does not carry the read mark to its destination, whose own records from before it closed the root are read first. A test of this reads `.guild/state.json` itself after `guild_session::flush()`: the writer's pending snapshot is what `project_state::load` answers, and it looks right either way. GLD-060, 2026-11-26.
- **A dev window with its own state rewrites the person's Codex hooks.** `install_codex` writes the block between `# >>> guild hooks >>>` and `# <<< guild hooks <<<` in `~/.codex/config.toml` with *this window's* script path, and `just desktop-dev <dir> <state>` puts that script under the scratch state: after M13.3's window check every Codex the installed Guild started posted to a script in a scratchpad about to be deleted. The installed Guild puts its own path back at its next start; until then, put it back by hand — only the paths inside the block — and say so. Seen 2026-10-01.
- **Running an agent CLI at all is what makes it "known".** GLD-078's rule is that Guild writes an agent's status door, and runs it in the background, only where its own directory exists — but `opencode --version` (v2.4.2), measured with an empty `HOME`, itself creates `~/.config/opencode`, `~/.local/{share,state}/opencode` and `~/.cache/opencode`; `codex --version` (0.172.0) creates nothing, while `codex app-server` creates `~/.codex` and its databases. A new background call of an agent at the window's start brings back the 109 MB first-run download GLD-074 found; ask `guild_session::harness::known` first. The OpenCode floor probe runs only when the person opens an OpenCode tab, and is left as it is. 2026-11-27.
- **An Enter sent blind into a harness's first screen can choose something else.** `claude`'s trust dialog stands on *No, exit* — the Enter a probe script sent to trust a scratch folder refused it, and every session there then stopped at the dialog with no transcript. `codex` may put an *update available* screen before its trust question, and an Enter there started `brew upgrade` (M13.3, 2026-10-01: stopped before the cask moved, the taps refreshed). Read the screen after each key: move to the option by arrow or number, and look again before Return.

- **A Codex TUI can inherit the right Guild environment while its shared daemon holds another window's.** On 2026-10-02 the 0.159.3 TUI used a 0.160.0 daemon whose `GUILD_SESSION` and hook socket named an old window. Real turns completed while the new window heard nothing. Guild launches Codex with `--no-daemon` (also supported by `resume`) so its hooks and task provenance stay with that terminal. A silent session is unknown, not idle; never stop it on that evidence.
- **Codex can write `[hooks.state]` inside Guild's hook markers.** The block's comments are not TOML boundaries. Replacing everything between them erased trust on refresh. Preserve those tables verbatim, test only private config paths, and never manufacture trust hashes. Codex writes them while it runs, so an install compares what it read: a block already there writes nothing, and a changed block is renamed in only over the bytes it was merged into, else the file is read again (`install_codex_with`) — writing a merged snapshot over a file re-read only to compare with the new text put a running Codex's trust back, GLD-078's review.
- **A stored completed turn is not a current process's completion.** The Codex rollout cursor is baselined before resume; only newly appended lifecycle records move its status. Assistant text and a quiet file are not `task_complete`, and a large backlog must be consumed before publishing its status. Reading any harness first resolves the window's conversation ID; neither Guild's UUID nor a newest-by-folder guess identifies a Codex/OpenCode conversation.

- **A conversation identity is not a status signal.** Codex's `SessionStart` can name the conversation before a turn happens, while `from_hook` correctly returns no turn status for it. Capture identity from the emitting pane before filtering by status, and register the start hook. A saved record missing the harness's id must remain visible rather than launch a new conversation under the old tab. 2026-10-03.
- **A late OSC 7501 answer is a "no" for the whole session.** Claude Code probes with `OSC 7501 ; ?` then `CSI c`, and settles support once: a DA1 reply that arrives before the 7501 reply means unsupported until it restarts (claude-code issue 105534). alacritty answers DA1 while it parses, so the probe is answered by the reader before the chunk is parsed (`terminal::program_status`), in a tab that was never drawn too. A probe answered from paint or after `advance` looks fine in a front tab and silently loses every hidden one. 2026-11-06.
- **A workflow submission binds the conversation at the turn's start.** The start is now the board's first `working`; for Codex and OpenCode the conversation id comes from a hook, which can be admitted a poll later — even a hook answered before the report was written can land after it. A start sent unbound is rejected by the store for good, so `Root::workflow_prompt_submitted` holds it in `unbound_starts` and `conversation_admitted` sends it once the id is recorded (review r1, 2026-11-07). Don't "fix" this by ordering a fixture: the window must not depend on the order.
- **Replacing a bundle can preserve stale icon metadata.** `ditto` preserves directory timestamps, and every nightly used the same `CFBundleVersion` and icon filename. Give icon content its own resource name, version builds separately from the marketing version, and touch only the replaced bundle after copying. Never clear global icon caches from the updater. 2026-10-03.

### A component Button label can overflow a constrained navigation row

`gpui-component::Button::label` places text in an inner `flex_none` div inside
its own centred label container. A width on the outer button does not truncate
that text, and an empty label still leaves a child beside a custom card body.
For a constrained Review navigation row, use a child with an explicit available
width and align the button at the start. Single-line controls can truncate with
a full-label tooltip; historical cards instead wrap the title and grow with it,
keeping the recorded date and outcome on separate lines. Do not pass an empty label alongside custom card contents. Keep
this adaptation in Guild; the vendored component is not patched for it.
- **ureq makes a status an error and drops the headers with the body.** `oauth/usage` began answering a token Guild had held for days with 429 on 2026-10-01, while the Keychain's current token got figures at the same minutes; the log said `Request("http status: 429")`, the card said *couldn't read*, and the clock and every card opening went on sending the same token. `Error::StatusCode` keeps the number and nothing else, so `Retry-After` was never seen. The request is built with `http_status_as_error(false)` and the status is read by `status_failure`; a 429 lets the token go, pauses the asking (`usage::rate_limit_pause`) and is said as *rate limited* over the figures kept.

- **A failed Keychain command launch is not missing credentials.** Converting `Command::output()` to `Option` made a temporary resource failure stop usage polling until Refresh. Keep launch errors retryable and reserve the stopped state for a failed Keychain response or absent token. The titlebar chip remains present with `-` even when neither provider has figures, so recovery controls stay reachable.

- **A full review goal is data, not a heading.** Long manual-review instructions once consumed most of a short native window and squeezed its fixed stage canvas away. Keep the heading bounded, disclose complete task text in a bounded scroller, and scroll the canvas/inspector together on narrow viewports. Headless GPUI bounds and click checks cover the small-window regression; browser snapshots alone do not prove native parity.
- **The desktop executable is not the CLI, and the socket selects the window.** Passing `instructions` or `review` to `guild-desktop` used to open an unintended window. Reject arguments before platform startup and use its sibling `guild`. An inherited harness `NO_COLOR` also reaches resumed terminal programs: remove it only from the explicitly authorized dev-window child environment, preserving Guild caller identity.

- **Review callers must follow resume bindings too.** A terminal keeps its launch-time `GUILD_SESSION` after an in-TUI resume; `started_as` maps that identifier to the current live record. Review and overlay commands once bypassed this map and refused the closed original record. Resolve through the window-owned binding before checking project, role and review ownership. Codex subprocess policy pins only this launch’s Guild variables; missing identity is never repaired with another session’s ID or `env -u GUILD_SESSION`.

- **Compact observers must budget height for their fixed controls.** A non-shrinking participant list overflowed the stage and budget footer; a second, unapproved Configure review row compounded the overlap. Keep compact participants in a flexing, minimum-zero scroller, and colour custom SVG button children explicitly as other Guild toolbars do. CLI show must retire an existing peek before exposing the same task's pinned host.
- **macOS printenv checks one variable at a time.** Supplying several variable names returned only the first with exit zero, falsely suggesting Guild sockets were missing. Check each name separately; successful socket connection and owner-authorized commands are separate evidence from environment inspection.

- **History is recorded results, not a profile editor.** Sharing the configuration canvas with historical selection left only a fixed 168-pixel activity strip and hid the reason for a failed run. Reserve the fixed canvas for profiles; recorded activity fills the remaining viewport, includes archived errors, and labels each navigation entry with its creation time and recorded state. Keep event wrappers non-shrinking and the event column minimum-zero/full-width. A selected node's border belongs outside its hoverable Button, whose own hover style can erase a custom border.

- **Button size and content height are independent.** Multi-line history rows inside a Small Button still reserved a single 24-pixel row, overlapping adjacent records; use intrinsic height and non-shrinking list items. Test the actual Button with long three-line content. Truncated headings also need non-shrinking, full-width text rows.
- **Hover observation needs pointer truth, not just nested hover callbacks.** Child controls can interfere with card hover notifications. Capture pointer movement against the actual card bounds, cancel dismissal on entry, and never dismiss while the pointer remains inside. Keep the trigger's own hover and observed state visible.

- **Project Settings is a tab, not a global input mode.** Only its active project-owned tab may route settings keyboard actions. A stored draft must not trap Tab/Escape in a terminal after switching away. Keep the persisted Review body tag compatible, render the settings page inside that body instead of the root overlay layer, and discard its view draft on tab close.

- **Button hover belongs to its variant.** Adding InteractiveElement::hover directly to a gpui-component Button collides with its internal hover style and panics in debug native windows. Use ButtonCustomVariant hover/active colors; render the actual control in a GPUI regression.

- **Navigation selection is not a button hover border.** Profile and history lists share an outer row selection frame; clicking changes that owner. A GPUI click regression measures the outer outline independently of the hovered button, rather than only checking a selected flag.

- **Measured overlay bounds must notify after the current paint.** Updating a viewport cell and refreshing the window only repaints a cached presentation; even an immediate entity notify during prepaint can be consumed by that frame. After reducing a 1000-point window to 640, the observer retained its right-hand controls outside the pane until another click. Defer the owning entity notification when measured bounds change, then clamp against those bounds on the next render. The native resize check and the GPUI viewport regression cover the complete card, not just pure coordinate arithmetic.

- **Review keyboard tests need Guild’s real keymap.** The editor installs a `NoAction` for Tab under `guild` so terminals retain it. A card tested under component Root alone can traverse successfully while the native Guild card cannot. Review cards explicitly restore Tab/Shift-Tab traversal in their own key handler; test with `editor::init` and `actions::install`, and send Enter key-up because Button activates on release.

### A first review gesture is not a CLI resend (2026-10-05)

The CLI send guard requires previous turn telemetry and refuses unknown status.
Reusing it unchanged for a person's Review button disabled a new Codex prompt
that was already accepting input. A review gesture may submit an initial prompt
when the live terminal accepts bracketed submission and no turn or request is
pending; working/waiting and unknown status after input to that process still refuse.
Persisted `had_turn` describes the conversation, not the restored process: using
it as the first-input guard disables resumed sessions after every restart. Track
input in the live terminal, including typing, paste, drops and submitted requests.
Keep the CLI origin strict, and test a fresh session rather than marking every
native fixture as having completed a turn.

- **A Codex resume's sandbox flag overrides the conversation's saved choice.**
  Passing Guild's fresh-launch `-s read-only` on every `codex resume` reset
  workspace and full access after a restart. Omit permission overrides on
  resume so Codex restores its own policy. The 2026-10-27 follow-up also removes
  fresh-launch overrides: Guild’s Claude permission setting must not override
  native Codex defaults or explicit sandbox/approval choices.
  A terminal process starting is not its history being painted: keep a local
  screen preview outside the live parser until conversation text appears,
  with interaction, exit and a bounded timeout revealing the real terminal.

- **A ready transport and an eligible first request are different clocks.** A
  fresh/restored Claude Code, Codex or OpenCode terminal can still be initializing
  when a person requests review. Disabling Run review until bracketed paste is
  already enabled hid this interval; the old capture enabled it immediately.
  Accept one first UI request and wait, bounded, without sending any bytes until
  transport is ready. Recheck live input, lifecycle, close and exit before
  delivery; duplicate guards cover startup waiting and turn acknowledgement.
  Never weaken CLI send provenance or send Enter into a startup dialog. The free
  six-case native fixture gates readiness and checks cancellation. 2026-10-05.

### Runtime integrity audit (2026-10-06)

- **Atomic rename replaces a symlink; it does not follow it.** Reads had followed
  the link, while saves replaced it and left the target unchanged. Resolve an
  existing link before choosing the draft sibling; refuse a dangling link.
  Preserve target permissions and propagate file/directory sync failures.
- **A reaped leader is no longer ownership of a process group.** A successful
  `try_wait` used to skip descendant cleanup. The shared synchronous owner uses
  `waitid(WNOWAIT)`, stops the group, then reaps once. On macOS, `killpg` can
  return EPERM during exit and for a group containing only its zombie leader:
  [XNU's group signal filtering](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/kern/kern_sig.c)
  excludes zombies. Accept that case only after a bounded wait for exit and
  checking that the group contains exactly the owned leader. Other failures
  remain errors. An inherited stdout can otherwise make a reader join exceed
  the process deadline, as version probing and dependency queries reproduced.
- **Piped stderr must have a reader, and EOF is not a provider result.** Drain
  diagnostics concurrently, retain a bounded tail, and report EOF without a
  result as an error. Provider adapters own this contract; interactive sessions
  run in a terminal.
- **Synthesis categories are claims, not verdicts.** Moving a critical finding
  to pre-existing used to bypass verification and produce PASS. Verify every
  retained candidate; unverified candidates cannot satisfy acceptance.
- **UI notifications are not write receipts.** An earlier queued failure was
  returned by a later successful synchronous save, and notification consumption
  could hide a failure from another caller. Each waiting save now receives its
  own completion; path-drain outcomes remain separate from UI consumption.
- **A failing command in an `if` pipeline does not trigger `set -e`.** The
  boundaries recipe announced success when `cargo tree` failed. Capture and
  check discovery first, then inspect its complete output. Avoid `grep -q` on
  a live producer under pipefail: an early match can kill it with SIGPIPE.
- **A statusline EOF test is not a shell-startup benchmark.** Measuring from
  spawn made scheduler delays in sh/date fail the old 500 ms check. Gate the
  installed HTTP adapter and require user output to finish before releasing it.
  Test production descriptor isolation; keep timeouts only as deadlock watchdogs.
- **Signal zero is not a portable assertion that a process is running.** Linux
  may retain an orphan zombie after SIGKILL while macOS has already reaped it.
  A fixture holds a lifetime pipe until termination; EOF tests cleanup without
  depending on the host's init/reaper. Never signal an archived or unowned PID.
- **A new script's first exec is not the code under test.** macOS assesses an
  executable it has not run before; under a full `guild-session` run, with
  dozens of fresh fixture scripts at once, that first exec measured 2.5 s
  against 0.15 s for a fresh file alone and under 0.16 s for a second exec of
  the same file. The child sits in `S` before its first line, so a 2 s
  deadline reads as glab or a harness not answering. On 2026-11-27 it failed
  the glab mid-batch deadline and the version probe on every full run, and on
  `ab3767b` too — not a regression. Run each fixture script once before a
  test's clock starts (`[ "$1" = warm ] && exit 0`, or the glab stand-in's
  own early exit) and keep the deadline what it proves.

- **Diagnostic encoding is not the command's exit status.** Strict UTF-8
  conversion of stderr turned an exit-zero dependency query into a failure.
  Read both pipes as bytes; decode diagnostics lossily on failure, keeping the
  exit status and readable tail. Successful stdout remains strict UTF-8 because
  replacing bytes there can corrupt machine-readable identifiers and paths.
- **Open the directory before publishing an atomic write.** A directory can
  permit writes but refuse a read handle. Discovering that after rename reported
  failure after replacing the editor file or rotating the state backup. Hold
  the directory handle before either mutation, then sync it after rename. A real
  post-rename sync failure still means durability is unconfirmed; it does not
  promise that the old bytes remain. Inject open failure in private tests rather
  than depending on chmod semantics or whether CI runs as root.

- **One socket read is not an HTTP request.** The CLI's multi-answer test window
  replied after its first read, which could contain headers alone; closing then
  raced the client's separate body write and Linux CI reported Broken pipe. Both
  test windows now share the Content-Length-aware reader. 2026-10-06.

- **A painted line is not a text field.** Get from Version Control reused the
  old append-only edit model and drew a literal caret: Cmd+A, navigation and
  mouse selection were absent across many forms. Guild single-line fields now
  share InputState through text_field. Test dispatch through the real keymap,
  including typing after selection, not only that background actions are blocked.
  Stopping an unbound printable key in a parent's bubble handler also prevents
  GPUI's platform text-input delivery; let text reach InputState and guard the
  terminal ancestor instead. Equal model values must not call set_value on every
  repaint, which would reset the user's selection.

- **A document-wide minimum width breaks Markdown prose wrapping.** A wide code line or table must own its overflow; widening the entire TextView makes every paragraph wrap outside the viewport. The actual Pane regression uses a long paragraph, a 160-character code line and a wide table at 600 px, plus the real README. GPUI Kit 0.7.1 exposes table scrolling; code blocks need nowrap, flex-column/start alignment and a stateful action surface so their intrinsic text width is scrollable. Selection/copy must reach both tails while the prose keeps the same bounds and selection as a narrow control.
- **Component `set_value` is loading, not editing, since the 0.7.1 migration.** It clears undo and emits no Change event. Use `replace_all` for a programmatic edit; keep `set_value` for disk reloads and field resets. A FileTab reload emits its own BufferChanged after updating disk state.
- **The new GPUI test platform does not deliver frame callbacks automatically.** A test of `on_next_frame` must call `Window::simulate_next_frame`; running the executor alone only draws ordinary invalidations. Preserve native frame ordering when testing a reveal before the first editor layout.

- **A component Button tooltip owns its single GPUI hover callback.** In 0.7.1, combining `Button::tooltip` with `Button::on_hover` panics during layout. The full window exposed this on the Review titlebar trigger; the standalone Markdown Pane did not contain it. Put custom hover observation on an enclosing hitbox and keep the tooltip on the button. The regression test must render both together, not just the button styles.

- **Component tooltip colors can come from two theme stores.** In 0.7.1, the string Button tooltip uses `tokens.popover` for its background and the legacy `popover_foreground` for text. Guild's legacy palette synchronization does not update component tokens; a dark overlay therefore showed white text on white. Use Guild's `view::tooltip::Tooltip` through the control's public interactivity for palette-bound chrome. Cursor captures are only evidence when the expected window is active and the pointer is still over the target; unfocused or displaced-pointer captures do not prove a component styling defect. Overlay header controls use the existing Kit base Button public API for Guild visual states and keyboard activation. Active-window native checks verified pointing-hand controls and all eight standard frame resize cursors; move away and back before capturing the system cursor.

- **A hook socket is notification, not durable conversation identity.** Codex
  hooks persist exact Guild/conversation candidates before checking the socket;
  restoration uses only an unambiguous candidate for that Guild record. A nested
  `session_id` in tool output is not the conversation: use the top-level JSON key,
  not a greedy text substitution. Never infer identity from the newest rollout
  in a folder or overwrite an existing saved binding.

- **Direct component theme edits leave renderable tokens and GPUI Base stale.** Use `ComponentTheme::update` for persistent changes: `sync_base` alone does not update `tokens`, so Find/Replace and component tooltips can remain white in a dark window. Change mode in one update and apply Guild overrides in a second, because a mode change loads the registered palette over the closure’s colors. Assign both colors and tokens for Guild’s solid roles, including equal colors: an equal assignment alone retains an old token or gradient. Use `Kind::is_light()` so Cloud receives light defaults. Keep the temporary Markdown selection layout override scoped and unsynchronized. Set Markdown header/link roles explicitly, and give document tables `surface.app` through their public style: Base's default table surface is a popup/card, not the page. Otherwise dark document text can land on a white table even though ThemeMode says Dark.

- **A terminal job's exited leader must stay unreaped until descendant cleanup.** `try_wait` consumes the PID reservation, and the controlling terminal can already be detached when its leader exits. Job supervision discovers groups by session ID before reaping, stops them, and publishes one stable completion. A heartbeat fixture reproduces the otherwise surviving background writer; an unrelated control process must stay alive. Overlay dismissal never owns this cleanup.
- **A closed wrapper's harness can be reparented before its cleanup grace ends.** Parent chains then lose the window, although the PTY's captured session and groups still belong to its unreaped leader. Resume and managed continuation consult the retained cleanup owner until completion; descendants of another open plain terminal remain blocking holders. A rollout-holder fixture must close its FD in short-lived helper children, otherwise lsof/ps repeatedly observe different holders rather than the stable process being tested.

- **Recovery controls must outrank the deferred surface they control.** A Terminal Jobs list appended last in the ordinary root tree still painted behind a terminal overlay, because that overlay uses `gpui::deferred`. The list's lower Cancel/Show here buttons disappeared under the program it was supposed to control. Draw the recovery list in a higher deferred priority; validate by cancelling a running program while its TUI stays visible, not by looking at an empty list.
- **Default foreground is not ANSI white.** A light palette may use dark default text, but programs pair explicit white with explicit dark backgrounds. Mapping white/bright-white to light-theme text makes those pairs unreadable. Keep semantic defaults separate, preserve truecolour/OSC overrides, and fix shell/TUI theme selection at launch instead of recolouring arbitrary output. Native colour checks must unset a builder's inherited `NO_COLOR`; a monochrome Claude can look readable while never exercising the theme setting.

- **A retained Markdown parse is not a virtualized document.** A full-height TextView inside an outer scroller still lays out offscreen blocks on scroll and hover. Retain TextViewState and use its bounded internal list; preserve the logical list anchor when a replacement changes block count. Test both append and full replacement, plus mode/theme transitions.
- **Inline image size is not heading size or raster size.** Base 0.7.1 fits ordinary inline images to 75% of line height; badges inside a heading therefore grow. Guild's public inline-image plugin measures native Img logical dimensions, keeps the document cache and constrains illustrations to the pane width. RenderImage::size reports raster pixels and must not substitute for logical layout.

- **Rich-text selection paints over glyphs in Kit 0.7.1.** An opaque editor selection token hides Preview text even though copying works. Scope a translucent selection token to the component facade's layout pass and restore the original immediately; do not change sibling/editor colors or synchronize that temporary value into global Base defaults. Native selection checks must keep the dev window focused between drag and Copy.

- **A percent max-width cannot constrain a custom inline object measured at MaxContent.** Base 0.7.1 does not pass the wrapping width to its inline plugin and does not resize a custom element in fit_text. Observe its post-indent/clipped right edge through public prepaint, retain that constraint per picture and refresh only on a change; cover both narrowing and widening. A native Img also needs a stable ID under the inline-object scope to retain animation state. HTML block nodes require a block plugin; claiming an inline data URL bypasses the component decoder.
- **Go's vendor directory is not a module graph.** `go list -m -json all`
  refuses effective vendor mode; only that specific refusal selects the
  `vendor/modules.txt` reader, after Go's own consistency check. Directory
  presence alone ignores `GOFLAGS`, old Go directives and workspace roots.
  Replacement trailers are not libraries, and a parent's directory can exist
  solely because a nested module has packages. Keep package ownership and
  browser boundaries, and do not replay raw Go answers across modes.
  **A watched filename is not a watched path:** a basename comparison cannot
  match `vendor/modules.txt`, ignored trees lose events, and a workspace's
  vendor may be outside the project. Exact input snapshots on the watcher
  thread feed the ordinary dependency-refresh batch, including atomic swaps.

- **Identity guards need a live display snapshot.** An Update request bound to a
  cached HEAD safely refuses changed identity, but caching only at popup-open
  makes the first click after every ordinary commit fail. Refresh the snapshot
  before delivering each worker status map; keep the queued request guard.
- **Fullscreen adds auxiliary AX windows.** For owned-window UI checks select
  the largest actual content window of the verified PID, not the first AX window:
  AppKit's fullscreen menu strip can come first and shift every click.

- **A newly selected dock terminal has no focus ancestry until its next layout.** Activating the dock alone let the start screen reclaim focus when `+` replaced a terminal body. The next Return could start a main-area session instead of reaching the shell. Add, select and close use dock activation, which waits for actual focus ancestry. A render-time notify is consumed by that draw: schedule the next activation frame explicitly. GPUI test mode may draw at the end of update before next-frame callbacks, whereas the native host runs those callbacks first; dispatch them inside the same update to reproduce the native order. A dismissed terminal menu restores its visible target before running the deed; Find retains its proxy focus until the shared query input is drawn. Verify with paste and Return after each gesture, without clicking the terminal body.


- **A retained terminal entity does not imply retained text.** A program can
  clear its alternate screen and leave it, restoring an empty main grid. Real
  PTY fixtures reproduce this with CSI ?1049h / 2J / ?1049l; trailing diagnostics
  survive, including post-exit resize. Terminal Jobs distinguishes a completed
  empty grid from a missing or explicitly deleted buffer. Check scrollback too:
  blank visible rows alone do not mean the terminal contains no saved text.

- **A popup trigger must preserve focus on mouse-down before saving it on click.**
  GPUI automatically focuses the nearest track_focus ancestor while the mouse
  is pressed. Terminal Jobs therefore used to remember Root, not the terminal;
  Escape closed the list but typing no longer reached the shell. Prevent the
  trigger's default mouse-down focus, then capture/restore the actual prior
  handle. Verify with a shell marker after Escape, not only disappearance.
  Open In's primary external-app button needs the same mouse-down prevention:
  otherwise returning from the app leaves ordinary typing at Root. Open In is
  external-only; Guild's own terminal commands elsewhere manage their body focus.
  Test actual down/up followed by text, plus the keyboard equivalent.

- **InputState Blur also means application deactivation, not only a new focus owner.**
  A transient list field dismissed itself on that event while its handle was
  still the window's focus. Removing it invoked Root's focus-lost fallback,
  and the next character after returning went into the file opened by the
  preceding ordinary Explorer click. TextField calls host blur handlers only
  when the input no longer owns the window focus. A full-Root regression opens
  new and existing files by ordinary click, deactivates/reactivates the window
  during the query, and checks both continued search and a real editor focus
  transfer. A component-only list host cannot exercise Root's fallback.

### A list search needs a platform input handler before its first IME key

On macOS, a consumed KeyDown returns before inputContext.handleEvent. Seeding a
new field from key_char therefore commits the keyboard spelling before an IME
can compose it. Register a list-local InputHandler while the actual list owns
focus, forward insert/marked-text callbacks to InputState, then reveal TextField
with that same entity. Preserve marked ranges and undo; never seed it again on
mount. Dormant input keeps printable-key IME preference false so the existing
Commit Space action can run first; unhandled keys still reach inputContext and
marked composition always takes platform priority. Platform callback tests are
not proof of native input-source/candidate-window behavior.

Key the dormant session by the list's FocusHandle, not by one shared string:
two Push lists are rendered in the same view. A shared key lets the inactive
list replace the active list's InputState on every render. Assert the complete
query, paste and Undo in both lists, not only the first matching selection.
The visible TextField's key follows the input entity for each new session.

- **InputState marked text does not emit Change.** In gpui-base 0.7.1,
  `replace_and_mark_text_in_range` changes the value and calls `cx.notify()`
  without `InputEvent::Change`, including cancellation with an empty string.
  After speed search mounts, the component's own platform handler receives
  those calls. TextField observes value changes as well as events, deduplicating
  caret/focus notifications and cached programmatic writes. `unmark_text` retains
  the value and commits the undo transaction; never reseed the field on render
  to synchronize composition. The after-mount regression uses the real component
  handler, checks empty query/highlights and safe Enter after cancellation, and
  checks Undo after commit and unmark. It is not native IME candidate-window evidence.

- **A file-selection cursor must not take over a mixed tree's ordinary arrows.**
  Explorer, Search and Find Usages intercepted Up/Down whenever their file-only
  selection had a cursor, skipping directories or matches forever after the
  first file. Only selection modifiers go through the file-only order. Plain
  movement uses the tree's rows and synchronizes file highlighting at the new
  destination. Explorer resolves that tree by source, including for parent and
  fold gestures; an external path must never become a project action target.

### A visible file highlight does not choose Enter's meaning

Search and Find Usages use file-selection state to paint file groups and seed
Shift ranges, but ordinary row navigation still means Enter folds the group.
Track explicit file-selection gestures separately from that highlight; otherwise
adding a visible selection silently replaces folding with opening an editor.
Test the key after navigation, not just the selected row. When Explorer prunes a
hidden file, keep its source and path long enough to recover a visible ancestor;
pruning first silently sends the next arrow to the project's first row.

### Disposing a focused name input must hand its focus back

Explorer naming needs an explicit focus handoff on success and Escape, before
its TextField disappears. Otherwise the window's focus-loss fallback can give
the next arrow to the active editor or terminal. Opening a newly created file or
following an open renamed editor can then take focus normally. Tests must type
through the real field and must not restore focus before asserting the next key.
A context-menu target is independent of selection: completing a project name
also clears any old library cursor, even when the menu did not select its row.


A moved path mapped onto itself has an empty suffix. `new.join("")` adds a trailing slash: Path equality can still pass, but atomic save sees a file as a directory and fails with ENOTDIR. Return the destination itself for an empty suffix; test the subsequent save, not only the new tab label.

Several panes can share one editor entity. Rebinding LSP for each pane can clear the provider installed by the first pane, then skip reinstalling because the document is already registered. Rebind once per editor identity; a multi-pane move regression must check the provider as well as text, selection and undo.

### A pending IDE proposal is a permission request for a captured path

A moved file's historical diff may keep its old identity, but an `openDiff` proposal cannot remain acceptable: `FILE_SAVED` permits the agent to write the captured old path. The regression reproduced a proposal staying parked after a successful move. Reject affected proposals within their originating project, and refuse acceptance while that source is moving or being renamed. Do not retarget the proposal silently.

### Freezing a buffer requires a reconciliation on every exit

Explorer Rename originally released editors only when their path changed: Enter on an unchanged name left Save blocked, and replacing the naming field leaked the same state. Ignoring watch events during the freeze also lost external edits after Escape. Release on completion, cancellation and replacement, then compare the current disk with the last clean buffer before allowing autosave. An unconditional reload or conflict flag is also wrong: an unchanged file must retain selection and undo without a false external-change warning.

### A collision dialog outlives the source pathname

A clipboard identity checked only before Paste did not protect the retry after Keep Both: a different file could occupy the source name while the dialog was open. Retain the selected identities in the operation and validate them at the worker's transfer attempts; checking destination collisions alone cannot authorize a replacement source.

- **Editor range geometry is published during paint.** Public range_to_bounds can reuse the first visible layout for an earlier offscreen offset. Guard by visible_row_range and suppress gutter hitboxes until bounds, scroll, text and font match the painted frame. Use native cells measured from that layout; never derive wrapped row heights. GPUI test hosts must explicitly deliver simulate_next_frame for the deferred geometry frame. 2026-10-10.

Porcelain blame omits metadata for hashes it has already described, including
`filename` on later four-field group headers. Retain the per-hash historical-path
fallback, then override it only on explicit filename records. A fixture that repeats
filename on every header does not reproduce normal Git output. CRLF content in
porcelain must also be normalized before comparing against FileTab's LF buffer.

An explicit task import's grammar belongs to its owner, not its filename. A Make
include through `common.mk` still resolves subsequent includes against the original
invocation cwd. Preserve all explicit/alternative paths (even absent ones) in the
catalogue's watch dependencies or cached discovery can reintroduce standalone
imported tasks after a nonconventional file changes.

A source task can retain its identity after moving to another row. Geometry and
saved-identity checks alone do not validate an old gutter handler: compare the
painted Rope and current row declaration before choosing or launching. The host
regression dispatches actual old mouse handlers without a new render in between.

Collapsed offsets can project to the next visible buffer row through public
`range_to_bounds`, just as an offscreen offset can project to the first layout.
The contiguous visible range alone is insufficient for gutter cells. Collapse
equal measured row positions to their last buffer row, share that projection
with click guards and include measured rows in the deferred frame identity.
Otherwise folded numbers overlap and hidden controls can alias a visible row.

A custom gutter outside Input does not inherit Input's wheel handler. Route both
painted cells and empty gutter space through the retained editor's public scroll
setter using measured line height, and test an actual wheel event over a number.

Replacing a chooser with a singleton argument form does not replace already
painted mouse handlers until the next frame. Bind rows to the form's unique focus
identity and captured task, and use checked access; a second click can otherwise
index the new list or reset its values. Test old handlers before executor/frame
advancement, rather than clicking again after a render.

Speculative standalone import failures must retain their owner through every
failure path, including cycles and depth limits. Filtering imported owners only
for missing reads still allows a bogus cwd cycle to erase a valid root runner.
Initialize the project's import catalogue when a supported project task editor
opens, independently of Output, or its dependency watch set does not exist.

`open_file_from` is not the only FileTab entry: saved source tabs pass through
`restore_tabs` → `restore_file_tab` → `FileTab::restore`. Initializing import
watches only on open leaves restored buttons stale after nonconventional imports
change, even though their own discovery succeeded. Cover the saved-tab route with
Output closed and assert painted controls after change/delete/reappearance,
while preserving deferred editor setup and project-only restoration.

- **Ending an overlay gesture must use the release position.** Clearing the gesture alone leaves geometry at the last delivered motion when the final motion was missed. Apply the left-button mouse-up coordinates before clearing it; the window capture handler must still receive release outside the card. A no-button move remains the fallback when mouse-up itself was lost.

### Terminal history: a conversation is already resolved, and Tab crosses backdrops

`Tab::session()` names the current `Body` conversation after `Root::rebind`.
Applying `binding::resolve` to that value a second time can select a different
conversation when process B resumed C and process A then resumed B. Resolve the
immutable process marker at reservation, but use the current Body identity
unchanged when selecting popup history. A real Root regression performs both
rebinds, reserves through `command_terminal`, and checks the selected history.

A mouse-occluding backdrop does not trap keyboard traversal. The ordinary shell
fixture can conceal this because it has few background tab stops; reproduce it
with the real Settings page underneath. Use a popup tab group with boundary
stops and wrap both directions before focus leaves it. Check repeated Tab and
Shift+Tab, then Escape and the exact prior focus. A scrollable full-context
card must use a hoverable tooltip or it disappears before its content can be
scrolled.

Empty-history widening removes its own button when the new scope has results.
Transfer its focus to the popup before changing scope. Recovery after any row
control disappears (including completion) must also return to the still-open
popup, keeping its nested scope menu first; `Root::refocus` must not send those
keys to the underlying terminal. Test keyboard widening against another
session's actual reserved receipt, not only a mouse click over empty groups.
Project Settings uses a different Body and capture-phase Escape handler from
application Settings; it must yield while Terminal Jobs is open.



Tab traversal does not reveal clipped buttons automatically. Terminal history
tracks each row's parent focus and asks its existing scroller to reveal that
row when keyboard focus enters an action; the fixed header remains outside it.
Changing scope replaces the scroll handle, discarding any queued reveal for the
old grouping. Returning to the header resets the row scroller to the top.
Tooltip regressions must move onto a visible part of the origin: debug bounds
include clipped content. Test both title and context entry points, then enter
the tooltip and scroll to its final content. Persisted receipts from earlier
isolated-window cases also mean an All-launches fixture can include other cases;
use the fixture's exact worktree when testing a single row.


Revealing a tall history row with ScrollHandle::scroll_to_item aligns its top
when the row exceeds the viewport, keeping its action buttons clipped. Track
and measure each button's parent through public focus/layout APIs and reveal
the focused action instead. Cover an actual 360 × 180 viewport, including Tab
and Shift+Tab after manual scrolling. Full-context tooltips are separate layout
roots and must explicitly use the selected interface font, just like Tooltip.


A relatively positioned design card's offsetLeft/offsetTop include its normal
flow position. Feeding them back into style.left/style.top during resize adds
that position on every gesture and makes the card drift. Retain the original
relative CSS offsets and add only the measured pointer delta. Check all edges
in sequence, including the unchanged coordinate on one-axis drags.

### A worktree branch argument can open Git's help manual

`git worktree add -b --help` can pass the new branch name to an internal
`git branch` invocation, which interprets it as help before worktree preparation
fails. Refuse trimmed worktree names beginning with `-` in the shared native
writer, and in the CLI entry before the no-base `main_branch` read. Checking
only the base or only hiding branch-popup create rows misses the dedicated `+`
field and CLI. Preserve Git's remaining refname validation. Neighboring branch
creation uses `switch -c`, which rejects these names as invalid branch names;
switch/delete targets come from Git's branch list, and the branch popup already
hides option-like create names.

- **A completed terminal grid must not retain live PTY ownership.** Retained job
  screens kept the master and the input channel's writer descriptor after
  supervisor completion. An isolated child with a 64-fd soft limit observed two
  extra descriptors for its first saved screen. Drop master/input ownership
  after supervised cleanup, while the independent reader drains and the grid
  remains. This reproduces retention, not the historical installed-app EMFILE.
- **An in-memory presentation change is not a successful save.** If Show output
  changes the target/presentation and persistence fails, its identical retry
  must persist again; equal state can skip writes only without a storage error.
- **Archive viewers link to a source launch, not another invocation.** Resolve
  the internal source UUID with matching owner/project, including old receipts
  without descriptors, before rendering Open run or looking for a live viewer.
  Never repeat review argv or derive archive identity from a title.

### A hidden source control still occupies footer space

`invisible()` retains layout width. Measuring the update group's bounds can
therefore prove an eight-point Explorer gap while leaving a large empty source
slot before it. Measure from the last visible button instead and omit the hidden
source node. Keep an anchor measured from Update before keyboard opening creates
the source node; otherwise the first popup frame can use an obsolete origin.
Check non-hover, hover and keyboard states at ordinary and compact widths, plus
a non-Git workspace where no branch/update placeholder belongs.

### Revealing a footer control must not change its row identity

A row chosen from the currently rendered source width can move the Update group
when hover reveals source. Its new parent creates hover=false while Root still
holds true, so leaving for empty space never clears it. Choose the row using the
bounded expanded footprint while omitting the hidden source node. Geometry
regressions must advance the frame scheduled by prepaint measurements before
checking enter/exit; the first frame alone can miss the relocation. Budget fixed
Explorer/Update/glyph widths before sharing labels, and wrap tools if their fixed
buttons exceed the available work surface. Restored oversized docks must begin
resizing from displayed height, rather than consuming the first drag to clamp
the saved height.

Expanded-width row selection is stable relative to source reveal, but branch or
source names refreshed in the background can still legitimately change the
parent. Invalidate Root's hover when that layout parent changes or disappears.
Test long-to-short and short-to-long model changes before mouse exit, assert an
actual row change, then check both the flag and the omitted source node.

### Restored dock resizing must start from displayed geometry

A saved bottom size can exceed the current window limit. Normalizing only on
mouse press leaves both keyboard resize actions subtracting from that invisible
excess, so the first decrease appears to do nothing. All numeric resize paths
start from bottom_dock_height before applying their delta. The clamp reserves
both gaps rendered around the dock, and rounds the frame budget up: at UI size
16, fractional layout rounding otherwise leaves 119.5 pt instead of MIN_WORK.
Native regressions measure the upper work surface and dispatch both keyboard
actions after oversized restoration and at an ordinary saved size.

### A duplicated terminal writer can escape its terminal owner

On macOS, libc::dup clears FD_CLOEXEC. portable-pty's master and reader were
close-on-exec, while Guild's independent input writer was not. Ordinary Rust
Command and language clients inherited that master, retaining the same PTY after
its terminal owner closed. Allocate the copy with F_DUPFD_CLOEXEC atomically,
and finish fallible writer/reader preparation before spawn. Regressions compare
unique character-device identities with one owned live terminal, then verify
input and preserved output after completion and resize. FD counts alone do not
identify how many PTYs remain. Do not duplicate the completed-grid release fix.

### A quit hook installed in render misses an unrendered owner

A Root can start an explicitly owned fake server before rendering, or remain
retained by a test/subview. Installing on_app_quit in render leaves this owner
out of application shutdown. Register the existing cleanup callback in
Root::from_state. Fake servers intentionally survive stdin EOF; readiness and
lifetime-socket EOF distinguish owner cleanup from a polite server exit. Test
guards clean on unwind before closing the observer, so failed assertions do not
leak children or falsely witness successful cleanup.

### A language protocol ID is not a window lifetime identity

A restarted client reuses request IDs. Disable/re-enable must not let an old
provider take the replacement client's answer. Capture the Root generation and
use a monotonically unique window token, reject changed policy/lifetime, clear
diagnostics and providers on disable, and bound abandoned request tokens.
Repository identity must be confirmed before first automatic launch; a metadata
error is not evidence of a non-Git directory.

- **Serializing whole snapshots does not merge settings fields.** A scoped disk save can block the UI and overwrite an ordinary edit made while waiting. Root queues field edits off the UI, publishes only after acknowledgement, and defers ordinary snapshots until the scoped queue drains. A blocked-writer regression proves notifications remain available and overlapping ordinary/scoped fields survive; failure preserves the effective policy and identical Retry. Quit drains owned work without borrowing App again.
- **A Git executable failure is not a non-Git repository.** Only NoRepository plus safe metadata checks can authorize a canonical non-Git preference key. NoGit and Failed stay unresolved errors and block launch. Identity pending/error is distinct from deliberate Disabled.
- **Child PATH is not an LSP test safety boundary.** Client replaces it with login PATH. Ordinary Root tests therefore select explicit absolute fake executables or /usr/bin/false even if their default launch guard regresses. The guard test positively witnesses the same absolute fake seam before checking the forbidden launch.

- **Request metadata expiry can silently shorten a caller deadline.** A 30-second generation-token TTL rejected Find Usages answers even though its caller still waited 60 seconds. Retention derives from the longest caller budget with scheduling grace; it is housekeeping, not a new search timeout. A fake-backed test ages references/definition tokens to 59 seconds and proves answers survive both take and prune paths without a real-time sleep, while abandoned tokens still expire.

### A single-line field can swallow its scrolling host's wheel

TextField isolates wheel events by default so a terminal Find field cannot scroll
the terminal behind it. Applying this isolation inside Settings also swallowed
vertical wheel gestures, making the page stop when the pointer crossed an input.
Scrolling hosts opt into vertical propagation; horizontal-dominant gestures stay
owned by the field. Test the actual input and a neighboring label with the same
wheel gesture, and retain a default-isolation control for other hosts.

### Wrapped settings rows need a definite content width

A five-second native scroll profile spent 2806 of 3769 main-thread samples in
Taffy layout. The scrolling page's nested flex rows had only a percentage width
and maximum width, forcing repeated intrinsic size calculation. Reusing the
measured host width, scaled navigation/padding tokens and existing maximum
reduced the median changed-frame interval from 85.7 ms to 17.6 ms in the same
bounded 240-event sequence. Notify the owning Root after a changed measurement:
refresh alone can retain the previous width across resize and leave hitboxes
behind the new layout. Pin actual bounds at narrow widths and larger fonts;
a single before/after scroll screenshot cannot demonstrate smooth movement.
- **A disclosure click must be checked in the cached native column.** A redundant
  mouse-down propagation/default handler on the activity control allowed the
  isolated GPUI simulated-click assertion to pass but prevented opening the
  disclosure in the real cached sidebar. Let the click handler stop propagation;
  retain keyboard activation and verify the gesture in an isolated dev window.

- **A mounted native Spinner is not necessarily visible.** AnimationElement
  requests frames during layout, before ordinary scroll containers clip their
  children. Window/workspace focus alone cannot stop a scrolled-off row. Activity
  marks check the real content mask in native canvas paint before requesting a
  frame; tests count callbacks in a clipped container, not just mounted widgets.
- **Preventing popup default focus and stopping mouse propagation differ.**
  Capture-phase prevent_default preserves the old input owner while allowing
  bubble-phase click tracking. stop_propagation on the activity trigger's press
  previously suppressed the native click. Verify both opening and typed input
  after Escape through the actual mouse path.


### Hook batching must preserve local interrupt/prompt order

Terminal Escape can be queued before a newer UserPromptSubmit reaches the hook
socket. Applying every hook before a boolean interrupt then cancels the new turn;
applying every interrupt first cancels the wrong turn in the reverse case. Keep
local receive/input stamps and merge these witnesses in order. Root-level tests
must exercise both orders through terminal input and the actual hook receiver.
Claude asynchronous PostToolUseFailure also finishes its identified tool: resolve
that request without declaring the entire turn failed or answering another wait.


### Codex approval hooks and rollout completions have different contracts

Codex 0.162.1 PermissionRequest carries turn_id but no tool_use_id; Bash approval
input can add a description absent from PreToolUse. Correlating only prompt_id
leaves the request unknown forever. Match exact turn/tool/command only when one
pre-tool call qualifies, never guess between identical parallel commands.
PermissionRequest precedes automatic reviewer decisions as well as user UI.
PostToolUse is success-only; this version has no PostToolUseFailure event.
A failed nested command instead publishes item_completed/CommandExecution with
its nested exec UUID, distinct from the outer code-mode custom tool call ID.
Use that exact thread/turn/item identity without reading command output as status.

A hook batch can be queued when its disk tool completion is read. Register those
requests before applying tool completions. Baselining every hook discards fresh
completion records; reconcile turn identities, retaining explicit send/resume
baselines. Duplicate starts cannot answer a same-turn wait, and delayed hooks
cannot reopen a completed turn or recreate unread state after Seen.

Native Codex subagents inherit Guild's routing environment too. Refuse explicit
agent_id and exact-rollout object-valued subagent sources before both socket posts
and durable identity writes; refusing status only leaves restart identity polluted.
Metadata from a shared cwd alone does not prove a subprocess emitted hooks.

Rollout task_started must also follow the ordered local input/hook batch. Reading
it first makes Probe drop a newer UserPromptSubmit as duplicate after an old
Escape, leaving the new turn interrupted. Codex Root regressions exercise both
Escape/prompt orders with disk starts and queued or later completions.

Codex Stop is tentative: another Stop hook can block and continue the same
turn_id (0.162.1 core/session/turn.rs). Settle only on the exact rollout's
task_complete or turn_aborted; otherwise subsequent real questions disappear.
For an in-TUI switch, an EOF baseline loses a current failed command already
recorded before the next poll. A hook naming the exact conversation and current
turn admits that turn's existing records while excluding other history. Without
that witness retain EOF baseline; never choose a conversation by cwd or recency.

Ordering hooks before disk is insufficient when UserPromptSubmit is missing:
a disk start already present at Escape can clear Board's interrupt on the next
poll. Snapshot the exact rollout boundary at terminal input, before sending the
interrupt bytes. Pending complete starts carry their source positions and cannot
clear an interrupt that followed them; newer starts/hooks remain available.

A hook-confirmed history replay can span multiple bounded reads. If a newer
prompt arrives before EOF, update its turn filter together with the current
turn; retaining the first filter permanently discards the newer failed-command
and turn completions as the cursor advances. A large-rollout regression exposed
this defect even though the single-turn resume regression passed.

### File scroll momentum outlives macOS's Ended phase (2026-10-13)

`gpui-pre-macos` 0.3.8 maps native MayBegin/Began to Started, native Ended to
Ended, and everything else to Moved; it does not expose momentumPhase.
Resetting a file's dominant-axis lock at Ended lets inertia drift diagonally.
Keep it until the next Started, explicit Cancelled, discrete Lines wheel or
an event owned by another surface. Zero Started waits for meaningful movement;
equal components choose vertical. Do not use GPUI's OngoingScroll for this
contract: it releases at Ended and can unlock during a large sideways drift.

A file-local hitbox behind the content respects occluding Find/completion/hover
controls. Preserve upstream scroll side effects by replaying only a changed
delta through the public Window API at the end of the current effect cycle.
Synchronous redispatch cannot work: GPUI temporarily takes the listener vector
out during dispatch. A replay guard prevents recursion. Markdown horizontal
blocks also need `restrict_scroll_to_axis`: otherwise a vertical-only delta is
remapped to horizontal by a single-axis Div. The page and blocks still own
clamping and scrolling; no global input filter or dependency patch is involved.

The phase information is lossy. A precise Moved without a previously observed
Started (opening/switching into a file mid-gesture, or an unphased device) passes
through unchanged. A precise unphased device after a trackpad gesture cannot be
distinguished from its momentum until another Started or a Lines event; no
invented timeout can resolve that without cutting off real inertia. macOS also
maps native cancellation to Moved. New file/preview instances have independent
state; hiding Markdown source or switching its mode clears the corresponding
state. Ordinary Lines wheel events, including native Shift-horizontal wheel,
clear a retained trackpad lock and pass through. Test the actual sequence
Started → Moved → Ended → Moved → Started, not just synthetic phase names.

### Codex PermissionRequest can be automatic work

Codex 0.162.1 emits PermissionRequest before choosing Guardian or user review.
There is no approval-resolution hook at command start; waiting for PostToolUse
or rollout completion draws a false question throughout a long command. A live
background task reproduced this across project switching. The exact turn_context
records approvals_reviewer; auto_review for built-in Bash/apply_patch is work,
while unknown/manual context, MCP overrides and genuine question tools keep
their waiting contract. Never infer this from global config or terminal text.

### A bounded tail forgets the policy of a long Codex turn

The automatic-review context is written near turn start, not before every permission. Searching only the final MiB worked in a 145 KiB live fixture but failed after a real turn produced more than a MiB of output. Retain the latest complete exact-turn context with an independent cursor; lifecycle resume baselines must not discard policy discovery. A long wall-clock test with little output does not cover large-output behavior.

### A dismiss backdrop can hide its own trigger

A fullscreen deferred `.occlude()` backdrop covers the ordinary titlebar usage
chip too. The chip still paints underneath, but GPUI excludes its hitbox from
hover. Testing a chip helper with `open=true` only changes its background and
misses this production-layer defect. Defer the open usage trigger above its
card/backdrop in its existing layout slot and occlude it there: without that
occlusion, the backdrop's mouse-down closes the card before the trigger's click
can toggle it, reopening it. Test the real Root content and dismiss layer with
simulated enter/leave, trigger toggle, card content and an underlying clickable
panel, for both readings and unavailable usage. Outside clicks must close the
card without reaching the underlying panel.

### Application activation is not activity visibility

A visible working ring must not use `window_focused` to decide whether to request
its next frame: clicking another monitor's application deactivates Guild without
occluding its window. The five column/menu callers used that flag and froze the
rings. Preserve shown-workspace guards and let native canvas paint check the
content mask and reduced motion. Disabled menu navigation does not hide a live
working status. GPUI stops the macOS display link on occlusion, independently
of activation. The inactive-caret policy is separate and intentionally steady.
Regression fixtures draw each actual caller independently, use Root's production
activation observer and count frame callbacks through deactivate/reactivate;
one working sibling must not mask another stalled ring.

### A row drag must end when Escape cancels it

An unchanged row order after Escape does not prove cancellation: GPUI can still
hold the native drag until the subsequent release. Worktree drags retain a
transient owner flag and a context marker. GPUI dispatches bindings before raw
key capture: suppress Escape with contextual NoAction at the deepest focus
context before the root cancels the drag. Root-focus checks alone miss ClosePanel
and an editor collapsing its additional cursors. Preserve those states and focus. Clear that flag on mouse release and cancel on the first
move with no button after an outside-window release. The regression checks
`has_active_drag`, preserved focus and a subsequent release, not only row order.

A native Root gesture can save global workspace state even with isolated rendering.
Run such regressions in a subprocess with its own `GUILD_STATE_DIR`, as the footer
and worktree-order tests do; a caller-set environment alone does not protect the
next ordinary `just check`. The ignored native case refuses direct invocation
without its fixture marker before constructing a Root.

### A drag that leaves the window comes back as the same drag

GPUI 0.3.8 already promotes an in-window drag to a native AppKit session:
`external_drag_payload` beside `on_drag` is asked once, when the pointer
leaves the viewport with the button held, and the window's own
`NSDraggingSource` offers copy only outside the application. No dragging
source of ours is needed in `system`. Two things follow. The release then
belongs to AppKit and never reaches the window, so the Explorer's clock ends
its state on `has_active_drag`/the physical button rather than on a drop. And
when the native session re-enters the source window GPUI *restores the same
drag value*, so an `on_drop::<FileDrag>` that trusted it would move files the
person had handed to another application, or not, depending on whether the
80 ms clock had noticed the hand-off yet. The resolver marks the gesture
outbound itself, which makes the in-window outcome independent of timing and of
whether the platform started its session. The test platform refuses that
session and `test_window` is crate-private, so a window test can prove only the
inert return; the copy in Finder is a check in a real window.

### A transient state above a list moves every row of it

The Explorer drew a running transfer's notice above its rows. A copy of a few
files lasts a frame or two, so every drop, paste and move pushed the whole list
down by the notice's height and back up: the person saw the Explorer twitch on
release. The copier's staging folder (`.guild-transfer-<uuid>`, beside the
destination) did the same a second time, because the watcher listed it while it
existed. Neither showed in a test that only looked at the result. Something that
appears for the length of an operation lies over the rows. Below them is not
enough: it takes its height from the list's viewport, and a list scrolled to its
end drops by that height when the notice goes. The copier's folder is hidden
only while its copy runs. The regression
draws a frame between the drop and the worker
(`window.dispatch_event` and `window.draw`, without parking) and compares a
row's bounds; the window check compared the top rows' pixels in every captured
frame, before and after.

### A synthetic drag to Finder fails for reasons that are not Guild's

Two things made correct drags look broken when driven with `CGEvent`. A
command-click leaves the Command flag on later synthetic mouse events unless
each event sets `flags = []`; AppKit then narrows the source's mask to
Generic, and a copy-only source (the Explorer's) intersects to nothing, so
Finder silently refuses the drop. A person holding Command sees the same.
And a Finder name-collision sheet ("Replace?") holds Finder's copy queue:
every later drop waits behind it, unseen if the sheet is on another space.
Check the offered mask with a drop target of your own (`draggingSourceOperationMask`)
before suspecting the source, and give Finder a second of hover before release.
A synthetic Escape does not cancel a native dragging session at all, Finder's
own included; that check needs a person.

### A full worktree order can contain an unopened raw-byte path

Git's parser preserves path bytes, including paths serde's PathBuf serializer
cannot encode. A valid row move includes every current row, so another unopened
path can make the whole Desk unsaveable. Preflight the full candidate before
inserting it into workspace state; refuse it with a warning and keep the old
serializable Desk. The regression moves real UTF-8 siblings around a raw-byte
Git-listed row, then proves a subsequent ordinary save still works. APFS rejects
creating that byte path, so the row remains an unopened parser fixture.

### A startup drop can discard rows the column has not learnt yet

Before the first Git list, remembered opened roots are already visible. Replacing
the saved order with only those paths loses the relative order of unseen saved
unopened rows. Do not commit a reorder until this repository's list is available.
Verify list arrival restores the original unopened order. A native drop regression
also flushes and reloads the isolated workspace immediately after its first drop,
before any other save can conceal a missing save in the gesture handler.

### MCP failures can leave Codex permission waiting behind

The exact 2026-10-15 incident conversation kept running Playwright while Guild
reported waiting. Its rollout records MCP execution as `item_completed` with an
`McpToolCall`, including terminal failure; the completion reader admitted only
`CommandExecution` and `FileChange`. Without a matching PostToolUse, such a failure can
leave its request waiting until turn completion. Read terminal MCP items through
the same conversation/turn/request guards. Do not clear every wait on unrelated
MCP activity or infer approval from terminal text.

### A small activity ring can rebuild a whole cached column at display rate

A canvas request_animation_frame notifies its current view, and GPUI dirties the
view ancestry. Putting the mark in a child Entity alone does not isolate the
column: ancestor invalidation still defeats its cache. On 2026-10-15 an optimized
isolated column with synthetic Working sessions rebuilt at approximately 60 Hz
and consumed 17.5% of one core, against near-zero steady idle. A per-window 50 ms
decorative clock reduced it to approximately 15 Hz and 5.8% on the same fixture.
Reuse paints already caused by real updates: a naive capped timer added twenty
redundant redraws to one hundred real updates in a second. Track paint recency
per owner, so frequent output in one cached sibling does not freeze another.
Arm from actually visible paint, share the clock across marks, and stop on
occlusion/reduced motion/window loss. Test redraw counts with virtual time;
next-frame callback counts alone cannot verify a timer-driven animation.
These CPU results describe the fixture, not the installed application's entire
historical energy consumption.


### Awaitable PTY notifications and GPUI's test scheduler

A real PTY reader or job supervisor now wakes terminal presentation directly.
GPUI's deterministic scheduler rejects an OS-thread wake unless the fixture
explicitly permits external activity with `cx.executor().allow_parking()`.
Apply this only to tests that actually own real child processes; do not disable
thread checking globally or alter production scheduling to satisfy a fake clock.
The first full event-driven terminal run exposed this in Run/Terminal panel
fixtures; their native worker fixtures now opt in explicitly.

A parser batch must publish its notification after releasing parser/grid locks.
Queue bounds apply between parser chunks, never by blocking a listener while it
holds the grid. Coalesce repaint hints, preserve ordered semantic events, and
release a blocked producer when its receiver is dropped. A synchronized chunk
with no repaint still needs a wake to arm its timeout.

### Reopening a process does not submit a continuation

A stopped Codex task resumed into a normal prompt and stayed unknown until the
person typed a prompt. SessionStart supplies identity, and a resume rollout
baseline intentionally skips old task_complete events; neither is composer
readiness. Keep agent send strict. Closed-task continuation belongs in literal
startup argv with a durable request reservation, not synthetic typing or a
status override. Pass `--` before text beginning with a dash. An absent caller
is the existing external operator; a supplied caller must resolve through the
window binding and task guard. Never fabricate a session identity for a receipt.

The startup handoff checks that its saved cwd still exists: portable-pty 0.9's
CommandBuilder falls back to HOME for a missing directory. Missing Guild hook
provenance likewise refuses this path rather than inheriting a controller's
session identity. Test the real reopen boundary with a private fake executable
and hook socket, not only the command builder. PTY child creation still does
not acknowledge model delivery or successful execution of that program.

A startup fake must accept valid global policy flags before `resume`. The old
literal-handoff fake required `$1=resume`; Body prepended `--sandbox` and
`--ask-for-approval`, so the fake exited 2 before recording count/provenance.
Record argv before validating it, and bind the owned fake through an absolute
launch-prefix seam: an environment variable naming a fake does not select it.

### An unrelated Codex daemon is not a conversation holder

A real dev-window continuation failed because the inspection rejected every
external `codex` whose argv lacked `resume ID`, including shared app-server
processes and unrelated fresh sessions. Inspect exact resume argv plus kernel
holders of the exact metadata-validated rollout. The file check also catches
in-process `/resume` changes that argv misses. Missing tools, warnings, malformed
results and changed file identity fail closed. `lsof -t` suppresses warnings;
use machine fields with warnings enabled. This remains a snapshot, not a lock
against later external launches, and never admits an open unknown target.

### Scheme detection must not rescan punctuation-separated suffixes

The terminal joins soft-wrapped rows into one logical line before looking for
links. A scheme check at every word boundary rescanned `a-a-a-...` after each
hyphen, turning 100,000 characters into 8.91 seconds on the UI thread with the
terminal lock held. Keep the end of the scheme-character run already examined;
a physical row width does not bound this work. Do not restore broad `name:` URI
inference: its boundary heuristics broke grep, JSON and Markdown locations.
The scheme guard (`terminal::links::unsupported_scheme_at`, pinned by
`hierarchical_and_listed_opaque_schemes_hide_embedded_links` and
`a_long_non_link_does_not_block_the_terminal` in
`tests/all/terminal_links.rs`) limits exclusion to `scheme://` and a short
opaque-scheme list, ending at whitespace, quotes or angle brackets. Other candidates are deliberately left to the path/disk rules.

### Named NSSound instances reject overlapping play requests

`NSSound soundNamed:` returns the cached named instance. Calling `play` while it
is already playing returns false, so independent completions in successive status
passes can silently lose their sound. Stop before play to restart the chime; the
per-pass coalescing rule does not extend to the duration of Glass. The regression
uses a silent Objective-C protocol double: ordinary tests must never play audio.
See [Apple's play contract](https://developer.apple.com/documentation/appkit/nssound/play%28%29).

- **Hiding AppKit's traffic-light container suppresses fullscreen hover controls.**
  The old `system::set_hidden` call on every render predated workspaces-sidebar-controls.
  It hid the whole native container, so reaching the top edge revealed the menu
  but no traffic lights. Leave the native reveal to macOS; Guild's own titlebar
  uses WORK_INSET while the native bar is retracted. Native automation must run
  the NSWorkspace run loop after activation before checking frontmostApplication;
  sleeping alone can leave that cached answer stale and falsely reject the PID.

- **GPUI 0.3.8 ignores window refresh during prepaint.** A footer width measured by canvas must defer an owning Root notification through `cx.defer`; `window.refresh()` in that phase schedules no next frame. Layout budgets and native minimums read those measurements on the following render. Cover both measurement paths without an unrelated input event.

### Frozen archives sharing a Cargo target can reuse another archive's desktop artifact

The adaptive refresh measurement fixture exported exact main and instrumented
candidate sources into ignored directories, sharing the dependency target to avoid
rebuilding GPUI. A second export with modified diagnostic code unexpectedly reported
Fresh and copied the previous executable: its SHA-256 was unchanged, and the cached
warning named an old source line. `cargo locate-project --workspace` still named the
correct archive. Do not trust a successful build alone in this arrangement. Preserve
frozen binaries separately, then `cargo clean -p guild-desktop --release` before EACH
archive build (or use separate target directories); verify the emitted compilation
path and binary hash against a source manifest. Never run measurements concurrently
with builds. The stale second-export binaries were detected before native execution.

### Front-history sampling belongs to the whole gesture

Sampling each setter independently records intermediate fronts that nobody saw.
An arrow across areas first selects the area's previous pane and then the pane
chosen by geometry; both changes belong to one history transaction. The same
rule applies after leaving a project for a dependency: opening the dependency
must not sample the departure again and make Back skip it. Keep the departure
sample outside nested suppression, then suspend samples until the gesture ends.

A deferred caret landing may block samples only while its text editor can still
appear in the front. A removed, binary, unreadable or closed target cannot land;
clear that pending jump rather than letting it disable navigation everywhere.
Returning to a body also has to dismiss Sessions and transfer keyboard focus:
changing `active_tab` alone can leave the selected target underneath the list.

### Per-project Git workers still share one process descriptor limit

A three-second poll wakes every open project's worker together. Command::output
allocates stdio pipes before spawning; 32 concurrent workers took the installed
nightly from 63 descriptors to 254 and state writes failed with EMFILE. Idle
lsof snapshots looked healthy because the pipes disappeared after each burst.
Bound reads in git::run before pipe allocation and hold the permit through output
drain, including errors. A per-worker bound is insufficient. The regression uses
32 synchronized callers and state saves in an isolated child with a 64-FD limit;
it neither changes the host limit nor touches user processes or state.

### Root fixtures must own settings and workspace writes

A temporary directory passed to `Root::diagnostic` now owns its settings and
workspace writes as well as its diagnostic runtime files. Previously its first
window geometry save wrote default preferences to the person's `settings.json`,
and `open_file` could replace their global workspace with fixture projects.
Ordinary edits, scoped preference workers and workspace snapshots must all use
the Root's explicit policy directory, rather than the process-global paths.

Production-policy fixtures (`Root::from_state`) can write on construction when
adopting panels, even without rendering. Run them through `isolation::isolated_test`
or an existing owned child wrapper with its own
`GUILD_STATE_DIR` and `GUILD_NO_GLOBAL_HOOKS=1`. Assert the resolved global path
before constructing Root, flush the store while fixture directories still exist,
and keep their cleanup in the waiting parent. Verify actual written bytes under
the child-owned directory so an outer test-runner override cannot mask a missing
wrapper override. Restore-only paths must also be audited for indirect saves.


### A worktree destination alias can leave a successful write waiting forever

Git reports canonical paths in `worktree list`; the command's pending `Listed`
receipt compares the destination with that list. A configured directory using
`/var` instead of `/private/var` on macOS, or another symlink alias, can therefore
create the checkout successfully without ever settling the command. Canonicalize
the existing parent directory in the shared destination gateway before building
its child path. The workflow native fixture exercises the actual guarded write,
list refresh and session launch; a write acknowledgement alone misses this bug.


### Strict workflow journals cannot use the settings backup replacement gap

The general settings Store moves primary to its backup before renaming the new
file. That contract relies on its backup reader. A workflow journal explicitly
refuses fallback because an older reservation can repeat an external effect or
replenish allowance. Its writer therefore syncs a unique owned scratch, renames
directly over primary and syncs the parent directory. A concurrent reader checks
that primary always contains one complete snapshot. Failed scratch files remain
evidence; settings/workspace backup behavior is unchanged.

### Project view state no longer owns the whole .guild directory

Forget-sessions-on-close used to remove `.guild` recursively. Configured workflows
and durable workflow-run journals also live there now. The close handler must
reset only ProjectState through its ordered writer, preserving configuration,
run/review history and other project resources. A fixture exercises the actual
final-holder Close Project path with forgetting enabled and retained run bytes;
ordinary tab-close tests do not reach that deletion branch.


### Publish a controlled process witness before announcing its path

A fixture waiting for `path.exists()` can read an empty or partial JSON file
when the child uses create-then-write. The configured native workflow fixture
exposed this under whole-workspace load. Publish the owned witness with a sibling
write and atomic rename; keep the actual identity/argv assertions. Increasing a
timeout does not qualify a partially published identity. The PTY input capture
fixtures had the same race: `captured` existed before its 8 MiB payload was
written, and an empty refusal capture could pass before any bytes were written.
A producer held after file creation reproduced the empty read deterministically.
Write and close `captured.pending`, then rename to `captured`; only the final
name is the completion signal. Keep the full payload/FIFO and refusal checks.
## Consume completed updater replies before scheduling Local polling (2026-10-18)

The updater runtime's `idle()` means its worker has no active/pending job; its
completed reply may still be waiting for the window. Enqueue replaces reply slots.
If Root schedules a due Local poll before draining those replies, a slow first
native frame can erase a completed Check. The worker knows the valid artifact but
Root stays checking forever; subsequent unchanged-metadata replies never repair it.
This occurred with the actual large signed native updater fixture, while tiny
headless bundles and fast GPUI frames passed.

Drain current-generation events first, then schedule periodic work. Restart the
Local cadence when a completed Check is received. The isolated Root regression
waits for a native candidate reply, makes the poll overdue without draining it,
and requires that one Root tick still activates that candidate. Do not infer that
an idle worker means its caller has consumed the result.

### Executable mappings are not open-file references during update recovery

A libproc open-file query did not find a running copied executable. The explicit
closed-app recovery guard queries bounded same-owner executable paths instead,
without reading/logging arguments or environment. macOS can retain a pre-rename
executable pathname: check both the canonical recorded destination and all derived
transaction slots. A /var versus /private/var alias in a test is not a canonical
transaction path. The guard supplements the owner's obligation to keep these
apps closed; a process snapshot does not prevent a subsequent launch. It never
signals a PID, and the shared recovery still revalidates native proofs and the
anchored destination under its lock before changing files.

### Acknowledgement is not the end of active update-pointer cleanup

The new window can read Acknowledged while the helper still retires the previous
bundle and active pointer. Re-reading only LaunchRequested leaves that window
permanently displaying manual recovery even though the swap succeeded. Re-read
every active receipt until the pointer disappears; allow a bounded finishing
grace for terminal cleanup. Clear candidate authority before inspection, then
resume discovery after cleanup. Manual Acknowledged cleanup keeps the current
app. A successful acknowledgement alone is not evidence that the new window
returned to its ready state; observe that separately.

### Reinstall proofs can be identical, and acknowledgement survives cleanup failure

Explicit Reinstall can select exactly the running artifact, so previous/next
UUID and seal can both match. Restoration must distinguish slot ownership as
well as proofs: if the previous slot is present, move that retained app back
before retiring the failed identical copy. Treating destination == previous as
already restored leaves a duplicate backup and a permanently active transaction.

Once the exact new instance acknowledged startup, retain Acknowledged if later
installed-receipt publication or backup/pointer cleanup fails. Backup removal can
succeed while its directory sync fails. Downgrading that state to RecoveryRequired
loses the authority to offer keep-current and can suggest an impossible restore.
Fault-inject cleanup errors before and after owned backup deletion and require
repeatable keep-current, including an already absent backup.

### Project drag edge clocks must measure the stationary viewport

A project drag can continue while the pointer stands still, so it owns a bounded
clock and checks the physical primary button as well as GPUI's active drag.
Measure the list outside its scrolling children: a canvas inside the scroller
moves with them and eventually loses its own visible bounds. The sidebar and
its actions footer use their actual content-mask/window-clipped geometry,
including while peeking. Departure and scroll limits pause, rather than destroy,
the held gesture; release, Escape, workspace change and hiding destroy it.
Use frame-scale elapsed-time steps rather than a whole row per timer wake. Cap
stall catch-up and refresh the time sample even while paused or at a limit.
In the pinned test scheduler, `advance_clock` drains intermediate timers despite
its executor wrapper's comment. To model a stalled callback, advance the test
scheduler's raw clock before draining tasks; a long ordinary hold is not a stall.
Notify Root after moving the scroll handle so the cached column is invalidated.
Project headers prevent default mouse focus transfer during mouse-down like worktree rows;
the completed header click explicitly focuses the sidebar for project speed search,
while worktree entry keeps its existing body activation behavior. Both
share the `column-drag` Escape context and retain their separate payloads.

### A drop line drawn in a gap needs the gap to take the drop

The column's drop line between items is drawn in the list's twelve-point gap,
which belongs to no row. Rows clearing their own hint as the pointer left them
put the line out and back on every crossing, and a release on the line itself
reached only the list, so it did nothing. Rows keep the hint when left; the
list clears it when the pointer leaves the list, and takes a release in a gap
by the hint. The root's capture-phase mouse-up ends every column drag before
any bubble-phase drop runs, so it must not clear the hint: it is cleared after
the release has been dispatched. A row reads its own zone at release from its
painted bounds, not from the last move. The first move after a press starts a
GPUI drag; `on_drag_move` listeners hear only the moves after it (2026-11-20).

### Tab release must agree with the painted hitbox

A tab strip's live ScrollHandle offset may change before the next draw. GPUI
still dispatches release to the old painted hitbox: recomputing its bounds from
the new offset can swallow the drop. Measure the tab's full and content-mask /
strip / window-clipped bounds during paint, and use that same geometry for move
and release. Regress MouseUp after changing the offset without drawing.
Tab drags also need the existing `column-drag` Escape context: capture listeners
alone run after bound editor actions and can clear multiple cursors. Projects
and tabs move on `on_next_frame`, not timer wakes; fixture clocks must explicitly
deliver `simulate_next_frame`. Paused lifecycle polling must not request redraws.

### Shell wrappers change process ancestry, and holders can exit between snapshots

A terminal's leader may be the initialized shell rather than the harness. IDE
refusal uses the owned PTY session; resume and continuation retain cleanup
ownership through the unreaped leader's SID and captured groups until cleanup
finishes. Parent chains do not survive wrapper exit. Retain that witness before
the process snapshot; a holder can exit or its cleanup can finish before the
identity query. Recheck changed process/rollout witnesses within the existing
grace bound; a live foreign holder or uncertain query still refuses. Fixture children reproduce ownership,
foreign rejection, refusal status and cleanup without a provider call.

- **A globally opened Settings page has no active body to retain input focus.** Root fallback must honor the shared TextField registry as well as component-managed inputs; otherwise the frame after a click takes focus back and initially empty fields ignore ordinary typing. Test a fresh no-project Root, including padded left/center/right clicks, rather than only initialized projects with prefilled values. Keep padding inside the focusable TextField boundary.

- **A Settings row must not reuse its menu control's id.** Network's registry-generated agent rows initially gave both the surrounding row and the native choice button the harness program id. After the first large scroll, row names could be absent until another repaint. Keep the stable registry id on the interactive menu only; removing the unnecessary row id restored first-scroll labels in the isolated native window. 2026-10-20.
- **Accepted sockets can inherit a nonblocking listener on macOS.** A loopback HTTP fixture polled accept on a nonblocking listener, then treated WouldBlock from the accepted stream as EOF. It sometimes replied to CONNECT before receiving the destination request. Set the accepted stream blocking before its bounded read timeout; a raw local accept probe confirmed inherited O_NONBLOCK on this host. 2026-10-20.


### PTY EOF is not the harness exit status

A harness can close its PTY descriptors before it exits. Publish completion only
once the owned child has ended: the reader can wait with `waitid(WEXITED | WNOWAIT)`
without reaping the leader or polling the UI. Keep the final parsed grid selectable.
On Darwin, closing stdio does not reliably produce early EOF while the controlling
TTY is retained. The regression uses a controlled early-EOF reader with a real
PTY child that exits later, alongside full real-harness launch/retry fixtures.


### Stopping mouse-down in capture can swallow a worktree button's click

The worktree row owns drag initiation, so its hover controls must isolate their
mouse-down. Stopping propagation during capture prevented Copy path from receiving
a click. Preserve focus with `prevent_default` in capture, then stop mouse-down in
bubble and stop the eventual click before it reaches the row. The cached-column
hover regression exercises actual copy, pin/unpin, main/master exclusions and
cancellation through the shared removal prompt.


### Key capture does not stop a bound action under a modal

GPUI dispatches bindings before capture_key_down. The worktree confirmation's
key handler alone let Cmd+Shift+B fold the covered sidebar. The focused modal
now stops Guild's named actions on its own bubble path before Root handlers;
a native shortcut regression pins the actual unchanged sidebar state. Focus is
kept inside its controls and restored on Cancel/Escape.

### Branch deletion hook records can have zero old OIDs

On the installed Git, reference-transaction input for branch -d has zero old and
new OIDs. Comparing that record cannot pin a confirmed commit. Read the exact
nonsymbolic ref during prepared, while Git holds its ref locks, and preserve
the original configured hook. Real tests cover loose/packed refs, a merged tip
changed between worktree removal and branch deletion, symbolic replacement,
a concurrent ref writer blocked by the lock, and a refusing user hook. A branch
failure after successful folder removal must remain an explicit partial failure.

- **A cursor tooltip can miss the only move that reveals its control.** Worktree actions are hidden until their row's group-hover frame. GPUI's ordinary `.tooltip` schedules only from MouseMove, so one move directly onto a hidden action followed by a stationary pointer shows no hint. Extra jiggle masked it in prior fixture checks. Worktree controls use Kit's public TooltipOverlay with prepaint-measured bounds and on-hover entry, preserving the upstream timer/positioner. Test one first move, advance the clock without another move, then switch adjacent controls and leave; test through the cached column.

- **Group-hover paint refinements do not reserve layout width.** Worktree actions that changed width/display through `group_hover` still shared the branch label's allocation; toggling display also hit GPUI's prepaint/paint ordering assertion. Row composition now records transient row hover in Root and notifies the cached column, rebuilding actual flex siblings and replacing diff counts. Assert label/action/count bounds through real pointer entry at the narrowest sidebar width; visible glyphs alone do not establish reflow.
### Terminal margins must affect the grid, not just the first painted row

A compact title bar did not translate into more terminal text: the terminal
reserved 13 points at both vertical edges, then floored the remaining height
to complete rows. Claude's own leading blank rows made screenshot-only guesses
misleading. Measure the actual terminal/content bounds and ask the child for its
PTY size at the same font and window size. Keep fit, paint, pointer mapping,
selection edges and scrollbar on the same content rectangle when insets differ
by axis. The terminal now uses 5 vertical / 13 horizontal points; the font and
program output are unchanged.

### Center the column grip without hiding its drag listeners

A 9-point target wholly inside a 232-point column had its center at 227.5,
visibly left of the boundary. The symmetric target now spans 227.5..236.5 and
is painted after the adjacent work, before deferred popups. Occlusion also hides
the root's move/up listeners; follow an owned resize with frame-scoped window
listeners so it works over the grip and peeking overlay. The fixture checks the
right-half press/release, both drag directions in peek, and boundary alignment.
Only the visible line is shortened by the card radius to avoid protruding past
rounded corners; shortening the hit target instead would make its ends unusable.

### Hover animation must not replace an interactive subtree

Wrapping a Terminal Jobs row in differently keyed enter/leave animation elements
changed the global identity of its action and tooltip descendants. Moving from
the information icon into its hoverable tooltip then disposed that tooltip.
Animate a decorative background sibling while the interactive row stays stable.
The existing tooltip regression enters the explicit information icon, moves onto
the card and scrolls to the program cwd. Scope changes also key the information
affordance by scope, so a rapid return cannot reuse the preceding scope's cached
tooltip. Compact-footer bounds are checked during hover with and without reduced
motion; target geometry must remain unchanged.
### Cyclic tab shortcuts must transfer keyboard focus explicitly

Changing `active_tab` alone leaves the old editor's focus handle alive. After
the old view disappears, consecutive frames can have empty focus paths, so the
window's focus-lost listener does not run again and shortcuts stop arriving at
Root. Arithmetic wraparound still looks correct in a model-only test. Tab
stepping sets `focusing_front` before rendering, using the existing body-focus
transfer. The isolated GPUI regression dispatches real key-down/repeat/key-up
events through Root over multiple circuits and fresh presses in both directions.
A native hold gesture still requires current macOS event and capture rights.

### Tab repeat throttling needs raw events after keymap resolution

GPUI action callbacks do not carry `KeyDownEvent::is_held`; ordinary capture
handlers run after bindings have already dispatched. An interceptor resolves
the existing keymap for this window and routes only a winning single-stroke
NextTab/PreviousTab binding to Root's raw capture handler. The handler discards
held repeats within 125 ms while fresh presses bypass the interval. There are
no timers or queued switches. Null/overridden bindings, pending multi-stroke
input and other actions retain GPUI routing; menu actions still call step_tab
directly. The raw route must consume the event so it cannot reach the editor.

### An isolated Shift release is also a GPUI keystroke

GPUI 0.3.8 synthesizes a modifier keystroke before dispatching the release's
ModifiersChangedEvent. Counting every intercepted keystroke therefore spoils
each double-Shift tap with its own release. The Root counter excludes standalone
modifier names, while ordinary shifted keys and mouse presses still spoil a tap.
The search parity Root regression dispatches actual modifier events over a focused
file and types into the resulting Search Everywhere query; detector-only tests
cannot expose this focus/dispatch interaction.

### Closing an owner's flock descriptor is not enough after a concurrent fork

A parallel child can inherit a coordinator's open file description before exec.
CLOEXEC closes it at exec, but a slow pre_exec extends flock ownership even after
the Rust owner drops its File. Workspace tests then refused an immediate reopen
as another coordinator despite dropping the original owner. Explicitly unlock in
the private claim owner's Drop before closing; never weaken admission or wait for
an unrelated child's exec. A deterministic duplicate-descriptor regression pins
release and the next live owner's continued exclusivity without process timing.

### Managed continuation needs stronger evidence than activity

Claude Stop hooks run in parallel and another hook can block stopping. Guild's
Stop observation cannot qualify managed live input, even with unchanged input
and a known current turn. OpenCode idle likewise does not prove an empty composer.
Keep their managed live paths unqualified and leave ordinary send policy alone.
Codex uses fresh exact-rollout completion, never the tentative Stop hook.

A launch keeps the same GUILD_SESSION across restoration. Delayed hooks must also
match its per-launch GUILD_RUNTIME and exact conversation before acknowledging a
continuation; filter a wrong launch before it can alter bindings or the Probe.
The nonce is launch evidence, never replacement caller/task authority.

OpenCode --session creates an ID when it is absent. A valid export followed by
history deletion reproduced an unintended startup. Reserve durably, then repeat
exact history/holder inspection before launch; a known missing history refuses
without creating a conversation. This is still a snapshot against uncooperative
external mutation, not a cross-application transaction.

A retained harness leader must not be reaped by exit observation before its owned
session is quiescent. An exited leader can leave a live member in another group.
Read-only recovery waits/refuses that member without invoking let_go or signalling
it. In GPUI fixture tests also advance the virtual clock: a coalesced terminal
update awaiting that clock can leave a naturally exited fixture looking live.

A disappearing PID is not proof of quiescence: a gated fixture forked a new live
member after the PID list, then its parent disappeared before getsid. Skipping
ESRCH admitted recovery incorrectly. Retry incomplete snapshots and require two
stable non-live session inventories including process birth identities while the
leader remains unreaped; bounded uncertainty must refuse rather than signal.

**A Codex rollout identifies a conversation, not its live writer.** A second
process can resume that conversation and append another turn while our PTY waits
for permission. Never assign managed turn ownership from generic disk starts or
let shared lifecycle events answer a managed runtime's human wait. Pair a current-launch hook's
turn with fresh disk start/completion, inspect external holders, and retire old
turns before new managed input. Status dedup must stay separate from managed
acknowledgement: PreToolUse may identify the current turn before its first prompt
hook or rollout poll, so suppressing a duplicate Board start must not suppress
the fresh source-bound start receipt. With no local turn witness, fail closed.

### Diff display text is not the copy source (2026-10-24)

`highlight::Text::of` normalizes CRLF for editor/syntax use. Reusing that string as a diff's copy source made an original byte range exceed its normalized snapshot and Copy returned nothing. Keep immutable original UTF-8 alongside source-backed display rows. Unified context rows need mappings to both sources: otherwise an old-side selection cannot cross unchanged context. GPUI regressions cover both failures, actual horizontal-tail copy and scrollbar extents that exclude fixed gutters.

### Diff control keys must stop at the viewer (2026-10-25)

Native buttons activate on KeyUp: letting their Enter KeyDown bubble from Push preview can invoke the host's Push handler first. Stop propagation without preventing ordinary button activation, and test both phases with a host sentinel. `NoAction` suppresses an Escape binding, not a satellite's parent KeyDown listener: an active diff gesture must consume that event too. Test the actual satellite frame, including its normal close behavior after cancellation. Diff source ranges and diff keys must share one CR/LF/CRLF scan; normalizing a string and tokenizing it again can collapse consecutive separators and create invalid selection rows.


### An abandoned LSP waiter is not an abandoned transport request (2026-10-30)

Returning from a Provider timeout/document-close branch, or dropping its future,
used to leave late answers in Client's map. Pruning Root's token table on a later
ask did not free those payloads. Register a request before enqueueing, give its
consumer an exclusive lease, and revoke local state on Drop before best-effort
protocol cancellation. Accept only the first answer to an active lease. Find's
auxiliary definition, symbol replacement, initialization, policy/generation
changes and server EOF need the same ownership, including answers already
received. A gated fake's post-answer notification witnesses dispatch; sleeps
alone do not prove the late answer reached the registry.


### Background syntax alone does not make a diff asynchronous (2026-10-30)

`rebuild` used to run both FileDiff::compute/hunks and Document::new on GPUI,
and recomputed every file after changing one. Moving only painting missed the
expensive path. Compute the two models from one immutable captured pair and
publish together; a result's ticket covers paint too. Clear the previous blob
halves on refetch so a fresh old side cannot combine with a previous new side.
A refresh can reuse an identical retained pair after both current halves arrive.

A real diff worker/blob reader now wakes GPUI directly. Only fixtures that own
these real threads opt into `cx.executor().allow_parking()`, like the PTY tests;
waiting for readiness belongs outside an entity update, never in a synchronous
production fallback hidden behind cfg(test).

### Sharing the label alone does not deduplicate a worktree refresh

Each old project worker listed and enriched every row of the same repository.
Sharing only `worktree list` but repeating times/line counts per subscriber leaves
the expensive fan-out in place. Root now assigns the whole read to one existing
worker per admitted common directory. Dirty/count invalidation remains separate
from list equality: an unchanged list does not prove unchanged working-tree lines.
A response captured before a completed write cannot settle its create/remove
waiter; the next epoch revokes that response even when the worker has already
finished and Root has collected it in the same polling turn.

### A polling tick must not revoke stationary work slower than its cadence

Root's shared Git reader advanced the desired epoch for every ordinary
`Some(false)` request. A read/enrichment spanning three seconds could therefore
be revoked on every tick and never publish. Coalesce an observation after active
work; only forced/count-invalidating freshness revokes its authority. The real
Root regression holds the followup too, proving the first new three-row snapshot
publishes before a restarted second read can make the test pass accidentally.

- **Group stop is not pipe EOF.** A different-group process can retain stdout/stderr. Revoke transport admission, wake capacity waits and interrupt local nonblocking IO before off-thread joins; never broaden kill authority to make EOF happen.
- **Closing the last PTY master changes child input.** Keep a cleanup anchor through captured-group signals. On Darwin, drain bounded pending output during grace and close the anchor before wait/reap; an exiting leader can wait on its tty. A closed UI reader must not create an early empty read or an unbounded join.
- **Queued is not consumed.** An input permit covers the active write and whole paste wrappers/Enter. Refusal precedes baseline/epoch/receipt success; failure after admission remains uncertain and cannot authorize a duplicate continuation. Quit finalization must not re-enter App to poll receipts.

### A custom title bar must own native window dragging as well as clicks

A transparent titlebar still belongs to AppKit unless WindowOptions sets
`app_owns_titlebar_drag`. Stopping a control's click cannot prevent AppKit
from moving the window when that press becomes a drag. Claim ownership,
start the OS move only from background presses, and stop mouse-down propagation
on every interactive control (including unavailable indicators and disabled
arrows). Occluding controls also blocks ancestor hover/move handlers, breaking
branch-source hover and column peek tracking. Keep gaps draggable and leave double-click policy to the existing
system-preference handler. Transparent full-Root previews need the same option;
satellites with a system title bar keep native ownership. TestWindow cannot
execute `start_window_move`, so dispatch tests observe its call boundary and
native gesture checks still require an isolated window and current permissions.

### A native alert's default button is not its keyboard focus

An NSAlert button can accept `makeFirstResponder` while `canBecomeKeyView` is
false with Full Keyboard Access off: Return still works, but Tab skips the
buttons. Close sheets give their own buttons keyboard eligibility, an explicit
cyclic key-view loop and initial Cancel focus, without changing system defaults.
`NSWindowWillBeginSheetNotification` arrives before `attachedSheet` is set, so
configuration must wait until AppKit finishes attaching the sheet. Keep that
observer scoped to the question and restore the previous Guild input on Cancel.
GPUI's test prompt cannot cover AppKit navigation; run the native regression with
`cargo run -p guild-desktop --features native-preview --example close-dialog-focus`
in addition to the close-handler tests.

### Image fit must follow current layout, not the previous painted viewport

A file preview whose explicit image size was calculated from last frame's canvas
measurement kept a 900-point illustration after resizing its pane to 360. Fit
uses current percentage constraints and Img's Contain fit, capped at intrinsic
size; only the scale label and subsequent explicit zoom consult the measurement.
The regression resizes 900 → 360 → 900 and checks bounds/centering plus actual
size. GPUI SVG raster dimensions also include its public smoothing factor;
dividing it out avoids showing a 23 × 41 SVG as 46 × 82. Its raster cap remains
a renderer limitation, not a promise of original dimensions for huge SVGs.

### A decoded preview still owns navigation, focus and atlas entries

SVG decoding is asynchronous: NotText before source arrives is not a definitive
failure. Keep the destination and search selection until the source is installed;
the existing five-frame selection budget starts after readiness. A preview's
focus handle must be tracked by its rendered container or Save/Close disappear
from key dispatch even though ordinary typing does nothing. Direct RenderImage
Arcs also bypass cache cleanup: explicitly drop_image on replacement and final
release. Defer cleanup past the current Window lease, since App.windows excludes
that window during its update. Tests must check actual key handlers and painted
platform atlas entries, not just hidden input or dropped CPU references.

### Resize ownership must exist before the first drag frame

The column registered window listeners only after a press was painted; dock,
Log, area, pane and Find splits still relied on Root's hovered hitbox. A slow
move over an occluding grip, a text field consuming bubble events, or a release
before the next paint could lose the gesture. Root and Push now mount capture
listeners on every frame, act only for an owned resize, apply the release
coordinate before clearing ownership, and recover on motion without Left.
The isolated Root regression covers two-point moves over the grip and repeated
press/release, including a release without a move and both column limits.

Find and Push also discarded slow movement by rounding each incremental delta
through Split's hundredth and then forgetting it. Keep the pressed fraction and
pointer, as area/pane borders do. Forty one-point moves must move the split in
both directions; the existing rounding and minimum sizes remain the model's.

### Native update controls need a focus boundary in empty Settings (2026-11-01)

A native button's own focus and keyboard click handlers do not defeat Root's
no-body fallback. With no project, a disclosure click redraws Settings and Root
can immediately reclaim focus, so Space/Enter never reach that button. The update
card retains a non-tab-stop focus boundary; the fallback recognizes descendants
of that boundary. Do not reuse the search field's focus handle for the card.
The actual Settings regression clicks a disclosure, redraws, then sends a clean
Space down/up pair. Native button activation occurs on KeyUp; a keydown-only
simulation cannot prove activation. Native isolated-window checks also exercise
Tab, category selection and independent disclosures after repaint.

### Release-note overlays need a local toolkit Tab context (2026-11-01)

Guild masks the outer Kit Root's Tab/Shift+Tab bindings under `guild` so terminal
and panel ownership survives. A native Button still activates with Enter/Space,
but a new overlay outside Settings' traversal host will not move focus on Tab
just because it contains native buttons or a FocusTrapContainer. The Update card
and What's New use a deeper local `Root` key context, restoring the toolkit's
own navigation and modal focus trap without rebinding the terminal context.
Actual Root tests click Added, send Tab and a full Space down/up pair, then assert
the other category's real release content. Keep the modal and popover focus
boundaries separate: a retained modal trap handle must not become a later
popover's boundary. Pointer continuity is measured over the card's whole bounds;
child buttons can occlude a parent's hover hitbox without leaving the card.

### Remote release pages must keep one authentication visibility (2026-11-02)

An anonymous release listing excludes drafts that an authorized owner can see.
If a later page is throttled, retrying only that page through gh shifts the page
boundaries: a draft can repeat an earlier ID or hide a release between pages.
Discard the partial listing and restart at page 1 with one transport for the
whole pass, retaining the original shared deadline and page bound. Listing
recovery does not cover the separate metadata asset-fetch fallback; test both.


### Search selection and repository menus must not rebuild their inputs

Search navigation asks for rows before drawing them. Rebuilding the tree in
both paths made twenty Down events build the same tree 82 times. The immutable
presentation now survives selection-only redraws; invalidate it when results,
folds, project paths or an empty query change. Keep the previous results while
a replacement query is running. A result count alone cannot identify a result
set.

Do not stat a repository's main directory in sidebar rendering. Resolve the
available folder when its menu opens and again when Copy Path or Reveal runs:
a main checkout can disappear or return while the menu is open. The fallback
is the first existing opened root: a buffer or session can retain a Desk record
for the missing main checkout, so the first record is not necessarily available.
If no root exists, preserve the previous best-known path (first retained root,
then captured target when there are no retained roots).


### A background file open changes when its editor becomes available

A constructor must not read a FIFO or ordinary file on GPUI. Validate the path
and opened handle, use nonblocking open for replacement races, and bound actual
bytes rather than trusting stat. When completion creates the editor, publish
its lifecycle event and realize it in the owning window, including a tab that
was hidden after Open. A never-rendered restored tab can retain a pending
snapshot. Search replacement already owns a worker-validated snapshot: adopt
that snapshot for its newly opened editor and revoke the redundant read, or
replacement silently refuses every previously unopened file. Tests must await
IO and any subsequent display-frame caret landing, not unwrap the editor in
the constructor's stack. Edits, saves and moves supersede outstanding reads.

Existing single-file diffs must attach on that lifecycle event too: opening a
source from a diff no longer supplies an editor immediately. Resolve current
project-origin holders by FileTab identity, and deduplicate following by editor
identity so normal edits do not rebuild subscriptions. Before any snapshot
exists, search must omit the overlay rather than substitute empty text for disk;
empty loaded and dirty buffers still count as real overlays. Search snapshots
are captured, so a later file completion cannot repair omitted matches.

### Encoded GIF size does not bound retained canvas frames

GPUI's image decoder expands every partial GIF frame to the logical canvas and
retains the results. Check canvas, frame rectangles and full-canvas frame count
through the same gif parser's metadata-only public API before calling GPUI;
checking the encoded file or the tiny update rectangle alone admits huge decoded
allocations. Preflight and decode must use the same immutable bytes. File preview
validation and decoded-image ownership are described in `docs/runtime-ownership.md`.

### Change events do not cover programmatic editor replacement

The editor's `set_value` deliberately suppresses `InputEvent::Change`. Caret-based
occurrence backgrounds therefore observe notifications and compare an immutable
rope clone by identity as well as the selection. Listening only to Change leaves
stale ranges after reload; comparing flattened strings on every notification
scans the file on caret blinking. `editor::Occurrences` uses the locked rope's
constant-time clone identity check and only scans changed text or a new identifier.

The range Fill's fallback foreground is the input engine's presentation default,
not a promise to use Guild's `text.primary`. It produced invisible white fills
in the Light native fixture. Occurrences supply the Guild palette role explicitly
and repaint that independent collection from `FileTab::restyle`; the all-palette
visibility/contrast regression protects the light case as well as dark themes.

A reload installed during drawing can replace the text without waking an
editor observer before that frame ends. `FileTab::install_read` explicitly
refreshes occurrences after `set_value`, just as it reconciles local Find.
The supported-large-file → small-file reload regression covers the real path;
the identity cache makes a later duplicate notification cheap.


Source snippets must notify the view that actually renders them. An immutable
snapshot shared across excerpt lines needs a retained observation in each mounted
source line; reading its entity alone does not invalidate a cached dock/popup.
The keyed link owns and replaces that observation with the source identity.
Diff occurrences use raw source ranges, including CR/LF/CRLF separators, never a
concatenation of displayed unified rows. An old-side selection crossing unified
context must paint with that context's old mapping.


### Native capture is not permission to take concurrent keyboard input

A foreground diagnostic window plus global CG event posting can receive the
person's concurrent typing, even though the fixture and PID are isolated. Stop
that trial rather than treating unexpected text as an editor regression. The
native example can show a background window without `WindowOptions.focus` or
`cx.activate`; raw CGWindow captures still work. Process-directed CG events alone
may be ignored without a key native window, so delivery needs observed behavior.
For a driver requiring frontmost activation, arrange an exclusive short input
pause. Derive pointer coordinates from actual window bounds and PNG backing
scale, never the resized image viewer. Keep failed/intermediate captures out of
the final gallery and name the native limitation separately from GPUI tests.
### A file URL does not select the default web browser

GPUI's macOS `open_url` delegates to `NSWorkspace openURL:`, so a `file:` URL
follows the file association: an HTML page can open in an editor. Resolve the
application for HTTPS and pass that application URL explicitly to NSWorkspace's
open API. Keep this in `desktop::system`; do not change the person's associations.
A native check on a machine whose HTML handler is already its web browser cannot
distinguish the routes. The browser protocol regression supplies different HTML
and HTTPS applications and checks the actual lookup and launch messages.

### Shell metadata must travel with synchronized replay

A raw OSC observer can see OSC133 C while ANSI Processor still buffers all preceding
text: the authoritative cursor is row 0 when the command boundary belongs to row 1,
and is already row 2 after release. Byte-by-byte prefix flushing does not fix it.
Normalize only recognized shell metadata into owned, one-time exact callback tokens
and let Processor replay them through the Term adapter. Normal titles and replies
must remain ordinary data. Zsh also emits CSI J *after* OSC133 A: clearing below the
cursor must revoke affected link locations without deleting that new prompt boundary.

### Printable input revokes managed readiness before platform commit

Moving printable bytes from on_key to the platform text handler created a gap where
a physical key no longer changed the input epoch until composition committed. The
public CLI recovery epoch-race fixture then accepted a continuation after user
activity. Record physical intent as uncertain input immediately; only the platform
commit proves inserted text. Never restore the duplicate raw key_char send to fix
provenance, and never infer a submitted turn from preedit or shell OSC boundaries.

### The bundled diagram helper must load Mermaid as a script, not eval

The official Mermaid 11.12 browser file works as a classic Blob-backed script
in system WKWebView; evaluating identical bytes through eval fails inside its
`__esbuild_esm_mermaid_nm` shim. PlantUML's official TeaVM ES module works through
Blob import; the file-based module experiment hung. Real helper tests cover
both load paths, Cyrillic, syntax failures and later recovery. An async IIFE
passed to evaluateJavaScript must be void: returning its Promise produces the
native "unsupported type" error while the asynchronous script is still running.
Snapshots preserve SVG CSS/text/foreignObject as PNG for GPUI; pixel tests reject
white snapshots and label-erased controls. Cargo/check never compile or fetch the
renderer. Explicit desktop/bundle preparation under `tools/diagrams/renderer` uses
the installed Apple toolchain and vendored JS; the app ships those resources.

The former optional component card/Settings focus workaround was removed when
shipping became built-in. No component installer remains. Bundle paths belong to
the running app, even after copying/renaming it; a development override must not
replace missing production resources. Sign the helper before writing its digest,
then seal the outer app without re-signing nested code afterward.

### The editor's public surface has one selection, and setting it drops the rest

`gpui-component`'s `InputState` keeps several cursors, but outside the crate
only the active one can be read (`selected_range`, `cursor`) or set:
`set_selected_range` and `insert` remove every other cursor, and nothing
public adds one. An edit of ours therefore acts at the active cursor and
leaves it the only one — `editor/editing.rs` says so where it applies — and
*Add Selection for Next Occurrence* cannot be built without a door upstream.
`replace` with no range set is the one call that reaches every cursor, with
the *same* text at each. On macOS the component binds `shift-alt-up/down` to
nothing — `AddCursorAbove/Below` are `cmd-alt-up/down` there — so those two
strokes were free for Move Line.

The history has the same blind spot: a replacement records whatever is
selected when it starts as undo's selection and a caret at the end of the
new text as redo's, and nothing public sets either. Selecting the range to
`replace` therefore makes undo select it; `editing::Journal` puts back the
action's own selections once the component's undo or redo has run. It must
mirror the history's position, never search it by text: equal texts recur
(Toggle Case twice), and a discarded redo branch or a typed letter can
reproduce an old pair exactly. So it follows only its own changes — told
apart by the buffer's rope instance — and ends at any other.

### Two roots in one checkout are two git threads (2026-11-16)

A project can be a checkout and another a folder inside it; each root gets
its own `StatusWorker`, so the per-root thread serializes nothing between
them. Auto fast-forward's review found a `switch` from one thread landing
between the other's preflight and its `merge --ff-only`, which then moved
the branch switched to. git has no single command that checks the branch
`HEAD` names and the checkout before a fast-forward (`update-ref --stdin`'s
`symref-verify` guards the ref, not the files). Every checkout-touching
`git::write` now waits for `git::turn` of its canonical checkout, and a
caller that needs several steps uninterrupted holds the turn and uses
`write_in_turn`. A process outside Guild is still not covered.

### An unknown enum value moves the whole settings file aside (2026-11-17)

`Settings` is one `serde` struct. A value its enum does not know — a theme a
newer build added, a mode an older one never had — fails the parse of the
*whole file*, and `Store::load` then moves it aside as `.unreadable-…` with
every other setting in it. `two-themes-customize` reads every new theme field
through a lenient `deserialize_with` (an unknown mode is System, a misshapen
tint is dropped alone) and never writes a new value into the old `theme` key:
writing `"cloud"` there would make the file unreadable to a build older than
Cloud. Tests feed unknown and malformed values through `Store::load` and
check that nothing was moved.

### An older build's save drops what it does not know (2026-11-17)

`#[serde(default)]` ignores unknown keys on read, and a re-serialised struct
writes only the fields it has. An older Guild beside a newer one therefore
erases `theme_mode` and `theme_custom` the first time it saves; the newer
build then migrates again from the preserved `theme` and the person's tints
are gone. There is no cross-process merge for any setting; say so rather than
promise one.

### At one OKLab lightness, hues differ in WCAG luminance (2026-11-17)

Keeping OKLab L keeps perceived lightness, not WCAG's relative luminance: a
green ground at Dark's lightness is brighter for WCAG than the brown it
replaces. A contrast guard that demanded the built-in contrast on the
selection grounds therefore refused nearly every hue on Dark while primary
text still read at six to one; the guard is WCAG's 4.5 there (or the
built-in value where lower). And lightening a saturated accent with its
chroma fixed leaves sRGB — `#0000ff` cannot be made legible on Dark's card
that way — so the accent's guard reduces chroma as the gamut requires.

### A closed tab's conversation is still held for a second (2026-11-22)

Closing a tab hangs its terminal up and gives the program a grace before the
kill and the reap, on a thread (`Emulator::let_go`). The record is closed at
once, so a resume pressed in that window found no tab and no open record and
started a second `claude --resume` on the same transcript while the first was
still exiting. A session terminal is counted in `terminal::retiring` from the
hang-up until its child is reaped; `resume_past` waits for it off the window's
thread and refuses after three seconds, and `guild session resume` refuses
while it is counted. `cmd-shift-t` still reopens at once.
