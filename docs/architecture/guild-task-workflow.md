# Using the project task workflow

Small settled fixes and bounded UI trials need the request, expected
result/constraints, implementation and targeted checks, not a separate
specification or plan. For substantial or ambiguous changes, persist agreed
behavior, scenarios and one implementation plan in OpenSpec when useful or
explicitly required. The presence of `openspec/` alone does not require a contract
or change. These instructions are not a Guild scheduler or runtime enforcement.
When OpenSpec is selected, read [OpenSpec](../../openspec/README.md).
Static checks establish neither behavioral pilot PASS nor global readiness.

## Entry and discovery

Ordinary software implementation/fix requests, singular or plural, select
`development-workflow`; `$development-workflow <task>` or a direct read of
`.claude/skills/development-workflow/SKILL.md` is also supported. Advice alone
starts no work; architecture-only stops at independently reviewed design.
The request carries bounded configured builder/parent-review authority through
internal phases; explicit restrictions and actual host permissions prevail.

Five skills share `.claude/skills` through the unchanged `.agents/skills` relative
symlink, with implicit invocation enabled:

| Skill | Responsibility |
| --- | --- |
| development-workflow | Intent, entry/continuity and sole contract routing |
| orchestration | Assignments, active supervision, human channels, recovery/resources/integration |
| spec-design | Project-appropriate contract and acceptance scenarios |
| engineering-quality | Complete fixes and relevant common/backend/web/desktop/CLI checks |
| acceptance-testing | Ongoing/final observations, exact evidence and named gaps |

Use the supported harness catalogue or direct reads; do not invent a slash
command or install global plugins. Canonical path/inode/reference checks establish
filesystem exposure, separately from actual harness discovery/selection/actions.
Guild exposes public capabilities independently of these files; it does not
select workflow policy. The skills neither inspect Rust nor patch bootstrap.

## Contract, effective policy and parent checkpoints

Read the effective `development-workflow` policy
for defaults/provenance and optional read-only legacy mapping; private `.guild`
files are not required. Follow [OpenSpec preparation and validation](../../openspec/README.md)
only when OpenSpec is selected. Otherwise skip OpenSpec commands, schema and
archive steps. Without OpenSpec, use a concise existing document/task note only
when persistence is useful or explicitly required. Existing but broken OpenSpec
is a setup gap only for work that selects it. Superpowers
supplies optional methods selected by task complexity, rather than mandatory
brainstorming, duplicate artifacts, dispatch or phase gates. Missing Superpowers
alone does not block work; project quality checks remain applicable.

Root owns scope, independent revust review and final combined acceptance. Designers
receive requirements/facts/decisions/questions; small fixes may skip a designer,
not their required managed implementation worktree. Builders remain for corrections
and never launch reviews/workers, steer sessions or answer another tool approval.

The [public Guild mapping](../../.claude/skills/orchestration/references/guild.md)
qualifies identities/capabilities and visible managed sessions. Outside Guild,
Git plus an existing qualified adapter can supply isolation; non-visible workers
need actual correlated questions, distinct tool approvals and durable resume.

Supervise each assigned ID fairly with host events/bounded waits (Guild normally
30 seconds, at most 60), list/read and durable artifacts. Timeout is pending;
`done` ends a turn, not task acceptance. Perform its authorized parent checkpoint,
then resume the same currently eligible idle/done task via `--from-file` and verify
start. Do not send to working/waiting or user-opened sessions. Remain active while
work/checkpoints can progress; no final response leaves workers running and no
monitoring loop survives the end of execution. Genuine human/external dependencies
pause affected actions. Recovery reconciles original IDs/processes/history before
retry; [state guidance](../../.claude/skills/orchestration/references/state.md)
provides handoff/atomic report fields and resource ownership, without a new engine.

## Review, acceptance and local qualification

