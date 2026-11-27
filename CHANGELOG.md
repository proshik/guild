# Changelog

What changed in Guild, release by release, for the person who works in it.
The form is [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/);
a release is a nightly from `main`, named by its date. A change a person
would notice adds its line under `Unreleased` in its own branch.

## [Unreleased]

### Fixed

- Guild's very first launch no longer opens What's new with the whole changelog; it appears from the next update on, with what that update brings.
- The empty window offers **Open…** first, to pick a folder that is already on the disk, and its rows can now be clicked, not only reached by their keys.
- Until you pick a default agent in Settings, a new tab opens the first agent you have installed — Claude Code, then Codex, then OpenCode, else a terminal — instead of always offering Claude Code. Starting an agent that is not installed now says so and gives the page to install it from, and `cmd-t` and **+** say it too instead of opening a tab that closes at once.
- The Explorer no longer renames, moves, cuts or sends to the Trash a folder that is open in Guild as a project, or that holds one: those menu items are disabled with a note saying why, the row cannot be dragged, and their keys show a warning instead.
- Guild no longer sets up Codex or OpenCode on a Mac where you have never used them: opening the window no longer creates `~/.codex`, `~/.config/opencode` or OpenCode's other folders, and no longer starts either program in the background. Their status hooks are added the first time you start them from Guild, so the status still shows from that first session.
- Reopening Guild with a Codex or OpenCode tab saved, after that program was uninstalled, no longer recreates its settings folder; and Guild adding its status hooks to Codex's `config.toml` no longer undoes a change Codex saved there at the same moment.

## [nightly-2026-11-27] - 2026-11-27

### Added

- A repository's header in the column says where its merge requests stand: a clock while a check is waiting, turning arrows while GitLab is being asked, and struck-through arrows in brass when they could not be checked — glab missing or not logged in, a network or proxy problem — with the reason in its hover. When all is well nothing is shown; point at the header to see a faint ✓ and when they were last checked.
- A merge request you created shows a small person after its number in the column, with "You created this merge request" in its hover. Guild asks glab which account you are logged in with on that GitLab, once per check.

### Changed

- Switching to another workspace and back no longer blanks the merge request icons in the column: the ones last read show at once, and they are brought up to date in the background.
- The repository's main branch is no longer asked for a merge request: its row shows the plain branch icon, and GitLab gets one request fewer per repository.
- A worktree's row no longer explains a missing merge request icon with "glab not found" or "glab is not logged in": that is about the whole repository, not one branch, and the row keeps the merge request it last read.
- The project column's edge no longer lights a line under the pointer; the resize cursor alone shows it can be dragged.

### Fixed

- The column no longer redraws itself whole every time merge requests are checked in the background and nothing about them changed; with many repositories that was a short stall every two minutes.
- When one branch's merge request could not be read, the other branches of the same project no longer lose theirs: each branch shows what was read for it, and only a branch whose own read failed says so in its hover.
- A project closed in one workspace, Guild restarted, and the project then reopened there by `guild session new` or `guild open` before you switch to that workspace — or a project in another workspace whose disk was not mounted when Guild started and is back before you switch to it — no longer loses that workspace's sessions: its tabs and conversations are all there when you visit it. A session removed while a project's disk is not mounted no longer comes back once the disk is.

## [nightly-2026-11-26-b20911a] - 2026-11-26

### Changed

- Guild is open source: its code is at https://github.com/proshik/guild, under the MIT or the Apache 2.0 licence, whichever you choose. Nightlies, and the updates the Update button offers, are published from that repository.

## [nightly-2026-11-26] - 2026-11-26

### Added

- In the column's Sessions mode, each status section — Waiting for you, Working, Done, Idle — has a chevron on its heading. A collapsed section's heading, with its count, moves to the bottom of the column, above Settled, and the list scrolls above it; click it there to expand the section back into its place. Settled now stays at the very bottom, set apart by a line, with muted rows; when open it takes at most half the column and its rows scroll inside. Which sections are collapsed and whether Settled is open are kept after a restart, the same in every workspace.
- A worktree whose branch has a GitLab merge request shows it in the column: the icon's colour says whether it can be merged, its corner badge says what is left — unresolved threads, an approval, a running or failed pipeline, a rebase — a hover lists what stands in the way, including a block GitLab names itself, and a click opens it. The status bar shows the active worktree's first open point and a link to its merge request, and the row's menu has View MR (⌘⌥G) and Copy MR URL. Commits not pushed yet show as ↑N, with or without a merge request. It uses your glab login and asks GitLab nothing else; Settings → Projects can turn it off.

### Fixed

- A long file path or web address that Claude Code or Codex broke onto the next line in the terminal is one link: `cmd`-hover underlines all of it on every line, and a `cmd`-click on any part opens it.

## [nightly-2026-11-24] - 2026-11-24

### Added

- One change of a file can go into the index, or back out of it, without the rest: in a diff, the `+` beside a change stages it and the `−` unstages it, and the toolbar's button does it for the current change; in the editor, a change marker's popup has the same button beside Rollback, and a change already in the index has an outlined marker. After you stage or unstage a change this way, the file's tick in the Commit panel follows what is staged: partial when only part of it is, and a partial tick commits just what is staged, while a full tick commits the whole file, whatever is already staged; Commit pressed right after waits for it, and a file you exclude stays excluded when it is edited. Stage Hunk and Unstage Hunk can be given keys in the keymap. A change already partly staged is staged or unstaged from the Unstaged or Staged diff; one that differs from the index only in line endings goes in with the whole file. If the file changed since its changes were shown, nothing is written and they are shown again, and a commit waiting for that write is not made; a new, deleted, renamed, linked or converted file is staged whole.

- A project group can be bound to a folder that holds its repositories, from the `…` now on the group's header. The folder is the first row under the header, named by its branch or its folder name, and `+` on it starts a session there, where the projects that sit in the folder are its subdirectories; the row says how many projects sit directly in the folder, and a project that does not — elsewhere, or deeper inside it — says `outside` on its own row. The group's `+` adds projects to it from a list. With the folder's row in front, the Explorer shows the whole folder; files of a project in it open in that project, with its language server. `guild session new --group NAME` starts such a session, and `guild group check` says how the groups stand against their folders.

### Changed

- A folder with only some of its files included shows its partial tick as a bolder dash that is easier to see.

### Fixed

- A session started by `guild session new`, `guild session new --group` or `guild worktree create --prompt` without `--front` in a workspace you have not opened since Guild started no longer wipes that workspace's saved tabs and split areas; they are there when you open it, with the new session among them, and you stay where you were. A task stopped with `guild agent stop` before you open that workspace stays stopped instead of starting again when you do.
- Scrolling the column no longer stutters with dozens of projects and sessions open, and the window does far less work while an agent's ring turns in the column.

## [nightly-2026-11-22] - 2026-11-22

### Added

- A button at the column's right edge in the title bar switches the column between Projects and Sessions; Toggle Session List does the same. Sessions lists the workspace's agents by who needs you first — waiting for you, working, done, idle — each row with its state, project, time since it last did something, branch and changes, and agent. Agents idle for days, and closed conversations their agent wrote to in that time, wait under Settled at the end, where a closed one can be continued; how many days is set in Settings → Tabs & agents. The list has its own search, arrow keys and Enter, a menu on each row and a filter by agent, state and project.

### Changed

- The status bar's Sessions button counts agents by state — working, waiting, done, failed when one has, and idle last in grey — and says on hover how many of each agent; with no agent open there is nothing to list, and it is not shown.
- The sessions list it opens is one column ordered by who needs you first, without the Running, Queue and Timeline tabs; each row says its state in words beside its mark, and the list now lines up with its button.
- The column can no longer be made narrower than the title bar's controls over it — the traffic lights, the history arrows and the mode button — so the mode button always sits inside it; a column saved narrower is widened to fit, and the narrowest width grows with the interface size.

### Fixed

- `guild agent list` marks a session that is open but has no tab in this window — one open in another Guild window that shares the project's folder, or in a workspace this window does not have — rather than listing it as if it ran here.
- Resuming a conversation from the start screen's Recent list a moment after closing its tab waits for its last process to end instead of starting a second one on the same conversation, and says so if it does not end; the window no longer pauses while the press checks the agent and its conversation on disk.

### Removed

- The project's session page that Toggle Session List opened over the tab. Closed conversations from the last few days are under Settled in the column's Sessions mode — as many days as Settings → Tabs & agents says. Older ones may still be on the start screen's Recent list, which keeps a project's latest conversations only; past that, resume them from the agent itself.

## [nightly-2026-11-21-94e548ec] - 2026-11-21

### Changed

