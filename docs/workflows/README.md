# Configured development processes

The portable definition is strict UTF-8 JSON v1. The public structural schema is
[definition-v1.schema.json](definition-v1.schema.json); semantic validation also
checks unique identities, references, action/role compatibility, single outcome
producers, dependency cycles and aggregate correction bounds. Unknown fields,
actions, validators and condition operators are errors before effects.

The shipped [development definition](../../crates/session/assets/workflows/development.json)
is the complete example. Definitions are selected under `.guild/workflows/ID.json`;
run state belongs separately under main-checkout `.guild/workflow-runs/`.
Loading a definition starts nothing and supplies no action authority.

Definitions resolve from shipped defaults, then project data, then explicit
selection/overrides. Objects merge by key; arrays and scalar values replace.
Explicit false and zero are preserved. A zero review/correction limit permits no
new corresponding admission; correction edges must be removed when setting the
aggregate correction limit to zero. An active run freezes its effective inputs;
new defaults and external skill edits apply only to future runs.

IDs use `[a-z][a-z0-9_-]{0,63}`. Labels are nonempty single-line text, at most 512
UTF-8 bytes. A graph has 1..256 steps. Dependencies must be acyclic, with stable
array order breaking ties. Bounded correction edges are separate from ordinary
dependencies. Limits are nonnegative integers, at most 10000 review calls and
100 correction rounds; defaults are 18 actual starts and 3 aggregate corrections.
These upper bounds do not grant a provider-call allowance.

Actions name native capabilities, never executable shell commands. The parent
owns review, acceptance, decisions and local delivery; assigned managed roles
own implementation. `contract.prepare` can use a parent or managed designer.
`review.request` requires an independent review role operated by the parent.
The host must still establish role availability, identity and real permission.

Capability conditions have a finite v1 vocabulary: `git-checkout` (a captured Git
commit), `native-origin` (the exact non-task native origin with one qualified
holder), and `openspec-executable` (an executable discovered through the host's
actual login PATH). Start records these host observations and their provenance
in the journal; replay uses those receipts. Absent/ambiguous native observation
is unknown and waits, rather than becoming false. These are start-time host
facts, not permission or readiness for later input: actual effect owners still
recheck source, identity, ownership and mutable runtime state at admission.
An outcome condition cannot skip an unfinished dependency or its producer.

Only one active run may belong to an origin. Distinct starts name the retained
run instead of allocating another ledger, under an origin admission claim.
Codex/OpenCode initiation waits for the actual original conversation identity;
it never freezes an unknown identity and later adopts whichever hook arrives.
Public assigned-child `recover` is refused. Only an actual native Process
navigation gesture grants bounded origin recovery; it grants no parent task
administration or reconciliation acknowledgement to the child.

Managed `contract.prepare` reserves the existing guarded checkout/session path
without inventing an accepted contract. Its exact assigned designer submits
checked contract evidence including the granted report. The original parent
accepts that retained report separately before review or implementation can
advance. Subsequent acceptance/delivery qualify the accepted child checkout and
baseline while preserving the separately pinned sole contract provenance.

Review preparation has its own durable pre-start attempt. A proven inspection/
configuration/preparation refusal retains its files, consumes no provider-call
allowance, and permits a corrected new request in a fresh owned preparation
slot. Same-request retries return the retained result; uncertain preparation is
not replayed. The logical serial review round and its later correction lineage
remain unchanged. No ambiguous/live archive or uncertain lease is deleted.

`RetryRequest.attempt` names either the positively refused native launch receipt
(using its retained checkout) or a completed external-review reservation with
qualified findings (same-session correction). It never names an arbitrary PID
or archive. A subsequent review is a separate parent-owned request.

The default graph makes implementation, code review, product acceptance and
local delivery conditional on non-architecture-only intent. Skipped dependencies
remain explicit, with their reasons. Configuration cannot publish, push, deploy,
install skills or bypass guarded worktree operations.

This document describes the portable definition contract. Runtime, transport,
native navigation and live acceptance remain separate implementation checkpoints;
a structurally valid JSON file alone establishes none of them.

