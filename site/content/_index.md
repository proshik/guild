---
title: Guild
layout: hextra-home
description: "Guild is a native macOS window for running many AI agents: it makes worktrees, starts Claude Code, Codex and OpenCode in them, shows who needs you first, and answers a command line your agents can use too."
---

{{< landing/hero
  title="Run your agents from one window"
  subtitle="Guild makes worktrees, starts agents and shows who needs you first."
  shot="sessions"
  alt="The Guild window: the column lists five agent sessions by who needs you first, waiting, done and idle, beside a diff an agent proposed with Apply and Refuse"
>}}

{{< landing/cli id="cli" title="Drive it from the command line"
  lead="Projects, workspaces, worktrees and sessions come from `guild` as well as from the window, typed by you or by an agent in one of its tabs."
  note="Every session Guild starts has `guild` on its PATH. An agent that was itself given a task cannot start another one."
  link="https://github.com/proshik/guild/blob/main/docs/guide.md#the-window"
  video="orchestrate"
  alt="Claude Code runs guild worktree create and guild agent wait; a readme worktree appears in the column and Codex works in it"
>}}
{{< landing/cmd run="guild workspace create NAME" >}}
A named set of projects; `guild workspace select` switches to it.
{{< /landing/cmd >}}
{{< landing/cmd run="guild worktree open PATH" >}}
A repository joins the column, with every worktree git lists for it.
{{< /landing/cmd >}}
{{< landing/cmd run="guild worktree create NAME --prompt TEXT" >}}
A new worktree, and an agent started in it with the task.
{{< /landing/cmd >}}
{{< landing/cmd run="guild session new --group NAME" >}}
A session over a group’s folder, where its projects sit side by side.
{{< /landing/cmd >}}
{{< landing/cmd run="guild agent list" >}}
Every session, and what its agent is doing.
{{< /landing/cmd >}}
{{< landing/cmd run="guild agent wait SESSION" >}}
Waits for the agent’s turn to end; `guild agent read` returns what it said.
{{< /landing/cmd >}}
{{< /landing/cli >}}

{{< landing/phases id="phases" title="From a worktree to a pushed commit"
  lead="Each step as it looks in Guild. Pick one, or let them play in turn."
>}}
{{< landing/phase id="organize" label="Organize" icon="folder-tree" video="open"
  title="Projects, worktrees and workspaces in one column"
  lead="A project is a repository, and its rows are its worktrees, read from git. Gather projects into groups, and sets of them into named workspaces."
  link="https://github.com/proshik/guild/blob/main/docs/guide.md#the-window"
  alt="A worktree made with git in a terminal appears in the column; it is opened with its files and main.go"
>}}
{{< landing/detail title="Worktrees from anywhere" >}}
One made by hand or by an agent in a terminal is there as surely as one made here.
{{< /landing/detail >}}
{{< landing/detail title="Groups bound to a folder" >}}
A group can sit on the folder that holds its repositories; a session there sees them all.
{{< /landing/detail >}}
{{< landing/detail title="Removed with care" >}}
Before a worktree goes, a sheet names what git would delete without a word.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="start" label="Start" icon="terminal" video="delegate"
  title="An agent per worktree, in a tab"
  lead="Start Claude Code, Codex or OpenCode in a worktree, from its `+` or with `guild session new`. Each runs its own CLI in Guild’s terminal, the way it runs anywhere else."
  link="https://github.com/proshik/guild/blob/main/docs/guide.md#terminal-sessions-in-the-desktop"
  alt="Codex at work in a worktree’s tab: it edits tides.go, adds a test and runs go test"
>}}
{{< landing/detail title="Your launch command" >}}
A prefix and arguments per agent, for a wrapper or a model of your choice.
{{< /landing/detail >}}
{{< landing/detail title="Areas and panes" >}}
Split the screen into areas, and a tab into panes, each with its own agent.
{{< /landing/detail >}}
{{< landing/detail title="Pick up where it stopped" >}}
A closed conversation is continued from the start screen or the Sessions list.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="follow" label="Follow" icon="bell" video="follow"
  title="See who needs you first"
  lead="With many agents at once, switch the column to Sessions: those waiting for you come first, then those working, done and idle. A tab waiting for an answer is marked."
  link="https://github.com/proshik/guild/blob/main/CHANGELOG.md"
  alt="The column in Sessions mode: two agents start working, one finishes, the other waits for an answer and is opened"
