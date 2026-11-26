# Runtime ownership and audit decisions

This is the implementation map after the 2026-10-06 reliability audit. Historical
milestones explain decisions; `docs/decisions.md` holds the rules in force, and
a task's file under `tasks/` its acceptance scenarios.

| Owner | Responsibility | Failure contract |
| --- | --- | --- |
| `guild-session` | Session vocabulary, state writer, shared synchronous process owner | Waiting saves receive their write result; process groups are stopped before reaping their leader. |
| `guild-desktop::root` | Window coordination and calling domain owners | UI and CLI mutations use the same guards; file boundaries alone do not enforce exclusive ownership of Root fields. |
| `guild-desktop::update::runtime` | One updater worker, bounded pending/progress/result slots, selected payload and final-copy ownership | Selection invalidation cancels work; stale event tickets grant no authority. The host takes `stop()` and joins off-thread before quit; journal handoff must explicitly transfer preparation ownership. |
| `guild-session::update::transaction` | Destination lock, verified sibling journal, running-binary helper admission and startup acknowledgement | Unarmed helpers are revoked/killed/reaped by their background owner. Armed helpers deliberately detach, observe original parent exit, and retain previous app until exact first-layout acknowledgement. Root integration of this owner remains pending in selectable-update-sources. |
| `guild-desktop::root::forge` | One thread per repository's merge request refresh; `guild_session::merge_requests::glab` runs its glab children (decision 56) | At most two glab children across all repositories, each a `ProcessGroup` admitted before it starts, its output read from pipes with no byte past the cap kept; ended at a deadline or cap with `SIGTERM`, a grace, then `SIGKILL`. The setting going off or the repository leaving the column only stops a refresh: no child more starts and a running one finishes — glab may be saving a refreshed login — its answer dropped; the job stays owned until its thread ends, none starts beside it, and it can no longer publish. The quit aborts every refresh (`SIGTERM`, 100 ms grace) and, 150 ms in, `SIGKILL`s every glab group still alive through `ProcessGroup::killer`, inside GPUI's 200 ms quit budget. A reply whose candidates, setting generation or write epoch moved is dropped. |
| `guild-desktop::view` | Native rendering and interaction | Test real GPUI controls for layout/input contracts, not browser substitutes. |
| `guild-cli` | Socket transport and release command | No dependency on the desktop or GPUI; the window owns command authorization. |

## Process lifetime

`guild_session::process::ProcessGroup` owns a `std::process::Child` and its
process group. It is used by review execution, LSP clients, Codex usage,
CLI version probing, dependency queries and the glab merge request reads. No caller can reap the child directly.
Exit observation leaves the leader unreaped until group cleanup; repeated stop
returns the cached status. Drop is a fallback and logs cleanup failures. A
process that explicitly creates a different group/session is outside this owner.

Protocol readers stay with their adapters. Interactive sessions use terminal ownership.
PTY terminals, login-shell PATH resolution and OpenCode history retain their
specialized lifecycle policies. They must not be mechanically converted without
checking their session, deadline and background-process contracts.

### Managed terminal recovery

Harness terminals observe exit with waitid/WNOWAIT, retaining the owned leader
until explicit cleanup or proven quiescence. Recovery never invokes hangup to
obtain readiness. Bounded read-only session inventories reject live members,
retry disappearing PIDs and require stable non-live birth identities. Darwin's
libproc hides zombie metadata; a small C shim uses the existing SDK's public
kinfo_proc layout through sysctl so Rust does not duplicate platform ABI offsets.
The existing cc build dependency compiles it; no new service or dependency exists.

A getsid/libproc-unavailable process may also be proven foreign by two equal,
nonzero public rusage monotonic births strictly older than the retained SID
leader. Its generation stays in both full snapshots, and the leader generation
is checked before/after. This is not an exit proof: state=2 and positive exit ticks
alone do not qualify quiescence. Missing/equal/newer/reused generations remain
uncertain. Exec preserves monotonic birth, setpgid cannot cross sessions and
setsid creates only the caller's PID-named session; retained IDs cannot be reused.
No wall-clock age ordering, signal or longer deadline is used.