- Rows in the Explorer and its libraries, the Commit panel, a commit's files in the Git log, Push Commits' files, Find and Find Usages share one look and behaviour: the selected row is filled in blue without an outline while its list has the keyboard and in grey while it does not, the row under the pointer turns a soft grey, the pointer stays an arrow, and file names are in the interface font. The arrows move the selection itself, and so does typing to search a list: the found row is selected.
- Command with ↑ or ↓ in those lists moves the selection just as ↑ or ↓ alone does, instead of moving an unseen cursor, so Push Commits always has a file for its diff; Command-Space and Control-Space no longer toggle a row there.
- A right click on a file or folder in the Explorer selects it before its menu opens; one that is already among several selected keeps them all selected.
- A folder in the Explorer that a dragged file will land in is lit in the colour of a dock that takes a dragged tool, so it is no longer mistaken for the selection.
- The parts macOS draws for Guild follow its Light or Dark theme rather than the system's: window title bars and traffic lights, the Push Commits and diff windows, open and save panels, the prompt before quitting and the menu bar's menus. With the theme set to System they follow macOS as before.

### Fixed

- In the Dark theme, a lit row, a secondary button and the other raised parts now stand out from the card they are on: the lit row in Go to File, Search Everywhere and Recent Files, the open commit in the git log, the Show output button in Terminal Jobs and the harness disc on the start screen, among others.
- Recent Files highlights its chosen file in the same grey as the other pickers rather than in blue.
- The Sessions button in the status bar shows its tick when every session is idle, and the Terminal Jobs list shows the icons it was missing: the square that cancels a running job, the info button beside every job, and the icons on a job's Show output, Hide and Delete output buttons. The diff window's file list shows the folder arrows and the tick beside the current file, and the agents menu shows a repository's chosen icon on its tile.
- With no project open, the title bar's Open In button now looks unavailable, its icons faded like the back and forward arrows beside it, instead of looking ready and doing nothing when clicked.
- The Terminal tool's tabs now light up under the pointer, as the buttons in a panel's head do, and the × on a terminal's or a run's tab lights up over its tab instead of showing a flat grey square.
- Dates in the Git log are shown whole — Yesterday 19:30 and 14.11.2026 no longer lose their first characters; when a row is short of room the author, the commit message and the branch names end in an ellipsis instead of being cut off.
- A narrow Commit panel keeps its name whole in the panel's head: the toolbar buttons that do not fit move into the ⋮ menu instead of the name being cut to COM, and a head too narrow even for the name ends it with an ellipsis.
- Built-in types such as `u32` and `int64` are drawn in the keyword's colour in the editor as well, as they already were in diffs, Annotate and previews, and a C macro's name in `#define` is drawn as a macro in the editor too.
- Code in a Markdown preview, inline and in blocks, is drawn in your code font, as in the editor and in diffs, instead of Menlo, and follows the font chosen in Settings.
- The code preview under Search Everywhere, Go to File and Find in Files is drawn at the editor's code size and line spacing instead of a smaller, denser copy of it.
- Branch, star, check, arrow, close, plus and dot marks across the window — in the column, the title bar, the Git log and its filters, the branches popup, Remove Worktree, Run and Process, the agents list, tab and search crosses, the font size buttons in Settings — are drawn as the window's own icons instead of characters of the interface font, which looked different in every font and smaller than the icons beside them.
- The window no longer grows wider by itself, past the screen and taking the column down to its narrowest, when a language server starts or indexes with a file open. The server's line in the status bar now gives way instead: its detail is cut first, then the progress bar and then the server's name go, while the spinner and the percentage stay; the whole line is in the hint over it.

## [nightly-2026-11-21] - 2026-11-21

### Added

- Drag files and folders between the Explorer and Finder, the Desktop or any other application, in either direction, and they are copied; the originals stay where they are. Dropped on a folder they land in it, on a file in its folder, and on empty space in the project's root; a name that is already there asks first. Inside the window a drag still moves them between folders as before.

### Changed

- A worktree's list of agent and terminal tabs in the column now starts collapsed, showing how many tabs it holds; click the chevron on the worktree to expand it, and that choice is remembered per workspace. After this update every list is collapsed once, including the ones you had left open.
- Project groups and ungrouped projects now share one order in the column. To put a project or a group between others, drag it by its head to the top or bottom edge of a project or group — or onto the Projects heading for the very top: a line shows where it will go, and only a line moves anything. A project let go of on a line between top-level items stands on its own there, leaving any group; on a line between a group's projects it goes into that group there; and the bottom edge of a group's last project is the place after the group. No line is shown where letting go would change nothing, and the dragged name is drawn beside the pointer rather than over the row it is aimed at. Let go of on the middle of a group's header, a project joins the group. Let go of on another project that is in no group, it asks whether to merge the two into a group, with an outlined field for the group's name that reads Group name until you type: Merge or Enter makes the group where the other project stood, Cancel or Escape changes nothing. The other popups that ask for a name — New Workspace, Rename Workspace, New Group and Rename Group — now also show their hint while the field is empty. A new group appears where its project was, and a project joining a group no longer moves the group. Under Recent activity, Most activity and Last user message a group is placed by its most active project and its projects are sorted inside it; dragging a group or a project to a new place switches the column to Manual, keeping the order it showed, so the dragged item lands where its line was and nothing else moves. In Manual order, workspaces saved before keep their groups above the other projects until you move one.
- The column's Projects heading now stands out as the heading of its list, and reads Projects & Groups while the workspace has a group. Groups and projects are set a step to its right and a group's projects a step further, so the list reads as a tree, and a group's header shows a folder and, after its name, how many projects it holds, so a group is not mistaken for a project, which keeps its coloured letter tile.
- Search in the column now finds project groups as well as projects: a group's matching name is highlighted and reached with the arrows like a project's, and Enter expands it. A project inside a collapsed group is shown, its group open, while the search finds it, and the group is collapsed again when the search ends.
- Settings shows which project group a repository is in: under WORKSPACES a group is listed under its workspace, with a folder and how many projects it holds, and its repositories a step further in, each marked as in the column with its coloured letter tile, or with the icon chosen for it, and a grouped repository's Repository card has a Group row with the group's name.

### Fixed

- The Explorer no longer jumps when files are moved, copied, pasted or dropped into it: the progress line with Cancel now shows over the bottom of the Explorer instead of pushing the files aside, and the folder a running copy is being prepared in is not shown.

## [nightly-2026-11-17] - 2026-11-17

### Added

- Customize the Light and Dark themes: Settings → Appearance → Customize theme tints a theme's background, accent and highlight with a colour — typed as hex, picked from the screen with the eyedropper, set by hue and saturation, or one of the presets — and an intensity. The window shows the change as you make it; Save keeps it, Reset returns the theme to its built-in colours, and Copy from takes the other theme's choices. Text always stays at least as readable as in the built-in theme, and code colours do not change.
- Keep each project's default branch (main or master) up to date without a click: Guild fetches its upstream when a project opens and every 5 minutes, and fast-forwards a clean default branch to it. A branch that has diverged, a checkout with uncommitted or unsaved changes, or a failed fetch is left alone, and the reason shows in Update branch's hint and source list; the commands are in the Console. Turn it off in Settings → Projects → Auto fast-forward default branch.
- Edit a file with familiar IDE keys: Cmd+D duplicates the line or the selection, Option+Shift+Up and Down move the lines, Ctrl+Shift+J joins the next line on, Cmd+/ toggles a line comment and Cmd+Option+/ a block comment in the file's language, Shift+Return and Cmd+Option+Return start a new line below or above without splitting the current one, and Ctrl+Shift+U switches the selection or the word under the caret between upper and lower case. Each is a single undo step; with several cursors they act at the main one.
- Group the projects of a workspace under named, collapsible headers in the column, each group's projects indented under its header. Choose Move to Group ▸ New Group… in a project's menu, or drag a project onto a group's header or among its projects; drop it among the ungrouped projects to take it out. Each workspace has its own groups, listed above the ungrouped projects; the sort mode orders projects within each group, and a group's collapsed state is remembered. Rename or delete a group from its header's menu — deleting returns its projects to the list, and a group goes away by itself once its last project leaves.
- See a worktree's agent and terminal tabs listed under it in the column, joined by a thin tree line, each with its agent's mark, the tab's name and its status. Click one to bring it to the front; hover it for × to close it, which asks first when an agent is still at work, as the tab strip does. A chevron on the worktree collapses its list, remembered per workspace; a collapsed worktree shows how many tabs it holds.

### Changed