External skill capture uses host-qualified project/global discovery roots, with
project-first auto selection. Root aliases are resolved once; selected skill
aliases and nested resource links must remain within their qualified scope.
The full selected directory is captured, including local Markdown link/image/
reference-definition resources. Code examples are not resource declarations.
Missing required instructions wait; missing optional instructions record a skip.
Neither case installs a package or creates a discovery root.

Snapshots retain original source, scope, resource types, access modes, link spellings and sorted
SHA-256 manifests, while preserving safe internal links with snapshot-relative targets.
The run-owned content directory is read-only and active assignments use it rather
than mutable originals. Escaping, dangling or cyclic links, credential/provider
resources and unresolved local Markdown references are named gaps. Capture is
bounded to 4096 entries, 32 directory levels, 16 MiB per file and 64 MiB total;
definition JSON is bounded to 4 MiB. These are diagnostic bounds, never permission
to silently omit resources. Snapshot verification reads retained bytes without
re-resolving current global or project instructions.

Source identities include the actual checkout, full baseline/HEAD and sorted
index, working, untracked, removed and recursive-submodule observations. A
same-HEAD edit is a different identity. Positively unversioned targets retain
file manifests with explicit absent Git revisions; no revision is invented.

Evidence binds an authorized producer, accepted contract, step/scenario, exact
source, actual tool/version/argv/time and retained output digests. An authored
`done` claim is never completion. Command-only output cannot satisfy native/API
requirements. Expected negative CLI/API results are explicit check expectations;
a qualified mismatch is FAIL, whereas inaccessible/stale/unqualified proof is
UNVERIFIED. Native records require actual process/window/gestures and retained
captures. Review records also require the installed bridge's launch, source and
actual-attempt qualification. Portable mechanical validation is not independent
review or a substitute for observing the real compiled product.

Run storage uses a single exclusive coordinator claim and numbered atomic event
receipts. Every reservation pins the complete request context and preceding
revision/digest before an external effect is admitted. Identical retries return
the original attempt; a changed context or stale revision is refused. A restored
unresolved reservation requires reconciliation and never permits blind replay.
Read-only history does not acquire execution authority. The kernel-held claim is
released with its owning file descriptor, without PID-file takeover or unlinking
a lock another process may hold.

The primary state must be readable and match contiguous committed events; a
lagging valid snapshot can be replayed. Corruption, missing primary data or a
failed write blocks mutation rather than loading an older backup and allowance.
Partial run directories and retained event bytes are preserved for diagnosis.


Explicit role model/effort values are bounded literal selections, distinct from
permissions and provider credentials. They are retained with the session and
reused by reopen/restore paths. Absent overrides keep the harness's existing
profile. Claude receives model/effort options; Codex receives model and a literal
reasoning-effort configuration value. OpenCode v2 uses its inherited per-launch
inline configuration, retaining unrelated permission/plugin/provider fields and
both string and object model selections. An explicit override cannot partially
rewrite JSONC or guess an unknown filesystem model to attach an effort variant;
that dependency reports an exact input gap. No global profile is rewritten.


A ready managed `session.assign` node uses the configured role's harness and
selections, a separate checkout at the pinned commit and one complete literal
assignment under the run's `artifacts/`. It references sealed contract resources
and selected skill snapshots, and grants a specific durable report location.
Original target dirty/untracked observations remain distinct from the new
checkout's source identity. The existing native task/worktree guards apply
before creation and launch; only a qualified live owning origin can dispatch.

Checkout reservation, actual checkout creation and native session launch are
separate receipts. A launch acknowledgement is not task delivery or checked
completion. Unresolved reservations withhold replay; a failed launch retains its
created checkout and recorded session facts. Inspection itself admits no work,
and retrying an existing request cannot start another checkout or process.


Native prompt submissions are correlated with the actual bound child launch and
recorded separately as delivery observations. Early hook events wait for their
launch receipt; duplicate or foreign sessions cannot record another delivery.
A delivered task remains active until its configured checked outcome is supplied.
These native mechanics do not by themselves prove real model behavior or final
acceptance.


A bound assigned child may inspect its run and submit evidence for its own step
through `checkpoint`. The host checks the delivered conversation, checkout,
accepted contract, current source and report grant, retaining the report without
finishing the step. The origin separately accepts that exact retained evidence;
changed source, fabricated delivery or a different conversation cannot qualify
it. Children cannot use that channel to accept another role's outcome, recover
the origin or cancel the run.