>}}
{{< landing/detail title="Reported by the agent" >}}
Each CLI says itself whether it is working, asking or finished.
{{< /landing/detail >}}
{{< landing/detail title="Unread until read" >}}
A finished turn stays marked until you have looked at it.
{{< /landing/detail >}}
{{< landing/detail title="Settled" >}}
Agents idle for days and closed conversations wait at the bottom, ready to continue.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="review" label="Review" icon="eye" video="review"
  title="Read the change before it lands"
  lead="With `claude --ide`, an edit arrives as a diff tab with Apply and Refuse. Any change opens side by side, and one hunk can be staged on its own."
  link="https://github.com/proshik/guild/blob/main/docs/guide.md#the-window"
  alt="An agent’s proposed change is applied from its diff tab; then one hunk of main.go is staged from the side-by-side diff"
>}}
{{< landing/detail title="The agent waits for you" >}}
Refuse, and it moves on without writing; apply, and it writes the file in its own turn.
{{< /landing/detail >}}
{{< landing/detail title="Stage a hunk" >}}
The `+` beside a change stages that change alone, and the `−` takes it back out.
{{< /landing/detail >}}
{{< landing/detail title="Who wrote this" >}}
Annotate shows who wrote each run of lines and when; a click opens the commit.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< landing/phase id="ship" label="Ship" icon="upload" video="ship"
  title="Commit and push from the same window"
  lead="Tick what goes into the commit and write its message. The Log searches the whole history, and Push Commits shows what is about to leave."
  link="https://github.com/proshik/guild/blob/main/docs/guide.md#the-window"
  alt="A commit is written in the Commit panel, appears at the top of the Log, and is pushed from the Push Commits window"
>}}
{{< landing/detail title="Branches and stashes" >}}
Switch, create and delete branches, and apply or drop a stash, where git agrees.
{{< /landing/detail >}}
{{< landing/detail title="The default branch kept current" >}}
A clean `main` or `master` is fast-forwarded to its upstream, checked every five minutes.
{{< /landing/detail >}}
{{< landing/detail title="Every write in the Console" >}}
Each command the window ran, with git’s own words back.
{{< /landing/detail >}}
{{< /landing/phase >}}
{{< /landing/phases >}}

