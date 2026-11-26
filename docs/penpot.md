# Penpot — where the window is designed

**A change a person would see in the window is drawn in Penpot and approved
before it is coded.** Penpot holds the window as the code draws it: every
screen built from one library of components, every number a token read off
`theme/`. A proposal is tried there, shown to the user, and only an approved
one becomes code.

Read this before UI work. The numbers themselves are in `theme/` and
`docs/design-system.md`; this document says where their picture lives and how
it is kept true.

---

## Where

The instance is Penpot's cloud, `https://design.penpot.app`, team
`7b7d55f9-965b-8000-8008-f839b2bb5db1`. Two files:

- **Guild — Library** (`42288968-6e5e-808d-8008-fc91921f4a35`), a shared
  library. Pages:
  - **Foundation** — *Swatches — active theme* (every colour token, bound),
    *Palettes — values side by side* (each palette's literal value per
    token, so Light can be read without switching the theme) and the
    *Type specimen*.
  - **Components** — one top-level board per area of the window, each a
    section of its components: Icons, Primitives, Title bar, Title bar
    states, Tabs, Column, Status bar, Work area, Code, Editor, Explorer,
    Changes, Diff, Git, Popups, Settings, Start, Dialogs, Notices, Updates,
    Usage, Terminal & Run, Problems & servers, Agents & Process, Project
    groups.
  - The token catalog.
- **Guild — Screens** (`42288968-6e5e-808d-8008-fc91921f4a36`). Pages
  **Screens** and **Screens — dialogs**. Every instance on them is a copy
  of a Library component; nothing on a screen is drawn by hand that the
  Library has. A third page, **Proposals**, holds the boards of changes
  drawn and not yet implemented; a board goes once its change is merged
  and promoted.

**Fonts.** Custom fonts belong to a team: Lilex and Lilex Nerd Font are
uploaded from `crates/desktop/assets/fonts/`, and a new team needs them
uploaded again.

**Names.**
- **Components** are `area/name` — `process/phase`, `settings/page network`,
  `icon/svg`, `markdown/modes` — and their layers `area / name`.
- **Variant properties and values** are capitalised words for real states
  only: `State=Rest|Hover|Selected`, `Selected=Yes|No`,
  `Mode=Editor|Split|Preview`. A state the code cannot reach is not a variant.
- **Screens** are 1180 × 760 boards, the size of the dev window's captures,
  named for what they show: `Main window — Process`, `Settings — Network`,
  `Settings — Network (lower)` for the same page scrolled, `Popup — …`,
  `Menu — …`. A file tab in front no longer widens the window, so the file
  screens are 1180 too. A board that shows one part at several widths, as
  `Status bar — a language server at work` does, is as wide as its widest
  frame.

**Tokens.**
- **Sets.** `common` holds every measure: `font.*`, `space.*`, `radius.*`,
  `border.*` after `theme/`'s constants, and `view.<area>.<measure>` for
  what the view code spells inline, each with its source file in the
  description. `palette/light` and `palette/dark` hold the colours,
  `color.<group>.<name>`, with the same names in both.
- **Themes.** Two themes, **Dark** (the default, active) and **Light**, each
  `common` plus its palette.
- **`color.system.*`** are AppKit's colours (title bar, title, traffic
  lights), measured from window captures in each macOS appearance. They are
  not Guild's. The traffic lights' key colours are the same in both
  appearances; the comment over the `system` table in the token script says
  how each value was measured. Since Guild hands its Theme mode to AppKit,
  each palette's `color.system.*` is what AppKit draws in that mode, not
  only the appearance it is usually seen with.
- **A value change does not repaint.** Penpot re-resolves a shape bound to
  a token only when the token is bound again: after loading a changed value,
  write it on every bound shape and bind once more, in both files.

---

## The process for a visible change

1. **Read what draws it.** Read the code that draws the part (`view/`,
   `theme/`) and the current Penpot screens that show it. If Penpot no longer
   matches the code, bring it up to date first, so the proposal is made
   against what the code actually draws.
2. **Draw the proposal on the Screens file's Proposals page.** The page is
   *Proposals*, made at first use, and each board is named
   `<change> — <screen>`. Build it from
   Library components and tokens, with real states only. A new component is
   drafted on the same page under the change's name, not in the Library.