Explicit parent cancellation records its request before stopping recorded native
children. The native adapter rechecks this coordinator's runtime ownership and
conversation, then uses the existing guarded task-stop/pane-close action. PTY
owner completion, rather than a closed view or a stored PID, confirms a stopped
group. A pending or unknown completion stays unconfirmed. Checkouts, source
changes, reports, history and unrelated sessions remain available; repeated
cancellation does not repeat a recorded stop.


The portable review ledger counts checked engine `started` attempts across find,
retries, synthesis and verification, including unsuccessful processes. Open or
ambiguous usage keeps its unspent allowance charged and blocks another serial
round. Completed parent preparation receipts enter the same ceiling; corrections
share one limit across plan and code. A terminal usage record reconciles attempt
identities; it does not establish a clean review. The native bridge must also
qualify the actual launcher/job, enforce its allowance and bind the archive to
its contract and source before using a result.


CLI initiation validates bounded strict request/overlay structure without resolving
project-owned references. Root loads the selected project definition and validates
the full shipped → project → explicit graph before any run/effect admission.

External revust preparation pins the selected installed package selector, executable
and helper identities. Read-only configuration inspection preserves raw source
settings/provenance; snapshots retain winning prompt/structured profile bytes and
cap calls to the reserved allowance. Execution supplies an explicit one-second
auto-close flag, or zero for an explicit per-run keep-open request, independently
of raw source preferences and launcher fallback defaults. These preparation
checks do not start a review or establish integrated review/UI acceptance.


An original live parent can request a ready `review.request` node with
`guild workflow review RUN --from-file request.json`. The versioned request names
`request_id`, `expected_revision`, `step`, selected absolute `package`, optional
`config_dir`/`profile`, literal `scope`, positive reserved `allowance` and optional
`keep_open`/`prior` archive usage receipts. The CLI transports the request; Root
checks its original conversation, source and configured role before reserving the
external launch. Assigned children cannot request independent review.

Each round uses the selected installed launcher and the existing Terminal Jobs
protocol. Its actual job identity, observer exit, attempt usage and checked archive
are separate observations. A normal process exit alone cannot finish a node:
source/contract, expected and reported roster, retained evidence and the installed
saved reader must agree. Findings and incomplete/uncertain results remain gaps;
uncertain usage retains its reservation. An exact request retry cannot launch
another round. Imported completed archives charge actual attempt identities and
do not accept their outcome for the current source.

Later serial rounds derive resources from the first retained configuration, not
mutable original profiles. Each gets its reservation cap under the original
policy ceiling; an initial small reservation does not replace that ceiling.
Changing package/profile selection in an active run requires reconciliation.
Executor environment overrides retain only qualified original file/key references
and digests. At round preparation those exact references resolve into a temporary
SDK user layer outside durable run/archive storage: directory0700, files0600.
The round keeps a value-free owner receipt before dispatch; the native job receives
only its path. Changed original sources, private bytes or ownership refuse launch.
Actual terminal job termination releases this layer; observer exit, hiding a view
or losing the in-memory owner does not. A crash retains the owner pointer for
reconciliation, never permission to reconstruct values or repeat the launch.
Uncertain jobs retain their private resource claim. Process navigation, full
restart recovery and correction controls remain work in progress under the
accepted contract.


A successful checkout followed by a positively refused native session launch is
a partial result, not permission to create another checkout. The original parent
can submit `guild workflow retry RUN --from-file request.json` with version1,
`request_id`, `expected_revision` and the exact failed `attempt`. Root verifies
the original checkout observation, unchanged checkout/source/accepted contract
and literal task before reserving another launch. A prior unknown launch,
existing session record, cancelled run, changed source or a newer native attempt
requires explicit reconciliation instead. Repeating the same key/payload returns
the retained receipt and creates no native callback. The native path still
rechecks the original live parent and uses the existing session action.