{{< landing/catalog id="more" title="Also built in"
  lead="Everything else the window does, by area, and where to find it."
>}}
{{< landing/area id="agents" name="Agents & terminal" >}}
{{< landing/entry name="Three agents" where="+ beside the tabs" >}}
Claude Code, Codex and OpenCode, each its own CLI in a terminal of Guild’s own.
{{< /landing/entry >}}
{{< landing/entry name="Status from the agent" where="Tab and column marks" >}}
Working, waiting for you, finished, and unread until you have looked.
{{< /landing/entry >}}
{{< landing/entry name="Sessions mode" where="Column ▸ Sessions" >}}
Every agent of the workspace, by who needs you first; Settled holds the idle and the closed.
{{< /landing/entry >}}
{{< landing/entry name="Diffs from Claude Code" where="Diff tab" >}}
With `claude --ide`, an edit arrives as a diff with Apply and Refuse.
{{< /landing/entry >}}
{{< landing/entry name="`guild` for agents" where="guild help" >}}
Worktrees, sessions and waiting on another agent, from the agent’s own terminal.
{{< /landing/entry >}}
{{< landing/entry name="Your launch command" where="Settings ▸ Tabs & agents" >}}
A prefix and arguments per agent, for a wrapper or a model of your choice.
{{< /landing/entry >}}
{{< landing/entry name="Links in the terminal" where="⌘-click" >}}
A file path opens in a tab at its line; a link is followed.
{{< /landing/entry >}}
{{< landing/entry name="Find in the terminal" where="Terminal" >}}
Search the history, with copy on select and bracketed paste.
{{< /landing/entry >}}
{{< landing/entry name="Continue a conversation" where="Start screen" >}}
What was said in a folder before, ready to resume.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="editor" name="Editor & search" >}}
{{< landing/entry name="Search Everywhere" where="⇧⇧" >}}
Files, symbols and actions in one popup.
{{< /landing/entry >}}
{{< landing/entry name="Find in Files" where="⌘⇧F" >}}
Hits above a preview of the code around them, on ripgrep’s own libraries.
{{< /landing/entry >}}
{{< landing/entry name="Replace in the project" where="⌘⇧R" >}}
Across every file of the project, from the same search.
{{< /landing/entry >}}
{{< landing/entry name="Go to Symbol and Find Action" where="⌥⌘O, ⇧⌘A" >}}
Jump to a declaration, or to any command by its name.
{{< /landing/entry >}}
{{< landing/entry name="Language servers" where="⌥F7" >}}
Definitions, usages and problems from the project’s servers.
{{< /landing/entry >}}
{{< landing/entry name="Thirty-four grammars" where="File tab" >}}
The same colours in a tab, a diff, Annotate and a commit’s files.
{{< /landing/entry >}}
{{< landing/entry name="Line editing keys" where="⌘D, ⌥⇧↑, ⌘/" >}}
Duplicate, move lines, comment, and extend the selection with ⌘↑.
{{< /landing/entry >}}
{{< landing/entry name="Saved as you type" where="300 ms" >}}
Written after the last keystroke, never over a change made on disk.
{{< /landing/entry >}}
{{< landing/entry name="Explorer" where="⌘B" >}}
`.gitignore` respected, git’s marks on every row, files dragged to and from Finder.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="git" name="Git" >}}
{{< landing/entry name="Commit panel" where="⌘K" >}}
Tick what goes in, write the message, commit with ⌘↩.
{{< /landing/entry >}}
{{< landing/entry name="Stage a hunk" where="+ in a diff" >}}
One change goes into the index, or back out, on its own.
{{< /landing/entry >}}
{{< landing/entry name="Side-by-side diffs" where="⇧⌘D" >}}
Every changed file, against the last commit, the index or any commit.
{{< /landing/entry >}}
{{< landing/entry name="Log" where="Git ▸ Log" >}}
The whole history, filtered by branch, user, date or a hash.
{{< /landing/entry >}}
{{< landing/entry name="Annotate" where="Annotate" >}}
Who wrote each run of lines and when; a click opens the commit.
{{< /landing/entry >}}
{{< landing/entry name="Push Commits" where="⇧⌘K" >}}
What is about to leave, file by file, before it does.
{{< /landing/entry >}}
{{< landing/entry name="Branches and stashes" where="Branch in the title bar" >}}
Switch, create and delete branches; apply or drop a stash.
{{< /landing/entry >}}
{{< landing/entry name="Default branch kept current" where="Settings ▸ Projects" >}}
A clean `main` or `master` is fast-forwarded to its upstream, checked every five minutes.
{{< /landing/entry >}}
{{< landing/entry name="Console" where="Git ▸ Console" >}}
Every write the window made, with git’s own words back.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< landing/area id="window" name="Window & settings" >}}
{{< landing/entry name="Areas and panes" where="⌃⌘⇧→, ⌃⌘⌥→" >}}
Split the screen into areas, and a tab into panes.
{{< /landing/entry >}}
{{< landing/entry name="Tool windows" where="⌘B, ⌘L, ⌘J" >}}
Explorer, Commit, Git and the rest in docks you move them between.
{{< /landing/entry >}}
{{< landing/entry name="Workspaces and groups" where="Column" >}}
Named sets of projects, and groups bound to the folder that holds them.
{{< /landing/entry >}}
{{< landing/entry name="Recent Projects" where="⌥⇧⌘O" >}}
The whole path matched, so two checkouts of one repository stay apart.
{{< /landing/entry >}}
{{< landing/entry name="A keymap of your own" where="Settings ▸ Keymap" >}}
Press the new keys, and they are in force at once.
{{< /landing/entry >}}
{{< landing/entry name="Light, dark and your tint" where="Settings ▸ Appearance" >}}
Follow macOS or pick a theme, and tint it with a colour of your own.
{{< /landing/entry >}}
{{< landing/entry name="Your plans’ limits" where="Title bar" >}}
How much of your Claude Code or Codex plan is left.
{{< /landing/entry >}}
{{< landing/entry name="Open In" where="Title bar" >}}
The project in your editor, terminal or Finder.
{{< /landing/entry >}}
{{< landing/entry name="Updates" where="Title bar" >}}
A new nightly shows up as Update, with what it brings.
{{< /landing/entry >}}
{{< /landing/area >}}
{{< /landing/catalog >}}