3. **Get the user's approval before any code.** Show exports of the proposal
   boards, and of the current screens beside them where the change replaces
   something.
4. **Implement it.** `theme/` stays the source of every number. A change that
   needs a new measure or colour adds it in `theme/` and in Penpot together:
   the code first, then `scripts/penpot-tokens.py`, whose output Penpot loads.
5. **Verify it in the dev window.** Check the running build in an isolated dev
   window (below) against the approved boards, and say what was compared and
   what differs.
6. **Bring Penpot to what was merged, in the same branch's work.**
   - Promote the proposal: its components into the Library, its screens onto
     Screens or Screens — dialogs.
   - Relink every screen copy a structural change made stale.
   - Remove the proposal boards.
   - Run the token script and check counts and hashes against both files.
7. **Some changes need no design.** A refactor, a test, a document, or
   behaviour nobody can see in the window — the same line as the changelog's.

---

## How to connect

- **The MCP server runs locally.** It is the npm package `@penpot/mcp`
  2.19.1, installed in `~/.local/share/penpot-mcp`; its launcher builds a
  runtime copy in `~/.cache/penpot-mcp/<version>`. A user LaunchAgent,
  `local.penpot-mcp` (`~/Library/LaunchAgents/local.penpot-mcp.plist`), starts
  it at login and keeps it alive; its log is `~/Library/Logs/penpot-mcp.log`.
  It listens on localhost only: 4400 serves the plugin, 4401 is the MCP
  endpoint and 4402 the plugin's WebSocket. Claude Code's user-scope server
  `penpot` points at `http://localhost:4401/mcp` (Streamable HTTP), and no
  key is involved. Read the server's `high_level_overview` first.
  - **The hosted MCP of design.penpot.app is not used.** With its plugin
    connected and a fresh key, it answered every call with "No Penpot
    instance connected for user token".
  - **The package follows the cloud's Penpot.** 2.19.1 is the published
    version nearest the cloud's Penpot 2.19.3. When Penpot updates, move to
    the npm version that matches it, not to `latest` blindly: `latest` was
    the older 2.15.4.
- **The plugin tab.** The tools act through a browser tab with the file open
  and the local plugin connected. The toolbar's Plugins (puzzle) icon installs
  `http://localhost:4400/manifest.json`; grant it access, Open it and press
  "Connect to MCP server" until it says "Connected". Its window stays open.
  The toolbar's own "MCP" button is the hosted server — not the one to press.
  The tools act on that tab's current page.
- **The tab must stay awake.** Use the dedicated Chrome profile started without
  background throttling. A Claude-in-Chrome tab in the ordinary browser works
  too, but sleeps: take a screenshot of it right before every call, or calls
  run ten times slower and time out.
- **One file at a time.** To switch files, release the tab with
  `penpot.closePlugin()` (the call itself never returns), navigate to the other
  file's URL, and reload the tab. Without the reload the text engine can stop
  re-measuring text. The plugin then reconnects by itself.
