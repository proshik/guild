# Ordinary development in Guild

Applies equally to Claude, Codex and OpenCode. Preserve GUILD_SESSION and
GUILD_SOCKET; never choose another caller/socket. Discover before design with
`guild workflow capabilities --json`. A v1 capabilities reply with entry_before_design:true, finalize_checks:true and start, status,
show, checkpoint, review, retry, cancel selects native mode ONCE for this task.
Positively absent host/CLI or unsupported capabilities selects portable before registration.
An incomplete identity, dead owned socket, authority gap or malformed reply is a
gap, never portable fallback. Retained native intent also forbids fallback.
After a native Start, a refusal is a retained gap, never a portable second flow.
An assigned builder does not start another process workflow.

Create/reuse one nonempty task note if OpenSpec is absent, or the same matching
OpenSpec change skeleton when present, before substantive design. This is draft
selection, not acceptance. Do not initialize a second spec or run independent
design/review outside the native graph. Keep preparing in the original checkout.

Start payload (JSON file, substitute actual identity/absolute target/task):

```json
{
  "version": 1, "request_id": "calculator-entry-1", "expected_revision": 0,
  "origin": "ACTUAL_GUILD_SESSION_UUID", "target": "/absolute/checkout",
  "selection": "tasks/calculator.md", "definition": null, "overrides": null,
  "task": "Complete requested calculator with the agreed checks",
  "architecture_only": false, "checks": []
}
```

Set architecture_only only for an actual design-only request. Resolve configured
role selections, review ceilings AND previously consumed/reserved usage before
effects; defaults grant no new calls. Declare known observable requirements in
checks (step, outcome, scenario, surface, expectation). The graph is frozen on
Start; checks:[] is legitimate while design chooses CLI/web/native/API. Finalize
the actual checks with the final ContractPrepare, not before Process appears.
Initially explicit checks must remain identical in the final list. The accepted
checks and contract are journaled together, before review/build; later checkpoint,
review and report validation use that retained list. It cannot be cleared or
changed after acceptance, including after source invalidation/correction.

The project development-workflow reference `native-entry.py` is a small public
CLI client: `probe`; `start REQUEST.json --receipt TARGET/.guild/workflow-runs/entry-TASK`; then
`contract RUN --receipt TARGET/.guild/workflow-runs/entry-TASK --checks FINAL-CHECKS.json`. It preserves full requests and
replies before retries. Start also calls Show; registration creates a linked
Process without accepting a draft or dispatching a builder/provider. Lost reply:
reuse the original complete request, not a new request ID. Do not create a new
run to recover an existing task. Read retained receipt/run first.

During design use `guild workflow status RUN --json`. Original-parent replies
include preparation.context with current source/contract and eligible configured
parent ContractPrepare steps, or preparation.gap. These observations are read-only,
not accepted evidence; stale bytes are rejected again at checkpoint. The helper
executes the current installed OpenSpec strict validation, or `/bin/test -s` for
a lightweight note, retains output, then supplies the normal source-qualified
ContractPrepare checkpoint. It never invents a successful tool result or review.
Inspect/finish the contract before invoking it; validation does not mean the
parent's independent design review has passed.

After ContractPrepare, inspect graph-derived next steps. Ready review.request
requires the parent's explicit `guild workflow review RUN --from-file REQUEST
--json`, using version, request_id, expected_revision, step, package (selected
installed revust), config_dir/profile/scope, allowance, keep_open and prior
(original linked usage). Read `guild workflow help` and retained status; do not
run the separate portable revust launcher, worktree/session dispatch or reports
in parallel. Runtime assignment launches only after matching accepted review.
Observe actual assignments/reports/jobs through Status; assigned children submit
their own qualified checkpoint, the parent accepts the retained report. Retry
uses the existing attempt identity; uncertain sends/usage are never replayed.
Use ordinary public checkpoint for actual acceptance/local delivery evidence.
Reconcile human questions/approval in the owning session; never answer them.

Native Process is visible while the parent prepares the contract (Active); later
phases remain blocked and the contract remains unaccepted. Explicit review and
worker restrictions remain effective. Native registration is not provider-call
authority or permission to publish, push, merge, install or deploy.

## Final requirements after design

Write FINAL-CHECKS.json under TARGET/.guild/workflow-runs/entry-TASK. For a
chosen API scenario, the literal public CheckRequirement shape is:

```json
[{"step":"accept","outcome":"accept","scenario":"sum-api",
  "surface":"api","expectation":{"kind":"http_status","value":200}}]
```

This is a requirement, not evidence that a request ran. Choose the actual
configured step/outcome, observable scenario and surface from the design. Native
UI needs native observations/screenshots and expectation observed; API needs an
actual request/response and success/http_status; CLI needs actual process output
and success/exit_code. Do not relabel generic command output as UI/API proof.
Include every initial explicit check unchanged; add new requirements as needed.

The helper's --checks supplies the final full list to the existing public
checkpoint's optional checks field. Direct callers submit version, request_id,
expected_revision, step, outcome, evidence and checks together. Omission keeps
legacy identities/requirements; it does not clear them. Only the original parent
can supply this field at ContractPrepare; assigned designers report as before,
and their parent can finalize the list when accepting the retained report.
Status exposes checks/checks_finalized. Stale/foreign/post-accept attempts cannot
replace the accepted list or erase existing evidence.

All helper receipts must be TARGET/.guild/workflow-runs/entry-TASK, even in a Git
repo without .guild ignored. This namespace is already source-excluded and
within permitted evidence roots. Other locations refuse before writes; no
.gitignore/global changes are needed. For discovery after a retained intent use
probe --receipt with that same directory. Same-key Start retries reach the owner
using the saved complete intent before reading today's changed/deleted draft.
Same-key checkpoint retries resend saved evidence/checks rather than rerun checks
or silently adopt a different --checks file.

When the initial list is empty, the helper requires --checks FILE at final
ContractPrepare. It never silently finalizes an implicit empty list. Explicit []
is reserved for a consciously selected flow without actual acceptance checks;
it cannot satisfy configured product acceptance.
