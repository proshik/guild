---
id: GLD-063
title: A worktree's GitLab merge request, its state and what blocks it, seen in the column
kind: feature
status: backlog
priority: 2
component: desktop
depends_on: []
touches:
  - crates/session/src/merge_requests/**
  - crates/session/src/lib.rs
  - crates/session/Cargo.toml
  - crates/session/tests/all/main.rs
  - crates/session/tests/all/merge_requests_glab.rs
  - crates/desktop/src/forge.rs
  - crates/desktop/src/root/forge.rs
  - crates/desktop/src/root/watch.rs
  - crates/desktop/src/root/render.rs
  - crates/desktop/src/root/projects.rs
  - crates/desktop/src/view/menu/mod.rs
  - crates/desktop/src/view/column/forge_tests.rs
  - crates/desktop/tests/all/menu.rs
  - crates/desktop/tests/all/git_worktrees.rs
  - crates/session/tests/all/merge_requests.rs
  - crates/session/tests/all/settings.rs
  - crates/desktop/src/view/forge.rs
  - crates/desktop/src/lib.rs
  - crates/desktop/src/column.rs
  - crates/desktop/src/root/column.rs
  - crates/desktop/src/view/column.rs
  - crates/desktop/src/view/menu/column.rs
  - crates/desktop/src/root/status.rs
  - crates/desktop/src/view/status.rs
  - crates/desktop/src/git/worktrees.rs
  - crates/desktop/src/root/git.rs
  - crates/session/src/settings.rs
  - crates/desktop/src/view/settings/catalog.rs
  - crates/desktop/src/view/settings/sections.rs
  - crates/desktop/src/root/settings.rs
  - crates/desktop/src/actions/**
  - crates/desktop/src/i18n/**
  - crates/desktop/src/icons/**
  - crates/desktop/assets/ui/**
  - crates/desktop/src/git.rs
  - crates/desktop/tests/all/main.rs
  - crates/desktop/tests/all/forge.rs
  - crates/desktop/tests/fixtures/forge/**
  - docs/runtime-ownership.md
  - docs/decisions.md
  - AGENTS.md
harness: any
reviews: {spec: 1, code: 2}
created: "2026-11-25"
needs_person: true
---

## Goal

The person works in one git worktree per GitLab merge request. Today the
column shows a worktree's branch, its age and its `+898 −67`, and nothing
about the MR: whether there is one, where it stands, and why it cannot be
merged all need the browser.

When this task is finished, a worktree's row in the column says whether its
branch has an MR, what state it is in, and the most important thing still in
its way. A hover over the MR icon shows the whole "path to merge", and a click
opens the MR in the browser. The row also shows commits not pushed yet, read
from git, with or without an MR.

The look is Guild's (decision 46's rule). This version **only shows**:
Guild writes nothing to GitLab.

**Decided by the person** (2026-11-25, recorded here as decided):

- The scope is to see only. Actions are non-goals (*Limits*).
- GitLab only. The state model is provider-neutral, so a later task can add
  GitHub.
- The source is `glab api`, run as a subprocess. Guild stores no token, and
  uses glab's own login, hosts and keyring. If glab is missing, or not logged
  in for the remote's host, the feature stays inactive: rows keep the plain
  branch glyph, and the hover says why only where it helps (narrowed below).
  One logical refresh per repository covers the branches of all its
  worktrees (*Refresh*; corrected after spec review r2, which showed one glab
  call cannot).
- (2026-11-25) **The adapter's base is the removed MR panel's glab reader**,
  commit `69c459cf` ("Read a project's merge requests through glab, whole or
  not at all"), cut down to the reads this task needs. Nothing else of that
  panel comes back: no actions, no GitLab writes, no permissions, no
  title-bar panel. See *Source: `glab api`*.
- Unpushed commits come from git, not from GitLab.
- The row's look, the badge set, the hover card, the context menu and the
  status bar are as drawn under *What the person sees*.
- A failed pipeline is red.
- (2026-11-25, answering this file's open questions) **The reason it is
  inactive is said only where it helps:**
  - glab knows the remote's host but is not logged in for it;
  - or glab is missing and the remote looks like GitLab.

  Everywhere else the row is a plain branch, with no explanation.
- (2026-11-25) **A setting, on by default,** turns the feature on and off.
  Off means no new glab request, and rows and the status bar as today; a
  request already running finishes and its answer is dropped (code review
  r1, arch+quality-1; worded so by code review r2, docs+tests-9).

**Proposals of the parent session the person did not object to:**

- `↑N` goes on the row's second line, not into the badge.
- The refresh triggers under *Refresh*.

Everything else marked **(proposal)** below was decided by the author of this
file or is left to the builder. It is open to the person's veto at the Penpot
step or before.

## What the person sees

### The row

A worktree's row in `crates/desktop/src/view/column.rs`. The leading glyph is
drawn at `:2207-2224` in a 14×18 box, `ui/workspace-git-branch.svg` in
`text.faint`. The second line is `activity-meta-line` at `:2283-2331`: the age,
then the secondary activity counts.

```
 ⑂  atl-740-backup-graph-st…     +898 −67      ← no MR: the branch glyph, as today
 ⇄✓ atl-740-backup-graph-st…     +898 −67      ← an MR: MR icon, coloured, with a badge
    !294 · 1m ago · ↑2                        ← MR number, age, unpushed commits
```

**The icon's colour is the MR's overall verdict.** It uses only palette
accents that exist (`theme.accent.*`):

| Colour | Meaning |
| --- | --- |
| green (`accent.green`) | GitLab says it can be merged: approved, threads resolved, pipeline fine, no conflict. |
| brass (`accent.brass`) | Open, but waiting on people: not approved, unresolved threads, pipeline running. |
| rust (`accent.rust`) | Needs a code change: a conflict with the target branch, **or a failed pipeline**. |
| grey (`text.secondary`, proposal) | Draft. |
| violet (`accent.violet`) | Merged. A hint that the worktree can go. |
| faint (`text.faint`, proposal) | Closed. The same hint. |

**(proposal)** Draft and Closed need two greys: `text.faint` is already the
plain branch glyph's colour, so Closed keeps it (with a different shape it
reads as "gone"), and Draft takes `text.secondary`.

**The badge in the icon's corner is the most important remaining fact:**

- 💬 unresolved threads
- 👁 awaiting approval
- ⏱ pipeline running
- ↓ behind the target branch, so a rebase is needed
- ✗ failed pipeline
- ✓ everything else is fine

So a red icon with ✓ means "it would be mergeable but for the conflict".

- **(proposal) Badge priority**, first match wins: ✗ failed pipeline → 💬
  unresolved threads → ↓ behind target → 👁 awaiting approval → ⏱ pipeline
  running → ✓.
  - A conflict is never a badge: the red colour says it.
  - So red with ✗ is a failed pipeline, and red with anything else is a
    conflict plus that fact.
- **✓ only when the answer establishes that every other check passed**
  (spec review r2, finding 8): approvals, threads, pipeline, rebase and
  GitLab's own detailed status, each read and passing. A check the answer
  does not establish (a field missing, GitLab still checking) gives no ✓.
- **GitLab's own blocker.** An open MR whose `detailed_merge_status` is
  `merge_request_blocked`, or any other value the checklist's lines do not
  explain, is blocked for a reason GitLab names and the checklist does not.
  - The hover card carries a line *Blocked: <GitLab's reason>*, with the
    known reasons translated in `i18n`; a status Guild does not know gives a
    generic line with GitLab's raw value (*GitLab says it cannot be merged
    yet: `<status>`*).
  - **(proposal)** Such a block is **brass** (it waits on something other
    than this branch's code, such as another MR) unless a red condition holds,
    and it has **no badge**: no ✓, because a check did not pass, and no new
    glyph beyond the person's six. The hover line and the status bar's text
    say what it is.
  - Which statuses the checklist explains is listed in the model and pinned
    by a test; the list is read from GitLab's documentation for the person's
    version, not assumed (not verified by this task's author).
- **(proposal)** A merged or closed MR has no badge. A draft is grey, whatever
  else is true, and gets the same badge as an open MR.

**The second line** becomes `!294 · 1m ago · ↑2`: the MR's number, the age as
today, and the unpushed commits.

- `↑N` shows whether or not there is an MR, and whether or not glab is
  present.
- With no MR and nothing unpushed, the line is exactly what it is today.
- **(proposal)** With no MR, only the age and `↑N` are shown.
- The column's group-folder rows (`folder_look`, GLD-055) are not worktrees
  and get none of this.

**Click on the MR icon** opens the MR's `web_url` in the default browser.

- **Code fact for the builder.** The assignment named "the `desktop::system`
  browser door". `system::browser::open_local_page`
  (`crates/desktop/src/system/browser.rs:6`) refuses anything that is not a
  `file:` URL (`:41-44`).
- An `https` link a terminal shows is opened with GPUI's `cx.open_url`
  (`crates/desktop/src/terminal/view/hover.rs:191`).
- **(proposal)** The MR link goes through `cx.open_url` like that one, and a
  test captures the URL instead of opening a browser.

**Hover on the icon** shows the "path to merge" checklist card:

```
 !299  [ATL-760] fix(chat): replies…
 ✓ Approved 1 of 1
 ✗ 6 unresolved threads
 ✓ Pipeline passed
 ✗ Conflict with master
 ↑ 2 commits not pushed
```

This is what answers "no unresolved comments, but it cannot be merged — why?".

- An MR GitLab blocks for its own reason adds *Blocked: <reason>* (see the
  badge rules above).
- **(proposal)** A draft adds a `Draft` line.
- **(proposal)** A merged or closed MR says `Merged <age>` or `Closed <age>`
  and lists nothing else.
- **(proposal)** A refresh that failed, or came back incomplete, keeps the
  last state in the card and adds one line, last updated `<age>` ago, with the
  short reason. An incomplete answer is never shown as "no MR" (*Refresh*).

**Hover on the plain branch glyph, while the feature is inactive,** says
why, in two cases only (decided):

- glab is configured for the remote's host but not logged in to it: *glab is
  not logged in to `<host>`*;
- glab is not on the login PATH and the remote looks like GitLab: *glab not
  found*.
  - **(proposal)** "Looks like GitLab" means the remote's host contains
    `gitlab` as a dot-separated label or part of one, such as
    `gitlab.example.com` or `my-gitlab.corp`.

In every other case the glyph has no hover, as today. That covers:

- a github.com remote;
- a host glab does not know;
- no remote;
- the setting turned off.

The person never sees "not a GitLab project".

### The setting

A toggle in Settings turns GitLab merge request status on and off. It is **on
by default**.

- Its label and hint go through `i18n` in English and Russian.
- **(proposal)** The wording:
  - title *GitLab merge requests in the column*;
  - hint *Show each worktree's merge request and what blocks it, read with
    your glab login. Off, Guild starts no new request; one already running
    finishes and its answer is dropped.* (code review r2, docs+tests-9)
- **(proposal)** Its place is **Projects → Git**, beside *auto fast-forward*
  (`Setting::AutoFastForward`, `crates/desktop/src/view/settings/catalog.rs:337-341`).
  - That toggle is the other per-repository background job that is on by
    default and runs nothing when off.
  - Network holds the proxy and its scopes, not features. Experimental is for
    unfinished work.
- **(proposal)** It is stored as a `guild-session` settings field, with
  `#[serde(default = "yes")]` like `auto_fast_forward_default_branch`
  (`crates/session/src/settings.rs:433-439`). A settings file without the key
  reads as on.
- **Off:**
  - no glab child is started;
  - a refresh in flight starts no further child; a glab already running
    finishes — it may be saving a refreshed login — and its answer is
    dropped (code review r1, arch+quality-1);
  - the remembered MR state is cleared;
  - rows draw the plain glyph with no "why" hover;
  - the status bar shows nothing of it;
  - the two menu items are absent;
  - ⌥⌘G does nothing.
- **`↑N` stays when off.** It is read from git, not GitLab, and the setting
  names GitLab.
  - **(proposal)** With no MR, the second line shows `↑N` as it does when
    glab is absent.
- **On again:** each shown repository is refreshed at once.

**The row's context menu** (`crates/desktop/src/view/menu/column.rs`,
decision 20) gains:

- **View MR** (⌥⌘G);
- **Copy MR URL**.

Both are shown only for a row whose branch has an MR. ⌥⌘G (`cmd-alt-g`) is
bound to nothing in `crates/desktop/src/actions/` today, checked by grep on
2026-11-25. The stroke acts on the active worktree; the menu item acts on the
row under the pointer (decision 20).

**The status bar**, for the active worktree, on the right
(`crates/desktop/src/view/status.rs`):

- `6 unresolved comments   !299 ↗`, in the icon's colour;
- `!299 ↗` opens the MR;
- no action button.
- **(proposal)** The text is the hover card's first failing line, and with
  nothing failing it is only `!299 ↗`.

**Every label goes through `i18n`**, in English and Russian. That includes
the hover card's lines, the reasons, the menu items and the status-bar text.

## How it works

### Source: `glab api`

**The base is commit `69c459cf`** (decided 2026-11-25): the removed MR
panel's `guild-session/src/merge_requests/glab.rs`, `model.rs` and their test
`crates/session/tests/all/merge_requests_glab.rs`. It is cut down to the reads
this task needs. What carries forward:

- `glab api` with `GLAB_NO_PROMPT=true`, `NO_COLOR=1` and
  `GLAB_NO_UPDATE_NOTIFIER=true`, no shell, `stdin` null for a read;
- a `ProcessGroup` per call, with a per-call deadline (10 s there) and a
  whole-refresh deadline (30 s there), and a stop flag checked while it waits;
- an answer cap (8 MB there) and a diagnostics cap (64 KB there);
- **whole or nothing**: a failed page, a pagination that changed under the
  read or a head that moved between list and detail is an error, and the
  caller keeps what it had;
- error classification by fixed facts only: not installed, signed out naming
  the host, no GitLab remote, otherwise one bounded line of stderr with URL
  userinfo and token-like words removed (`bounded_line`).

What does **not** come back:

- the five writes (thread, reply, resolve, approve, merge) and anything that
  sends a body;
- the `user` (account) read, which this task has no use for;
- project discovery through the `projects/:id` placeholder: glab resolves it
  with a separate lookup against its base remote before the main request
  (spec review r2, finding 12). The host and project path come from the
  worktree's upstream remote (*Which MR*, below) and are passed explicitly;
- the target-project listing `projects/<id>/merge_requests?state=opened`,
  replaced by a lookup in the source project (*Which MR*);
- spawning `glab` by name through an inherited `PATH`.

Changed while carrying forward:

- **The executable** is found as `update/remote/gh.rs:14-29` finds `gh`: on
  the login PATH (`guild_session::login_path::path_or_inherited`), an
  absolute, executable file, canonicalized. The GitHub updater is a
  reference **only** for that and for proxy integration (`gh.rs:76-80`),
  not the adapter's base.
- **The output cap is enforced while the child runs**, on stdout and stderr
  both. The base checked stdout's length in its wait loop and stderr only
  after a failure.
- **Admission comes before resources**: a child's temporary files or pipes
  are allocated only after it holds a global admission permit (*Bounds*).
- **(proposal) Where it lives**: in `guild-session::merge_requests`, as the
  base had it, because it is a process exchange that needs no window and is
  tested without one. The desktop's refresh owner (`root/forge.rs`) calls it
  off the window's thread and passes the proxy environment from
  `crate::network`.

**The base is reachable only through the reflog** (`git show 69c459cf`; the
panel is the commits `051087ff^..4db76eac`, on no branch and no tag). A reflog
entry expires, so the builder copies what it needs out of it at the start of
the work.

The provider-specific part is the argv and the parsing. The verdict, the
badge and the hover lines are computed by a pure provider-neutral model from
a provider-neutral state.

- **The answer's shape** is the builder's call, recorded in the decision named
  under *Fit*: GraphQL through `glab api graphql` (MRs of several source
  branches with approvals, head pipeline and discussions in one query, then
  further pages), or REST (a list, then each MR's detail, approvals and
  discussions). Either way it is **one logical refresh** (*Refresh*), not one
  call.
- **Observed on the person's machine, not verified by this task's author:**
  - `glab` 1.118 (present at `/opt/homebrew/bin/glab`, `1.118.0`);
  - logged in to a self-hosted GitLab;
  - REST `projects/<path>/merge_requests?source_branch=<b>&state=opened`
    returns `detailed_merge_status`, `has_conflicts`,
    `blocking_discussions_resolved` and `draft`.

  Approvals, the head pipeline, the unresolved-thread count and "behind the
  target" need GraphQL fields or extra REST calls. Their names are **not
  verified**: the builder checks them against the person's GitLab version and
  records which fields are used.
- **Which MR a branch maps to.** The association is keyed by **GitLab host,
  source project and the remote branch name** (spec review r2, finding 7):
  - the remote and the remote branch come from `Ahead::destination`'s rule
    (`crates/desktop/src/git.rs:1226-1247`): the upstream's remote, and on it
    the upstream's own branch name, so a local `bar` tracking `origin/foo`
    looks up `foo`. With no upstream, the first remote `git remote` lists and
    the local branch name, as the same rule falls back;
  - the remote's URL, read through `git::run` (`remote get-url`), gives the
    host and the project path, for https and scp/ssh forms;
  - the MR is looked up **in that source project only**, by source branch;
    an MR from a fork into its parent is a non-goal (*Limits*, finding 6);
  - **(proposal)** an open MR wins over a merged or closed one, and among MRs
    in the same state the most recently updated wins. So merged and closed
    MRs are found too, not only `state=opened`; the query asks for every
    state, or for open first and the rest when a branch has no open MR. The
    builder chooses and records it.
- **Verdict from GitLab's fields (proposal), first match wins:**
  1. merged → violet;
  2. closed → faint;
  3. draft → grey;
  4. a conflict or a failed head pipeline → red;
  5. GitLab's detailed status says mergeable → green;
  6. GitLab's own blocker (`merge_request_blocked` or a status the checklist
     does not explain) → brass, with the *Blocked* line (finding 8);
  7. anything else open → brass, including GitLab still checking.
- **Which host glab is asked, and whether it is logged in:**
  - the host is matched against the hosts glab is configured for. Not
    verified: how glab exposes its hosts, and how an ssh host alias maps. The
    builder checks.
  - **`glab auth status` contacts the network** (spec review r2, finding 2):
    for a configured host with no credential glab 1.118 builds an
    unauthenticated client and asks GitLab. **Implementation step:**
    establish whether glab offers a local, credential-free check that a
    configured host has a login, and record the answer. Until one is found,
    a signed-out host costs **at most one unauthenticated probe per refresh
    window**, not zero, and its answer is remembered until the next window.
- **The child's environment** (spec review r2, finding 1). `--hostname`
  selects glab's login host, but glab's API client resolves its endpoint
  separately, so an inherited override can send the selected host's
  authorization header elsewhere. The child is started with the host given
  explicitly and these inherited variables **removed**:
  - endpoint overrides: `GITLAB_HOST`, `GITLAB_API_HOST`, `GITLAB_URI`,
    `GITLAB_API_PROTOCOL`, `GL_HOST` and any other host or endpoint override
    glab 1.118 reads;
  - environment credentials: `GITLAB_TOKEN`, `GITLAB_ACCESS_TOKEN`,
    `OAUTH_TOKEN`. **Why:** the person decided the source is glab's own
    login, hosts and keyring. A token in the environment is bound to no host,
    exists only when Guild was started from a shell, and would make the
    answer depend on how Guild was launched;
  - debug switches (`DEBUG`, `GLAB_DEBUG` and glab's HTTP debug variable),
    which can print request headers into the diagnostics Guild reads.

  Kept: `GLAB_CONFIG_DIR`, because it says where glab's own login lives, and
  glab's per-host configuration (including a host's `api_host`), which is the
  person's own configuration of glab. Set: `GLAB_NO_PROMPT`, `NO_COLOR`,
  `GLAB_NO_UPDATE_NOTIFIER`. The builder checks the variable names against
  glab 1.118's source, and **the adapter boundary test pins the whole list**:
  a fake `glab` records the environment it was given.
- **Bounds (proposal):**
  - the per-call and whole-refresh deadlines of the base;
  - the output caps, enforced while the child runs;
  - **a global admission limit of 2 glab children across all repositories**
    (spec review r2, finding 4), taken before any temporary file or pipe is
    allocated. glab children are outside the four-permit git read budget
    (`crates/desktop/src/git/read_budget.rs:8`), and startup or turning the
    setting on asks every shown repository at once (traps: *Per-project Git
    workers still share one process descriptor limit*);
  - per-repository coalescing within that limit (*Refresh*).
- **Nothing secret is logged.** stdout is parsed and dropped. A failure is
  reduced to a short reason, and glab's stderr is shown only as that reason.
  Fixtures are synthetic: an answer from the person's GitLab contains their
  company's data and is never committed.

### Unpushed commits: from git

`↑N` is the number of commits on the worktree's branch that a push would
send — the same question Push Commits answers.

- It is read through `git::run`, beside the times and the lines that
  `counted` already reads for every shown worktree
  (`crates/desktop/src/root/git.rs:1061-1095`,
  `crates/desktop/src/git/worktrees.rs:163` and `:194`). Reads through
  `git::run` are bounded by the shared read permits (traps: *Per-project Git
  workers still share one process descriptor limit*).
- **From refs only** (spec review r2, finding 10): `rev-list --count`, never a
  `git status` walk. `status` can refresh and write the index in the
  background (Guild's status wrapper passes no `--no-optional-locks`,
  `crates/desktop/src/git.rs:155-159`), and this read must add no index
  writer and no worktree scan.
- **With an upstream**: `rev-list --count <upstream>..HEAD`.
- **With no upstream**, Push Commits' existing reachability rule
  (`git::ahead_args`, `crates/desktop/src/git.rs:1320-1343`; spec review r2,
  finding 9): the commits on `HEAD` that no ref of the chosen remote holds,
  `rev-list --count HEAD --not --remotes=<remote>`, with the remote chosen by
  `Ahead::destination`'s rule. So a branch pushed under another name without
  `-u` counts nothing that remote already has, and the two surfaces agree on
  one repository state.
- **No meaningful remote comparison, no `↑N`**: no remote at all, a detached
  head, or a refusal. The merge-base-with-main fallback of the first draft is
  dropped: it called pushed commits unpushed.
- **(proposal)** Commits the upstream has and the branch does not (behind
  upstream) are not shown: the person asked for unpushed commits only.

### Refresh

- **When:**
  - on start;
  - when the window regains focus;
  - after a push or a branch switch made from Guild;
  - every 2 minutes while the window is active.
  - Each is one logical refresh per repository.
- **A logical refresh is not a call count** (spec review r2, finding 3). It is
  everything needed to know the shown branches' MRs of one repository, and it
  may take several glab calls:
  - its work is batched **by GitLab host and source project**: two worktrees
    of one repository can track projects on different hosts, and each batch
    is its own calls;
  - **every page is fetched**, of the MR lists and of the discussions, so a
    second page cannot hide an MR or an unresolved thread;
  - its total work is bounded: the per-call and whole-refresh deadlines, a
    most-pages limit as the base's `PAGES`, the output caps and the global
    admission limit;
  - **an incomplete answer is unknown, not empty**: a batch that failed, timed
    out or hit a bound keeps its branches' last known state (with the failure
    in the hover card), and is never shown as "no MR".
- **One pending refresh per repository** (spec review r2, finding 5). A
  trigger while a refresh is out does not fold into it, because the request
  already in flight captured its branches and remotes; it marks one pending
  refresh, which runs when the current one ends. Several triggers in that time
  are one pending refresh.
- **A reply is checked before it publishes**: against the worktree's current
  branch, upstream, remote host and project, and against the setting's
  generation. A reply for a branch that has since been switched, or from
  before the setting was turned off and on, is dropped; the pending refresh
  brings the current state. This is how the git coordinator already keeps an
  old reply from overwriting a newer epoch (`crates/desktop/src/root/git.rs:1385`,
  `crates/desktop/src/root/git/shared.rs:121-164`).
- While a refresh is out, the last known state stays drawn.
- **(proposal)** Focus refreshes are debounced: a focus within 30 seconds of
  the last refresh asks nothing. This keeps cmd-tab from becoming a request
  each time.
- **(proposal)** The 2-minute clock runs only while Guild is the active
  application, and stops otherwise.
  - Traps: *Application activation is not activity visibility* is about
    animation and does not apply here. For a network poll, activation is the
    intended signal.
- **(proposal)** Only repositories with a worktree shown in the column are
  asked.
- `↑N` follows git's existing column refresh, not the 2-minute clock.
- An answer only notifies the column when the state it carries changed. Traps:
  *A small activity ring can rebuild a whole cached column at display rate*
  is the cost of notifying more often.

## Fit with the rules in force

- **Nothing on the window's thread.** The glab child and the parsing run on a
  background thread. The answer returns to the window and is applied there, as
  `root/usage.rs` does with Codex's limits (decision 47).
- **Process groups with deadlines.** Every glab child is a
  `guild_session::process::ProcessGroup`, killed as a group at its deadline,
  with its leader reaped only after the group is stopped
  (`docs/runtime-ownership.md`, *Process lifetime*). The ownership table there
  gains a row for the new owner.
- **The proxy only through `network.rs`.** The policy is applied with
  `crate::network::apply` or `apply_policy` (`crates/desktop/src/network.rs:26-41`).
  - **(proposal)** The scope is `Scope::Guild`, as for `gh` updates and
    Codex's usage (`usage/codex.rs:132`).
  - The Network settings' hint for that scope then names it:
    `Text::NetworkGuildHint` (`crates/desktop/src/i18n/english.rs:322`) gains
    "merge request status", in both languages.
- **Git reads through `git::run`; no new write.** The remote URL, the
  upstream and the ahead count are reads. `git::Write` is not touched, so
  decision 21's list and its test do not change.
- **No token held.** Like decisions 46 and 47, Guild never reads or stores
  glab's credentials. glab is the only thing that talks to GitLab, and Guild
  reads only what `glab api` prints.
- **The model stays out of GPUI.** The state, the verdict, the badge choice
  and the hover lines form a pure module, tested without a window.
  - **(proposal)** The adapter and the model live in
    `guild-session::merge_requests`, where the base had them; `guild-session`
    has no GPUI, which `just boundaries` holds. The desktop keeps only the
    refresh owner (`root/forge.rs`) and the drawing (`view/forge.rs`).
  - The module names are the author's proposal.
- **A decision is recorded.** Reading an external service through its own CLI
  is a rule at the level of 46 and 47, so `docs/decisions.md` gets a new
  numbered decision. It says:
  - glab is the source, its environment scrubbed of endpoint and credential
    overrides;
  - a setting, on by default, starts no new request when off;
  - no token is held;
  - one logical refresh per repository, batched by host and project, whole
    or unknown, inside a global admission limit;
  - the MR is the source project's, by the upstream's remote branch name;
  - nothing is written to GitLab.
- **`AGENTS.md`'s map** names the new modules.

## Acceptance

- [x] **Penpot first.** WHEN code is written THEN the person has approved a
      proposal on the Screens file's Proposals page (`docs/penpot.md`, steps
      1–3), drawn from the Library's components and tokens. It shows:
      - a row in each verdict colour, with representative badges, and an MR
        GitLab blocks for its own reason;
      - the second line with `!N` and `↑N`;
      - the hover card;
      - the two menu items;
      - the status-bar text;
      - the plain glyph's "why inactive" hover;
      - the Settings toggle in its section.
- [x] WHEN a worktree's branch has no MR, or the feature is inactive, THEN its
      row draws the plain branch glyph exactly as today.
- [x] WHEN a branch has an MR THEN the row draws the MR icon in its verdict's
      colour, per the table, with the badge chosen by the recorded priority.
      A pure test covers every verdict and every badge, including:
      - red with ✓ for a conflict with nothing else remaining;
      - red with ✗ for a failed pipeline;
      - grey for a draft with a conflict;
      - no ✓ when a check is not established by the answer.
- [x] WHEN an open MR's detailed status is `merge_request_blocked`, with
      approvals, threads, pipeline and rebase all passing THEN it is brass
      with no badge, the hover card says *Blocked:* with GitLab's reason, and
      the status bar says the same. WHEN the status is one Guild does not
      know THEN the line is the generic one with GitLab's raw value.
- [x] WHEN a branch has an MR THEN the row's second line reads
      `!<iid> · <age>`, followed by ` · ↑N` when N > 0.
- [x] WHEN a branch has commits its upstream lacks THEN `↑N` shows, with or
      without an MR, and with glab absent. WHEN it has no upstream THEN `↑N`
      counts what no ref of the chosen remote holds, as Push Commits does,
      and a branch pushed under another name without `-u` shows nothing
      unpushed. WHEN there is no remote or `HEAD` is detached THEN no `↑N`.
      Each case has a git fixture test, and the count runs no `git status`.
- [x] WHEN a local branch `bar` tracks `origin/foo` THEN the MR looked up is
      the one whose source branch is `foo` in `origin`'s project, not one
      sourced from `bar`. A selection test covers it.
- [x] WHEN the MR icon is clicked THEN the MR's web URL is opened in the
      default browser. A test captures the URL.
- [x] WHEN the pointer rests on the MR icon THEN the hover card lists:
      - the MR's number and title;
      - approvals given of required;
      - unresolved threads;
      - the pipeline's state;
      - a conflict with the target, by the target's name;
      - behind the target, when it is;
      - unpushed commits.

      Each line is marked ✓ or ✗, in the person's language.
- [x] WHEN a row with an MR is right-clicked THEN the menu has View MR, with
      ⌥⌘G shown, and Copy MR URL. View MR opens the URL; Copy MR URL puts it
      on the clipboard. WHEN ⌥⌘G is pressed THEN the active worktree's MR
      opens, and nothing happens without one.
- [x] WHEN the active worktree has an MR THEN the status bar's right side
      shows the recorded text and `!<iid> ↗` in the verdict's colour, and
      `!<iid> ↗` opens the MR.
- [x] WHEN glab is not on the login PATH THEN no request is made, and the
      rows keep the plain glyph.
- [x] WHEN glab is configured for the remote's host but not logged in THEN
      no request goes to that host, and the rows keep the plain glyph. (The
      implementation step found a local, credential-free check: glab's own
      `config.yml`, see *Implementation notes*; a stale login that answers
      401 is remembered until the next refresh window.)
- [x] WHEN glab is configured for the remote's host but not logged in THEN
      the glyph's hover says glab is not logged in to that host.
- [x] WHEN glab is missing and the remote's host looks like GitLab THEN the
      hover says glab was not found.
- [x] WHEN the remote is github.com, a host glab does not know, or absent
      THEN the glyph has no hover. A pure test covers each case.
- [x] WHEN Settings is opened THEN the GitLab merge request toggle is in the
      recorded section, on for a settings file without its key, with its
      title and hint in English and Russian.
- [x] WHEN the toggle is off THEN:
      - no glab child starts, on any trigger (a fake `glab` counts zero
        calls);
      - a request in flight starts nothing more, finishes, and its answer
        is dropped (code review r1, arch+quality-1);
      - rows and the status bar draw as today, with no MR icon, no "why"
        hover, no menu items, and ⌥⌘G inert;
      - `↑N` still shows.
- [x] WHEN the toggle is turned on again THEN each shown repository is asked
      at once.
- [x] WHEN glab does not answer before the deadline THEN its process group is
      killed and the last known state stays drawn, with the failure in the
      hover card. A test uses a fake `glab` that never answers.
- [x] WHEN a repository has several worktrees THEN one logical refresh
      covers their branches: one batch per GitLab host and source project,
      each fetching every page of MRs and discussions. A fake `glab` that
      answers two pages proves the second is read, and its call log shows no
      call outside the batches.
- [x] WHEN a batch fails, times out or hits a bound THEN its branches keep
      their last known state with the failure in the hover card, and none is
      shown as having no MR.
- [x] WHEN every shown repository refreshes at once THEN no more than 2 glab
      children run at a time across all of them, no child's output files or
      pipes exist before it holds a permit, and a child whose stdout or
      stderr passes its cap is stopped while it runs.
- [x] WHEN a worktree's branch is switched while a fake `glab` read for it is
      held open THEN, once the read is released, its reply is dropped, a
      pending refresh runs, and the row shows the new branch's MR. A reply
      from before the setting was turned off and on is dropped the same way.
- [x] WHEN a branch has several MRs THEN the recorded choice picks one. WHEN
      its MR was merged or closed THEN the row is violet or faint.
- [x] WHEN the window starts, regains focus (outside the debounce), finishes a
      push or a branch switch from Guild, or has been active for 2 minutes
      since the last refresh THEN each repository gets one logical refresh,
      or one pending refresh if one is out. Otherwise nothing is asked.
      Tested with virtual time.
- [x] WHEN a glab child is spawned THEN:
      - it is a `ProcessGroup`;
      - its proxy comes from `crate::network` under the recorded scope;
      - it runs off the window's thread;
      - its argv and environment are what the adapter's test pins: the host
        explicit, and every variable named under *The child's environment*
        removed even when the test's own environment sets it.
- [x] WHEN the task's diff is read THEN:
      - nothing is written to GitLab;
      - no token is read or stored;
      - `git::Write` is unchanged;
      - `guild-session` and `guild-cli` gain no GPUI (`just boundaries`).
- [x] Every new label is in `i18n` in English and Russian.
- [x] `docs/decisions.md` has the new decision, and `AGENTS.md`'s map and
      `docs/runtime-ownership.md`'s table name the new owner.
- [x] `CHANGELOG.md` has one line under `## [Unreleased]` → `### Added`,
      written for the person. Draft wording: *A worktree whose branch has a
      GitLab merge request shows it in the column: the icon's colour says
      whether it can be merged, its badge says what is left, a hover lists
      what stands in the way, and a click opens it. Commits not pushed yet
      show as ↑N. It uses your glab login, and Settings → Projects can turn
      it off.*
- [x] **Penpot brought to what was merged** (`docs/penpot.md`, step 6), in
      the same branch's work:
      - the proposal promoted into the Library and Screens;
      - stale copies relinked;
      - the token script's output checked against both files.
      Done 2026-11-26, against the code and the dev window's captures:
      - **Library**, a new *Merge requests* section on Components:
        `forge/mr glyph` (`State`: Mergeable, Mergeable with threads,
        Threads, Approval, Running, Behind, Conflict, Failed, Draft,
        Waiting, Merged, Closed — the icons from `assets/ui`),
        `forge/card line` (`Mark`: Pass, Fail, Unpushed, None),
        `forge/path card` (`Failure`: No, Yes — 420 wide, its line
        wrapping inside), `status/merge request` (`Verdict`: Waiting,
        Needs change, Draft, Mergeable, Merged, Closed) and
        `settings/page projects`, with the new toggle's hint.
      - **Screens**: *Main window — merge requests*, *— merge request
        refresh failed*, *— merge request unknown*, *— merge request not
        logged in*, *Menu — worktree row with a merge request* and
        *Settings — Projects*; the seven proposal boards removed.
      - **Relinking**: no existing Library component's structure changed,
        so no screen copy went stale; the six new screens' copies all
        point at the Library, none broken.
      - **Tokens**: nine `view.forge.*` measures from `view/forge.rs`
        added to the script and both files; common 543, palettes 74 / 74,
        the same hashes in the script, the Library and Screens.

## Checks

- **Test first:** each acceptance item gets a failing test before its code,
  failing for the right reason.
- Pure tests cover:
  - the verdict and badge table;
  - the hover lines;
  - the MR choice among several;
  - the parsing of synthetic GraphQL or REST fixtures under
    `tests/fixtures/forge/`.
- Adapter tests start from the base's `merge_requests_glab.rs` and use a fake
  `glab` on an override path (the base's `on_path`). They cover:
  - argv and the scrubbed environment;
  - glab missing or logged out, and the signed-out probe bound;
  - deadline kill, and the output cap enforced while the child runs;
  - every page read, and an incomplete answer kept as unknown;
  - the global admission limit across repositories;
  - a reply dropped after a branch switch, with the pending refresh.
- Git fixture tests cover the ahead count: with an upstream, without one
  (including a branch pushed under another name), with no remote, and
  detached; and the MR key for a local branch tracking another name.
- GPUI tests cover the row's icon and badge selectors, the hover card, the
  click with the browser captured, the menu items, ⌥⌘G and the status bar.
- The Settings toggle's tests cover:
  - its row in the recorded section;
  - the default for a file without the key;
  - off and on again, with the fake `glab`'s call count and an in-flight
    request let finish and its answer dropped.
- Isolated `just check`, with:
  - a scratch `GUILD_STATE_DIR`;
  - `GUILD_NO_GLOBAL_HOOKS=1`;
  - `GUILD_SOCKET`, `GUILD_SESSION`, `GUILD_HOOK_SOCKET` and
    `GUILD_USER_STATUSLINE` unset.
- **Real-window check at the end**, in an isolated dev window
  (`just desktop-dev`), of the hover, the click, the menu and the status bar.
  - It runs against the person's real glab login and their GitLab host. It is
    a read of their account, so it runs only with their permission for that
    run (`needs_person`).
  - Clicks only while the person is at the machine.
- Reviews: revust with the `codex-only` profile only, at most one spec round
  and two code rounds (`reviews`). No other provider or profile without the
  person's explicit exception.
- **Not run:**
  - any write to GitLab;
  - GitHub;
  - a GitLab version other than the person's.

## Limits / non-goals

- **A host with an explicit port is inactive** (decided 2026-11-26). glab
  1.118 refuses `--hostname host:port` ("Error parsing --hostname: invalid
  hostname"), so a remote whose host carries a port starts no glab: the
  row stays plain and its hover says *glab cannot address a host with a
  port: host:port* (Russian through i18n). The port stays in the host's
  identity (code review r1, arch+quality-3). glab reads such a host from
  `GITLAB_HOST`, with the login kept under that exact `host:port` key —
  measured against a local server; supporting it is a later task.
- **No action on GitLab.** A status-bar Fix, Squash and merge, and
  Commit and push, Create MR, Merge, and an agent launched to work through
  review threads are a possible later task. They are not drawn here as
  disabled controls either.
- **No GitHub** (`gh`). The model stays provider-neutral so that a later task
  only adds an adapter.
- No token storage, no Guild-owned HTTP client to GitLab, and no settings page
  for GitLab hosts: glab's configuration is the only one.
- **No merge requests from forks** (spec review r2, finding 6). The MR is
  looked up in the source project the upstream points at, so an MR from a
  fork into its parent project is not found and the row shows no MR.
- Nothing of the removed MR panel returns beyond the glab reads: no actions,
  no GitLab writes, no permissions, no title-bar panel.
- No `guild` CLI command for MR status.
- One setting only, the toggle. There are no per-repository or per-host
  switches and no refresh-interval setting.
- No notifications, sounds or Dock badge when an MR's state changes.
- `CHANGELOG.md` is not in `touches`, by `tasks/README.md`'s rule for
  append-only files.

## Implementation notes (2026-11-25)

What the builder found or chose while implementing, against the text above.
Each is either a finding the task asked to be established, or a call it
left to the builder.

- **Logged in or not is read locally (finding 2's implementation step).**
  glab 1.118 keeps its hosts in `config.yml`. *Corrected by code review r1
  (arch+quality-4):* the order first written here was a guess. glab's
  `SearchConfigFile` (`internal/config/config_file.go`, v1.118) takes
  `GLAB_CONFIG_DIR` alone when it is set, even with no file in it; else
  the legacy `~/.config/glab-cli/config.yml` when it exists; else XDG's
  config home (`$XDG_CONFIG_HOME/glab-cli`, on macOS by default
  `~/Library/Application Support/glab-cli` — the person's). A
  host with a non-empty `token`, or `use_keyring: true`, is signed in; a
  host listed with neither is signed out; an `ssh_host` names its glab
  host. So a signed-out host and a missing glab cost **no** request
  (`guild_session::merge_requests::hosts`). A token's value is read only as
  far as "not empty".
- **REST, per branch** (the builder's call between GraphQL and REST): every
  page of `merge_requests?source_branch=…&state=all&order_by=updated_at`,
  then for an open MR its detail, `approvals` and every page of
  `discussions` — the base's own calls, with field names that are
  documented and were already observed on the person's GitLab. GraphQL
  field names were not verified, and REST needs none.
- **"Behind the target" is GitLab's `need_rebase`**: the ↓ badge and the
  card's *Needs a rebase onto <target>* line follow GitLab's detailed
  status, not a diverged-commits count — a branch merely behind a target
  that GitLab would still merge is not in the way.
- **Approvals GitLab does not offer** (403/404 on `approvals`) are unknown,
  not a failure of the batch: no line and no ✓.
- **The card always says the conflict either way**: *Conflict with <target>*
  ✗ or *No conflict with <target>* ✓, as the approved *Hover card* board
  draws it.
- **The status bar says threads as comments** (*6 unresolved comments*), as
  the approved *Status bar* board has it; every other line is the card's.
- **The glab child runs in `/`**, so no repository's
  `.git/glab-cli/config.yml` is read and no remote is guessed; its update
  check is switched off (`GLAB_CHECK_UPDATE=false`) beside the base's
  `GLAB_NO_UPDATE_NOTIFIER`. glab's own telemetry setting is the person's
  and is left alone.
- **Where the code lives**: the provider-neutral model and the glab reader
  in `guild-session::merge_requests` (`status`, `glab`, `hosts`, `remote`);
  the refresh scheduler, pure, in `guild-desktop::forge`; the asking in
  `root/forge.rs`; the drawing in `view/forge.rs`.
- **A push or a switch from Guild** is recognised by the git verb of the
  Console's line for a write git carried out (`push`, `switch`).

## Code review r1 (2026-11-25)

Fixed, each with a test written to fail first (or, where the fix came
first, shown to fail by putting the old behaviour back):

- **Adapter** (`merge_requests_glab.rs`): a listed record without its
  project is an unknown answer (bugs+impl-9); an MR closed or `not_open`
  between the list and its detail is no answer (bugs+impl-10);
  `ci_must_pass` with no pipeline is *No pipeline has run* ✗
  (bugs+impl-11); an approvals answer stating nothing is unknown, while
  `4f339d4d`'s real answer stays *No approval required* ✓ (bugs+impl-12);
  a malformed answer is said by its kind, not GitLab's value
  (bugs+impl-4); the newest MR wins to the nanosecond (bugs+impl-15);
  config values read as YAML scalars — quotes, inline comments, nulls,
  flow values (bugs+impl-3); the https port kept, as glab keys hosts
  (arch+quality-3); glab's own config file order (arch+quality-4);
  `GLAB_PROXY`, `PROXY`, `GITLAB_SUBFOLDER` and the CI overrides scrubbed,
  the environment test naming every override itself (arch+quality-2).
- **Cancellation** (arch+quality-1, docs+tests-3): a stop starts nothing
  more, checked under the lock a child is spawned under, and lets a
  running glab finish, its answer dropped — glab may be saving a refreshed
  login; a deadline, a cap and an abort send `SIGTERM` and kill after a
  grace. Tested with a stand-in that traps `TERM` and logs it.
- **Ownership** (bugs+impl-1, bugs+impl-2): a refresh is owned until its
  thread ends; a repository leaving the column stops it and none starts
  beside it when the repository returns; the quit aborts every refresh and
  waits for its thread before it completes.
- **One deadline per refresh** (bugs+impl-7), through `read_until`.
- **Per repository and per candidate** (bugs+impl-13, bugs+impl-6): two
  clones of one project keep their own answers and failures; a worktree
  whose destination moved shows nothing of the old project; View MR and
  Copy MR URL take the URL the row shows when they run.
- **After a write** (bugs+impl-5): tested in the window's own order — the
  switch's line lands while A's read is out and before the list after the
  switch: A's reply is dropped, nothing is asked until the new list lands,
  then B is asked.
- **Unknown, not no MR** (adversarial-9): a branch asked and never answered
  draws the plain glyph, its hover *Merge request unknown: <why>*; the
  card's failure line dates the last read that landed, apart from the
  failure (bugs+impl-14).
- **Git** (docs+tests-5, bugs+impl-8): a remote changed in `.git/config` is
  news to the poll, by the file's modification time and length; the
  upstream is read by `%(refname:lstrip=2)`, so a tag named like the
  branch does not hide it.
- **Tests** (docs+tests-9): each click in the view test must replace a
  sentinel URL.

Re-checked in the isolated dev window with the stand-in `glab` only (no
GitLab read): a repository whose first refresh failed draws the plain
glyph, its hover *Merge request unknown: glab: 404 Not Found (HTTP 404)*;
`ci_must_pass` with no pipeline reads *No pipeline has run* ✗; a refresh
failing after a read says *Read 1m ago · the refresh after it failed: …*;
turning the setting off with two glab children running let both finish (no
`SIGTERM`), asked nothing more and turned every row plain, `↑1` kept; and
`⌘Q` with two running sent each `SIGTERM`, the window quit in under a
second and no glab or its child outlived it. The check found one defect,
fixed: the longer failure line wrapped past the card's bottom edge —
GPUI lays a tooltip out at its minimum size — and a check line naming a
long target branch ran past its right edge (`145278c5`).

Recorded, not changed:

- **adversarial-3**: a host whose `use_keyring: true` is stale — the
  keyring entry gone — reads as signed in, so one unauthenticated read is
  made per refresh window until glab's 401 turns the row to *Not logged
  in*. This is within the relaxed acceptance of spec review r2 (no request
  for a host glab lists as signed out; a stale login costs one).
- **adversarial-4**: the 8 MB limit is on the output Guild captures; what
  glab buffers inside itself before writing is outside Guild's control.
  *Since code review r2 (its adversarial-5)* the capture itself is bounded:
  glab writes into pipes Guild reads as they come, keeping no byte past the
  cap, and a stream past it ends the child (`SIGTERM`, grace, kill, reap).
  Before, glab wrote into temporary files the cap was only looked at, which
  could grow past it between looks and during the grace.

## Code review r2 (2026-11-25)

The last allowed round. Fixed, each with a test that failed first — or,
where the fix came first, was shown to fail with the old behaviour put
back:

- **Adapter**: a record without its source branch (bugs+impl-8) or with a
  link that is not an absolute http or https URL (adversarial-2) makes the
  batch unknown; the detail's `draft` wins (bugs+impl-13); `not_approved`
  with no failing approvals line is GitLab's block, *it needs approval*,
  with no count (bugs+impl-9); an expired deadline gets no permit and
  starts no child, looked at with the stop right before the spawn — the
  test sees the fork itself (bugs+impl-12); output is captured from pipes
  with no byte past the cap kept, the test counting the bytes
  (adversarial-5); a relative `GLAB_CONFIG_DIR` is made absolute once for
  the local read and every child (arch+quality-2); an empty YAML block
  scalar is empty, with no YAML dependency (bugs+impl-3); `ssh_host` names
  a glab host for ssh and scp-style remotes only (arch+quality-3); a
  host's `subfolder` is cut from the project path at a segment boundary
  (adversarial-4).
- **The quit** (bugs+impl-1): GPUI 0.3.8 gives a quit 200 ms
  (`SHUTDOWN_TIMEOUT`) and no way to ask for more, so the window cannot
  wait for an owner's grace. The abort's grace is 100 ms, and 150 ms into
  the quit every glab group still alive is `SIGKILL`ed through
  `ProcessGroup::killer`, which signals only while the group is unreaped.
  Recorded in decision 56. Tested with a stand-in whose group ignores
  `SIGTERM`, in a test process of its own.
- **The window**: a repository leaving the column loses its refresh's
  right to publish (bugs+impl-7); the column's candidates are taken in
  before replies land (bugs+impl-6), and only the active workspace's
  repositories are asked, tested with two workspaces and the stand-in's
  call log (docs+tests-4); a 401 signs its host out for the rest of the
  refresh (bugs+impl-2); an empty read followed by a failure is unknown
  (bugs+impl-11); the MR is looked up at the push URL, `↑N` on the
  tracking ref (bugs+impl-4); one budget is tested through the refresh
  loop with two batches (docs+tests-8); the toggle's hint and this task's
  acceptance say what off does (docs+tests-9).

Rejected after looking, no code changed:

- **bugs+impl-10** (a localized `ahead` hiding `↑N`). `for-each-ref`'s
  `%(upstream:track)` is not translated: git sets ref-filter's translated
  words only for the porcelain `git branch`. Checked with Homebrew git
  2.55.0, whose Russian catalogue has *впереди %d*: under
  `LANG=ru_RU.UTF-8`, `git branch -vv` printed `[origin/threads: впереди
  2]` while `git for-each-ref --format='%(upstream:track,nobracket)'`
  printed `ahead 2`, and `git status` in the same environment answered in
  Russian.

Recorded, not changed:

- **bugs+impl-5**: a remote URL changed only in a file included from
  `.git/config` is noticed at the next Git invalidation — a watcher batch,
  activation, a command — not by the cheap poll, which looks at
  `.git/config` itself. Includes for remote URLs are rare; the staleness
  is bounded by the next invalidation and accepted.
- **docs+tests-10**: rolling back to a build from before GLD-063 and saving
  settings there drops `gitlab_merge_requests`, so the newer build reads
  it as on again. A downgrade was rejected as a non-goal at spec review
  r2.

## Code review r3 (2026-11-26)

A narrow round over r2's fixes. Fixed, each test failing first:

- **lenses-1**: a glab child goes into the live list under the same lock
  an abort takes, so the quit's kill pass sees every child started before
  the abort returned; tested with a child held between its start and its
  listing.
- **lenses-2**: the kill pass passes over a group whose owner is reaping
  it and comes back, so one slow reap does not delay another group's
  `SIGKILL`; the reused-ID guard stays.
- **lenses-4**: glab runs with `--include`; signed out is GitLab's 401
  from the status line, never a message's words — a project's 403 saying
  "not authenticated" is that project's failure, and a 401 whose `errors`
  array glab prints with no status is signed out. Headers stay out of the
  diagnostics. Both shapes measured with glab 1.118 against a local
  server.
- **lenses-6**: every change of the desk — a workspace switched, a project
  opened or closed — takes the column in at once, revoking a hidden
  repository's refresh before the next poll.
- **lenses-7**: an MR's link is parsed: http or https, a non-empty host;
  `https://@/` is incomplete.
- **lenses-8**: a node's `!tag` or `&anchor` comes before its block
  header; `token: !!str |-` with nothing under it is empty.

Recorded, not changed:

- **lenses-3**: a glab replaced by a wrapper that leaves a helper in
  another process group holding stdout or stderr can strand a capture
  thread per refresh. Guild finds glab itself on the login PATH, and glab
  does not do this.
- **lenses-5** (from before this work): a legacy glab profile that puts
  the path prefix in `api_host` without `subfolder` is not stripped; only
  `subfolder` is honoured.

## Verification (2026-11-25)

**Tests** (each written before its code, except the two view tests, which
were written after the drawing and then shown to fail by breaking
`forge_shown`):

- `guild-session` `tests/all/merge_requests.rs` (17): verdict, badge order,
  ✓ only when established, GitLab's blocker, the checklist's lines and
  order, `detailed_merge_status` and pipeline words.
- `guild-session` `tests/all/merge_requests_glab.rs` (24), against a stand-in
  `glab`: a whole read with the host pinned, no MR, open over merged and no
  fork's, merged needing no detail, a branch name as one query value,
  every page of MRs and discussions, a failed page failing the batch,
  approvals not offered, no approval required, a moved head, the deadline
  kill, output capped while running on stdout and on stderr, at most two
  children across reads, a deadline while waiting for a permit with no
  child started, the scrubbed environment, the config dir and the prepared
  proxy passing through, glab not installed, 401 as signed out, a bounded
  line; remote URLs; glab's hosts and config dir read locally.
- `guild-desktop` `tests/all/git_worktrees.rs` (4 new): destination and
  `↑N` with an upstream, `bar` tracking `origin/foo`, no upstream (pushed
  under another name, never pushed), no remote and detached.
- `guild-desktop` `tests/all/forge.rs` (12): asked once, the focus debounce,
  the two-minute clock, a write, one pending refresh, a reply dropped after
  a branch switch, off forgetting and dropping a late reply and on asking at
  once, a failed batch keeping the last state, batching by host and
  project, reasons only where they help, the wording in both languages.
- `guild-desktop` lib: `view::column::forge_tests` (the row's glyph, badge,
  `↑N`, hover card, click and status-bar link opening the URL, off; the
  plain glyph's reason), `root::forge::tests` (which writes ask again),
  `view::settings::catalog` (the row, its place and search), `actions`
  (`cmd-alt-g` bound); `tests/all/menu.rs` (the menu's group, its stroke
  in front only); `guild-session` settings (on by default, survives).
- **Covered at the model's level, not with a glab held open**: a branch
  switch during a read is tested on `Forge` with the answer landed after the
  inputs moved; the push or switch trigger is tested as the Console line's
  verb and as `Trigger::Write`, not through a real push in a window.

**Isolated dev window** (`GUILD_STATE_DIR` of its own, no global hooks,
the person's Guild untouched), against the approved boards:

- With a stand-in `glab` (fixture `ZDOTDIR` and `GLAB_CONFIG_DIR`): every
  verdict colour and badge as the *MR glyph* and *Column rows* boards; the
  hover card, the blocked card; a click on the icon, the status-bar link,
  View MR from the menu and `⌘⌥G` each opened the MR's (local) page; Copy
  MR URL put it on the clipboard; the menu as the *Row menu* board; the
  status bar's *Conflict with main  !305 ↗* and *Pipeline failed  !306 ↗*;
  the signed-out host's reason; Settings → Projects → Git as its board; off
  turned every row plain, emptied the status bar, kept `↑1` and asked
  nothing even on coming back; on asked at once. The signed-out host and
  the github.com remote were never asked; inherited `GITLAB_*` tokens and
  URLs never reached glab.
- With glab missing from the login PATH: *glab not found* on a
  GitLab-looking remote, nothing on github.com, no request.
- **One real read** of the person's GitLab host through their glab (a
  fixture repository with a work project's remote, no network for git): !294
  green with ✓, !299 on both atl-760 rows — the `atl-760-review-fixes-r5` row
  tracks `origin/atl-760-chat-thread-routing` — with the threads badge and
  `↑1`. It found a bug: *No approval required* drawn with ✗, because that
  GitLab answers `approved: false` with nothing required and no
  `approvals_left`. Fixed with a regression test (`4f339d4d`); the fix was
  re-checked with the stand-in, not with a second real read.

**What differs from the boards**:

- The hover card opens at the pointer, over its row (GPUI's tooltip
  placement), where the *Hover card* board draws it to the side.
- !299 is green with a threads badge: GitLab says it is mergeable while 3
  threads are open (a project that does not require them resolved). The
  colour follows GitLab, the badge the threads, as the model decides.

## Open questions for the person

None. The two questions of the first draft were answered on 2026-11-25 and
are recorded above, in *Goal* and under *The setting*.

## Context

- Guild's design system is the reference for the look. See decision 46's rule and
  `docs/penpot.md`.
- **Code read on 2026-11-25**, branch `gld-063-gitlab-mr-status` at
  `b2db7d3f`:
  - `crates/desktop/src/view/column.rs:1851-1935`: the row's data, with the
    age and the `+N −N` lines.
  - `crates/desktop/src/view/column.rs:2207-2224`: the leading glyph.
  - `crates/desktop/src/view/column.rs:2283-2331`: the second line.
  - `crates/desktop/src/root/git.rs:1061-1095`: `counted`, which reads times
    and lines for each shown worktree.
  - `crates/desktop/src/root/git.rs:1098-1120`: the Push tab's `ahead`, with
    `git::upstream` and `git::remotes`.
  - `crates/desktop/src/git/worktrees.rs:163`, `:194`: `times` and `lines`.
  - `crates/desktop/src/update/remote/gh.rs`: the `gh api` adapter, a
    reference only for absolute executable discovery and proxy integration.
  - `crates/desktop/src/git.rs:1226-1247`: `Ahead::destination`, the remote
    and remote branch name a push goes to.
  - `crates/desktop/src/git.rs:1320-1343`: `ahead_args`, Push Commits'
    no-upstream reachability rule.
  - `crates/desktop/src/git/read_budget.rs:8`: the four git read permits,
    which glab children do not take.
  - `crates/desktop/src/usage/codex.rs:116-190`: a bounded child exchange
    under `Scope::Guild`.
  - `crates/desktop/src/network.rs:26-41`: `policy`, `apply` and
    `apply_policy`.
  - `crates/session/src/network.rs:21-32`: the scopes.
  - `crates/desktop/src/system/browser.rs:6-57`: `file:` URLs only.
  - `crates/desktop/src/terminal/view/hover.rs:191`: `cx.open_url` for an
    `https` link.
- **The adapter's base, `69c459cf`**, and the removed panel around it,
  `051087ff^..4db76eac`, are on no branch and no tag: they are reachable only
  through the reflog (checked 2026-11-25), which expires. The builder takes
  what it needs from `git show 69c459cf` early in the work. Its
  `crates/session/src/merge_requests/glab.rs` (718 lines) holds the
  transport, the error classification and the five writes that do not come
  back; `model.rs` and `crates/session/tests/all/merge_requests_glab.rs` go
  with it.
- **Spec review r2** (revust, `codex-only`, one architecture reviewer).
  The accepted findings are recorded in this file where they apply, each
  citing its number.
- **Decisions:**
  - 19: status bar;
  - 20: a menu per surface;
  - 21: git writes and their guards, untouched;
  - 33: a worktree is a root;
  - 46 and 47: a CLI's or a service's own credentials, a bounded exchange.
- **Traps:**
  - *Per-project Git workers still share one process descriptor limit*;
  - *A small activity ring can rebuild a whole cached column at display rate*;
  - *Application activation is not activity visibility*;
  - *The `claude` on the `PATH` may be somebody else's*, which applies equally
    to `glab`: use the login PATH, absolute paths only.
- **Overlap.** `touches` overlaps GLD-062 on `crates/desktop/src/root/git.rs`,
  so the two are not run at the same time.