Managed live input is Codex-only and reads a private latch — the hook, rollout
and Esc machinery the displayed board used before OSC 7501
(`guild_session::status::Latch`), fed only by Codex sessions and never shown —
plus one refusal on top: while the program's own root OSC 7501 record is
`working` or `blocked`, no automatic input, whatever the latch says. In the
latch: a current-launch hook identifies the submitted
turn, its fresh exact rollout start/completion confirms that same turn, input
epoch stays unchanged, and retained leader/foreground and foreign-holder
inspection remain qualified. Shared rollout events cannot identify a writer or
answer a locally observed permission/question. A tool-first hook may establish
the current turn even when its prompt hook was lost; managed start confirmation
is separate from the latch's status deduplication. An authenticated Codex Stop may
supply that turn identity when the prompt hook was lost, in either arrival order;
it never publishes completion or resolves a wait. Fresh exact rollout completion
and the unchanged input epoch are still required. With no current-runtime turn hook,
live readiness stays unqualified, even after a disk completion. Missing local
resolution telemetry keeps an actual managed-runtime human wait blocked. Exact
turn/tool completions retain the latch's request correlation; provenance-free legacy
command terminals keep their ordinary status policy and cannot qualify managed
live input. No pending human
request or uncertain submission permits input. Claude Stop and OpenCode idle remain activity
observations, not readiness. Startup and live reservations use the existing strict
ledger; launch/submission and exact runtime/conversation/input start observation
remain separate. A retained native-workflow task keeps its original journal owner.

Managed OpenCode startup may exclude only a positively inspected registered
2.0.21 service. It reads the private same-user registration through no-symlink
components, authenticates read-only literal-loopback GETs with curl stdin, and
checks exact target history/cwd, inactive execution, empty inbox/forms, no foreign
TUI or established client, PID generation/listener and shared storage identity.
Both outer continuation inspections must retain the same non-secret witness.
Errors expose no credential or raw response. The executor remains standalone;
workflow recovery retains its stricter existing holder guard. Unknown service
versions, custom launch arguments/prefixes or uncertain clients fail closed.

### Editor task runs

Each project retains ordered Run entries with stable selected identities. An
invocation captures its source identity, quoted command and cwd. Distinct
invocations retain independent TerminalTab owners; rerunning the same invocation
joins its previous process tree before replacing that entry. Rerun, Stop, Clear
and terminal menus address the selected/captured entry. Hiding a panel or switching
projects does not dispose it. A live close asks Stop and close or Cancel; project
disposal and quit stop and reap every retained Run terminal. No Run is restarted
from project state.

Discovery reads bounded source snapshots off-thread through the registered
grammars, without running a build system. Incomplete walks and unresolved import
contexts cannot prove a standalone source; those candidates are omitted. The
saved-source launch check uses path and exact buffer content, independently of
discovery generations, because a successful save also triggers watcher discovery.

### Document notification work

Each Root owns a deadline index with at most one pending debounce per Document.
Document generations revoke deadlines and successful-save subscriptions on close,
server/policy disposal or editor replacement. FileTab-owned lifecycle listeners
consult live seats before rebinding; retained closed editors cannot revive paths.
Successful atomic writes emit an immutable Rope proof. Undo/reload becoming clean
is not a write; pending didChange precedes didSave for the exact written text.
Independent remaining holders retarget authority when the primary editor closes.

### Inline annotation and historical navigation

Project FileTabs retain their EditorState while annotation is toggled. Blame
requests and buffer mappings have separate generations; changing the path, HEAD,
disk or buffer invalidates the appropriate result. Clicks require the current
mapped text and measured visible row. Historical filenames and original lines
travel with porcelain groups, including repeated hash metadata. External sources
cannot request project blame. Saved legacy Annotate bodies remain restorable.

Git Log navigation pins project, exact hash and historical path to one epoch.
The existing worker resolves the immutable commit and its changed files; old
pages/replies cannot overwrite a newer intent. Rename destinations and sources
match explicitly. Missing commits/files show an unavailable state with no fallback
selection. These reads do not move the source editor or write Git.

## Persistence and errors

Editor saves resolve an existing symlink, write beside its target and preserve
permissions. A dangling symlink is refused. The directory handle is opened before
publishing the replacement, so an open failure preserves the original bytes and
state backup. File and directory sync errors reach
the caller; an error after rename can mean bytes changed but durability was not
confirmed. It is not a promise to undo a completed rename. Atomic replacement
does not preserve hard-link identity or prevent another program writing later.