- Theme is now System, Light or Dark, and System follows macOS's appearance while Guild is open. Light is the former Cloud and Dark the former Midnight, unchanged; the other palettes are gone — if you used Light or Cloud, Guild opens in Light, and on any other palette it opens in Dark. A new install starts on System.
- An open Push Commits or diff window now changes theme with the main window.
- In a file, Cmd+Backspace deletes the whole line, and Cmd+D duplicates instead of showing the diff; Show Diff keeps Cmd+D in Changes, the Log and everywhere outside the editor, and Find Action.
- See the selected worktree in the column as a quiet, faintly tinted card with an outline, the same tint its tab in front wears; hovering a row now stands out more than the selection, and worktree rows are a little more compact.
- Update branch in the title bar is now a single ↓ button: clicking it opens a compact list of sources under it — main and master first, the one in use checked, names like `origin/main` and `main` in larger type — and choosing one asks to confirm before anything is fetched or merged; Cancel changes nothing. The source label beside the button is gone, its hint is a short "Update branch…", and the list's footer shows only how the last update went instead of the recovery text and commit hashes.
- Recent activity now keeps projects with a running agent session on top, in the order they became active: a session counts from its first turn until it fails, its agent exits or its tab is closed, and its turns, questions and pauses between turns do not move the project. The rest are sorted by their latest turn.

### Fixed

- Releasing Ctrl+Cmd closes the harness dropdown without opening a session; press Enter to open the highlighted choice.
- Update branch no longer moves other branches while it fetches: a remote with an extra configured fetch mapping (such as one that copies main onto a local branch) could have that branch overwritten by the fetch. Only the chosen source's remote-tracking branch is updated now.
- Update branch's hint shows its title, its source or the reason it cannot run, and why auto fast-forward skipped the branch on lines of their own, instead of one run-on line.

## [nightly-2026-11-14] - 2026-11-14

### Added

- Extend selections in any text file with Cmd+Up through words, quote contents, quoted text, sentences and paragraphs, with syntax-aware expressions and blocks in supported code; Cmd+Down restores the previous selections, including the original caret or mouse selection.

- Select multiple commits in Git Log with Command-click or Shift-click to inspect their combined files and diffs.
- Preview Mermaid and PlantUML files and Markdown diagram fences offline as soon as Guild is installed. Source, Split and Preview share the same editable buffer; native images fit each pane and preserve Cyrillic labels. The small helper uses system WebKit, with browser scripts shipped inside Guild.app and updated with Guild.
- Terminal text input supports platform composition and Dictation insertion, and shell-aware command navigation, output selection and exit status use the existing terminal menu. Supported launched shells report their current directory without changing personal startup files.
- Cmd-click a file path in a terminal that points outside the project to open it in a read-only tab — text at its `:line:column`, images including SVG, Markdown — with its full path in the tab's tooltip. Files Guild cannot show, such as PDFs, videos and archives, open in their default macOS application wherever they are; a program, script or installer is never handed to macOS, so nothing is run by a click.

### Changed

- Terminal Find computes match totals and the current match number in the background, with cancellation when the query changes.
- An agent tab's status now comes only from what the agent itself reports (OSC 7501): Claude Code 2.1.295 or later, Codex 0.163.0 or later, and OpenCode 2.2 (working, question mark and idle — OpenCode does not report a finished turn, so it gets no unread mark or sound). The question mark names the reason — permission, question or sign-in — only when the agent gives one. A harness version that does not report shows no status, no unread mark and no sound, and `guild agent send` refuses it with "status unavailable"; `guild agent wait` on it ends with "no status report received (OSC 7501)". Esc no longer changes a tab's status by itself.
- A finished turn you have already looked at no longer comes back as unread when the agent repeats that it is done.
- The small counts under a worktree's name in the column now show only sessions that need you — waiting `?`, failed `!` or with an unread answer `●` — each in its mark's colour; sessions still working are no longer counted there, and the row's tooltip keeps every exact number.

### Fixed

- Tab close marks accept clicks across their reserved slot and stay reliable when the pointer moves slightly during a click.
- Keep saved application preferences and workspaces intact when development diagnostics or tests run beside Guild, so reopening or updating Guild retains those preferences.
- Retrying a failed agent session keeps the current theme and terminal appearance instead of reverting to its old display settings.
- Dim terminal text respects a program's default background color, including in restored session previews and after changing themes.
- Recent activity puts the project where a request most recently started on top again, including after a restart, and still does not jump while an agent runs tools.
- Terminal ANSI underlines retain single, double, curly, dotted and dashed styles and their requested color, including underlined spaces.
- Preserve terminal combining accents and wide CJK/emoji cell geometry, including highlights and block cursors, without shifting following text or box drawing.
- Guild started from the Dock no longer runs out of open files with thirty or more agent sessions, which made saves fail with "Too many open files": it raises its open-file limit at start.
- A state file Guild cannot read because of a system error such as "Too many open files" is no longer taken for a broken one: it stays exactly where it is instead of being renamed to `.unreadable-…`, the notice gives the system's reason, and Guild does not save a default or an older copy over it until it restarts.

## [nightly-2026-11-05] - 2026-11-05

### Added

- Click a source editor Git marker to inspect the original block, move between changes in that file, or use its rollback icon with normal Undo/Redo. Rollback preserves pasted CRLF bytes outside the selected block.

- Show narrow Git line markers in the main project source editor against HEAD, including unsaved edits: green for added lines, brass for modified lines and short rust marks at deletion boundaries. Saving keeps the HEAD comparison; large or dense sources skip markers.
- Highlight matching whole identifiers in a file with a single caret click, including Unicode names, read-only library sources, diffs, annotations and source previews. Markdown keeps cross-block Preview selection, with occurrence highlighting in Source mode. To keep large and dense files responsive, occurrence backgrounds are skipped above 2 MiB or 4096 matches of the current identifier.
- Git Log shows colored parent edges for branches and merges, and its commit files can be folded by directory or with Expand All and Collapse All.

- Toggle soft line wrap for the current file from the status bar without changing its contents.

### Changed

- Use rectangular row fills from edge to edge in file trees, keeping text and icon indentation unchanged.
- Show Git file statuses as subtle row backgrounds in Explorer, Commit, Git Log and Push instead of letter badges or coloured filenames.

### Fixed

- Keep caret clicks, selection and Copy available in read-only library sources while preventing edits.
- Terminal Find keeps its close button at the top right, with balanced card padding and a wrapped screen-only hint in narrow panes.

- Cmd-clicking a local HTML page path in terminal output opens it in the default browser; paths with line numbers still open their source.

- Saving state no longer fails because another Guild process takes its temporary file.

- The default Cmd+Alt+- shortcut collapses Log files and Changes folders on macOS.

- Git Log keeps folders folded when the same commit files refresh, and keeps keyboard navigation in its file list after expanding or collapsing from speed search.

- Keep file opening and reloading responsive, reject special files and oversized GIF animations, and avoid repeated clipboard reads, diff text copies and offscreen review animation work.
- File icons, disclosure arrows, Search rows, menu headings and failed-server indicators follow the interface font size; the terminal font preview keeps the selected terminal size.
- Moving through Search results reuses the result tree, and repository folder actions follow the available main checkout without checking the filesystem while drawing the sidebar.

## [nightly-2026-11-02-1171a43a] - 2026-11-02

### Fixed

- Remote update checks reuse verified release metadata and can use authorized GitHub CLI access when anonymous requests reach GitHub’s rate limit.

## [nightly-2026-11-02] - 2026-11-02

### Added

- Open PNG, JPEG, WebP, GIF, ICO and SVG files in image tabs with fit, actual size, zoom and dimensions; switch SVG previews to their editable source.

### Changed

- Manage updates in one card with the installed version, a single current action, three shared release-note groups and icon rows across Settings, Update and What's New, and collapsible additional settings with consistent disclosure icons and keyboard focus. Restart confirmation explains what happens to current work.

### Fixed

- Keep sidebar, dock and split resize handles following the pointer through slow and repeated drags, and finish resizing reliably when released over content.
- Keep keyboard focus in What's New from its first frame and dismiss the Update card on time when the pointer moves away.

## [nightly-2026-10-31-fc279ab3] - 2026-10-31

### Fixed

- Navigate native close confirmations with Tab and Shift+Tab, use the focused button with Space or Return/Escape shortcuts, and return to the previous input after cancelling.

- Keep active projects in a stable order under Recent activity and append newly active projects below them, so activity updates no longer make the list jump.
- Keep title-bar controls clickable without moving the window when the pointer shifts; drag the window only from empty title-bar background.

## [nightly-2026-10-31] - 2026-10-31

### Added

- Control Codex sandbox access and command approvals independently, both enabled by default. Choices apply to new, resumed and retried processes, and override permission options in launch Arguments; disabling approvals keeps sandbox restrictions enforced.
- Hold Cmd+Ctrl to show the new-tab menu, choose with Up/Down and release to launch, or use Enter or a row number shown before its icon; Escape cancels; reorder the four choices in Settings → Tabs & agents.

### Fixed

- Preserve independent language-server requests when cancellation meets a full queue, release completed terminal job workers while keeping their output, and show terminal input refusals from all terminal surfaces in notices.

- Bound language-server and terminal input buffers, reject whole inputs when capacity is exhausted, and stop transport workers without waiting for inherited pipes to close.

- Reduce idle work for unchanged files and terminal job history while keeping edits, saves and review activity current.

- Keep worktree lists updating when a slow Git refresh spans several polling ticks.