The headless correction reducer follows only configured finite correction edges.
A correction reopens its target and dependency/condition consumers, retaining all
historical checked evidence. Prior evidence IDs/outcomes cannot finish the next
submission even at the same source identity. A contract correction permits a new
checked revision of the same selected contract and invalidates its earlier review.
Plan and code share the graph ceiling; Root must additionally reconcile imported
corrections and reserve actual external review usage. The native same-conversation
path described below adds durable delivery receipts without changing the pure reducer.


Root reconstructs native assignments, review ownership and checked outcomes in
actual receipt revision order. It never consults a future callback while reducing
an earlier reservation. Review observations bind the exact reserved round rather
than the first row with the same configured step. A late observer exit or native
presentation closure updates its ownership record without replacing a checked
result with pending activity. This ordering also preserves stopped-child activity
after earlier delivery receipts and is required before correction resubmissions.


### Native Process inspection — first visible iteration

Starting a run adds a non-session Process tab beside its actual origin without
selecting it. The origin strip button and `guild workflow show RUN` select/reuse
that run. Closing Process retains the origin, run and its navigation button.
The view renders frozen graph labels/roles, derived node states, actual assignment
checkout/session receipts, saved review/job facts and aggregate used/reserved/
remaining/correction counts. These are runtime snapshots, not phase controls.
Inspection refreshes on the existing Root worker; saved viewing does not poll a
provider, resume a harness or admit another effect. Unreadable retained state is
a local Process gap; ordinary restored bodies remain available.

This iteration does not expose unfinished correction/recovery actions. Linked
child recovery, full parent/Process group close/move semantics, richer evidence/
question navigation and integrated acceptance remain incomplete in the accepted
OpenSpec tasks. Existing body wire tags are unchanged. Older builds do not know
the new `process` body tag: retain their pre-feature workspace/project state when
trying a downgrade; do not expect an old build to read mixed Process state.


The origin's **Open process** icon remains after closing Process; it is a user
navigation action, not a harness-only control. The adjacent single-body Process
tab is icon-only with a localized Process/parent tooltip and its own hover close
target. Existing CLI `guild workflow show RUN` also reopens/selects the same run;
none of these presentation actions needs a model call. Mixed-pane tabs retain
ordinary labels and split behavior. Configured steps follow derived dependency
order even when the definition array lists a dependent before its predecessor.


The Open process icon appears only while that RunId has no open Process body.
Inactive views and unfocused mixed panes also count as open. While open, the
entire reopen child is omitted, with no placeholder or extra flex gap: the origin
tab fits its visible content. Closing the view adds the compact reopen button;
reopening removes it again under the same RunId. Width may change naturally.

When Open process is the last item, its own hit target supplies the trailing
inset; the origin adds no extra right padding. Activity indicators retain their
normal padding when they follow the button.

Closing an origin removes its linked Process bodies after the existing close
confirmation succeeds. Cancelling preserves both views and the live terminal.
Process-only close preserves the origin and same-run reopen action. The rule
applies to tab, keyboard, menu, bulk, pane and project/workspace close paths;
closing an area only rehomes its tabs as before. In mixed panes, only matching
Process bodies are removed; another retained origin body keeps its views.
Durable runs, review history, children and worktrees are not cancelled or removed
by this view rule. Restored unloaded references are resolved against their
snapshot and the original tab/pane identity, without dismissing a later view.
Group movement remains pending.

Forget-sessions-on-project-close resets only saved view state; it preserves
configured workflows and durable workflow-run/review history in `.guild`.

Assigned child tabs expose localized Process navigation only after native launch
and correlated task delivery prove the exact run link. Navigation reuses an
owned live parent across restored workspaces. `guild workflow recover RUN
--from-file REQUEST --json` uses the owning socket and requires a live original
parent. An assigned child's native Process click uses a separate trusted
navigation entry; child IPC cannot grant that authority. A stopped parent resumes only its exact stored
conversation/harness with the saved model, effort and native policy; missing
identity/history/policy, uncertain holders, missing cwd or stale callbacks refuse.
Legacy records without launch policy require explicit reconciliation.