The state writer serializes writes and coalesces queued snapshots by path. A
synchronous caller waits on a completion channel; the UI failure queue is not
its result channel. Newer queued snapshots may supersede older queued snapshots,
as before. A successful write clears the path's previous failure; consuming a
notice does not clear the outcome needed when draining a workspace before removal.

Review acceptance is calculated from verified candidates and scenarios. A
pre-existing verdict can pass; merely putting a finding in the synthesis
pre-existing category cannot. Disabling verification remains explicitly unverified.

## Test audit decisions

| Tests | Contract / weakness | Decision and evidence |
| --- | --- | --- |
| Editor source guards | Exact source checks can be brittle, but these protect two intentional patches in a dependency outside workspace test execution. | Keep alongside behavioral editor tests; do not delete as generic source-grep tests. |
| Review output and disclosure | Truncation must preserve complete output and native controls must reveal it. | Keep behavioral tests. An isolated truncation mutation broke two output tests; native disclosure tests already exist in `view/review_output/tests.rs`. |
| CLI usage/statusline timing | High default concurrency made startup compete with filesystem/process tests. | Use event-based fixtures: the installed HTTP adapter is held behind a release gate while the real statusline must finish. Ten-second waits are deadlock watchdogs, not throughput assertions. Local `just test`, `just quick` and CI use four test threads to bound process load. |
| File saving | Pure autosave decisions did not exercise actual symlinks or failed renames. | Add filesystem regressions for relative links, dangling links, permissions, saved-record identity and failed-write preservation. |
| Process completion | Existing cancellation tests missed an already-exited leader and inherited pipes. | Add local subprocess regressions for review, LSP, version probing and dependency queries. Descendant termination uses lifetime-pipe EOF, independent of macOS/Linux zombie reaping; retain usage deadline tests. |
| Review synthesis | Ordinary findings were verified; alternative categories bypassed the verifier. | Exercise each category through the pipeline with confirmed, unverified and pre-existing verdicts. |
| State persistence | Failure reporting and happy saves were tested separately. | Add failure-then-recovery regression; serialize tests consuming the process-wide notification queue. |
| Dependency boundaries | Shell conditional pipelines hid discovery failure and early-match SIGPIPE. | Execute the real recipe with controlled Cargo output, including a large forbidden tree and a discovery error. |

No existing tests were deleted. Defect reproductions were observed failing against their original owners before
implementation changed; additional preservation checks retain existing behavior. Existing native tests run
in the normal workspace suite; this audit does not claim a manual GUI walkthrough
or live provider validation.

## Refactoring decisions

Extract process ownership and review acceptance because reproduced defects cross
adapter boundaries. Keep Root coordination in place: its size is a maintenance
risk, but moving fields without a concrete ownership contract would only relocate
coupling. Review event persistence and large Root subjects warrant separate
measurement and acceptance scenarios before a broad rewrite. No runtime framework,
visual redesign or vendored dependency change is required for these repairs.

## CI portability

The shared crates target Unix processes and sockets. `just test-headless` runs
session, review and CLI tests with a locked dependency graph; the manual workflow
now has an Ubuntu job alongside the full macOS desktop job. GPUI/AppKit desktop
coverage remains macOS-specific. This is not a claim of Windows support.