- **A duplicate connection.** After a reload the server can keep the dead
  connection and refuse the new one ("Duplicate connection for given user
  token"), and every call then fails. The server is ours: restart its agent
  with `launchctl kickstart -k gui/$(id -u)/local.penpot-mcp`.
- **When saving stops.**
  - **Look at the tab's banner first.** "Penpot cannot reach the server" is
    the frontend losing the server, not the last operation's fault.
  - **Check with a server-side export.** A new shape that exports as a hidden
    0.01 svg has not been saved.
  - **Never reload a tab with unsaved changes.** Wait, re-check, and report.

---

## Working rules that have cost us

- **Keep calls short.** About 60 texts or one screen per call, well under the
  plugin's 120 s. A call that outruns it keeps running in the background and
  can stop saving.
- **Probe saves.** After a big batch, export something just made — the export
  server reads the saved file a few seconds behind the plugin.
- **Never delete and re-create a token in one call.** Saving once stopped and
  the queue was lost. To move a token between sets, add it in one call and
  remove the old one in the next.
- **Never switch the token theme in Guild — Screens.** The switch re-resolves
  the whole file and freezes auto-width texts at their main's width, and icon
  boards come apart. Check Light values in the Library, on Foundation.
- **Relink every screen after a structural change.** A change to a Library
  component's structure reaches no existing copy: the Update banner does not
  carry structure. Swap each stale copy on every screen, nested ones included.
  - **A swap resets the copy's overrides** — labels, icons, layer names. Note
    them first and set them back, then compare an export.
- **Tokens fail silently in several ways.**
  - **A wrong-type token is ignored** without an error. A sizing or border
    token on a padding or gap does nothing; add a spacing alias
    (`{space.button_side}`) to the script instead.
  - **A radius needs the four corner properties.**
  - **Applying a token a property already holds unbinds it.**
- **A copy's structure cannot change.** Children cannot be added to or
  reordered in a copy. A screen that needs extra rows detaches its outer
  screen-level copy; the parts inside stay linked.
- **Fix text that renders in a serif fallback.** A new text sometimes exports
  in a serif fallback, or with stale glyph colours after a theme change.
  Changing its characters (append a space, pause, restore) fixes it.
- **Capture the dev window this way.** Compare against the dev window, never
  the person's Guild.
  - **Build and isolate it.** Run `cargo build -p guild-desktop`, then run it
    from a fixture project with its own `GUILD_STATE_DIR`, with `GUILD_SOCKET`,
    `GUILD_SESSION` and `GUILD_HOOK_SOCKET` unset and `GUILD_NO_GLOBAL_HOOKS=1`.
    `just desktop-dev <dir>` does the build and the state dir.
  - **Drive it without typing.** Use clicks addressed to its pid and menu
    items through System Events. Never type while the person is at the
    machine: keystrokes land wherever the focus is.
  - **Capture the window itself** with `screencapture -x -o -l <window id>`.
  - **Clean up.** Hand focus back to the app that was in front, and quit the
    window by pid.
  - **The screen must be unlocked**
    (`ioreg -n Root -d1 | grep CGSSessionScreenIsLocked`): a locked screen
    fails every capture and starves the plugin tab.

---

## The token script

`scripts/penpot-tokens.py` reads `crates/desktop/src/theme/` (the palettes,
the constants, the derived tints) and the measures the view code spells
inline, listed in the script with their source lines. It prints the sets as
JSON:

```bash
python3 -I scripts/penpot-tokens.py .
```

The output is `{"order", "default", "palettes", "measures"}`. `measures` is
the `common` set, and `palettes.<Kind>.tokens` is the `palette/<kind>` set.
Every entry has a type, a name and a value, which can be a Penpot expression
over other tokens (`{space.card_gap} + {border.hairline}`), and `src` says
where the value comes from.

**Load it into Penpot** per file, through the MCP server. In each set, add the
names Penpot lacks with `set.addToken({type, name, value})` and write changed
values. Removals go in a separate call (see the rules above).

**Then check both files:**

- **Counts.** Each set's token count must equal the script's.
- **Hashes.** Hash each set the same way on both sides and compare: sort the
  `name=value` lines, normalise each value (a number to six significant
  digits, anything else lower-cased), and take FNV-1a 32 over the lines
  joined with `\n`.

On 2026-11-21, after decision 54's row tokens, the counts were 531 / 74 / 74;
on 2026-11-22, after the Sessions mode's three activity gaps, 534 / 74 / 74;
on 2026-11-26, after GLD-063's nine `view.forge.*` measures, 543 / 74 / 74.

---

## Known gaps

- **Fonts are stand-ins.**
  - The UI face is Inter, standing in for SF Pro; the code and terminal faces
    are Lilex and Lilex Nerd Font.
  - Lilex has no italic in the team, so comments are upright.
- **Some states are drawn from code only.** Process's Active and Waiting steps
  and its review rows need a provider to reach, so the window was never driven
  to them.
- **AppKit's dark title bar is this machine's.** macOS tints a dark window's
  bar with the desktop picture, so `color.system.titlebar` and `title` in
  `palette/dark` hold the values measured here on 2026-11-21, from a plain
  titled AppKit window rather than the Push Commits window itself.