- Reduce repeated Git work when several linked worktrees are open, while keeping repository lists and per-worktree changes fresh after edits and Git actions.

- Keep large diff views responsive while comparisons update, reuse unchanged files, and discard superseded results without mixing rows with newer source text.
- Release language-server requests and late replies when a wait times out, a document or search closes, or an obsolete request is cancelled, keeping concurrent requests independent.

## [nightly-2026-10-29] - 2026-10-29

### Added

- Type while the sidebar has focus to find project groups, highlight matching names, and cycle results with the arrow keys; Enter opens the selected project and Escape keeps your current project.

- Sort project groups by recent activity, observed turn count, last confirmed user message, or a saved manual order. Activity is collected from Guild sessions after this update; dragging a project switches to Manual and preserves worktree order and pins.

### Changed

- Keep Back and Forward beside Workspaces, branch controls at the content edge, and Sessions at the right of the workspace segment in the bottom status bar, opening upward and hiding with the column. Sessions shows one aggregate status beside its localized label, with a gray check-circle when inactive. Browse Running, Queue and Timeline in the session navigator, and find each footer tool on its saved dock side. The Update branch hover hint shows only its current source or blocking reason; recovery details and history are available in the source popup.

- Commit file lists share selection across mouse and keyboard actions, keep opened menus stable during refresh, show configured shortcuts in English and Russian, and offer bulk inclusion, immediate Add to Git, batch Trash and expandable untracked directories. Literal filenames no longer include matching siblings; large selections and untracked project folders work, and a preparation failure stops the commit while keeping its message. Copying a project folder’s relative path produces `.`.
- Find new-tab defaults and per-agent launch settings under Tabs & agents, with clear guidance on where each agent’s permissions are managed. Claude permission choices no longer enable OpenCode automatic approvals.

### Fixed

- Context submenus open beside the current row immediately and stay aligned when switching, scrolling or resizing.
- Let Codex use its native permissions on launch as well as resume, without overriding them with Guild’s Claude permission setting.

## [nightly-2026-10-26] - 2026-10-26

### Added

- Toggle Terminal Jobs with Ctrl+Cmd+J and Usage with Ctrl+Cmd+U; their titlebar hints show your current shortcut.

- Find and replace with case, whole-word, regex, multiline and selection controls in the editor. Replace in Files (Cmd+Shift+R) includes unsaved buffers, keeps per-file Undo and follows your save policy, refusing stale or unsafe files.

- Resume saved Claude, Codex and OpenCode tasks with a checked startup continuation, inspect readiness without restoring tabs, and continue a completed Codex turn without replaying uncertain delivery.

### Changed

- Diff inspectors place old line numbers beside the center divider, offer independent date-and-author gutters for each revision, and show clickable full change ranges beside the scrollbar. Compact author gutters sit between source-adjacent line numbers and the central connectors, with continuous separators, curved whole-block connections, color through number gutters, blue chronological blame shading and the editor’s checked Annotate with Git Blame menu.
- Diff inspectors now use compact mode/edit icons with hover hints, a distinct revision strip and a file-tree picker; file stepping keeps the selected layout stable while new content loads.
- Inspect diffs with scrollable long lines, selectable text, expandable full-file context, intraline highlights, connected change blocks and resizable columns, with clear revision labels and bounded change/file navigation.

### Fixed

- Find remote updates sooner after launch and show installed/latest versions, release notes and Download/Restart controls in Settings, with technical history collapsed.

- Show muted hints in optional new app names and blinking carets in focused search fields, keep Recent Files as a compact recent-file selector, and make control hover visible across themes.

- Hide empty-field hints while focused and keep them muted otherwise, without changing entered or suggested values. Floating dialogs keep following a drag across text fields and finish at the release position.

- Align local Find and Replace fields and show their search/navigation icons; keep the Replace in Files dialog title free of menu punctuation.

- Keep local selection search tied to the actual edit and Undo/Redo transition, including project replacement and reload. Single Replace checks only its own output size, and multiline project search keeps the correct field focused through Tab navigation.
- Keep search within its original selection after Undo/Redo. Project replacement retains excluded files across aliases, checks newly edited aliases without resolving paths on the window thread, and completes before automatic result refresh. Reopening Find/Replace in Files returns typing to its query.
- Open Search Everywhere with double Shift while an editor is focused; its query has a border and a keyboard footer.
- Open local Replace with Cmd+R instead of Run, and keep Cmd+Shift+F opening Find in Files while an editor is focused. Filename search prefers exact names and basename prefixes over weaker fuzzy matches.

- Report Claude and OpenCode live-continuation limits without looking for Codex history.
- Recover a saved OpenCode task alongside a verified idle local service while refusing foreign terminals, clients and pending human input.
- Confirm Codex continuation starts when only the current runtime’s final hook survives, while still requiring rollout completion before another submission.
- Keep proven unrelated older processes from blocking safe recovery of exited task panes.
- Both sides of split diffs offer equal fixed click strips beside their line numbers for the Annotate with Git Blame menu; mirrored number alignment keeps visible spacing symmetric, and number columns fit the file’s line count and font instead of reserving blank space.
- Diff centers stay continuous below short files, and divider lines no longer cross context labels.
- Diff file navigation keeps the selected file when lists change or folders are collapsed; double-click selection copies accented text intact and handles long words without freezing.

## [nightly-2026-10-24] - 2026-10-24

### Added

- Copy worktree paths, pin worktrees above Projects while retaining repository actions and row menus, and delete worktrees from compact hover controls with smaller glyphs, readable unopened rows, hints above the buttons branch names that truncate before the controls and balanced hover feedback on Copy, Pin and Delete. The confirmation lets you also safely delete the shown branch, with that option checked by default.

- Configure installed and manually added Open In apps in General Settings: show or hide applications, reorder them, or add named custom apps from a searchable installed-app list with their real icons.

### Changed

- Terminal Jobs shows structured review phases, live provisional findings, assessed results and explicit clean completion, with categorized finding details and saved results restored after restart, including stopped reviews.

- Terminal Jobs uses compact two-line jobs, secondary action icons and explicit launch details, with soft hover feedback and a shorter empty-output label.

- Open the current folder in the selected application with Cmd+O, or choose an application with Cmd+Shift+O; the titlebar shows and remembers the selected app. Add Project and Go to File remain available through menus and custom shortcuts.

- Enlarge compact icon controls while preserving panel sizes; use outlined single-line tooltips with current keyboard shortcuts and retain multiline diagnostics.

### Fixed

- Holding either tab-switching shortcut keeps cycling at up to eight switches per second, including after releasing and holding it again. Individual presses remain immediate.

- Open folders in external terminals instead of a Guild terminal tab; keyboard Open In immediately selects the first available action.

- Preserve typing focus when opening a folder externally, and fall back to Finder when the selected application disappears.

- Show a provider usage hint on the limits chip, including when readings are unavailable.

- The project column resize handle is centered on its boundary and highlights on hover and while dragging.

- Terminal tabs use compact vertical margins so more rows fit without changing the font or horizontal spacing.
- Failed Claude Code, Codex and OpenCode sessions show readable guidance above their retained terminal output, with an explicit guarded retry.

- Resuming a Claude conversation no longer shows a generic warning about earlier Guild instructions; concrete launch errors remain visible.

## [nightly-2026-10-21-da2df283] - 2026-10-21

### Added
- Settings now has an Experimental section: opt in to new Development workflows; existing processes remain usable when it is off.
- Ordinary development in capable Guild opens Process before design and shows contract preparation as active; the shared workflow guide keeps review and assignment on that run.
- Moving an origin or its linked Process tab keeps their views together in the same project and workspace; unrelated panes stay in place.
- Checked process review findings can return to the same assigned conversation for correction and a source-specific recheck, with retained history and shared limits. Process shows actual correction delivery separately from completion.
- Recovering a stopped workflow origin can supply its current recovery summary at startup in the same Claude or Codex conversation; Process distinguishes startup submission from observed work and parent acknowledgement.

- Recovered workflow origins can receive one current reconciliation handoff at a qualified prompt; Process shows delivery separately from acknowledgement and subsequent work.

- Recovered processes show concrete reconciliation blockers and linked sessions; the original parent can explicitly acknowledge qualified facts through `guild workflow reconcile` before the next eligible effect.

- Assigned child tabs can return to their Process and reuse the original parent, or resume its exact saved conversation after ownership checks; uncertain recovery stays visible and requires parent attention.

- Inspect configured development runs in a native Process tab, with their actual steps, linked sessions/worktrees and shared review allowance; use its compact linked tab or reopen a closed view from the origin icon; the origin tab fits its visible content without extra space after the reopen icon. You can also use `guild workflow show`.

- Configured process reviews can use the selected installed review tool, retaining linked Terminal Jobs, checked source-specific results and aggregate call usage.
- Start, inspect and cancel configured development-process runs and submit checked outcomes through the Guild command line, with frozen skill resources, retained request receipts and configured managed worktree sessions.