Recovery reserves a durable attempt before holder probes or launch. Duplicate
requests reuse receipts; an unresolved crash reservation is delivery-unknown and
is never replayed. OpenCode export verifies the exact saved session and cwd; an
OpenCode runtime without qualified conversation ownership blocks recovery.
Read-only status/show never request recovery. Showing an un-restored foreign
workspace refuses rather than implicitly starting its saved terminal sessions.
A show pending before origin close cannot recreate that group; a later explicit
historical inspection remains available.

The Process attention card distinguishes reservation, live reuse, requested
same-conversation launch and recovery gaps. Launch is neither readiness nor
orchestration continuation. Retained child/source/review ledger and native
waiting states are reconciled, no task is replayed and no question is answered.
Automatic resource/continuation recovery remains gated and task6.3 incomplete.
New review/retry dispatch and automatic assignments wait after an actual or
uncertain resume. Merely revealing an already-live parent does not pause its
workflow, inject input or require approval. A refused prelaunch probe shows its
gap without claiming a resumed parent; independent source/effect guards still
apply. Unresolved reservations and legacy gaps without no-launch evidence fail
closed. Later live navigation cannot clear a prior resume barrier. Explicit
history inspection, evidence recording and cancellation remain separate.

Recovery placement keeps a standalone Process immediately after the restored
origin. A missing historical area UUID may map only to the single existing area
of the original project/workspace, with an explicit localized fallback message
and both IDs retained in the recovery observation (accepted design section9).
Several possible areas cause a visible gap before launch; no area is guessed or
created, no frozen identity is rewritten, and unrelated mixed panes stay put.


### User iteration08: native Process composition (2026-10-19)

The accepted flowPage prototype is the visual/interaction reference for the
Process body: contextual title/status, selectable derived topological phases,
main worker/selected-step details/recent durable events and an adjacent context
column for recorded source, pinned contract/evidence, resources and review usage.
The configured graph determines phase count, identities and labels. Selection
is inspection state, independent of active execution, retained on refresh; a
missing selection reports its fallback. Missing node telemetry remains unknown
without hiding configured phases. Pending questions/permissions expose only
qualified native status and existing-session navigation; absent request text
and answer controls are explicitly unavailable.

At narrow actual pane widths the context follows the main content and phases
stack; all content is scroll-reachable. The existing native frame minimum is
preserved. Per-pane geometry/scroll caches support mixed/duplicate Process
bodies without width contention. Existing origin/child navigation, close and
resume barriers remain intact. Inspection never dispatches a task/review or
answers approval. Unsupported dispatch/retry/correction/automatic continuation
controls are explained, not simulated. This UI checkpoint does not complete
full6.3, correction/review lifecycle, group movement or integrated acceptance.


### Explicit recovery reconciliation (Iteration09)

`guild workflow reconcile RUN --from-file PATH --json` is an owning-window
operation. Its strict version-one payload is:

```json
{"version":1,"request_id":"inspect-recovery","expected_revision":0,"assessment":null}
```

This inspects only; it starts no session or review. The original parent receives
`reconciliation.fingerprint`, `clear`, concrete blocker codes/session targets and
the current run revision. Status/Process inspection remains available without
resume; assigned children can inspect but cannot acknowledge as parent. Questions
and tool permissions are dependencies, never answered by Process. Follow eligible
existing session links to inspect the actual request; absent request details are
explicitly unavailable.

After all blockers are independently resolved, the exact original parent submits
a new stable request key, the returned current revision and the complete SHA256
in `assessment`. This is explicit source/identity-qualified acknowledgement,
not a kernel observation treated as human approval. Repeating the identical
acknowledgement coalesces. An unresolved acknowledgement reservation is unknown
and cannot be replayed. A wrong parent/conversation, stale assessment, source,
assignment/report, review/job/usage or resource change is refused.

The acknowledgement preserves old recovery/effect receipts, completed outcomes
and the used/reserved ledger. It authorizes existing eligible unfinished work;
the existing guarded checkout/launch owner continues its admitted assignment
without creating another child for completed work. Mutable source/native facts
are rechecked at admission and native callbacks. New facts can make an older
acknowledgement stale; inspect and explicitly acknowledge the changed state.
Live-only reveal adds no pause and cannot waive an older unknown effect.