New fixtures use temporary directories, child-only environment overrides and
local stand-in CLIs; no user's model installation, credentials or real sessions
are needed. POSIX sh/sleep, Git, Rust and just are runner prerequisites. Process
lifetime is observed through protocol/pipe completion rather than the speed of
PID reaping. Platform-specific production behavior (Darwin's EPERM) stays behind
cfg and has the same public completion assertions as other supported Unix hosts.

The HTTP post is a small installed adapter; ordinary statusline integration tests
still exercise its real local socket request. The nonblocking test replaces only
that adapter with a gated fixture, leaving production backgrounding and stream
redirection unchanged. This avoids relying on curl's timeout or a fast machine.

## Terminal jobs

`desktop::terminal_job::LiveJobs` owns retained terminal entities independently
of overlays. Its receipt reservation is saved synchronously before spawn; a
failed write forbids launch. Descriptors and environment overrides stay in
memory. Unreadable receipts block launches rather than rolling reservations
back to a backup. Recovered unfinished entries become interrupted, never a PID
list to kill or an instruction to relaunch.

Book owns an immutable ordered receipt snapshot until its metadata changes.
LiveJobs shares that snapshot and owns a separate revision/error-keyed overlay
only during storage failures. Mutable receipt access invalidates snapshots;
equal presentation/release updates avoid mutable access. Persistence retains the
existing owned wire form. Activity checks admission before deriving sources but
drains completed work on every poll and observes archive changes periodically,
independently of receipt revisions.

The job-specific PTY supervisor installs ownership immediately after spawn,
keeps its leader unreaped while stopping session groups, and publishes completion
once. Cancellation is asynchronous; application shutdown first cancels all jobs,
then joins their supervisors and flushes receipts. Terminal output remains until
native release, while durable completion remains after release. Ordinary tab
PTY ownership continues through its existing path.

## Terminal descriptors and language preferences

The terminal prepares its writer and reader before spawning a child. Its writer
uses atomic F_DUPFD_CLOEXEC: ordinary subprocesses and language clients must not
inherit a live terminal master. Completed-grid ownership stays independent of
PTY ownership; the existing completion supervisor continues releasing it. Root
registers its quit cleanup in its constructor, including roots never rendered.

Repository language policy is stored in Settings under the canonical common Git
directory, or a confirmed canonical non-Git root. Identity resolution runs
off-thread and gates first launch. Per-Root reconciliation stops disabled owned
clients, clears their documents, diagnostics and editor providers, and advances
their generation. Window requests hold exclusive leases naming the project,
language, client generation and registered transport request. Protocol IDs alone
cannot retrieve answers. Dropping a consumer future, document close or operation
replacement revokes that exact pending entry and any arrived payload before
best-effort `$/cancelRequest`. Find tabs own cancellation guards for references
and auxiliary definition; symbol flights release obsolete rankings independently
per language. EOF, writer failure and client stop clear all retained requests.
The registry accepts only active IDs and the first answer, rejects late/unknown
or duplicate replies, and expires safety entries after 120 seconds during
response dispatch/poll/take without requiring another ask. Caller timers retain
their existing five-second and Find Usages 60-second deadlines. Root has no
second metadata token table. Enabling reconciles existing
buffers after initialization and preserves remembered startup failures.

Scoped preference edits enqueue field-level requests; an owned background worker
awaits Store's existing serialized writer. The UI publishes only the acknowledged
field into its latest Settings, preserving newer ordinary edits. Ordinary snapshots
are deferred until the scoped queue drains. Failed writes preserve effective policy
and drafts; Retry retains the edit, not an old whole-Settings copy. Quit drains
owned preference writes without re-entering App. Settings host selection and field
drafts are transient and released with their host or removed target.

Ordinary Root tests disable installed language-server launches per instance.
Explicit fake fixtures own bounded lifetime sockets, readiness witnesses and
cleanup guards; no test needs a host PATH mutation or installed indexer. Historical
orphan provenance is not inferred from PPID=1.


## Configured workflow task cancellation

The workflow writer reserves cancellation and each child stop separately. Root
rechecks the window-owned task/runtime/conversation and calls the same guarded
task stop as the CLI. The shared stop exposes the terminal owner's `let_go`
completion receiver before closing that task's pane. Root polls that receiver
without blocking the UI; only its successful completion records a stopped group.
A deadline/disconnected completion records pending or unknown, never inferred
termination or a PID-based kill. View closure preserves worktree/history and does
not stop unrelated panes. A lost completion leaves the retained stop attempt unconfirmed; recovery must
reconcile its owner rather than repeat teardown after restart.


## Configured external-review observers

The same Root workflow writer owns installed-launcher `ProcessGroup` observers.
The native terminal registry independently owns their actual jobs. Observer exit
never cancels or proves completion of a job; hiding a presentation keeps its
owner. Actual native receipts are queued back to the workflow writer, which
qualifies saved archives off the UI thread and retains actual usage uncertainty.
The existing Root poll loop includes intents that failed before job creation and
stops periodic reads once observer and job/result observations are retained.
A subsequent serial launch cannot replace an earlier live observer owner.

Workflow journal primary snapshots use direct atomic replacement with file and
parent-directory synchronization. They cannot use the general settings Store's
primary-to-backup rotation: strict workflow readers reject backup fallback and
must always see a complete primary while another reader observes the run.


A round with executor environment overrides records qualified original references,
not values. Its private SDK layer lives in a fresh0700 temporary directory with
0600 files, an exact payload digest and a value-free ownership marker. A separate
owner receipt is synchronized in the control directory before SDK preparation;
the reserved round retains it before launcher execution. Preparation failure
releases only its own layer; failed cleanup reports the retained owner. Actual
terminal job termination releases the layer and records that fact with the archive
result. Observer exit and dropping an in-memory handle retain it when the native
job could still be running. Restart reconciliation must inspect that retained
claim and actual job ownership before any cleanup or dependent effect.


Workflow origin admission retains append-only run attribution in its existing
kernel-held origin claim before creating run storage. Partial/corrupt associated
storage blocks that origin without poisoning unrelated origins; legacy start
receipts can recover attribution. Claim files and uncertain archives are never
removed to make admission pass. Review execution flags are released only after
actual owner completion and accounted usage; archive completion alone does not
end a keep-open job. A proven refusal before acquiring a launcher owner records
closed zero-start usage and releases only its qualified private lease. Missing
jobs after a spawned launcher remain uncertain. Cancelled assigned children can
be reconciled by exact saved identity and existing bounded conversation-holder
inspection, with native identity/holder rechecks on callback. A refused stop or
PID-only probe is not absence. Unknown child launches retain their origin barrier.


Recovered-origin acknowledgement binds orchestration facts, not ordinary parent
activity. Its assessment projects only that exact origin's Working/Done/Idle
status into one stable category. Waiting/unknown states and all child statuses
remain significant. Raw observations and input-readiness checks are untouched.
Queued effect permits carry the exact origin and use the same projection at
native execution; legacy map-only permits still compare strictly. This grants
no parent-input transport and answers no human approval. Existing acknowledgement
history is immutable; a pre-projection receipt may need a fresh acknowledgement.


### Strict draft policy (user decision, 2026-10-20)

Known unsent input blocks automatic handoff independently of completion/clean
flags. Process persistently explains that Guild did not submit or change it and
links to the original session to finish, send or remove it. There is no force,
clear-draft or approval-waiver action. Removing input does not qualify the other
identity/source/readiness guards and cannot replay uncertain delivery.

Unknown draft absence is not an empty prompt. The installed OpenCode adapter
cannot qualify original empty-input readiness, so both startup payload and later
automatic prompt handoff are withheld. Its exact stopped conversation may still
be restored through the existing native owner without --prompt. No startup
handoff submission/reservation receipt is created for that unsent payload.
Process distinguishes this adapter gap from an observed draft and retains the
Open session action. Historic unknown/submitted receipts remain unchanged and
keep their stronger no-replay state. Claude/Codex startup ownership is unchanged.
This supersedes iteration11's OpenCode automatic startup claim, not the required
final three-harness scope: full OpenCode automatic continuation remains pending
qualified support and parent acceptance, with no typed-API exception.


Code05 input correction: the existing Prompt.pending includes local unobserved
submissions and conservatively unknown non-text input effects, so both correction
reservation and final native Prompt comparison refuse a competing payload.
Generic terminal submission retains its existing explicit-caller transport
semantics; workflow guards remain at admission, writer and final native comparison.
Enter emitting CR transfers unsent draft to awaiting-adapter-event state, including
Ctrl/Cmd/Fn modifiers; Shift/Option+Enter
keep multiline text unsent. Navigation, interrupt and deletion preserve existing
drafts but cannot create a known-text draft. They still mark input as used and
withhold automatic sending. A later submission remains pending until an actual
adapter observation; Enter or completion alone never qualifies workflow emptiness. These are terminal
input facts, not new workflow states or send authority. OpenCode automatic input
remains unavailable where original emptiness cannot be qualified.
Native review is retired. Existing `.guild/review/`, `guild-agents.json` and
`guild-workflow.json` data remains untouched and inert; Guild neither initializes
nor migrates it. External revust uses generic Terminal Jobs/archive_viewer APIs.

### Isolated public CLI regression prerequisites

`just test-crate guild-desktop` builds the current `guild-cli` binary before the
desktop tests; optional Cargo filters follow the crate name. Bare Cargo desktop
regressions must first run `cargo build -p guild-cli --bin guild` with the same
target directory and profile. The test-only `GUILD_TEST_CLI` override names an
explicit separately built CLI for isolated-target qualification; its caller owns
that build prerequisite. This is not a desktop/CLI dependency edge or installation.


### Bounded transport ownership

LSP owns three local IO workers and a pre-created reaper. Its pipes use
nonblocking IO and event-driven polls on peer readiness or a local stop descriptor; process group stop never substitutes
for local interruption. The leader stays reserved until original-group stop/reap.
A separately owned process retaining stdout/stderr cannot hold worker joins.
Partial thread setup revokes IO and hands all started workers to that owner.
Headers are incrementally bounded at 8 KiB, bodies at 16 MiB. Partial frames use
the existing 120-second retention window; idle streams do not expire. Capacity
waiting pauses the peer-body clock.

LSP events/outgoing each retain at most 256 operations / 32 MiB encoded bytes.
Incoming body capacity is reserved before allocation, event batches retain their
permits through processing, and active writes retain outgoing permits. Pending
leases retain at most 256 requests / 32 MiB encoded answers plus URI metadata
charged conservatively to the same byte cap. Unknown/late answers cannot allocate
retained slots. One state lock serializes registration and terminal failure;
capacity/size/IO failure settles leases and revokes admission. Reasons contain no
payload. Stderr drains continuously in at most 8-KiB fragments. JSON tree,
allocator and bounded encoding/decoding overhead are additional to wire budgets.

PTY input admits a whole operation FIFO, at most 256 operations / 8 MiB including
the active write. Refusal cannot deliver a prefix or mutate prompt/input epoch
facts; post-admission IO failure is uncertain delivery. Transcript baseline is
committed after capacity validation and before writer visibility. Reader aliases
share O_NONBLOCK and poll on WouldBlock, preserving CLOEXEC. Ordinary cleanup
retains a master anchor through bounded grace/drain and original captured-group
signals, then closes it before reap. Supervised cleanup owns its anchor until
signals are sent. No new group/kill authority is granted.

Transport reapers join off GPUI; app-quit awaits a weak, live-owner-only completion
snapshot. Quit final receipt persistence owns plain metadata/slots in background,
never re-entering App. Run rerun keeps one pending task until the previous actual
supervisor completion; entry/project disposal cancels that task. Owned tests
observe encoded/item counts, real slow IO, strict continuation receipts and a
separately test-owned group holding real pipes. None is a provider workload.


### Diff calculation ownership

Each DiffTab owns one CPU worker and a coalescing mailbox: one active job,
one latest pending input per current path, and one completed result per path.
Immutable source pairs carry a monotonically increasing ticket and comparison
identity. FileDiff/hunks, original-byte Document and optional syntax paint are
computed off GPUI and accepted atomically only for that ticket. Removed paths,
inspection changes and new source pairs revoke authority before old work can
publish. Mode, navigation and context projection never calculate hunks/documents.
Unchanged paths retain their models; an unchanged blob refresh moves its last
ready pair back without recalculation. Bytes above the existing 4 MiB side limit
are not duplicated into the compute mailbox.

The worker checks cancellation before/between algorithm stages and before
publication. Both diff algorithms use similar's 150 ms deadline and complete
fallback scripts; allocation/tokenization and syntax parsing are not hard
wall-clock interruption promises. Completion and blob delivery wake the entity
through a coalesced Notify, independently of Root/satellite polling or rendering.
Stop clears tickets/mailboxes and wakes the worker. App quit awaits its off-thread
join; ordinary owner Drop transfers the retained JoinHandle to a background
cleanup task. No worker/task is created per keystroke. Existing Git read gateways
and provider/process ownership are unchanged.

### Common-repository worktree reads

Root's `root::git::shared::Coordinator` admits only canonical common Git directory
identities resolved on existing StatusWorkers. A repository owns one active read
reply and one latest pending refresh epoch. Its representative worker performs
both raw listing and conditional/full enrichment once; status maps and mutations
remain per-project. Identity failures stay isolated and retry on a later wave.
Forced/count-invalidating refreshes revoke old publication and stop further enrichment at stage
boundaries; current synchronous Git calls retain the existing read gateway owner.
Representative close does not destroy the active reply receiver. Last subscriber
close cancels enrichment and drops queued requests/comparison snapshots; no new
service/thread exists. Root disposal drops the workers' existing ask senders.
Every wave reads Git; previous raw rows are comparison state, never a freshness
cache. Completed writes force a new epoch before collected old replies publish.

A failed representative is tried at most once per refresh epoch before another
already-admitted sibling is selected, serially. Later waves prefer the last
successful source. Errors complete the current readers and never cache a success;
a missing cwd cannot strand live siblings or poison an unrelated repository.

Ordinary cadence during an active read coalesces a later observation without
revoking current publication, so slow but stationary work can finish. Full intent
survives representative errors until success. The exhausted-death worker retains
its own visible error; bounded fallback and last-success preference let a live
sibling complete full refresh/command settlement without restarting that worker.

Normal transport idle has no periodic timeout: empty/full queue waits use actual
condition notifications, and IO poll has no timeout unless a partial LSP frame
has its existing deadline. Stop closes registered queues under their own state
locks and signals a local CLOEXEC socket pair; all workers wake without peer EOF.
IO wrappers own wake descriptors, released when IO ends. The separate post-EOF
unreaped-child observer may check exit every 25 ms until completion/stop.

### Image file previews

Each FileTab owns its retained image task and generation. Bounded file reads and
GPUI decoding run on the background executor. Reload or a new SVG preview revokes
the previous generation; closing the last file holder drops its task. A synchronous
decode already executing may finish off-thread, but owns no UI authority. SVG disk
snapshots are installed on the window thread, preserving the retained editor; a
new dirty buffer wins over an outstanding disk read. Preview/source changes never
write the source. GIF frames and timing remain owned by GPUI's decoder and Img.

Direct RenderImage previews explicitly release their atlas entries on replacement
and last FileTab release. Cleanup is deferred to the end of the outer App update:
the currently leased Window is then back in App.windows, so drop_image covers it
and every other window. A remaining FileTab holder retains its current image.
The headless platform atlas regression checks real painted entries in two windows,
replacement removal, retained ownership and final release inside a Window update.

Source-location navigation retains project/history/protocol destinations while SVG
source is loading, and keeps search selection outside the post-layout frame budget.
File notifications wake the pending landing rather than polling it every frame.
Definitive read/non-text failure or a superseding destination cancels it. External
source landings pin tab, origin and path. Preview focus is registered in its rendered
container so Save and Close keep the window's existing action ancestry.

## Bundled diagram renderer

Root owns desktop::diagrams::runtime::Manager: one serial worker, bounded
engine/source request identities, an active cancellation flag and decoded cache.
Only exact resources from the current Guild.app are used. Explicit development
launchers supply their prepared directory; production ignores that override.
No app-support package discovery, adoption, installation, deletion or networking
exists. Resource verification and PNG decoding run off the UI thread before
executing the separate signed bundled helper. JS hashes are immutable Guild pins;
the helper digest is a sealed bundle resource generated after inner signing.

Consumers hold leases by engine/source; individual fence spans retain independent
layout views. Source replacement, Source mode and close release leases. Late or
cancelled replies have no replacement authority. Natural logical dimensions
control layout; physical pixels control decoding/cache budgets. Measurements
schedule corrective frames via window.defer after drawing, rather than trying to
invalidate during prepaint. Hidden TextView rows invalidate their existing inline
layout on manager notifications, retaining native selection and scroll state.

Transport owns a ProcessGroup and its input/output threads. Cancellation/timeout
stop/reap before pipe joins. The native helper alone links system WebKit/AppKit
and runs it on its main thread. It returns PNG, never editor overlays. HTTP/file/
stdlib access and navigation are restricted. WebKit XPC services remain system
managed; no global WebKit PID is signalled. JS is separate Resources content.

Manager retains at most 16 MiB decoded pixels plus one bounded 16 MiB decode;
CPU/GPU accounting stays conservatively within 64 MiB. Eviction/replacement and
close explicitly discard native atlas entries after the current Window lease.
Quit cancels, drops pending replies and awaits worker join off-thread. Release is
a fallback. Only active work has a 20 ms poll; idle owners have no timer. Missing
or corrupt resources produce packaging errors, with no runtime fallback/download.
See docs/architecture/local-diagram-preview.md for the renderer's contract.