### Fixed
- Tasks registered before design can finalize their actual acceptance checks with the prepared contract; temporary Guild connection gaps no longer select a parallel workflow.

- Imported review usage and uncertain accounting stay charged after a refused start or restart; omitting prior receipts cannot restore allowance.

- Explicit session sends keep working after Enter without a prompt-start event; Ctrl/Cmd/Fn+Enter correctly records a submission while workflow input guards stay strict.

- Automatic corrections wait for manual submissions to be observed; multiline input remains a draft, while navigation and interrupt keys no longer create a false draft notice.

- Process explains when a draft blocks automatic continuation and when OpenCode cannot verify empty input; OpenCode restores the original conversation without an automatic recovery prompt.

- A recovered parent keeps its reconciliation acknowledgement when its turn finishes; changed ownership, source, approvals or child state still require reconciliation.

- OpenCode recovery recognizes v2 exported conversation locations while refusing conflicting saved identities.

- Closing a Process view no longer leaves its parent stuck after restart; temporary inspection queue errors keep unknown groups protected until their actual reply.

- Process conditions resolve independently of configuration order, and changed source invalidates outcomes reached through conditional paths.
- Completed reviews allow a new review of changed source; cancellation distinguishes confirmed absent owners from unknown processes and usage.
- Restored Process views retain their parent association before their run snapshot loads; a damaged run no longer blocks unrelated origins.

- Process acceptance and local delivery follow the accepted implementation checkout; managed designers can prepare a contract before the parent accepts it.
- Unfinished outcome conditions keep dependent work blocked; distinct starts preserve the origin's active run and conversation identity.
- Failed review preparation can be corrected without losing its evidence or charging a review; child API callers cannot initiate parent recovery.
- Forgetting project sessions also forgets their fallback view state and commit draft, while retaining process history; orphan Process views do not prevent unrelated tabs from moving.

- Returning from a child to an already-live parent keeps its workflow running; only actual or uncertain resume requires continuation attention.

- Closing an origin session dismisses its linked Process views after confirmation, while keeping child tabs and run history. Closing Process alone keeps the session.
- Completed configured reviews retain their checked result when later launcher or presentation receipts arrive.
- Retry a failed configured session launch in its already-created worktree, preserving the original task and refusing changed or uncertain resources.
- Development-process overrides can use roles supplied by the selected project definition.
- Creating a worktree in an aliased directory completes when Git lists its canonical path.

### Changed

- Process now has selectable configured phases, focused worker and step details, recent recorded events, and a responsive context column with contract files, evidence and actual review accounting. Selecting a phase preserves the running work.

## [nightly-2026-10-20] - 2026-10-20

### Added

- Configure a global HTTP proxy in Network settings, choose which Guild requests and new sessions use it, select proxy or direct routing per agent, bypass internal hosts, and check the connection.

### Removed

- Built-in review commands, profiles and activity views. External review tools continue through Terminal Jobs; existing native review archives and settings are preserved.

### Fixed

- Launching Claude Code, Codex or OpenCode through a prefix in zsh no longer prints a missing-function error when no same-name shell function exists.

## [nightly-2026-10-19-3f0ea5e9] - 2026-10-19

### Added

- Configure separate launch prefixes and arguments after the program for Claude Code, Codex and OpenCode in Sessions settings, including shell functions; changes apply to future terminal sessions.

## [nightly-2026-10-19] - 2026-10-19

### Added

- Choose Remote or Local updates in Settings, with Nightly and Stable channels, a local build directory, verified downloads and explicit replacement confirmation.

- Stage a completed local development build with `just build-stage`, using an optional destination directory without installing the app.
- Choose which Usage, Open In and Terminal Jobs controls appear in Appearance → Action Bar. Add them from a menu with matching icons, remove or arrange them in a compact horizontal preview by dragging or pressing Option+←/→ on a focused item; changes apply immediately across projects and survive restart. Restore defaults restores the original items and order. Shared buttons in Terminal Jobs and Review also keep balanced text and icon spacing.

### Fixed

- Holding a dragged project at either edge of the Workspaces sidebar, or a tab at either end of an overflowing tab bar, scrolls smoothly so distant targets can be reached and dropped on in one drag.
- Tab strips have a thin separator across their full width, including empty space and the edge above Settings.

- Long project names and inline rename fields stay inside the sidebar, keeping project controls reachable.

### Changed


- Agent terminal sessions discover Guild's public CLI without development workflow rules; existing Claude snapshots retain their saved text until a permitted rebuild or a fresh conversation. Guild no longer creates development workflow settings.

## [nightly-2026-10-17-d3f958f9] - 2026-10-17

### Fixed

- Quitting a saved-run viewer now closes its terminal overlay and returns keyboard focus; its exit status and retained output remain available in Terminal Jobs.

## [nightly-2026-10-17-e7150326] - 2026-10-17

### Changed

- Markdown files now open in Editor by default. Choose Editor, Editor and Preview, or Preview for newly opened files across all projects in Settings → Editor; open panes and saved views keep their modes.

### Fixed

- Opening many projects no longer lets simultaneous background Git polls exhaust file descriptors and prevent state from being saved.

## [nightly-2026-10-17] - 2026-10-17

### Removed

- Claude Code stream mode and its mode controls; existing conversations now reopen in terminals.

### Added

- An experimental “Adaptive rendering up to 120 Hz” option in Settings → Terminal, saved and applied to open terminals without restarting them. It is off by default and may increase energy use while active.
- Controllers can continue a stopped Codex task with a task file at resume, with durable receipts that prevent duplicate launch attempts.
- Open existing file paths in terminal and agent output with Cmd-click even when no line number is printed, including encoded file hyperlinks.
- Play Glass when any agent (Claude Code, Codex or OpenCode) finishes a turn you are not looking at.

### Changed
- Workspaces keeps centered marks inside its card and Open/Get from Version Control buttons on the window footer baseline; folding hides them, and the title-bar toggle pins a peek.
- The footer stays on one line as icon gaps tighten; the window retains at least 900 × 500 points of content space, and restored window/dock sizes fit the available work area.
- Native traffic-light controls appear again when hovering the top edge in macOS full screen.

- Back and Forward follow every switch of tabs, sessions, panes, worktrees and workspaces, including switches made by commands.

- Terminal integrations can register archive viewers through the neutral `archive_viewer` field; existing saved runs and older clients remain readable.

### Fixed

- Footer sizing updates after measurement, source controls clear stale hover after workspace switching, and closed side panels preserve open-panel widths.
- Quiet terminals sleep between events, unchanged terminal rows reuse prepared text, and run timers update only the visible Output panel.
- Working indicators share a bounded animation cadence, reducing unnecessary window redraws and CPU usage.

- Claude, Codex and OpenCode tabs and session rows now share worktree activity marks: a rotating working ring, a waiting question, a failure mark and an unread dot, with steady harness logos.

## [nightly-2026-10-15] - 2026-10-15

### Added

- Drag linked worktrees within a project to arrange them by priority; the order survives restart and the primary checkout stays first. Escape cancels a drag without closing panels or changing editor cursors.

### Changed

- Terminal Jobs uses compact rows and inset separators like the usage card, omits unavailable-output messages and avoids repeating the selected worktree while keeping archive recovery.

### Fixed

- Clear stale Codex permission indicators after the matching browser or other MCP tool finishes, including failed calls.

- Closed Codex tasks can receive a startup continuation while unrelated Codex sessions are running; active holders of their conversation still block reopening.

- Hidden terminal jobs no longer leave overlapping titles over the terminal; reopen their output through Terminal Jobs.
- The usage card lists the current provider only once, and its titlebar chip brightens on hover and stays interactive while the card is open.

- Visible activity rings keep rotating when another application takes focus, including session menus with unavailable navigation.

## [nightly-2026-10-13-a5fbab56] - 2026-10-13

### Changed

- Menus have clearer inset separators and red destructive actions, with shortcuts to copy a worktree path or ask to remove it; the usage card shows green, yellow or red limits with reset times beside them.

- Floating menus, pickers and cards have a clearer outline in every palette.

### Fixed

- Long Codex tasks retain their automatic approval status after large command output, without showing a false question.
- Terminal Jobs hides completed launches from deleted worktrees while retaining live jobs and saved archives.

## [nightly-2026-10-13-957e29cf] - 2026-10-13

### Fixed

- Codex sessions using automatic approval review keep showing as working while commands run, including when another project is selected.

## [nightly-2026-10-13-85b7523e] - 2026-10-13

### Added

- Settings now includes workspaces and shared repository preferences, with global language-server defaults, per-repository overrides, project icons and defaults for future worktrees, with a directory picker. Disabling a language server stops it and clears its editor results.

### Changed