[Parent revust setup/accounting](../../.claude/skills/development-workflow/references/review.md)
keeps package qualification separate from provider authority. An existing usable
local package is valid; missing package installation requires the trusted published
Homebrew route and actual permissions. The pilot formula is currently unpublished;
no installation/publication occurs. New routine review is parent-only, with no
native registration mandate; preserve existing native configuration/history/limits
and name incompatibilities without changing identity or launching another engine.

For this pilot, all plan/code/retry/correction stages across both branches share
18 actual calls and at most three correction rounds; reserve the next batch before
launch. Root owns the separate six-start evaluation ledger, counting invalid and
failed starts. These are user pilot limits, not application defaults. Effective
revust profiles and builder roles remain independent of native settings.

Use [acceptance evidence](../../.claude/skills/acceptance-testing/references/evidence.md)
for regressions plus relevant ongoing/final live API/browser/native/CLI observations
on exact reviewed snapshots. Root integrates exact prerequisite/consumer versions
and checks combined scenarios. Missing required review/live/integration evidence
is UNVERIFIED; document validation, model-judged prose and fixture replay cannot
establish live PASS. Preserve pending pilot/global tasks and immutable archives.

The user-approved local draft process deviates from generic writing-skills
baseline-first/per-skill deployment cadence and five-repetition experiments.
Observed supervision and independently confirmed contract/routing failures support
only those concrete rules. Other behavior stays UNVERIFIED until root's bounded
observations; the gate before rollout remains.
No new runtime/helper framework or dependency is introduced.

## Source principles and approved adaptation

Source: Louis Claxton, Anthropic,
[The AI-native SDLC playbook](https://claude.com/resources/articles/the-ai-native-sdlc-playbook),
dated 2026-08-21, re-read 2026-10-14. The right column records explicit user
adaptations, not a claim that the article mandates our tools or permissions.

| Source principle | User-approved local adaptation |
| --- | --- |
| Preserve intent in accessible versioned artifacts | When persistence is selected, one project-appropriate contract and exact digests shared with fresh designers and managed builders |
| Apply engineering constraints through skills during design/build | Five local skills, established design system and relevant engineering surface guidance |
| Evaluate continuously alongside implementation | Meaningful regressions, ongoing/final live acceptance and bounded root-owned fresh-context evaluations |
| Accepted evidence triggers the next stage | Active parent supervision performs authorized checkpoints and resumes the same eligible worker without manual restarts |
| Keep human responsibility for material decisions | Real correlated human answers, separate host tool approvals and parent-owned revust review |

Publication/deployment/global rollout retain separate user authorization; no
automated push, merge or production loop is inferred. Local prototypes and static
checks are not behavioral qualification. These principles authorize no extra calls.

## Legacy transition and delivery

A minimal explicit-only `guild-sdd` redirect remains for named legacy callers
that still use the old entry. Remove the alias once no current caller needs it.

Native Guild review implementation, UI and commands are retired. External revust
runs independently through generic Terminal Jobs; existing native archives/settings
remain untouched and are not converted. Preserve session provenance and external
review histories and limits. No compatibility proxy is supplied.

Operational reports go only to explicitly assigned paths, outside-checkout writes
require their scope, and runtime directories/user settings/archives are never
committed. Preserve shared resource ownership; timeout/crash does not free a lease.
Prepare a local branch and concrete MR/PR text; `ready` requires actual acceptance.
Push is manual unless explicitly instructed, with no standing publication/merge,
production, global install/removal or cleanup. Archive only after accepted scope
is evidenced or explicitly revised.

## Retired native review

Guild no longer supplies native review commands, configuration or observers.
Use the installed external review tool according to the initiating workflow.
Generic terminal jobs, archive viewers and literal overlays remain public APIs.
Existing `.guild/review/`, `.guild/guild-agents.json` and
`.guild/guild-workflow.json` bytes remain untouched and are not read as runtime
review state. Historical task registrations do not trigger execution on restart.