Resume, reconciliation, readiness and orchestration continuation remain distinct.
The acknowledgement records `ready:false`, `continued:false`; no original task,
permission answer or new prompt is delivered. This slice supports the explicit
parent checkpoint/command path. Automatic harness handoff/injection, broader
resource recovery, same-session correction, group movement and review lifecycle
expansion remain pending; no generic three-harness startup-text transport exists.
Recorded private review environment leases are observed without values or
release/recreation. A retained/corrupt/unknown owner and pending/unknown actual
review usage retain the barrier. Closed archive bytes and evidence are checked
for freshness; accounting reconciliation does not turn findings into acceptance.


Iteration10 adds a narrow workflow-owned origin handoff, independently of general
agent-send/task-session privileges. Only an explicitly reserved actual stopped
origin recovery authorizes it; live reveals and historical inspection authorize
no input. Exact original conversation, settings, PID/view generation, fresh run,
contract/source/child/review/resource facts and an untouched resumed terminal are
rechecked before input. An authoritative completed turn plus bracketed prompt
transport is required. Busy, waiting, unknown, draft and covered prompts are not
eligible; no timer or rendered prompt text supplies readiness. In particular,
Codex Stop alone remains tentative: the existing exact rollout adapter must
observe a newly appended completed turn. A silent initial resume prompt remains
an explicit unsupported automatic-input wait, not a guessed empty prompt.

The deterministic bounded handoff contains current identities, completed and next
configured nodes, child delivery/status/report digests, evidence, actual review
accounting and unresolved blockers/resources. It never replays the initial task
or acknowledges reconciliation. The exclusive queue/journal reserves the stable
recovery intent and prompt generation before input. Reserved-without-observation,
submitted-without-turn-witness and unknown outcomes never automatically replay,
including after restart. A proven pre-input refusal may retry the same bounded
intent with a retained new attempt and freshly assessed facts. The existing PTY
submission and turn-start observation owners transport the summary. Process
separates qualified readiness, submission, observed turn start, uncertainty,
explicit parent acknowledgement and a subsequent checked parent checkpoint.
Automatic continuation is not inferred from a process, paste or turn start.

Free native Root/PTY fixtures qualify the Codex adapter path; Claude/OpenCode
adapter eligibility is covered by shared guards, with live end-to-end handoff
acceptance still UNVERIFIED. No new three-harness startup-text transport or real
model invocation is introduced. Full6.2/6.3 and A01–A24 remain pending integrated
parent acceptance, including unsupported silent-startup and resource recovery.


Iteration11 supersedes the silent-startup limitation above. A compound
`origin_recovery` reservation binds a bounded current summary digest and exact
original binding/source/assessment before launch. Only a positively stopped,
qualified native recovery supplies that literal payload through the existing
`reopen_prompt` owner: Claude positional prompt, Codex `resume ID -- PROMPT`,
OpenCode `--session ID --prompt PROMPT`. OpenCode first exports and qualifies the
existing exact conversation/cwd; its session flag alone can create a missing
session and is insufficient. Installed help/version qualification is read-only;
no real conversation/provider is launched by builder acceptance.

Live reveals never submit the candidate payload. No user-session send privilege,
task conversion, inferred readiness, terminal scraping or automatic stop/relaunch
is added. Source/contract/effect uncertainty withholds the candidate; changed
child/source/native facts refuse the reserved launch. A known unanswered child
question may be described without answering it or admitting its dependent effect.
Startup submission, independent adapter turn-start observation, qualified prompt
readiness, explicit original-parent acknowledgement and a subsequent workflow
action remain separate. Trust/auth/permission may block a submitted startup
payload; launch is not proof of their resolution. A lost observation/crash retains
uncertainty and cannot race the guarded post-completion fallback, even after
another recovery or live reveal. A proven pre-start refusal records no submission.

Free executable Root/PTY fixtures exercise exact Claude/Codex/OpenCode startup
argv, an independently gated adapter lifecycle, repeated child navigation,
source-qualified parent acknowledgement and next unfinished work without replay.
The OpenCode test process table is scoped to actual descendants in test builds
only; production retains its conservative whole-host foreign-holder refusal.
Real-host/model acceptance, automatic resource recovery, broader lifecycle and
full A01–A24 integration remain pending parent-owned qualification.


### Checked findings and same-session correction