- Titlebar usage, Open In and Terminal Jobs controls have equal spacing.
- Titlebar controls have clearer outlines; Settings moves from the titlebar to File, keeping its shortcut. Settings group captions use uppercase labels.
- Settings navigation runs continuously from the top with a Back action, section icons and a divider against the content; quieter surfaces, group headings and outlined controls clarify the hierarchy. Cards and rows across Guild have softer corners.
- Workspace and repository settings use compact controls, readable editable fields and an icon picker beside its trigger; repository language-server preferences appear on the same scrolling page.

### Fixed

- Codex sessions release resolved permission indicators after failed or denied commands and completed turns, while unanswered requests remain blocked when opened.
- Trackpad scrolling in files and Markdown previews stays on the initial axis through momentum; a new gesture can switch direction.
- Terminal Jobs remains visible in the titlebar even when its list is empty.
- Settings pages scroll smoothly with bounded row layout, including when the pointer crosses an editable field.
- Terminal input descriptors no longer escape into background subprocesses, and language servers stop when Guild quits before its first frame. Completed terminal output remains available.

## [nightly-2026-10-13] - 2026-10-13

### Changed

- Worktree and workspace activity separates unanswered requests, observed errors, ongoing work and unread answers. Two-line worktree summaries open a session list that reaches the exact tab and pane, including hidden sessions. Larger activity marks use a smoothly rotating gray ring with a fading tail for ongoing work.
- The footer stays below bottom tools, with workspace switching beside the column toggle and Explorer beside branch controls. Compact layouts keep controls reachable with long branch names. Restored bottom tools respond to the first mouse or keyboard resize and leave room for the work area. New workspace creation stops at five; saved workspaces above that limit remain available.

### Fixed

- Failed Claude tools release their own permission request, and quickly starting a new prompt after Escape no longer leaves activity stuck working.
- Find and Replace bars, component tooltips and editor controls follow the selected dark or light palette, including Cloud, without losing their input when themes change.
- Archived review viewers can be reopened after quitting, reuse the same live viewer from related rows, and release completed terminal resources while keeping output. Retrying Show output saves receipts again after a storage failure.

## [nightly-2026-10-12-e9b1bcb9] - 2026-10-12

### Fixed

- Worktree creation rejects names beginning with a hyphen before running Git, instead of opening a branch manual or failing after preparation.

## [nightly-2026-10-12] - 2026-10-12

### Changed

- Open saved review archives with any compatible viewer executable, regardless of its filename.

### Fixed

- Recent Files and Locations show readable, scrollable rows and fit smaller windows.
- Editor line numbers stay visible while scrolling; gutter actions reject outdated geometry.

## [nightly-2026-10-11-da2d8a25] - 2026-10-11

### Added

- Terminal Jobs opens in the current session, with worktree, project and all-launch scopes, ongoing/completed groups and preserved original launch context.

### Fixed

- Terminal Jobs labels each launch’s project, worktree, last observed branch and session. Saved review viewers have distinct status and close controls; the terminal overlay cross hides its view while preserving size and position.

- Terminal and review overlays have compact headers, readable themed tooltips and clear control cursors. Resize from any side or corner of the normal frame, with no extra margins around the content.

## [nightly-2026-10-11] - 2026-10-11

### Added

- Run Just, Make, package.json and Taskfile declarations from green editor buttons, with saved-source checks and argument inputs. Independent Run tabs keep separate commands and output, with selected rerun, stop and close controls.

### Changed

- Clicking a task triangle runs that declaration directly; missing arguments use a compact input, and Option-click edits remembered launch arguments. Run controls sit close to the right of content-sized line numbers and compact blame labels, beside folding controls.

- Git Blame now annotates the current editable file with compact date and author labels. Unsaved edits are uncommitted; clicking a committed line reveals its exact commit and historical file in Git Log.
- Source Run tabs show concise runner and task names, with source, captured directory and full command available on hover.

### Fixed

- Folded editor rows keep hidden gutter numbers and controls out of view, and scrolling over the gutter scrolls the file. Repeated clicks on a replaced task chooser no longer affect another form.
- Blame preserves historical paths when Git omits repeated metadata and retains attribution for unchanged CRLF lines.
- Task discovery follows bounded import chains with their owning grammar and directory, refreshes opened and restored source tabs when imported files change, and ignores words inside Just attribute arguments. Stale declaration clicks are rejected after buffer edits.

- The editor gutter menu uses aligned checks and compact leading spacing.
- The gutter offers Run only for files with executable tasks, and reports when a task becomes unavailable before the click.

## [nightly-2026-10-10] - 2026-10-10

### Added

- Drag selected Explorer files and folders to move them; hold over a folder to unfold it, scroll at the list edge, and press Escape to cancel.

- Copy, cut and paste Explorer files and folders with Cmd+C/X/V, or move them with F6. Name conflicts offer Skip, Keep Both or Cancel; recursive copies run in the background.

### Changed

- Explorer Rename selects the filename stem and supports Shift+F6; create a directory with Cmd+Option+N and send selected files and folders to Trash with Cmd+Backspace while the tree has focus.

### Fixed

- Renaming or moving files and folders keeps open editors, unsaved edits, undo history and pane layout, and saves to the new path.

- Long notifications now stay inside the window, including stacks of three, with scrollable text and always-visible Copy and Close controls. Click Close to dismiss; reading or scrolling a message no longer dismisses it.


## [nightly-2026-10-09-276bd5b1] - 2026-10-09

### Added

- Reopen saved revmux runs from Terminal Jobs in the original terminal interface, including after Guild restarts, without starting another review.

### Fixed

- Explorer keeps keyboard focus after renaming a closed file, creating a folder or cancelling a name. Project menu actions no longer continue navigation in a previously selected library.

- Explorer arrows move between files and folders consistently, including inside external libraries. Search and Find Usages keep individual matches reachable after selecting files. Folding or refreshing the tree keeps the cursor in its source; Enter still folds Search and Find Usages groups after ordinary navigation.

## [nightly-2026-10-09] - 2026-10-09

### Added

- Type in Explorer, Commit files, Push commits or files, and Git Log commit files to highlight and navigate names without filtering, even with the wrong Russian or English keyboard layout.

- Option+F12 shows or hides the Terminal panel without changing its selected shell.

- A movable Terminal panel opens multiple independent shells per project or worktree. Tabs keep running when hidden or moved, and new terminals start in the selected worktree directory.

### Fixed

- Workspace open, clone and hide buttons sit beside the workspace name and remain visible with long names.
- Back and forward arrows align with the left edge of the work surface, above Explorer when open, and follow workspace column resizing.

- Dock terminals keep keyboard focus when switching or closing tabs and leaving their menu; Find takes the query field. Crowded workspace controls remain reachable, and bottom resizing no longer covers the tool buttons.

- Terminal Jobs is compact, dismisses like other menus and lists newest launches first with dates and status. Output opens at a moderate size; empty, unavailable and explicitly deleted output are explained, with localized Show output and Delete saved output actions.

- Markdown Preview scrolls long documents without laying out every offscreen block, keeps inline and HTML badge rows at their intrinsic size without clipping nested illustrations or stopping image animations, and keeps selected text, code-copy icons and tooltips readable in light and dark themes.
- Go External Libraries now follows vendor mode, opens vendored sources, and refreshes when vendor metadata changes, including workspace dependencies.

### Changed
- The workspaces column stays full height while the compact branch strip separates the work area from the bottom dock. Update branch now uses a downward arrow.
- Current branch and its update icon sit below the left dock when open, and below the main panel when closed. The four footer tools stay centered across the whole work surface to the right of workspaces.
- Branch update follows fresh Git state, preserves recovery history across refusals and language changes, and keeps footer tools clear of crowded status text.
- Branch updates follow a local source’s upstream, bound stalled fetches, and retain recovery guidance with a link to the worktree’s Guild terminal. Hidden sidebars keep the full work surface; Guild Update uses the shared titlebar style.

- Navigation and Guild Update now follow the main panel. Choose a source beside Update branch to merge it into the current branch; Push Commits remains available from File and its shortcut. The footer path label is removed.

## [nightly-2026-10-08-0625e1a] - 2026-10-08

### Changed

- The gear opens Guild Settings directly. The legacy native review profile editor is removed from settings navigation; previously open editor tabs restore as Guild Settings without changing saved profiles or review history.

## [nightly-2026-10-08] - 2026-10-08

### Added

- Terminal programs can run in movable, resizable overlays without opening another tab. Hide or close keeps them running; Terminal Jobs reopens or cancels them, including after their original session closes. The `guild terminal` commands preserve completion status and wait without cancelling on timeout.

### Fixed

- Terminal Jobs uses a compact toolbar icon matching adjacent controls, and overlay close buttons use centered icons. The old Review launcher is removed from the title bar; existing native review history remains available in project settings.

- Keep light terminal prompts readable and let Guild-launched Claude follow the terminal theme.

- Markdown tables now share the document background, with readable headers, links and selection in the selected theme.

