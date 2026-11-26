# Guild specifications

`specs/` records the existing contract, not an assertion that every scenario was
retested during migration.
`changes/` holds outstanding work when there is any; unchecked tasks must not be represented as
implemented. Decisions in force are in docs/decisions.md.

Ordinary implementation/fix requests use the project `development-workflow`
skill implicitly; `$development-workflow <task>` or a direct read of
`.claude/skills/development-workflow/SKILL.md` is also supported. Codex's
`.agents/skills` is the same source as Claude's `.claude/skills`; other harnesses
must verify discovery or read the file directly. No provider plugin/global install
is required. Read [the workflow guide](../docs/architecture/guild-task-workflow.md)
for roles, parent supervision, review and local prototype limitations.

Only when OpenSpec is selected, use the installed CLI to obtain artifact
instructions and validate:

```sh
openspec list --json
openspec status --change <change> --json
openspec instructions tasks --change <change> --json
openspec validate --all --strict --no-interactive
```

When using OpenSpec, run `openspec list --json` for current changes and read their
task checklists.
A static list here can outlive both implementation and archival.

Native Guild review is removed. Existing main-checkout `.guild/review/` archives
and native settings remain untouched; Guild neither initializes nor reconciles
them. External installed revust remains parent-owned through generic terminal APIs.
Workflow receipts use an explicitly assigned retained location separate from
engine manifests; outside-checkout worker report writes need explicit scope.

The presence of `openspec/` alone does not require a contract or change. Small
settled fixes and bounded UI trials need the request, expected result/constraints,
implementation and targeted checks, not a separate specification or plan.
For substantial or ambiguous changes, persist agreed behavior, scenarios and one
implementation plan in a matching OpenSpec change when useful or explicitly
required. Apply Superpowers methods as needed without a duplicate spec/plan.
Without OpenSpec, use a lightweight note only when persistence is useful or
required. Skip OpenSpec CLI, schema/delta and archive steps unless OpenSpec is
selected. Initialize only when adoption is explicitly requested, preserving
existing instructions. Broken OpenSpec is a setup gap only for work that selects
it; absence alone does not require installing or initializing it.
Historical changes retain their recorded choices and permissions. Artifact
completion and schema validation establish document structure, not implemented
behavior, safety or acceptance. Pending pilot/global tasks remain unchecked.