The original parent may use `workflow retry` with the exact completed external
review attempt to return its checked findings along the unique configured
correction edge. A mandatory archive must agree with the installed saved reader,
frozen source/contract/roster, complete source inventory and actual call ledger.
Findings, unresolved questions, degraded or incomplete output never qualify clean.

For a managed implementation target, Root reserves one correction claim and the
source-bound summary before input. Delivery uses the existing native task prompt
owner in the original assigned session/conversation/checkout and selection. It
refuses working/waiting/manual sessions, drafts, unknown identity, changed source
and conflicting senders. Actual adapter turn submission is recorded separately
from reserved/submitted/unknown. Repeating an identical request reuses its receipt;
a positive pre-input refusal permits an explicit retry under the same claim.
Unknown delivery survives restart and cannot be replayed. General session-send
privileges are unchanged.

A parent-owned contract target returns to a fresh parent checkpoint without
sending harness input. Both paths require changed source and new checked evidence;
completed historical receipts remain visible. The parent accepts the new worker
report and explicitly requests the subsequent serial review. That round uses the
original retained package/profile/configuration and one already charged correction
claim; mutable profiles cannot replace its lineage or replenish its allowance.

A complete archive can be recorded while a keep-open presentation PTY still runs.
Its private environment lease remains until the actual owned terminal completes.
Hide/show never launches another provider. Workflow cancellation stops its owned
jobs through existing terminal actions; a late archive remains history-only and
cannot finish a cancelled node. Missing/ambiguous owners retain explicit gaps and
leases. The one-second final auto-exit default is unchanged.


Moving the origin or an open Process uses the ordinary Move to Area/drop route
as one related group in the same project/workspace. All known Process views of
the original parent stay together; assigned children remain independent. Mixed
tabs transfer only related bodies through native pane adoption; terminal owners,
pane identities and unrelated bodies remain intact. Unknown associations or
foreign-seat members refuse before movement; select a related pane when a mixed
tab contains several groups. Movement never resumes, cancels or rewrites a run.


Prior review imports are bookkeeping before launch admission. Root durably pins
each supplied import identity, original archive owner/metadata and observed event
prefix before a later allowance, package or preparation refusal. Known calls
remain charged; unreadable or incomplete accounting holds new review admission
across omitted `prior`, another request key and coordinator reload. Multiple
historical uncertain imports hold the remaining capacity without authorizing
parallel execution. Actual imported history beyond today's ceiling is retained,
with no remaining allowance, rather than discarded as though it never ran.

The same original import can reconcile from matching complete, append-only
history without charging its known attempts twice. Changed import IDs, archive
owners/paths, source/contract/correction context, manifest or prior event bytes
cannot substitute another history. Old native observer/correction snapshots do
not erase later import facts. Neither import nor reconciliation accepts the
review's findings/outcome, launches an executor, edits the original archive or
answers approvals. The public request vocabulary and configured limits are
unchanged; older persisted rounds remain readable.


## Ordinary task entry

[Public native entry](native-entry.md) registers Process before substantive
parent design, using the actual original session and one evolving draft selection.
The shared project skill/adapter chooses native once on public capabilities
including entry_before_design:true,
otherwise portable before Start; it never runs both controllers. A started
parent-owned ContractPrepare node is Active in the existing graph; old journals
without the preparation-start observation keep their historical state. No draft
is accepted and no review/worker starts from this observation.

Original-parent Status exposes current preparation.context (source, contract,
eligible steps and current requirements), or a concrete gap. This is a read-only DTO,
not acceptance or a permit. ContractPrepare recaptures final selected bytes and
qualifies actual evidence under the existing source/revision/authority guards.
The progressive skill client uses public CLI plus actual note/OpenSpec validation;
source identity remains owned by Root. Contract bytes and final requirements may evolve until ContractPrepare; definition stays
frozen on Start. ContractPrepare durably freezes accepted requirements together
with the contract, preserving initially explicit checks. All downstream evidence
consumers use that retained list; omission never clears it. Limits/imported usage remain the same
ledger, not a fresh review allowance. Final ordinary-harness/native acceptance
requires parent observation; simulated CLI/Root tests are not model evidence.