- Codex tabs retain conversation identities when hook notifications cannot reach Guild, and unidentified tabs offer a conversation picker.

## [nightly-2026-10-07] - 2026-10-07

### Added

- Markdown files open with native Editor, Editor and Preview, and Preview modes, live unsaved previews, images, and a remembered mode for each pane. Mode buttons stay beside the tabs even when the tab row overflows, separated from the surrounding controls. Markdown source wraps long lines and highlights headings, emphasis, links, code, and quotes. Paragraphs fit the pane while wide code and tables scroll independently; code blocks can be copied with one click.

- Get from Version Control clones HTTPS and SSH repositories with progress and cancellation, then opens them in the workspace where cloning began.

### Fixed

- The clone destination starts in the last project location, the current project’s parent, or your home folder, then appends the repository name automatically.

- Get from Version Control is available under File → New and in the Projects toolbar; the empty-window button is removed.

- The clone dialog groups errors with expandable Git output and places the folder chooser beside the destination.

- Text fields across Guild now support selection, cursor movement, mouse editing, undo/redo and a blinking caret, including repository URLs, searches and names.

## [nightly-2026-10-06-7691dac] - 2026-10-06

### Added

- Nightly desktop downloads can be built on GitHub as verified Apple Silicon application archives with checksums.

## [nightly-2026-10-06] - 2026-10-06

### Fixed

- Assigned sessions without a registered review task can run manual reviews; worktree implementation requests carry authorization through bounded reviews and corrections.

- Dependency queries preserve their command outcome when diagnostic output contains non-UTF-8 bytes.
- A directory that cannot be opened for a durable save is reported before replacing the file or its state backup.

- Saving a file through a symbolic link preserves the link and updates its target; a broken link is reported instead of replaced.
- Finished reviews, language servers and short CLI queries stop their remaining child processes, preventing hangs on inherited output streams.
- Stream sessions report an unexpected exit with diagnostic output instead of silently completing an unfinished turn.
- Review verifies findings classified as pre-existing or open questions before deciding whether the review passes.
- A successful state save no longer reports an earlier background write failure.

## [nightly-2026-10-05-56660ab] - 2026-10-05

### Fixed

- Run review stays available while fresh or restored Claude Code, Codex and OpenCode tabs initialize, delivering the first request once their prompt is ready.

## [nightly-2026-10-05-87be9ce] - 2026-10-05

### Fixed

- Codex tabs keep their chosen permissions after a restart and show their previous conversation while history reloads.

## [nightly-2026-10-05-3235788] - 2026-10-05

### Fixed

- Restored agent sessions can receive a Review request after restarting Guild without needing another completed turn first.

## [nightly-2026-10-05] - 2026-10-05

### Added

- Request review from the current agent session or choose another open session in the same checkout directly from Review.
- Review finders run concurrently under a bounded limit; CLI launches can explicitly skip synthesis or verification, with skipped verification reported as unverified.

- Review activity opens from the titlebar, with tab-owned compact and expanded observation. Pinned views can be moved and resized while the terminal stays usable; closing observation leaves the review running.
- Project Settings separates review profiles, future-run defaults and history from Guild Settings. Shared local settings refuse stale saves and offer reload.

### Changed

- Review uses the same simple notes icon in the titlebar and Project Settings.

- Review Defaults brings the default reviewer and automatic review profile together. Choose profiles by name; internal task-role assignments are no longer shown.

- Project Settings opens in a regular project tab, separately from Guild Settings, with Profiles, Defaults and History.

### Removed

- The extra Review status control in the bottom status bar.

### Fixed

- Compact Review keeps the standard four reviewer rows visible, with a single row for profile status and launch actions.

- Review can send the first request in a new ready agent session without requiring a previous completed turn.

- Tab and Shift-Tab reach Review card controls, and Enter activates them, while terminal Tab keeps its existing behavior.

- Long review names wrap in recent history without cutting off the title or overlapping the date and result.

- Finished and cancelled reviews no longer show undispatched stages as still waiting.

- Review cards have a clear heading, readable profile identity and a separate run-status row spaced away from the header divider.

- The Review titlebar button uses the same neutral hover and pressed colors as neighboring controls.

- Empty review popups keep their message close to the heading.

- Review history dates stay compact, project switches match personal Settings, and review profiles explain how they inherit project reviewer defaults.
- Recent reviews in the empty Review card uses the same footer-row spacing and type size as the usage card.

- Review observation keeps its controls inside the pane when the window shrinks, and expanded views show the full participant table without cutting off Verify.

- Guild Settings and Project Settings tabs use distinct monochrome icons with consistent size and styling.

- Settings navigation responds to hover and keeps selected items highlighted consistently across Guild and Project Settings; interface text follows the chosen size and palette.

- Review finding titles wrap within narrow cards, and secondary controls use the selected palette for readable text in light and dark themes.

- Profile and recorded-review headings stay readable in native Settings, and observation does not overlap its trigger tooltip.

- Project profile stages open their editable inspector immediately, with a shared Save/Cancel footer and readable configuration cards.

- Project Settings uses its designed heading and icon navigation; selected profiles and historical reviews keep their outline while hovered.

- The review control uses native hover styling without crashing debug windows at startup.

- Review results separate colour-coded findings and scenario evidence into expandable cards; activity groups tool calls, and history rows no longer overlap. Hover previews remain open while entering their controls.
- Review history shows timestamps and recorded results in a full-height scrolling journal instead of the profile editor; stage selection keeps a visible outline.
- Review pin and expand controls stay visible in both palettes, and compact activity keeps its budget footer reachable on short screens.
- Resumed Codex sessions keep review and overlay commands tied to their live tab; Guild identity is pinned for that launch’s shell commands.
- Keep review stage cards and their inspector reachable by scrolling on narrow screens.
- Empty review previews fit their content, and long recorded tasks keep the stage canvas and activity reachable. Project history explains when a review has no live tab owner.
- Manual reviews launched from an ordinary Guild session appear in that session’s activity; CLI commands sent to the desktop executable now explain the correct command without opening another window.

## [nightly-2026-10-04-2bcbe70] - 2026-10-04

### Fixed
- Harness usage stays visible with `-` before data arrives; temporary Keychain command failures no longer stop automatic refresh.

## [nightly-2026-10-04] - 2026-10-04

### Added

- Manual reviews can be launched through Guild CLI with an explicit project, file layers, profile and call budget; repeated request identities cannot dispatch another run.

- Session and pane overlays show logs or native review activity without covering other panes. Minimize and close affect only the view; work continues. Harnesses can open and update their own overlays through the Guild CLI. Review separates activity, frozen inputs and findings, with explicit cancellation and startup errors.

- Review opens beside the current session with a native stage canvas, project profiles and an emitted conversation. Launch a read-only review of selected committed, staged, unstaged or untracked changes; closing the tab keeps it running, and the status bar reopens it. Profiles can be edited, restored, imported and exported, while each round keeps its source snapshot and configuration.

### Changed

- Project review and workflow settings are initialized locally in the main checkout’s `.guild` directory and shared by its worktrees; reopening preserves personal choices.

- New Guild development tasks use OpenSpec contracts and project-configured native review, with a 12-call task budget and explicit merge or publication handoff.

- Review observation has a compact header, stage track, reviewer status table, participant filters and a separately scrolling activity feed. Tool commands and output are readable; setup evidence stays in Inputs. New runs retain timing, while older runs show unknown timing explicitly.
- The usage chip takes its figures from the sessions running in Guild's tabs first: a Codex session's own record, and a Claude Code session's status line when you have a status line of your own (it still shows exactly as before). Anthropic and Codex are asked only when no session has said anything in the last five minutes, or when you press Refresh. The card says where the figures came from and how old they are, and calls them outdated after half an hour.
- The Settings tab title starts with a capital letter in both interface languages.
- Settings now group related preferences, search across categories and shortcuts, and offer direct choices with harness logos, theme previews and keyboard focus.
- Themes have independent names: Dusk, Ember, Graphite and Midnight; Cloud adds a second light palette. Existing theme choices are preserved.

- Push Commits follows the selected Guild palette while keeping its existing inspection and push controls.
- Commit opens with Cmd+K, Run (formerly Output) with Cmd+4, Find with Cmd+3, and Git Log with Cmd+9; custom shortcuts keep their assignments.
- Terminal font controls adjust by half a point in both directions.
- Project and External Libraries files, Find/Search file groups, Commit, Git Log files and Push files support Command-click toggles, Shift ranges, keyboard selection and copying selected paths. Commit can include or exclude selected files with Space and open their diffs with Enter. Deselected or folded files cannot remain implicit action targets.

### Fixed

- Minimized overlays reopen from a compact corner button without reserving an empty row above the terminal.
- Long tool output in Review and its session overlay starts as a compact preview. Click Show full output to read it, then Collapse output to return; the archive keeps the complete output.