{{< landing/bento id="local" title="Local, and yours"
  lead="A window on your Mac around the agent CLIs you already use. Nothing in it needs a server."
>}}
{{< landing/tile size="lead" title="The agents’ own CLIs" shot="tile-start"
  alt="A worktree’s start screen in Guild: Claude Code, OpenCode, Codex and a terminal to start"
>}}
Claude Code, Codex and OpenCode each run their own command line, permission prompts and slash
commands included, in a terminal of Guild’s own.
{{< /landing/tile >}}
{{< landing/tile size="side" figure="21" title="named git writes, and nothing else" >}}
`--force`, `--hard` and `-D` are spelled nowhere in Guild, and a rollback asks first.
{{< /landing/tile >}}
{{< landing/tile size="side" figure="~/.config/guild" mono="true" title="No server, no account" >}}
Sessions, tabs and settings are files on your Mac.
{{< /landing/tile >}}
{{< landing/tile size="half" title="Your settings left alone" >}}
Guild hands its hooks only to the sessions it starts. Your `~/.claude/settings.json` is never
edited.
{{< /landing/tile >}}
{{< landing/tile size="half" title="Native, in Rust" >}}
One Rust workspace on GPUI. The terminal is Guild’s own, on alacritty_terminal.
{{< /landing/tile >}}
{{< /landing/bento >}}

{{< landing/interview id="faq" title="Questions"
  link="https://github.com/proshik/guild#readme" linktext="The README goes into the detail"
>}}
{{< landing/qa q="Which agents does Guild run?" >}}
Claude Code, Codex and OpenCode, each in its own CLI. Their status needs Claude Code 2.1.295,
Codex 0.163.0 or OpenCode 2.2 or later.
[Terminal sessions](https://github.com/proshik/guild/blob/main/docs/guide.md#terminal-sessions-in-the-desktop)
{{< /landing/qa >}}
{{< landing/qa q="Can an agent drive Guild?" >}}
Yes. Every session Guild starts has `guild` on its PATH: an agent can make a worktree, start
another agent there with a task, wait for it and read its answer. One that was itself given a task
cannot start another. [The window](https://github.com/proshik/guild/blob/main/docs/guide.md#the-window)
{{< /landing/qa >}}
{{< landing/qa q="Does it need an account or a server?" >}}
No. Sessions, tabs and settings are local files in `~/.config/guild`.
[Running it](https://github.com/proshik/guild/blob/main/docs/guide.md#running-it)
{{< /landing/qa >}}
{{< landing/qa q="Does Guild change my agent’s settings?" >}}
No. Its hooks go only to the sessions it starts, through a settings file of its own; your
`~/.claude/settings.json` stays as it is.
[Project rules](https://github.com/proshik/guild/blob/main/AGENTS.md)
{{< /landing/qa >}}
{{< landing/qa q="Can it rewrite my git history?" >}}
Guild writes to a repository through twenty-one named git commands and nothing else. `--force`,
`--hard`, `-D` and `stash pop` are spelled nowhere, and a rollback asks first.
[Decisions](https://github.com/proshik/guild/blob/main/docs/decisions.md)
{{< /landing/qa >}}
{{< landing/qa q="How does it update?" >}}
Guild is released as nightlies. When one is ready, Update appears in the title bar, with what it
brings. [Changelog](https://github.com/proshik/guild/blob/main/CHANGELOG.md)
{{< /landing/qa >}}
{{< landing/qa q="Is Guild open source?" >}}
Yes: Guild is licensed under [MIT](https://github.com/proshik/guild/blob/main/LICENSE-MIT) or
[Apache 2.0](https://github.com/proshik/guild/blob/main/LICENSE-APACHE), at your option.
{{< /landing/qa >}}
{{< landing/qa q="What does it run on?" >}}
macOS 13 or later, on Apple silicon.
[Releases](https://github.com/proshik/guild/releases)
{{< /landing/qa >}}
{{< /landing/interview >}}

{{< landing/close title="Download Guild" shot="close"
  alt="The Commit panel with a message ready, beside the Git log"
>}}
1. Download Guild from the [releases](https://github.com/proshik/guild/releases) and move it to
   Applications.
2. Choose **File ▸ Open…** and pick a repository.
3. Choose an agent on the start screen that appears, or with the agent button beside the tabs.

Guild is not notarized yet, so macOS refuses it the first time. On macOS 15 and later, open it once, then choose Open Anyway in System Settings ▸ Privacy & Security. On macOS 13 and 14, Control-click it in Applications and choose Open.
{{< /landing/close >}}