- Review uses the same unpainted resize gaps as the main workspace, shows profile descriptions and a visible Delete profile action, and keeps activity compact until a run starts. Storage paths are available on hover.

- Review immediately highlights the selected profile and opens its selected stage for editing. Navigation and inspector widths can be resized, and profile forms have a draggable overall scrollbar with fixed save controls.

- Review cards have space between them and can be dragged without changing the pipeline. Profile navigation separates library actions and history, stage choices use keyboard-accessible popup menus, and titlebar controls share a height with more space before Settings.

- Review separates profile navigation and stage settings into framed panels, draws connected reviewer cards on a dotted canvas, and shows readable instruction summaries instead of raw prompt metadata.

- The usage card no longer says "couldn't read" when Anthropic is limiting how often the figures are asked: it says "rate limited", keeps the last figures, waits longer before asking again and picks up Claude Code's current sign-in.
- Codex records its conversation identity at session start for restart and update recovery. A missing identity keeps the session visible with an explanation instead of silently starting a new conversation.
- App bundles identify each build and icon resource separately, and update refreshes the replaced bundle's timestamp so macOS can notice the new icon.

## [nightly-2026-10-02-6d8110c] - 2026-10-02

### Changed

- Guild has a new app icon: two silver contours on a graphite tile.
- Push Commits uses dark dialog colours on both sides, with branch and multiple-commit inspection, an editable destination, diff preview, optional commit details and a Run Git hooks choice.

### Fixed

- Agent commands read the exact Claude Code, Codex or OpenCode conversation, report missing telemetry as unknown, and keep Codex hooks tied to their own Guild session.
- The Push button's options arrow stays visible and grows with the interface font size.
- Push Commits and the separate diff window follow the interface font size and family, including changes while open. Push controls grow with the text, its actions move to a second footer row when needed, and tooltips use the selected interface font.

## [nightly-2026-10-02] - 2026-10-02

### Fixed

- Removing a worktree waits for Guild to finish saving its state and closes it in every workspace, so `.guild` cannot reappear during deletion.
- Dragging a background tab brings it to the front and focuses its contents as soon as the drag begins.
- A dropped tab settles into its final position and briefly lights up, making the completed move visible.
- Dropping a tab uses the place where the mouse is released, so a fast drag across several tabs no longer stops at the last intermediate position.
- Cmd+T and New Session open the default tab selected in Settings, including changes made while Guild is running.
- A command-line worktree removal refused for changes now explains how to find the tracked or untracked paths blocking it, and why ignored-file consent cannot bypass them.

## [nightly-2026-10-01] - 2026-10-01

### Added

- The work divides into areas, each a card with its own tabs — `⌃⌘⇧→` beside, `⌃⌘⇧↓` below — and a tab divides into panes — `⌃⌘⌥→`, `⌃⌘⌥↓`; `⌃⌘⇧W` closes an area. A new area or pane starts empty, on the start screen; `⌃⌘` and an arrow moves between them, and a tab dragged onto another area's strip, or sent there from its menu's *Move to Area*, moves without restarting. All of it comes back after a restart.
- Swift files get a language server — Apple's `sourcekit-lsp`, which comes with Xcode and its command line tools: go to definition, find usages and problems. It starts in a Swift package (`Package.swift`) or in an Xcode project set up with `xcode-build-server config` (`buildServer.json`); anywhere else the status bar says which of the two it needs.
- In an Xcode project, the language servers' list offers *Set up for Xcode*: it runs `xcode-build-server config`, which writes `buildServer.json`, and starts Swift's server.
- Go to definition from Swift into the standard library or an SDK framework opens the module's interface, into an Objective-C framework opens its header, and into a Swift package opens its source — read only, under External Libraries.
- A dependency's file in front — opened by go to definition or from External Libraries — is shown in the Explorer: External Libraries unfolds to it and its row is selected, and Reveal Active File does the same.
- A worktree with no tab open shows a start screen: every harness and the terminal to start, and the recent sessions of each to resume.
- `guild session resume` brings a closed session back into its tab — a task whose tab was closed can be continued by the agent that started it.

### Changed

- The tab strip's ⋮ menu no longer carries Settings — it opens from the title bar's gear and ⌘,
- The side panels are cards of their own beside the work, as the bottom panel already was.
- Confirmation sheets ask one short question with the thing's name in it — closing a tab says whether its agent is mid-turn or waiting for you — and add a line under it only where something is lost for good.
- A tab where a session Guild knows is resumed — `claude --resume`, `opencode --session` or `codex resume` in a terminal, or `/resume` in a Claude tab — becomes that session's tab, and a task resumed there stays a task.

### Fixed
- OpenCode tasks now start with their message instead of treating it as a folder to open.
- OpenCode tabs report their turns again, so closed tasks can resume and receive messages through `guild`.

- Closing a Codex or opencode tab asks first, as closing a Claude Code tab does; the setting is now *Ask before closing an agent's tab*.
- A session that starts with a slash command is named by the command, `/fix-review`, instead of its markup.
- A closed Codex or opencode tab comes back with its conversation — from Reopen Closed Tab, the Sessions list or the start screen, and after a restart — as a Claude Code one does; it used to be forgotten the moment it closed.
- A reopened Codex or opencode tab says again what its agent is doing, and `guild` recognises the session in it; it used to go silent once reopened.

## [nightly-2026-09-26] - 2026-09-26

### Added

- `guild worktree create NAME --from-file PATH` (or `--prompt`) makes the worktree and opens a session in it that starts on the task, in one command.
- `guild worktree create --base REF` starts the new branch from REF.
- The usage card reads Codex's limits too, and the chip shows the limits of the agent in the tab you are in, with its logo; every agent is listed under *All providers*.

### Changed

- `guild worktree create` starts the new branch from the repository's main branch, not from whatever branch the checkout it was run from is on.
- Sessions Guild starts are told that work for a new worktree belongs to a session in that worktree.

### Fixed

- Coming back to a project or a worktree brings forward the tab you left there, not its last one — after switching projects, switching workspaces or restarting Guild.
- A session an agent starts in another project's worktree opens in the workspace that holds that project, not in the agent's own.

## [nightly-2026-09-25-66f8297] - 2026-09-25

### Changed

- Codex and OpenCode wear their own logos in their own colours — on their tabs, the add button and its menu, and the usage card.

### Fixed

- A tab's cross no longer hides the end of its name: under the pointer it takes the place of the tab's icon, and the tab keeps its width.
- The Settings tab's cog is as large as the other tabs' icons.

## [nightly-2026-09-25-9f7ad1d] - 2026-09-25

### Added

- Lilex Duo, a second face for code, in Settings as *Lilex Duo (Beta)*.

### Changed

- Code and the terminal have a font each: *Monospace font* for files and diffs, *Terminal font* for terminal tabs, chosen with a button per face or *Custom* for any installed one.
- Every palette draws the interface in the system font, code in Lilex and the terminal in Lilex Nerd Font; before, only Slate did.

### Removed

- The build and the `↑N` count at the right end of the status bar. Update in the title bar offers a new release, and About says which one is running.

## [nightly-2026-09-25] - 2026-09-25

### Added

- Update in the title bar: when a newer nightly is ready, a pill beside Back and Forward restarts Guild into it, and hovering it lists what the update brings.
- What's new: the first start of a new nightly lists what changed since the one you ran before.
- Back and Forward in the title bar, beside the branch.
- Your Claude Code plan's limits in the title bar: the share left of the scarcest window, and a card with each window and when it resets.
- Open In, beside Settings: the project's folder in Finder, in a terminal tab or in an installed editor, or its path copied.
- The terminal passes the mouse to programs that ask for it — a press, a drag and a release.
- Option works as Meta in the terminal.
- A file dropped on a terminal is typed as its path.
- Annotate with Git Blame from the right button on the editor's line numbers.
- The Git Console selects and copies like an editor.

### Changed

- The terminal uses a leading of 16 on 12, and fits more lines.
- Block and box-drawing characters, dashes and double lines are drawn to the cell's edges, so `claude`'s frames meet without gaps.
- A jump to a line centres the line it lands on.
- Closing the tab in front brings its neighbour forward, and the keyboard follows it.
- Push Commits opens on the main window's display, over it.
- A row in the column shows its `+N −M` on the second line, beside its age.

### Removed

- The project popup in the title bar: the branch beside it named the project already. Switch projects from the column or Recent Projects.

### Fixed

- Closing a terminal tab hangs up its shell and the jobs it started.
- A selection dragged out of a terminal scrolls it.
- A program that never ends a synchronized frame no longer freezes the terminal.
- A terminal started without a locale gets UTF-8.
- The selected tab's underline stays whole under the pointer.
- An idle window no longer repaints for a caret nobody can see, and a window behind another app keeps its caret steady.
- `just install` sees the Guild it is run from, and no longer swaps the bundle under a running window.
- Tests no longer leave folders behind in `$TMPDIR`.
